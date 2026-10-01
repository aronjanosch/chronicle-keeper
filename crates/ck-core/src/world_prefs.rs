//! Per-world UI state that should travel with the folder: pinned pages and
//! dismissed "Unfinished" rows. Lives in `.ck/prefs.json` (unknown keys kept).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const MAX_ITEMS: usize = 200;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldPrefs {
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default)]
    pub gaps_dismissed: Vec<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn file(world_root: &Path) -> PathBuf {
    world_root.join(".ck").join("prefs.json")
}

pub fn read(world_root: &Path) -> WorldPrefs {
    std::fs::read_to_string(file(world_root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn clean(list: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    list.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && seen.insert(s.clone()))
        .take(MAX_ITEMS)
        .collect()
}

pub fn write(world_root: &Path, mut prefs: WorldPrefs) -> AppResult<WorldPrefs> {
    prefs.pinned = clean(prefs.pinned);
    prefs.gaps_dismissed = clean(prefs.gaps_dismissed);
    let dir = world_root.join(".ck");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create .ck: {e}")))?;
    let json = serde_json::to_string_pretty(&prefs)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize prefs: {e}")))?;
    let tmp = dir.join("prefs.json.tmp");
    std::fs::write(&tmp, json)
        .and_then(|()| std::fs::rename(&tmp, file(world_root)))
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write prefs: {e}")))?;
    Ok(prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_dedupes_and_keeps_unknown_keys() {
        let root = std::env::temp_dir().join(format!("ck-prefs-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        assert_eq!(read(&root), WorldPrefs::default());
        std::fs::create_dir_all(root.join(".ck")).unwrap();
        std::fs::write(file(&root), r#"{"future":{"a":1}}"#).unwrap();
        let mut p = read(&root);
        p.pinned = vec!["A.md".into(), " A.md ".into(), "".into(), "B.md".into()];
        p.gaps_dismissed = vec!["page:X.md".into()];
        let w = write(&root, p).unwrap();
        assert_eq!(w.pinned, vec!["A.md", "B.md"]);
        let back = read(&root);
        assert_eq!(back.pinned, w.pinned);
        assert_eq!(back.gaps_dismissed, vec!["page:X.md"]);
        assert!(back.extra.contains_key("future"));
        std::fs::remove_dir_all(&root).ok();
    }
}
