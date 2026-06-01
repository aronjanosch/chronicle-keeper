use serde_json::{json, Map, Value};

use crate::error::{AppError, AppResult};
use crate::llm;
use crate::models::{RecapRequest, RecapResponse, SummarizeRequest, SummarizeResponse};
use crate::prompts::{build_metadata_prompt, build_recap_prompt, build_summary_prompt};
use crate::state::AppState;
use crate::store::{artifacts, campaigns, codex, sessions, tags};

pub async fn summarize_session(
    state: &AppState,
    req: &SummarizeRequest,
) -> AppResult<SummarizeResponse> {
    let prep = state.with_db(|conn| -> AppResult<_> {
        let session = sessions::get_session_object(conn, &req.session_id)?;
        let cfg = crate::config::get_config_map(conn)?;

        let transcript_text = match req.transcript_id {
            Some(id) => artifacts::get_content(conn, id)?,
            None => artifacts::latest_content(conn, &req.session_id, "transcript")?,
        }
        .ok_or_else(|| AppError::BadRequest("Transcript not found for session.".into()))?;

        let resolved = llm::resolve(
            conn,
            &cfg,
            req.provider.as_deref(),
            req.model.as_deref(),
            req.base_url.as_deref(),
        )?;

        let language = cfg
            .get("default_language")
            .cloned()
            .unwrap_or_else(|| "en".into());
        // Campaign codex glossary: Phase 1 freeform paste + Phase 2 structured entries.
        // Both pass into the prompt verbatim so the LLM can recognize and correctly
        // spell NPCs/places/items the ASR mangled.
        let campaign_id = session
            .get("campaign")
            .and_then(|c| c.get("campaign_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let campaign = campaign_id
            .as_deref()
            .and_then(|cid| campaigns::get_campaign(conn, cid).ok().flatten());
        let codex_text = campaign
            .as_ref()
            .map(campaigns::codex_freeform_text)
            .unwrap_or_default();
        let gm = campaign.as_ref().map(|c| c.gm.clone()).unwrap_or_default();
        let codex_entries = campaign_id
            .as_deref()
            .map(|cid| codex::list_entries(conn, cid))
            .transpose()?
            .unwrap_or_default();
        // Campaign tag vocabulary so metadata extraction reuses canonical tags
        // instead of inventing a fresh (differently-cased / English) set.
        let known_tags = campaign_id
            .as_deref()
            .map(|cid| tags::distinct_tags(conn, cid))
            .transpose()?
            .unwrap_or_default();
        Ok((
            session,
            transcript_text,
            language,
            codex_text,
            codex_entries,
            gm,
            known_tags,
            resolved,
        ))
    })?;
    let (session, transcript_text, language, codex_text, codex_entries, gm, known_tags, resolved) =
        prep;

    let session_context = build_context(
        &session,
        req.title.as_deref(),
        &codex_text,
        &codex_entries,
        &gm,
    );
    let summary_prompt = build_summary_prompt(
        &transcript_text,
        req.title.as_deref(),
        req.context.as_deref(),
        &language,
        req.system_prompt.as_deref(),
        Some(&session_context),
    );

    let summary_text = llm::chat(
        resolved.transport,
        &resolved.api_base,
        &resolved.api_key,
        &resolved.model,
        &summary_prompt,
        resolved.timeout,
        false,
    )
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Cloud LLM request failed: {}", e.0)))?;
    if summary_text.is_empty() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "LLM returned an empty summary."
        )));
    }

    // Metadata is best-effort: a failure here must not sink the summary the user
    // already paid for. But don't swallow it silently — a quiet empty string
    // looks identical to "no metadata found" and hides a broken auto-fill.
    let metadata_text = match llm::chat(
        resolved.transport,
        &resolved.api_base,
        &resolved.api_key,
        &resolved.model,
        &build_metadata_prompt(&summary_text, &language, &known_tags),
        resolved.timeout,
        true,
    )
    .await
    {
        Ok(text) => text,
        Err(e) => {
            tracing::warn!("metadata extraction failed, skipping auto-fill: {}", e.0);
            String::new()
        }
    };
    let metadata = parse_metadata(&metadata_text);

    let session_id = req.session_id.clone();
    let provider = resolved.provider.clone();
    let model = resolved.model.clone();
    let metadata_for_merge = metadata.clone();
    let summary_for_db = summary_text.clone();
    state.with_db(|conn| -> AppResult<()> {
        artifacts::insert_artifact(
            conn,
            &session_id,
            "summary",
            &provider,
            &model,
            &summary_for_db,
        )?;
        if let Some(md) = &metadata_for_merge {
            merge_metadata(conn, &session_id, md)?;
        }
        Ok(())
    })?;

    Ok(SummarizeResponse {
        summary: summary_text,
        provider: resolved.provider,
        model: resolved.model,
        summary_path: None,
        metadata,
    })
}

