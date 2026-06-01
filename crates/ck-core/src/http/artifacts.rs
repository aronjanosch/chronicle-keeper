use axum::extract::{Path as AxPath, State};
use axum::Json;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::models::ArtifactInfo;
use crate::state::AppState;
use crate::store::artifacts;

async fn list(
    state: &AppState,
    session_id: &str,
    kind: &str,
) -> AppResult<Json<Vec<ArtifactInfo>>> {
    let items = state.with_db(|conn| artifacts::list_artifacts(conn, session_id, Some(kind)))?;
    Ok(Json(items))
}

async fn content(state: &AppState, session_id: &str, artifact_id: i64) -> AppResult<String> {
    let art = state
        .with_db(|conn| artifacts::get_artifact(conn, artifact_id))?
        .filter(|a| a.session_id == session_id)
        .ok_or_else(|| AppError::NotFound(format!("Artifact not found: {artifact_id}")))?;
    state
        .with_db(|conn| artifacts::get_content(conn, art.id))?
        .ok_or_else(|| AppError::NotFound(format!("Artifact not found: {artifact_id}")))
}

async fn delete(state: &AppState, session_id: &str, artifact_id: i64) -> AppResult<Json<Value>> {
    let _art = state
        .with_db(|conn| artifacts::get_artifact(conn, artifact_id))?
        .filter(|a| a.session_id == session_id)
        .ok_or_else(|| AppError::NotFound(format!("Artifact not found: {artifact_id}")))?;
    state.with_db(|conn| artifacts::delete_artifact(conn, artifact_id))?;
    Ok(Json(
        json!({ "status": "deleted", "artifact_id": artifact_id }),
    ))
}

// transcripts
pub async fn list_transcripts(
    State(s): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Vec<ArtifactInfo>>> {
    list(&s, &id, "transcript").await
}
pub async fn transcript_content(
    State(s): State<AppState>,
    AxPath((id, aid)): AxPath<(String, i64)>,
) -> AppResult<String> {
    content(&s, &id, aid).await
}
pub async fn delete_transcript(
    State(s): State<AppState>,
    AxPath((id, aid)): AxPath<(String, i64)>,
) -> AppResult<Json<Value>> {
    delete(&s, &id, aid).await
}

// summaries
pub async fn list_summaries(
    State(s): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Vec<ArtifactInfo>>> {
    list(&s, &id, "summary").await
}
pub async fn summary_content(
    State(s): State<AppState>,
    AxPath((id, aid)): AxPath<(String, i64)>,
) -> AppResult<String> {
    content(&s, &id, aid).await
}
pub async fn delete_summary(
    State(s): State<AppState>,
    AxPath((id, aid)): AxPath<(String, i64)>,
) -> AppResult<Json<Value>> {
    delete(&s, &id, aid).await
}
