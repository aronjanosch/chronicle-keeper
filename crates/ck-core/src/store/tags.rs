//! Campaign tag vocabulary. Tags have no table of their own — they live as the
//! `tags` list inside each session's `session.toml [metadata]` (files are
//! truth). This module is the single place that reads, normalizes and rewrites
//! them across a campaign so the vocabulary stays one consistent set instead
//! of a per-session free-for-all:
//!
//! - the summarizer injects [`distinct_tags`] into the metadata prompt (reuse,
//!   don't reinvent),
//! - new extractions fold through [`merge_into`] using the campaign [`vocab`]
//!   (a `combat` collapses onto an existing `Kampf`),
//! - the tag-manager UI calls [`rename`] / [`delete`] to merge or drop a tag
//!   across every session at once.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde_json::Value;

use crate::error::AppResult;
use crate::session_files;
use crate::store::{campaigns, sessions};

/// Normalize one tag: trim and collapse internal whitespace. Blank → None.
pub fn normalize_tag(raw: &str) -> Option<String> {
    let t = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!t.is_empty()).then_some(t)
}

/// `(tag, count)` across a campaign's sessions, most-used first then
/// alphabetical. Case/space variants are folded to their first-seen spelling so
/// legacy rows don't show up as near-duplicates.
pub fn tag_counts(conn: &Connection, campaign_id: &str) -> AppResult<Vec<(String, usize)>> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut spelling: HashMap<String, String> = HashMap::new();
    for tags in session_tag_lists(conn, campaign_id)? {
        for raw in tags {
            let Some(tag) = normalize_tag(&raw) else {
                continue;
            };
            let key = tag.to_lowercase();
            *counts.entry(key.clone()).or_insert(0) += 1;
            spelling.entry(key).or_insert(tag);
        }
    }
    let mut out: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(k, n)| (spelling.remove(&k).unwrap_or(k), n))
        .collect();
    out.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    Ok(out)
}

/// Distinct tags only, in the same order as [`tag_counts`] — the tag library fed
/// into the metadata prompt.
pub fn distinct_tags(conn: &Connection, campaign_id: &str) -> AppResult<Vec<String>> {
    Ok(tag_counts(conn, campaign_id)?
        .into_iter()
        .map(|(t, _)| t)
        .collect())
}

/// `lowercase → canonical spelling` map for write-time folding, so a freshly
/// extracted `combat` is stored as the campaign's established `Kampf`.
pub fn vocab(conn: &Connection, campaign_id: &str) -> AppResult<HashMap<String, String>> {
    Ok(tag_counts(conn, campaign_id)?
        .into_iter()
        .map(|(t, _)| (t.to_lowercase(), t))
        .collect())
}

/// Fold `incoming` tags into a session's `existing` tags: normalize, map
/// case-variants onto the established campaign spelling, dedupe
/// case-insensitively. Existing tags keep their order; new ones append.
pub fn merge_into(
    existing: &[Value],
    incoming: &[Value],
    vocab: &HashMap<String, String>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for v in existing.iter().chain(incoming.iter()) {
        let Some(raw) = v.as_str() else { continue };
        let Some(norm) = normalize_tag(raw) else {
            continue;
        };
        let key = norm.to_lowercase();
        let spelled = vocab.get(&key).cloned().unwrap_or(norm);
        if seen.insert(spelled.to_lowercase()) {
            out.push(spelled);
        }
    }
    out
}

/// Rename a tag across every session in the campaign (case-insensitive match),
/// merging onto `to` where a session already carries it. Returns the number of
/// sessions changed. A blank `to` is treated as a delete.
pub fn rename(conn: &Connection, campaign_id: &str, from: &str, to: &str) -> AppResult<usize> {
    let Some(from) = normalize_tag(from) else {
        return Ok(0);
    };
    let to = normalize_tag(to);
    let from_key = from.to_lowercase();
    rewrite_each(conn, campaign_id, |tags| {
        let mut out: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for t in tags {
            let replacement = if t.to_lowercase() == from_key {
                match &to {
                    Some(to) => to.clone(),
                    None => continue, // rename-to-blank == drop
                }
            } else {
                t.clone()
            };
            if seen.insert(replacement.to_lowercase()) {
                out.push(replacement);
            }
        }
        out
    })
}

/// Remove a tag from every session in the campaign (case-insensitive). Returns
/// the number of sessions changed.
pub fn delete(conn: &Connection, campaign_id: &str, tag: &str) -> AppResult<usize> {
    let Some(tag) = normalize_tag(tag) else {
        return Ok(0);
    };
    let key = tag.to_lowercase();
    rewrite_each(conn, campaign_id, |tags| {
        tags.iter()
            .filter(|t| t.to_lowercase() != key)
            .cloned()
            .collect()
    })
}

// --- internals ---------------------------------------------------------------

fn world_session_dirs(conn: &Connection, campaign_id: &str) -> AppResult<Vec<std::path::PathBuf>> {
    Ok(campaigns::world_root_for_id(conn, campaign_id)?
        .map(|root| sessions::session_dirs(&root))
        .unwrap_or_default())
}

