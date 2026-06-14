//! Vault page endpoints (files-as-truth).

use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::campaigns;
use crate::{history, trash, vault};

pub(super) fn vault_root(state: &AppState, campaign_id: &str) -> AppResult<PathBuf> {
    let path = state
        .with_db(|conn| campaigns::get_campaign(conn, campaign_id))?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?
        .vault_path;
    match path {
        Some(p) => Ok(PathBuf::from(p)),
        None => Err(AppError::BadRequest(
            "This campaign has no vault folder attached".into(),
        )),
    }
}

// Best-effort incremental reindex after a CK-side mutation. The index is a
// cache — a failure here must never fail the write itself. Also records the
// write in the suppress map so the watcher drops its echo.
fn reindex_page(state: &AppState, root: &std::path::Path, rel: &str) {
    state.note_vault_write(root, rel);
    let _ = state.with_index(root, |conn| {
        let _ = crate::store::index::upsert_path(conn, root, rel);
    });
}

fn reindex_remove(state: &AppState, root: &std::path::Path, rel: &str) {
    let _ = state.with_index(root, |conn| {
        let _ = crate::store::index::remove_path(conn, rel);
    });
}

fn reindex_all(state: &AppState, root: &std::path::Path) {
    let _ = state.with_index(root, |conn| {
        let _ = crate::store::index::rebuild(conn, root);
    });
}

#[derive(Deserialize)]
pub struct AttachRequest {
    pub path: Option<String>,
}

// Files-as-truth relocate: the given folder becomes this world's root. A
// folder that already is a world must carry the same id; a bare folder is
// adopted in place (additive artifacts only — a foreign vault of .md pages
// gets `codex_root = "."`). The old folder is left untouched on disk — the
// registry simply points elsewhere now.
pub async fn attach(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<AttachRequest>,
) -> AppResult<Json<Value>> {
    let path = req.path.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let Some(p) = path else {
        return Err(AppError::BadRequest("A folder path is required".into()));
    };
    let new_root = std::path::PathBuf::from(p);
    if let Ok(old) = vault_root(&state, &campaign_id) {
        state.evict_index(&old);
    }
    let campaign = state
        .with_db(|conn| {
            let detail = campaigns::get_campaign(conn, &campaign_id)?
                .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
            match crate::world_config::read(&new_root)? {
                Some(cfg) if cfg.id != campaign_id => {
                    return Err(AppError::BadRequest(
                        "That folder already belongs to another world".into(),
                    ));
                }
                Some(_) => {}
                None => {
                    let codex_root = vault::adopt_vault_layout(&new_root)?;
                    vault::write_world_config(&new_root, &campaign_id, &detail.name, &codex_root)?;
                }
            }
            campaigns::register_world_dir(conn, &new_root)?;
            campaigns::get_campaign(conn, &campaign_id)
        })?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
    Ok(Json(serde_json::to_value(campaign).unwrap()))
}

#[derive(Deserialize)]
pub struct SniffRequest {
    pub path: String,
}

// Layout sniff for the New-World "open existing" preview: what would adopting
// this folder do? Read-only, touches nothing.
pub async fn sniff(Json(req): Json<SniffRequest>) -> AppResult<Json<Value>> {
    let root = PathBuf::from(req.path.trim());
    if req.path.trim().is_empty() || !root.is_dir() {
        return Err(AppError::BadRequest("Folder does not exist".into()));
    }
    if let Some(cfg) = crate::world_config::read(&root).ok().flatten() {
        return Ok(Json(json!({
            "mode": "world",
            "name": cfg.name,
            "md_pages": vault::count_pages(&cfg.codex_dir(&root)),
        })));
    }
    if root.join("Codex").is_dir() {
        return Ok(Json(json!({
            "mode": "ck-layout",
            "md_pages": vault::count_pages(&root.join("Codex")),
        })));
    }
    let pages = vault::count_pages(&root);
    Ok(Json(json!({
        "mode": if pages > 0 { "foreign" } else { "empty" },
        "md_pages": pages,
        "assets": vault::count_assets(&root),
    })))
}

#[derive(Deserialize)]
pub struct ImportRequest {
    pub path: String,
}

