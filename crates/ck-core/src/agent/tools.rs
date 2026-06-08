//! Tool registry + dispatch (agent-tools-and-permissions-spec.md).
//! All paths resolve through the traversal-safe `vault.rs`; everything is
//! scoped to the world folder. Read + write tiers; structural/shell land in 6.4.

use serde_json::{json, Value};

use crate::codex_update::transcript_turns;
use crate::error::AppError;
use crate::llm::agent::ToolDef;
use crate::state::AppState;
use crate::store::index;
use crate::world_config::WorldConfig;
use crate::{session_files, vault};

pub const RESULT_CAP: usize = 16 * 1024;
const MAX_TRANSCRIPT_SLICE: usize = 100;
const MAX_SEARCH_HITS: usize = 20;
/// Per-side cap on diff previews shown in approval cards.
const PREVIEW_CAP: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Read,
    Write,
}

pub fn tier_of(name: &str) -> Tier {
    match name {
        "create_page" | "edit_page" | "write_page" => Tier::Write,
        _ => Tier::Read,
    }
}

pub struct ToolCtx<'a> {
    pub state: &'a AppState,
    pub world_root: &'a std::path::Path,
    pub cfg: &'a WorldConfig,
}

pub fn read_tools() -> Vec<ToolDef> {
    fn obj(props: Value, required: &[&str]) -> Value {
        json!({ "type": "object", "properties": props, "required": required })
    }
    vec![
        ToolDef {
            name: "search_pages".into(),
            description: "Full-text search over Codex pages. Returns path, snippet and summary per hit.".into(),
            schema: obj(
                json!({
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "description": "max hits, default 10" }
                }),
                &["query"],
            ),
        },
        ToolDef {
            name: "read_page".into(),
            description: "Read one Codex page (frontmatter + body) by vault-relative path.".into(),
            schema: obj(json!({ "path": { "type": "string" } }), &["path"]),
        },
        ToolDef {
            name: "list_pages".into(),
            description: "List Codex pages (path, kind, summary), optionally under one folder.".into(),
            schema: obj(json!({ "folder": { "type": "string" } }), &[]),
        },
        ToolDef {
            name: "get_backlinks".into(),
            description: "Pages whose wikilinks point at the given page.".into(),
            schema: obj(json!({ "path": { "type": "string" } }), &["path"]),
        },
        ToolDef {
            name: "list_sessions".into(),
            description: "List play sessions: number, title, date.".into(),
            schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "read_summary".into(),
            description: "Read the summary of one session by session number.".into(),
            schema: obj(json!({ "session": { "type": "integer" } }), &["session"]),
        },
        ToolDef {
            name: "search_transcripts".into(),
            description: "Search raw session transcripts. Returns matching numbered turns with session and turn range.".into(),
            schema: obj(
                json!({
                    "query": { "type": "string" },
                    "session": { "type": "integer", "description": "limit to one session" }
                }),
                &["query"],
            ),
        },
        ToolDef {
            name: "read_transcript".into(),
            description: "Read a slice of one session transcript by 1-based turn range (max 100 turns).".into(),
            schema: obj(
                json!({
                    "session": { "type": "integer" },
                    "from_turn": { "type": "integer" },
                    "to_turn": { "type": "integer" }
                }),
                &["session", "from_turn", "to_turn"],
            ),
        },
    ]
}

