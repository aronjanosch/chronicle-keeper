use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::error::AppResult;
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
    /// Per-world `.ck/index.db` connections, keyed by vault path. First access
    /// opens + rebuilds; the cache lives for the process.
    pub indexes: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<Connection>>>>>,
    /// File watchers keeping each open world's index fresh (external edits).
    pub watchers: Arc<Mutex<HashMap<PathBuf, crate::index_watch::WatchHandle>>>,
    /// Per-vault change counters, bumped by the watcher on external edits.
    /// The frontend polls `GET .../vault/seq` and refreshes when it moves.
    pub vault_seqs: Arc<Mutex<HashMap<PathBuf, Arc<AtomicU64>>>>,
    /// Echo guard: CK's own vault writes, so the watcher skips them.
    pub suppress: crate::index_watch::SuppressMap,
    /// One active Keeper run per world: campaign id → cancel flag. Insert =
    /// run start (second insert → 409), flag set = abort requested.
    pub agent_runs: Arc<Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>>,
}

impl AppState {
    pub fn new(paths: Paths) -> Result<Self> {
        let conn = crate::db::open(&paths.db_path())?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            paths,
            auth_token: None,
            model_progress: Arc::new(Mutex::new(ModelProgress::default())),
            indexes: Arc::new(Mutex::new(HashMap::new())),
            watchers: Arc::new(Mutex::new(HashMap::new())),
            vault_seqs: Arc::new(Mutex::new(HashMap::new())),
            suppress: Arc::new(Mutex::new(HashMap::new())),
            agent_runs: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Run a closure with the locked DB connection.
    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        // Recover the guard on poison: SQLite state is intact after a panicked
        // borrow, so don't let one bad request kill the whole process.
        let conn = self.db.lock().unwrap_or_else(|e| e.into_inner());
        f(&conn)
    }

    /// Open-or-get the index for a vault. First call builds it (full scan,
    /// hash-skip — cheap on a warm index) and starts the file watcher.
    pub fn index_for(&self, vault: &Path) -> AppResult<Arc<Mutex<Connection>>> {
        let mut map = self.indexes.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(conn) = map.get(vault) {
            return Ok(conn.clone());
        }
        let conn = crate::store::index::open_index(vault)?;
        crate::store::index::rebuild(&conn, vault)?;
        let arc = Arc::new(Mutex::new(conn));
        map.insert(vault.to_path_buf(), arc.clone());
        match crate::index_watch::start(
            vault.to_path_buf(),
            arc.clone(),
            self.suppress.clone(),
            self.seq_for(vault),
        ) {
            Ok(handle) => {
                let mut watchers = self.watchers.lock().unwrap_or_else(|e| e.into_inner());
                watchers.insert(vault.to_path_buf(), handle);
            }
            Err(e) => tracing::warn!("vault watcher not started for {}: {e}", vault.display()),
        }
        Ok(arc)
    }

    fn seq_for(&self, vault: &Path) -> Arc<AtomicU64> {
        let mut map = self.vault_seqs.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(vault.to_path_buf()).or_default().clone()
    }

    /// Current change counter for a vault, starting its watcher if needed.
    pub fn vault_seq(&self, vault: &Path) -> AppResult<u64> {
        self.index_for(vault)?;
        Ok(self.seq_for(vault).load(Ordering::Relaxed))
    }

    /// Note a CK-side vault write so the watcher ignores its echo.
    pub fn note_vault_write(&self, vault: &Path, rel: &str) {
        crate::index_watch::record_write(&self.suppress, &vault.join(rel));
    }

    /// Run a closure with the locked index connection for a vault.
    pub fn with_index<T>(&self, vault: &Path, f: impl FnOnce(&Connection) -> T) -> AppResult<T> {
        let idx = self.index_for(vault)?;
        let conn = idx.lock().unwrap_or_else(|e| e.into_inner());
        Ok(f(&conn))
    }

    /// Drop a cached index connection + its watcher (vault re-pointed or deleted).
    pub fn evict_index(&self, vault: &Path) {
        let mut map = self.indexes.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(vault);
        let mut watchers = self.watchers.lock().unwrap_or_else(|e| e.into_inner());
        watchers.remove(vault); // Drop stops the watcher thread
        let mut seqs = self.vault_seqs.lock().unwrap_or_else(|e| e.into_inner());
        seqs.remove(vault);
    }
}