// Copy-in import: .md pages from a user folder into this world's Codex.
pub async fn import_notes(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<ImportRequest>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    let report = vault::import_folder(&root, std::path::Path::new(req.path.trim()))?;
    reindex_all(&state, &root);
    Ok(Json(
        json!({ "imported": report.imported, "renamed": report.renamed, "assets": report.assets }),
    ))
}

// Pages as JSON, enriched with index-known aliases + tags (empty when the
// index is unavailable — enrichment never fails the listing).
fn pages_json(state: &AppState, root: &std::path::Path) -> AppResult<Vec<Value>> {
    let meta = state
        .with_index(root, crate::store::index::page_meta)
        .and_then(|r| r)
        .unwrap_or_default();
    Ok(vault::list_pages(root)?
        .into_iter()
        .map(|p| {
            let (aliases, tags) = meta.get(&p.path).cloned().unwrap_or_default();
            let mut v = serde_json::to_value(&p).unwrap_or(Value::Null);
            v["aliases"] = json!(aliases);
            v["tags"] = json!(tags);
            v
        })
        .collect())
}

pub async fn list_pages(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    Ok(Json(json!({ "pages": pages_json(&state, &root)? })))
}

pub async fn list_tree(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    Ok(Json(json!({
        "folders": vault::list_folders(&root)?,
        "pages": pages_json(&state, &root)?,
    })))
}

#[derive(Deserialize)]
pub struct FolderRequest {
    pub path: String,
}

