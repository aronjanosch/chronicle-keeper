//! Session review HTTP surface (contract §7). Generation and clarification
//! (SC-05) arrive with their own SSE handlers; everything here is the durable
//! record and its safe application.

use std::convert::Infallible;
use std::path::PathBuf;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::Stream;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::review_generate::{self, ClarifyRequest, GenProgress, GenerateRequest};
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

/// Streaming generation (contract §7). Frames: `{stage:"reading"|"grounding"|
/// "building"}`, then `{stage:"done", ...}` or `{stage:"error", code, message}`.
/// Dropping the stream cancels the run, so a closed tab cannot leave a
/// half-generated review behind.
pub async fn generate(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<GenerateRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    tokio::spawn(async move {
        let frame = |val: &Value| {
            Event::default()
                .json_data(val)
                .unwrap_or_else(|_| Event::default())
        };
        let cancel = match state.review_job_begin(&session_id) {
            Ok(flag) => flag,
            Err(e) => {
                let _ = tx.send(frame(
                    &json!({ "stage": "error", "code": 409, "message": e.to_string() }),
                ));
                return;
            }
        };
        // A failed send means the client is gone: stop the run rather than
        // finish a review nobody is waiting for.
        let send = |val: Value| {
            if tx.send(frame(&val)).is_err() {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        };
        let result =
            review_generate::generate_streamed(&state, &session_id, &req, &cancel, |p| match p {
                GenProgress::Reading => send(json!({ "stage": "reading" })),
                GenProgress::Grounding => send(json!({ "stage": "grounding" })),
                GenProgress::Building => send(json!({ "stage": "building" })),
            })
            .await;
        state.review_job_end(&session_id);
        match result {
            Ok(run) => {
                // The revision the client must send back with its next mutation
                // comes from the persisted bytes, not from the run in memory.
                let revision = crate::session_review::load(&run_dir(&state, &session_id))
                    .ok()
                    .flatten()
                    .map(|l| l.revision)
                    .unwrap_or_default();
                send(json!({ "stage": "done", "revision": revision, "run": run }));
            }
            Err(e) => send(json!({
                "stage": "error",
                "code": e.status().as_u16(),
                "message": e.to_string(),
            })),
        }
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|ev| (Ok(ev), rx))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Best-effort session directory for the done frame; an unresolvable session
/// already failed generation.
fn run_dir(state: &AppState, session_id: &str) -> PathBuf {
    paths(state, session_id)
        .map(|(dir, _, _)| dir)
        .unwrap_or_default()
}

/// Explicit Cancel. Disconnecting does the same thing.
pub async fn cancel_generate(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> AppResult<Json<Value>> {
    let cancelled = state.review_job_cancel(&session_id);
    Ok(Json(json!({ "cancelled": cancelled })))
}

/// Clarify an open question with the GM's own answer. SSE, like generation,
/// because it calls the provider; the GM's text is the evidence it records.
pub async fn clarify(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<ClarifyRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    tokio::spawn(async move {
        let send = |val: Value| {
            let ev = Event::default()
                .json_data(&val)
                .unwrap_or_else(|_| Event::default());
            let _ = tx.send(ev);
        };
        send(json!({ "stage": "building" }));
        match review_generate::clarify(&state, &session_id, &req).await {
            Ok(run) => {
                let revision = crate::session_review::load(&run_dir(&state, &session_id))
                    .ok()
                    .flatten()
                    .map(|l| l.revision)
                    .unwrap_or_default();
                send(json!({ "stage": "done", "revision": revision, "run": run }));
            }
            Err(e) => send(json!({
                "stage": "error",
                "code": e.status().as_u16(),
                "message": e.to_string(),
            })),
        }
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|ev| (Ok(ev), rx))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
