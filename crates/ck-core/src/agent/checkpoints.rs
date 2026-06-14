//! Undo for AI writes: before each write-tier dispatch the target file is
//! snapshotted to `.ck/checkpoints/<chat-id>/<seq>.json`. Undo restores in
//! reverse order through the normal vault write path. Cap 50 per chat
//! (oldest dropped); the whole folder dies with the chat.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::vault;

const MAX_PER_CHAT: usize = 50;

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    /// Vault-relative page path.
    path: String,
    /// File content before the write; `None` = file did not exist (a create).
    content: Option<String>,
}

pub fn dir_for(world_root: &Path, chat_id: &str) -> PathBuf {
    world_root.join(".ck").join("checkpoints").join(chat_id)
}

fn entries_sorted(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    files.sort();
    files
}

/// Snapshot `rel` before a write. Must run before the file is touched.
pub fn record(world_root: &Path, chat_id: &str, vault_root: &Path, rel: &str) -> AppResult<()> {
    let dir = dir_for(world_root, chat_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create checkpoints dir: {e}")))?;
    let existing = entries_sorted(&dir);
    for old in existing
        .iter()
        .take((existing.len() + 1).saturating_sub(MAX_PER_CHAT))
    {
        let _ = std::fs::remove_file(old);
    }
    let seq = existing
        .last()
        .and_then(|p| p.file_stem()?.to_str()?.parse::<u64>().ok())
        .map_or(1, |n| n + 1);
    let content = vault::read_page(vault_root, rel).ok().map(|p| p.content);
    let cp = Checkpoint {
        path: rel.to_string(),
        content,
    };
    let json = serde_json::to_string(&cp)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize checkpoint: {e}")))?;
    std::fs::write(dir.join(format!("{seq:05}.json")), json)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write checkpoint: {e}")))
}

pub fn count(world_root: &Path, chat_id: &str) -> usize {
    entries_sorted(&dir_for(world_root, chat_id)).len()
}

/// Restore checkpoints in reverse order. `last` = newest only, `all` = every
/// one. Returns the vault-relative paths touched (for reindexing).
pub fn undo(
    world_root: &Path,
    chat_id: &str,
    vault_root: &Path,
    all: bool,
) -> AppResult<Vec<String>> {
    let dir = dir_for(world_root, chat_id);
    let mut files = entries_sorted(&dir);
    files.reverse();
    if !all {
        files.truncate(1);
    }
    let mut restored = Vec::new();
    for file in files {
        let raw = std::fs::read_to_string(&file)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("read checkpoint: {e}")))?;
        let cp: Checkpoint = serde_json::from_str(&raw)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("parse checkpoint: {e}")))?;
        match &cp.content {
            Some(content) => {
                vault::write_page(vault_root, &cp.path, content)?;
            }
            None => match vault::delete_page(vault_root, &cp.path) {
                Ok(()) | Err(AppError::NotFound(_)) => {}
                Err(e) => return Err(e),
            },
        }
        let _ = std::fs::remove_file(&file);
        restored.push(cp.path);
    }
    Ok(restored)
}

pub fn delete_for_chat(world_root: &Path, chat_id: &str) {
    let _ = std::fs::remove_dir_all(dir_for(world_root, chat_id));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-cp-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("Codex")).unwrap();
        dir
    }

    #[test]
    fn undo_restores_edit_and_removes_create() {
        let root = tmp("undo");
        let vault = root.join("Codex");
        std::fs::write(vault.join("A.md"), "old body\n").unwrap();

        record(&root, "chat1", &vault, "A.md").unwrap();
        std::fs::write(vault.join("A.md"), "new body\n").unwrap();
        record(&root, "chat1", &vault, "B.md").unwrap(); // create: no prior file
        std::fs::write(vault.join("B.md"), "fresh\n").unwrap();
        assert_eq!(count(&root, "chat1"), 2);

        let restored = undo(&root, "chat1", &vault, false).unwrap();
        assert_eq!(restored, ["B.md"]);
        assert!(!vault.join("B.md").exists());

        let restored = undo(&root, "chat1", &vault, true).unwrap();
        assert_eq!(restored, ["A.md"]);
        assert_eq!(
            std::fs::read_to_string(vault.join("A.md")).unwrap(),
            "old body\n"
        );
        assert_eq!(count(&root, "chat1"), 0);

        delete_for_chat(&root, "chat1");
        assert!(!dir_for(&root, "chat1").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cap_drops_oldest() {
        let root = tmp("cap");
        let vault = root.join("Codex");
        for i in 0..(MAX_PER_CHAT + 5) {
            std::fs::write(vault.join("P.md"), format!("v{i}\n")).unwrap();
            record(&root, "c", &vault, "P.md").unwrap();
        }
        assert_eq!(count(&root, "c"), MAX_PER_CHAT);
        // Oldest snapshots (v0..v4) dropped; undoing everything lands on v5.
        undo(&root, "c", &vault, true).unwrap();
        assert_eq!(std::fs::read_to_string(vault.join("P.md")).unwrap(), "v5\n");
        std::fs::remove_dir_all(&root).ok();
    }
}