pub async fn create_folder(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<FolderRequest>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    vault::create_folder(&root, &req.path)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct MoveRequest {
    pub from: String,
    pub to: String,
}

pub async fn move_entry(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<MoveRequest>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    if req.from.ends_with(".md") && req.to.ends_with(".md") {
        move_page_cascade(&state, &root, &world_root, &req.from, &req.to)?;
    } else {
        vault::move_entry(&root, &req.from, &req.to)?;
        history::move_history(&world_root, &req.from, &req.to);
        // Folder move: every child path changed — full rebuild.
        reindex_all(&state, &root);
    }
    Ok(Json(json!({ "ok": true })))
}

fn move_page_cascade(
    state: &AppState,
    root: &std::path::Path,
    world_root: &std::path::Path,
    from: &str,
    to: &str,
) -> AppResult<()> {
    // Collect inbound links before the index forgets the old target.
    let sources = state
        .with_index(root, |conn| {
            crate::store::index::sources_linking_to(conn, from)
        })
        .and_then(|r| r)
        .unwrap_or_default();
    vault::move_entry(root, from, to)?;
    history::move_history(world_root, from, to);
    rewrite_links_after_rename(state, world_root, root, from, to, sources);
    reindex_remove(state, root, from);
    reindex_page(state, root, to);
    Ok(())
}

// Rename cascade: rewrite [[OldName]] → [[NewName]] in every page that linked
// to the moved page (display labels + #headings preserved). Best-effort.
fn rewrite_links_after_rename(
    state: &AppState,
    world_root: &std::path::Path,
    root: &std::path::Path,
    from: &str,
    to: &str,
    sources: Vec<(String, String)>,
) {
    let stem = |p: &str| {
        std::path::Path::new(p)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    };
    let old_title = stem(from);
    let new_title = stem(to);
    if old_title.is_empty() || new_title.is_empty() || old_title.eq_ignore_ascii_case(&new_title) {
        return; // folder-only move: links resolve by name, nothing to rewrite
    }
    let mut seen = std::collections::HashSet::new();
    for (src, _) in sources {
        // The moved page may link to itself — it now lives at the new path.
        let src = if src == from { to.to_string() } else { src };
        if !seen.insert(src.clone()) {
            continue;
        }
        let Ok(page) = vault::read_page(root, &src) else {
            continue;
        };
        if let Some(updated) =
            crate::store::index::rewrite_link_names(&page.content, &old_title, &new_title)
        {
            let _ = history::record(world_root, root, &src, "user");
            if vault::write_page(root, &src, &updated).is_ok() {
                reindex_page(state, root, &src);
            }
        }
    }
}

pub async fn delete_page(
    State(state): State<AppState>,
    Path((campaign_id, page)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    trash::trash_paths(&world_root, &root, &[(page.clone(), false)])?;
    reindex_remove(&state, &root, &page);
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_folder(
    State(state): State<AppState>,
    Path((campaign_id, folder)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    let pages = vault::page_paths_in_folder(&root, &folder)?;
    trash::trash_paths(&world_root, &root, &[(folder.clone(), true)])?;
    for rel in pages {
        reindex_remove(&state, &root, &rel);
    }
    Ok(Json(json!({ "ok": true })))
}

// ── Trash (Phase 13D): list / restore / empty `.ck/trash` ─────────

pub async fn trash_list(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    Ok(Json(json!({ "groups": trash::list(&world_root) })))
}

#[derive(Deserialize)]
pub struct TrashIdRequest {
    pub id: Option<String>,
}

pub async fn trash_restore(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<TrashIdRequest>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    let id = req
        .id
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("id is required".into()))?;
    let restored = trash::restore(&world_root, &root, id)?;
    for rel in &restored {
        reindex_page(&state, &root, rel);
    }
    Ok(Json(json!({ "restored": restored })))
}

pub async fn trash_empty(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<TrashIdRequest>,
) -> AppResult<Json<Value>> {
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    Ok(Json(
        json!({ "removed": trash::empty(&world_root, req.id.as_deref()) }),
    ))
}

// ── Page history (Phase 13A): versions, snapshots, restore ────────

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub origin: Option<String>,
    pub limit: Option<usize>,
    pub ts: Option<u64>,
}

/// World-wide recent versions — the "everything the Keeper changed" feed.
pub async fn history_recent(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<Value>> {
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let versions = history::recent(&world_root, q.origin.as_deref(), limit);
    Ok(Json(json!({ "versions": versions })))
}

/// Without `ts`: the page's version list. With `ts`: that snapshot's content
/// (`null` = the page did not exist before that save).
pub async fn page_history(
    State(state): State<AppState>,
    Path((campaign_id, page)): Path<(String, String)>,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<Value>> {
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    match q.ts {
        Some(ts) => {
            let (meta, content) = history::read_version(&world_root, &page, ts)?;
            Ok(Json(
                json!({ "ts": meta.ts, "origin": meta.origin, "content": content }),
            ))
        }
        None => Ok(Json(
            json!({ "versions": history::list_page(&world_root, &page)? }),
        )),
    }
}

#[derive(Deserialize)]
pub struct HistoryRestoreRequest {
    pub page: String,
    pub ts: u64,
}

/// Put the page back to the state captured in a snapshot. The pre-restore
/// state is snapshotted first, so a restore is itself undoable. Restoring a
/// "did not exist" snapshot moves the page to the trash.
pub async fn history_restore(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<HistoryRestoreRequest>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    let (_, content) = history::read_version(&world_root, &req.page, req.ts)?;
    let _ = history::record_now(&world_root, &root, &req.page, "user");
    match content {
        Some(c) => {
            vault::write_page(&root, &req.page, &c)?;
            reindex_page(&state, &root, &req.page);
            Ok(Json(json!({ "ok": true, "deleted": false })))
        }
        None => {
            trash::trash_paths(&world_root, &root, &[(req.page.clone(), false)])?;
            reindex_remove(&state, &root, &req.page);
            Ok(Json(json!({ "ok": true, "deleted": true })))
        }
    }
}

// ── Bulk operations (Phase 13B): tag / move / delete many pages ───

#[derive(Deserialize)]
pub struct BulkRequest {
    /// "tag" | "move" | "delete"
    pub action: String,
    pub pages: Vec<String>,
    pub tag: Option<String>,
    /// Move target folder ("" = vault root).
    pub folder: Option<String>,
}

pub async fn bulk(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<BulkRequest>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    if req.pages.is_empty() {
        return Err(AppError::BadRequest("no pages selected".into()));
    }
    let mut errors: Vec<Value> = Vec::new();
    let mut done = 0usize;
    let base = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();

    match req.action.as_str() {
        "delete" => {
            // One trash group for the whole selection → one-click restore.
            let items: Vec<(String, bool)> = req
                .pages
                .iter()
                .filter(|p| {
                    let exists = root.join(p).is_file();
                    if !exists {
                        errors.push(json!({ "page": p, "error": "not found" }));
                    }
                    exists
                })
                .map(|p| (p.clone(), false))
                .collect();
            if !items.is_empty() {
                trash::trash_paths(&world_root, &root, &items)?;
                for (rel, _) in &items {
                    reindex_remove(&state, &root, rel);
                }
                done = items.len();
            }
        }
        "move" => {
            let folder = req.folder.as_deref().unwrap_or("").trim().trim_matches('/');
            for page in &req.pages {
                let name = base(page);
                let to = if folder.is_empty() {
                    name
                } else {
                    format!("{folder}/{name}")
                };
                if to == *page {
                    continue; // already there
                }
                // Same filename → same title: links resolve by name, no rewrite needed.
                match vault::move_entry(&root, page, &to) {
                    Ok(()) => {
                        history::move_history(&world_root, page, &to);
                        reindex_remove(&state, &root, page);
                        reindex_page(&state, &root, &to);
                        done += 1;
                    }
                    Err(e) => errors.push(json!({ "page": page, "error": e.to_string() })),
                }
            }
        }
        "tag" => {
            let tag = req
                .tag
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .ok_or_else(|| AppError::BadRequest("tag is required".into()))?
                .trim_start_matches('#')
                .to_string();
            for page in &req.pages {
                match vault::read_page(&root, page) {
                    Ok(p) => {
                        let updated = vault::fm_append_list_value(&p.content, "tags", &tag);
                        if updated == p.content {
                            continue; // already tagged
                        }
                        let _ = history::record_now(&world_root, &root, page, "user");
                        match vault::write_page(&root, page, &updated) {
                            Ok(_) => {
                                reindex_page(&state, &root, page);
                                done += 1;
                            }
                            Err(e) => errors.push(json!({ "page": page, "error": e.to_string() })),
                        }
                    }
                    Err(e) => errors.push(json!({ "page": page, "error": e.to_string() })),
                }
            }
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "Unknown bulk action: {other}"
            )))
        }
    }
    Ok(Json(json!({ "done": done, "errors": errors })))
}

// ── World backup (Phase 13E) ──────────────────────────────────────

pub async fn backup(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    let path = crate::backup::backup_world(&world_root)?;
    Ok(Json(json!({ "path": path })))
}

// Caret-insert snippets for the editor's slash menu (Phase 8C).
pub async fn list_snippets(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    let snippets: Vec<Value> = vault::list_snippets(&root)
        .into_iter()
        .map(|(name, content)| json!({ "name": name, "content": content }))
        .collect();
    Ok(Json(json!({ "snippets": snippets })))
}

#[derive(Deserialize)]
pub struct AssetQuery {
    pub name: String,
}

// Pasted/dropped editor media: raw bytes in the body, filename in the query.
pub async fn upload_asset(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Query(q): Query<AssetQuery>,
    body: axum::body::Bytes,
) -> AppResult<Json<Value>> {
    if body.is_empty() {
        return Err(AppError::BadRequest("empty asset body".into()));
    }
    let root = vault_root(&state, &campaign_id)?;
    let rel = vault::save_asset(&root, &q.name, &body)?;
    state.note_vault_write(&root, &rel);
    let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
    Ok(Json(json!({ "path": rel, "name": name })))
}

/// Asset bytes for `![[name]]` embeds. Fetched with the auth header and
/// rendered via an object URL (an <img src> can't carry `X-CK-Token`).
pub async fn asset_bytes(
    State(state): State<AppState>,
    Path((campaign_id, name)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let root = vault_root(&state, &campaign_id)?;
    let path = vault::find_asset(&root, &name)?;
    let bytes =
        std::fs::read(&path).map_err(|_| AppError::NotFound(format!("Asset not found: {name}")))?;
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
        .unwrap_or("")
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    };
    Ok(([(header::CONTENT_TYPE, mime)], bytes))
}

pub async fn read_page(
    State(state): State<AppState>,
    Path((campaign_id, page)): Path<(String, String)>,
) -> AppResult<Json<vault::Page>> {
    let root = vault_root(&state, &campaign_id)?;
    Ok(Json(vault::read_page(&root, &page)?))
}

#[derive(Deserialize)]
pub struct WriteRequest {
    pub content: String,
}

pub async fn write_page(
    State(state): State<AppState>,
    Path((campaign_id, page)): Path<(String, String)>,
    Json(req): Json<WriteRequest>,
) -> AppResult<Json<vault::Page>> {
    let root = vault_root(&state, &campaign_id)?;
    if let Ok((world_root, _)) = world_cfg(&state, &campaign_id) {
        let _ = history::record(&world_root, &root, &page, "user");
    }
    let result = vault::write_page(&root, &page, &req.content)?;
    reindex_page(&state, &root, &page);
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub title: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub folder: Option<String>,
}

fn default_kind() -> String {
    "lore".into()
}

fn language_name(code: &str) -> String {
    match code
        .trim()
        .to_lowercase()
        .split(['-', '_'])
        .next()
        .unwrap_or("")
    {
        "de" => "German".into(),
        "en" => "English".into(),
        "fr" => "French".into(),
        "es" => "Spanish".into(),
        "it" => "Italian".into(),
        "pt" => "Portuguese".into(),
        "nl" => "Dutch".into(),
        "pl" => "Polish".into(),
        "ru" => "Russian".into(),
        "" => "the same language as the page content".into(),
        other => format!("the language with code \"{other}\""),
    }
}

// Map the first path segment (folder name) to a kind where unambiguous.
fn kind_from_folder(path: &str) -> Option<&'static str> {
    let folder = path.split('/').next().unwrap_or("").to_lowercase();
    match folder.as_str() {
        "npcs" | "npc" | "characters" | "character" | "persons" | "people" | "personen" => {
            Some("npc")
        }
        "pcs" | "pc" | "players" | "party" | "heroes" | "spieler" => Some("pc"),
        "places" | "place" | "locations" | "location" | "cities" | "orte" => Some("place"),
        "factions" | "faction" | "organizations" | "organisations" | "groups" | "guilds"
        | "fraktionen" => Some("faction"),
        "items" | "item" | "artifacts" | "gear" | "weapons" | "gegenstände" => Some("item"),
        "events" | "event" | "ereignisse" => Some("event"),
        "lore" | "lores" | "history" | "knowledge" | "concepts" => Some("lore"),
        _ => None,
    }
}

