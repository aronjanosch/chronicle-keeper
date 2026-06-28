//! Update the Codex (Phase 5): after a session is summarized, the LLM proposes
//! page edits — new pages, refreshed `summary:` one-liners, appended body notes,
//! relationship fields — as a reviewable set. Nothing writes until the user
//! commits. Two-stage generation keeps precision claims transcript-grounded:
//! stage 1 drafts candidates from the summary + page list; stage 2 verifies each
//! against retrieved transcript turns and cites the range. Proposals are
//! ephemeral until commit: one JSON file per run in `Sessions/<NNN>/`, never in
//! the index (files stay truth; a proposal is not yet true).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::llm;
use crate::state::AppState;
use crate::store::{artifacts, sessions};
use crate::vault;

pub const PROPOSALS_FILE: &str = "codex-proposals.json";

/// Max transcript turns retrieved per proposal for the grounding pass.
const MAX_TURNS_PER_PROPOSAL: usize = 40;
/// Max stored excerpt length per proposal (the "Show it" text).
const MAX_EXCERPT_CHARS: usize = 1500;
/// Max `## Notes`-tail characters injected per mentioned page in stage 1.
const TAIL_CHARS: usize = 600;

// ── Data model (codex-proposals.json) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalRun {
    pub session_id: String,
    pub generated_at: String,
    pub provider: String,
    pub model: String,
    /// open | committed | skipped
    pub status: String,
    /// Rough stage-2 input size, for the list footer.
    #[serde(default)]
    pub token_estimate: u64,
    pub proposals: Vec<Proposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    /// Relative vault path of the target page; `None` for new pages.
    pub page: Option<String>,
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub folder: Option<String>,
    pub changes: Vec<Change>,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub grounding: Option<Grounding>,
    /// Stage 2 could not verify this against the transcript.
    #[serde(default)]
    pub ungrounded: bool,
    /// accepted | rejected | edited
    pub decision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Change {
    Summary {
        #[serde(default)]
        old: String,
        new: String,
    },
    Body {
        anchor: String,
        text: String,
    },
    Rel {
        field: String,
        add: String,
        #[serde(default)]
        note: String,
    },
    New {
        #[serde(default)]
        summary: String,
        #[serde(default)]
        body: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grounding {
    /// Inclusive 1-based transcript turn range.
    pub turns: (usize, usize),
    /// The cited transcript lines (v1's "Hear it": show, don't play).
    pub excerpt: String,
}

pub enum UpdateProgress {
    /// Prompt sent, waiting on the candidate pass.
    Candidates,
    /// Candidates drafted; the transcript-grounding pass is running.
    Grounding,
}

// ── Run file I/O ──────────────────────────────────────────────────

pub fn run_path(session_dir: &Path) -> PathBuf {
    session_dir.join(PROPOSALS_FILE)
}

pub fn read_run(session_dir: &Path) -> AppResult<Option<ProposalRun>> {
    let raw = match std::fs::read_to_string(run_path(session_dir)) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Internal(anyhow::anyhow!("read proposals: {e}"))),
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("parse proposals: {e}")))
}

pub fn write_run(session_dir: &Path, run: &ProposalRun) -> AppResult<()> {
    let raw = serde_json::to_string_pretty(run)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("encode proposals: {e}")))?;
    std::fs::write(run_path(session_dir), raw)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write proposals: {e}")))
}

/// Session dir + vault root for a vault session (errors for bare uploads).
pub fn session_paths(state: &AppState, session_id: &str) -> AppResult<(PathBuf, PathBuf)> {
    let sid = session_id.to_string();
    state.with_db(move |conn| {
        let loc = sessions::locate(conn, &sid)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {sid}")))?;
        let Some((root, cfg)) = loc.world else {
            return Err(AppError::BadRequest(
                "This session has no world — assign it to a world first.".into(),
            ));
        };
        Ok((loc.dir, cfg.codex_dir(&root)))
    })
}

