//! World trash (`.ck/trash/`): soft delete for vault pages and folders.
//! Each delete is one group `<millis>/` holding `meta.json` + the moved files
//! under `files/<vault-rel>`. Restore moves them back (collisions suffixed);
//! groups older than [`PRUNE_DAYS`] are dropped on every list/delete.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const PRUNE_DAYS: u64 = 30;

#[derive(Serialize, Deserialize, Clone)]
pub struct TrashItem {
    /// Vault-relative path of the deleted page or folder.
    pub rel: String,
    /// "page" | "folder"
    pub kind: String,
    /// Pages inside a deleted folder (1 for a page).
    pub pages: usize,
}

#[derive(Serialize, Deserialize)]
pub struct TrashGroup {
    pub id: String,
    /// Unix seconds.
    pub deleted_at: u64,
    pub items: Vec<TrashItem>,
}

fn trash_dir(world_root: &Path) -> PathBuf {
    world_root.join(".ck").join("trash")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Traversal guard for vault-relative paths coming from HTTP/tools.
fn safe_rel(rel: &str) -> AppResult<PathBuf> {
    let p = Path::new(rel);
    let mut any = false;
    for c in p.components() {
        match c {
            Component::Normal(s) if !s.to_string_lossy().starts_with('.') => any = true,
            _ => return Err(AppError::BadRequest("invalid path".into())),
        }
    }
    if !any {
        return Err(AppError::BadRequest("empty path".into()));
    }
    Ok(p.to_path_buf())
}

fn io_err(what: &str) -> impl Fn(std::io::Error) -> AppError + '_ {
    move |e| AppError::Internal(anyhow::anyhow!("{what}: {e}"))
}

fn count_pages_under(path: &Path) -> usize {
    if path.is_file() {
        return usize::from(path.extension().and_then(|e| e.to_str()) == Some("md"));
    }
    let Ok(rd) = std::fs::read_dir(path) else {
        return 0;
    };
    rd.flatten().map(|e| count_pages_under(&e.path())).sum()
}

/// Move pages/folders (`(rel, is_folder)`) into one new trash group.
/// Returns the group id. All-or-nothing is not guaranteed — already-moved
/// items stay in the trash if a later one fails (restorable either way).
pub fn trash_paths(
    world_root: &Path,
    vault_root: &Path,
    items: &[(String, bool)],
) -> AppResult<String> {
    if items.is_empty() {
        return Err(AppError::BadRequest("nothing to delete".into()));
    }
    let dir = trash_dir(world_root);
    let mut id = format!("{}", now_secs() * 1000);
    let mut n = 1;
    while dir.join(&id).exists() {
        n += 1;
        id = format!("{}-{n}", now_secs() * 1000);
    }
    let files = dir.join(&id).join("files");
    std::fs::create_dir_all(&files).map_err(io_err("create trash group"))?;

    let mut meta_items = Vec::new();
    for (rel, is_folder) in items {
        let rel_path = safe_rel(rel)?;
        let src = vault_root.join(&rel_path);
        let ok = if *is_folder { src.is_dir() } else { src.is_file() };
        if !ok {
            return Err(AppError::NotFound(format!("Not found: {rel}")));
        }
        let pages = count_pages_under(&src);
        let dst = files.join(&rel_path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(io_err("create trash dir"))?;
        }
        std::fs::rename(&src, &dst).map_err(io_err("move to trash"))?;
        meta_items.push(TrashItem {
            rel: rel.clone(),
            kind: if *is_folder { "folder".into() } else { "page".into() },
            pages,
        });
    }
    let meta = TrashGroup { id: id.clone(), deleted_at: now_secs(), items: meta_items };
    let json = serde_json::to_string(&meta)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize trash meta: {e}")))?;
    std::fs::write(dir.join(&id).join("meta.json"), json).map_err(io_err("write trash meta"))?;
    Ok(id)
}

fn read_group(dir: &Path) -> Option<TrashGroup> {
    let raw = std::fs::read_to_string(dir.join("meta.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

/// All trash groups, newest first. Prunes expired groups as a side effect.
pub fn list(world_root: &Path) -> Vec<TrashGroup> {
    prune(world_root, PRUNE_DAYS);
    let Ok(rd) = std::fs::read_dir(trash_dir(world_root)) else {
        return Vec::new();
    };
    let mut groups: Vec<TrashGroup> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| read_group(&e.path()))
        .collect();
    groups.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at).then(b.id.cmp(&a.id)));
    groups
}

fn unique_dest(vault_root: &Path, rel: &Path) -> PathBuf {
    let dst = vault_root.join(rel);
    if !dst.exists() {
        return dst;
    }
    let parent = dst.parent().map(Path::to_path_buf).unwrap_or_else(|| vault_root.to_path_buf());
    let stem = dst.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string();
    let ext = dst
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let mut n = 2;
    loop {
        let cand = parent.join(format!("{stem}-{n}{ext}"));
        if !cand.exists() {
            return cand;
        }
        n += 1;
    }
}

