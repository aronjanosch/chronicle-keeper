//! Keeper preparation suggestions (SC-07).
//!
//! Suggestions are proposals: this module reads, prompts, and returns: it never
//! writes `prep.md` or a world page. Accepting one goes through the ordinary
//! prep mutation path, so nothing here can put text in front of the GM that
//! they did not choose to keep.
//!
//! The context is a fixed priority ladder under one character budget — selected
//! threads and linked pages first, the current preparation next, then the three
//! most recent summaries, and the world brief last. Every included source is
//! labeled in the prompt and every omission is stated, so a suggestion that
//! cites something can be traced to text the model was actually given.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::sessions;
use crate::{llm, vault};

/// The UX caps the panel at five; the server enforces it so a chatty model
/// cannot flood the panel.
pub const MAX_SUGGESTIONS: usize = 5;
/// Total injected source text. Prompt scaffolding is not counted; sources are.
const MAX_CONTEXT_CHARS: usize = 24_000;
const MAX_LINKED_PAGES: usize = 12;
const RECENT_SUMMARIES: usize = 3;
const MIN_USEFUL_CHARS: usize = 400;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestRequest {
    #[serde(default)]
    pub instruction: Option<String>,
    /// Extra page/thread paths the GM pointed at, on top of the prep's own.
    #[serde(default)]
    pub linked_paths: Vec<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub id: String,
    /// opening | scene | reminder — the prep sections.
    pub section: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub text: String,
    /// Only paths that were part of the supplied context survive.
    pub links: Vec<String>,
    /// Why this fits, for the card's expandable rationale.
    pub why: String,
    /// `grounded` when it follows from the supplied sources, `idea` when it is
    /// the Keeper's own invention. An idea is never presented as world fact.
    pub kind: String,
}

pub enum SuggestProgress {
    Reading,
    Building,
}

/// One labeled block of source text.
struct Source {
    label: String,
    path: Option<String>,
    body: String,
}

/// Fill the budget in priority order. Returns the rendered context, the paths
/// the model is allowed to cite, and the labels that did not fit.
fn budgeted(sources: Vec<Source>) -> (String, Vec<String>, Vec<String>) {
    let mut out = String::new();
    let mut cited = Vec::new();
    let mut omitted = Vec::new();
    let mut left = MAX_CONTEXT_CHARS;

    for source in sources {
        let total = source.body.chars().count();
        if total == 0 {
            continue;
        }
        if left < MIN_USEFUL_CHARS {
            omitted.push(source.label);
            continue;
        }
        let (body, dropped) = if total <= left {
            (source.body.clone(), 0)
        } else {
            let kept = truncate_on_boundary(&source.body, left);
            let dropped = total - kept.chars().count();
            (kept, dropped)
        };
        let header = match &source.path {
            Some(path) => format!("## {} ({})", source.label, path),
            None => format!("## {}", source.label),
        };
        out.push_str(&header);
        out.push('\n');
        out.push_str(body.trim_end());
        out.push('\n');
        if dropped > 0 {
            out.push_str(&format!("_[{dropped} characters omitted]_\n"));
        }
        out.push('\n');
        left = left.saturating_sub(body.chars().count());
        if let Some(path) = source.path {
            cited.push(path);
        }
    }
    (out, cited, omitted)
}

/// Cut at the last paragraph break, then the last line break, then the last
/// space inside the budget, so a source never ends mid-word.
fn truncate_on_boundary(body: &str, budget: usize) -> String {
    let head: String = body.chars().take(budget).collect();
    for sep in ["\n\n", "\n", " "] {
        if let Some(at) = head.rfind(sep) {
            if at >= budget / 2 {
                return head[..at].to_string();
            }
        }
    }
    head
}

fn page_source(vault_root: &Path, rel: &str) -> Option<Source> {
    let page = vault::read_page(vault_root, rel).ok()?;
    Some(Source {
        label: format!("Linked page — {}", page.title),
        path: Some(rel.to_string()),
        body: page.content,
    })
}

/// Summaries of the sessions before this one, newest first.
fn recent_summaries(world_root: &Path, current_number: Option<i64>) -> Vec<Source> {
    let mut entries: Vec<(i64, std::path::PathBuf)> = sessions::session_dirs(world_root)
        .into_iter()
        .filter_map(|dir| {
            let st = crate::session_files::read_session_toml(&dir).ok()??;
            let number = st.number?;
            match current_number {
                Some(current) if number >= current => None,
                _ => Some((number, dir)),
            }
        })
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    entries
        .into_iter()
        .filter_map(|(number, dir)| {
            let body = std::fs::read_to_string(crate::session_files::summary_md_path(&dir)).ok()?;
            (!body.trim().is_empty()).then(|| Source {
                label: format!("Summary of session {number}"),
                path: None,
                body,
            })
        })
        .take(RECENT_SUMMARIES)
        .collect()
}

