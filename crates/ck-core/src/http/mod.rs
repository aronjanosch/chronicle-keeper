mod agent;
mod artifacts;
mod atlas;
mod campaigns;
mod codex;
mod codex_update;
mod index;
mod llm;
mod migration;
mod prompts;
mod sessions;
mod transcribe;
mod upload;
mod vault;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{delete, get, post, put};
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
        // campaigns
        .route("/campaigns", get(campaigns::list).post(campaigns::create))
        .route("/seed-example", post(campaigns::seed_example))
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
        .route("/campaigns/:id/export-world", post(campaigns::export_world))
        // campaign tag vocabulary (tag manager)
        .route("/campaigns/:id/tags", get(campaigns::list_tags))
        .route("/campaigns/:id/tags/rename", post(campaigns::rename_tag))
        .route("/campaigns/:id/tags/delete", post(campaigns::delete_tag))
        // codex paste-import (writes vault pages)
        .route("/campaigns/:id/codex/import", post(codex::import))
        .route("/campaigns/:id/codex/import/commit", post(codex::commit))
        // vault pages
        .route("/vault/sniff", post(vault::sniff))
        .route("/campaigns/:id/vault", axum::routing::put(vault::attach))
        .route("/campaigns/:id/vault/import", post(vault::import_notes))
        .route("/campaigns/:id/vault/enhance", post(vault::enhance_pages))
        .route("/campaigns/:id/vault/tree", get(vault::list_tree))
        .route("/campaigns/:id/vault/folders", post(vault::create_folder))
        .route(
            "/campaigns/:id/vault/folders/*folder",
            axum::routing::delete(vault::delete_folder),
        )
        .route("/campaigns/:id/vault/move", post(vault::move_entry))
        .route("/campaigns/:id/vault/promote", post(vault::promote_page))
        .route("/campaigns/:id/vault/bulk", post(vault::bulk))
        // trash (Phase 13D) + page history (13A) + world backup (13E)
        .route("/campaigns/:id/vault/trash", get(vault::trash_list))
        .route(
            "/campaigns/:id/vault/trash/restore",
            post(vault::trash_restore),
        )
        .route("/campaigns/:id/vault/trash/empty", post(vault::trash_empty))
        .route("/campaigns/:id/vault/history", get(vault::history_recent))
        .route(
            "/campaigns/:id/vault/history/*page",
            get(vault::page_history),
        )
        .route(
            "/campaigns/:id/vault/history-restore",
            post(vault::history_restore),
        )
        .route("/campaigns/:id/backup", post(vault::backup))
        .route("/campaigns/:id/vault/kinds", get(vault::kind_schemas))
        .route("/campaigns/:id/vault/snippets", get(vault::list_snippets))
        .route("/campaigns/:id/vault/templates", get(vault::list_templates))
        .route(
            "/campaigns/:id/vault/templates/:name",
            put(vault::save_template).delete(vault::delete_template),
        )
        .route("/campaigns/:id/vault/assets", post(vault::upload_asset))
        .route("/campaigns/:id/vault/assets/*name", get(vault::asset_bytes))
        .route(
            "/campaigns/:id/vault/pages",
            get(vault::list_pages).post(vault::create_page),
        )
        .route(
            "/campaigns/:id/vault/pages/*page",
            get(vault::read_page)
                .put(vault::write_page)
                .delete(vault::delete_page),
        )
        // atlas maps (files-as-truth: <world>/Atlas/<id>.json)
        .route(
            "/campaigns/:id/atlas/maps",
            get(atlas::list_maps).post(atlas::create_map),
        )
        .route(
            "/campaigns/:id/atlas/maps/:map",
            get(atlas::read_map)
                .put(atlas::write_map)
                .delete(atlas::delete_map),
        )
        .route(
            "/campaigns/:id/atlas/maps/:map/image",
            get(atlas::map_image).put(atlas::replace_image),
        )
        // vault index (links/backlinks, search, page tags, change counter)
        .route("/campaigns/:id/vault/seq", get(index::seq))
        .route("/campaigns/:id/vault/index/links", get(index::links))
        .route("/campaigns/:id/vault/diagnostics", get(index::diagnostics))
        .route("/campaigns/:id/vault/index/tags", get(index::tags))
        .route("/campaigns/:id/vault/relations", get(index::relations))
        .route("/campaigns/:id/vault/query", get(index::query))
        .route("/campaigns/:id/vault/search", get(index::search))
        .route("/campaigns/:id/timeline", get(index::timeline))
        .route("/campaigns/:id/sessions/search", get(index::session_search))
        // the Keeper (Phase 6): agent chats
        .route(
            "/campaigns/:id/agent/chats",
            get(agent::list_chats).post(agent::create_chat),
        )
        .route(
            "/campaigns/:id/agent/chats/:cid",
            get(agent::get_chat).delete(agent::delete_chat),
        )
        .route(
            "/campaigns/:id/agent/chats/:cid/messages",
            post(agent::send_message),
        )
        .route("/campaigns/:id/agent/chats/:cid/abort", post(agent::abort))
        .route(
            "/campaigns/:id/agent/chats/:cid/approve",
            post(agent::approve),
        )
        .route("/campaigns/:id/agent/chats/:cid/undo", post(agent::undo))
        .route(
            "/campaigns/:id/agent/chats/:cid/attachments",
            get(agent::list_attachments).post(agent::add_attachment),
        )
        .route(
            "/campaigns/:id/agent/chats/:cid/attachments/:aid",
            axum::routing::delete(agent::delete_attachment),
        )
        // the Keeper's memory + World Brief
        .route("/campaigns/:id/agent/skills", get(agent::list_skills))
        .route("/skills", get(agent::list_skills))
        .route("/campaigns/:id/agent/memory", get(agent::list_memory))
        .route(
            "/campaigns/:id/agent/memory/:name",
            delete(agent::delete_memory),
        )
        .route(
            "/campaigns/:id/agent/brief",
            get(agent::get_brief).post(agent::run_brief),
        )
        // sessions
        .route("/sessions", get(sessions::list))
        .route("/session/:id", get(sessions::detail))
        .route("/session/:id/metadata", get(sessions::metadata))
        .route("/session-metadata", post(sessions::set_metadata))
        .route("/sessions/:id", delete(sessions::delete))
        // Update the Codex (Phase 5): AI page proposals after a summary
        .route(
            "/sessions/:id/codex-update",
            get(codex_update::get)
                .post(codex_update::generate)
                .put(codex_update::put),
        )
        .route(
            "/sessions/:id/codex-update/commit",
            post(codex_update::commit),
        )
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
        // migration (0.X → 1.0 world format)
        .route("/migrations/status", get(migration::status))
        .route("/migrations/run", post(migration::run))
        // summarization + export + llm providers
        .route("/summarize", post(llm::summarize))
        .route("/summarize/stream", post(llm::summarize_stream))
        .route("/export", post(llm::export_notes))
        .route("/llm-providers", get(llm::list_providers))
        .route("/llm-providers/:id", axum::routing::put(llm::put_provider))
        .route("/llm-providers/:id/test", post(llm::test_provider))
        .route("/llm-providers/:id/ping", get(llm::ping_provider))
        .route("/llm-providers/:id/models", get(llm::list_provider_models))
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
            delete(artifacts::delete_summary).put(artifacts::update_summary),
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