/// Generate the campaign "story so far" recap: roll up every session's latest
/// summary (chronological) into one narrative. Operates on summaries, not
/// transcripts — cheaper, already-clean text, and avoids re-reading raw audio
/// output. Stores the result on the campaign and returns it.
pub async fn generate_recap(
    state: &AppState,
    campaign_id: &str,
    req: &RecapRequest,
) -> AppResult<RecapResponse> {
    let prep = state.with_db(|conn| -> AppResult<_> {
        let campaign = campaigns::get_campaign(conn, campaign_id)?
            .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
        let cfg = crate::config::get_config_map(conn)?;

        // Sessions come back newest-first; recap reads oldest-first.
        let mut list = sessions::list_campaign_sessions(conn, campaign_id)?;
        list.reverse();

        let mut blocks: Vec<String> = Vec::new();
        for s in &list {
            if !s.has_summary {
                continue;
            }
            let Some(text) = artifacts::latest_content(conn, &s.session_id, "summary")? else {
                continue;
            };
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let num = s.session_number.unwrap_or(0);
            let title = s.title.as_deref().unwrap_or("").trim();
            let header = if title.is_empty() {
                format!("### Session {num}")
            } else {
                format!("### Session {num} — {title}")
            };
            blocks.push(format!("{header}\n{text}"));
        }

        let resolved = llm::resolve(
            conn,
            &cfg,
            req.provider.as_deref(),
            req.model.as_deref(),
            req.base_url.as_deref(),
        )?;
        Ok((campaign, blocks, resolved))
    })?;
    let (campaign, blocks, resolved) = prep;

    if blocks.is_empty() {
        return Err(AppError::BadRequest(
            "No session summaries yet — summarize at least one session before building a recap."
                .into(),
        ));
    }
    let sessions_used = blocks.len();
    let sessions_block = blocks.join("\n\n");
    let prompt = build_recap_prompt(&campaign.name, &sessions_block, &campaign.default_language);

    let recap_text = llm::chat(
        resolved.transport,
        &resolved.api_base,
        &resolved.api_key,
        &resolved.model,
        &prompt,
        resolved.timeout,
        false,
    )
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Recap LLM request failed: {}", e.0)))?;
    let recap_text = recap_text.trim().to_string();
    if recap_text.is_empty() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "LLM returned an empty recap."
        )));
    }

    let cid = campaign_id.to_string();
    let recap_for_db = recap_text.clone();
    let recap_updated_at =
        state.with_db(move |conn| campaigns::set_recap(conn, &cid, &recap_for_db))?;

    Ok(RecapResponse {
        recap: recap_text,
        recap_updated_at,
        provider: resolved.provider,
        model: resolved.model,
        sessions_used,
    })
}

/// Build the prompt's session-context object from the session + campaign.
fn build_context(
    session: &Value,
    title_override: Option<&str>,
    codex: &str,
    codex_entries: &[crate::models::CodexEntry],
    gm: &str,
) -> Value {
    let campaign = session
        .get("campaign")
        .cloned()
        .unwrap_or_else(|| json!({}));
    json!({
        "campaign_name": campaign.get("campaign_name"),
        "session_number": campaign.get("session_number"),
        "title": title_override.map(Value::from).or_else(|| campaign.get("title").cloned()),
        "date": campaign.get("date"),
        "gm": gm,
        "speakers": session.get("speakers").cloned().unwrap_or_else(|| json!([])),
        "codex": codex,
        "codex_entries": codex_entries.iter().map(|e| json!({
            "name": e.name,
            "kind": e.kind,
            "body": e.body,
        })).collect::<Vec<_>>(),
    })
}

/// Tolerant JSON parse: strip ``` fences then parse.
fn parse_metadata(text: &str) -> Option<Value> {
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    serde_json::from_str(t).ok()
}