const BATCH_SIZE: usize = 15;
const EXCERPT_CHARS: usize = 600;

// Returns the first path segment ("NPCs" from "NPCs/Main/Aragorn.md"), or ""
// for root-level pages. Used to group pages by top-level folder.
fn top_folder(page_path: &str) -> &str {
    match page_path.find('/') {
        Some(i) => &page_path[..i],
        None => "",
    }
}

fn build_batch_prompt(
    entries: &[(String, String)],
    folder_kind: Option<&str>,
    lang_name: &str,
) -> String {
    let (kind_clause, schema) = match folder_kind {
        Some(k) => (
            format!("All pages are of kind `{k}`.\n"),
            r#"[{"title":"…","summary":"…"},…]"#.to_string(),
        ),
        None => (
            "Classify each page. `kind` must be exactly one of: pc, npc, place, faction, item, lore\n\
             pc=player character  npc=other person/creature  place=location\n\
             faction=group/org  item=object/weapon  lore=concept/event/rule\n"
                .to_string(),
            r#"[{"title":"…","kind":"…","summary":"…"},…]"#.to_string(),
        ),
    };
    let mut out = format!(
        "Analyze these tabletop-RPG wiki pages.\n\
         Output ONLY a raw JSON array — no prose, no markdown fences.\n\
         {kind_clause}\
         `summary`: one concise sentence (max 20 words) in {lang_name}.\n\
         Schema: {schema}\n\n"
    );
    for (title, content) in entries {
        let excerpt: String = content.chars().take(EXCERPT_CHARS).collect();
        out.push_str(&format!("=== {title} ===\n{excerpt}\n\n"));
    }
    out
}

