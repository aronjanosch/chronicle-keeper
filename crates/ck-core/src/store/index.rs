//! Per-world `.ck/index.db` — a rebuildable cache over the vault's `.md`
//! pages: wikilink graph, aliases, tags, headings, FTS. Never source of truth;
//! deleting the file is safe (rebuilt on next open).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};
use crate::vault;

// 5: normalize_name now NFC-normalizes — stored aliases/target_names must be rebuilt.
pub const SCHEMA_VERSION: &str = "6";

const SCHEMA: &str = "
CREATE TABLE pages (
    path         TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    kind         TEXT,
    summary      TEXT,
    frontmatter  TEXT,
    content_hash TEXT NOT NULL,
    modified_at  INTEGER NOT NULL
);
CREATE TABLE page_aliases (
    alias     TEXT NOT NULL,
    page_path TEXT NOT NULL REFERENCES pages(path) ON DELETE CASCADE,
    PRIMARY KEY (alias, page_path)
);
CREATE INDEX idx_aliases_alias ON page_aliases(alias);
CREATE TABLE page_tags (
    page_path TEXT NOT NULL REFERENCES pages(path) ON DELETE CASCADE,
    tag       TEXT NOT NULL,
    PRIMARY KEY (page_path, tag)
);
CREATE INDEX idx_tags_tag ON page_tags(tag);
CREATE TABLE page_links (
    source_path TEXT NOT NULL REFERENCES pages(path) ON DELETE CASCADE,
    target_path TEXT,
    link_text   TEXT NOT NULL,
    heading     TEXT,
    PRIMARY KEY (source_path, link_text)
);
CREATE INDEX idx_links_target ON page_links(target_path);
CREATE TABLE page_headings (
    page_path TEXT NOT NULL REFERENCES pages(path) ON DELETE CASCADE,
    level     INTEGER NOT NULL,
    text      TEXT NOT NULL,
    anchor    TEXT NOT NULL,
    PRIMARY KEY (page_path, anchor)
);
CREATE TABLE page_relations (
    source_path TEXT NOT NULL REFERENCES pages(path) ON DELETE CASCADE,
    predicate   TEXT NOT NULL,
    link_text   TEXT NOT NULL,
    target_path TEXT,
    PRIMARY KEY (source_path, predicate, link_text)
);
CREATE INDEX idx_relations_target ON page_relations(target_path);
CREATE TABLE page_media (
    source_path TEXT NOT NULL REFERENCES pages(path) ON DELETE CASCADE,
    target      TEXT NOT NULL,
    PRIMARY KEY (source_path, target)
);
CREATE TABLE scan_errors (
    path  TEXT PRIMARY KEY,
    error TEXT NOT NULL
);
CREATE VIRTUAL TABLE pages_fts USING fts5(path UNINDEXED, title, summary, body);
CREATE TABLE index_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

/// World root for a campaign's vault path. Resolved by the `.ck/config.toml`
/// marker (vault itself, else its parent — `…/<World>/Codex` → `…/<World>`);
/// falls back on the folder name for not-yet-provisioned vaults. Adopted
/// foreign vaults (`codex_root = "."`) keep `.ck` inside the vault itself.
pub fn world_root_of(vault: &Path) -> &Path {
    if crate::world_config::config_path(vault).exists() {
        return vault;
    }
    if let Some(parent) = vault.parent() {
        if crate::world_config::config_path(parent).exists() {
            return parent;
        }
    }
    if vault.file_name().map(|n| n == "Codex").unwrap_or(false) {
        vault.parent().unwrap_or(vault)
    } else {
        vault
    }
}

pub fn index_db_path(vault: &Path) -> PathBuf {
    world_root_of(vault).join(".ck").join("index.db")
}

/// Open (creating/migrating as needed). Version mismatch → drop + recreate.
pub fn open_index(vault: &Path) -> AppResult<Connection> {
    let path = index_db_path(vault);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create .ck: {e}")))?;
    }
    let conn = Connection::open(&path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("open index.db: {e}")))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let version: Option<String> = conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .ok();
    if version.as_deref() != Some(SCHEMA_VERSION) {
        recreate(&conn)?;
    }
    Ok(conn)
}

fn recreate(conn: &Connection) -> AppResult<()> {
    let names: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT type, name FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%'",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.filter_map(Result::ok).collect()
    };
    for (ty, name) in names {
        // fts5 shadow tables drop with their virtual table; ignore failures.
        let _ = conn.execute_batch(&format!("DROP {ty} IF EXISTS \"{name}\""));
    }
    conn.execute_batch(SCHEMA)?;
    conn.execute(
        "INSERT INTO index_meta (key, value) VALUES ('schema_version', ?1), ('last_full_rebuild', '')",
        params![SCHEMA_VERSION],
    )?;
    Ok(())
}

// ── Parsing ───────────────────────────────────────────────────────

#[derive(Debug)]
pub struct RawLink {
    pub link_text: String,       // raw inside [[ ]], incl. #heading and |label
    pub target_name: String,     // normalized name segment
    pub heading: Option<String>, // anchor slug if [[Page#Heading]]
}

#[derive(Debug)]
pub struct ParsedPage {
    pub path: String,
    pub title: String,
    pub kind: Option<String>,
    pub summary: String,
    pub frontmatter_json: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub headings: Vec<(i64, String, String)>, // (level, text, anchor)
    pub links: Vec<RawLink>,
    pub relations: Vec<(String, String)>, // typed relations: (predicate, [[link]] inner text)
    pub media: Vec<String>,               // ![[file.ext]] embed targets (name only, no |size)
    pub content_hash: String,
    pub modified_at: i64,
}