// ── Generation ────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct UpdateRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

pub async fn generate_streamed<F: FnMut(UpdateProgress) + Send>(
    state: &AppState,
    session_id: &str,
    req: &UpdateRequest,
    mut emit: F,
) -> AppResult<ProposalRun> {
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
        // kind → relationship-capable (list-type) infobox fields, for `rel` proposals.
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
        let number = loc.st.number;
        let world_ctx = crate::agent::context::world_context(&root, &world_cfg);
        Ok((
            loc.dir, vault_root, summary, transcript, resolved, language, pages, rel_fields,
            number, world_ctx,
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

    // Stage 1 — candidate pass (summary-scoped).
    emit(UpdateProgress::Candidates);
    let lang_name = crate::codex_import::language_name(&language);
    let stage1 = crate::agent::context::apply_world_context(
        &build_candidate_prompt(
            &summary,
            &pages,
            &vault_root,
            &rel_fields,
            number,
            &lang_name,
        ),
        &world_ctx,
    );
    let raw = llm::chat(&chat_req(&resolved, &stage1), true)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Codex update request failed: {}", e.0)))?;
    let (mut proposals, entity_hints) = parse_candidates(&raw, &pages);

    if proposals.is_empty() {
        let run = ProposalRun {
            session_id: session_id.to_string(),
            generated_at: crate::store::now(),
            provider: resolved.provider.clone(),
            model: resolved.model.clone(),
            status: "open".into(),
            token_estimate: 0,
            proposals,
        };
        write_run(&session_dir, &run)?;
        return Ok(run);
    }

    // Stage 2 — grounding pass (transcript-scoped).
    emit(UpdateProgress::Grounding);
    let turns = transcript_turns(&transcript);
    let mut retrieved: Vec<Vec<usize>> = Vec::with_capacity(proposals.len());
    for p in &proposals {
        let hints = entity_hints.get(&p.id).map(Vec::as_slice).unwrap_or(&[]);
        retrieved.push(matching_turns(&turns, &search_terms(p, hints)));
    }
    let stage2 = build_grounding_prompt(&proposals, &retrieved, &turns);
    let token_estimate = (stage2.len() / 4) as u64;
    let verdicts = match llm::chat(&chat_req(&resolved, &stage2), true).await {
        Ok(raw) => parse_verdicts(&raw),
        Err(e) => {
            // Grounding is what makes proposals trustworthy — don't ship
            // unverified claims as accepted; flag everything instead.
            tracing::warn!(
                "grounding pass failed, marking all proposals ungrounded: {}",
                e.0
            );
            HashMap::new()
        }
    };
    for (i, p) in proposals.iter_mut().enumerate() {
        let verdict = verdicts.get(&p.id);
        let grounded = verdict.map(|v| v.0).unwrap_or(false) && !retrieved[i].is_empty();
        if grounded {
            let (start, end) = clamp_range(verdict.unwrap().1, &retrieved[i]);
            p.grounding = Some(Grounding {
                turns: (start, end),
                excerpt: excerpt_of(&turns, start, end),
            });
            p.decision = "accepted".into();
        } else {
            p.ungrounded = true;
            p.decision = "rejected".into();
        }
    }

    let run = ProposalRun {
        session_id: session_id.to_string(),
        generated_at: crate::store::now(),
        provider: resolved.provider.clone(),
        model: resolved.model.clone(),
        status: "open".into(),
        token_estimate,
        proposals,
    };
    write_run(&session_dir, &run)?;
    Ok(run)
}

fn chat_req<'a>(resolved: &'a llm::Resolved, prompt: &'a str) -> llm::ChatRequest<'a> {
    llm::ChatRequest {
        transport: resolved.transport,
        api_base: &resolved.api_base,
        api_key: &resolved.api_key,
        model: &resolved.model,
        prompt,
        timeout_secs: resolved.timeout,
        num_ctx_max: resolved.num_ctx_max,
    }
}

// ── Stage 1: candidates ───────────────────────────────────────────

fn build_candidate_prompt(
    summary: &str,
    pages: &[vault::PageInfo],
    vault_root: &Path,
    rel_fields: &HashMap<String, Vec<String>>,
    session_number: Option<i64>,
    lang_name: &str,
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
    // Recent body tails of pages the summary mentions, so appends don't repeat
    // what a page already records.
    let mut tails = String::new();
    let summary_lower = crate::store::index::normalize_name(summary);
    let mut tail_count = 0;
    for p in pages {
        if tail_count >= 12 || p.title.len() < 3 {
            break;
        }
        if !summary_lower.contains(&crate::store::index::normalize_name(&p.title)) {
            continue;
        }
        if let Ok(page) = vault::read_page(vault_root, &p.path) {
            let (_, body) = vault::split_frontmatter(&page.content);
            let tail: String = tail_chars(body, TAIL_CHARS);
            if !tail.trim().is_empty() {
                tails.push_str(&format!("=== {} ===\n…{}\n\n", p.title, tail));
                tail_count += 1;
            }
        }
    }
    let mut rel_lines = String::new();
    for (kind, fields) in rel_fields {
        if !fields.is_empty() {
            rel_lines.push_str(&format!("  {kind}: {}\n", fields.join(", ")));
        }
    }
    let session_label = session_number
        .map(|n| format!("S{n}"))
        .unwrap_or_else(|| "S?".into());
    format!(
        "You maintain a tabletop-RPG campaign wiki (\"codex\"). A new session was \
summarized. Propose codex updates the wiki keeper should review.\n\n\
Return ONLY a JSON object:\n\
{{\"proposals\": [{{\n\
  \"title\": \"page name\",\n\
  \"kind\": \"pc|npc|place|faction|item|lore\",\n\
  \"is_new\": false,\n\
  \"folder\": \"folder for NEW pages only, picked from existing paths\",\n\
  \"summary_new\": \"refreshed one-liner, or null if unchanged\",\n\
  \"body_append\": \"1-3 sentences of new events to append under ## Notes, or null\",\n\
  \"rels\": [{{\"field\": \"allies\", \"add\": \"[[Other Page]]\", \"note\": \"why\"}}],\n\
  \"rationale\": \"one sentence: why this change\",\n\
  \"entities\": [\"names/aliases to locate this in the transcript\"]\n\
}}]}}\n\n\
Rules:\n\
- Only propose changes the summary clearly supports. Do not invent.\n\
- Existing pages: use the EXACT title from the page list; set is_new=false.\n\
- New pages only for entities that matter beyond this session; set is_new=true and pick a folder.\n\
- `summary_new` is the one-liner the summarizer memorizes (max ~25 words). Only when the old one is outdated.\n\
- `body_append` records session events; prefix with \"{session_label} — \". Don't repeat what the page tails below already say.\n\
- `rels` only for clear new relationships, using these list fields per kind (omit otherwise):\n{rel_lines}\
- Write prose in {lang_name}. Keep proper names verbatim.\n\
- 3–10 proposals; fewer is better than noisy.\n\n\
Session summary:\n\"\"\"\n{summary}\n\"\"\"\n\n\
Existing pages:\n{page_list}\n\
Recent page tails:\n{tails}"
    )
}

fn tail_chars(s: &str, n: usize) -> String {
    let trimmed = s.trim();
    let start = trimmed
        .char_indices()
        .rev()
        .take(n)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    trimmed[start..].to_string()
}

/// Returns the proposals plus stage-2 retrieval hints (entity names per
/// proposal id) — hints feed grounding but are not persisted.
fn parse_candidates(
    raw: &str,
    pages: &[vault::PageInfo],
) -> (Vec<Proposal>, HashMap<String, Vec<String>>) {
    let parsed = parse_json_lenient(raw);
    let arr = match &parsed {
        Value::Object(map) => map.get("proposals").and_then(Value::as_array).cloned(),
        Value::Array(a) => Some(a.clone()),
        _ => None,
    }
    .unwrap_or_default();

    let by_title: HashMap<String, &vault::PageInfo> = pages
        .iter()
        .map(|p| (crate::store::index::normalize_name(&p.title), p))
        .collect();

    let mut out = Vec::new();
    let mut hints: HashMap<String, Vec<String>> = HashMap::new();
    for (i, v) in arr.iter().enumerate() {
        let Some(obj) = v.as_object() else { continue };
        let s = |k: &str| {
            obj.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("null"))
                .map(str::to_string)
        };
        let Some(title) = s("title") else { continue };
        let kind = s("kind").unwrap_or_else(|| "lore".into()).to_lowercase();
        if !crate::vault::KINDS.contains(&kind.as_str()) {
            continue;
        }
        let existing = by_title.get(&crate::store::index::normalize_name(&title));
        let is_new = existing.is_none()
            || obj.get("is_new").and_then(Value::as_bool).unwrap_or(false) && existing.is_none();

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
            if let Some(text) = s("body_append") {
                changes.push(Change::Body {
                    anchor: "## Notes".into(),
                    text,
                });
            }
        }
        if let Some(rels) = obj.get("rels").and_then(Value::as_array) {
            for r in rels {
                let f = r.get("field").and_then(Value::as_str).unwrap_or("").trim();
                let a = r.get("add").and_then(Value::as_str).unwrap_or("").trim();
                if !f.is_empty() && !a.is_empty() {
                    changes.push(Change::Rel {
                        field: f.to_string(),
                        add: a.to_string(),
                        note: r
                            .get("note")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                    });
                }
            }
        }
        if changes.is_empty() {
            continue;
        }
        let entities: Vec<String> = obj
            .get("entities")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let id = format!("p{}", i + 1);
        hints.insert(id.clone(), entities);
        out.push(Proposal {
            id,
            page: existing.map(|p| p.path.clone()),
            title,
            kind: existing.and_then(|p| p.kind.clone()).unwrap_or(kind),
            folder: if is_new { s("folder") } else { None },
            changes,
            rationale: s("rationale").unwrap_or_default(),
            grounding: None,
            ungrounded: false,
            decision: "accepted".into(),
        });
    }
    (out, hints)
}