pub fn write_tools() -> Vec<ToolDef> {
    fn obj(props: Value, required: &[&str]) -> Value {
        json!({ "type": "object", "properties": props, "required": required })
    }
    vec![
        ToolDef {
            name: "create_page".into(),
            description: "Create a new Codex page. Full file content including `---` frontmatter (kind, summary). Errors if the page already exists.".into(),
            schema: obj(
                json!({
                    "path": { "type": "string", "description": "vault-relative, e.g. NPCs/Baron Aldric.md" },
                    "content": { "type": "string" }
                }),
                &["path", "content"],
            ),
        },
        ToolDef {
            name: "edit_page".into(),
            description: "Replace one exact string in a Codex page. old_str must match exactly once — read the page first. Use for targeted edits.".into(),
            schema: obj(
                json!({
                    "path": { "type": "string" },
                    "old_str": { "type": "string" },
                    "new_str": { "type": "string" }
                }),
                &["path", "old_str", "new_str"],
            ),
        },
        ToolDef {
            name: "write_page".into(),
            description: "Overwrite a whole Codex page with new content. Only for restructures where edit_page is impractical.".into(),
            schema: obj(
                json!({
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                }),
                &["path", "content"],
            ),
        },
    ]
}

fn norm_md_path(raw: &str) -> String {
    let p = raw.trim().trim_matches('/');
    if p.to_lowercase().ends_with(".md") {
        p.to_string()
    } else {
        format!("{p}.md")
    }
}

fn cap_preview(s: &str) -> String {
    if s.len() <= PREVIEW_CAP {
        return s.to_string();
    }
    let mut end = PREVIEW_CAP;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[truncated]", &s[..end])
}

