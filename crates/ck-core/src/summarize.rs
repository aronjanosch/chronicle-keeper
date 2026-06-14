use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::llm;
use crate::models::{RecapRequest, RecapResponse, SummarizeRequest, SummarizeResponse};
use crate::prompts::{build_metadata_prompt, build_recap_prompt, build_summary_prompt};
use crate::state::AppState;
use crate::store::{artifacts, campaigns, sessions, tags};

/// Progress signal for a streaming summarize run. The prose summary streams
/// token by token (`Token`); the JSON metadata pass never streams (partial JSON
/// is garbage), so it's a single `Metadata` stage flip before the blocking call.
pub enum SummaryProgress {
    /// Prep done, prompt sent, waiting on the model's first token. Covers the
    /// long prefill dead zone on big local prompts.
    Reading,
    /// One chunk of the summary text as it arrives.
    Token(String),
    /// Summary finished; the metadata/tag extraction call is now running.
    Metadata,
}

/// Blocking summarize: same contract as before, no progress events.
pub async fn summarize_session(
    state: &AppState,
    req: &SummarizeRequest,
) -> AppResult<SummarizeResponse> {
    run_summarize(state, req, &mut |_| {}).await
}

/// Streaming summarize: identical result + persistence to the blocking path, but
/// emits `SummaryProgress` as the prose streams in. The authoritative summary +
/// parsed metadata are still computed server-side from the accumulated text and
/// returned in the response — the streamed tokens are display-only.
pub async fn summarize_session_streamed<F: FnMut(SummaryProgress) + Send>(
    state: &AppState,
    req: &SummarizeRequest,
    mut emit: F,
) -> AppResult<SummarizeResponse> {
    run_summarize(state, req, &mut emit).await
}