// File embeds (`![[img.png]]`) are media; extensionless `![[Note]]`
// transclusions stay deferred.
fn is_media_target(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| !e.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

pub(crate) fn normalize_name(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    // NFC: macOS stores filenames NFD, link text is typically NFC.
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .nfc()
        .collect()
}

fn anchor_of(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
        .collect()
}

fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

// Body scan for headings + wikilinks, skipping fenced code blocks.
// `![[file.ext]]` embeds are collected as media refs; `![[Note]]`
// transclusions are skipped (deferred).
fn scan_body(
    body: &str,
    headings: &mut Vec<(i64, String, String)>,
    links: &mut Vec<RawLink>,
    media: &mut Vec<String>,
) {
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let level = 1 + rest.chars().take_while(|c| *c == '#').count();
            let text = rest.trim_start_matches('#').trim();
            if level <= 6 && !text.is_empty() && trimmed.as_bytes().get(level) == Some(&b' ') {
                let anchor = anchor_of(text);
                if !anchor.is_empty() && !headings.iter().any(|(_, _, a)| a == &anchor) {
                    headings.push((level as i64, text.to_string(), anchor));
                }
            }
        }
        let bytes = line.as_bytes();
        let mut i = 0;
        while let Some(start) = line[i..].find("[[").map(|p| p + i) {
            let embed = start > 0 && bytes.get(start - 1) == Some(&b'!');
            let Some(end) = line[start + 2..].find("]]").map(|p| p + start + 2) else {
                break;
            };
            let raw = &line[start + 2..end];
            i = end + 2;
            if raw.trim().is_empty() {
                continue;
            }
            let target = raw.split('|').next().unwrap_or(raw).trim();
            if embed {
                if is_media_target(target) && !media.iter().any(|m| m == target) {
                    media.push(target.to_string());
                }
                continue;
            }
            let (name, heading) = match target.split_once('#') {
                // Block refs ([[Page#^id]]) are a locked Drop — not indexed.
                Some((_, h)) if h.starts_with('^') => continue,
                Some((n, h)) => (n, Some(anchor_of(h))),
                None => (target, None),
            };
            let name = normalize_name(name);
            if name.is_empty() {
                continue;
            }
            if !links.iter().any(|l| l.link_text == raw) {
                links.push(RawLink {
                    link_text: raw.to_string(),
                    target_name: name,
                    heading: heading.filter(|h| !h.is_empty()),
                });
            }
        }
    }
}

fn rel_of(vault_root: &Path, abs: &Path) -> String {
    abs.strip_prefix(vault_root)
        .unwrap_or(abs)
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub fn parse_page(vault_root: &Path, abs: &Path, content: &str) -> ParsedPage {
    let (fm, body) = vault::split_frontmatter(content);
    let title = abs
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    let summary = vault::fm_get(&fm, "summary")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_default();
    let frontmatter_json = serde_json::Value::Object(
        fm.iter()
            .map(|(k, v)| {
                let val = if v.len() == 1 {
                    serde_json::Value::String(v[0].clone())
                } else {
                    serde_json::Value::Array(
                        v.iter().cloned().map(serde_json::Value::String).collect(),
                    )
                };
                (k.clone(), val)
            })
            .collect(),
    )
    .to_string();

    let mut aliases: Vec<String> = vec![normalize_name(&title)];
    for a in vault::fm_list(&fm, "aliases") {
        let n = normalize_name(a);
        if !n.is_empty() && !aliases.contains(&n) {
            aliases.push(n);
        }
    }
    let mut tags: Vec<String> = Vec::new();
    for t in vault::fm_list(&fm, "tags") {
        let t = t.trim().trim_start_matches('#').to_string();
        if !t.is_empty() && !tags.contains(&t) {
            tags.push(t);
        }
    }

    // Typed relations (Phase 9A): any frontmatter value that is a single
    // `[[wikilink]]` — the key is the predicate (`located_in: "[[Ashfall]]"`).
    let mut relations: Vec<(String, String)> = Vec::new();
    for (key, values) in &fm {
        if matches!(
            key.as_str(),
            "kind" | "aliases" | "tags" | "summary" | "cssclasses" | "publish" | "permalink"
        ) {
            continue;
        }
        for v in values {
            let v = v.trim();
            let Some(inner) = v.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) else {
                continue;
            };
            let inner = inner.trim();
            if inner.is_empty() || inner.contains("[[") || inner.starts_with("#^") {
                continue;
            }
            if !relations.iter().any(|(p, t)| p == key && t == inner) {
                relations.push((key.clone(), inner.to_string()));
            }
        }
    }

    let mut headings = Vec::new();
    let mut links = Vec::new();
    let mut media = Vec::new();
    scan_body(body, &mut headings, &mut links, &mut media);

    let rel = rel_of(vault_root, abs);
    let modified_at = abs
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    ParsedPage {
        path: rel,
        title,
        kind: vault::fm_get(&fm, "kind")
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        summary,
        frontmatter_json,
        aliases,
        tags,
        headings,
        links,
        relations,
        media,
        content_hash: fnv1a(content.as_bytes()),
        modified_at,
    }
}

// ── Writes ────────────────────────────────────────────────────────

fn insert_page(conn: &Connection, p: &ParsedPage, body: &str) -> AppResult<()> {
    // INSERT OR REPLACE deletes the old row → CASCADE wipes child rows.
    conn.execute(
        "INSERT OR REPLACE INTO pages (path, title, kind, summary, frontmatter, content_hash, modified_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![p.path, p.title, p.kind, p.summary, p.frontmatter_json, p.content_hash, p.modified_at],
    )?;
    for alias in &p.aliases {
        conn.execute(
            "INSERT OR IGNORE INTO page_aliases (alias, page_path) VALUES (?1, ?2)",
            params![alias, p.path],
        )?;
    }
    for tag in &p.tags {
        conn.execute(
            "INSERT OR IGNORE INTO page_tags (page_path, tag) VALUES (?1, ?2)",
            params![p.path, tag],
        )?;
    }
    for (level, text, anchor) in &p.headings {
        conn.execute(
            "INSERT OR IGNORE INTO page_headings (page_path, level, text, anchor) VALUES (?1, ?2, ?3, ?4)",
            params![p.path, level, text, anchor],
        )?;
    }
    for link in &p.links {
        conn.execute(
            "INSERT OR IGNORE INTO page_links (source_path, target_path, link_text, heading) VALUES (?1, NULL, ?2, ?3)",
            params![p.path, link.link_text, link.heading],
        )?;
    }
    for (predicate, link_text) in &p.relations {
        conn.execute(
            "INSERT OR IGNORE INTO page_relations (source_path, predicate, link_text, target_path) VALUES (?1, ?2, ?3, NULL)",
            params![p.path, predicate, link_text],
        )?;
    }
    for target in &p.media {
        conn.execute(
            "INSERT OR IGNORE INTO page_media (source_path, target) VALUES (?1, ?2)",
            params![p.path, target],
        )?;
    }
    conn.execute("DELETE FROM scan_errors WHERE path = ?1", params![p.path])?;
    conn.execute("DELETE FROM pages_fts WHERE path = ?1", params![p.path])?;
    conn.execute(
        "INSERT INTO pages_fts (path, title, summary, body) VALUES (?1, ?2, ?3, ?4)",
        params![p.path, p.title, p.summary, body],
    )?;
    Ok(())
}