fn search_terms(p: &Proposal, hints: &[String]) -> Vec<String> {
    let mut terms = vec![p.title.clone()];
    terms.extend(hints.iter().cloned());
    for c in &p.changes {
        if let Change::Rel { add, .. } = c {
            terms.push(add.trim_matches(['[', ']']).to_string());
        }
    }
    terms.retain(|t| t.len() >= 3);
    terms.sort();
    terms.dedup();
    terms
}

// ── Stage 2: grounding ────────────────────────────────────────────

/// Number the transcript into 1-based "turns": one per text line, with the
/// current `[Speaker]` block label folded in.
pub(crate) fn transcript_turns(transcript: &str) -> Vec<String> {
    let mut turns = Vec::new();
    let mut speaker = String::new();
    for line in transcript.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
            speaker = line[1..line.len() - 1].to_string();
            continue;
        }
        if speaker.is_empty() {
            turns.push(line.to_string());
        } else {
            turns.push(format!("{speaker}: {line}"));
        }
    }
    turns
}

/// Turn indices (0-based) matching any term, with ±1 context, capped.
fn matching_turns(turns: &[String], terms: &[String]) -> Vec<usize> {
    let terms: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
    let mut hits: Vec<usize> = Vec::new();
    for (i, t) in turns.iter().enumerate() {
        let lower = t.to_lowercase();
        if terms.iter().any(|term| lower.contains(term)) {
            for j in i.saturating_sub(1)..=(i + 1).min(turns.len().saturating_sub(1)) {
                if hits.last() != Some(&j) && !hits.contains(&j) {
                    hits.push(j);
                }
            }
        }
        if hits.len() >= MAX_TURNS_PER_PROPOSAL {
            break;
        }
    }
    hits.truncate(MAX_TURNS_PER_PROPOSAL);
    hits
}

