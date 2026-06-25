//! FoundryVTT bridge (Phase 23): one-way push of vault pages → Foundry Journal
//! entries. CK is the source of truth; Foundry is a projection, never read back.

pub mod client;
pub mod read;
pub mod sync;
pub mod system;

pub use client::{fetch_status, FoundryClient};

/// The Foundry major version the bridge is validated against (v14.364). A live
/// world on another major is allowed but flagged on Test — Foundry's document
/// schemas drift between majors (cf. RollTable `text`→`name`, `Scene#background`
/// →`Level`), so a mismatch is the likely first suspect when a sync misbehaves.
pub const SUPPORTED_FOUNDRY_MAJOR: &str = "14";

/// True when `version` (Foundry's `generation`/full version string) shares the
/// supported major. Unparseable / empty versions are treated as compatible — we
/// only warn on a *known* mismatch, never on uncertainty.
pub fn version_compatible(version: &str) -> bool {
    match version.trim().split('.').next() {
        Some(major) if !major.is_empty() => major == SUPPORTED_FOUNDRY_MAJOR,
        _ => true,
    }
}

use crate::config;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

/// Connection settings for the Foundry bridge (stored in the global app DB).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FoundrySettings {
    pub server_url: String,
    pub user_id: String,
    pub password: String,
}

impl FoundrySettings {
    pub fn is_complete(&self) -> bool {
        !self.server_url.is_empty() && !self.user_id.is_empty() && !self.password.is_empty()
    }
}

/// Read the bridge settings from the global app DB.
pub fn load_settings(state: &AppState) -> AppResult<FoundrySettings> {
    state.with_db(|conn| {
        Ok(FoundrySettings {
            server_url: config::get_value(conn, "foundry_server_url")?.unwrap_or_default(),
            user_id: config::get_value(conn, "foundry_user_id")?.unwrap_or_default(),
            password: config::get_value(conn, "foundry_password")?.unwrap_or_default(),
        })
    })
}

// ---------------------------------------------------------------------------
// Journal write helpers — thin wrappers over the generic `modifyDocument` op.
// ---------------------------------------------------------------------------

impl FoundryClient {
    /// Creates a `JournalEntry`-typed folder; returns its id.
    pub async fn create_folder(
        &mut self,
        name: &str,
        parent_id: Option<&str>,
    ) -> AppResult<String> {
        let resp = self
            .modify_document(
                "Folder",
                "create",
                json!({ "data": [{ "name": name, "type": "JournalEntry", "folder": parent_id }] }),
            )
            .await?;
        first_id(&resp)
    }

    /// Creates a `JournalEntry` with a single text page; returns `(journalId,
    /// pageId)`. `path` is stamped into `flags["chronicle-keeper"].path` as the
    /// self-describing identity backstop.
    pub async fn create_journal(
        &mut self,
        name: &str,
        html: &str,
        folder_id: Option<&str>,
        path: &str,
    ) -> AppResult<(String, String)> {
        let resp = self
            .modify_document(
                "JournalEntry",
                "create",
                json!({ "data": [{
                    "name": name,
                    "folder": folder_id,
                    "pages": [{ "name": name, "type": "text", "text": { "content": html, "format": 1 } }],
                    "flags": { "chronicle-keeper": { "path": path } },
                }] }),
            )
            .await?;
        let doc = resp
            .get("result")
            .and_then(|r| r.get(0))
            .ok_or_else(missing)?;
        let journal_id = doc
            .get("_id")
            .and_then(|v| v.as_str())
            .ok_or_else(missing)?;
        let page_id = doc
            .get("pages")
            .and_then(|p| p.get(0))
            .and_then(|p| p.get("_id"))
            .and_then(|v| v.as_str())
            .ok_or_else(missing)?;
        Ok((journal_id.to_string(), page_id.to_string()))
    }