/// Re-index one page from disk (or drop it if the file is gone).
pub fn upsert_path(conn: &Connection, vault_root: &Path, rel: &str) -> AppResult<()> {
    let abs = vault_root.join(rel);
    let content = match std::fs::read_to_string(&abs) {
        Ok(c) => c,
        Err(e) => {
            remove_path(conn, rel)?;
            if abs.exists() {
                // Unreadable (e.g. not UTF-8) — surface in diagnostics.
                conn.execute(
                    "INSERT OR REPLACE INTO scan_errors (path, error) VALUES (?1, ?2)",
                    params![rel, e.to_string()],
                )?;
            }
            return Ok(());
        }
    };
    let parsed = parse_page(vault_root, &abs, &content);
    let (_, body) = vault::split_frontmatter(&content);
    insert_page(conn, &parsed, body)?;
    resolve_links(conn)
}

pub fn remove_path(conn: &Connection, rel: &str) -> AppResult<()> {
    conn.execute("DELETE FROM pages WHERE path = ?1", params![rel])?;
    conn.execute("DELETE FROM pages_fts WHERE path = ?1", params![rel])?;
    conn.execute("DELETE FROM scan_errors WHERE path = ?1", params![rel])?;
    resolve_links(conn)
}

/// Full scan with content_hash skip. Idempotent, cheap on a warm index.
pub fn rebuild(conn: &Connection, vault_root: &Path) -> AppResult<()> {
    let mut files = Vec::new();
    collect_md(vault_root, &mut files);
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM scan_errors", [])?;
    let mut seen: Vec<String> = Vec::with_capacity(files.len());
    for abs in &files {
        let content = match std::fs::read_to_string(abs) {
            Ok(c) => c,
            Err(e) => {
                let rel = rel_of(vault_root, abs);
                tx.execute(
                    "INSERT OR REPLACE INTO scan_errors (path, error) VALUES (?1, ?2)",
                    params![rel, e.to_string()],
                )?;
                continue;
            }
        };
        let parsed = parse_page(vault_root, abs, &content);
        seen.push(parsed.path.clone());
        let existing: Option<String> = tx
            .query_row(
                "SELECT content_hash FROM pages WHERE path = ?1",
                params![parsed.path],
                |r| r.get(0),
            )
            .ok();
        if existing.as_deref() == Some(parsed.content_hash.as_str()) {
            continue;
        }
        let (_, body) = vault::split_frontmatter(&content);
        insert_page(&tx, &parsed, body)?;
    }
    // Drop rows whose file vanished.
    let stale: Vec<String> = {
        let mut stmt = tx.prepare("SELECT path FROM pages")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.filter_map(Result::ok)
            .filter(|p| !seen.contains(p))
            .collect()
    };
    for path in &stale {
        tx.execute("DELETE FROM pages WHERE path = ?1", params![path])?;
        tx.execute("DELETE FROM pages_fts WHERE path = ?1", params![path])?;
    }
    resolve_links(&tx)?;
    tx.execute(
        "INSERT OR REPLACE INTO index_meta (key, value) VALUES ('last_full_rebuild', ?1)",
        params![crate::store::now()],
    )?;
    tx.commit()?;
    Ok(())
}

fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if !vault::is_reserved_dir(&name) {
                collect_md(&path, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md")
            && !vault::is_reserved_page(&name)
        {
            out.push(path);
        }
    }
}

/// Re-resolve every link's target against pages + aliases. Collision rule:
/// shortest path, then lexicographic. `[[Page#Heading]]` also needs the anchor.
pub fn resolve_links(conn: &Connection) -> AppResult<()> {
    let mut by_alias: HashMap<String, Vec<String>> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT alias, page_path FROM page_aliases")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for (alias, path) in rows.filter_map(Result::ok) {
            by_alias.entry(alias).or_default().push(path);
        }
    }
    for paths in by_alias.values_mut() {
        paths.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    }
    let anchors: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT page_path, anchor FROM page_headings")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.filter_map(Result::ok).collect()
    };
    let links: Vec<(String, String, Option<String>)> = {
        let mut stmt = conn.prepare("SELECT source_path, link_text, heading FROM page_links")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.filter_map(Result::ok).collect()
    };
    for (source, link_text, heading) in links {
        let target = link_text.split('|').next().unwrap_or(&link_text);
        let name = normalize_name(target.split('#').next().unwrap_or(target));
        let resolved = by_alias.get(&name).and_then(|paths| {
            paths
                .iter()
                .find(|p| match &heading {
                    Some(h) => anchors.iter().any(|(pp, a)| pp == *p && a == h),
                    None => true,
                })
                .cloned()
        });
        conn.execute(
            "UPDATE page_links SET target_path = ?1 WHERE source_path = ?2 AND link_text = ?3",
            params![resolved, source, link_text],
        )?;
    }
    // Typed relations resolve the same way (no heading requirement).
    let rels: Vec<(String, String, String)> = {
        let mut stmt =
            conn.prepare("SELECT source_path, predicate, link_text FROM page_relations")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.filter_map(Result::ok).collect()
    };
    for (source, predicate, link_text) in rels {
        let target = link_text.split('|').next().unwrap_or(&link_text);
        let name = normalize_name(target.split('#').next().unwrap_or(target));
        let resolved = by_alias.get(&name).and_then(|paths| paths.first().cloned());
        conn.execute(
            "UPDATE page_relations SET target_path = ?1 WHERE source_path = ?2 AND predicate = ?3 AND link_text = ?4",
            params![resolved, source, predicate, link_text],
        )?;
    }
    Ok(())
}