fn changes_digest(p: &Proposal) -> String {
    let mut parts = Vec::new();
    for c in &p.changes {
        match c {
            Change::Summary { new, .. } => parts.push(format!("new summary: {new}")),
            Change::Body { text, .. } => parts.push(format!("note: {text}")),
            Change::Rel { field, add, .. } => parts.push(format!("relationship {field}: {add}")),
            Change::New { summary, body } => parts.push(format!("new page: {summary} {body}")),
        }
    }
    parts.join(" | ")
}

fn build_grounding_prompt(
    proposals: &[Proposal],
    retrieved: &[Vec<usize>],
    turns: &[String],
) -> String {
    let mut claims = String::new();
    for (p, hits) in proposals.iter().zip(retrieved) {
        claims.push_str(&format!(
            "--- claim {} (page: {}) ---\n{}\n",
            p.id,
            p.title,
            changes_digest(p)
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
        "You verify proposed wiki changes against raw session-transcript lines. \
For each claim, decide whether the cited transcript lines actually support it.\n\n\
Return ONLY a JSON object:\n\
{{\"results\": [{{\"id\": \"p1\", \"grounded\": true, \"start\": 12, \"end\": 18}}]}}\n\n\
Rules:\n\
- `grounded` is true ONLY if the lines clearly support the claim. ASR text is \
noisy — allow misspelled names, but not invented facts.\n\
- `start`/`end` is the line-number range (from the numbers shown) that best \
supports the claim. Omit or null when not grounded.\n\
- Judge every claim.\n\n{claims}"
    )
}

/// id → (grounded, claimed (start,end) 1-based).
fn parse_verdicts(raw: &str) -> HashMap<String, (bool, (usize, usize))> {
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

/// Clamp the model's claimed range to turns we actually retrieved (1-based out).
fn clamp_range(claimed: (usize, usize), retrieved: &[usize]) -> (usize, usize) {
    let lo = retrieved.iter().min().map(|i| i + 1).unwrap_or(1);
    let hi = retrieved.iter().max().map(|i| i + 1).unwrap_or(1);
    let start = claimed.0.clamp(lo, hi);
    let end = claimed.1.clamp(start, hi);
    (start, end)
}

fn excerpt_of(turns: &[String], start: usize, end: usize) -> String {
    let mut out = String::new();
    for i in start..=end.min(turns.len()) {
        if i == 0 {
            continue;
        }
        let line = &turns[i - 1];
        if out.len() + line.len() > MAX_EXCERPT_CHARS {
            out.push('…');
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn parse_json_lenient(raw: &str) -> Value {
    serde_json::from_str(raw.trim())
        .or_else(|_| {
            let start = raw.find(['{', '[']);
            let end = raw.rfind(['}', ']']);
            match (start, end) {
                (Some(s), Some(e)) if e > s => serde_json::from_str(&raw[s..=e]),
                _ => Ok(Value::Null),
            }
        })
        .unwrap_or(Value::Null)
}

// ── Decisions + commit ────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct DecisionPatch {
    /// open | skipped (committed is set by the commit endpoint).
    pub status: Option<String>,
    #[serde(default)]
    pub proposals: Vec<ProposalPatch>,
}

#[derive(Debug, Deserialize)]
pub struct ProposalPatch {
    pub id: String,
    pub decision: Option<String>,
    /// Edited-before-commit: replaces the proposal's change list verbatim.
    pub changes: Option<Vec<Change>>,
}

pub fn apply_decisions(session_dir: &Path, patch: &DecisionPatch) -> AppResult<ProposalRun> {
    let mut run = read_run(session_dir)?
        .ok_or_else(|| AppError::NotFound("No codex-update run for this session.".into()))?;
    if let Some(status) = &patch.status {
        if matches!(status.as_str(), "open" | "skipped") {
            run.status = status.clone();
        }
    }
    for p in &patch.proposals {
        if let Some(target) = run.proposals.iter_mut().find(|x| x.id == p.id) {
            if let Some(d) = &p.decision {
                if matches!(d.as_str(), "accepted" | "rejected" | "edited") {
                    target.decision = d.clone();
                }
            }
            if let Some(c) = &p.changes {
                target.changes = c.clone();
            }
        }
    }
    write_run(session_dir, &run)?;
    Ok(run)
}

#[derive(Debug, Serialize)]
pub struct CommitReport {
    pub applied: usize,
    /// Proposals that could not apply cleanly (target page appeared/vanished).
    pub stale: Vec<String>,
    /// Vault-relative paths of every file touched.
    pub files: Vec<String>,
}

pub fn commit(session_dir: &Path, vault_root: &Path, ids: &[String]) -> AppResult<CommitReport> {
    let world_root = vault::world_root_of(vault_root);
    let mut run = read_run(session_dir)?
        .ok_or_else(|| AppError::NotFound("No codex-update run for this session.".into()))?;
    let mut report = CommitReport {
        applied: 0,
        stale: Vec::new(),
        files: Vec::new(),
    };

    for id in ids {
        let Some(p) = run.proposals.iter_mut().find(|x| &x.id == id) else {
            continue;
        };
        match apply_proposal(world_root.as_deref(), vault_root, p) {
            Ok(path) => {
                report.applied += 1;
                if !report.files.contains(&path) {
                    report.files.push(path);
                }
                if p.decision != "edited" {
                    p.decision = "accepted".into();
                }
            }
            Err(_) => {
                p.decision = "stale".into();
                report.stale.push(p.id.clone());
            }
        }
    }
    run.status = "committed".into();
    write_run(session_dir, &run)?;
    Ok(report)
}

/// Apply one proposal file-first; returns the vault-relative path touched.
fn apply_proposal(world_root: Option<&Path>, vault_root: &Path, p: &Proposal) -> AppResult<String> {
    match &p.page {
        None => {
            let stem = vault::safe_page_filename(&p.title);
            let rel = match p.folder.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(f) => format!("{f}/{stem}.md"),
                None => format!("{stem}.md"),
            };
            if vault_root.join(&rel).exists() {
                return Err(AppError::BadRequest(format!("Page already exists: {rel}")));
            }
            let (summary, body) = p
                .changes
                .iter()
                .find_map(|c| match c {
                    Change::New { summary, body } => Some((summary.clone(), body.clone())),
                    _ => None,
                })
                .unwrap_or_default();
            let mut content = vault::page_file_content(&p.title, &p.kind, &summary, &body);
            for c in &p.changes {
                if let Change::Rel { field, add, .. } = c {
                    content = vault::fm_append_list_value(&content, field, add);
                }
            }
            if let Some(wr) = world_root {
                let _ = crate::history::record_now(wr, vault_root, &rel, "keeper");
            }
            vault::write_page(vault_root, &rel, &content)?;
            Ok(rel)
        }
        Some(rel) => {
            let page = vault::read_page(vault_root, rel)?;
            let mut content = page.content;
            for c in &p.changes {
                match c {
                    Change::Summary { new, .. } => {
                        content = vault::overwrite_summary(&content, new);
                    }
                    Change::Body { anchor, text } => {
                        content = vault::append_under_heading(&content, anchor, text);
                    }
                    Change::Rel { field, add, .. } => {
                        content = vault::fm_append_list_value(&content, field, add);
                    }
                    Change::New { .. } => {}
                }
            }
            if let Some(wr) = world_root {
                let _ = crate::history::record_now(wr, vault_root, rel, "keeper");
            }
            vault::write_page(vault_root, rel, &content)?;
            Ok(rel.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_turns_fold_speakers() {
        let t = "[Aria]\nHello there.\nWe move on.\n\n[GM]\nThe door opens.";
        let turns = transcript_turns(t);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0], "Aria: Hello there.");
        assert_eq!(turns[2], "GM: The door opens.");
    }

    #[test]
    fn matching_turns_adds_context_and_caps() {
        let turns: Vec<String> = (0..100)
            .map(|i| {
                if i == 50 {
                    "Ulric appears".into()
                } else {
                    format!("line {i}")
                }
            })
            .collect();
        let hits = matching_turns(&turns, &["ulric".into()]);
        assert_eq!(hits, vec![49, 50, 51]);
    }

    #[test]
    fn parse_candidates_maps_existing_pages_and_kinds() {
        let pages = vec![vault::PageInfo {
            path: "NPCs/Ulric.md".into(),
            title: "Ulric".into(),
            kind: Some("npc".into()),
            summary: "Old one-liner.".into(),
            modified: None,
        }];
        let raw = r#"{"proposals":[
            {"title":"Ulric","kind":"npc","is_new":false,"summary_new":"New liner.","body_append":"S14 — thing happened.","rationale":"r","entities":["Ulric"]},
            {"title":"The Bronze Sigil","kind":"lore","is_new":true,"folder":"Lore","summary_new":"A sigil.","rationale":"new"},
            {"title":"Bad","kind":"weapon","summary_new":"x"}
        ]}"#;
        let (out, hints) = parse_candidates(raw, &pages);
        assert_eq!(out.len(), 2);
        assert_eq!(hints["p1"], vec!["Ulric".to_string()]);
        assert_eq!(out[0].page.as_deref(), Some("NPCs/Ulric.md"));
        assert!(matches!(out[0].changes[0], Change::Summary { .. }));
        assert!(matches!(out[0].changes[1], Change::Body { .. }));
        assert!(out[1].page.is_none());
        assert!(matches!(out[1].changes[0], Change::New { .. }));
        assert_eq!(out[1].folder.as_deref(), Some("Lore"));
    }

    #[test]
    fn parse_verdicts_reads_ranges() {
        let raw = r#"{"results":[{"id":"p1","grounded":true,"start":5,"end":9},{"id":"p2","grounded":false}]}"#;
        let v = parse_verdicts(raw);
        assert_eq!(v["p1"], (true, (5, 9)));
        assert_eq!(v["p2"].0, false);
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-cu-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn commit_applies_summary_body_rel_and_new_page() {
        let vault_root = tmp_dir("commit-vault");
        let sess = tmp_dir("commit-sess");
        std::fs::create_dir_all(vault_root.join("NPCs")).unwrap();
        std::fs::write(
            vault_root.join("NPCs/Ulric.md"),
            "---\nkind: npc\nsummary: Old liner.\nallies: [\"[[Mira]]\"]\n---\n\n# Ulric\n\n## Notes\n\nS13 — old note.\n",
        )
        .unwrap();
        let run = ProposalRun {
            session_id: "s1".into(),
            generated_at: "t".into(),
            provider: "p".into(),
            model: "m".into(),
            status: "open".into(),
            token_estimate: 0,
            proposals: vec![
                Proposal {
                    id: "p1".into(),
                    page: Some("NPCs/Ulric.md".into()),
                    title: "Ulric".into(),
                    kind: "npc".into(),
                    folder: None,
                    changes: vec![
                        Change::Summary {
                            old: "Old liner.".into(),
                            new: "New liner.".into(),
                        },
                        Change::Body {
                            anchor: "## Notes".into(),
                            text: "S14 — new note.".into(),
                        },
                        Change::Rel {
                            field: "allies".into(),
                            add: "[[Vassa]]".into(),
                            note: String::new(),
                        },
                    ],
                    rationale: String::new(),
                    grounding: None,
                    ungrounded: false,
                    decision: "accepted".into(),
                },
                Proposal {
                    id: "p2".into(),
                    page: None,
                    title: "The Bronze Sigil".into(),
                    kind: "lore".into(),
                    folder: Some("Lore".into()),
                    changes: vec![Change::New {
                        summary: "A sigil.".into(),
                        body: "S14 — found.".into(),
                    }],
                    rationale: String::new(),
                    grounding: None,
                    ungrounded: false,
                    decision: "accepted".into(),
                },
            ],
        };
        write_run(&sess, &run).unwrap();
        let report = commit(&sess, &vault_root, &["p1".into(), "p2".into()]).unwrap();
        assert_eq!(report.applied, 2);
        assert!(report.stale.is_empty());

        let ulric = std::fs::read_to_string(vault_root.join("NPCs/Ulric.md")).unwrap();
        assert!(ulric.contains("summary: \"New liner.\""));
        assert!(ulric.contains("S13 — old note."));
        assert!(ulric.contains("S14 — new note."));
        assert!(ulric.contains("[[Mira]]"));
        assert!(ulric.contains("[[Vassa]]"));

        let sigil = std::fs::read_to_string(vault_root.join("Lore/The Bronze Sigil.md")).unwrap();
        assert!(sigil.contains("kind: lore"));
        assert!(sigil.contains("summary: \"A sigil.\""));
        assert!(sigil.contains("S14 — found."));

        let back = read_run(&sess).unwrap().unwrap();
        assert_eq!(back.status, "committed");

        for d in [&vault_root, &sess] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn commit_marks_collision_stale() {
        let vault_root = tmp_dir("stale-vault");
        let sess = tmp_dir("stale-sess");
        std::fs::write(vault_root.join("Thing.md"), "# Thing\n").unwrap();
        let run = ProposalRun {
            session_id: "s1".into(),
            generated_at: "t".into(),
            provider: "p".into(),
            model: "m".into(),
            status: "open".into(),
            token_estimate: 0,
            proposals: vec![Proposal {
                id: "p1".into(),
                page: None,
                title: "Thing".into(),
                kind: "lore".into(),
                folder: None,
                changes: vec![Change::New {
                    summary: String::new(),
                    body: String::new(),
                }],
                rationale: String::new(),
                grounding: None,
                ungrounded: false,
                decision: "accepted".into(),
            }],
        };
        write_run(&sess, &run).unwrap();
        let report = commit(&sess, &vault_root, &["p1".into()]).unwrap();
        assert_eq!(report.applied, 0);
        assert_eq!(report.stale, vec!["p1".to_string()]);
        assert_eq!(
            read_run(&sess).unwrap().unwrap().proposals[0].decision,
            "stale"
        );
        for d in [&vault_root, &sess] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn decisions_patch_merges() {
        let sess = tmp_dir("dec");
        let run = ProposalRun {
            session_id: "s1".into(),
            generated_at: "t".into(),
            provider: "p".into(),
            model: "m".into(),
            status: "open".into(),
            token_estimate: 0,
            proposals: vec![Proposal {
                id: "p1".into(),
                page: None,
                title: "X".into(),
                kind: "lore".into(),
                folder: None,
                changes: vec![Change::New {
                    summary: "a".into(),
                    body: String::new(),
                }],
                rationale: String::new(),
                grounding: None,
                ungrounded: false,
                decision: "accepted".into(),
            }],
        };
        write_run(&sess, &run).unwrap();
        let patch = DecisionPatch {
            status: Some("skipped".into()),
            proposals: vec![ProposalPatch {
                id: "p1".into(),
                decision: Some("edited".into()),
                changes: Some(vec![Change::New {
                    summary: "edited".into(),
                    body: String::new(),
                }]),
            }],
        };
        let updated = apply_decisions(&sess, &patch).unwrap();
        assert_eq!(updated.status, "skipped");
        assert_eq!(updated.proposals[0].decision, "edited");
        assert!(
            matches!(&updated.proposals[0].changes[0], Change::New { summary, .. } if summary == "edited")
        );
        std::fs::remove_dir_all(&sess).ok();
    }
}
