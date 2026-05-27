mod artifacts;
mod campaigns;
mod llm;
mod sessions;
mod transcribe;
mod upload;

use axum::extract::{DefaultBodyLimit, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tower_http::cors::{Any, CorsLayer};

use crate::config::{apply_update, get_config_map, to_response, ConfigResponse, UpdateConfigRequest};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    // CORS is wide-open here; the Tauri shell tightens it (token + webview
    // origin) when embedding. The standalone dev server is loopback-only.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health))
        .route("/config", get(read_config).put(write_config))
        // campaigns
        .route("/campaigns", get(campaigns::list).post(campaigns::create))
        .route("/campaigns/:id", get(campaigns::detail).put(campaigns::update))
        .route(
            "/campaigns/:id/sessions",
            get(campaigns::list_sessions).post(campaigns::create_session),
        )
        .route("/next-session-number", get(campaigns::next_session_number))
        // sessions
        .route("/sessions", get(sessions::list))
        .route("/session/:id", get(sessions::detail))
        .route("/session/:id/metadata", get(sessions::metadata))
        .route("/session-metadata", post(sessions::set_metadata))
        .route("/sessions/:id", delete(sessions::delete))
        // upload + speakers
        .route("/upload", post(upload::upload))
        .route("/label-speakers", post(upload::label_speakers))
        // transcription
        .route("/providers", get(transcribe::providers))
        .route("/transcribe", post(transcribe::transcribe))
        // summarization + export + llm providers
        .route("/prompts", get(llm::list_prompts))
        .route("/summarize", post(llm::summarize))
        .route("/export", post(llm::export_notes))
        .route("/llm-providers", get(llm::list_providers))
        .route("/llm-providers/:id", axum::routing::put(llm::put_provider))
        .route("/llm-providers/:id/test", post(llm::test_provider))
        // artifacts
        .route("/sessions/:id/transcripts", get(artifacts::list_transcripts))
        .route("/sessions/:id/transcripts/:aid/content", get(artifacts::transcript_content))
        .route("/sessions/:id/transcripts/:aid", delete(artifacts::delete_transcript))
        .route("/sessions/:id/summaries", get(artifacts::list_summaries))
        .route("/sessions/:id/summaries/:aid/content", get(artifacts::summary_content))
        .route("/sessions/:id/summaries/:aid", delete(artifacts::delete_summary))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024 * 1024))
        .with_state(state)
        .layer(cors)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn read_config(State(state): State<AppState>) -> AppResult<Json<ConfigResponse>> {
    let map = state.with_db(get_config_map)?;
    Ok(Json(to_response(&map)))
}

async fn write_config(
    State(state): State<AppState>,
    Json(req): Json<UpdateConfigRequest>,
) -> AppResult<Json<ConfigResponse>> {
    if let Some(root) = &req.output_root {
        validate_output_root(root)?;
    }
    let map = state.with_db(|conn| -> AppResult<_> {
        apply_update(conn, &req)?;
        get_config_map(conn)
    })?;
    Ok(Json(to_response(&map)))
}

fn validate_output_root(path: &str) -> AppResult<()> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(
            "Session data folder path cannot be empty.".into(),
        ));
    }
    std::fs::create_dir_all(trimmed).map_err(|e| {
        AppError::BadRequest(format!("Cannot create session data folder {trimmed}: {e}"))
    })?;
    Ok(())
}
