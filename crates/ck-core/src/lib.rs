pub mod agent;
pub mod atlas;
pub mod backup;
pub mod codex_import;
pub mod codex_update;
pub mod config;
pub mod db;
pub mod error;
pub mod export;
pub mod foundry;
pub mod history;
pub mod http;
pub mod index_watch;
pub mod llm;
pub mod models;
pub mod normalize;
pub mod paths;
pub mod prompts;
pub mod seed;
pub mod session_files;
pub mod session_search;
pub mod state;
pub mod store;
pub mod summarize;
pub mod timeline;
pub mod transcript_format;
#[cfg(feature = "transcription")]
pub mod transcription;
pub mod trash;
pub mod vault;
pub mod world_config;

use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::net::TcpListener;

use crate::paths::Paths;
use crate::state::AppState;

/// Bind an axum server for the core API and return the listener plus the actual
/// bound address (useful when binding to an ephemeral port). The caller drives
/// `serve` to run it — this lets the Tauri shell learn the chosen port before
/// the webview loads.
pub async fn bind(addr: SocketAddr, seed_example: bool) -> Result<(TcpListener, AppState)> {
    let paths = Paths::resolve()?;
    let state = AppState::new(paths)?;
    // Desktop-only onboarding: seed a sample campaign on a fresh DB. Best-effort —
    // a seed failure must never block the app from starting. Skipped while a
    // legacy DB awaits migration (seeding first would duplicate its demo).
    if seed_example && !state.paths.legacy_db_path().exists() {
        if let Err(e) = state.with_db(seed::seed_example_if_first) {
            tracing::warn!("example seed skipped: {e}");
        }
    }
    // Migration (1.7-F) is user-triggered via the migration screen — not auto-run.
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    Ok((listener, state))
}

/// Run the API server on an already-bound listener until shutdown.
pub async fn serve(listener: TcpListener, state: AppState) -> Result<()> {
    let app = http::router(state);
    axum::serve(listener, app).await.context("axum serve")?;
    Ok(())
}