fn parse_batch_response(
    raw: &str,
    titles: &[String],
    folder_kind: Option<&str>,
) -> Vec<Option<(String, String)>> {
    let n = titles.len();
    let mut results = vec![None; n];

    let parsed = serde_json::from_str::<serde_json::Value>(raw.trim())
        .or_else(|_| {
            let s = raw.find('[');
            let e = raw.rfind(']');
            match (s, e) {
                (Some(s), Some(e)) if e > s => serde_json::from_str(&raw[s..=e]),
                _ => Ok(serde_json::Value::Null),
            }
        })
        .unwrap_or(serde_json::Value::Null);

    let arr = match parsed.as_array() {
        Some(a) => a,
        None => return results,
    };

    let title_idx: std::collections::HashMap<String, usize> = titles
        .iter()
        .enumerate()
        .map(|(i, t)| (t.to_lowercase(), i))
        .collect();

    for (pos, item) in arr.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let summary = obj
            .get("summary")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let Some(summary) = summary else { continue };

        let kind = if let Some(k) = folder_kind {
            k.to_string()
        } else {
            let raw_kind = obj
                .get("kind")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_lowercase())
                .unwrap_or_default();
            if !crate::vault::KINDS.contains(&raw_kind.as_str()) {
                continue;
            }
            raw_kind
        };

        let idx = obj
            .get("title")
            .and_then(|v| v.as_str())
            .and_then(|t| title_idx.get(&t.trim().to_lowercase()).copied())
            .unwrap_or(pos);
        if idx < n {
            results[idx] = Some((kind, summary));
        }
    }
    results
}

