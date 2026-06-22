//! FoundryVTT bridge (Phase 23): one-way push of vault pages → Foundry Journal
//! entries. CK is the source of truth; Foundry is a projection, never read back.

pub mod client;
pub mod sync;

pub use client::FoundryClient;

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

/// CK-owned identity map persisted at `.ck/foundry-map.json`: vault page path →
/// Foundry journal/page ids, and vault folder path → Foundry folder id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FoundryMap {
    #[serde(default)]
    pub pages: HashMap<String, MapEntry>,
    #[serde(default)]
    pub folders: HashMap<String, String>,
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
}