fn prep_source(session_dir: &Path) -> Option<Source> {
    let prep = crate::session_prep::read(session_dir).ok()?;
    if prep.cards.is_empty() && prep.notes.trim().is_empty() {
        return None;
    }
    let mut body = String::new();
    for card in &prep.cards {
        let section = serde_json::to_value(card.section)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "scene".into());
        let title = card.title.clone().unwrap_or_default();
        body.push_str(&format!("- [{section}] {title} {}\n", card.text));
    }
    if !prep.notes.trim().is_empty() {
        body.push_str("\nNotes:\n");
        body.push_str(prep.notes.trim());
        body.push('\n');
    }
    Some(Source {
        label: "Preparation so far".into(),
        path: None,
        body,
    })
}

fn build_prompt(
    instruction: Option<&str>,
    context: &str,
    cited: &[String],
    omitted: &[String],
    language: &str,
    next_number: Option<i64>,
) -> String {
    let focus = instruction
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("The GM asks you to focus on: {s}\n\n"))
        .unwrap_or_default();
    let session = next_number
        .map(|n| format!("session {n}"))
        .unwrap_or_else(|| "the next session".into());
    let paths = if cited.is_empty() {
        "(no page paths were supplied — return an empty `links` array)".to_string()
    } else {
        cited.join("\n")
    };
    let missing = if omitted.is_empty() {
        String::new()
    } else {
        format!(
            "These sources did not fit the context budget, so say nothing about them: {}.\n",
            omitted.join(", ")
        )
    };
    format!(
        "You are helping a game master prepare {session}. Suggest at most \
         {MAX_SUGGESTIONS} things worth preparing.\n\n\
         {focus}\
         Write in {language}. Return JSON only:\n\
         {{\"items\":[{{\"section\":\"opening|scene|reminder\",\"title\":\"short label\",\
         \"text\":\"one or two sentences\",\"links\":[\"exact page path\"],\
         \"why\":\"what in the sources this follows from\",\"kind\":\"grounded|idea\"}}]}}\n\n\
         Rules:\n\
         - A suggestion is a possibility, never a claim that something happened.\n\
         - `kind` is `grounded` only when the sources below support it; otherwise it is \
         `idea`, your own invention, and `why` must say so.\n\
         - `links` may contain only these exact paths, and only when relevant:\n{paths}\n\
         - At most one `opening`. Prefer unresolved business over new inventions.\n\
         - Do not repeat what the preparation already contains.\n\
         {missing}\n\
         Sources follow.\n\n{context}"
    )
}

/// Parse the model's array. Unknown sections are dropped rather than coerced,
/// links outside the supplied context are removed, and the cap is enforced here
/// so nothing downstream has to trust the model's arithmetic.
pub(crate) fn parse_suggestions(raw: &str, cited: &[String]) -> AppResult<Vec<Suggestion>> {
    let parsed = crate::codex_update::parse_json_lenient(raw);
    let arr = match &parsed {
        Value::Object(map) => map
            .get("items")
            .or_else(|| map.get("suggestions"))
            .and_then(Value::as_array)
            .cloned(),
        Value::Array(a) => Some(a.clone()),
        _ => None,
    }
    .ok_or_else(|| AppError::Unprocessable("response has no `items` array".into()))?;

    let allowed: HashSet<&str> = cited.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    let mut openings = 0;
    for value in arr {
        if out.len() >= MAX_SUGGESTIONS {
            break;
        }
        let Some(obj) = value.as_object() else {
            continue;
        };
        let s = |key: &str| {
            obj.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let section = s("section").unwrap_or_default();
        if !matches!(section.as_str(), "opening" | "scene" | "reminder") {
            continue;
        }
        if section == "opening" {
            openings += 1;
            if openings > 1 {
                continue;
            }
        }
        let Some(text) = s("text") else { continue };
        let links = obj
            .get("links")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|link| allowed.contains(*link))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let kind = match s("kind").as_deref() {
            Some("grounded") => "grounded",
            _ => "idea",
        };
        out.push(Suggestion {
            id: uuid::Uuid::new_v4().to_string(),
            section,
            title: s("title"),
            text,
            links,
            why: s("why").unwrap_or_default(),
            kind: kind.to_string(),
        });
    }
    if out.is_empty() {
        return Err(AppError::Unprocessable(
            "no usable suggestions in the response".into(),
        ));
    }
    Ok(out)
}

fn cancelled(cancel: &AtomicBool) -> AppResult<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::BadRequest("Cancelled".into()));
    }
    Ok(())
}

