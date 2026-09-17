use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::Json;

use crate::error::{AppError, AppResult};
use crate::session_prep::{self, PrepResponse, PutPrepRequest};
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
