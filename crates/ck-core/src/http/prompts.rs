//! Summary prompt template endpoints. The user-managed library of system prompts
//! backing the Summarize template picker and the Settings template manager.
//! Pattern mirrors `codex.rs`.

use axum::extract::{Path, State};
use axum::Json;

use crate::error::AppResult;
use crate::models::{PromptTemplate, PromptTemplateCreate, PromptTemplateUpdate};
use crate::state::AppState;
use crate::store::prompts;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<PromptTemplate>>> {
    state.with_db(|conn| Ok(Json(prompts::list(conn)?)))
}

pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<PromptTemplateCreate>,
) -> AppResult<Json<PromptTemplate>> {
    state.with_db(|conn| Ok(Json(prompts::create(conn, &req)?)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PromptTemplateUpdate>,
) -> AppResult<Json<PromptTemplate>> {
    state.with_db(|conn| Ok(Json(prompts::update(conn, &id, &req)?)))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    state.with_db(|conn| {
        prompts::delete(conn, &id)?;
        Ok(Json(serde_json::json!({ "status": "ok" })))
    })
}

/// Re-create any deleted builtin templates; returns the full list.
pub async fn restore_defaults(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PromptTemplate>>> {
    state.with_db(|conn| Ok(Json(prompts::restore_defaults(conn)?)))
}
