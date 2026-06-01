use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::paths::Paths;

/// Progress of the one-time transcription-model download, polled by the frontend
/// via `GET /model-status` while a `/transcribe` request is in flight.
///
/// Lives here (not in the feature-gated `transcription` module) so the status
/// endpoint compiles in the transcription-disabled server build too — there it
/// simply stays `Idle`.
#[derive(Clone, Debug, Serialize)]
pub struct ModelProgress {
    /// "idle" | "downloading" | "extracting" | "ready" | "transcribing" | "error"
    pub phase: String,
    pub downloaded: u64,
    /// Total bytes if the server sent Content-Length, else 0 (unknown).
    pub total: u64,
    pub message: Option<String>,
    pub track_done: u64,
    pub track_total: u64,
}

impl Default for ModelProgress {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            downloaded: 0,
            total: 0,
            message: None,
            track_done: 0,
            track_total: 0,
        }
    }
}

impl ModelProgress {
    pub fn set(handle: &Arc<Mutex<Self>>, phase: &str, downloaded: u64, total: u64) {
        // Recover the guard on poison rather than cascading a panic into a
        // process kill — a panicked writer leaves the progress struct usable.
        let mut p = handle.lock().unwrap_or_else(|e| e.into_inner());
        *p = Self {
            phase: phase.into(),
            downloaded,
            total,
            ..Default::default()
        };
    }

    /// Report transcription progress: `done` tracks finished of `total`, `label`
    /// the track now being processed. Polled via `GET /model-status`.
    pub fn set_transcribe(handle: &Arc<Mutex<Self>>, done: u64, total: u64, label: &str) {
        let mut p = handle.lock().unwrap_or_else(|e| e.into_inner());
        *p = Self {
            phase: "transcribing".into(),
            message: Some(label.to_string()),
            track_done: done,
            track_total: total,
            ..Default::default()
        };
    }

    pub fn set_error(handle: &Arc<Mutex<Self>>, message: String) {
        let mut p = handle.lock().unwrap_or_else(|e| e.into_inner());
        p.phase = "error".into();
        p.message = Some(message);
    }
}

/// Shared application state handed to every axum handler.
///
/// SQLite is single-writer; a `Mutex<Connection>` is the simplest correct
/// choice for a local single-user app and avoids a pool dependency. DB calls
/// are short, so brief lock contention is a non-issue here.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub paths: Paths,
    /// When set, every request (except /health and CORS preflight) must carry a
    /// matching `x-ck-token` header. The Tauri shell sets a per-launch token;
    /// the standalone dev server leaves it `None`. Also the Sprint-2 auth seam.
    pub auth_token: Option<String>,
    /// Shared progress of the one-time model download (see [`ModelProgress`]).
    pub model_progress: Arc<Mutex<ModelProgress>>,
}

impl AppState {
    pub fn new(paths: Paths) -> Result<Self> {
        let conn = crate::db::open(&paths.db_path())?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            paths,
            auth_token: None,
            model_progress: Arc::new(Mutex::new(ModelProgress::default())),
        })
    }

    /// Run a closure with the locked DB connection.
    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        // Recover the guard on poison: SQLite state is intact after a panicked
        // borrow, so don't let one bad request kill the whole process.
        let conn = self.db.lock().unwrap_or_else(|e| e.into_inner());
        f(&conn)
    }
}
