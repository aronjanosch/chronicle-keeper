//! SC-05: grounded development generation for the session review.
//!
//! Two stages, same shape as the Phase 5 proposal pass it replaces: stage 1
//! drafts candidates from the summary plus bounded world context; stage 2
//! retrieves transcript turns and verifies each factual claim against them.
//! What the transcript cannot support becomes a question, never a factual
//! update — the GM answers it, or it stays open.
//!
//! The model never supplies paths or file bytes. The server resolves every
//! page through the vault safeguards, groups developments so one page has one
//! owner per run, and renders exact before/after previews through the same
//! function that applies them.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::codex_update::{
    clamp_range, excerpt_of, matching_turns, parse_json_lenient, render_target, target_path,
    transcript_turns, Change,
};
use crate::error::{AppError, AppResult};
use crate::llm;
use crate::session_review::{
    self as review, Decision, Development, Evidence, Possibility, Question, QuestionStatus,
    ReviewRun, ReviewStatus, Target,
};
use crate::state::AppState;
use crate::store::{artifacts, sessions};
use crate::vault;

/// Stop drafting past this many items; a noisy review is a useless one.
const MAX_ITEMS: usize = 12;
/// Summary characters quoted as evidence for an unverified claim.
const SUMMARY_EXCERPT_CHARS: usize = 400;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// Future consequences are opt-in and never produce page writes.
    #[serde(default)]
    pub include_possibilities: bool,
    /// Regenerate only these developments. Empty means a full run.
    #[serde(default)]
    pub development_ids: Vec<String>,
    /// Required when a current run exists.
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub base_revision: Option<String>,
}

pub enum GenProgress {
    /// Reading the summary, the page list, and prior applied updates.
    Reading,
    /// Verifying candidates against retrieved transcript turns.
    Grounding,
    /// Resolving pages and rendering exact previews.
    Building,
}

// ── Candidates (stage 1 output, never persisted as-is) ────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateKind {
    Development,
    Question,
    Possibility,
}

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub kind: CandidateKind,
    pub id: String,
    pub title: String,
    pub text: String,
    /// Existing page path, when the candidate matched one.
    pub page: Option<String>,
    pub page_kind: String,
    pub folder: Option<String>,
    pub changes: Vec<Change>,
    pub entities: Vec<String>,
    pub links: Vec<String>,
}

fn cancelled(cancel: &AtomicBool) -> AppResult<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::Conflict("Generation was cancelled.".into()));
    }
    Ok(())
}

// ── Stage 1: candidate prompt ─────────────────────────────────────

pub(crate) fn build_candidate_prompt(
    summary: &str,
    pages: &[vault::PageInfo],
    rel_fields: &HashMap<String, Vec<String>>,
    applied_history: &[String],
    session_number: Option<i64>,
    lang_name: &str,
    include_possibilities: bool,
) -> String {
    let mut page_list = String::new();
    for p in pages {
        page_list.push_str(&format!(
            "- {} (kind: {}, path: {}) — {}\n",
            p.title,
            p.kind.as_deref().unwrap_or("lore"),
            p.path,
            p.summary
        ));
    }
    let mut rel_lines = String::new();
    for (kind, fields) in rel_fields {
        if !fields.is_empty() {
            rel_lines.push_str(&format!("  {kind}: {}\n", fields.join(", ")));
        }
    }
    let applied = if applied_history.is_empty() {
        "(none)\n".to_string()
    } else {
        applied_history
            .iter()
            .map(|t| format!("- {t}\n"))
            .collect::<String>()
    };
    let session_label = session_number
        .map(|n| format!("S{n}"))
        .unwrap_or_else(|| "S?".into());
    let possibility_rule = if include_possibilities {
        "- `possibility` is what MIGHT happen next. It never changes the world; it is prep material.\n"
    } else {
        "- Do NOT return `possibility` items.\n"
    };
    format!(
        "You review what happened in a tabletop-RPG session and propose updates to the \
campaign wiki (\"codex\"). Separate what happened from what is uncertain.\n\n\
Return ONLY a JSON object:\n\
{{\"items\": [{{\n\
  \"classification\": \"development|question|possibility\",\n\
  \"title\": \"page name for a development, short label otherwise\",\n\
  \"text\": \"one or two sentences: what changed, what is uncertain, or what might happen\",\n\
  \"kind\": \"pc|npc|place|faction|item|lore (developments only)\",\n\
  \"is_new\": false,\n\
  \"folder\": \"folder for NEW pages only, picked from existing paths\",\n\
  \"summary_new\": \"refreshed one-liner, or null if unchanged\",\n\
  \"body_append\": \"1-3 sentences of new events to append under ## Notes, or null\",\n\
  \"rels\": [{{\"field\": \"allies\", \"add\": \"[[Other Page]]\", \"note\": \"why\"}}],\n\
  \"links\": [\"[[Pages]] this relates to\"],\n\
  \"entities\": [\"names/aliases to locate this in the transcript\"]\n\
}}]}}\n\n\
Classification rules:\n\
- `development` is something that HAPPENED and changes the world. It needs at least one of \
summary_new, body_append, or rels.\n\
- `question` is uncertain: a player theory, an NPC's claim, an outcome nobody confirmed. \
No page changes.\n\
{possibility_rule}\
- A plan the players discussed is NOT a development. An NPC's allegation is attributed, not fact.\n\n\
Page rules:\n\
- `title` is a page NAME only — never a path, never a folder prefix (\"Ulric\", not \"NPCs/Ulric\").\n\
- Existing pages: use the EXACT title from the page list; set is_new=false.\n\
- Never propose a new page for a name already in the page list, whatever folder it sits in.\n\
- `summary_new` is the one-liner the summarizer memorizes (max ~25 words). Only when the old one is outdated.\n\
- `body_append` records session events; prefix with \"{session_label} — \".\n\
- `rels` only for clear new relationships, using these list fields per kind (omit otherwise):\n{rel_lines}\
- Write prose in {lang_name}. Keep proper names verbatim.\n\
- At most {MAX_ITEMS} items; fewer is better than noisy.\n\n\
Already applied in an earlier review — do not propose these again:\n{applied}\n\
Session summary:\n\"\"\"\n{summary}\n\"\"\"\n\n\
Existing pages:\n{page_list}"
    )
}

