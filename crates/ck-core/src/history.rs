//! Page version history (`.ck/history/`): a snapshot of the page taken
//! *before* every save, tagged with who saved (`user` | `keeper`). The diff of
//! version N against version N+1 (or the live file) is therefore "what N's
//! origin changed". One JSON file per version under
//! `.ck/history/<vault-rel>/<millis>-<origin>.json`; capped per page; rapid
//! user saves (editor autosave) coalesce into one version.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const MAX_PER_PAGE: usize = 50;
/// Successive user saves within this window keep only the oldest pre-state.
const COALESCE_SECS: u64 = 300;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    /// File content before the save; `None` = the save created the page.
    content: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VersionMeta {
    /// Unix milliseconds (also the filename key).
    pub ts: u64,
    /// "user" | "keeper"
    pub origin: String,
}

fn history_root(world_root: &Path) -> PathBuf {
    world_root.join(".ck").join("history")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

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

fn page_dir(world_root: &Path, rel: &str) -> AppResult<PathBuf> {
    Ok(history_root(world_root).join(safe_rel(rel)?))
}

fn parse_name(path: &Path) -> Option<VersionMeta> {
    let stem = path.file_stem()?.to_str()?;
    let (ts, origin) = stem.split_once('-')?;
    Some(VersionMeta { ts: ts.parse().ok()?, origin: origin.to_string() })
}

fn versions_in(dir: &Path) -> Vec<(PathBuf, VersionMeta)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, VersionMeta)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .filter_map(|p| parse_name(&p).map(|m| (p, m)))
        .collect();
    out.sort_by_key(|(_, m)| m.ts);
    out
}

fn read_snapshot(path: &Path) -> Option<Option<String>> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<Snapshot>(&raw).ok().map(|s| s.content)
}

/// Snapshot `rel`'s current state before a save by `origin`. Must run before
/// the file is touched. No-ops when nothing changed since the last version, or
/// when coalescing a rapid user save train.
pub fn record(world_root: &Path, vault_root: &Path, rel: &str, origin: &str) -> AppResult<()> {
    record_inner(world_root, vault_root, rel, origin, true)
}

/// As [`record`], but never coalesced — for saves that must be individually
/// restorable (history restore, bulk edits).
pub fn record_now(world_root: &Path, vault_root: &Path, rel: &str, origin: &str) -> AppResult<()> {
    record_inner(world_root, vault_root, rel, origin, false)
}

/// Mark a page's creation: a "did not exist" snapshot, written right after the
/// create (when the pre-state is known to be absent).
pub fn record_create(world_root: &Path, rel: &str, origin: &str) -> AppResult<()> {
    let dir = page_dir(world_root, rel)?;
    if !versions_in(&dir).is_empty() {
        return Ok(()); // page has history already (e.g. restored after delete)
    }
    write_version(&dir, &[], origin, None)
}

fn record_inner(
    world_root: &Path,
    vault_root: &Path,
    rel: &str,
    origin: &str,
    coalesce: bool,
) -> AppResult<()> {
    let dir = page_dir(world_root, rel)?;
    let current = crate::vault::read_page(vault_root, rel).ok().map(|p| p.content);
    let versions = versions_in(&dir);
    if let Some((last_path, last)) = versions.last() {
        if read_snapshot(last_path).as_ref() == Some(&current) {
            return Ok(());
        }
        if coalesce
            && origin == "user"
            && last.origin == "user"
            && now_ms().saturating_sub(last.ts) < COALESCE_SECS * 1000
        {
            return Ok(());
        }
    }
    write_version(&dir, &versions, origin, current)
}

fn write_version(
    dir: &Path,
    versions: &[(PathBuf, VersionMeta)],
    origin: &str,
    content: Option<String>,
) -> AppResult<()> {
    std::fs::create_dir_all(dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create history dir: {e}")))?;
    for (old, _) in versions.iter().take((versions.len() + 1).saturating_sub(MAX_PER_PAGE)) {
        let _ = std::fs::remove_file(old);
    }
    let mut ts = now_ms();
    if let Some((_, last)) = versions.last() {
        ts = ts.max(last.ts + 1); // keep filenames strictly ordered
    }
    let json = serde_json::to_string(&Snapshot { content })
        .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize snapshot: {e}")))?;
    std::fs::write(dir.join(format!("{ts}-{origin}.json")), json)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write snapshot: {e}")))
}

/// Keep history attached across renames/moves (pages and folders alike).
pub fn move_history(world_root: &Path, from: &str, to: &str) {
    let (Ok(from_rel), Ok(to_rel)) = (safe_rel(from), safe_rel(to)) else {
        return;
    };
    let src = history_root(world_root).join(from_rel);
    if !src.exists() {
        return;
    }
    let dst = history_root(world_root).join(to_rel);
    if dst.exists() {
        return; // destination already has its own history — leave both alone
    }
    if let Some(parent) = dst.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::rename(&src, &dst);
}

/// Versions for one page, oldest first.
pub fn list_page(world_root: &Path, rel: &str) -> AppResult<Vec<VersionMeta>> {
    Ok(versions_in(&page_dir(world_root, rel)?).into_iter().map(|(_, m)| m).collect())
}

/// One version's snapshot. `content: None` means the page did not exist yet.
pub fn read_version(
    world_root: &Path,
    rel: &str,
    ts: u64,
) -> AppResult<(VersionMeta, Option<String>)> {
    let dir = page_dir(world_root, rel)?;
    let (path, meta) = versions_in(&dir)
        .into_iter()
        .find(|(_, m)| m.ts == ts)
        .ok_or_else(|| AppError::NotFound(format!("No version {ts} for {rel}")))?;
    let content = read_snapshot(&path)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("unreadable snapshot")))?;
    Ok((meta, content))
}

