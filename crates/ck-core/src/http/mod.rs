mod artifacts;
mod campaigns;
mod codex;
mod llm;
mod prompts;
mod sessions;
mod transcribe;
mod upload;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tower_http::cors::{Any, CorsLayer};

use crate::config::{
    apply_update, get_config_map, to_response, ConfigResponse, UpdateConfigRequest,
};
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
        .route("/sync/force-mirror", post(force_mirror))
        // campaigns
        .route("/campaigns", get(campaigns::list).post(campaigns::create))
        .route(
            "/campaigns/:id",
            get(campaigns::detail)
                .put(campaigns::update)
                .delete(campaigns::delete),
        )
        .route(
            "/campaigns/:id/sessions",
            get(campaigns::list_sessions).post(campaigns::create_session),
        )
        .route("/next-session-number", get(campaigns::next_session_number))
        .route("/campaigns/:id/recap", post(campaigns::generate_recap))
        // campaign tag vocabulary (tag manager)
        .route("/campaigns/:id/tags", get(campaigns::list_tags))
        .route("/campaigns/:id/tags/rename", post(campaigns::rename_tag))
        .route("/campaigns/:id/tags/delete", post(campaigns::delete_tag))
        // codex entries (Phase 2)
        .route(
            "/campaigns/:id/codex/entries",
            get(codex::list).post(codex::create),
        )
        .route(
            "/campaigns/:id/codex/entries/:eid",
            axum::routing::put(codex::update).delete(codex::delete),
        )
        .route("/campaigns/:id/codex/import", post(codex::import))
        .route("/campaigns/:id/codex/import/commit", post(codex::commit))
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
        .route("/model-status", get(model_status))
        // summary prompt templates (user-managed library)
        .route(
            "/prompt-templates",
            get(prompts::list).post(prompts::create),
        )
        .route(
            "/prompt-templates/restore-defaults",
            post(prompts::restore_defaults),
        )
        .route(
            "/prompt-templates/:id",
            axum::routing::put(prompts::update).delete(prompts::delete),
        )
        // summarization + export + llm providers
        .route("/summarize", post(llm::summarize))
        .route("/export", post(llm::export_notes))
        .route("/llm-providers", get(llm::list_providers))
        .route("/llm-providers/:id", axum::routing::put(llm::put_provider))
        .route("/llm-providers/:id/test", post(llm::test_provider))
        .route("/llm-providers/:id/ping", get(llm::ping_provider))
        // artifacts
        .route(
            "/sessions/:id/transcripts",
            get(artifacts::list_transcripts),
        )
        .route(
            "/sessions/:id/transcripts/:aid/content",
            get(artifacts::transcript_content),
        )
        .route(
            "/sessions/:id/transcripts/:aid",
            delete(artifacts::delete_transcript),
        )
        .route("/sessions/:id/summaries", get(artifacts::list_summaries))
        .route(
            "/sessions/:id/summaries/:aid/content",
            get(artifacts::summary_content),
        )
        .route(
            "/sessions/:id/summaries/:aid",
            delete(artifacts::delete_summary),
        )
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), require_token))
        .with_state(state)
        .layer(cors)
}

/// Gate requests on `x-ck-token` when the state carries a token. `/health` and
/// CORS preflight (OPTIONS) are always allowed.
async fn require_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(expected) = &state.auth_token {
        let exempt = req.method() == Method::OPTIONS || req.uri().path() == "/health";
        if !exempt {
            let provided = headers.get("x-ck-token").and_then(|v| v.to_str().ok());
            if provided != Some(expected.as_str()) {
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }
    Ok(next.run(req).await)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Current state of the one-time model download. The frontend polls this while a
/// `/transcribe` request is in flight to render a progress bar. Returns `idle`
/// when no download is happening (e.g. model already present).
async fn model_status(State(state): State<AppState>) -> Json<crate::state::ModelProgress> {
    let p = state
        .model_progress
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Json(p)
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
    // Trigger an immediate sync so credentials take effect without a restart.
    let sync_state = state.clone();
    tokio::spawn(async move {
        crate::sync::sync_once_recording_error(&sync_state).await;
    });
    Ok(Json(to_response(&map)))
}

/// Make the server an exact copy of this device. Destructive; UI-confirmed.
async fn force_mirror(State(state): State<AppState>) -> AppResult<Json<Value>> {
    crate::sync::force_mirror_sync(&state).await?;
    Ok(Json(json!({ "status": "ok" })))
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