#[derive(Deserialize)]
pub struct EnhanceRequest {
    /// Top-level folder names to enhance. Empty = all folders.
    #[serde(default)]
    pub folders: Vec<String>,
}

pub async fn enhance_pages(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<EnhanceRequest>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    let world_root = world_cfg(&state, &campaign_id).map(|(r, _)| r).ok();
    let (target, lang_name) = state.with_db(|conn| {
        let cfg = crate::config::get_config_map(conn)?;
        let target = crate::llm::resolve(conn, &cfg, None, None, None)?;
        let language = crate::store::campaigns::get_campaign(conn, &campaign_id)
            .ok()
            .flatten()
            .and_then(|c| Some(c.default_language).filter(|s| !s.trim().is_empty()))
            .or_else(|| cfg.get("default_language").cloned())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "en".into());
        Ok::<_, crate::error::AppError>((target, language_name(&language)))
    })?;

    let wanted: std::collections::HashSet<String> = req.folders.iter().cloned().collect();
    let all = req.folders.is_empty();

    // Group pages needing enhancement by top-level folder.
    let mut by_folder: std::collections::HashMap<String, Vec<(String, String, String)>> =
        Default::default();
    for info in vault::list_pages(&root)? {
        let folder = top_folder(&info.path).to_string();
        if !all && !wanted.contains(&folder) {
            continue;
        }
        let abs = root.join(&info.path);
        if let Ok(content) = std::fs::read_to_string(&abs) {
            if vault::needs_frontmatter_enhance(&content) {
                by_folder.entry(folder).or_default().push((
                    info.path.clone(),
                    info.title.clone(),
                    content,
                ));
            }
        }
    }

    let mut enhanced = 0usize;
    let mut failed = 0usize;

    for (folder, pages) in &by_folder {
        let folder_kind = kind_from_folder(folder);
        for chunk in pages.chunks(BATCH_SIZE) {
            let entries: Vec<(String, String)> = chunk
                .iter()
                .map(|(_, title, content)| (title.clone(), content.clone()))
                .collect();
            let titles: Vec<String> = entries.iter().map(|(t, _)| t.clone()).collect();
            let prompt = build_batch_prompt(&entries, folder_kind, &lang_name);
            let raw = match crate::llm::chat(
                &crate::llm::ChatRequest {
                    transport: target.transport,
                    api_base: &target.api_base,
                    api_key: &target.api_key,
                    model: &target.model,
                    prompt: &prompt,
                    timeout_secs: target.timeout,
                    num_ctx_max: target.num_ctx_max,
                },
                true,
            )
            .await
            {
                Ok(r) => r,
                Err(_) => {
                    failed += chunk.len();
                    continue;
                }
            };
            let batch_results = parse_batch_response(&raw, &titles, folder_kind);
            for ((rel, _, content), result) in chunk.iter().zip(batch_results) {
                let outcome =
                    result.or_else(|| folder_kind.map(|k| (k.to_string(), String::new())));
                if let Some((kind, summary)) = outcome {
                    let updated = vault::set_frontmatter_fields(content, &kind, &summary);
                    if let Some(wr) = &world_root {
                        let _ = history::record(wr, &root, rel, "keeper");
                    }
                    if vault::write_page(&root, rel, &updated).is_ok() {
                        reindex_page(&state, &root, rel);
                        enhanced += 1;
                    } else {
                        failed += 1;
                    }
                } else {
                    failed += 1;
                }
            }
        }
    }

    let total: usize = by_folder.values().map(|v| v.len()).sum();
    let skipped = total - enhanced - failed;
    Ok(Json(
        json!({ "enhanced": enhanced, "skipped": skipped, "failed": failed }),
    ))
}