fn repair_prompt(original: &str, raw: &str, error: &str) -> String {
    format!(
        "Your previous response could not be used: {error}\n\n\
Return ONLY the JSON object the instructions asked for, with no prose around it.\n\n\
--- original instructions ---\n{original}\n\n--- your response ---\n{raw}"
    )
}

// ── Stage 1: parsing and validation ───────────────────────────────

/// Parse and validate a stage-1 response. Unknown keys are ignored; an invalid
/// classification, a development with nothing to change, or a question with no
/// text fails the whole response so a repair attempt can run. A malformed
/// response never becomes an apparently successful empty review.
pub(crate) fn parse_candidates(
    raw: &str,
    pages: &[vault::PageInfo],
    include_possibilities: bool,
) -> AppResult<Vec<Candidate>> {
    let parsed = parse_json_lenient(raw);
    let arr = match &parsed {
        Value::Object(map) => map.get("items").and_then(Value::as_array).cloned(),
        Value::Array(a) => Some(a.clone()),
        _ => None,
    }
    .ok_or_else(|| AppError::Unprocessable("response has no `items` array".into()))?;

    let by_title: HashMap<String, &vault::PageInfo> = pages
        .iter()
        .map(|p| (crate::store::index::normalize_name(&p.title), p))
        .collect();
    let by_path: HashMap<String, &vault::PageInfo> = pages
        .iter()
        .map(|p| {
            let rel = p.path.strip_suffix(".md").unwrap_or(&p.path);
            (crate::store::index::normalize_name(rel), p)
        })
        .collect();

    let mut out = Vec::new();
    for (i, v) in arr.iter().take(MAX_ITEMS).enumerate() {
        let obj = v
            .as_object()
            .ok_or_else(|| AppError::Unprocessable(format!("item {} is not an object", i + 1)))?;
        let s = |k: &str| {
            obj.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("null"))
                .map(str::to_string)
        };
        let classification = s("classification").unwrap_or_default().to_lowercase();
        let kind = match classification.as_str() {
            "development" => CandidateKind::Development,
            "question" => CandidateKind::Question,
            "possibility" => CandidateKind::Possibility,
            other => {
                return Err(AppError::Unprocessable(format!(
                    "unknown classification: {other}"
                )))
            }
        };
        if kind == CandidateKind::Possibility && !include_possibilities {
            continue;
        }
        let raw_title = s("title")
            .ok_or_else(|| AppError::Unprocessable(format!("item {} has no title", i + 1)))?;
        let text = s("text").unwrap_or_default();
        let links = str_list(obj.get("links"));
        let entities = str_list(obj.get("entities"));

        if kind != CandidateKind::Development {
            if text.is_empty() {
                return Err(AppError::Unprocessable(format!(
                    "{classification} \"{raw_title}\" has no text"
                )));
            }
            out.push(Candidate {
                kind,
                id: format!("c{}", i + 1),
                title: raw_title,
                text,
                page: None,
                page_kind: String::new(),
                folder: None,
                // A possibility can never carry page writes.
                changes: Vec::new(),
                entities,
                links,
            });
            continue;
        }

        let (title_folder, title) = vault::split_page_title(&raw_title);
        if title.is_empty() {
            return Err(AppError::Unprocessable(
                "development has an empty title".into(),
            ));
        }
        let page_kind = s("kind").unwrap_or_else(|| "lore".into()).to_lowercase();
        if !vault::KINDS.contains(&page_kind.as_str()) {
            return Err(AppError::Unprocessable(format!(
                "unknown page kind: {page_kind}"
            )));
        }
        let existing = by_path
            .get(&crate::store::index::normalize_name(
                raw_title.strip_suffix(".md").unwrap_or(&raw_title),
            ))
            .or_else(|| by_title.get(&crate::store::index::normalize_name(&title)));
        let is_new = existing.is_none();

        let mut changes = Vec::new();
        if is_new {
            changes.push(Change::New {
                summary: s("summary_new").unwrap_or_default(),
                body: s("body_append").unwrap_or_default(),
            });
        } else {
            if let Some(new) = s("summary_new") {
                let old = existing.map(|p| p.summary.clone()).unwrap_or_default();
                if !new.eq_ignore_ascii_case(old.trim()) {
                    changes.push(Change::Summary { old, new });
                }
            }
            if let Some(body) = s("body_append") {
                changes.push(Change::Body {
                    anchor: "## Notes".into(),
                    text: body,
                });
            }
        }
        for r in obj
            .get("rels")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
        {
            let field = r.get("field").and_then(Value::as_str).unwrap_or("").trim();
            let add = r.get("add").and_then(Value::as_str).unwrap_or("").trim();
            if !field.is_empty() && !add.is_empty() {
                changes.push(Change::Rel {
                    field: field.to_string(),
                    add: add.to_string(),
                    note: r
                        .get("note")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                });
            }
        }
        if changes.is_empty() {
            return Err(AppError::Unprocessable(format!(
                "development \"{title}\" changes nothing"
            )));
        }

        out.push(Candidate {
            kind,
            id: format!("c{}", i + 1),
            title,
            text,
            page: existing.map(|p| p.path.clone()),
            page_kind: existing.and_then(|p| p.kind.clone()).unwrap_or(page_kind),
            folder: if is_new {
                s("folder").or(title_folder)
            } else {
                None
            },
            changes,
            entities,
            links,
        });
    }
    Ok(out)
}

