use std::convert::Infallible;
use std::path::PathBuf;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::Stream;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::prep_suggest::{self, SuggestProgress, SuggestRequest};
use crate::session_prep::{self, CarryRequest, PrepResponse, PutPrepRequest};
use crate::state::AppState;
use crate::store::sessions;

pub async fn get(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> AppResult<Json<PrepResponse>> {
    state.with_db(|conn| {
        let loc = sessions::locate(conn, &session_id)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {session_id}")))?;
        if loc.world.is_none() {
            return Err(AppError::Unprocessable(
                "Session preparation requires a world session".into(),
            ));
        }
        Ok(Json(session_prep::read(&loc.dir)?))
    })
}

pub async fn put(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<PutPrepRequest>, JsonRejection>,
) -> AppResult<Json<PrepResponse>> {
    let Json(request) = payload.map_err(|error| AppError::Unprocessable(error.body_text()))?;
    state.with_db(|conn| {
        let loc = sessions::locate(conn, &session_id)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {session_id}")))?;
        if loc.world.is_none() {
            return Err(AppError::Unprocessable(
                "Session preparation requires a world session".into(),
            ));
        }
        Ok(Json(session_prep::put(&loc.dir, request)?))
    })
}

/// Session directory and owning world root of a world session.
fn world_session(state: &AppState, session_id: &str) -> AppResult<(PathBuf, PathBuf)> {
    let sid = session_id.to_string();
    state.with_db(move |conn| {
        let loc = sessions::locate(conn, &sid)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {sid}")))?;
        let Some((root, _)) = loc.world else {
            return Err(AppError::Unprocessable(
                "Session preparation requires a world session".into(),
            ));
        };
        Ok((loc.dir, root))
    })
}

/// Keeper prep suggestions. SSE like review generation: it calls the provider,
/// and a disconnect must stop the run. Nothing it returns is persisted.
pub async fn suggest(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<SuggestRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    tokio::spawn(async move {
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
        state.review_job_end(&job);
        match result {
            Ok(suggestions) => send(json!({ "stage": "done", "suggestions": suggestions })),
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

/// Carry chosen cards/possibilities from an earlier session into this one.
/// Called on the destination; both sessions must belong to the same world.
pub async fn carry(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<CarryRequest>, JsonRejection>,
) -> AppResult<Json<PrepResponse>> {
    let Json(request) = payload.map_err(|error| AppError::Unprocessable(error.body_text()))?;
    let (dest_dir, dest_world) = world_session(&state, &session_id)?;
    let (source_dir, source_world) = world_session(&state, &request.source_session_id)?;
    if dest_world != source_world {
        return Err(AppError::Unprocessable(
            "Both sessions must belong to the same world".into(),
        ));
    }
    Ok(Json(session_prep::carry(
        &dest_dir,
        &session_id,
        &source_dir,
        &request,
    )?))
}