/// Merge LLM-extracted lists into the session's metadata without overwriting
/// existing (user-edited) values, then materialize the names/locations/items as
/// auto-extracted codex entries on the parent campaign.
fn merge_metadata(
    conn: &rusqlite::Connection,
    session_id: &str,
    metadata: &Value,
) -> AppResult<()> {
    use rusqlite::{params, OptionalExtension};
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT metadata_json, campaign_id FROM sessions WHERE session_id = ?1",
            params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((existing_json, campaign_id)) = row else {
        return Ok(());
    };
    let mut existing: Map<String, Value> = serde_json::from_str(&existing_json).unwrap_or_default();

    // Tags fold through the campaign vocabulary so case/language variants collapse
    // onto the established spelling (a freshly extracted `combat` becomes `Kampf`).
    let tag_vocab = campaign_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|cid| tags::vocab(conn, cid))
        .transpose()?
        .unwrap_or_default();

    if let Value::Object(new_map) = metadata {
        for (key, values) in new_map {
            let Some(new_list) = values.as_array() else {
                continue;
            };
            let entry = existing.entry(key.clone()).or_insert_with(|| json!([]));
            if key == "tags" {
                let existing_tags = entry.as_array().cloned().unwrap_or_default();
                *entry = json!(tags::merge_into(&existing_tags, new_list, &tag_vocab));
                continue;
            }
            let mut merged: Vec<Value> = entry.as_array().cloned().unwrap_or_default();
            for v in new_list {
                if !v.is_null() && !merged.contains(v) {
                    merged.push(v.clone());
                }
            }
            *entry = Value::Array(merged);
        }
    }
    conn.execute(
        "UPDATE sessions SET metadata_json = ?1 WHERE session_id = ?2",
        params![Value::Object(existing).to_string(), session_id],
    )?;

    // Promote extracted names into the campaign codex (auto-extract; never
    // overwrites a row the user already touched — `upsert_auto` is a no-op when
    // the natural key exists).
    if let Some(cid) = campaign_id.as_deref().filter(|s| !s.is_empty()) {
        if let Value::Object(map) = metadata {
            for (key, kind) in [
                ("characters", "npc"),
                ("locations", "place"),
                ("items", "item"),
            ] {
                let Some(list) = map.get(key).and_then(Value::as_array) else {
                    continue;
                };
                for v in list {
                    if let Some(name) = extract_name(v) {
                        let _ = codex::upsert_auto(conn, cid, &name, kind);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Metadata list items are usually strings, but a model may emit
/// `{name, description}` objects. Accept both; trim whitespace.
fn extract_name(v: &Value) -> Option<String> {
    let raw = match v {
        Value::String(s) => s.as_str(),
        Value::Object(map) => map
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| map.get("character_name").and_then(Value::as_str))?,
        _ => return None,
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::campaigns;

    #[test]
    fn merge_metadata_extracts_codex_entries() {
        let conn = crate::db::open_in_memory().unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp", 1).unwrap();
        conn.execute(
            "INSERT INTO sessions (session_id, campaign_id) VALUES ('s1', 'c1')",
            [],
        )
        .unwrap();
        let metadata = json!({
            "characters": ["Aragorn", { "name": "Gandalf" }],
            "locations": ["Bree"],
            "items": ["Andúril"],
            "events": ["Battle"],   // ignored: not a codex kind
            "tags": ["combat"],     // ignored
        });
        merge_metadata(&conn, "s1", &metadata).unwrap();
        let entries = codex::list_entries(&conn, "c1").unwrap();
        let names: std::collections::BTreeSet<String> =
            entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains("Aragorn"));
        assert!(names.contains("Gandalf"));
        assert!(names.contains("Bree"));
        assert!(names.contains("Andúril"));
        assert_eq!(entries.iter().filter(|e| e.kind == "npc").count(), 2);
        assert_eq!(entries.iter().filter(|e| e.kind == "place").count(), 1);
        assert_eq!(entries.iter().filter(|e| e.kind == "item").count(), 1);
        assert!(entries.iter().all(|e| e.source == "auto"));
    }

    #[test]
    fn merge_metadata_skips_user_edited_entry() {
        let conn = crate::db::open_in_memory().unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp", 1).unwrap();
        conn.execute(
            "INSERT INTO sessions (session_id, campaign_id) VALUES ('s1', 'c1')",
            [],
        )
        .unwrap();
        // Pre-existing user entry with a corrected spelling and a body.
        codex::create_entry(
            &conn,
            "c1",
            &crate::models::CodexEntryCreate {
                name: "Aragorn".into(),
                kind: "npc".into(),
                body: "Heir of Isildur".into(),
                detail: String::new(),
            },
        )
        .unwrap();
        // LLM emits a different casing — must be treated as the same row, not overwritten.
        merge_metadata(&conn, "s1", &json!({ "characters": ["aragorn"] })).unwrap();
        let entries = codex::list_entries(&conn, "c1").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Aragorn");
        assert_eq!(entries[0].body, "Heir of Isildur");
        assert_eq!(entries[0].source, "manual");
    }
}
