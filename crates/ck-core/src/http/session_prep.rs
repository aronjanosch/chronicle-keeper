use std::convert::Infallible;
use tracing::Instrument;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::Stream;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::prep_suggest::{self, SuggestProgress, SuggestRequest};
use crate::session_prep::{self, CarryRequest, OpsRequest, PrepCtx, PrepResponse};
use crate::state::AppState;
use crate::store::sessions;

pub async fn get(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> AppResult<Json<PrepResponse>> {
    let ctx = prep_ctx(&state, &session_id)?;
    let response = session_prep::read(&ctx)?;
    reindex(&state, &ctx, &response);
    Ok(Json(response))
}

pub async fn ops(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<OpsRequest>, JsonRejection>,
) -> AppResult<Json<PrepResponse>> {
    let Json(request) = payload.map_err(|error| AppError::Unprocessable(error.body_text()))?;
    let ctx = prep_ctx(&state, &session_id)?;
    let lock = state.world_write_lock(&ctx.world_root).await;
    let _guard = lock.lock().await;
    let response = session_prep::apply(&ctx, &request)?;
    reindex(&state, &ctx, &response);
    Ok(Json(response))
}

pub(crate) fn prep_ctx(state: &AppState, session_id: &str) -> AppResult<PrepCtx> {
    let sid = session_id.to_string();
    state.with_db(move |conn| {
        let loc = sessions::locate(conn, &sid)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {sid}")))?;
        PrepCtx::of(conn, &loc)
    })
}

/// The prep page is an ordinary Codex page: keep the index current and tell
/// the watcher the write was ours. Reads can write too (legacy migration).
fn reindex(state: &AppState, ctx: &PrepCtx, response: &PrepResponse) {
    let Some(rel) = &response.page else { return };
    state.note_vault_write(&ctx.vault, rel);
    let _ = state.with_index(&ctx.vault, |conn| {
        let _ = crate::store::index::upsert_path(conn, &ctx.vault, rel);
    });
}

/// Keeper prep suggestions. SSE like review generation: it calls the provider,
/// and a disconnect must stop the run. Nothing it returns is persisted.
pub async fn suggest(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<SuggestRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    tokio::spawn(
        async move {
            let frame = |val: &Value| {
                Event::default()
                    .json_data(val)
                    .unwrap_or_else(|_| Event::default())
            };
            let job = format!("{session_id}:prep");
            let cancel = match state.review_job_begin(&job) {
                Ok(flag) => flag,
                Err(e) => {
                    let _ = tx.send(frame(
                        &json!({ "stage": "error", "code": 409, "message": e.to_string() }),
                    ));
                    return;
                }
            };
            // The SSE body is dropped the moment the client disconnects; stop the
            // run then instead of at the next progress frame.
            let disconnect = {
                let (tx, cancel) = (tx.clone(), cancel.clone());
                tokio::spawn(async move {
                    tx.closed().await;
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                })
            };
            let send = |val: Value| {
                if tx.send(frame(&val)).is_err() {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            };
            let result =
                prep_suggest::suggest_streamed(&state, &session_id, &req, &cancel, |p| match p {
                    SuggestProgress::Reading => send(json!({ "stage": "reading" })),
                    SuggestProgress::Building => send(json!({ "stage": "building" })),
                })
                .await;
            disconnect.abort();
            state.review_job_end(&job);
            match result {
                Ok(suggestions) => send(json!({ "stage": "done", "suggestions": suggestions })),
                Err(e) => send(json!({
                    "stage": "error",
                    "code": e.status().as_u16(),
                    "message": e.to_string(),
                })),
            }
        }
        .in_current_span(),
    );
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|ev| (Ok(ev), rx))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Carry chosen cards/possibilities from an earlier session into this one.
/// Called on the destination; both sessions must belong to the same world.
pub async fn carry(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<CarryRequest>, JsonRejection>,
) -> AppResult<Json<PrepResponse>> {
    let Json(request) = payload.map_err(|error| AppError::Unprocessable(error.body_text()))?;
    let dest = prep_ctx(&state, &session_id)?;
    let source = prep_ctx(&state, &request.source_session_id)?;
    if dest.world_root != source.world_root {
        return Err(AppError::Unprocessable(
            "Both sessions must belong to the same world".into(),
        ));
    }
    let lock = state.world_write_lock(&dest.world_root).await;
    let _guard = lock.lock().await;
    let response = session_prep::carry(&dest, &source, &request)?;
    reindex(&state, &dest, &response);
    Ok(Json(response))
}