// ── Queries ───────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct LinkRow {
    pub source_path: String,
    pub target_path: Option<String>,
    pub link_text: String,
}

pub fn all_links(conn: &Connection) -> AppResult<Vec<LinkRow>> {
    let mut stmt = conn.prepare(
        "SELECT source_path, target_path, link_text FROM page_links ORDER BY source_path",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(LinkRow {
            source_path: r.get(0)?,
            target_path: r.get(1)?,
            link_text: r.get(2)?,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub fn unresolved_count(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM page_links WHERE target_path IS NULL",
        [],
        |r| r.get(0),
    )?)
}

/// Pages with no inbound resolved link. (Session mentions not yet considered.)
pub fn orphan_count(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM pages p WHERE NOT EXISTS \
         (SELECT 1 FROM page_links l WHERE l.target_path = p.path)",
        [],
        |r| r.get(0),
    )?)
}

#[derive(Debug, serde::Serialize)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub kind: Option<String>,
    pub summary: Option<String>,
    pub snippet: String,
}

/// Optional facets narrowing a full-text search. Applied in SQL so ranking and
/// the 50-hit cap stay correct (client-side filtering would drop capped hits).
#[derive(Debug, Default)]
pub struct SearchFacets {
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub folder: Option<String>,
    pub edited_after: Option<i64>,
    pub edited_before: Option<i64>,
}

impl SearchFacets {
    pub fn is_empty(&self) -> bool {
        self.kind.is_none()
            && self.tag.is_none()
            && self.folder.is_none()
            && self.edited_after.is_none()
            && self.edited_before.is_none()
    }
}

pub fn search(conn: &Connection, q: &str) -> AppResult<Vec<SearchHit>> {
    search_faceted(conn, q, &SearchFacets::default())
}