#[derive(Serialize)]
pub struct RecentVersion {
    pub path: String,
    pub ts: u64,
    pub origin: String,
}

/// Newest versions across the whole world (the "everything the Keeper
/// changed" feed), filtered by origin when given.
pub fn recent(world_root: &Path, origin: Option<&str>, limit: usize) -> Vec<RecentVersion> {
    let root = history_root(world_root);
    let mut out = Vec::new();
    fn walk(root: &Path, dir: &Path, origin: Option<&str>, out: &mut Vec<RecentVersion>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, origin, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(m) = parse_name(&p) {
                    if origin.is_none_or(|o| o == m.origin) {
                        let rel = p
                            .parent()
                            .and_then(|d| d.strip_prefix(root).ok())
                            .map(|r| r.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_default();
                        out.push(RecentVersion { path: rel, ts: m.ts, origin: m.origin });
                    }
                }
            }
        }
    }
    walk(&root, &root, origin, &mut out);
    out.sort_by(|a, b| b.ts.cmp(&a.ts));
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("ck-hist-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let vault = root.join("Codex");
        std::fs::create_dir_all(&vault).unwrap();
        (root, vault)
    }

    #[test]
    fn record_list_read_and_dedupe() {
        let (root, vault) = tmp("basic");
        // create: pre-state is "did not exist"
        record(&root, &vault, "A.md", "user").unwrap();
        std::fs::write(vault.join("A.md"), "v1\n").unwrap();
        // keeper edit: snapshots v1
        record(&root, &vault, "A.md", "keeper").unwrap();
        std::fs::write(vault.join("A.md"), "v2\n").unwrap();
        // unchanged content → no new version even from another origin
        std::fs::write(vault.join("A.md"), "v1\n").unwrap();
        record(&root, &vault, "A.md", "keeper").unwrap();

        let versions = list_page(&root, "A.md").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].origin, "user");
        assert_eq!(versions[1].origin, "keeper");
        let (_, content) = read_version(&root, "A.md", versions[0].ts).unwrap();
        assert_eq!(content, None);
        let (_, content) = read_version(&root, "A.md", versions[1].ts).unwrap();
        assert_eq!(content.as_deref(), Some("v1\n"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn user_saves_coalesce_keeper_saves_dont() {
        let (root, vault) = tmp("coalesce");
        std::fs::write(vault.join("A.md"), "v1\n").unwrap();
        record(&root, &vault, "A.md", "user").unwrap();
        std::fs::write(vault.join("A.md"), "v2\n").unwrap();
        record(&root, &vault, "A.md", "user").unwrap(); // within window → skipped
        std::fs::write(vault.join("A.md"), "v3\n").unwrap();
        record(&root, &vault, "A.md", "keeper").unwrap(); // keeper never coalesces
        let versions = list_page(&root, "A.md").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[1].origin, "keeper");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn move_recent_and_guards() {
        let (root, vault) = tmp("move");
        std::fs::create_dir_all(vault.join("NPCs")).unwrap();
        std::fs::write(vault.join("NPCs/Aragorn.md"), "v1\n").unwrap();
        record(&root, &vault, "NPCs/Aragorn.md", "keeper").unwrap();

        move_history(&root, "NPCs/Aragorn.md", "NPCs/Strider.md");
        assert!(list_page(&root, "NPCs/Aragorn.md").unwrap().is_empty());
        assert_eq!(list_page(&root, "NPCs/Strider.md").unwrap().len(), 1);
        // folder-level move
        move_history(&root, "NPCs", "People");
        assert_eq!(list_page(&root, "People/Strider.md").unwrap().len(), 1);

        let recent_all = recent(&root, None, 10);
        assert_eq!(recent_all.len(), 1);
        assert_eq!(recent_all[0].path, "People/Strider.md");
        assert!(recent(&root, Some("user"), 10).is_empty());
        assert_eq!(recent(&root, Some("keeper"), 10).len(), 1);

        assert!(record(&root, &vault, "../escape.md", "user").is_err());
        assert!(list_page(&root, "../escape.md").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cap_drops_oldest() {
        let (root, vault) = tmp("cap");
        for i in 0..(MAX_PER_PAGE + 5) {
            std::fs::write(vault.join("P.md"), format!("v{i}\n")).unwrap();
            record(&root, &vault, "P.md", "keeper").unwrap();
        }
        let versions = list_page(&root, "P.md").unwrap();
        assert_eq!(versions.len(), MAX_PER_PAGE);
        let (_, content) = read_version(&root, "P.md", versions[0].ts).unwrap();
        assert_eq!(content.as_deref(), Some("v5\n"));
        std::fs::remove_dir_all(&root).ok();
    }
}
