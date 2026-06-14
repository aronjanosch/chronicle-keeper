//! File watcher: keeps a world's `.ck/index.db` in sync with external edits
//! (Obsidian, text editors, folder sync). One watcher per open world, watching
//! the vault folder; dot-dirs and reserved dirs are filtered in `process`, so
//! the index writing to itself can never feed back — even when an adopted
//! vault (`codex_root = "."`) holds `.ck/` inside the watched tree.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use notify::{RecursiveMode, Watcher};
use rusqlite::Connection;

use crate::store::index;

const DEBOUNCE: Duration = Duration::from_millis(200);
const SUPPRESS_WINDOW: Duration = Duration::from_secs(3);

/// CK's own writes, recorded (path → mtime at write, when) so the watcher can
/// drop the echo instead of reparsing what we just indexed ourselves.
pub type SuppressMap = Arc<Mutex<HashMap<PathBuf, (SystemTime, Instant)>>>;

pub struct WatchHandle {
    _watcher: notify::RecommendedWatcher,
    stop: Arc<AtomicBool>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Record a CK-side write. Call right after writing a vault file.
pub fn record_write(suppress: &SuppressMap, abs: &Path) {
    // Canonical key: events arrive symlink-resolved (e.g. /tmp → /private/tmp).
    let abs = abs.canonicalize().unwrap_or_else(|_| abs.to_path_buf());
    let mtime = abs
        .metadata()
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut map = suppress.lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, (_, when)| when.elapsed() < SUPPRESS_WINDOW);
    map.insert(abs, (mtime, Instant::now()));
}

pub fn start(
    vault: PathBuf,
    index: Arc<Mutex<Connection>>,
    suppress: SuppressMap,
    seq: Arc<AtomicU64>,
) -> notify::Result<WatchHandle> {
    let (tx, rx) = mpsc::channel::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            for path in event.paths {
                let _ = tx.send(path);
            }
        }
    })?;
    watcher.watch(&vault, RecursiveMode::Recursive)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    // Events arrive symlink-resolved; strip_prefix needs the canonical root.
    let base = vault.canonicalize().unwrap_or(vault);
    std::thread::spawn(move || debounce_loop(rx, base, index, suppress, seq, stop2));
    Ok(WatchHandle {
        _watcher: watcher,
        stop,
    })
}

fn debounce_loop(
    rx: mpsc::Receiver<PathBuf>,
    vault: PathBuf,
    index: Arc<Mutex<Connection>>,
    suppress: SuppressMap,
    seq: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
) {
    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let timeout = if pending.is_empty() {
            Duration::from_millis(500)
        } else {
            Duration::from_millis(50)
        };
        match rx.recv_timeout(timeout) {
            Ok(path) => {
                pending.insert(path, Instant::now());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        let due: Vec<PathBuf> = pending
            .iter()
            .filter(|(_, t)| t.elapsed() >= DEBOUNCE)
            .map(|(p, _)| p.clone())
            .collect();
        for path in due {
            pending.remove(&path);
            process(&vault, &index, &suppress, &seq, &path);
        }
    }
}

// Re-stat rather than trusting the event kind: FSEvents coalesces, and editor
// atomic saves (write-temp-then-rename) report misleading kinds.
fn process(
    vault: &Path,
    index: &Arc<Mutex<Connection>>,
    suppress: &SuppressMap,
    seq: &Arc<AtomicU64>,
    abs: &Path,
) {
    let Ok(rel) = abs.strip_prefix(vault) else {
        return;
    };
    let mut parts: Vec<&str> = Vec::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(s) => match s.to_str() {
                Some(s) if !s.starts_with('.') && !crate::vault::is_reserved_dir(s) => {
                    parts.push(s)
                }
                _ => return,
            },
            _ => return,
        }
    }
    if parts.is_empty() {
        return;
    }
    if abs.extension().and_then(|e| e.to_str()) != Some("md") {
        // Folder created/renamed/deleted (or unknown deleted path): nothing to
        // index, but the Explorer tree changed — signal the frontend.
        if abs.is_dir() || !abs.exists() {
            seq.fetch_add(1, Ordering::Relaxed);
        }
        return;
    }

    {
        let mut map = suppress.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((mtime, when)) = map.get(abs).cloned() {
            let unchanged = abs
                .metadata()
                .and_then(|m| m.modified())
                .map(|cur| cur <= mtime)
                .unwrap_or(false);
            if when.elapsed() < SUPPRESS_WINDOW && unchanged {
                map.remove(abs);
                return;
            }
            map.remove(abs);
        }
    }

    let rel = parts.join("/");
    let conn = index.lock().unwrap_or_else(|e| e.into_inner());
    let result = if abs.is_file() {
        index::upsert_path(&conn, vault, &rel)
    } else {
        index::remove_path(&conn, &rel)
    };
    if let Err(e) = result {
        tracing::warn!("watcher reindex {rel}: {e}");
    }
    seq.fetch_add(1, Ordering::Relaxed);
}