async fn run_summarize(
    state: &AppState,
    req: &SummarizeRequest,
    emit: &mut (dyn FnMut(SummaryProgress) + Send),
) -> AppResult<SummarizeResponse> {
    let prep = state.with_db(|conn| -> AppResult<_> {
        let session = sessions::get_session_object(conn, &req.session_id)?;
        let cfg = crate::config::get_config_map(conn)?;

        let transcript_text = match req.transcript_id {
            Some(id) => artifacts::get_content(conn, &req.session_id, id)?,
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
        // Campaign codex glossary, passed into the prompt verbatim so the LLM can
        // recognize and correctly spell NPCs/places/items the ASR mangled.
        let campaign_id = session
            .get("campaign")
            .and_then(|c| c.get("campaign_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let campaign = campaign_id
            .as_deref()
            .and_then(|cid| campaigns::get_campaign(conn, cid).ok().flatten());
        // Codex freeform notes retired (Phase 2) — page `summary:` frontmatter
        // is the context source; richer injection lands with Phase 4.
        let codex_text = String::new();
        let gm = campaign.as_ref().map(|c| c.gm.clone()).unwrap_or_default();
        // Files-as-truth: the glossary one-liners come from vault page `summary:`
        // frontmatter (every world has a vault by construction).
        let codex_entries: Vec<crate::models::CodexEntry> = campaign
            .as_ref()
            .and_then(|c| c.vault_path.as_deref())
            .map(|vp| {
                crate::vault::list_pages(std::path::Path::new(vp))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| crate::models::CodexEntry {
                        entry_id: String::new(),
                        campaign_id: campaign_id.clone().unwrap_or_default(),
                        name: p.title,
                        kind: p.kind.unwrap_or_else(|| "lore".into()),
                        body: p.summary,
                        detail: String::new(),
                        source: String::new(),
                        updated_at: String::new(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Campaign tag vocabulary so metadata extraction reuses canonical tags
        // instead of inventing a fresh (differently-cased / English) set.
        let known_tags = campaign_id
            .as_deref()
            .map(|cid| tags::distinct_tags(conn, cid))
            .transpose()?
            .unwrap_or_default();
        let world_ctx = campaign_id
            .as_deref()
            .map(|cid| crate::agent::context::world_context_for_campaign(conn, cid))
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
            world_ctx,
        ))
    })?;
    let (
        session,
        transcript_text,
        language,
        codex_text,
        codex_entries,
        gm,
        known_tags,
        resolved,
        world_ctx,
    ) = prep;

    let session_context = build_context(
        &session,
        req.title.as_deref(),
        &codex_text,
        &codex_entries,
        &gm,
    );
    let summary_prompt = crate::agent::context::apply_world_context(
        &build_summary_prompt(
            &transcript_text,
            req.title.as_deref(),
            req.context.as_deref(),
            &language,
            req.system_prompt.as_deref(),
            Some(&session_context),
        ),
        &world_ctx,
    );

    // Prep is done and the prompt is built; on a big local prompt the model now
    // spends a long stretch in prefill before the first token. Flip to "reading"
    // so the user sees motion instead of a frozen pane.
    emit(SummaryProgress::Reading);
    let summary_text = llm::chat_stream(
        &llm::ChatRequest {
            transport: resolved.transport,
            api_base: &resolved.api_base,
            api_key: &resolved.api_key,
            model: &resolved.model,
            prompt: &summary_prompt,
            timeout_secs: resolved.timeout,
            num_ctx_max: resolved.num_ctx_max,
        },
        |tok| emit(SummaryProgress::Token(tok.to_string())),
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
    emit(SummaryProgress::Metadata);
    let metadata_text = match llm::chat(
        &llm::ChatRequest {
            transport: resolved.transport,
            api_base: &resolved.api_base,
            api_key: &resolved.api_key,
            model: &resolved.model,
            prompt: &build_metadata_prompt(&summary_text, &language, &known_tags),
            timeout_secs: resolved.timeout,
            num_ctx_max: resolved.num_ctx_max,
        },
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
            replace_metadata(conn, &session_id, md)?;
        }
        Ok(())
    })?;

    // Write summary.md for vault sessions (best-effort).
    let session_path = session
        .get("session_path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if crate::session_files::is_vault_session_path(session_path) {
        let campaign = session.get("campaign").cloned().unwrap_or_default();
        let number = campaign.get("session_number").and_then(Value::as_i64);
        let date = campaign.get("date").and_then(Value::as_str);
        let title = campaign.get("title").and_then(Value::as_str);
        let generated_at = crate::store::now();
        let _ = crate::session_files::write_summary_md(
            std::path::Path::new(session_path),
            &summary_text,
            number,
            date,
            title,
            &resolved.provider,
            &resolved.model,
            &generated_at,
        );
    }

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
        let world_ctx = crate::agent::context::world_context_for_campaign(conn, campaign_id);
        Ok((campaign, blocks, resolved, world_ctx))
    })?;
    let (campaign, blocks, resolved, world_ctx) = prep;

    if blocks.is_empty() {
        return Err(AppError::BadRequest(
            "No session summaries yet — summarize at least one session before building a recap."
                .into(),
        ));
    }
    let sessions_used = blocks.len();
    let sessions_block = blocks.join("\n\n");
    let prompt = crate::agent::context::apply_world_context(
        &build_recap_prompt(&campaign.name, &sessions_block, &campaign.default_language),
        &world_ctx,
    );

    let recap_text = llm::chat(
        &llm::ChatRequest {
            transport: resolved.transport,
            api_base: &resolved.api_base,
            api_key: &resolved.api_key,
            model: &resolved.model,
            prompt: &prompt,
            timeout_secs: resolved.timeout,
            num_ctx_max: resolved.num_ctx_max,
        },
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

/// Replace the session's metadata (session.toml) with the LLM-extracted lists.
///
/// Each re-summary is a fresh take on the same session, so its lists *replace*
/// the previous ones per category — they don't accumulate. Without this, trying
/// a second model just appends a reworded copy of every event, character, etc.,
/// and the lists grow without bound. Re-summary wins over manual edits; the
/// inline editor in the session view is the place to curate afterwards.
/// No auto-stub Codex pages (Phase 1.7-E) — the Codex is authored by the user
/// or Phase 5 AI proposals.
fn replace_metadata(
    conn: &rusqlite::Connection,
    session_id: &str,
    metadata: &Value,
) -> AppResult<()> {
    let Some(loc) = sessions::locate(conn, session_id)? else {
        return Ok(());
    };
    let mut st = loc.st;

    // Tags fold through the campaign vocabulary so case/language variants collapse
    // onto the established spelling (a freshly extracted `combat` becomes `Kampf`).
    let tag_vocab = loc
        .world
        .as_ref()
        .map(|(_, cfg)| tags::vocab(conn, &cfg.id))
        .transpose()?
        .unwrap_or_default();

    let names = |key: &str| {
        metadata
            .get(key)
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(extract_name).collect::<Vec<_>>())
    };
    if let Some(v) = names("characters") {
        st.metadata.characters = v;
    }
    if let Some(v) = names("locations") {
        st.metadata.locations = v;
    }
    if let Some(v) = names("events") {
        st.metadata.events = v;
    }
    if let Some(v) = names("items") {
        st.metadata.items = v;
    }
    if let Some(list) = metadata.get("tags").and_then(Value::as_array) {
        st.metadata.tags = tags::merge_into(&[], list, &tag_vocab);
    }
    crate::session_files::write_session_toml_file(&loc.dir, &st)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write session.toml: {e}")))?;
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

    fn world_with_session(tag: &str) -> (rusqlite::Connection, std::path::PathBuf, String) {
        let conn = crate::db::open_in_memory().unwrap();
        let tmp = std::env::temp_dir().join(format!("ck-sum-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).unwrap();
        crate::config::set_value(&conn, "output_root", &tmp.to_string_lossy()).unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp", 1, None, false, false).unwrap();
        let s = sessions::create_campaign_session(&conn, "c1", Some(1), None, None).unwrap();
        (conn, tmp, s.session_id)
    }

    #[test]
    fn replace_metadata_replaces_and_accepts_name_objects() {
        let (conn, tmp, sid) = world_with_session("names");
        let metadata = json!({
            "characters": ["Aragorn", { "name": "Gandalf" }],
            "locations": ["Bree"],
            "tags": ["combat"],
        });
        replace_metadata(&conn, &sid, &metadata).unwrap();
        let loc = sessions::locate(&conn, &sid).unwrap().unwrap();
        assert_eq!(loc.st.metadata.characters, vec!["Aragorn", "Gandalf"]);
        assert_eq!(loc.st.metadata.locations, vec!["Bree"]);
        assert_eq!(loc.st.metadata.tags, vec!["combat"]);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn replace_metadata_does_not_accumulate_across_resummaries() {
        let (conn, tmp, sid) = world_with_session("resummary");
        // First model's take.
        replace_metadata(
            &conn,
            &sid,
            &json!({ "events": ["The party crossed the bridge"], "tags": ["combat"] }),
        )
        .unwrap();
        // A second model rewords the same beat — must replace, not append.
        replace_metadata(
            &conn,
            &sid,
            &json!({ "events": ["Party fought their way over the bridge"], "tags": ["combat"] }),
        )
        .unwrap();

        let loc = sessions::locate(&conn, &sid).unwrap().unwrap();
        assert_eq!(
            loc.st.metadata.events,
            vec!["Party fought their way over the bridge"]
        );
        assert_eq!(loc.st.metadata.tags, vec!["combat"]);
        std::fs::remove_dir_all(&tmp).ok();
    }
}