// World root + parsed `.ck/config.toml` (defaults when unreadable).
pub(super) fn world_cfg(
    state: &AppState,
    campaign_id: &str,
) -> AppResult<(PathBuf, crate::world_config::WorldConfig)> {
    let root = state
        .with_db(|conn| campaigns::world_root_for_id(conn, campaign_id))?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
    let cfg = crate::world_config::read(&root)
        .ok()
        .flatten()
        .unwrap_or_default();
    Ok((root, cfg))
}

// Merged per-kind infobox schemas (built-ins + this world's overrides).
pub async fn kind_schemas(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (_, cfg) = world_cfg(&state, &campaign_id)?;
    let kinds: Vec<Value> = cfg
        .kind_schemas()
        .into_iter()
        .map(|(kind, fields)| json!({ "kind": kind, "fields": fields }))
        .collect();
    Ok(Json(json!({ "kinds": kinds })))
}

pub async fn create_page(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<CreateRequest>,
) -> AppResult<Json<vault::Page>> {
    let root = vault_root(&state, &campaign_id)?;
    let (world_root, cfg) = world_cfg(&state, &campaign_id)?;
    let fields = cfg
        .kind_schemas()
        .into_iter()
        .find(|(k, _)| k == &req.kind)
        .map(|(_, f)| f)
        .unwrap_or_default();
    let content = vault::new_page_content(&world_root, &req.kind, &req.title, &fields);
    let page = vault::create_page_with(&root, &req.title, req.folder.as_deref(), &content)?;
    // First version = "did not exist": restoring it deletes the page again.
    let _ = history::record_create(&world_root, &page.path, "user");
    reindex_page(&state, &root, &page.path);
    Ok(Json(page))
}

#[derive(Deserialize)]
pub struct PromoteRequest {
    pub page: String,
    pub kind: String,
    #[serde(default)]
    pub folder: Option<String>,
}

/// Phase 16: capture → real page. Rewrites the frontmatter to the target
/// kind (existing values kept, `inbox` tag dropped), appends the kind's
/// missing template headings, and optionally moves the page.
pub async fn promote_page(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<PromoteRequest>,
) -> AppResult<Json<vault::Page>> {
    let root = vault_root(&state, &campaign_id)?;
    let (world_root, cfg) = world_cfg(&state, &campaign_id)?;
    let fields = cfg
        .kind_schemas()
        .into_iter()
        .find(|(k, _)| k == &req.kind)
        .map(|(_, f)| f)
        .unwrap_or_default();
    let page = vault::read_page(&root, &req.page)?;
    let headings = vault::template_headings(&world_root, &req.kind);
    let content = vault::promote_content(&page.content, &req.kind, &fields, &headings);
    let _ = history::record(&world_root, &root, &req.page, "user");
    vault::write_page(&root, &req.page, &content)?;
    reindex_page(&state, &root, &req.page);

    let mut path = req.page.clone();
    if let Some(folder) = req.folder.as_deref().map(str::trim) {
        let name = req.page.rsplit('/').next().unwrap_or(&req.page);
        let to = if folder.is_empty() {
            name.to_string()
        } else {
            format!("{folder}/{name}")
        };
        if to != req.page {
            move_page_cascade(&state, &root, &world_root, &req.page, &to)?;
            path = to;
        }
    }
    Ok(Json(vault::read_page(&root, &path)?))
}