/// Diff payload for a write call's approval card: `{path, old, new}`.
/// `Err` = the call is invalid as-is (no point asking the user).
pub fn write_preview(ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Result<Value, String> {
    let str_arg = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let path = norm_md_path(&str_arg("path"));
    if path == ".md" {
        return Err("missing 'path'".into());
    }
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    match name {
        "create_page" => {
            if vault::read_page(&vault_root, &path).is_ok() {
                return Err(format!("Page already exists: {path} — use edit_page or write_page."));
            }
            Ok(json!({ "path": path, "old": Value::Null, "new": cap_preview(&str_arg("content")) }))
        }
        "edit_page" => {
            let page = vault::read_page(&vault_root, &path)
                .map_err(|_| format!("Page not found: {path} — read or list pages first."))?;
            let old_str = str_arg("old_str");
            if old_str.is_empty() {
                return Err("old_str is empty".into());
            }
            match page.content.matches(&old_str).count() {
                0 => Err("old_str not found in the page — read the page and copy the exact text.".into()),
                1 => Ok(json!({ "path": path, "old": cap_preview(&old_str), "new": cap_preview(&str_arg("new_str")) })),
                n => Err(format!("old_str matches {n} times — include more surrounding context to make it unique.")),
            }
        }
        "write_page" => {
            let old = vault::read_page(&vault_root, &path)
                .ok()
                .map_or(Value::Null, |p| Value::String(cap_preview(&p.content)));
            Ok(json!({ "path": path, "old": old, "new": cap_preview(&str_arg("content")) }))
        }
        other => Err(format!("not a write tool: {other}")),
    }
}

/// Run one read-tier tool. `Err` content goes back to the model as a
/// `ToolResult { is_error: true }` — it is conversational, not an HTTP error.
pub fn dispatch(ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Result<String, String> {
    let str_arg = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let int_arg = |k: &str| args.get(k).and_then(Value::as_i64);
    match name {
        "search_pages" => {
            let query = str_arg("query");
            let limit = int_arg("limit").unwrap_or(10).clamp(1, 50) as usize;
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let mut hits = ctx
                .state
                .with_index(&vault_root, |conn| index::search(conn, &query))
                .map_err(app_err)?
                .map_err(app_err)?;
            // FTS ANDs all tokens — too strict for model-phrased queries
            // ("Thornhold ruler"). Empty + multi-word → merge per-token hits.
            if hits.is_empty() && query.split_whitespace().count() > 1 {
                let mut seen = std::collections::HashSet::new();
                for tok in query.split_whitespace() {
                    let more = ctx
                        .state
                        .with_index(&vault_root, |conn| index::search(conn, tok))
                        .map_err(app_err)?
                        .map_err(app_err)?;
                    for h in more {
                        if seen.insert(h.path.clone()) {
                            hits.push(h);
                        }
                    }
                }
            }
            if hits.is_empty() {
                return Ok("No pages match.".into());
            }
            Ok(hits
                .iter()
                .take(limit)
                .map(|h| {
                    let summary = h.summary.as_deref().unwrap_or("");
                    let summary = if summary.is_empty() {
                        String::new()
                    } else {
                        format!("\n  summary: {summary}")
                    };
                    format!("- {} ({}){summary}\n  …{}…", h.path, h.title, strip_b(&h.snippet))
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "read_page" => {
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let page = vault::read_page(&vault_root, &str_arg("path")).map_err(app_err)?;
            Ok(page.content)
        }
        "list_pages" => {
            let folder = str_arg("folder");
            let folder = folder.trim().trim_matches('/');
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let pages = vault::list_pages(&vault_root).map_err(app_err)?;
            let lines: Vec<String> = pages
                .iter()
                .filter(|p| folder.is_empty() || p.path.starts_with(&format!("{folder}/")))
                .map(|p| {
                    let kind = p.kind.as_deref().unwrap_or("");
                    let kind = if kind.is_empty() { String::new() } else { format!(" [{kind}]") };
                    let summary = if p.summary.trim().is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", p.summary.trim())
                    };
                    format!("- {}{kind}{summary}", p.path)
                })
                .collect();
            if lines.is_empty() {
                return Ok("No pages.".into());
            }
            Ok(lines.join("\n"))
        }
        "get_backlinks" => {
            let path = str_arg("path");
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let links = ctx
                .state
                .with_index(&vault_root, |conn| index::sources_linking_to(conn, &path))
                .map_err(app_err)?
                .map_err(app_err)?;
            if links.is_empty() {
                return Ok("No backlinks.".into());
            }
            Ok(links
                .iter()
                .map(|(src, text)| format!("- {src} (as [[{text}]])"))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "list_sessions" => {
            let mut entries = super::context::session_entries(ctx.world_root);
            entries.sort_by_key(|(n, _, _)| std::cmp::Reverse(*n));
            if entries.is_empty() {
                return Ok("No sessions.".into());
            }
            Ok(entries
                .iter()
                .map(|(n, title, date)| {
                    let title = if title.is_empty() { String::new() } else { format!(" — {title}") };
                    let date = if date.is_empty() { String::new() } else { format!(" ({date})") };
                    format!("- Session {n}{title}{date}")
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "read_summary" => {
            let n = int_arg("session").ok_or("missing 'session'")?;
            let dir = session_dir(ctx, n)?;
            let path = session_files::summary_md_path(&dir);
            std::fs::read_to_string(&path)
                .map_err(|_| format!("Session {n} has no summary yet."))
        }
        "search_transcripts" => {
            let query = str_arg("query").to_lowercase();
            if query.trim().is_empty() {
                return Err("empty query".into());
            }
            let only = int_arg("session");
            let mut sessions = super::context::session_entries(ctx.world_root);
            sessions.sort_by_key(|(n, _, _)| std::cmp::Reverse(*n));
            // Whole-phrase match first; model-phrased multi-word queries
            // rarely appear verbatim, so fall back to any-token matching.
            let tokens: Vec<String> = query.split_whitespace().map(str::to_string).collect();
            let mut out: Vec<String> = Vec::new();
            for pass in 0..2 {
                for (n, _, _) in &sessions {
                    let n = *n;
                    if only.is_some_and(|o| o != n) {
                        continue;
                    }
                    let Ok(turns) = transcript_of(ctx, n) else { continue };
                    for (i, t) in turns.iter().enumerate() {
                        let lower = t.to_lowercase();
                        let hit = if pass == 0 {
                            lower.contains(&query)
                        } else {
                            tokens.iter().any(|tok| lower.contains(tok))
                        };
                        if hit {
                            out.push(format!("- session {n}, turn {}: {t}", i + 1));
                            if out.len() >= MAX_SEARCH_HITS {
                                break;
                            }
                        }
                    }
                    if out.len() >= MAX_SEARCH_HITS {
                        break;
                    }
                }
                if !out.is_empty() || tokens.len() < 2 {
                    break;
                }
            }
            if out.is_empty() {
                return Ok("No transcript matches.".into());
            }
            Ok(out.join("\n"))
        }
        "read_transcript" => {
            let n = int_arg("session").ok_or("missing 'session'")?;
            let from = int_arg("from_turn").ok_or("missing 'from_turn'")?.max(1) as usize;
            let to = int_arg("to_turn").ok_or("missing 'to_turn'")? as usize;
            let turns = transcript_of(ctx, n)?;
            if turns.is_empty() {
                return Err(format!("Session {n} has no transcript."));
            }
            let to = to.min(turns.len()).min(from + MAX_TRANSCRIPT_SLICE - 1);
            if from > to {
                return Err(format!("Turn range out of bounds (1–{}).", turns.len()));
            }
            Ok(turns[from - 1..to]
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{}: {t}", from + i))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "create_page" => {
            let path = norm_md_path(&str_arg("path"));
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            if vault::read_page(&vault_root, &path).is_ok() {
                return Err(format!("Page already exists: {path}"));
            }
            vault::write_page(&vault_root, &path, &str_arg("content")).map_err(app_err)?;
            reindex(ctx, &vault_root, &path);
            Ok(format!("Created {path}."))
        }
        "edit_page" => {
            let path = norm_md_path(&str_arg("path"));
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let page = vault::read_page(&vault_root, &path).map_err(app_err)?;
            let old_str = str_arg("old_str");
            match page.content.matches(&old_str).count() {
                1 => {}
                0 => return Err("old_str not found in the page.".into()),
                n => return Err(format!("old_str matches {n} times — not unique.")),
            }
            let content = page.content.replacen(&old_str, &str_arg("new_str"), 1);
            vault::write_page(&vault_root, &path, &content).map_err(app_err)?;
            reindex(ctx, &vault_root, &path);
            Ok(format!("Edited {path}."))
        }
        "write_page" => {
            let path = norm_md_path(&str_arg("path"));
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            vault::write_page(&vault_root, &path, &str_arg("content")).map_err(app_err)?;
            reindex(ctx, &vault_root, &path);
            Ok(format!("Wrote {path}."))
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

/// Suppress the watcher echo + refresh the index row, like every CK-side
/// vault write. Index is a cache — failure must not fail the write.
fn reindex(ctx: &ToolCtx<'_>, vault_root: &std::path::Path, rel: &str) {
    ctx.state.note_vault_write(vault_root, rel);
    let _ = ctx.state.with_index(vault_root, |conn| {
        let _ = index::upsert_path(conn, vault_root, rel);
    });
}

fn app_err(e: AppError) -> String {
    e.to_string()
}

fn strip_b(s: &str) -> String {
    s.replace("<b>", "").replace("</b>", "")
}

fn session_dir(ctx: &ToolCtx<'_>, number: i64) -> Result<std::path::PathBuf, String> {
    let sessions = ctx.world_root.join("Sessions");
    let rd = std::fs::read_dir(&sessions).map_err(|_| "No sessions.".to_string())?;
    for e in rd.flatten() {
        let dir = e.path();
        if let Ok(Some(st)) = session_files::read_session_toml(&dir) {
            if st.number == Some(number) {
                return Ok(dir);
            }
        }
    }
    Err(format!("Session {number} not found."))
}

fn transcript_of(ctx: &ToolCtx<'_>, number: i64) -> Result<Vec<String>, String> {
    let dir = session_dir(ctx, number)?;
    let raw = std::fs::read_to_string(session_files::transcript_md_path(&dir))
        .map_err(|_| format!("Session {number} has no transcript."))?;
    Ok(transcript_turns(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::path::PathBuf;

    fn fixture_world(tag: &str) -> (AppState, PathBuf, WorldConfig) {
        let dir = std::env::temp_dir().join(format!("ck-tools-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("Codex/NPCs")).unwrap();
        std::fs::write(
            dir.join("Codex/Thornhold.md"),
            "---\nkind: place\nsummary: A fortified town.\n---\n\nRuled by [[Baron Aldric]].\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("Codex/NPCs/Baron Aldric.md"),
            "---\nkind: npc\nsummary: Ruler of Thornhold.\n---\n\nStern but fair.\n",
        )
        .unwrap();
        let sess = dir.join("Sessions/001");
        std::fs::create_dir_all(&sess).unwrap();
        std::fs::write(
            sess.join("session.toml"),
            "number = 1\ntitle = \"Arrival\"\ndate = \"2026-05-01\"\n",
        )
        .unwrap();
        std::fs::write(
            sess.join("transcript.md"),
            "[GM]\nYou arrive at Thornhold.\nThe gates are shut.\n[Lyra]\nI knock loudly.\n",
        )
        .unwrap();
        std::fs::write(sess.join("summary.md"), "The party reached Thornhold.\n").unwrap();

        let appdata = dir.join("appdata");
        std::fs::create_dir_all(&appdata).unwrap();
        let state = AppState::new(crate::paths::Paths { data_dir: appdata }).unwrap();
        let cfg = WorldConfig {
            id: "w".into(),
            name: "W".into(),
            ..Default::default()
        };
        (state, dir, cfg)
    }

    fn call(ctx: &ToolCtx<'_>, name: &str, args: Value) -> Result<String, String> {
        dispatch(ctx, name, &args)
    }

    #[test]
    fn read_tier_tools_roundtrip() {
        let (state, root, cfg) = fixture_world("rt");
        let ctx = ToolCtx { state: &state, world_root: &root, cfg: &cfg };

        let pages = call(&ctx, "list_pages", json!({})).unwrap();
        assert!(pages.contains("Thornhold.md [place] — A fortified town."));
        let scoped = call(&ctx, "list_pages", json!({ "folder": "NPCs" })).unwrap();
        assert!(scoped.contains("Baron Aldric"));
        assert!(!scoped.contains("Thornhold.md"));

        let page = call(&ctx, "read_page", json!({ "path": "Thornhold.md" })).unwrap();
        assert!(page.contains("Ruled by [[Baron Aldric]]."));

        let hits = call(&ctx, "search_pages", json!({ "query": "fortified" })).unwrap();
        assert!(hits.contains("Thornhold.md"));

        let back = call(&ctx, "get_backlinks", json!({ "path": "NPCs/Baron Aldric.md" })).unwrap();
        assert!(back.contains("Thornhold.md"));

        let sessions = call(&ctx, "list_sessions", json!({})).unwrap();
        assert!(sessions.contains("Session 1 — Arrival (2026-05-01)"));

        let summary = call(&ctx, "read_summary", json!({ "session": 1 })).unwrap();
        assert!(summary.contains("reached Thornhold"));

        let found = call(&ctx, "search_transcripts", json!({ "query": "knock" })).unwrap();
        assert!(found.contains("session 1, turn 3: Lyra: I knock loudly."));

        let slice =
            call(&ctx, "read_transcript", json!({ "session": 1, "from_turn": 1, "to_turn": 2 }))
                .unwrap();
        assert!(slice.contains("1: GM: You arrive at Thornhold."));
        assert!(!slice.contains("knock"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn errors_are_conversational() {
        let (state, root, cfg) = fixture_world("err");
        let ctx = ToolCtx { state: &state, world_root: &root, cfg: &cfg };
        assert!(call(&ctx, "read_summary", json!({ "session": 99 })).is_err());
        assert!(call(&ctx, "nope", json!({})).is_err());
        assert!(call(&ctx, "read_page", json!({ "path": "../../etc/passwd" })).is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}
