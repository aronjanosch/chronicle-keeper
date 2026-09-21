//! Update the Codex (Phase 5): generate / review / commit AI page proposals.

use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::Stream;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::codex_update::{self, DecisionPatch, UpdateProgress, UpdateRequest};
use crate::error::AppResult;
use crate::state::AppState;

/// Streaming generation over SSE. Frames:
///   {stage:"candidates"}        stage-1 candidate pass running
///   {stage:"grounding"}         stage-2 transcript verification running
///   {stage:"done", run}         persisted run (also readable via GET)
///   {stage:"error", message}
pub async fn generate(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<UpdateRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    tokio::spawn(async move {
        let send = |val: Value| {
            let ev = Event::default()
                .json_data(&val)
                .unwrap_or_else(|_| Event::default());
            let _ = tx.send(ev);
        };
        let result = codex_update::generate_streamed(&state, &session_id, &req, |p| match p {
            UpdateProgress::Candidates => send(json!({ "stage": "candidates" })),
            UpdateProgress::Grounding => send(json!({ "stage": "grounding" })),
        })
        .await;
        match result {
            Ok(run) => send(json!({ "stage": "done", "run": run })),
            Err(e) => send(json!({ "stage": "error", "message": e.to_string() })),
        }
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|ev| (Ok(ev), rx))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Current run + decisions; `{"status":"none"}` when never generated.
pub async fn get(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (dir, _) = codex_update::session_paths(&state, &session_id)?;
    match codex_update::read_run(&dir)? {
        Some(run) => Ok(Json(serde_json::to_value(run).unwrap_or(Value::Null))),
        None => Ok(Json(json!({ "status": "none" }))),
    }
}

/// Save review state: decisions, edited changes, skip.
pub async fn put(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(patch): Json<DecisionPatch>,
) -> AppResult<Json<Value>> {
    let (dir, _) = codex_update::session_paths(&state, &session_id)?;
    let run = codex_update::apply_decisions(&dir, &patch)?;
    Ok(Json(serde_json::to_value(run).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
pub struct CommitRequest {
    pub ids: Vec<String>,
}

pub async fn commit(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<CommitRequest>,
) -> AppResult<Json<Value>> {
    let (dir, vault_root) = codex_update::session_paths(&state, &session_id)?;
    // Once a new review owns this session, the legacy route must not write the
    // same pages a second time (contract §8).
    if crate::session_review::legacy_commit_blocked(&dir)? {
        return Err(crate::error::AppError::Conflict(
            "This session uses the current review. Apply updates from Review.".into(),
        ));
    }
    let report = codex_update::commit(&dir, &vault_root, &req.ids)?;
    // Index is a rebuildable cache — refresh touched pages best-effort.
    for rel in &report.files {
        state.note_vault_write(&vault_root, rel);
        let _ = state.with_index(&vault_root, |conn| {
            let _ = crate::store::index::upsert_path(conn, &vault_root, rel);
        });
    }
    Ok(Json(serde_json::to_value(report).unwrap_or(Value::Null)))
}
