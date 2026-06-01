pub mod codex_import;
pub mod config;
pub mod db;
pub mod error;
pub mod export;
pub mod http;
pub mod llm;
pub mod models;
pub mod normalize;
pub mod paths;
pub mod prompts;
pub mod seed;
pub mod state;
pub mod store;
pub mod summarize;
pub mod sync;
pub mod transcript_format;
#[cfg(feature = "transcription")]
pub mod transcription;

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
    // a seed failure must never block the app from starting.
    if seed_example {
        if let Err(e) = state.with_db(seed::seed_example_if_first) {
            tracing::warn!("example seed skipped: {e}");
        }
    }
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
