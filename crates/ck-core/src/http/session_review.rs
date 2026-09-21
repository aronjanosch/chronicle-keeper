//! Session review HTTP surface (contract §7). Generation and clarification
//! (SC-05) arrive with their own SSE handlers; everything here is the durable
//! record and its safe application.

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::error::{AppError, AppResult};
use crate::session_review::{
    self as review, ApplyReport, ApplyRequest, FinishRequest, PutReviewRequest, RecoverRequest,
    ReopenRequest, ReviewRun,
};
use crate::state::AppState;
use crate::store::sessions;

/// Session directory, its world's vault root, and the world root.
fn paths(state: &AppState, session_id: &str) -> AppResult<(PathBuf, PathBuf, PathBuf)> {
    let sid = session_id.to_string();
    state.with_db(move |conn| {
        let loc = sessions::locate(conn, &sid)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {sid}")))?;
        let Some((root, cfg)) = loc.world else {
            return Err(AppError::Unprocessable(
                "Session review requires a world session".into(),
            ));
        };
        let vault_root = cfg.codex_dir(&root);
        Ok((loc.dir, vault_root, root))
    })
}

fn body<T>(payload: Result<Json<T>, JsonRejection>) -> AppResult<T> {
    payload
        .map(|Json(request)| request)
        .map_err(|error| AppError::Unprocessable(error.body_text()))
}

fn run_response(run: &ReviewRun, revision: &str, flags: Option<Value>) -> Value {
    json!({
        "status": "ok",
        "revision": revision,
        "run": run,
        "flags": flags,
    })
}

/// Written pages are canon now; the index is a rebuildable cache, so refresh
/// what this request touched and let a failure stay silent.
fn refresh_index(state: &AppState, vault_root: &std::path::Path, report: &ApplyReport) {
    for rel in report.groups.iter().flat_map(|g| g.written.iter()) {
        state.note_vault_write(vault_root, rel);
        let _ = state.with_index(vault_root, |conn| {
            let _ = crate::store::index::upsert_path(conn, vault_root, rel);
        });
    }
}

/// Re-read after a write so the caller gets the run and revision it must send
/// back with the next mutation.
fn reload(session_dir: &std::path::Path, report: ApplyReport) -> AppResult<Json<Value>> {
    let loaded = review::load(session_dir)?
        .ok_or_else(|| AppError::NotFound("No review for this session.".into()))?;
    Ok(Json(json!({
        "status": "ok",
        "revision": loaded.revision,
        "run": loaded.run,
        "report": report,
    })))
}

/// Current run with freshness/recovery flags, the legacy adapter when only old
/// proposals exist, or `{"status":"none"}`.
pub async fn get(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (dir, _, _) = paths(&state, &session_id)?;
    match review::load(&dir)? {
        Some(loaded) => {
            let current = review::current_source_revisions(&dir);
            let flags = review::flags(&loaded, &current);
            Ok(Json(run_response(
                &loaded.run,
                &loaded.revision,
                Some(serde_json::to_value(flags).unwrap_or(Value::Null)),
            )))
        }
        None => match review::legacy_view(&dir)? {
            Some(view) => Ok(Json(view)),
            None => Ok(Json(json!({ "status": "none" }))),
        },
    }
}

/// Decisions, adjustments, and question actions.
pub async fn put(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<PutReviewRequest>, JsonRejection>,
) -> AppResult<Json<Value>> {
    let request = body(payload)?;
    let (dir, _, _) = paths(&state, &session_id)?;
    let (run, revision) = review::put(&dir, request)?;
    Ok(Json(run_response(&run, &revision, None)))
}

/// Apply selected developments. The world write lock spans the whole
/// preflight/journal/write sequence so two applies cannot interleave.
pub async fn apply(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<ApplyRequest>, JsonRejection>,
) -> AppResult<Json<Value>> {
    let request = body(payload)?;
    let (dir, vault_root, world_root) = paths(&state, &session_id)?;
    let lock = state.world_write_lock(&vault_root).await;
    let _guard = lock.lock().await;
    let report = review::apply(&dir, &vault_root, Some(&world_root), request)?;
    refresh_index(&state, &vault_root, &report);
    reload(&dir, report)
}

/// Resume or close out an interrupted application.
pub async fn recover(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<RecoverRequest>, JsonRejection>,
) -> AppResult<Json<Value>> {
    let request = body(payload)?;
    let (dir, vault_root, world_root) = paths(&state, &session_id)?;
    let lock = state.world_write_lock(&vault_root).await;
    let _guard = lock.lock().await;
    let report = review::recover(&dir, &vault_root, Some(&world_root), request)?;
    refresh_index(&state, &vault_root, &report);
    reload(&dir, report)
}

pub async fn finish(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<FinishRequest>, JsonRejection>,
) -> AppResult<Json<Value>> {
    let request = body(payload)?;
    let (dir, _, _) = paths(&state, &session_id)?;
    let (run, revision) = review::finish(&dir, request)?;
    Ok(Json(run_response(&run, &revision, None)))
}

pub async fn reopen(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<ReopenRequest>, JsonRejection>,
) -> AppResult<Json<Value>> {
    let request = body(payload)?;
    let (dir, _, _) = paths(&state, &session_id)?;
    let (run, revision) = review::reopen(&dir, request)?;
    Ok(Json(run_response(&run, &revision, None)))
}