    /// Replaces (not appends) a journal page's HTML body.
    pub async fn update_journal_page(
        &mut self,
        journal_id: &str,
        page_id: &str,
        html: &str,
    ) -> AppResult<()> {
        self.modify_document(
            "JournalEntryPage",
            "update",
            json!({
                "updates": [{ "_id": page_id, "text": { "content": html } }],
                "parentUuid": format!("JournalEntry.{journal_id}"),
                "diff": true,
                "recursive": true,
            }),
        )
        .await
        .map(|_| ())
    }

    /// Deletes a `JournalEntry`.
    pub async fn delete_journal(&mut self, journal_id: &str) -> AppResult<()> {
        self.modify_document("JournalEntry", "delete", json!({ "ids": [journal_id] }))
            .await
            .map(|_| ())
    }

    /// Creates a gridless `Scene` whose background lives on a base `Level`
    /// (Foundry v14 moved `Scene#background` onto `Level#background`). `level_id`
    /// is the caller-chosen id of that base level, also set as `initialLevel`.
    pub async fn create_scene(
        &mut self,
        name: &str,
        bg_src: &str,
        width: u32,
        height: u32,
        map_id: &str,
        level_id: &str,
    ) -> AppResult<String> {
        let resp = self
            .modify_document(
                "Scene",
                "create",
                json!({ "data": [{
                    "name": name,
                    "width": width,
                    "height": height,
                    "padding": 0.0,
                    "grid": { "type": 0, "size": 100 },
                    "levels": [{
                        "_id": level_id,
                        "name": "Base",
                        "elevation": 0,
                        "background": { "src": bg_src },
                    }],
                    "initialLevel": level_id,
                    "flags": { "chronicle-keeper": { "map_id": map_id } },
                }] }),
            )
            .await?;
        first_id(&resp)
    }

    /// Updates a `Scene`'s dimensions and its base level's background in place.
    pub async fn update_scene(
        &mut self,
        scene_id: &str,
        level_id: &str,
        bg_src: &str,
        width: u32,
        height: u32,
    ) -> AppResult<()> {
        self.modify_document(
            "Scene",
            "update",
            json!({ "updates": [{
                "_id": scene_id,
                "width": width,
                "height": height,
                "levels": [{ "_id": level_id, "background": { "src": bg_src } }],
            }] }),
        )
        .await
        .map(|_| ())
    }

    /// Deletes a `Scene` (its embedded notes go with it).
    pub async fn delete_scene(&mut self, scene_id: &str) -> AppResult<()> {
        self.modify_document("Scene", "delete", json!({ "ids": [scene_id] }))
            .await
            .map(|_| ())
    }

    /// Places a map `Note` on a scene linking to a journal entry; returns its id.
    pub async fn create_note(
        &mut self,
        scene_id: &str,
        x: i64,
        y: i64,
        entry_id: &str,
        label: &str,
    ) -> AppResult<String> {
        let resp = self
            .modify_document(
                "Note",
                "create",
                json!({
                    "parentUuid": format!("Scene.{scene_id}"),
                    "data": [{
                        "x": x,
                        "y": y,
                        "entryId": entry_id,
                        "text": label,
                        "fontSize": 24,
                        "iconSize": 40,
                        "texture": { "src": "icons/svg/book.svg" },
                    }],
                }),
            )
            .await?;
        first_id(&resp)
    }