/// Resolve `[[Wikilinks]]` (or bare page names) to vault-relative paths,
/// dropping anything this vault does not have.
fn resolve_links(vault_root: &Path, links: &[String]) -> Vec<String> {
    links
        .iter()
        .filter_map(|l| {
            let name = l
                .trim()
                .trim_start_matches("[[")
                .trim_end_matches("]]")
                .trim();
            vault::find_page(vault_root, name)
        })
        .collect()
}

fn str_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// ── Stage 2: transcript grounding ─────────────────────────────────

fn search_terms(c: &Candidate) -> Vec<String> {
    let mut terms = vec![c.title.clone()];
    terms.extend(c.entities.iter().cloned());
    for ch in &c.changes {
        if let Change::Rel { add, .. } = ch {
            terms.push(add.trim_matches(['[', ']']).to_string());
        }
    }
    terms.retain(|t| t.len() >= 3);
    terms.sort();
    terms.dedup();
    terms
}

fn claim_digest(c: &Candidate) -> String {
    let mut parts = Vec::new();
    if !c.text.is_empty() {
        parts.push(c.text.clone());
    }
    for ch in &c.changes {
        match ch {
            Change::Summary { new, .. } => parts.push(format!("new summary: {new}")),
            Change::Body { text, .. } => parts.push(format!("note: {text}")),
            Change::Rel { field, add, .. } => parts.push(format!("relationship {field}: {add}")),
            Change::New { summary, body } => parts.push(format!("new page: {summary} {body}")),
        }
    }
    parts.join(" | ")
}

pub(crate) fn build_grounding_prompt(
    candidates: &[&Candidate],
    retrieved: &[Vec<usize>],
    turns: &[String],
) -> String {
    let mut claims = String::new();
    for (c, hits) in candidates.iter().zip(retrieved) {
        claims.push_str(&format!(
            "--- claim {} (page: {}) ---\n{}\n",
            c.id,
            c.title,
            claim_digest(c)
        ));
        if hits.is_empty() {
            claims.push_str("(no transcript lines matched this entity)\n\n");
            continue;
        }
        claims.push_str("transcript lines:\n");
        for &i in hits {
            claims.push_str(&format!("{}. {}\n", i + 1, turns[i]));
        }
        claims.push('\n');
    }
    format!(
        "You verify claimed session events against raw transcript lines. For each claim, \
decide whether the cited lines actually show it happened.\n\n\
Return ONLY a JSON object:\n\
{{\"results\": [{{\"id\": \"c1\", \"grounded\": true, \"start\": 12, \"end\": 18}}]}}\n\n\
Rules:\n\
- `grounded` is true ONLY if the lines clearly show the event. ASR text is noisy — allow \
misspelled names, but not invented facts.\n\
- A plan someone proposed, a theory, or an NPC's unverified claim is NOT grounded.\n\
- `start`/`end` is the line-number range (from the numbers shown) that best supports the \
claim. Omit or null when not grounded.\n\
- Judge every claim.\n\n{claims}"
    )
}

/// id → (grounded, claimed 1-based range).
pub(crate) fn parse_verdicts(raw: &str) -> HashMap<String, (bool, (usize, usize))> {
    let parsed = parse_json_lenient(raw);
    let arr = match &parsed {
        Value::Object(map) => map.get("results").and_then(Value::as_array).cloned(),
        Value::Array(a) => Some(a.clone()),
        _ => None,
    }
    .unwrap_or_default();
    let mut out = HashMap::new();
    for v in &arr {
        let Some(obj) = v.as_object() else { continue };
        let Some(id) = obj.get("id").and_then(Value::as_str) else {
            continue;
        };
        let grounded = obj
            .get("grounded")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let num = |k: &str| obj.get(k).and_then(Value::as_u64).map(|n| n as usize);
        let start = num("start").unwrap_or(0);
        let end = num("end").unwrap_or(start);
        out.insert(id.to_string(), (grounded, (start, end.max(start))));
    }
    out
}

fn summary_excerpt(summary: &str, title: &str) -> String {
    let needle = crate::store::index::normalize_name(title);
    for para in summary.split("\n\n") {
        if !needle.is_empty()
            && crate::store::index::normalize_name(para).contains(&needle)
            && !para.trim().is_empty()
        {
            return crate::codex_update::tail_chars(para, SUMMARY_EXCERPT_CHARS);
        }
    }
    crate::codex_update::tail_chars(summary, SUMMARY_EXCERPT_CHARS)
}

// ── Assembly: exact targets, one owner per page ───────────────────

