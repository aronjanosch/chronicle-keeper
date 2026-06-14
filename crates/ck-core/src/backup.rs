//! World snapshot backups: zip the world folder into `<world>/Backups/`,
//! keep the newest [`KEEP`] zips. Excludes the rebuildable index cache,
//! session audio (often GBs — the recordings stay on disk either way) and
//! the Backups folder itself. Complements page history at world granularity.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

const KEEP: usize = 10;
const BACKUPS_DIR: &str = "Backups";

fn is_excluded(rel: &str) -> bool {
    if rel.starts_with(&format!("{BACKUPS_DIR}/")) || rel.starts_with(".ck/index.db") {
        return true;
    }
    let mut parts = rel.split('/');
    parts.next() == Some("Sessions") && parts.next().is_some() && parts.next() == Some("audio")
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Zip the world into `Backups/<name>-<stamp>.zip`, prune to the newest
/// [`KEEP`]. Returns the zip path.
pub fn backup_world(world_root: &Path) -> AppResult<String> {
    let name = world_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("world")
        .replace(['/', '\\', ':'], "_");
    let dir = world_root.join(BACKUPS_DIR);
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create Backups/: {e}")))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut out_path = dir.join(format!("{name}-{stamp}.zip"));
    let mut n = 2;
    while out_path.exists() {
        out_path = dir.join(format!("{name}-{stamp}-{n}.zip"));
        n += 1;
    }

    let mut files = Vec::new();
    collect_files(world_root, &mut files);

    let file = std::fs::File::create(&out_path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create backup zip: {e}")))?;
    let mut w = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);

    for abs in files {
        let rel = abs
            .strip_prefix(world_root)
            .unwrap_or(&abs)
            .to_string_lossy()
            .replace('\\', "/");
        if is_excluded(&rel) {
            continue;
        }
        let bytes = std::fs::read(&abs)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("read {rel}: {e}")))?;
        w.start_file(rel.clone(), opts)
            .and_then(|()| w.write_all(&bytes).map_err(Into::into))
            .map_err(|e| AppError::Internal(anyhow::anyhow!("zip {rel}: {e}")))?;
    }
    w.finish()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("finish backup zip: {e}")))?;
    prune(&dir);
    Ok(out_path.to_string_lossy().into_owned())
}

// The timestamp in the filename sorts lexicographically — newest last.
fn prune(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut zips: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("zip"))
        .collect();
    zips.sort();
    for old in zips.iter().take(zips.len().saturating_sub(KEEP)) {
        let _ = std::fs::remove_file(old);
    }
}

/// Back up every world touched this session (its index was opened). Called by
/// the Tauri shell on app close; failures are logged, never fatal.
pub fn backup_open_worlds(state: &crate::state::AppState) {
    let vaults: Vec<PathBuf> = {
        let map = state.indexes.lock().unwrap_or_else(|e| e.into_inner());
        map.keys().cloned().collect()
    };
    let mut seen = std::collections::HashSet::new();
    for vault in vaults {
        let Some(world_root) = crate::vault::world_root_of(&vault) else {
            continue;
        };
        if !seen.insert(world_root.clone()) {
            continue;
        }
        match backup_world(&world_root) {
            Ok(path) => tracing::info!("world backed up on close: {path}"),
            Err(e) => tracing::warn!("backup failed for {}: {e}", world_root.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_excludes_cache_audio_and_self_then_prunes() {
        let dir = std::env::temp_dir().join(format!("ck-backup-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let world = dir.join("My World");
        std::fs::create_dir_all(world.join(".ck")).unwrap();
        std::fs::create_dir_all(world.join("Codex")).unwrap();
        std::fs::create_dir_all(world.join("Sessions/001/audio")).unwrap();
        std::fs::write(world.join(".ck/config.toml"), "id = \"w\"").unwrap();
        std::fs::write(world.join(".ck/index.db"), "cache").unwrap();
        std::fs::write(world.join("Codex/Page.md"), "hello").unwrap();
        std::fs::write(world.join("Sessions/001/audio/track.flac"), "audio").unwrap();

        let path = backup_world(&world).unwrap();
        assert!(path.contains("Backups"));
        let f = std::fs::File::open(&path).unwrap();
        let mut z = zip::ZipArchive::new(f).unwrap();
        let names: Vec<String> = (0..z.len())
            .map(|i| z.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&".ck/config.toml".to_string()));
        assert!(names.contains(&"Codex/Page.md".to_string()));
        assert!(!names.iter().any(|n| n.contains("index.db")));
        assert!(!names.iter().any(|n| n.contains("/audio/")));
        assert!(!names.iter().any(|n| n.starts_with("Backups/")));

        // a second backup must not swallow the first into itself, and pruning
        // keeps the newest KEEP
        for _ in 0..(KEEP + 2) {
            backup_world(&world).unwrap();
        }
        let count = std::fs::read_dir(world.join("Backups"))
            .unwrap()
            .flatten()
            .count();
        assert_eq!(count, KEEP);
        std::fs::remove_dir_all(&dir).ok();
    }
}