    /// Deletes the given notes from a scene.
    pub async fn delete_notes(&mut self, scene_id: &str, ids: &[String]) -> AppResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.modify_document(
            "Note",
            "delete",
            json!({ "parentUuid": format!("Scene.{scene_id}"), "ids": ids }),
        )
        .await
        .map(|_| ())
    }

    // -- ad-hoc stubs (Keeper create tools; fire-and-forget, not in foundry-map) --

    /// Creates an `Actor`; returns its id. `system` is the game-system-specific
    /// stat block (5e `attributes.hp`, Daggerheart's shape, …) — when `None` the
    /// actor is a bare stub the GM finishes in Foundry. `items` are embedded item
    /// documents (weapons/spells/features). Both are passed through verbatim: the
    /// Keeper authors them by mirroring a real same-system actor (see foundry_get_actor),
    /// so CK never needs a per-system schema.
    pub async fn create_actor(
        &mut self,
        name: &str,
        actor_type: &str,
        system: Option<&Value>,
        items: Option<&Value>,
    ) -> AppResult<String> {
        let data = actor_create_data(name, actor_type, system, items);
        let resp = self
            .modify_document("Actor", "create", json!({ "data": [data] }))
            .await?;
        first_id(&resp)
    }

    /// Creates a `RollTable` of plain-text results; returns its id. `entries` are
    /// `(text, weight)` pairs that tile the `1..=N` roll range in order.
    pub async fn create_rolltable(
        &mut self,
        name: &str,
        entries: &[(String, u32)],
    ) -> AppResult<String> {
        let mut low = 1u32;
        let results: Vec<Value> = entries
            .iter()
            .map(|(text, weight)| {
                let w = (*weight).max(1);
                let high = low + w - 1;
                // v14 TableResult: `name` is the label, `description` the chat
                // output (the old single `text` field was removed in v13/v14).
                let r = json!({
                    "type": "text",
                    "name": text,
                    "description": text,
                    "weight": w,
                    "range": [low, high],
                });
                low = high + 1;
                r
            })
            .collect();
        let resp = self
            .modify_document(
                "RollTable",
                "create",
                json!({ "data": [{
                    "name": name,
                    "formula": format!("1d{}", (low - 1).max(1)),
                    "results": results,
                }] }),
            )
            .await?;
        first_id(&resp)
    }

    /// Posts a `ChatMessage` to the table's chat log; returns its id. The content
    /// is sent as HTML (Foundry renders it). `author` is the posting user's 16-char
    /// `_id` (v14 renamed the old `user` field to `author`).
    pub async fn post_chat(&mut self, html: &str, author: &str) -> AppResult<String> {
        let resp = self
            .modify_document(
                "ChatMessage",
                "create",
                json!({ "data": [{ "content": html, "author": author }] }),
            )
            .await?;
        first_id(&resp)
    }

    /// Creates a gridless, background-less `Scene` (a blank canvas); returns its id.
    pub async fn create_scene_stub(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
    ) -> AppResult<String> {
        let level_id = random_id();
        let resp = self
            .modify_document(
                "Scene",
                "create",
                json!({ "data": [{
                    "name": name,
                    "width": width,
                    "height": height,
                    "padding": 0.0,
                    "grid": { "type": 0, "size": 100 },
                    "levels": [{ "_id": level_id, "name": "Base", "elevation": 0 }],
                    "initialLevel": level_id,
                }] }),
            )
            .await?;
        first_id(&resp)
    }
}

/// Builds the `Actor` create payload, attaching the optional `system` stat block
/// and `items` only when present (a bare stub carries neither).
fn actor_create_data(
    name: &str,
    actor_type: &str,
    system: Option<&Value>,
    items: Option<&Value>,
) -> Value {
    let mut data = json!({ "name": name, "type": actor_type });
    if let Value::Object(ref mut m) = data {
        if let Some(sys) = system {
            m.insert("system".into(), sys.clone());
        }
        if let Some(its) = items {
            m.insert("items".into(), its.clone());
        }
    }
    data
}

/// A 16-char alphanumeric id in Foundry's `randomID` shape (hex is a valid
/// subset), for embedded ids that must be set before creation.
pub(crate) fn random_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..16].to_string()
}

