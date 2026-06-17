//! Keeper agent endpoints: chat CRUD, the SSE message stream, permission
//! approve, undo, abort. SSE frames (one JSON object per `data:` line):
//!   {type:"text_delta", text}
//!   {type:"tool_start", name, args_summary, diff?}
//!   {type:"tool_result", name, summary, is_error}
//!   {type:"permission_request", request_id, name, args, diff}
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

use crate::agent::{
    self, attachments, brief, chats, checkpoints, memory, AskRequest, Decision, PermissionGate,
    RealLlm, TurnEvent,
};
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
    Ok(Json(json!({
        "events": chats::load_chat(&root, &chat_id)?,
        "undoable": checkpoints::count(&root, &chat_id),
    })))
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
    let aborted = {
        let runs = state.agent_runs.lock().unwrap_or_else(|e| e.into_inner());
        match runs.get(&campaign_id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    };
    // A run parked on a permission ask only sees the flag once the ask
    // resolves — dropping the sender resolves it as a deny.
    let mut asks = state.agent_asks.lock().unwrap_or_else(|e| e.into_inner());
    asks.retain(|_, (cid, _)| cid != &campaign_id);
    Ok(Json(json!({ "aborted": aborted })))
}

#[derive(Deserialize)]
pub struct ApproveRequest {
    pub request_id: String,
    pub decision: String,
}