/// Turn verified candidates into a persistable run. Developments that share a
/// page are combined into one compound development, because two independent
/// toggles writing the same file would silently overwrite each other.
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble(
    session_id: &str,
    provider: &str,
    model: &str,
    vault_root: &Path,
    summary: &str,
    turns: &[String],
    candidates: Vec<Candidate>,
    verdicts: &HashMap<String, (bool, (usize, usize))>,
    retrieved: &HashMap<String, Vec<usize>>,
    source_revisions: Vec<review::SourceRevision>,
) -> AppResult<ReviewRun> {
    let mut questions: Vec<Question> = Vec::new();
    let mut possibilities: Vec<Possibility> = Vec::new();
    // path → (candidates that own it, evidence collected)
    let mut grouped: Vec<(String, bool, Vec<Candidate>, Vec<Evidence>)> = Vec::new();

    for c in candidates {
        match c.kind {
            CandidateKind::Possibility => {
                possibilities.push(Possibility {
                    id: Uuid::new_v4().to_string(),
                    title: c.title,
                    text: c.text,
                    // A link the model invented is not a source. Only pages
                    // that actually resolve in this vault survive.
                    source_links: resolve_links(vault_root, &c.links),
                    decision: review::PossibilityDecision::Pending,
                    destination_session_id: None,
                });
                continue;
            }
            CandidateKind::Question => {
                questions.push(Question {
                    id: Uuid::new_v4().to_string(),
                    text: if c.text.is_empty() {
                        c.title.clone()
                    } else {
                        format!("{}: {}", c.title, c.text)
                    },
                    evidence: vec![Evidence::Summary {
                        excerpt: summary_excerpt(summary, &c.title),
                    }],
                    status: QuestionStatus::Pending,
                    confirmation: None,
                });
                continue;
            }
            CandidateKind::Development => {}
        }

        let hits = retrieved.get(&c.id).cloned().unwrap_or_default();
        let verdict = verdicts.get(&c.id);
        let grounded = verdict.map(|v| v.0).unwrap_or(false) && !hits.is_empty();
        if !grounded {
            // Unverified is not false — it is unresolved, and the GM decides.
            questions.push(Question {
                id: Uuid::new_v4().to_string(),
                text: format!(
                    "Did this happen? {} — the transcript does not confirm it.",
                    if c.text.is_empty() {
                        c.title.clone()
                    } else {
                        c.text.clone()
                    }
                ),
                evidence: vec![Evidence::Summary {
                    excerpt: summary_excerpt(summary, &c.title),
                }],
                status: QuestionStatus::Pending,
                confirmation: None,
            });
            continue;
        }

        let (start, end) = clamp_range(verdict.map(|v| v.1).unwrap_or((0, 0)), &hits);
        let evidence = Evidence::Transcript {
            start_turn: start,
            end_turn: end,
            excerpt: excerpt_of(turns, start, end),
        };

        let (rel, exists) = target_path(
            vault_root,
            c.page.as_deref(),
            &c.title,
            &c.page_kind,
            c.folder.as_deref(),
        )?;
        match grouped.iter_mut().find(|(path, _, _, _)| path == &rel) {
            Some((_, _, cands, evs)) => {
                cands.push(c);
                evs.push(evidence);
            }
            None => grouped.push((rel, exists, vec![c], vec![evidence])),
        }
    }

    let mut developments = Vec::new();
    for (rel, exists, cands, evidence) in grouped {
        let compound = cands.len() > 1;
        let changes: Vec<Change> = cands.iter().flat_map(|c| c.changes.clone()).collect();
        let first = &cands[0];
        let (before, after) = render_target(
            vault_root,
            &rel,
            exists,
            &first.title,
            &first.page_kind,
            &changes,
        )?;
        if Some(after.as_str()) == before.as_deref() {
            continue; // nothing would change on disk
        }
        let title = if compound {
            format!("{} ({} updates)", first.title, cands.len())
        } else {
            first.title.clone()
        };
        let description = cands
            .iter()
            .map(|c| {
                if c.text.is_empty() {
                    c.title.clone()
                } else {
                    c.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        developments.push(Development {
            id: Uuid::new_v4().to_string(),
            title,
            description,
            evidence,
            targets: vec![Target {
                base_hash: match &before {
                    Some(b) => review::hash_of(b),
                    None => review::ABSENT_HASH.to_string(),
                },
                path: rel,
                before,
                after,
            }],
            decision: Decision::Pending,
            application_id: None,
            compound,
        });
    }

    let run = ReviewRun {
        schema_version: review::SCHEMA_VERSION,
        run_id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        generated_at: crate::store::now(),
        provider: provider.to_string(),
        model: model.to_string(),
        status: ReviewStatus::Open,
        source_revisions,
        developments,
        questions,
        possibilities,
        applications: Vec::new(),
    };
    review::validate(&run)?;
    Ok(run)
}

/// Merge a regenerated run into the current one, replacing only the requested
/// groups. Applied work is immutable and its pages stay owned by the old run:
/// an overlap means the GM needs a fresh full review, not a silent join.
pub(crate) fn merge_regenerated(
    mut current: ReviewRun,
    fresh: ReviewRun,
    development_ids: &[String],
) -> AppResult<ReviewRun> {
    let mut replaced_paths: Vec<String> = Vec::new();
    for id in development_ids {
        let dev = current
            .developments
            .iter()
            .find(|d| &d.id == id)
            .ok_or_else(|| AppError::Unprocessable(format!("Unknown development: {id}")))?;
        if dev.decision == Decision::Applied {
            return Err(AppError::Conflict(
                "Applied developments cannot be regenerated.".into(),
            ));
        }
        replaced_paths.extend(dev.targets.iter().map(|t| t.path.clone()));
    }
    current
        .developments
        .retain(|d| !development_ids.contains(&d.id));

    let kept_paths: Vec<String> = current
        .developments
        .iter()
        .flat_map(|d| d.targets.iter().map(|t| t.path.clone()))
        .collect();
    for dev in &fresh.developments {
        for t in &dev.targets {
            if kept_paths.contains(&t.path) {
                return Err(AppError::Conflict(format!(
                    "{} is already owned by another update in this review. Regenerate the whole review.",
                    t.path
                )));
            }
        }
    }

    // Only the regenerated pages come back; anything else the fresh run drafted
    // belongs to a group the GM did not ask to redo.
    current.developments.extend(
        fresh
            .developments
            .into_iter()
            .filter(|d| d.targets.iter().any(|t| replaced_paths.contains(&t.path))),
    );
    current.questions.extend(fresh.questions);
    current.possibilities.extend(fresh.possibilities);
    current.generated_at = fresh.generated_at;
    current.source_revisions = fresh.source_revisions;
    review::validate(&current)?;
    Ok(current)
}

// ── Orchestration ─────────────────────────────────────────────────

/// Run both stages and persist the result. Cancellation is checked before each
/// provider call and before persisting, so a cancelled run leaves the previous
/// review and the world untouched.
pub async fn generate_streamed<F: FnMut(GenProgress) + Send>(
    state: &AppState,
    session_id: &str,
    req: &GenerateRequest,
    cancel: &AtomicBool,
    mut emit: F,
) -> AppResult<ReviewRun> {
    emit(GenProgress::Reading);
    let sid = session_id.to_string();
    let (provider_o, model_o, base_o) = (
        req.provider.clone(),
        req.model.clone(),
        req.base_url.clone(),
    );
    let prep = state.with_db(move |conn| -> AppResult<_> {
        let loc = sessions::locate(conn, &sid)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {sid}")))?;
        let Some((root, world_cfg)) = loc.world else {
            return Err(AppError::BadRequest(
                "This session has no world — assign it to a world first.".into(),
            ));
        };
        let summary = artifacts::latest_content(conn, &sid, "summary")?
            .ok_or_else(|| AppError::BadRequest("No summary yet — summarize first.".into()))?;
        let transcript = artifacts::latest_content(conn, &sid, "transcript")?
            .ok_or_else(|| AppError::BadRequest("No transcript for this session.".into()))?;
        let cfg = crate::config::get_config_map(conn)?;
        let resolved = llm::resolve(
            conn,
            &cfg,
            provider_o.as_deref(),
            model_o.as_deref(),
            base_o.as_deref(),
        )?;
        let language = crate::store::campaigns::get_campaign(conn, &world_cfg.id)
            .ok()
            .flatten()
            .map(|c| c.default_language)
            .filter(|s| !s.trim().is_empty())
            .or_else(|| cfg.get("default_language").cloned())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "en".into());
        let vault_root = world_cfg.codex_dir(&root);
        let pages = vault::list_pages(&vault_root)?;
        let rel_fields: HashMap<String, Vec<String>> = world_cfg
            .kind_schemas()
            .into_iter()
            .map(|(kind, fields)| {
                let lists = fields
                    .into_iter()
                    .filter(|f| f.ftype == "list")
                    .map(|f| f.name)
                    .collect();
                (kind, lists)
            })
            .collect();
        let world_ctx = crate::agent::context::world_context(&root, &world_cfg);
        Ok((
            loc.dir,
            vault_root,
            summary,
            transcript,
            resolved,
            language,
            pages,
            rel_fields,
            loc.st.number,
            world_ctx,
        ))
    })?;
    let (
        session_dir,
        vault_root,
        summary,
        transcript,
        resolved,
        language,
        pages,
        rel_fields,
        number,
        world_ctx,
    ) = prep;

    // A current run guards regeneration: its revision must still be the one the
    // GM was looking at, and its applied work is history, not a suggestion.
    let current = review::load(&session_dir)?;
    if let Some(loaded) = &current {
        let run_id = req.run_id.as_deref().unwrap_or_default();
        let base_revision = req.base_revision.as_deref().unwrap_or_default();
        if run_id != loaded.run.run_id {
            return Err(AppError::Conflict(
                "This is not the current review run.".into(),
            ));
        }
        review::require_revision(loaded, base_revision)?;
    } else if !req.development_ids.is_empty() {
        return Err(AppError::NotFound("No review for this session.".into()));
    }
    let applied_history: Vec<String> = current
        .as_ref()
        .map(|l| {
            l.run
                .developments
                .iter()
                .filter(|d| d.decision == Decision::Applied)
                .map(|d| d.title.clone())
                .collect()
        })
        .unwrap_or_default();

    let lang_name = crate::codex_import::language_name(&language);
    let stage1 = crate::agent::context::apply_world_context(
        &build_candidate_prompt(
            &summary,
            &pages,
            &rel_fields,
            &applied_history,
            number,
            &lang_name,
            req.include_possibilities,
        ),
        &world_ctx,
    );

    cancelled(cancel)?;
    let raw = llm::chat(&resolved.chat_req(&stage1), true)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Review generation failed: {}", e.0)))?;
    let candidates = match parse_candidates(&raw, &pages, req.include_possibilities) {
        Ok(c) => c,
        Err(first) => {
            // One bounded repair attempt, then a recoverable error — never an
            // apparently successful empty review.
            cancelled(cancel)?;
            let repair = repair_prompt(&stage1, &raw, &first.to_string());
            let retry = llm::chat(&resolved.chat_req(&repair), true)
                .await
                .map_err(|e| {
                    AppError::Internal(anyhow::anyhow!("Review generation failed: {}", e.0))
                })?;
            parse_candidates(&retry, &pages, req.include_possibilities)?
        }
    };

    // Stage 2 — verify the factual claims against the transcript.
    emit(GenProgress::Grounding);
    let turns = transcript_turns(&transcript);
    let factual: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.kind == CandidateKind::Development)
        .collect();
    let mut retrieved: HashMap<String, Vec<usize>> = HashMap::new();
    let mut retrieved_ordered: Vec<Vec<usize>> = Vec::with_capacity(factual.len());
    for c in &factual {
        let hits = matching_turns(&turns, &search_terms(c));
        retrieved.insert(c.id.clone(), hits.clone());
        retrieved_ordered.push(hits);
    }
    let verdicts = if factual.is_empty() {
        HashMap::new()
    } else {
        cancelled(cancel)?;
        let stage2 = build_grounding_prompt(&factual, &retrieved_ordered, &turns);
        match llm::chat(&resolved.chat_req(&stage2), true).await {
            Ok(raw) => parse_verdicts(&raw),
            Err(e) => {
                // Grounding is what makes a development trustworthy. Without it
                // everything falls back to a question, never to canon.
                tracing::warn!("review grounding failed, nothing is verified: {}", e.0);
                HashMap::new()
            }
        }
    };

    emit(GenProgress::Building);
    cancelled(cancel)?;
    let fresh = assemble(
        session_id,
        &resolved.provider,
        &resolved.model,
        &vault_root,
        &summary,
        &turns,
        candidates,
        &verdicts,
        &retrieved,
        review::current_source_revisions(&session_dir),
    )?;

    let run = match (current, req.development_ids.is_empty()) {
        (Some(loaded), false) => merge_regenerated(loaded.run, fresh, &req.development_ids)?,
        _ => fresh,
    };
    cancelled(cancel)?;
    review::save(&session_dir, &run)?;
    Ok(run)
}

// ── Clarification ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClarifyRequest {
    pub run_id: String,
    pub base_revision: String,
    pub question_id: String,
    /// What the GM says actually happened. This is the evidence.
    pub text: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

/// Turn a GM's answer to an open question into a development.
///
/// The evidence is the GM's own words, recorded verbatim — clarification
/// resolves uncertainty through their authority, never by inventing transcript
/// support the session does not contain. The question is only marked resolved
/// once the new development is persisted.
pub async fn clarify(
    state: &AppState,
    session_id: &str,
    req: &ClarifyRequest,
) -> AppResult<ReviewRun> {
    if req.text.trim().is_empty() {
        return Err(AppError::Unprocessable(
            "A clarification needs the GM's answer.".into(),
        ));
    }
    let sid = session_id.to_string();
    let (provider_o, model_o, base_o) = (
        req.provider.clone(),
        req.model.clone(),
        req.base_url.clone(),
    );
    let (session_dir, vault_root, resolved, language, pages, number) =
        state.with_db(move |conn| -> AppResult<_> {
            let loc = sessions::locate(conn, &sid)?
                .ok_or_else(|| AppError::NotFound(format!("Session not found: {sid}")))?;
            let Some((root, world_cfg)) = loc.world else {
                return Err(AppError::BadRequest(
                    "This session has no world — assign it to a world first.".into(),
                ));
            };
            let cfg = crate::config::get_config_map(conn)?;
            let resolved = llm::resolve(
                conn,
                &cfg,
                provider_o.as_deref(),
                model_o.as_deref(),
                base_o.as_deref(),
            )?;
            let language = cfg
                .get("default_language")
                .cloned()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "en".into());
            let vault_root = world_cfg.codex_dir(&root);
            let pages = vault::list_pages(&vault_root)?;
            Ok((
                loc.dir,
                vault_root,
                resolved,
                language,
                pages,
                loc.st.number,
            ))
        })?;

    let loaded = review::load(&session_dir)?
        .ok_or_else(|| AppError::NotFound("No review for this session.".into()))?;
    if loaded.run.run_id != req.run_id {
        return Err(AppError::Conflict(
            "This is not the current review run.".into(),
        ));
    }
    review::require_revision(&loaded, &req.base_revision)?;
    let mut run = loaded.run;
    let q_idx = run
        .questions
        .iter()
        .position(|q| q.id == req.question_id)
        .ok_or_else(|| AppError::NotFound("Unknown question.".into()))?;
    if run.questions[q_idx].status == QuestionStatus::Resolved {
        return Err(AppError::Conflict(
            "This question is already resolved.".into(),
        ));
    }

    let session_label = number
        .map(|n| format!("S{n}"))
        .unwrap_or_else(|| "S?".into());
    let mut page_list = String::new();
    for p in &pages {
        page_list.push_str(&format!(
            "- {} (kind: {}, path: {})\n",
            p.title,
            p.kind.as_deref().unwrap_or("lore"),
            p.path
        ));
    }
    let lang_name = crate::codex_import::language_name(&language);
    let prompt = format!(
        "The game master answered an open question about their session. Turn that answer \
into ONE codex update.\n\n\
Return ONLY a JSON object with a single item, same shape as before:\n\
{{\"items\": [{{\"classification\": \"development\", \"title\": \"page name\", \
\"kind\": \"pc|npc|place|faction|item|lore\", \"text\": \"what changed\", \
\"summary_new\": \"refreshed one-liner or null\", \
\"body_append\": \"{session_label} — 1-2 sentences, or null\", \
\"rels\": []}}]}}\n\n\
Rules:\n\
- Record only what the answer states. Do not add detail it does not contain.\n\
- Use the EXACT title from the page list when the page exists.\n\
- Write prose in {lang_name}. Keep proper names verbatim.\n\n\
Question:\n\"\"\"\n{}\n\"\"\"\n\n\
The game master's answer:\n\"\"\"\n{}\n\"\"\"\n\n\
Existing pages:\n{page_list}",
        run.questions[q_idx].text,
        req.text.trim()
    );

    let raw = llm::chat(&resolved.chat_req(&prompt), true)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Clarification failed: {}", e.0)))?;
    let candidate = parse_candidates(&raw, &pages, false)?
        .into_iter()
        .find(|c| c.kind == CandidateKind::Development)
        .ok_or_else(|| AppError::Unprocessable("The answer produced no codex update.".into()))?;

    let (rel, exists) = target_path(
        &vault_root,
        candidate.page.as_deref(),
        &candidate.title,
        &candidate.page_kind,
        candidate.folder.as_deref(),
    )?;
    if run
        .developments
        .iter()
        .any(|d| d.targets.iter().any(|t| t.path == rel))
    {
        return Err(AppError::Conflict(format!(
            "{rel} already has an update in this review. Adjust that one instead."
        )));
    }
    let (before, after) = render_target(
        &vault_root,
        &rel,
        exists,
        &candidate.title,
        &candidate.page_kind,
        &candidate.changes,
    )?;

    run.developments.push(Development {
        id: Uuid::new_v4().to_string(),
        title: candidate.title,
        description: if candidate.text.is_empty() {
            req.text.trim().to_string()
        } else {
            candidate.text
        },
        evidence: vec![Evidence::GmConfirmation {
            text: req.text.trim().to_string(),
            confirmed_at: crate::store::now(),
        }],
        targets: vec![Target {
            base_hash: match &before {
                Some(b) => review::hash_of(b),
                None => review::ABSENT_HASH.to_string(),
            },
            path: rel,
            before,
            after,
        }],
        decision: Decision::Pending,
        application_id: None,
        compound: false,
    });
    // Resolved only now that the candidate exists: a failure above leaves the
    // question open with its confirmation intact.
    run.questions[q_idx].status = QuestionStatus::Resolved;
    run.questions[q_idx].confirmation = Some(Evidence::GmConfirmation {
        text: req.text.trim().to_string(),
        confirmed_at: crate::store::now(),
    });
    review::validate(&run)?;
    review::save(&session_dir, &run)?;
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The shared regression fixture: docks session, magistrate exposed, the
    /// courier's fate unknown, a plan for next time, a captain's allegation.
    const TRANSCRIPT: &str = "[GM]\nThe magistrate stands before the crowd at the docks.\n\n\
[Player]\nI show everyone the forged letter and accuse him.\n\n\
[GM]\nThe crowd turns on him. He runs before the watch arrives.\n\n\
[Player]\nMaybe the courier has been released now?\n\n\
[GM]\nYou don't know where the courier is.\n\n\
[Player]\nNext time we should search the old warehouse.\n\n\
[Watch captain]\nI hear the dockworkers helped him escape.";

    const SUMMARY: &str = "The party publicly accused the magistrate at the docks and he fled.\n\n\
Whether the courier has been released is unknown.";

    fn fixture_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-gen-{tag}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("NPCs")).unwrap();
        std::fs::write(
            dir.join("NPCs/Magistrate.md"),
            "---\nkind: npc\nsummary: In office, allied with the city watch.\n---\n\n# Magistrate\n\n## Notes\n",
        )
        .unwrap();
        dir
    }

    fn pages_of(dir: &Path) -> Vec<vault::PageInfo> {
        vault::list_pages(dir).unwrap()
    }

    fn grounded(id: &str, start: usize, end: usize) -> HashMap<String, (bool, (usize, usize))> {
        HashMap::from([(id.to_string(), (true, (start, end)))])
    }

    fn assemble_fixture(
        dir: &Path,
        candidates: Vec<Candidate>,
        verdicts: &HashMap<String, (bool, (usize, usize))>,
    ) -> AppResult<ReviewRun> {
        let turns = transcript_turns(TRANSCRIPT);
        let mut retrieved = HashMap::new();
        for c in &candidates {
            retrieved.insert(c.id.clone(), matching_turns(&turns, &search_terms(c)));
        }
        assemble(
            "s12",
            "test",
            "test",
            dir,
            SUMMARY,
            &turns,
            candidates,
            verdicts,
            &retrieved,
            Vec::new(),
        )
    }

    #[test]
    fn accusation_is_a_development_and_the_release_stays_a_question() {
        let dir = fixture_vault("classify");
        let raw = r#"{"items":[
          {"classification":"development","title":"Magistrate","kind":"npc",
           "text":"Publicly accused at the docks; he fled before the watch arrived.",
           "summary_new":"Exposed at the docks and on the run.",
           "body_append":"S12 — Publicly accused; fled the docks.",
           "entities":["magistrate"]},
          {"classification":"question","title":"Courier",
           "text":"Has the courier been released?"}
        ]}"#;
        let candidates = parse_candidates(raw, &pages_of(&dir), false).unwrap();
        assert_eq!(candidates.len(), 2);

        let run = assemble_fixture(&dir, candidates, &grounded("c1", 1, 3)).unwrap();
        assert_eq!(run.developments.len(), 1);
        assert_eq!(run.developments[0].targets[0].path, "NPCs/Magistrate.md");
        assert!(matches!(
            run.developments[0].evidence[0],
            Evidence::Transcript { .. }
        ));
        assert_eq!(run.developments[0].decision, Decision::Pending);
        assert_eq!(run.questions.len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn an_unverified_claim_becomes_a_question_not_a_write() {
        let dir = fixture_vault("ungrounded");
        let raw = r#"{"items":[
          {"classification":"development","title":"Magistrate","kind":"npc",
           "text":"The dockworkers helped him escape.",
           "body_append":"S12 — The dockworkers helped him escape.",
           "entities":["magistrate"]}
        ]}"#;
        let candidates = parse_candidates(raw, &pages_of(&dir), false).unwrap();
        // The captain only claimed it, so the grounding pass says no.
        let run = assemble_fixture(&dir, candidates, &HashMap::new()).unwrap();
        assert!(run.developments.is_empty());
        assert_eq!(run.questions.len(), 1);
        assert!(matches!(
            run.questions[0].evidence[0],
            Evidence::Summary { .. }
        ));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn two_updates_to_one_page_become_one_compound_development() {
        let dir = fixture_vault("compound");
        let raw = r#"{"items":[
          {"classification":"development","title":"Magistrate","kind":"npc",
           "text":"Exposed at the docks.","summary_new":"Exposed and on the run.",
           "entities":["magistrate"]},
          {"classification":"development","title":"Magistrate","kind":"npc",
           "text":"Fled before the watch arrived.",
           "body_append":"S12 — Fled before the watch arrived.","entities":["magistrate"]}
        ]}"#;
        let candidates = parse_candidates(raw, &pages_of(&dir), false).unwrap();
        let mut verdicts = grounded("c1", 1, 3);
        verdicts.extend(grounded("c2", 1, 3));
        let run = assemble_fixture(&dir, candidates, &verdicts).unwrap();

        assert_eq!(run.developments.len(), 1);
        assert!(run.developments[0].compound);
        assert_eq!(run.developments[0].evidence.len(), 2);
        let after = &run.developments[0].targets[0].after;
        assert!(after.contains("Exposed and on the run."));
        assert!(after.contains("Fled before the watch arrived."));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn malformed_output_fails_instead_of_producing_an_empty_review() {
        let dir = fixture_vault("malformed");
        let pages = pages_of(&dir);
        assert!(parse_candidates("not json at all", &pages, false).is_err());
        assert!(parse_candidates(
            r#"{"items":[{"classification":"gossip","title":"X"}]}"#,
            &pages,
            false
        )
        .is_err());
        // A development that changes nothing is not a development.
        assert!(parse_candidates(
            r#"{"items":[{"classification":"development","title":"Magistrate","kind":"npc","text":"hm"}]}"#,
            &pages,
            false
        )
        .is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn possibilities_are_off_unless_asked_for() {
        let dir = fixture_vault("possibility");
        let raw = r#"{"items":[
          {"classification":"possibility","title":"Retaliation",
           "text":"The magistrate's allies may strike back.","links":["[[Magistrate]]"]}
        ]}"#;
        let pages = pages_of(&dir);
        assert!(parse_candidates(raw, &pages, false).unwrap().is_empty());

        let candidates = parse_candidates(raw, &pages, true).unwrap();
        let run = assemble_fixture(&dir, candidates, &HashMap::new()).unwrap();
        assert_eq!(run.possibilities.len(), 1);
        assert_eq!(run.possibilities[0].source_links, ["NPCs/Magistrate.md"]);
        assert!(run.developments.is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_model_supplied_path_cannot_escape_the_vault() {
        let dir = fixture_vault("traversal");
        let raw = r#"{"items":[
          {"classification":"development","title":"../../etc/passwd","kind":"lore",
           "text":"Nope.","summary_new":"Nope.","entities":["passwd"]}
        ]}"#;
        let candidates = parse_candidates(raw, &pages_of(&dir), false).unwrap();
        let run = assemble_fixture(&dir, candidates, &grounded("c1", 1, 2)).unwrap();
        for d in &run.developments {
            for t in &d.targets {
                assert!(!t.path.contains(".."), "escaped the vault: {}", t.path);
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn regeneration_replaces_only_the_named_groups() {
        let dir = fixture_vault("regen");
        let raw = r#"{"items":[
          {"classification":"development","title":"Magistrate","kind":"npc",
           "text":"Exposed at the docks.","summary_new":"Exposed and on the run.",
           "entities":["magistrate"]}
        ]}"#;
        let candidates = parse_candidates(raw, &pages_of(&dir), false).unwrap();
        let current = assemble_fixture(&dir, candidates.clone(), &grounded("c1", 1, 3)).unwrap();
        let fresh = assemble_fixture(&dir, candidates, &grounded("c1", 1, 3)).unwrap();
        let target_id = current.developments[0].id.clone();

        let merged =
            merge_regenerated(current.clone(), fresh.clone(), &[target_id.clone()]).unwrap();
        assert_eq!(merged.developments.len(), 1);
        assert_ne!(merged.developments[0].id, target_id);

        // Regenerating nothing while the fresh run claims a kept page is a
        // conflict, not a silent second owner.
        let err = merge_regenerated(current.clone(), fresh, &[]).unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));

        let mut applied = current.clone();
        applied.developments[0].decision = Decision::Applied;
        let err = merge_regenerated(
            applied,
            assemble_fixture(&dir, Vec::new(), &HashMap::new()).unwrap(),
            &[target_id],
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        std::fs::remove_dir_all(dir).ok();
    }
}
