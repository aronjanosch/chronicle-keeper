//! Standalone dev server for the core API (no Tauri). Lets the whole HTTP
//! contract be exercised with curl during the rewrite.

use std::net::SocketAddr;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ck_core=debug,info".into()),
        )
        .init();

    let port: u16 = std::env::var("CK_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8000);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let (listener, state) = ck_core::bind(addr).await?;
    let local = listener.local_addr()?;
    tracing::info!("ck-core listening on http://{local}");

    ck_core::serve(listener, state).await
}
