//! Keeper agent endpoints: chat CRUD, the SSE message stream, abort.
//! SSE frames (one JSON object per `data:` line):
//!   {type:"text_delta", text}
//!   {type:"tool_start", name, args_summary}
//!   {type:"tool_result", name, summary, is_error}
//!   {type:"turn_done"}
//!   {type:"error", message}

use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::Stream;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agent::{self, chats, RealLlm, TurnEvent};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

use super::vault::world_cfg;

pub async fn list_chats(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    Ok(Json(json!({ "chats": chats::list_chats(&root)? })))
}

pub async fn create_chat(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    let meta = chats::create_chat(&root)?;
    Ok(Json(serde_json::to_value(meta).unwrap_or_default()))
}

pub async fn get_chat(
    State(state): State<AppState>,
    Path((campaign_id, chat_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    Ok(Json(json!({ "events": chats::load_chat(&root, &chat_id)? })))
}

pub async fn delete_chat(
    State(state): State<AppState>,
    Path((campaign_id, chat_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    chats::delete_chat(&root, &chat_id)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn abort(
    State(state): State<AppState>,
    Path((campaign_id, _chat_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let runs = state.agent_runs.lock().unwrap_or_else(|e| e.into_inner());
    let aborted = match runs.get(&campaign_id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    };
    Ok(Json(json!({ "aborted": aborted })))
}

#[derive(Deserialize)]
pub struct MessageRequest {
    pub text: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

/// Claim the per-world run slot. Err(Conflict) while another run is active.
fn claim_run(state: &AppState, campaign_id: &str) -> AppResult<Arc<AtomicBool>> {
    let mut runs = state.agent_runs.lock().unwrap_or_else(|e| e.into_inner());
    if runs.contains_key(campaign_id) {
        return Err(AppError::Conflict(
            "The Keeper is already working on this world — wait or abort first.".into(),
        ));
    }
    let flag = Arc::new(AtomicBool::new(false));
    runs.insert(campaign_id.to_string(), flag.clone());
    Ok(flag)
}

fn release_run(state: &AppState, campaign_id: &str) {
    let mut runs = state.agent_runs.lock().unwrap_or_else(|e| e.into_inner());
    runs.remove(campaign_id);
}

pub async fn send_message(
    State(state): State<AppState>,
    Path((campaign_id, chat_id)): Path<(String, String)>,
    Json(req): Json<MessageRequest>,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    if req.text.trim().is_empty() {
        return Err(AppError::BadRequest("Empty message.".into()));
    }
    let (root, cfg) = world_cfg(&state, &campaign_id)?;
    chats::load_chat(&root, &chat_id)?; // 404 before the stream starts
    let resolved = state.with_db(|conn| {
        let app_cfg = crate::config::get_config_map(conn)?;
        crate::llm::resolve(
            conn,
            &app_cfg,
            req.provider.as_deref(),
            req.model.as_deref(),
            req.base_url.as_deref(),
        )
    })?;
    let cancel = claim_run(&state, &campaign_id)?;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let st = state.clone();
    tokio::spawn(async move {
        let send = |val: Value| {
            let ev = Event::default()
                .json_data(&val)
                .unwrap_or_else(|_| Event::default());
            let _ = tx.send(ev);
        };
        let llm = RealLlm { resolved };
        let turn_ctx = agent::TurnCtx {
            state: &st,
            world_root: &root,
            cfg: &cfg,
            chat_id: &chat_id,
        };
        let result = agent::run_turn(
            &turn_ctx,
            &req.text,
            &llm,
            &cancel,
            |e| match e {
                TurnEvent::TextDelta(t) => send(json!({ "type": "text_delta", "text": t })),
                TurnEvent::ToolStart { name, args_summary } => {
                    send(json!({ "type": "tool_start", "name": name, "args_summary": args_summary }))
                }
                TurnEvent::ToolResult { name, summary, is_error } => send(json!({
                    "type": "tool_result", "name": name, "summary": summary, "is_error": is_error
                })),
            },
        )
        .await;
        release_run(&st, &campaign_id);
        match result {
            Ok(()) => send(json!({ "type": "turn_done" })),
            Err(e) => send(json!({ "type": "error", "message": e.to_string() })),
        }
    });

    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|ev| (Ok(ev), rx))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