pub fn search_faceted(
    conn: &Connection,
    q: &str,
    facets: &SearchFacets,
) -> AppResult<Vec<SearchHit>> {
    let q = q.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    // Quote each token as a phrase + prefix star: FTS5 operators in user input
    // (-, ", :, [) would otherwise be syntax errors.
    let fts_query = q
        .split_whitespace()
        .map(|t| format!("\"{}\"*", t.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ");

    let mut sql = String::from(
        "SELECT f.path, p.title, p.kind, p.summary, snippet(pages_fts, 3, '<b>', '</b>', '…', 12) \
         FROM pages_fts f JOIN pages p ON p.path = f.path \
         WHERE pages_fts MATCH ?1",
    );
    let mut binds: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(fts_query)];
    if let Some(kind) = &facets.kind {
        binds.push(Box::new(kind.clone()));
        sql.push_str(&format!(" AND p.kind = ?{}", binds.len()));
    }
    if let Some(tag) = &facets.tag {
        binds.push(Box::new(tag.clone()));
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM page_tags t WHERE t.page_path = f.path AND t.tag = ?{})",
            binds.len()
        ));
    }
    if let Some(folder) = &facets.folder {
        binds.push(Box::new(format!("{}/%", folder.trim_end_matches('/'))));
        sql.push_str(&format!(" AND f.path LIKE ?{}", binds.len()));
    }
    if let Some(after) = facets.edited_after {
        binds.push(Box::new(after));
        sql.push_str(&format!(" AND p.modified_at >= ?{}", binds.len()));
    }
    if let Some(before) = facets.edited_before {
        binds.push(Box::new(before));
        sql.push_str(&format!(" AND p.modified_at <= ?{}", binds.len()));
    }
    sql.push_str(" ORDER BY rank LIMIT 50");

    let mut stmt = conn.prepare(&sql)?;
    let bind_refs: Vec<&dyn rusqlite::types::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(bind_refs.as_slice(), |r| {
        Ok(SearchHit {
            path: r.get(0)?,
            title: r.get(1)?,
            kind: r.get(2)?,
            summary: r.get(3)?,
            snippet: r.get(4)?,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub fn tag_counts(conn: &Connection) -> AppResult<Vec<(String, i64)>> {
    let mut stmt = conn
        .prepare("SELECT tag, COUNT(*) FROM page_tags GROUP BY tag ORDER BY COUNT(*) DESC, tag")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// path → (aliases beyond the title, tags) for enriching page lists.
#[allow(clippy::type_complexity)]
pub fn page_meta(conn: &Connection) -> AppResult<HashMap<String, (Vec<String>, Vec<String>)>> {
    let mut out: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT a.page_path, a.alias FROM page_aliases a JOIN pages p ON p.path = a.page_path \
             WHERE a.alias <> lower(p.title)",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for (path, alias) in rows.filter_map(Result::ok) {
            out.entry(path).or_default().0.push(alias);
        }
    }
    {
        let mut stmt = conn.prepare("SELECT page_path, tag FROM page_tags")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for (path, tag) in rows.filter_map(Result::ok) {
            out.entry(path).or_default().1.push(tag);
        }
    }
    Ok(out)
}

#[derive(Debug, serde::Serialize)]
pub struct Diagnostics {
    pub broken_links: Vec<BrokenLink>,
    pub orphans: Vec<OrphanPage>,
    pub broken_media: Vec<BrokenMedia>,
    pub scan_errors: Vec<ScanError>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct BrokenLink {
    pub source_path: String,
    pub link_text: String,
}

#[derive(Debug, serde::Serialize)]
pub struct OrphanPage {
    pub path: String,
    pub title: String,
    pub kind: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct BrokenMedia {
    pub source_path: String,
    pub target: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ScanError {
    pub path: String,
    pub error: String,
}

// Non-md vault files (for media resolution) + sync-conflict filenames,
// in one walk. Same scope rules as collect_md.
fn collect_diag_files(
    dir: &Path,
    root: &Path,
    media: &mut Vec<String>,
    conflicts: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if !vault::is_reserved_dir(&name) {
                collect_diag_files(&path, root, media, conflicts);
            }
            continue;
        }
        let lower = name.to_lowercase();
        if lower.contains(".sync-conflict-") || lower.contains("conflicted copy") {
            conflicts.push(rel_of(root, &path));
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            media.push(rel_of(root, &path));
        }
    }
}

/// One row of `all_frontmatter`: (path, title, kind, frontmatter_json).
pub type PageFrontmatter = (String, String, Option<String>, String);

/// Every page's stored frontmatter — the timeline extracts `date:` from it.
pub fn all_frontmatter(conn: &Connection) -> AppResult<Vec<PageFrontmatter>> {
    let mut stmt =
        conn.prepare("SELECT path, title, kind, COALESCE(frontmatter, '{}') FROM pages")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    Ok(rows.filter_map(Result::ok).collect())
}

// ── Dataview-lite queries (Phase 9C) ──────────────────────────────
// `LIST FROM #npc AND kind:place WHERE location = [[Ashfall]] AND status != dead`
// Read-only; evaluated over indexed tags + frontmatter. Values compare
// wikilink-insensitively (`[[Ashfall|the city]]` == `Ashfall`).

#[derive(Debug)]
enum Cond {
    Eq(String, String),
    Ne(String, String),
    Contains(String, String),
}

#[derive(Debug, serde::Serialize)]
pub struct QueryHit {
    pub path: String,
    pub title: String,
    pub kind: Option<String>,
    pub summary: String,
}

// `[[Ashfall|the city]]` / `"Ashfall"` / `Ashfall` → comparable key.
fn query_norm(v: &str) -> String {
    let v = v.trim().trim_matches('"').trim();
    let v = v
        .strip_prefix("[[")
        .and_then(|s| s.strip_suffix("]]"))
        .unwrap_or(v);
    let v = v.split('|').next().unwrap_or(v);
    let v = v.split('#').next().unwrap_or(v);
    normalize_name(v)
}

// (tags, kinds, conditions)
type ParsedQuery = (Vec<String>, Vec<String>, Vec<Cond>);

fn parse_query(q: &str) -> Result<ParsedQuery, String> {
    let s = q.trim();
    let s = if s.len() >= 4 && s[..4].eq_ignore_ascii_case("list") {
        s[4..].trim()
    } else {
        s
    };
    if s.is_empty() {
        return Err("Empty query — try `LIST FROM #npc`".into());
    }
    // Split off WHERE (case-insensitive, whole word, byte-safe vs the original).
    let lower = s.to_lowercase();
    let widx = {
        let mut found = None;
        let mut start = 0;
        while let Some(i) = lower[start..].find("where").map(|p| p + start) {
            let before_ok = i == 0 || lower.as_bytes()[i - 1] == b' ';
            let after_ok = lower.as_bytes().get(i + 5).is_none_or(|b| *b == b' ');
            if before_ok && after_ok && s.get(..i).is_some() && s.get(i + 5..).is_some() {
                found = Some(i);
                break;
            }
            start = i + 5;
        }
        found
    };
    let (from_part, where_part) = match widx {
        Some(i) => (&s[..i], Some(&s[i + 5..])),
        None => (s, None),
    };
    let from_part = from_part.trim();
    let from_part = if from_part.len() >= 4 && from_part[..4].eq_ignore_ascii_case("from") {
        from_part[4..].trim()
    } else if from_part.is_empty() {
        from_part
    } else {
        return Err(format!("Expected FROM or WHERE, got “{from_part}”"));
    };

    let split_and = |part: &str| -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = part.trim();
        loop {
            let lc = rest.to_lowercase();
            match lc
                .find(" and ")
                .filter(|i| rest.get(..*i).is_some() && rest.get(i + 5..).is_some())
            {
                Some(i) => {
                    out.push(rest[..i].trim().to_string());
                    rest = &rest[i + 5..];
                }
                None => break,
            }
        }
        if !rest.trim().is_empty() {
            out.push(rest.trim().to_string());
        }
        out
    };

    let mut tags = Vec::new();
    let mut kinds = Vec::new();
    for term in split_and(from_part) {
        if let Some(t) = term.strip_prefix('#') {
            tags.push(t.to_lowercase());
        } else if let Some(k) = term.to_lowercase().strip_prefix("kind:") {
            kinds.push(k.trim().to_string());
        } else {
            return Err(format!("FROM terms are #tag or kind:<kind>, got “{term}”"));
        }
    }

    let mut conds = Vec::new();
    if let Some(w) = where_part {
        for c in split_and(w) {
            let cond = if let Some((f, v)) = c.split_once("!=") {
                Cond::Ne(f.trim().to_string(), query_norm(v))
            } else if let Some((f, v)) = c.split_once('=') {
                Cond::Eq(f.trim().to_string(), query_norm(v))
            } else {
                let lc = c.to_lowercase();
                match lc.find(" contains ").filter(|i| c.get(..*i).is_some() && c.get(i + 10..).is_some()) {
                    Some(i) => Cond::Contains(c[..i].trim().to_string(), query_norm(&c[i + 10..])),
                    None => return Err(format!("WHERE conditions are `field = value`, `field != value` or `field contains value`, got “{c}”")),
                }
            };
            conds.push(cond);
        }
    }
    if tags.is_empty() && kinds.is_empty() && conds.is_empty() {
        return Err("Query needs a FROM or WHERE clause".into());
    }
    Ok((tags, kinds, conds))
}

/// Run a dataview-lite query. `Err` carries a user-facing parse message.
pub fn run_query(conn: &Connection, q: &str) -> AppResult<Result<Vec<QueryHit>, String>> {
    let (tags, kinds, conds) = match parse_query(q) {
        Ok(p) => p,
        Err(e) => return Ok(Err(e)),
    };
    let mut tag_map: HashMap<String, Vec<String>> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT page_path, tag FROM page_tags")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for (path, tag) in rows.filter_map(Result::ok) {
            tag_map.entry(path).or_default().push(tag.to_lowercase());
        }
    }
    let mut hits = Vec::new();
    let mut stmt =
        conn.prepare("SELECT path, title, kind, summary, COALESCE(frontmatter, '{}') FROM pages ORDER BY title COLLATE NOCASE")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    for (path, title, kind, summary, fm_json) in rows.filter_map(Result::ok) {
        let page_tags = tag_map.get(&path).cloned().unwrap_or_default();
        // a page tag matches the query tag or any parent segment (`character/ranger` matches #character)
        if !tags.iter().all(|t| {
            page_tags
                .iter()
                .any(|pt| pt == t || pt.starts_with(&format!("{t}/")))
        }) {
            continue;
        }
        if !kinds.is_empty() && !kinds.iter().any(|k| kind.as_deref() == Some(k)) {
            continue;
        }
        let fm: serde_json::Value = serde_json::from_str(&fm_json).unwrap_or_default();
        let field_vals = |f: &str| -> Vec<String> {
            match &fm[f] {
                serde_json::Value::String(s) => vec![query_norm(s)],
                serde_json::Value::Array(a) => a
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(query_norm)
                    .collect(),
                _ => Vec::new(),
            }
        };
        let ok = conds.iter().all(|c| match c {
            Cond::Eq(f, v) => field_vals(f).iter().any(|x| x == v),
            Cond::Ne(f, v) => !field_vals(f).iter().any(|x| x == v),
            Cond::Contains(f, v) => field_vals(f).iter().any(|x| x.contains(v.as_str())),
        });
        if !ok {
            continue;
        }
        hits.push(QueryHit {
            path,
            title,
            kind,
            summary,
        });
    }
    Ok(Ok(hits))
}

/// Everything the diagnostics panel shows. Index reads + one fs walk
/// (media existence, conflict filenames).
pub fn diagnostics(conn: &Connection, vault_root: &Path) -> AppResult<Diagnostics> {
    let broken_links = {
        let mut stmt = conn.prepare(
            "SELECT source_path, link_text FROM page_links WHERE target_path IS NULL \
             ORDER BY source_path, link_text",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(BrokenLink {
                source_path: r.get(0)?,
                link_text: r.get(1)?,
            })
        })?;
        rows.filter_map(Result::ok).collect()
    };
    let orphans = {
        let mut stmt = conn.prepare(
            "SELECT path, title, kind FROM pages p WHERE NOT EXISTS \
             (SELECT 1 FROM page_links l WHERE l.target_path = p.path) ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(OrphanPage {
                path: r.get(0)?,
                title: r.get(1)?,
                kind: r.get(2)?,
            })
        })?;
        rows.filter_map(Result::ok).collect()
    };
    let scan_errors = {
        let mut stmt = conn.prepare("SELECT path, error FROM scan_errors ORDER BY path")?;
        let rows = stmt.query_map([], |r| {
            Ok(ScanError {
                path: r.get(0)?,
                error: r.get(1)?,
            })
        })?;
        rows.filter_map(Result::ok).collect()
    };

    let mut files = Vec::new();
    let mut conflicts = Vec::new();
    collect_diag_files(vault_root, vault_root, &mut files, &mut conflicts);
    conflicts.sort();
    // Resolve like Obsidian: exact relative path, else filename match anywhere.
    let norm = |s: &str| -> String {
        use unicode_normalization::UnicodeNormalization;
        s.to_lowercase().nfc().collect()
    };
    let rel_set: std::collections::HashSet<String> = files.iter().map(|f| norm(f)).collect();
    let name_set: std::collections::HashSet<String> = files
        .iter()
        .filter_map(|f| f.rsplit('/').next())
        .map(norm)
        .collect();
    let broken_media = {
        let mut stmt = conn
            .prepare("SELECT source_path, target FROM page_media ORDER BY source_path, target")?;
        let rows = stmt.query_map([], |r| {
            Ok(BrokenMedia {
                source_path: r.get(0)?,
                target: r.get(1)?,
            })
        })?;
        rows.filter_map(Result::ok)
            .filter(|m| {
                let t = norm(&m.target);
                !rel_set.contains(&t)
                    && !t
                        .rsplit('/')
                        .next()
                        .map(|n| name_set.contains(n))
                        .unwrap_or(false)
            })
            .collect()
    };

    Ok(Diagnostics {
        broken_links,
        orphans,
        broken_media,
        scan_errors,
        conflicts,
    })
}