fn first_id(resp: &Value) -> AppResult<String> {
    resp.get("result")
        .and_then(|r| r.get(0))
        .and_then(|d| d.get("_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(missing)
}

fn missing() -> AppError {
    AppError::Internal(anyhow::anyhow!("foundry response missing expected id"))
}

// ---------------------------------------------------------------------------
// path → JournalEntry id map (CK owns identity; `.ck/foundry-map.json`)
// ---------------------------------------------------------------------------

/// One synced page's Foundry coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapEntry {
    pub journal_id: String,
    pub page_id: String,
}

/// One synced atlas map's Foundry coordinates: the Scene and the Note ids it
/// currently carries (recreated each sync, so they are tracked to be cleared).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneEntry {
    pub scene_id: String,
    /// The scene's base `Level` id (carries the background in v14).
    #[serde(default)]
    pub level_id: String,
    #[serde(default)]
    pub note_ids: Vec<String>,
}

/// CK-owned identity map persisted at `.ck/foundry-map.json`: vault page path →
/// Foundry journal/page ids, vault folder path → Foundry folder id, and atlas
/// map id → Foundry scene/notes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FoundryMap {
    #[serde(default)]
    pub pages: HashMap<String, MapEntry>,
    #[serde(default)]
    pub folders: HashMap<String, String>,
    #[serde(default)]
    pub scenes: HashMap<String, SceneEntry>,
}

fn map_path(world_root: &Path) -> std::path::PathBuf {
    world_root.join(".ck").join("foundry-map.json")
}

