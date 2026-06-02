use std::net::SocketAddr;

use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Launch the embedded ck-core API on an ephemeral loopback port, then open the
/// webview with the chosen base URL + a per-launch auth token injected before
/// the page scripts run. The server is an in-process tokio task (no sidecar).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Linux/WebKitGTK reliability: some compositors (nested/VM Wayland, certain
    // GPU drivers) crash the webview with "Error 71 (Protocol error)" or render
    // blank. Force the X11 backend (via Xwayland) and disable the DMABUF
    // renderer unless the user has set these explicitly. No-op on macOS/Windows.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("GDK_BACKEND").is_none() {
            std::env::set_var("GDK_BACKEND", "x11");
        }
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ck_core=info,info".into()),
        )
        .init();

    let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let (listener, mut state) = rt
        .block_on(ck_core::bind(addr, true))
        .expect("bind ck-core");
    let bound = listener.local_addr().expect("local addr");

    let token = random_token();
    state.auth_token = Some(token.clone());
    let base = format!("http://{bound}");
    tracing::info!("embedded ck-core on {base}");

    // Background multi-device sync runs on the same runtime as the API server.
    // No-op until the user configures a sync URL + token (see ck_core::sync).
    let sync_state = state.clone();

    // Serve forever on the runtime in a background thread.
    std::thread::spawn(move || {
        rt.block_on(async move {
            tokio::spawn(sync_loop(sync_state));
            if let Err(e) = ck_core::serve(listener, state).await {
                tracing::error!("ck-core server stopped: {e:#}");
            }
        });
    });

    let init_script =
        format!("window.__CK_API_BASE__ = {base:?}; window.__CK_TOKEN__ = {token:?};");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Chronicle Keeper")
                .inner_size(1200.0, 800.0)
                .min_inner_size(800.0, 600.0)
                .initialization_script(&init_script)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Periodic offline-first sync: flush once on startup, then every 5 minutes.
/// `sync_once` is a cheap no-op when no sync server is configured. Unsynced
/// writes carry a persisted `dirty` flag, so anything missed on shutdown is
/// pushed on the next launch — no explicit shutdown flush needed.
async fn sync_loop(state: ck_core::state::AppState) {
    ck_core::sync::sync_once_recording_error(&state).await;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(300));
    tick.tick().await; // first tick is immediate — consume it
    loop {
        tick.tick().await;
        ck_core::sync::sync_once_recording_error(&state).await;
    }
}

fn random_token() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("rng");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
