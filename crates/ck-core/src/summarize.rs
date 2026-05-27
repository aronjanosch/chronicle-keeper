use std::path::PathBuf;

use chrono::Local;
use serde_json::{json, Map, Value};

use crate::error::{AppError, AppResult};
use crate::llm::{self, Transport};
use crate::models::{SummarizeRequest, SummarizeResponse};
use crate::normalize::sanitize_folder_name;
use crate::prompts::{build_metadata_prompt, build_summary_prompt};
use crate::state::AppState;
use crate::store::{artifacts, sessions};

struct Resolved {
    provider: String,
    transport: Transport,
    api_base: String,
    api_key: String,
    model: String,
    timeout: u64,
    needs_key: bool,
}

pub async fn summarize_session(state: &AppState, req: &SummarizeRequest) -> AppResult<SummarizeResponse> {
    // --- gather everything from the DB up front (sync) ---
    let prep = state.with_db(|conn| -> AppResult<_> {
        let session = sessions::get_session_object(conn, &req.session_id)?;
        let cfg = crate::config::get_config_map(conn)?;

        let transcript_path = match &req.transcript_path {
            Some(p) if !p.is_empty() => Some(p.clone()),
            _ => artifacts::latest_path(conn, &req.session_id, "transcript")?,
        };
        let transcript_path = transcript_path
            .ok_or_else(|| AppError::BadRequest("Transcript not found for session.".into()))?;

        let provider = req
            .provider
            .clone()
            .unwrap_or_else(|| cfg.get("summary_provider").cloned().unwrap_or_else(|| "ollama".into()))
            .to_lowercase();
        let p = llm::get(&provider)
            .ok_or_else(|| AppError::BadRequest(format!("Unknown provider: {provider}")))?;
        let saved = llm::get_key(conn, &provider)?.unwrap_or_default();

        let api_key = saved.api_key.clone();
        if p.needs_key && api_key.is_empty() {
            return Err(AppError::Internal(anyhow::anyhow!(
                "No API key saved for {}. Add it in Settings → LLM providers.",
                p.name
            )));
        }

        let api_base = req
            .base_url
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| Some(saved.api_base.clone()).filter(|s| !s.is_empty()))
            .or_else(|| {
                if p.transport == Transport::Ollama {
                    cfg.get("ollama_base_url").cloned()
                } else {
                    None
                }
            })
            .or_else(|| p.default_api_base.map(str::to_string))
            .unwrap_or_default();

        let model = req
            .model
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| Some(saved.default_model.clone()).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| p.default_model.to_string());

        let timeout: u64 = if p.transport == Transport::Ollama {
            cfg.get("ollama_timeout_seconds").and_then(|s| s.parse().ok()).unwrap_or(120)
        } else {
            cfg.get("litellm_timeout_seconds").and_then(|s| s.parse().ok()).unwrap_or(120)
        };

        let language = cfg.get("default_language").cloned().unwrap_or_else(|| "en".into());
        Ok((session, transcript_path, language, Resolved {
            provider,
            transport: p.transport,
            api_base,
            api_key,
            model,
            timeout,
            needs_key: p.needs_key,
        }))
    })?;
    let (session, transcript_path, language, resolved) = prep;
    let _ = resolved.needs_key;

    let transcript_text = std::fs::read_to_string(&transcript_path)
        .map_err(|_| AppError::BadRequest("Transcript not found for session.".into()))?;

    let session_context = build_context(&session, req.title.as_deref());
    let summary_prompt = build_summary_prompt(
        &transcript_text,
        req.title.as_deref(),
        req.context.as_deref(),
        &language,
        req.system_prompt.as_deref(),
        Some(&session_context),
    );

    // --- LLM calls (async) ---
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
        return Err(AppError::Internal(anyhow::anyhow!("LLM returned an empty summary.")));
    }

    let metadata_text = llm::chat(
        resolved.transport,
        &resolved.api_base,
        &resolved.api_key,
        &resolved.model,
        &build_metadata_prompt(&summary_text, &language),
        resolved.timeout,
        true,
    )
    .await
    .unwrap_or_default();
    let metadata = parse_metadata(&metadata_text);

    // --- persist ---
    let session_path = session.get("session_path").and_then(Value::as_str).unwrap_or("").to_string();
    let summary_path = match &req.output_path {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
            let dir = PathBuf::from(&session_path).join("summaries").join(format!(
                "{}_{}_{}",
                resolved.provider,
                sanitize_folder_name(&resolved.model),
                ts
            ));
            std::fs::create_dir_all(&dir).map_err(|e| AppError::Internal(e.into()))?;
            dir.join("summary.md")
        }
    };
    std::fs::write(&summary_path, &summary_text).map_err(|e| AppError::Internal(e.into()))?;
    let summary_path_str = summary_path.to_string_lossy().into_owned();

    let session_id = req.session_id.clone();
    let provider = resolved.provider.clone();
    let model = resolved.model.clone();
    let metadata_for_merge = metadata.clone();
    state.with_db(|conn| -> AppResult<()> {
        artifacts::insert_artifact(conn, &session_id, "summary", &provider, &model, &summary_path_str)?;
        if let Some(md) = &metadata_for_merge {
            merge_metadata(conn, &session_id, md)?;
        }
        Ok(())
    })?;

    Ok(SummarizeResponse {
        summary: summary_text,
        provider: resolved.provider,
        model: resolved.model,
        summary_path: Some(summary_path_str),
        metadata,
    })
}

/// Build the prompt's session-context object from the session + campaign.
fn build_context(session: &Value, title_override: Option<&str>) -> Value {
    let campaign = session.get("campaign").cloned().unwrap_or_else(|| json!({}));
    json!({
        "campaign_name": campaign.get("campaign_name"),
        "session_number": campaign.get("session_number"),
        "title": title_override.map(Value::from).or_else(|| campaign.get("title").cloned()),
        "date": campaign.get("date"),
        "speakers": session.get("speakers").cloned().unwrap_or_else(|| json!([])),
    })
}

/// Tolerant JSON parse: strip ``` fences then parse.
fn parse_metadata(text: &str) -> Option<Value> {
    let t = text.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    serde_json::from_str(t).ok()
}

/// Merge LLM-extracted lists into the session's metadata without overwriting
/// existing (user-edited) values.
fn merge_metadata(conn: &rusqlite::Connection, session_id: &str, metadata: &Value) -> AppResult<()> {
    use rusqlite::{params, OptionalExtension};
    let existing_json: Option<String> = conn
        .query_row("SELECT metadata_json FROM sessions WHERE session_id = ?1", params![session_id], |r| r.get(0))
        .optional()?;
    let Some(existing_json) = existing_json else { return Ok(()) };
    let mut existing: Map<String, Value> =
        serde_json::from_str(&existing_json).unwrap_or_default();

    if let Value::Object(new_map) = metadata {
        for (key, values) in new_map {
            let Some(new_list) = values.as_array() else { continue };
            let entry = existing.entry(key.clone()).or_insert_with(|| json!([]));
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
    Ok(())
}