/// Sources linking to `rel` (resolved), with the raw link text — rename uses
/// this. Includes frontmatter relations so renames rewrite those values too.
pub fn sources_linking_to(conn: &Connection, rel: &str) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT source_path, link_text FROM page_links WHERE target_path = ?1 \
         UNION SELECT source_path, link_text FROM page_relations WHERE target_path = ?1",
    )?;
    let rows = stmt.query_map(params![rel], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[derive(Debug, serde::Serialize)]
pub struct RelationRow {
    pub source_path: String,
    pub predicate: String,
    pub link_text: String,
    pub target_path: Option<String>,
}

/// Every typed relation in the world (graph edges + reverse-relation rail).
pub fn all_relations(conn: &Connection) -> AppResult<Vec<RelationRow>> {
    let mut stmt = conn.prepare(
        "SELECT source_path, predicate, link_text, target_path FROM page_relations \
         ORDER BY source_path, predicate, link_text",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(RelationRow {
            source_path: r.get(0)?,
            predicate: r.get(1)?,
            link_text: r.get(2)?,
            target_path: r.get(3)?,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Rewrite the name segment of every `[[old_name…]]` to `new_name`, preserving
/// `#heading` and `|label`. Case-insensitive on the name; writes the new
/// title's exact casing. `None` when nothing matched.
pub fn rewrite_link_names(content: &str, old_name: &str, new_name: &str) -> Option<String> {
    let target = normalize_name(old_name);
    let mut out = String::with_capacity(content.len() + 16);
    let mut changed = false;
    let mut rest = content;
    while let Some(start) = rest.find("[[") {
        let (before, after) = rest.split_at(start + 2);
        out.push_str(before);
        let Some(end) = after.find("]]") else {
            rest = after;
            break;
        };
        let inner = &after[..end];
        let name_end = inner.find(['#', '|']).unwrap_or(inner.len());
        let (name, tail) = inner.split_at(name_end);
        if normalize_name(name) == target {
            out.push_str(new_name);
            out.push_str(tail);
            changed = true;
        } else {
            out.push_str(inner);
        }
        out.push_str("]]");
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-index-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, rel: &str, content: &str) {
        let abs = dir.join(rel);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(abs, content).unwrap();
    }

    #[test]
    fn parse_extracts_everything() {
        let dir = tmp_vault("parse");
        write(
            &dir,
            "Characters/Aragorn.md",
            "---\nkind: npc\nsummary: A ranger.\naliases:\n  - Strider\ntags: [character/ranger]\n---\n\n# Aragorn\n\nMet at [[Rivendell#Geography|the valley]] with [[Gandalf]].\n\n## Background\n\n```\n[[NotALink]]\n```\n![[portrait.png]]\n",
        );
        let abs = dir.join("Characters/Aragorn.md");
        let content = std::fs::read_to_string(&abs).unwrap();
        let p = parse_page(&dir, &abs, &content);
        assert_eq!(p.path, "Characters/Aragorn.md");
        assert_eq!(p.title, "Aragorn");
        assert_eq!(p.kind.as_deref(), Some("npc"));
        assert_eq!(p.aliases, ["aragorn", "strider"]);
        assert_eq!(p.tags, ["character/ranger"]);
        assert_eq!(
            p.headings.iter().map(|h| h.2.as_str()).collect::<Vec<_>>(),
            ["aragorn", "background"]
        );
        let names: Vec<&str> = p.links.iter().map(|l| l.target_name.as_str()).collect();
        assert_eq!(names, ["rivendell", "gandalf"]);
        assert_eq!(p.links[0].heading.as_deref(), Some("geography"));
        assert_eq!(p.media, ["portrait.png"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn relations_index_resolve_rename_and_query() {
        let dir = tmp_vault("relations");
        write(&dir, "Ashfall.md", "---\nkind: place\n---\n# Ashfall\n");
        write(
            &dir,
            "Kel.md",
            "---\nkind: npc\nlocation: \"[[Ashfall|the city]]\"\ntags: [npc]\n---\n# Kel\n",
        );
        write(
            &dir,
            "Mara.md",
            "---\nkind: npc\nlocation: \"[[Ashfall]]\"\nstatus: alive\nallies:\n  - \"[[Kel]]\"\n  - \"[[Missing]]\"\ntags: [npc]\n---\n# Mara\n",
        );
        let conn = open_index(&dir).unwrap();
        rebuild(&conn, &dir).unwrap();

        let rels = all_relations(&conn).unwrap();
        let find = |src: &str, pred: &str, txt: &str| {
            rels.iter()
                .find(|r| r.source_path == src && r.predicate == pred && r.link_text == txt)
                .unwrap()
        };
        assert_eq!(
            find("Mara.md", "location", "Ashfall")
                .target_path
                .as_deref(),
            Some("Ashfall.md")
        );
        assert_eq!(
            find("Kel.md", "location", "Ashfall|the city")
                .target_path
                .as_deref(),
            Some("Ashfall.md")
        );
        assert_eq!(
            find("Mara.md", "allies", "Kel").target_path.as_deref(),
            Some("Kel.md")
        );
        assert_eq!(find("Mara.md", "allies", "Missing").target_path, None);

        // rename rewrite picks up frontmatter-only linkers
        let srcs = sources_linking_to(&conn, "Ashfall.md").unwrap();
        assert!(srcs.iter().any(|(s, _)| s == "Mara.md"));
        assert!(srcs.iter().any(|(s, _)| s == "Kel.md"));

        // dataview-lite
        let hits = run_query(&conn, "LIST FROM #npc WHERE location = [[Ashfall]]")
            .unwrap()
            .unwrap();
        let titles: Vec<&str> = hits.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, ["Kel", "Mara"]);
        let hits = run_query(&conn, "FROM kind:npc WHERE allies contains kel")
            .unwrap()
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Mara");
        let hits = run_query(&conn, "LIST FROM kind:npc WHERE status != alive")
            .unwrap()
            .unwrap();
        assert_eq!(hits[0].title, "Kel"); // absent field passes !=
        assert!(run_query(&conn, "LIST nonsense").unwrap().is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rebuild_resolves_and_breaks_links() {
        let dir = tmp_vault("rebuild");
        write(
            &dir,
            "Aragorn.md",
            "---\nkind: npc\naliases:\n  - Strider\n---\nSee [[Rivendell]] and [[Nowhere]].\n",
        );
        write(
            &dir,
            "Rivendell.md",
            "---\nkind: place\n---\nHome of [[Strider]].\n## Geography\nHills.\n",
        );
        let conn = open_index(&dir).unwrap();
        rebuild(&conn, &dir).unwrap();

        let links = all_links(&conn).unwrap();
        assert_eq!(links.len(), 3);
        let find = |src: &str, txt: &str| {
            links
                .iter()
                .find(|l| l.source_path == src && l.link_text == txt)
                .unwrap()
        };
        assert_eq!(
            find("Aragorn.md", "Rivendell").target_path.as_deref(),
            Some("Rivendell.md")
        );
        assert_eq!(find("Aragorn.md", "Nowhere").target_path, None);
        // alias resolution
        assert_eq!(
            find("Rivendell.md", "Strider").target_path.as_deref(),
            Some("Aragorn.md")
        );
        assert_eq!(unresolved_count(&conn).unwrap(), 1);

        // hash-skip rebuild is a no-op; removing a file drops its rows
        rebuild(&conn, &dir).unwrap();
        assert_eq!(all_links(&conn).unwrap().len(), 3);
        std::fs::remove_file(dir.join("Rivendell.md")).unwrap();
        rebuild(&conn, &dir).unwrap();
        assert_eq!(find("Aragorn.md", "Rivendell").link_text, "Rivendell"); // stale local copy
        let links = all_links(&conn).unwrap();
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|l| l.source_path == "Aragorn.md"));
        assert_eq!(unresolved_count(&conn).unwrap(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolves_nfd_filename_against_nfc_link() {
        // macOS stores filenames NFD; typed link text is NFC.
        let dir = tmp_vault("nfc");
        write(
            &dir,
            "Gefa\u{308}ngnis.md",
            "---\nkind: place\n---\nA jail.\n",
        ); // NFD ä
        write(&dir, "Source.md", "See [[Gef\u{e4}ngnis]].\n"); // NFC ä
        let conn = open_index(&dir).unwrap();
        rebuild(&conn, &dir).unwrap();
        assert_eq!(unresolved_count(&conn).unwrap(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn heading_links_need_anchor() {
        let dir = tmp_vault("heading");
        write(&dir, "A.md", "[[B#Real]] and [[B#Fake]]\n");
        write(&dir, "B.md", "## Real\ntext\n");
        let conn = open_index(&dir).unwrap();
        rebuild(&conn, &dir).unwrap();
        let links = all_links(&conn).unwrap();
        let real = links.iter().find(|l| l.link_text == "B#Real").unwrap();
        let fake = links.iter().find(|l| l.link_text == "B#Fake").unwrap();
        assert_eq!(real.target_path.as_deref(), Some("B.md"));
        assert_eq!(fake.target_path, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn search_and_tags() {
        let dir = tmp_vault("search");
        write(&dir, "Rivendell.md", "---\nkind: place\nsummary: Elf haven.\ntags: [Location/City]\n---\nThe hidden valley of the elves.\n");
        write(
            &dir,
            "Moria.md",
            "---\ntags: [Location/Dungeon]\n---\nDark dwarven halls.\n",
        );
        let conn = open_index(&dir).unwrap();
        rebuild(&conn, &dir).unwrap();
        let hits = search(&conn, "valley elv").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "Rivendell.md");
        assert!(search(&conn, "\"weird:[query]").unwrap().is_empty()); // no syntax error
        let tags = tag_counts(&conn).unwrap();
        assert_eq!(tags.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reserved_pages_not_indexed() {
        let dir = tmp_vault("reserved");
        write(&dir, "AGENTS.md", "Always answer in German please.\n");
        write(
            &dir,
            "Rivendell.md",
            "---\nkind: place\n---\nElf haven valley.\n",
        );
        let conn = open_index(&dir).unwrap();
        rebuild(&conn, &dir).unwrap();
        assert!(search(&conn, "German").unwrap().is_empty());
        let hits = search(&conn, "valley").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "Rivendell.md");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn faceted_search_narrows() {
        let dir = tmp_vault("facets");
        write(
            &dir,
            "Towns/Bree.md",
            "---\nkind: place\ntags: [town]\n---\nA muddy crossroads town.\n",
        );
        write(
            &dir,
            "People/Barliman.md",
            "---\nkind: npc\ntags: [town]\n---\nInnkeeper of the muddy town.\n",
        );
        let conn = open_index(&dir).unwrap();
        rebuild(&conn, &dir).unwrap();

        // bare query matches both
        assert_eq!(search(&conn, "muddy").unwrap().len(), 2);
        // kind facet
        let f = SearchFacets {
            kind: Some("place".into()),
            ..Default::default()
        };
        let hits = search_faceted(&conn, "muddy", &f).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "Towns/Bree.md");
        // folder facet (prefix)
        let f = SearchFacets {
            folder: Some("People".into()),
            ..Default::default()
        };
        let hits = search_faceted(&conn, "muddy", &f).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "People/Barliman.md");
        // tag facet matches both
        let f = SearchFacets {
            tag: Some("town".into()),
            ..Default::default()
        };
        assert_eq!(search_faceted(&conn, "muddy", &f).unwrap().len(), 2);
        // date facet in the future excludes everything
        let f = SearchFacets {
            edited_after: Some(i64::MAX),
            ..Default::default()
        };
        assert!(search_faceted(&conn, "muddy", &f).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn collision_prefers_shortest_path() {
        let dir = tmp_vault("collision");
        write(&dir, "Gil.md", "x\n");
        write(&dir, "Deep/Nested/Gil.md", "y\n");
        write(&dir, "Ref.md", "[[Gil]]\n");
        let conn = open_index(&dir).unwrap();
        rebuild(&conn, &dir).unwrap();
        let links = all_links(&conn).unwrap();
        let l = links.iter().find(|l| l.link_text == "Gil").unwrap();
        assert_eq!(l.target_path.as_deref(), Some("Gil.md"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rewrite_preserves_heading_and_label() {
        let content = "See [[Aragorn]] and [[aragorn#Background|the king]], not [[Aragorn II]].\n";
        let out = rewrite_link_names(content, "Aragorn", "Elessar").unwrap();
        assert_eq!(
            out,
            "See [[Elessar]] and [[Elessar#Background|the king]], not [[Aragorn II]].\n"
        );
        assert!(rewrite_link_names("no links here", "Aragorn", "Elessar").is_none());
        assert!(rewrite_link_names("[[Gandalf]]", "Aragorn", "Elessar").is_none());
    }

    #[test]
    fn diagnostics_reports_all_groups() {
        let dir = tmp_vault("diag");
        write(
            &dir,
            "Aragorn.md",
            "See [[Nowhere]].\n![[portrait.png]]\n![[Assets/map.jpg|640]]\n![[missing.png]]\n",
        );
        write(&dir, "Orphan.md", "alone\n");
        write(
            &dir,
            "Notes.sync-conflict-20260603-ABCDEF.md",
            "conflict copy\n",
        );
        std::fs::write(dir.join("portrait.png"), b"png").unwrap();
        std::fs::create_dir_all(dir.join("Assets")).unwrap();
        std::fs::write(dir.join("Assets/map.jpg"), b"jpg").unwrap();
        std::fs::write(dir.join("Broken.md"), [0xff, 0xfe, 0x00]).unwrap(); // not UTF-8
        let conn = open_index(&dir).unwrap();
        rebuild(&conn, &dir).unwrap();
        let d = diagnostics(&conn, &dir).unwrap();
        assert_eq!(d.broken_links.len(), 1);
        assert_eq!(d.broken_links[0].link_text, "Nowhere");
        assert!(d.orphans.iter().any(|o| o.path == "Orphan.md"));
        assert_eq!(d.broken_media.len(), 1);
        assert_eq!(d.broken_media[0].target, "missing.png");
        assert_eq!(d.scan_errors.len(), 1);
        assert_eq!(d.scan_errors[0].path, "Broken.md");
        assert_eq!(d.conflicts, ["Notes.sync-conflict-20260603-ABCDEF.md"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn schema_version_mismatch_recreates() {
        let dir = tmp_vault("version");
        write(&dir, "A.md", "hello\n");
        {
            let conn = open_index(&dir).unwrap();
            rebuild(&conn, &dir).unwrap();
            conn.execute(
                "UPDATE index_meta SET value = '0' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        }
        let conn = open_index(&dir).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM pages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0); // recreated empty
        rebuild(&conn, &dir).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM pages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