fn session_tag_lists(conn: &Connection, campaign_id: &str) -> AppResult<Vec<Vec<String>>> {
    Ok(world_session_dirs(conn, campaign_id)?
        .iter()
        .filter_map(|dir| session_files::read_session_toml(dir).ok().flatten())
        .map(|st| st.metadata.tags)
        .collect())
}

/// Apply `f` to each session's normalized tag list; persist (session.toml)
/// only the sessions whose tags actually changed. Returns sessions changed.
fn rewrite_each(
    conn: &Connection,
    campaign_id: &str,
    f: impl Fn(&[String]) -> Vec<String>,
) -> AppResult<usize> {
    let mut changed = 0;
    for dir in world_session_dirs(conn, campaign_id)? {
        let Some(mut st) = session_files::read_session_toml(&dir).ok().flatten() else {
            continue;
        };
        let old: Vec<String> = st
            .metadata
            .tags
            .iter()
            .filter_map(|t| normalize_tag(t))
            .collect();
        let new = f(&old);
        if new == old {
            continue;
        }
        st.metadata.tags = new;
        if session_files::write_session_toml_file(&dir, &st).is_ok() {
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_files::{SessionMetadata, SessionToml};
    use crate::store::campaigns;
    use serde_json::json;

    fn world(tag: &str) -> (Connection, std::path::PathBuf) {
        let conn = crate::db::open_in_memory().unwrap();
        let tmp = std::env::temp_dir().join(format!("ck-tags-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).unwrap();
        crate::config::set_value(&conn, "output_root", &tmp.to_string_lossy()).unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp", 1, None, false, false).unwrap();
        (conn, tmp)
    }
    fn add_session(conn: &Connection, n: i64, tags: &[&str]) {
        crate::store::sessions::create_campaign_session(conn, "c1", Some(n), None, None).unwrap();
        let root = campaigns::world_root_for_id(conn, "c1").unwrap().unwrap();
        let dir = root
            .join("Sessions")
            .join(crate::session_files::padded_number(n));
        let mut st: SessionToml = crate::session_files::read_session_toml(&dir)
            .unwrap()
            .unwrap();
        st.metadata = SessionMetadata {
            tags: tags.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        crate::session_files::write_session_toml_file(&dir, &st).unwrap();
    }
    fn tags_for(conn: &Connection, n: i64) -> Vec<String> {
        let root = campaigns::world_root_for_id(conn, "c1").unwrap().unwrap();
        let dir = root
            .join("Sessions")
            .join(crate::session_files::padded_number(n));
        crate::session_files::read_session_toml(&dir)
            .unwrap()
            .unwrap()
            .metadata
            .tags
    }

    #[test]
    fn counts_fold_case_and_space_variants() {
        let (conn, tmp) = world("counts");
        add_session(&conn, 1, &["Kampf", "Mysterium"]);
        add_session(&conn, 2, &["kampf", " Mysterium "]);
        let counts = tag_counts(&conn, "c1").unwrap();
        assert_eq!(counts, vec![("Kampf".into(), 2), ("Mysterium".into(), 2)]);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn merge_into_folds_to_existing_spelling() {
        let mut v = HashMap::new();
        v.insert("kampf".into(), "Kampf".into());
        let existing = vec![json!("Kampf")];
        let incoming = vec![json!("combat"), json!("KAMPF"), json!("Mysterium")];
        // 'KAMPF' dedupes onto existing 'Kampf'; 'combat' has no vocab entry so
        // it stays as-is (case-insensitive dedupe still applies).
        let out = merge_into(&existing, &incoming, &v);
        assert_eq!(out, vec!["Kampf", "combat", "Mysterium"]);
    }

    #[test]
    fn rename_merges_across_sessions() {
        let (conn, tmp) = world("rename");
        add_session(&conn, 1, &["Combat", "Mystery"]);
        add_session(&conn, 2, &["Kampf"]);
        let n = rename(&conn, "c1", "Combat", "Kampf").unwrap();
        assert_eq!(n, 1, "only session 1 changed");
        assert_eq!(tags_for(&conn, 1), vec!["Kampf", "Mystery"]);
        assert_eq!(tags_for(&conn, 2), vec!["Kampf"]);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn rename_dedupes_when_target_present() {
        let (conn, tmp) = world("dedupe");
        add_session(&conn, 1, &["Combat", "Kampf"]);
        rename(&conn, "c1", "Combat", "Kampf").unwrap();
        assert_eq!(tags_for(&conn, 1), vec!["Kampf"]);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn delete_removes_across_sessions() {
        let (conn, tmp) = world("delete");
        add_session(&conn, 1, &["ttrpg", "Kampf"]);
        add_session(&conn, 2, &["TTRPG"]);
        let n = delete(&conn, "c1", "ttrpg").unwrap();
        assert_eq!(n, 2);
        assert_eq!(tags_for(&conn, 1), vec!["Kampf"]);
        assert_eq!(tags_for(&conn, 2), Vec::<String>::new());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
