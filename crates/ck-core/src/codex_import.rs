//! LLM-powered codex import. The user pastes campaign notes in any shape — one
//! file per NPC, one giant document, a Notion/Google-Doc export, loose bullets —
//! and the configured summary LLM distills them into structured codex entries
//! (`name + kind + one-line body`). We never parse note formats ourselves: the
//! model is the parser. The proposed entries are returned for review and saved
//! only on an explicit commit, because a bulk extract can over- or mis-extract.

use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::llm;
use crate::models::CodexEntryCreate;
use crate::state::AppState;
use crate::store::codex::KINDS;

/// Upper bound on pasted note size. Past this, a single import is both useless
/// (it overruns the model's context) and a foot-gun (runaway token cost on a BYO
/// cloud key). Distilling notes in batches is the right move at this scale.
const MAX_IMPORT_BYTES: usize = 256 * 1024;

/// Distill pasted notes into proposed codex entries. Does not touch the DB
/// beyond reading config — the caller reviews and commits. The codex inherits
/// the campaign's summary provider/model so the import "voice" matches summaries,
/// and writes one-line bodies in the campaign's language (falling back to config,
/// then English).
pub async fn import(
    state: &AppState,
    campaign_id: &str,
    text: &str,
) -> AppResult<Vec<CodexEntryCreate>> {
    let text = text.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest(
            "Nothing to import — paste some notes first.".into(),
        ));
    }
    if text.len() > MAX_IMPORT_BYTES {
        return Err(AppError::BadRequest(format!(
            "Notes are too large to import at once ({} KB; max {} KB). Split them and import in batches.",
            text.len() / 1024,
            MAX_IMPORT_BYTES / 1024,
        )));
    }
    let (target, language) = state.with_db(|conn| -> AppResult<_> {
        let cfg = crate::config::get_config_map(conn)?;
        let target = llm::resolve(conn, &cfg, None, None, None)?;
        let language = crate::store::campaigns::get_campaign(conn, campaign_id)
            .ok()
            .flatten()
            .map(|c| c.default_language)
            .filter(|s| !s.trim().is_empty())
            .or_else(|| cfg.get("default_language").cloned())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "en".into());
        Ok((target, language))
    })?;

    let prompt = build_prompt(text, &language);
    let raw = llm::chat(
        &llm::ChatRequest {
            transport: target.transport,
            api_base: &target.api_base,
            api_key: &target.api_key,
            model: &target.model,
            prompt: &prompt,
            timeout_secs: target.timeout,
            num_ctx_max: target.num_ctx_max,
        },
        /* json_mode */ true,
    )
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("LLM import request failed: {}", e.0)))?;

    Ok(parse_entries(&raw))
}

fn build_prompt(notes: &str, language: &str) -> String {
    let lang_name = language_name(language);
    format!(
        "You are extracting a tabletop-RPG campaign glossary (\"codex\") from a Game \
Master's raw notes. The notes may be in ANY shape: one file per entity, one big \
document, a Notion or Google-Doc export, or loose bullet points. Ignore all \
formatting, headings, frontmatter, and markup — read for meaning.\n\n\
Return ONLY a JSON object of the form:\n\
{{\"entries\": [{{\"name\": \"...\", \"kind\": \"...\", \"body\": \"...\", \"detail\": \"...\"}}]}}\n\n\
Rules:\n\
- `kind` MUST be exactly one of these English keywords: pc, npc, place, faction, item, lore. \
Use `pc` for a player character / member of the party, `npc` for any other character. Do NOT translate the kind.\n\
- `name` is the proper name of the thing, copied VERBATIM from the notes (keep the original spelling and language — do not translate names). Omit leading articles unless part of the name.\n\
- `body` is ONE concise sentence (max ~25 words) capturing what the summarizer should \
know about it. Write the body in {lang_name}. No markdown, no line breaks.\n\
- `detail` is a SHORT distilled paragraph (2-5 sentences) with the richer context worth \
remembering — relationships, motives, secrets, appearance. Distill and rewrite in your own \
words; do NOT copy the raw notes verbatim. Write in {lang_name}. Plain prose, no markdown. \
Leave it an empty string if the notes say nothing beyond the one-liner.\n\
- Create one entry per distinct entity. Merge duplicates. Do not invent anything \
that is not in the notes.\n\
- Include player characters / party members as `pc` entries when the notes describe them.\n\
- If the notes contain nothing glossary-worthy, return {{\"entries\": []}}.\n\n\
Notes:\n\
\"\"\"\n{notes}\n\"\"\""
    )
}