/// Read context, ask once, return proposals. One provider call per run — the
/// panel renders whatever comes back without asking again per card.
pub async fn suggest_streamed<F: FnMut(SuggestProgress) + Send>(
    state: &AppState,
    session_id: &str,
    req: &SuggestRequest,
    cancel: &AtomicBool,
    mut emit: F,
) -> AppResult<Vec<Suggestion>> {
    emit(SuggestProgress::Reading);
    let sid = session_id.to_string();
    let (provider, model, base) = (
        req.provider.clone(),
        req.model.clone(),
        req.base_url.clone(),
    );
    let extra_paths = req.linked_paths.clone();
    let instruction = req.instruction.clone();
    let (prompt, resolved, cited) = state.with_db(move |conn| -> AppResult<_> {
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
            provider.as_deref(),
            model.as_deref(),
            base.as_deref(),
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

        // Priority ladder: what the GM pointed at, then their own preparation,
        // then recent play, then the world brief.
        let prep = crate::session_prep::read(&loc.dir)?;
        let mut wanted: Vec<String> = Vec::new();
        for path in prep
            .selected_threads
            .iter()
            .chain(prep.cards.iter().flat_map(|c| c.links.iter()))
            .chain(extra_paths.iter())
        {
            if !wanted.iter().any(|p| p == path) {
                wanted.push(path.clone());
            }
        }
        let mut sources: Vec<Source> = wanted
            .iter()
            .take(MAX_LINKED_PAGES)
            .filter_map(|rel| page_source(&vault_root, rel))
            .collect();
        sources.extend(prep_source(&loc.dir));
        sources.extend(recent_summaries(&root, loc.st.number));
        if let Some(brief) = crate::agent::brief::read(&root) {
            sources.push(Source {
                label: "World brief".into(),
                path: None,
                body: brief.body,
            });
        }

        let (context, cited, omitted) = budgeted(sources);
        let lang_name = crate::codex_import::language_name(&language);
        let prompt = crate::agent::context::apply_world_context(
            &build_prompt(
                instruction.as_deref(),
                &context,
                &cited,
                &omitted,
                &lang_name,
                loc.st.number.map(|n| n + 1),
            ),
            &crate::agent::context::world_context(&root, &world_cfg),
        );
        Ok((prompt, resolved, cited))
    })?;

    cancelled(cancel)?;
    emit(SuggestProgress::Building);
    let raw = llm::chat(&resolved.chat_req(&prompt), true)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Prep suggestions failed: {}", e.0)))?;
    cancelled(cancel)?;
    parse_suggestions(&raw, &cited)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(label: &str, path: Option<&str>, body: &str) -> Source {
        Source {
            label: label.into(),
            path: path.map(str::to_string),
            body: body.into(),
        }
    }

    #[test]
    fn budget_keeps_priority_order_and_states_omissions() {
        let big = "word ".repeat(MAX_CONTEXT_CHARS / 4);
        let (context, cited, omitted) = budgeted(vec![
            source("Linked page — Magistrate", Some("NPCs/Magistrate.md"), &big),
            source("Preparation so far", None, "- [scene] courier"),
            source("Summary of session 11", None, "The docks were quiet."),
        ]);
        assert!(context.starts_with("## Linked page — Magistrate (NPCs/Magistrate.md)"));
        assert!(context.contains("characters omitted]_"));
        assert_eq!(cited, vec!["NPCs/Magistrate.md".to_string()]);
        // The page ate the budget, so the later sources are named, not guessed at.
        assert!(omitted.contains(&"Summary of session 11".to_string()));
        assert!(context.chars().count() < MAX_CONTEXT_CHARS + 500);
    }

    #[test]
    fn truncation_does_not_split_a_word() {
        let kept = truncate_on_boundary("alpha beta gamma delta", 14);
        assert_eq!(kept, "alpha beta");
    }

    #[test]
    fn parse_caps_sections_links_and_openings() {
        let raw = r#"{"items":[
          {"section":"opening","text":"At the east gate.","links":["NPCs/Magistrate.md"],
           "why":"the prep opens there","kind":"grounded"},
          {"section":"opening","text":"A second opening.","kind":"grounded"},
          {"section":"quest","text":"Unknown section."},
          {"section":"scene","text":"The courier returns.","links":["NPCs/Ghost.md"],"kind":"idea"},
          {"section":"reminder","text":"One."},
          {"section":"reminder","text":"Two."},
          {"section":"reminder","text":"Three."},
          {"section":"reminder","text":"Four."}
        ]}"#;
        let cited = vec!["NPCs/Magistrate.md".to_string()];
        let out = parse_suggestions(raw, &cited).unwrap();
        assert_eq!(out.len(), MAX_SUGGESTIONS);
        assert_eq!(out.iter().filter(|s| s.section == "opening").count(), 1);
        assert_eq!(out[0].links, vec!["NPCs/Magistrate.md".to_string()]);
        // A link that was never in the context cannot survive the copy.
        let scene = out.iter().find(|s| s.section == "scene").unwrap();
        assert!(scene.links.is_empty());
        assert_eq!(scene.kind, "idea");
        // No `kind` at all is an idea, never presented as grounded.
        assert!(out.iter().all(|s| s.kind == "grounded" || s.kind == "idea"));
        assert_eq!(out.last().unwrap().kind, "idea");
    }

    #[test]
    fn parse_rejects_output_without_usable_items() {
        assert!(parse_suggestions("{\"items\":[]}", &[]).is_err());
        assert!(parse_suggestions("not json at all", &[]).is_err());
    }
}