pub fn read_map(world_root: &Path) -> FoundryMap {
    let text = std::fs::read_to_string(map_path(world_root)).unwrap_or_default();
    if text.is_empty() {
        return FoundryMap::default();
    }
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn write_map(world_root: &Path, map: &FoundryMap) -> AppResult<()> {
    let path = map_path(world_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry-map mkdir: {e}")))?;
    }
    let text = serde_json::to_string_pretty(map)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry-map encode: {e}")))?;
    std::fs::write(&path, text)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry-map write: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Markdown + wikilink transforms
// ---------------------------------------------------------------------------

/// Converts page markdown (frontmatter already stripped) to HTML for a Foundry
/// text page, after rewriting `[[wikilinks]]` to Foundry `@UUID` references.
pub fn body_to_html(body: &str, resolve: &impl Fn(&str) -> Option<String>) -> String {
    let rewritten = rewrite_wikilinks(body, resolve);
    let parser = pulldown_cmark::Parser::new(&rewritten);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

/// Rewrites `[[Target]]`, `[[Target|Label]]`, `[[Target#Heading|Label]]` to
/// `@UUID[JournalEntry.<id>]{Label}` when `resolve(target)` yields a journal
/// id; otherwise to plain `Label` text (no red-link explosion — mirrors the
/// "stub, don't alarm" stance). `resolve` maps a link target name to a journal id.
pub fn rewrite_wikilinks(body: &str, resolve: &impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some(end) = body[i + 2..].find("]]") {
                let inner = &body[i + 2..i + 2 + end];
                out.push_str(&render_wikilink(inner, resolve));
                i += 2 + end + 2;
                continue;
            }
        }
        // Push one UTF-8 char so we never split a multibyte sequence.
        let ch = body[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn render_wikilink(inner: &str, resolve: &impl Fn(&str) -> Option<String>) -> String {
    let (target_part, label) = match inner.split_once('|') {
        Some((t, l)) => (t.trim(), l.trim().to_string()),
        None => (inner.trim(), inner.trim().to_string()),
    };
    let target = target_part.split('#').next().unwrap_or(target_part).trim();
    let label = if label.contains('#') {
        // "Target#Heading" with no explicit label → show the target name.
        target.to_string()
    } else {
        label
    };
    match resolve(target) {
        Some(id) => format!("@UUID[JournalEntry.{id}]{{{label}}}"),
        None => label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver() -> impl Fn(&str) -> Option<String> {
        |name: &str| match name.to_lowercase().as_str() {
            "alpha" => Some("aaaaaaaaaaaaaaaa".to_string()),
            _ => None,
        }
    }

    #[test]
    fn actor_create_data_attaches_only_present_fields() {
        let bare = actor_create_data("Goblin", "npc", None, None);
        assert_eq!(bare["name"], "Goblin");
        assert_eq!(bare["type"], "npc");
        assert!(bare.get("system").is_none());
        assert!(bare.get("items").is_none());

        let sys = json!({ "attributes": { "hp": { "value": 7, "max": 7 } } });
        let items = json!([{ "name": "Scimitar", "type": "weapon" }]);
        let statted = actor_create_data("Goblin", "npc", Some(&sys), Some(&items));
        assert_eq!(statted["system"], sys);
        assert_eq!(statted["items"], items);
    }

    #[test]
    fn version_compat_matches_major_only() {
        assert!(version_compatible("14.364"));
        assert!(version_compatible("14"));
        assert!(!version_compatible("13.331"));
        assert!(!version_compatible("15.2"));
        // Unknown / empty → treated as compatible (warn only on a known mismatch).
        assert!(version_compatible(""));
        assert!(version_compatible("   "));
    }

    #[test]
    fn resolved_link_becomes_uuid() {
        assert_eq!(
            rewrite_wikilinks("see [[Alpha]] now", &resolver()),
            "see @UUID[JournalEntry.aaaaaaaaaaaaaaaa]{Alpha} now"
        );
    }

    #[test]
    fn explicit_label_is_kept() {
        assert_eq!(
            rewrite_wikilinks("[[Alpha|the first]]", &resolver()),
            "@UUID[JournalEntry.aaaaaaaaaaaaaaaa]{the first}"
        );
    }

    #[test]
    fn unresolved_link_falls_back_to_plain_text() {
        assert_eq!(
            rewrite_wikilinks("go to [[Beta]]", &resolver()),
            "go to Beta"
        );
    }

    #[test]
    fn heading_only_link_labels_with_target() {
        assert_eq!(
            rewrite_wikilinks("[[Alpha#Lore]]", &resolver()),
            "@UUID[JournalEntry.aaaaaaaaaaaaaaaa]{Alpha}"
        );
    }

    #[test]
    fn non_links_and_unicode_pass_through() {
        let input = "café — [single] {brace} stays, [[Beta]] drops";
        assert_eq!(
            rewrite_wikilinks(input, &resolver()),
            "café — [single] {brace} stays, Beta drops"
        );
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    // Live smoke against a real Foundry world. Ignored by default; run with:
    //   FOUNDRY_URL=… FOUNDRY_USER_ID=… FOUNDRY_PASSWORD=… \
    //     cargo test -p ck-core foundry_live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn foundry_live_journal_cycle() {
        let url = std::env::var("FOUNDRY_URL").unwrap();
        let uid = std::env::var("FOUNDRY_USER_ID").unwrap();
        let pw = std::env::var("FOUNDRY_PASSWORD").unwrap();

        let mut c = FoundryClient::connect(&url, &uid, &pw)
            .await
            .expect("connect");
        let folder = c
            .create_folder("CK rust smoke", None)
            .await
            .expect("folder");
        println!("folder {folder}");

        let body = body_to_html("# Hi\n\nLink to [[Nowhere]] and **bold**.", &|_| None);
        let (jid, pid) = c
            .create_journal(
                "CK rust smoke entry",
                &body,
                Some(&folder),
                "Codex/Smoke/x.md",
            )
            .await
            .expect("journal");
        println!("journal {jid} page {pid}");

        c.update_journal_page(&jid, &pid, "<p>Replaced only.</p>")
            .await
            .expect("update");
        c.delete_journal(&jid).await.expect("delete journal");
        // Clean the folder too (raw op; no public helper).
        c.modify_document("Folder", "delete", serde_json::json!({ "ids": [folder] }))
            .await
            .expect("delete folder");
        c.close().await;
        println!("RUST FOUNDRY SMOKE PASS");
    }

    // Live smoke for the ad-hoc create tools (actor / rolltable / scene). Same
    // env + invocation as above, with `foundry_create` as the filter:
    //   FOUNDRY_URL=… FOUNDRY_USER_ID=… FOUNDRY_PASSWORD=… \
    //     cargo test -p ck-core foundry_live_create -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn foundry_live_create_cycle() {
        let url = std::env::var("FOUNDRY_URL").unwrap();
        let uid = std::env::var("FOUNDRY_USER_ID").unwrap();
        let pw = std::env::var("FOUNDRY_PASSWORD").unwrap();

        let mut c = FoundryClient::connect(&url, &uid, &pw)
            .await
            .expect("connect");

        let actor = c
            .create_actor("CK smoke actor", "npc", None, None)
            .await
            .expect("actor");
        println!("actor {actor}");

        // Statted actor: a freeform system blob is passed through verbatim (the
        // Keeper would mirror a real same-system actor's shape here).
        let statted = c
            .create_actor(
                "CK smoke statted",
                "npc",
                Some(&json!({ "attributes": { "hp": { "value": 12, "max": 12 } } })),
                Some(&json!([{ "name": "Club", "type": "weapon" }])),
            )
            .await
            .expect("statted actor");
        println!("statted actor {statted}");

        let table = c
            .create_rolltable(
                "CK smoke loot",
                &[("50 gp".into(), 3), ("Potion of Healing".into(), 1)],
            )
            .await
            .expect("rolltable");
        println!("rolltable {table}");

        let scene = c
            .create_scene_stub("CK smoke scene", 2000, 2000)
            .await
            .expect("scene");
        println!("scene {scene}");

        // Clean up everything created (actor/rolltable have no public helper).
        c.modify_document("Actor", "delete", json!({ "ids": [actor, statted] }))
            .await
            .expect("delete actor");
        c.modify_document("RollTable", "delete", json!({ "ids": [table] }))
            .await
            .expect("delete rolltable");
        c.delete_scene(&scene).await.expect("delete scene");

        c.close().await;
        println!("RUST FOUNDRY CREATE SMOKE PASS");
    }

    // Phase 25 live-play reads + post_chat. Confirms the `world` emit ack carries
    // the snapshot (the open transport question) and that the agnostic parsers find
    // real documents. Dumps the snapshot's top-level keys first so any schema drift
    // is visible. Same env + invocation, filter `foundry_live_read`:
    //   FOUNDRY_URL=… FOUNDRY_USER_ID=… FOUNDRY_PASSWORD=… \
    //     cargo test -p ck-core foundry_live_read -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn foundry_live_read_cycle() {
        let url = std::env::var("FOUNDRY_URL").unwrap();
        let uid = std::env::var("FOUNDRY_USER_ID").unwrap();
        let pw = std::env::var("FOUNDRY_PASSWORD").unwrap();

        let mut c = FoundryClient::connect(&url, &uid, &pw)
            .await
            .expect("connect");

        let world = c.fetch_world().await.expect("fetch world");
        if let Value::Object(map) = &world {
            let keys: Vec<&String> = map.keys().collect();
            println!("world snapshot top-level keys: {keys:?}");
        } else {
            println!("UNEXPECTED world payload shape: {world}");
        }

        println!("--- list_actors ---\n{}", read::list_actors(&world));
        println!("--- scene_state ---\n{}", read::scene_state(&world));
        println!("--- lookup 'stealth' ---\n{}", read::lookup(&world, "stealth"));

        // get_actor against the first actor present, to prove the raw `system`
        // blob extraction on a real (system-specific) sheet.
        if let Some(first) = read::collection(&world, "actors")
            .first()
            .and_then(|a| a.get("name"))
            .and_then(|n| n.as_str())
        {
            println!("--- get_actor {first:?} ---\n{}", read::get_actor(&world, first));
        }

        let id = c
            .post_chat("<p>CK Phase 25 live-read smoke.</p>", &uid)
            .await
            .expect("post chat");
        println!("posted chat message {id}");

        c.close().await;
        println!("RUST FOUNDRY READ SMOKE PASS");
    }
}
