use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::models::{SessionInfo, SessionMetadataRequest};
use crate::state::AppState;
use crate::store::sessions;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<SessionInfo>>> {
    state.with_db(|conn| Ok(Json(sessions::list_sessions(conn)?)))
}

pub async fn detail(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| Ok(Json(sessions::get_session_object(conn, &session_id)?)))
}

pub async fn metadata(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| Ok(Json(sessions::get_campaign_metadata(conn, &session_id)?)))
}

pub async fn set_metadata(
    State(state): State<AppState>,
    Json(req): Json<SessionMetadataRequest>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        let campaign = sessions::set_campaign_metadata(conn, &req)?;
        Ok(Json(json!({ "status": "success", "campaign": campaign })))
    })
}

pub async fn delete(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        sessions::delete_session(conn, &session_id)?;
        Ok(Json(
            json!({ "status": "deleted", "session_id": session_id }),
        ))
    })
}