pub async fn approve(
    State(state): State<AppState>,
    Path((_campaign_id, _chat_id)): Path<(String, String)>,
    Json(req): Json<ApproveRequest>,
) -> AppResult<Json<Value>> {
    let decision = match req.decision.as_str() {
        "allow_once" => Decision::AllowOnce,
        "allow_chat" => Decision::AllowChat,
        "deny" => Decision::Deny,
        other => return Err(AppError::BadRequest(format!("Unknown decision: {other}"))),
    };
    let sender = {
        let mut asks = state.agent_asks.lock().unwrap_or_else(|e| e.into_inner());
        asks.remove(&req.request_id).map(|(_, tx)| tx)
    };
    let Some(sender) = sender else {
        return Err(AppError::NotFound("No pending permission request.".into()));
    };
    let _ = sender.send(decision); // run gone = nothing to resolve
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct UndoRequest {
    pub scope: Option<String>,
}

pub async fn undo(
    State(state): State<AppState>,
    Path((campaign_id, chat_id)): Path<(String, String)>,
    Json(req): Json<UndoRequest>,
) -> AppResult<Json<Value>> {
    {
        let runs = state.agent_runs.lock().unwrap_or_else(|e| e.into_inner());
        if runs.contains_key(&campaign_id) {
            return Err(AppError::Conflict(
                "The Keeper is working — stop it before undoing.".into(),
            ));
        }
    }
    let (root, cfg) = world_cfg(&state, &campaign_id)?;
    chats::load_chat(&root, &chat_id)?; // 404 on unknown chat
    let vault_root = cfg.codex_dir(&root);
    let all = req.scope.as_deref() == Some("all");
    let restored = checkpoints::undo(&root, &chat_id, &vault_root, all)?;
    for rel in &restored {
        state.note_vault_write(&vault_root, rel);
        let _ = state.with_index(&vault_root, |conn| {
            if vault_root.join(rel).is_file() {
                let _ = crate::store::index::upsert_path(conn, &vault_root, rel);
            } else {
                let _ = crate::store::index::remove_path(conn, rel);
            }
        });
    }
    Ok(Json(json!({
        "restored": restored,
        "remaining": checkpoints::count(&root, &chat_id),
    })))
}

pub async fn list_attachments(
    State(state): State<AppState>,
    Path((campaign_id, chat_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    chats::load_chat(&root, &chat_id)?;
    Ok(Json(attachments::list_json(&root, &chat_id)))
}

/// Add an attachment. A body carrying `content` is a dropped file copied into
/// the world; otherwise it is a live vault reference (`kind` = page/session/
/// transcript).
pub async fn add_attachment(
    State(state): State<AppState>,
    Path((campaign_id, chat_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    chats::load_chat(&root, &chat_id)?;
    let att = if let Some(content) = body.get("content").and_then(Value::as_str) {
        let name = body
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("attachment.txt");
        attachments::add_file(&root, &chat_id, name, content)?
    } else {
        attachments::add_ref(&root, &chat_id, &body)?
    };
    Ok(Json(serde_json::to_value(att).unwrap_or_default()))
}

pub async fn delete_attachment(
    State(state): State<AppState>,
    Path((campaign_id, chat_id, att_id)): Path<(String, String, String)>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    attachments::remove(&root, &chat_id, &att_id)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn list_memory(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    Ok(Json(memory::list_json(&root)))
}

pub async fn delete_memory(
    State(state): State<AppState>,
    Path((campaign_id, name)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    memory::delete_memory(&root, &name).map_err(AppError::BadRequest)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn list_skills(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let root = agent::skills::skills_root(&state);
    Ok(Json(json!({
        "path": root.to_string_lossy(),
        "skills": agent::skills::list_json(&root),
    })))
}

pub async fn get_brief(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (root, cfg) = world_cfg(&state, &campaign_id)?;
    Ok(Json(brief::status(&root, &cfg)))
}

#[derive(Deserialize)]
pub struct BriefRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

pub async fn run_brief(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<BriefRequest>,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let (root, cfg) = world_cfg(&state, &campaign_id)?;
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
        let result = brief::run_brief(&st, &root, &cfg, &llm, &cancel, |e| match e {
            TurnEvent::TextDelta(t) => send(json!({ "type": "text_delta", "text": t })),
            TurnEvent::ToolStart { name, args_summary, diff } => {
                send(json!({ "type": "tool_start", "name": name, "args_summary": args_summary, "diff": diff }))
            }
            TurnEvent::ToolResult { name, summary, is_error } => {
                send(json!({ "type": "tool_result", "name": name, "summary": summary, "is_error": is_error }))
            }
            TurnEvent::Notice(message) => send(json!({ "type": "notice", "message": message })),
        })
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

#[derive(Deserialize)]
pub struct MessageRequest {
    pub text: String,
    pub mode: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    #[serde(default)]
    pub images: Vec<crate::llm::agent::Image>,
    /// Editor state this turn — focused page + open tabs. Ephemeral context.
    #[serde(default)]
    pub focus: Option<agent::attachments::Focus>,
}

/// Production gate: emit a `permission_request` SSE frame, park on a oneshot
/// until `/approve` resolves it (or abort drains it → deny).
struct SseGate {
    state: AppState,
    campaign_id: String,
    tx: tokio::sync::mpsc::UnboundedSender<Event>,
}

impl PermissionGate for SseGate {
    async fn ask(&self, req: AskRequest) -> Decision {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut asks = self
                .state
                .agent_asks
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            asks.insert(req.id.clone(), (self.campaign_id.clone(), tx));
        }
        let frame = json!({
            "type": "permission_request",
            "request_id": req.id,
            "name": req.name,
            "args": req.args,
            "diff": req.diff,
        });
        let ev = Event::default()
            .json_data(&frame)
            .unwrap_or_else(|_| Event::default());
        let _ = self.tx.send(ev);
        rx.await.unwrap_or(Decision::Deny)
    }
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
    if req.text.trim().is_empty() && req.images.is_empty() {
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
        let gate = SseGate {
            state: st.clone(),
            campaign_id: campaign_id.clone(),
            tx: tx.clone(),
        };
        let turn_ctx = agent::TurnCtx {
            state: &st,
            world_root: &root,
            cfg: &cfg,
            chat_id: &chat_id,
            mode: agent::Mode::parse(req.mode.as_deref()),
            focus: req.focus.as_ref(),
        };
        let result = agent::run_turn(
            &turn_ctx,
            &req.text,
            &req.images,
            &llm,
            &gate,
            &cancel,
            |e| match e {
                TurnEvent::TextDelta(t) => send(json!({ "type": "text_delta", "text": t })),
                TurnEvent::ToolStart { name, args_summary, diff } => {
                    send(json!({ "type": "tool_start", "name": name, "args_summary": args_summary, "diff": diff }))
                }
                TurnEvent::ToolResult { name, summary, is_error } => send(json!({
                    "type": "tool_result", "name": name, "summary": summary, "is_error": is_error
                })),
                TurnEvent::Notice(message) => send(json!({ "type": "notice", "message": message })),
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
