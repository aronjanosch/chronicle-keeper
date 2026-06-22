//! One-way sync: vault pages → Foundry Journal entries. CK owns identity via
//! `.ck/foundry-map.json`, so re-syncs update in place (no duplicates) and
//! removed pages are deleted from Foundry.

use super::{
    body_to_html, read_map, write_map, FoundryClient, FoundryMap, FoundrySettings, MapEntry,
};
use crate::error::AppResult;
use crate::vault;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Map key (not a real vault dir) for the single Foundry folder every synced
/// page lives under — keeps CK's projection in one collapsible/deletable place
/// instead of dumping journals and top-level folders into the Foundry root.
const ROOT_KEY: &str = "\u{0}__ck_root__";

#[derive(Debug, Default, serde::Serialize)]
pub struct SyncReport {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub errors: Vec<String>,
}

/// Pushes every vault page to Foundry. Two passes so `[[wikilinks]]` can resolve
/// to journals created in the same run; stale mapped pages are deleted first.
/// Everything nests under one `root_name` folder (the world's name).
pub async fn sync_world(
    settings: &FoundrySettings,
    world_root: &Path,
    vault_root: &Path,
    root_name: &str,
) -> AppResult<SyncReport> {
    let pages = vault::list_pages(vault_root)?;
    let mut map = read_map(world_root);
    let mut report = SyncReport::default();

    let mut client =
        FoundryClient::connect(&settings.server_url, &settings.user_id, &settings.password).await?;

    let root_id = ensure_root(&mut client, &mut map, root_name).await?;

    // Delete journals for pages that no longer exist in the vault.
    let present: HashSet<&str> = pages.iter().map(|p| p.path.as_str()).collect();
    let stale: Vec<String> = map
        .pages
        .keys()
        .filter(|k| !present.contains(k.as_str()))
        .cloned()
        .collect();
    for path in stale {
        if let Some(entry) = map.pages.get(&path) {
            match client.delete_journal(&entry.journal_id).await {
                Ok(()) => report.deleted += 1,
                Err(e) => report.errors.push(format!("delete {path}: {e}")),
            }
        }
        map.pages.remove(&path);
    }

    // Pass 1: ensure every page has a folder + an (initially empty) journal.
    for p in &pages {
        let folder_id = match ensure_folder(&mut client, &mut map, page_dir(&p.path), &root_id).await
        {
            Ok(id) => id,
            Err(e) => {
                report.errors.push(format!("folder for {}: {e}", p.path));
                None
            }
        };
        if !map.pages.contains_key(&p.path) {
            match client
                .create_journal(&p.title, "", folder_id.as_deref(), &p.path)
                .await
            {
                Ok((journal_id, page_id)) => {
                    map.pages.insert(
                        p.path.clone(),
                        MapEntry {
                            journal_id,
                            page_id,
                        },
                    );
                    report.created += 1;
                }
                Err(e) => report.errors.push(format!("create {}: {e}", p.path)),
            }
        }
    }

    // Link target (page title, lowercased) → journal id, for wikilink rewriting.
    let name_to_jid: HashMap<String, String> = pages
        .iter()
        .filter_map(|p| {
            map.pages
                .get(&p.path)
                .map(|e| (p.title.to_lowercase(), e.journal_id.clone()))
        })
        .collect();
    let resolve = |name: &str| name_to_jid.get(&name.to_lowercase()).cloned();

    // Pass 2: render each page body to HTML and replace the journal page.
    for p in &pages {
        let Some(entry) = map.pages.get(&p.path).cloned() else {
            continue;
        };
        let page = match vault::read_page(vault_root, &p.path) {
            Ok(pg) => pg,
            Err(e) => {
                report.errors.push(format!("read {}: {e}", p.path));
                continue;
            }
        };
        let (_fm, body) = vault::split_frontmatter(&page.content);
        let html = body_to_html(body, &resolve);
        match client
            .update_journal_page(&entry.journal_id, &entry.page_id, &html)
            .await
        {
            Ok(()) => report.updated += 1,
            Err(e) => report.errors.push(format!("update {}: {e}", p.path)),
        }
    }

    client.close().await;
    write_map(world_root, &map)?;
    Ok(report)
}

/// Ensures the single top-level container folder exists (reused across syncs
/// via the map), returning its id.
async fn ensure_root(
    client: &mut FoundryClient,
    map: &mut FoundryMap,
    name: &str,
) -> AppResult<String> {
    if let Some(id) = map.folders.get(ROOT_KEY) {
        return Ok(id.clone());
    }
    let name = if name.trim().is_empty() {
        "Chronicle Keeper"
    } else {
        name.trim()
    };
    let id = client.create_folder(name, None).await?;
    map.folders.insert(ROOT_KEY.to_string(), id.clone());
    Ok(id)
}

/// Ensures the Foundry journal-folder chain for a vault directory exists under
/// `root_id`, creating each missing level, and returns the deepest folder's id.
/// A top-level (empty `dir`) page lands directly in the root container.
async fn ensure_folder(
    client: &mut FoundryClient,
    map: &mut FoundryMap,
    dir: &str,
    root_id: &str,
) -> AppResult<Option<String>> {
    if dir.is_empty() {
        return Ok(Some(root_id.to_string()));
    }
    let mut parent: Option<String> = Some(root_id.to_string());
    let mut cumulative = String::new();
    for comp in dir.split('/') {
        if !cumulative.is_empty() {
            cumulative.push('/');
        }
        cumulative.push_str(comp);
        if let Some(id) = map.folders.get(&cumulative) {
            parent = Some(id.clone());
            continue;
        }
        let id = client.create_folder(comp, parent.as_deref()).await?;
        map.folders.insert(cumulative.clone(), id.clone());
        parent = Some(id);
    }
    Ok(parent)
}

/// Directory portion of a page path (`"A/B/x.md"` → `"A/B"`, top-level → `""`).
fn page_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}