fn collect_md_rels(vault_root: &Path, abs: &Path, out: &mut Vec<String>) {
    if abs.is_file() {
        if abs.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(rel) = abs.strip_prefix(vault_root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        return;
    }
    let Ok(rd) = std::fs::read_dir(abs) else {
        return;
    };
    for e in rd.flatten() {
        collect_md_rels(vault_root, &e.path(), out);
    }
}

/// Move a group's items back into the vault (name collisions get a `-N`
/// suffix). Returns the vault-relative paths of every restored `.md` page,
/// for reindexing.
pub fn restore(world_root: &Path, vault_root: &Path, id: &str) -> AppResult<Vec<String>> {
    let group_dir = trash_dir(world_root).join(safe_rel(id)?);
    let group = read_group(&group_dir)
        .ok_or_else(|| AppError::NotFound(format!("No such trash entry: {id}")))?;
    let mut restored = Vec::new();
    for item in &group.items {
        let rel = safe_rel(&item.rel)?;
        let src = group_dir.join("files").join(&rel);
        if !src.exists() {
            continue;
        }
        let dst = unique_dest(vault_root, &rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(io_err("create restore dir"))?;
        }
        std::fs::rename(&src, &dst).map_err(io_err("restore from trash"))?;
        collect_md_rels(vault_root, &dst, &mut restored);
    }
    let _ = std::fs::remove_dir_all(&group_dir);
    Ok(restored)
}

/// Delete all groups (or one, when `id` is given). Returns groups removed.
pub fn empty(world_root: &Path, id: Option<&str>) -> usize {
    let dir = trash_dir(world_root);
    match id {
        Some(id) => {
            let Ok(rel) = safe_rel(id) else { return 0 };
            usize::from(std::fs::remove_dir_all(dir.join(rel)).is_ok())
        }
        None => {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                return 0;
            };
            rd.flatten()
                .filter(|e| e.path().is_dir())
                .filter(|e| std::fs::remove_dir_all(e.path()).is_ok())
                .count()
        }
    }
}

/// Drop groups older than `days`.
pub fn prune(world_root: &Path, days: u64) {
    let Ok(rd) = std::fs::read_dir(trash_dir(world_root)) else {
        return;
    };
    let cutoff = now_secs().saturating_sub(days * 86_400);
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        match read_group(&p) {
            Some(g) if g.deleted_at >= cutoff => {}
            // Expired or unreadable meta — either way the group is dead weight.
            _ => {
                let _ = std::fs::remove_dir_all(&p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("ck-trash-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let vault = root.join("Codex");
        std::fs::create_dir_all(&vault).unwrap();
        (root, vault)
    }

    #[test]
    fn trash_list_restore_roundtrip() {
        let (root, vault) = tmp("roundtrip");
        std::fs::create_dir_all(vault.join("NPCs")).unwrap();
        std::fs::write(vault.join("NPCs/Aragorn.md"), "# Aragorn\n").unwrap();
        std::fs::write(vault.join("Note.md"), "note\n").unwrap();

        let id = trash_paths(&root, &vault, &[("NPCs/Aragorn.md".into(), false)]).unwrap();
        assert!(!vault.join("NPCs/Aragorn.md").exists());
        let groups = list(&root);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items[0].rel, "NPCs/Aragorn.md");
        assert_eq!(groups[0].items[0].pages, 1);

        let restored = restore(&root, &vault, &id).unwrap();
        assert_eq!(restored, ["NPCs/Aragorn.md"]);
        assert!(vault.join("NPCs/Aragorn.md").is_file());
        assert!(list(&root).is_empty());

        // restore collision → suffix
        let id = trash_paths(&root, &vault, &[("Note.md".into(), false)]).unwrap();
        std::fs::write(vault.join("Note.md"), "new occupant\n").unwrap();
        let restored = restore(&root, &vault, &id).unwrap();
        assert_eq!(restored, ["Note-2.md"]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn folder_trash_and_bulk_group() {
        let (root, vault) = tmp("folder");
        std::fs::create_dir_all(vault.join("Old/Deep")).unwrap();
        std::fs::write(vault.join("Old/A.md"), "a\n").unwrap();
        std::fs::write(vault.join("Old/Deep/B.md"), "b\n").unwrap();
        std::fs::write(vault.join("C.md"), "c\n").unwrap();

        let id =
            trash_paths(&root, &vault, &[("Old".into(), true), ("C.md".into(), false)]).unwrap();
        assert!(!vault.join("Old").exists());
        let g = &list(&root)[0];
        assert_eq!(g.items.len(), 2);
        assert_eq!(g.items[0].pages, 2);

        let mut restored = restore(&root, &vault, &id).unwrap();
        restored.sort();
        assert_eq!(restored, ["C.md", "Old/A.md", "Old/Deep/B.md"]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn empty_and_prune() {
        let (root, vault) = tmp("empty");
        std::fs::write(vault.join("A.md"), "a\n").unwrap();
        std::fs::write(vault.join("B.md"), "b\n").unwrap();
        trash_paths(&root, &vault, &[("A.md".into(), false)]).unwrap();
        let id2 = trash_paths(&root, &vault, &[("B.md".into(), false)]).unwrap();
        assert_eq!(empty(&root, Some(&id2)), 1);
        assert_eq!(list(&root).len(), 1);
        assert_eq!(empty(&root, None), 1);
        assert!(list(&root).is_empty());

        // traversal guards
        assert!(trash_paths(&root, &vault, &[("../escape.md".into(), false)]).is_err());
        assert!(restore(&root, &vault, "../oops").is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}