/// Map an ISO-ish language code to a name the model reliably understands. Falls
/// back to the raw code (LLMs handle bare codes fine) for anything unlisted.
fn language_name(code: &str) -> String {
    match code
        .trim()
        .to_lowercase()
        .split(['-', '_'])
        .next()
        .unwrap_or("")
    {
        "de" => "German".into(),
        "en" => "English".into(),
        "fr" => "French".into(),
        "es" => "Spanish".into(),
        "it" => "Italian".into(),
        "pt" => "Portuguese".into(),
        "nl" => "Dutch".into(),
        "pl" => "Polish".into(),
        "ru" => "Russian".into(),
        "" => "the same language as the notes".into(),
        other => format!("the language with code \"{other}\""),
    }
}

/// Tolerant parse of the model's JSON. Accepts `{\"entries\": [...]}` or a bare
/// array, and entries that are strings or objects. Validates kind, trims, and
/// dedupes within the batch (case-insensitive name+kind).
fn parse_entries(raw: &str) -> Vec<CodexEntryCreate> {
    let parsed: Value = serde_json::from_str(raw.trim())
        .or_else(|_| {
            // Some models wrap JSON in prose or fences — grab the first {...} or [...].
            let start = raw.find(['{', '[']);
            let end = raw.rfind(['}', ']']);
            match (start, end) {
                (Some(s), Some(e)) if e > s => serde_json::from_str(&raw[s..=e]),
                _ => Ok(Value::Null),
            }
        })
        .unwrap_or(Value::Null);

    let arr = match &parsed {
        Value::Object(map) => map.get("entries").and_then(Value::as_array).cloned(),
        Value::Array(a) => Some(a.clone()),
        _ => None,
    }
    .unwrap_or_default();

    let mut out: Vec<CodexEntryCreate> = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for v in &arr {
        let Some(obj) = v.as_object() else { continue };
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let kind = obj
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let body = obj
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let detail = obj
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() || !KINDS.contains(&kind.as_str()) {
            continue;
        }
        if seen.insert((name.to_lowercase(), kind.clone())) {
            out.push(CodexEntryCreate {
                name,
                kind,
                body,
                detail,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wrapped_object() {
        let raw =
            r#"{"entries":[{"name":"Aragorn","kind":"npc","body":"Ranger heir of Isildur"}]}"#;
        let e = parse_entries(raw);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].name, "Aragorn");
        assert_eq!(e[0].kind, "npc");
    }

    #[test]
    fn parses_bare_array_and_fenced_prose() {
        let raw = "Here you go:\n```json\n[{\"name\":\"Bree\",\"kind\":\"place\",\"body\":\"Town\"}]\n```";
        let e = parse_entries(raw);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].kind, "place");
    }

    #[test]
    fn prompt_localizes_body_language() {
        let de = build_prompt("notes", "de");
        assert!(de.contains("Write the body in German"));
        // Region tags collapse to the base language.
        assert!(build_prompt("notes", "de-CH").contains("Write the body in German"));
        // Unknown codes fall back without panicking.
        assert!(build_prompt("notes", "xx").contains("code \"xx\""));
        // The kind enum stays English regardless of language.
        assert!(de.contains("pc, npc, place, faction, item, lore"));
    }

    #[test]
    fn drops_bad_kinds_and_dedupes() {
        let raw = r#"{"entries":[
            {"name":"X","kind":"weapon","body":""},
            {"name":"Sauron","kind":"npc","body":"a"},
            {"name":"sauron","kind":"npc","body":"dup"}
        ]}"#;
        let e = parse_entries(raw);
        assert_eq!(e.len(), 1, "bad kind dropped, duplicate name+kind merged");
        assert_eq!(e[0].name, "Sauron");
    }
}
