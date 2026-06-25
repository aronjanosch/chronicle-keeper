//! Tool registry + dispatch (agent-tools-and-permissions-spec.md).
//! All paths resolve through the traversal-safe `vault.rs`; everything is
//! scoped to the world folder. Read / write / structural / shell tiers.

use std::time::{Duration, Instant};

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
/// Shell: wall-clock cap and combined stdout+stderr cap.
const SHELL_TIMEOUT: Duration = Duration::from_secs(60);
const SHELL_OUTPUT_CAP: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Read,
    Memory,
    Write,
    Structural,
    Shell,
    Foundry,
}

pub fn tier_of(name: &str) -> Tier {
    match name {
        "read_memory" | "write_memory" | "delete_memory" => Tier::Memory,
        "create_page"
        | "edit_page"
        | "multi_edit_page"
        | "insert_into_page"
        | "write_page" => Tier::Write,
        "rename_page" | "move_page" | "delete_page" | "create_folder" => Tier::Structural,
        "run_command" => Tier::Shell,
        // Writes: always-ask, no remote undo. Live-play reads (foundry_get_actor /
        // list_actors / scene_state / lookup) are ungated — they fall through to
        // Tier::Read since they only query the table, never mutate it.
        "sync_foundry"
        | "foundry_create_actor"
        | "foundry_create_scene"
        | "foundry_create_rolltable"
        | "foundry_post_chat" => Tier::Foundry,
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
            name: "search_summaries".into(),
            description: "Full-text search across the curated session summaries — the cleanest record of what happened in play. Search here before the raw transcripts. Returns matching sessions with a snippet.".into(),
            schema: obj(json!({ "query": { "type": "string" } }), &["query"]),
        },
        ToolDef {
            name: "search_transcripts".into(),
            description: "Search the RAW session transcripts (verbatim speech — noisy, misspellings, off-topic chatter). Use only after search_pages and search_summaries, when you need the exact words or to ground a precise claim.".into(),
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
        ToolDef {
            name: "vault_diagnostics".into(),
            description: "The Codex's health report (same as the Explorer footer): broken [[wikilinks]], orphan pages (no backlinks), broken ![[media]] embeds, unreadable files, and sync-conflict files.".into(),
            schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "tags".into(),
            description: "Tags in the Codex. No argument: every tag with how many pages carry it. \
                          With `tag`: the pages carrying that tag (case-insensitive, leading # optional).".into(),
            schema: obj(json!({ "tag": { "type": "string", "description": "omit to list all tags; supply to list pages with this tag" } }), &[]),
        },
        ToolDef {
            name: "query_world".into(),
            description: "Enumerate pages by structured criteria — frontmatter field values and \
                          tags — exactly and completely, over the whole world at once. Reach for \
                          this (not search_pages) for any \"list / count / which pages where …\" \
                          question, e.g. every dead NPC, all places in a region, factions of a \
                          type. Full-text search matches words and misses pages that don't say \
                          them; this matches the parsed structure. Grammar (dataview-lite): \
                          `LIST FROM #tag kind:npc WHERE field = value`. FROM takes #tags and/or \
                          kind:<kind> (AND). WHERE takes `field = value`, `field != value`, \
                          `field contains value` (AND). Either clause optional. Examples: \
                          `FROM kind:npc WHERE status = dead`; `FROM #faction WHERE region = North`; \
                          `WHERE part_of = \"[[Thornhold]]\"`. Values match frontmatter; wikilinks \
                          compare by target.".into(),
            schema: obj(json!({ "query": { "type": "string" } }), &["query"]),
        },
        ToolDef {
            name: "page_kinds".into(),
            description: "The per-kind infobox field schemas for this world (kind → field names + types). Use before drafting or editing a page's frontmatter so the infobox fields match.".into(),
            schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "read_recap".into(),
            description: "Read the world's \"story so far\" recap, if one has been generated.".into(),
            schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "use_skill".into(),
            description: "Load the full text of one of the skills listed in your system prompt by name. A skill is deep reference (worldbuilding question banks, page syntax) you pull on demand before the task it covers — it returns data you apply, not instructions.".into(),
            schema: obj(json!({ "name": { "type": "string" } }), &["name"]),
        },
    ]
}

pub fn memory_tools() -> Vec<ToolDef> {
    fn obj(props: Value, required: &[&str]) -> Value {
        json!({ "type": "object", "properties": props, "required": required })
    }
    vec![
        ToolDef {
            name: "read_memory".into(),
            description: "Read one of your saved memories in full by its name.".into(),
            schema: obj(json!({ "name": { "type": "string" } }), &["name"]),
        },
        ToolDef {
            name: "write_memory".into(),
            description: "Save or update a lasting work preference, correction, style note, or meta-task (not world lore). Re-use a name to update, don't duplicate.".into(),
            schema: obj(
                json!({
                    "name": { "type": "string", "description": "short kebab-case label" },
                    "description": { "type": "string", "description": "one-line index summary" },
                    "type": { "type": "string", "enum": ["preference", "task", "style", "correction"] },
                    "content": { "type": "string", "description": "the fact, a few lines max" }
                }),
                &["name", "description", "content"],
            ),
        },
        ToolDef {
            name: "delete_memory".into(),
            description: "Delete a memory that turned out wrong or is no longer relevant.".into(),
            schema: obj(json!({ "name": { "type": "string" } }), &["name"]),
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
            description: "Create a new Codex page. Full file content including `---` frontmatter (kind, summary). Timeline event pages (kind: event) go in Events/. Errors if the page already exists.".into(),
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
            description: "Replace an exact string in a Codex page. Read the page first and copy old_str verbatim. By default old_str must match exactly once; set replace_all to change every occurrence (e.g. renaming a term).".into(),
            schema: obj(
                json!({
                    "path": { "type": "string" },
                    "old_str": { "type": "string" },
                    "new_str": { "type": "string" },
                    "replace_all": { "type": "boolean", "description": "replace every occurrence instead of requiring a unique match" }
                }),
                &["path", "old_str", "new_str"],
            ),
        },
        ToolDef {
            name: "multi_edit_page".into(),
            description: "Apply several exact-string edits to one Codex page in a single call. Edits run in order on the evolving text and are all-or-nothing — if any old_str fails to match, none are applied. Prefer this over repeated edit_page calls.".into(),
            schema: obj(
                json!({
                    "path": { "type": "string" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_str": { "type": "string" },
                                "new_str": { "type": "string" },
                                "replace_all": { "type": "boolean" }
                            },
                            "required": ["old_str", "new_str"]
                        }
                    }
                }),
                &["path", "edits"],
            ),
        },
        ToolDef {
            name: "insert_into_page".into(),
            description: "Add text to a Codex page without rewriting the rest of it. Omit heading \
                          to append at the end; supply heading to insert under that markdown \
                          heading (created at the end if absent).".into(),
            schema: obj(
                json!({
                    "path": { "type": "string" },
                    "text": { "type": "string" },
                    "heading": { "type": "string", "description": "exact heading line, e.g. ## Fantastic Locations; omit to append at end" }
                }),
                &["path", "text"],
            ),
        },
        ToolDef {
            name: "write_page".into(),
            description: "Overwrite a whole Codex page with new content. Only for restructures where the targeted edit tools are impractical.".into(),
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

pub fn structural_tools() -> Vec<ToolDef> {
    fn obj(props: Value, required: &[&str]) -> Value {
        json!({ "type": "object", "properties": props, "required": required })
    }
    vec![
        ToolDef {
            name: "rename_page".into(),
            description: "Rename a Codex page (keeps its folder). Rewrites [[wikilinks]] that point at the old name. new_name is the new title, no path or extension.".into(),
            schema: obj(
                json!({
                    "path": { "type": "string" },
                    "new_name": { "type": "string", "description": "new page title, e.g. Baroness Mira" }
                }),
                &["path", "new_name"],
            ),
        },
        ToolDef {
            name: "move_page".into(),
            description: "Move a Codex page into a different folder (filename unchanged). Pass new_folder as vault-relative; empty string = Codex root.".into(),
            schema: obj(
                json!({
                    "path": { "type": "string" },
                    "new_folder": { "type": "string" }
                }),
                &["path", "new_folder"],
            ),
        },
        ToolDef {
            name: "delete_page".into(),
            description: "Delete a Codex page. Reversible via undo, but always confirm intent.".into(),
            schema: obj(json!({ "path": { "type": "string" } }), &["path"]),
        },
        ToolDef {
            name: "create_folder".into(),
            description: "Create an empty folder in the Codex by vault-relative path.".into(),
            schema: obj(json!({ "path": { "type": "string" } }), &["path"]),
        },
    ]
}

pub fn foundry_tools() -> Vec<ToolDef> {
    fn obj(props: Value, required: &[&str]) -> Value {
        json!({ "type": "object", "properties": props, "required": required })
    }
    vec![
        ToolDef {
            name: "sync_foundry".into(),
            description: "Push the whole world to the connected FoundryVTT world (one-way projection — \
                          CK stays the source of truth, Foundry is never read back). Codex pages map to \
                          journals, folders to journal folders, [[wikilinks]] to @UUID links; atlas maps \
                          become scenes (art uploaded as the background) with a linked note per pin. \
                          Removed pages/maps are deleted from Foundry. Always asks first, and there is \
                          no remote undo. Only offered when the Foundry bridge is configured."
                .into(),
            schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "foundry_create_actor".into(),
            description: "Create an Actor in the connected FoundryVTT world. To stat an NPC for the \
                          user's game system, first call foundry_get_actor on an existing actor of the \
                          same type to learn that system's exact field paths, then pass a matching \
                          `system` object (and optional `items`) here — CK writes them through verbatim, \
                          so the NPC is fully statted with no per-system code. Omit `system` for a bare \
                          stub the GM finishes in Foundry. Fire-and-forget; see the foundry-bridge skill."
                .into(),
            schema: obj(
                json!({
                    "name": { "type": "string" },
                    "actor_type": { "type": "string", "description": "Foundry actor type, e.g. npc or character; defaults to npc" },
                    "system": { "type": "object", "description": "Game-system stat block, mirroring the shape of an existing same-type actor (see foundry_get_actor). Omit for a bare stub." },
                    "items": { "type": "array", "description": "Embedded item documents (weapons/spells/features), each a {name,type,system} object matching the system's item shape.", "items": { "type": "object" } }
                }),
                &["name"],
            ),
        },
        ToolDef {
            name: "foundry_create_scene".into(),
            description: "Create a blank Scene (no background) in the connected FoundryVTT world. \
                          For map-backed scenes use sync_foundry instead. See the foundry-bridge skill."
                .into(),
            schema: obj(
                json!({
                    "name": { "type": "string" },
                    "width": { "type": "integer", "description": "pixels, default 3000" },
                    "height": { "type": "integer", "description": "pixels, default 3000" }
                }),
                &["name"],
            ),
        },
        ToolDef {
            name: "foundry_create_rolltable".into(),
            description: "Create a RollTable (loot/encounter table) with plain-text results in the \
                          connected FoundryVTT world. See the foundry-bridge skill."
                .into(),
            schema: obj(
                json!({
                    "name": { "type": "string" },
                    "entries": {
                        "type": "array",
                        "description": "result rows; each tiles the roll range in order",
                        "items": { "type": "object", "properties": {
                            "text": { "type": "string" },
                            "weight": { "type": "integer", "description": "roll weight, default 1" }
                        }, "required": ["text"] }
                    }
                }),
                &["name", "entries"],
            ),
        },
        ToolDef {
            name: "foundry_list_actors".into(),
            description: "List every Actor (name + type) in the connected FoundryVTT world — \
                          who/what exists at the table. Read-only, no approval needed. See the \
                          foundry-bridge skill."
                .into(),
            schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "foundry_get_actor".into(),
            description: "Look up one Actor by name OR by actor id in the connected FoundryVTT world: \
                          core fields, its items, and the raw system stat block (game-system specific \
                          — interpret it for the user). Pass the actorId from foundry_scene_state to \
                          resolve a specific token when names collide. Read-only, no approval needed."
                .into(),
            schema: obj(json!({ "name": { "type": "string", "description": "Actor name or actor id." } }), &["name"]),
        },
        ToolDef {
            name: "foundry_scene_state".into(),
            description: "Describe the active Scene in the connected FoundryVTT world — its name, \
                          size, and the tokens placed on it (with their linked actor ids). Read-only, \
                          no approval needed."
                .into(),
            schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "foundry_lookup".into(),
            description: "Search the installed game system's compendium packs (rules / skills / \
                          items) by name in the connected FoundryVTT world — the GM's own system + \
                          version, offline. Read-only, no approval needed."
                .into(),
            schema: obj(json!({ "query": { "type": "string" } }), &["query"]),
        },
        ToolDef {
            name: "foundry_system_info".into(),
            description: "Inspect the connected world's game system: its id/version, the Actor and \
                          Item types it defines, and (with `type`) that type's default `system` data \
                          model — the exact field paths to fill when statting an actor. Use this to \
                          stat an NPC on an empty world without an existing actor to sample. If the \
                          system defines its schema in code (a DataModel), it says so and points you \
                          to foundry_get_actor instead. Read-only, no approval needed."
                .into(),
            schema: obj(
                json!({
                    "doc": { "type": "string", "description": "Document class for the schema: Actor (default) or Item" },
                    "type": { "type": "string", "description": "A type name (e.g. npc, weapon) to get its default system schema; omit for just the type listing" }
                }),
                &[],
            ),
        },
        ToolDef {
            name: "foundry_post_chat".into(),
            description: "Post a message to the table's chat log in the connected FoundryVTT world \
                          (markdown rendered to HTML). The one live-play write — always asks first, \
                          no remote undo. Use only when the user asks to say something to the table."
                .into(),
            schema: obj(json!({ "message": { "type": "string" } }), &["message"]),
        },
    ]
}

pub fn shell_tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "run_command".into(),
        description: "Run a shell command in the world folder (grep/awk/sed/ls over the vault, batch text surgery). Runs via /bin/sh -c with cwd = world root, 60s timeout, output capped. Always asks the user per call.".into(),
        schema: json!({
            "type": "object",
            "properties": { "command": { "type": "string" } },
            "required": ["command"],
        }),
    }]
}

fn norm_md_path(raw: &str) -> String {
    let p = raw.trim().trim_matches('/');
    if p.to_lowercase().ends_with(".md") {
        p.to_string()
    } else {
        format!("{p}.md")
    }
}

struct EditOp {
    old: String,
    new: String,
    all: bool,
}

fn parse_edits(args: &Value) -> Vec<EditOp> {
    args.get("edits")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|e| EditOp {
                    old: e
                        .get("old_str")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    new: e
                        .get("new_str")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    all: e
                        .get("replace_all")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Apply exact-string edits in order on the evolving content. All-or-nothing:
/// the first failure aborts with a 1-based message and no partial result.
/// On an exact miss we retry with whitespace-normalized matching so a copy
/// that differs only in indentation or run-length of spaces still lands —
/// the change is still shown in the approval diff before it commits.
fn apply_edits(content: &str, edits: &[EditOp]) -> Result<String, String> {
    if edits.is_empty() {
        return Err("no edits provided".into());
    }
    let mut cur = content.to_string();
    for (i, e) in edits.iter().enumerate() {
        let n = i + 1;
        if e.old.is_empty() {
            return Err(format!("edit {n}: old_str is empty"));
        }
        match (cur.matches(&e.old).count(), e.all) {
            (0, _) => {
                let spans = ws_match_spans(&cur, &e.old);
                match (spans.len(), e.all) {
                    (0, _) => {
                        return Err(format!(
                            "edit {n}: old_str not found — read the page and copy the exact text."
                        ))
                    }
                    (1, _) | (_, true) => cur = replace_spans(&cur, &spans, &e.new),
                    (m, false) => {
                        return Err(format!(
                            "edit {n}: old_str matches {m} times (whitespace-normalized) — \
                             set replace_all or add surrounding context."
                        ))
                    }
                }
            }
            (_, true) => cur = cur.replace(&e.old, &e.new),
            (1, false) => cur = cur.replacen(&e.old, &e.new, 1),
            (m, false) => {
                return Err(format!(
                "edit {n}: old_str matches {m} times — set replace_all or add surrounding context."
            ))
            }
        }
    }
    Ok(cur)
}

/// Collapse every run of whitespace to one space; map each normalized byte
/// back to the original byte offset it came from (plus a final sentinel at
/// `s.len()`), so a match in normalized space maps to an original span.
fn normalize_ws(s: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(s.len());
    let mut map = Vec::with_capacity(s.len() + 1);
    let mut prev_ws = false;
    for (i, ch) in s.char_indices() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                map.push(i);
            }
            prev_ws = true;
        } else {
            for _ in 0..ch.len_utf8() {
                map.push(i);
            }
            out.push(ch);
            prev_ws = false;
        }
    }
    map.push(s.len());
    (out, map)
}

/// Original-text spans whose whitespace-normalized form equals `old`'s.
fn ws_match_spans(content: &str, old: &str) -> Vec<(usize, usize)> {
    let (norm_old_raw, _) = normalize_ws(old);
    let needle = norm_old_raw.trim();
    if needle.is_empty() {
        return Vec::new();
    }
    let (norm, map) = normalize_ws(content);
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(rel) = norm[from..].find(needle) {
        let ns = from + rel;
        let ne = ns + needle.len();
        spans.push((map[ns], map[ne]));
        from = ne;
    }
    spans
}

/// Replace each original span (left-to-right, non-overlapping) with `new`.
fn replace_spans(content: &str, spans: &[(usize, usize)], new: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut last = 0;
    for &(s, e) in spans {
        out.push_str(&content[last..s]);
        out.push_str(new);
        last = e;
    }
    out.push_str(&content[last..]);
    out
}

/// "Page not found" plus the closest real paths, so a guessed path turns into
/// one corrected read instead of a dead round-trip.
fn not_found_with_suggestions(vault_root: &std::path::Path, wanted: &str) -> String {
    let near = nearest_pages(vault_root, wanted, 5);
    if near.is_empty() {
        format!("Page not found: {wanted} — use list_pages or search_pages.")
    } else {
        let list = near
            .iter()
            .map(|p| format!("- {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("Page not found: {wanted}. Did you mean:\n{list}")
    }
}

/// Rank known page paths against a guessed one: exact basename, then basename
/// substring, then shared word count. Returns up to `n` non-zero matches.
fn nearest_pages(vault_root: &std::path::Path, wanted: &str, n: usize) -> Vec<String> {
    fn base(p: &str) -> String {
        p.rsplit('/')
            .next()
            .unwrap_or(p)
            .trim_end_matches(".md")
            .to_lowercase()
    }
    fn words(s: &str) -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    }
    let wb = base(wanted);
    let ww = words(&wb);
    let mut scored: Vec<(i32, String)> = vault::list_pages(vault_root)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| {
            let pb = base(&p.path);
            let score = if pb == wb {
                1000
            } else if pb.contains(&wb) || wb.contains(&pb) {
                500
            } else {
                let pw = words(&pb);
                ww.iter().filter(|w| pw.contains(w)).count() as i32 * 100
            };
            (score > 0).then_some((score, p.path))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(n).map(|(_, p)| p).collect()
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

/// Approval-card payload for a gated call. Write tools carry `{path, old,
/// new}` (a diff); structural carry `{path, action, summary, to?}` (a
/// sentence); shell carries `{command, cwd}`. `Err` = the call is invalid
/// as-is — surface it to the model, never to the user.
pub fn gate_preview(ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Result<Value, String> {
    match tier_of(name) {
        Tier::Write => write_preview(ctx, name, args),
        Tier::Structural => structural_preview(ctx, name, args),
        Tier::Foundry => foundry_preview(ctx, name, args),
        Tier::Shell => {
            let cmd = args
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if cmd.is_empty() {
                return Err("empty command".into());
            }
            Ok(json!({ "command": cmd, "cwd": ctx.world_root.display().to_string() }))
        }
        Tier::Read | Tier::Memory => Err(format!("not a gated tool: {name}")),
    }
}

/// Approval card for a Foundry tool. `action` carries the tool name (no `new`)
/// so the UI shows the summary, not a diff. The create-* tools describe what
/// one document they will make; `sync_foundry` describes the whole push.
fn foundry_preview(ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Result<Value, String> {
    if !crate::foundry::load_settings(ctx.state)
        .map(|s| s.is_complete())
        .unwrap_or(false)
    {
        return Err(
            "The Foundry bridge is not configured — set it up in Settings → Foundry VTT bridge."
                .into(),
        );
    }
    let str_arg = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").trim();
    let summary = match name {
        "foundry_create_actor" => {
            let t = str_arg("actor_type");
            let t = if t.is_empty() { "npc" } else { t };
            format!(
                "create actor “{}” ({t}) in FoundryVTT — bare stub, no remote undo",
                str_arg("name")
            )
        }
        "foundry_create_scene" => {
            let w = args.get("width").and_then(Value::as_u64).unwrap_or(3000);
            let h = args.get("height").and_then(Value::as_u64).unwrap_or(3000);
            format!(
                "create blank scene “{}” ({w}×{h}) in FoundryVTT — no remote undo",
                str_arg("name")
            )
        }
        "foundry_create_rolltable" => {
            let n = args
                .get("entries")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            format!(
                "create roll table “{}” ({n} entr{}) in FoundryVTT — no remote undo",
                str_arg("name"),
                if n == 1 { "y" } else { "ies" }
            )
        }
        "foundry_post_chat" => {
            let msg = str_arg("message");
            let preview: String = msg.chars().take(80).collect();
            let ellipsis = if msg.chars().count() > 80 { "…" } else { "" };
            format!("post to the FoundryVTT chat log: “{preview}{ellipsis}” — no remote undo")
        }
        _ => return foundry_sync_preview(ctx),
    };
    Ok(json!({ "action": name, "summary": summary }))
}

/// The whole-world push summary (page create / update / delete counts).
fn foundry_sync_preview(ctx: &ToolCtx<'_>) -> Result<Value, String> {
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    let pages = vault::list_pages(&vault_root).map_err(app_err)?;
    let map = crate::foundry::read_map(ctx.world_root);
    let present: std::collections::HashSet<&str> = pages.iter().map(|p| p.path.as_str()).collect();
    let updated = pages
        .iter()
        .filter(|p| map.pages.contains_key(&p.path))
        .count();
    let created = pages.len() - updated;
    let removed = map
        .pages
        .keys()
        .filter(|k| !present.contains(k.as_str()))
        .count();
    let mut parts = Vec::new();
    if created > 0 {
        parts.push(format!("{created} new"));
    }
    if updated > 0 {
        parts.push(format!("{updated} updated"));
    }
    if removed > 0 {
        parts.push(format!("{removed} removed"));
    }
    let detail = if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join(", "))
    };
    let maps = crate::atlas::list_maps(ctx.world_root)
        .map(|m| m.len())
        .unwrap_or(0);
    let maps_clause = if maps == 0 {
        String::new()
    } else {
        format!(
            " + {maps} atlas map{} as scenes",
            if maps == 1 { "" } else { "s" }
        )
    };
    let summary = format!(
        "push {} Codex page{}{maps_clause} to FoundryVTT{detail} — one-way, no remote undo",
        pages.len(),
        if pages.len() == 1 { "" } else { "s" },
    );
    Ok(json!({ "action": "sync_foundry", "summary": summary }))
}

/// Run the one-way Codex → Foundry push and report the counts. Async (network);
/// the agent loop calls this directly rather than through `dispatch`.
pub async fn run_foundry_sync(ctx: &ToolCtx<'_>) -> Result<String, String> {
    let settings = crate::foundry::load_settings(ctx.state).map_err(app_err)?;
    if !settings.is_complete() {
        return Err(
            "The Foundry bridge is not configured — set it up in Settings → Foundry VTT bridge."
                .into(),
        );
    }
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    let report =
        crate::foundry::sync::sync_world(&settings, ctx.world_root, &vault_root, &ctx.cfg.name)
            .await
            .map_err(|e| format!("Foundry sync failed: {e}"))?;
    let mut out = format!(
        "Synced the Codex to FoundryVTT — {} created, {} updated, {} deleted.",
        report.created, report.updated, report.deleted
    );
    if report.scenes > 0 {
        out.push_str(&format!(
            " Projected {} atlas map(s) as scenes.",
            report.scenes
        ));
    }
    if !report.errors.is_empty() {
        out.push_str(&format!("\n{} page(s) failed:", report.errors.len()));
        for e in report.errors.iter().take(20) {
            out.push_str(&format!("\n- {e}"));
        }
    }
    Ok(out)
}

/// Foundry tools touch the network, so the agent loop runs them directly (async)
/// rather than through the synchronous `dispatch`.
pub fn is_foundry_async(name: &str) -> bool {
    matches!(
        name,
        "sync_foundry"
            | "foundry_create_actor"
            | "foundry_create_scene"
            | "foundry_create_rolltable"
            | "foundry_post_chat"
            | "foundry_list_actors"
            | "foundry_get_actor"
            | "foundry_scene_state"
            | "foundry_lookup"
            | "foundry_system_info"
    )
}

/// Async runner for every Foundry tool. The create-* tools are fire-and-forget
/// bare stubs (system-agnostic; no stat block, no foundry-map tracking, so a
/// repeat call makes a duplicate).
pub async fn run_foundry_tool(
    ctx: &ToolCtx<'_>,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    if name == "sync_foundry" {
        return run_foundry_sync(ctx).await;
    }
    let settings = crate::foundry::load_settings(ctx.state).map_err(app_err)?;
    if !settings.is_complete() {
        return Err(
            "The Foundry bridge is not configured — set it up in Settings → Foundry VTT bridge."
                .into(),
        );
    }
    let str_arg = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let mut client = crate::foundry::FoundryClient::connect(
        &settings.server_url,
        &settings.user_id,
        &settings.password,
    )
    .await
    .map_err(|e| format!("Foundry connect failed: {e}"))?;

    let result = match name {
        "foundry_create_actor" => {
            let aname = str_arg("name");
            let atype = {
                let t = str_arg("actor_type");
                if t.is_empty() {
                    "npc".to_string()
                } else {
                    t
                }
            };
            let system = args.get("system").filter(|v| v.is_object());
            let items = args.get("items").filter(|v| v.is_array());
            if aname.is_empty() {
                Err("name is required".to_string())
            } else {
                let statted = if system.is_some() { "statted" } else { "bare" };
                client
                    .create_actor(&aname, &atype, system, items)
                    .await
                    .map(|id| format!("Created {statted} actor “{aname}” ({atype}) in Foundry [id {id}]."))
                    .map_err(|e| format!("create actor failed: {e}"))
            }
        }
        "foundry_create_scene" => {
            let sname = str_arg("name");
            let w = args.get("width").and_then(Value::as_u64).unwrap_or(3000) as u32;
            let h = args.get("height").and_then(Value::as_u64).unwrap_or(3000) as u32;
            if sname.is_empty() {
                Err("name is required".to_string())
            } else {
                client
                    .create_scene_stub(&sname, w, h)
                    .await
                    .map(|id| format!("Created blank scene “{sname}” ({w}×{h}) in Foundry [id {id}]."))
                    .map_err(|e| format!("create scene failed: {e}"))
            }
        }
        "foundry_create_rolltable" => {
            let tname = str_arg("name");
            let entries: Vec<(String, u32)> = args
                .get("entries")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| {
                            let text = e.get("text").and_then(Value::as_str)?.trim().to_string();
                            if text.is_empty() {
                                return None;
                            }
                            let weight =
                                e.get("weight").and_then(Value::as_u64).unwrap_or(1).max(1) as u32;
                            Some((text, weight))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if tname.is_empty() {
                Err("name is required".to_string())
            } else if entries.is_empty() {
                Err("at least one entry with text is required".to_string())
            } else {
                let n = entries.len();
                client
                    .create_rolltable(&tname, &entries)
                    .await
                    .map(|id| format!("Created roll table “{tname}” with {n} entries in Foundry [id {id}]."))
                    .map_err(|e| format!("create roll table failed: {e}"))
            }
        }
        "foundry_list_actors" => client
            .fetch_world()
            .await
            .map(|w| crate::foundry::read::list_actors(&w))
            .map_err(|e| format!("fetch world failed: {e}")),
        "foundry_scene_state" => client
            .fetch_world()
            .await
            .map(|w| crate::foundry::read::scene_state(&w))
            .map_err(|e| format!("fetch world failed: {e}")),
        "foundry_get_actor" => {
            let aname = str_arg("name");
            if aname.is_empty() {
                Err("name is required".to_string())
            } else {
                client
                    .fetch_world()
                    .await
                    .map(|w| crate::foundry::read::get_actor(&w, &aname))
                    .map_err(|e| format!("fetch world failed: {e}"))
            }
        }
        "foundry_lookup" => {
            let query = str_arg("query");
            if query.is_empty() {
                Err("query is required".to_string())
            } else {
                client
                    .fetch_world()
                    .await
                    .map(|w| crate::foundry::read::lookup(&w, &query))
                    .map_err(|e| format!("fetch world failed: {e}"))
            }
        }
        "foundry_system_info" => {
            let doc = {
                let d = str_arg("doc");
                if d.is_empty() {
                    "Actor".to_string()
                } else {
                    d
                }
            };
            let want_type = str_arg("type");
            let want = if want_type.is_empty() {
                None
            } else {
                Some(want_type.as_str())
            };
            match client.fetch_world().await {
                Ok(world) => {
                    let status = crate::foundry::fetch_status(&settings.server_url).await;
                    let sys_id = status
                        .as_ref()
                        .and_then(|s| s.get("system"))
                        .and_then(Value::as_str);
                    let template = match sys_id {
                        Some(id) => client.fetch_template(id).await.ok().flatten(),
                        None => None,
                    };
                    Ok(crate::foundry::system::system_info(
                        &world,
                        status.as_ref(),
                        template.as_ref(),
                        &doc,
                        want,
                    ))
                }
                Err(e) => Err(format!("fetch world failed: {e}")),
            }
        }
        "foundry_post_chat" => {
            let msg = str_arg("message");
            if msg.trim().is_empty() {
                Err("message is empty".to_string())
            } else {
                let html = crate::foundry::body_to_html(&msg, &|_| None);
                client
                    .post_chat(&html, &settings.user_id)
                    .await
                    .map(|id| format!("Posted to the Foundry chat log [id {id}]."))
                    .map_err(|e| format!("post chat failed: {e}"))
            }
        }
        other => Err(format!("unknown foundry tool: {other}")),
    };
    client.close().await;
    result
}

fn write_preview(ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Result<Value, String> {
    let str_arg = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let path = norm_md_path(&str_arg("path"));
    if path == ".md" {
        return Err("missing 'path'".into());
    }
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    match name {
        "create_page" => {
            if vault::read_page(&vault_root, &path).is_ok() {
                return Err(format!(
                    "Page already exists: {path} — use edit_page or write_page."
                ));
            }
            Ok(json!({ "path": path, "old": Value::Null, "new": cap_preview(&str_arg("content")) }))
        }
        "edit_page" => {
            let page = vault::read_page(&vault_root, &path)
                .map_err(|_| format!("Page not found: {path} — read or list pages first."))?;
            let op = EditOp {
                old: str_arg("old_str"),
                new: str_arg("new_str"),
                all: args
                    .get("replace_all")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            apply_edits(&page.content, std::slice::from_ref(&op))?;
            Ok(json!({ "path": path, "old": cap_preview(&op.old), "new": cap_preview(&op.new) }))
        }
        "multi_edit_page" => {
            let page = vault::read_page(&vault_root, &path)
                .map_err(|_| format!("Page not found: {path} — read or list pages first."))?;
            let edits = parse_edits(args);
            apply_edits(&page.content, &edits)?;
            let join = |pick: fn(&EditOp) -> &String| {
                edits
                    .iter()
                    .map(|e| pick(e).as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n")
            };
            Ok(
                json!({ "path": path, "old": cap_preview(&join(|e| &e.old)), "new": cap_preview(&join(|e| &e.new)) }),
            )
        }
        "insert_into_page" => {
            vault::read_page(&vault_root, &path)
                .map_err(|_| format!("Page not found: {path} — read or list pages first."))?;
            let heading = str_arg("heading");
            let text = str_arg("text");
            if text.trim().is_empty() {
                return Err("text is empty".into());
            }
            let preview = if heading.trim().is_empty() {
                cap_preview(&text)
            } else {
                cap_preview(&format!("{}\n\n{}", heading.trim(), text.trim()))
            };
            Ok(json!({ "path": path, "old": Value::Null, "new": preview }))
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

fn structural_preview(ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Result<Value, String> {
    let str_arg = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    match name {
        "create_folder" => {
            let p = str_arg("path");
            let p = p.trim().trim_matches('/');
            if p.is_empty() {
                return Err("missing 'path'".into());
            }
            Ok(
                json!({ "path": p, "action": "create_folder", "summary": format!("Create folder {p}/") }),
            )
        }
        "delete_page" => {
            let path = norm_md_path(&str_arg("path"));
            vault::read_page(&vault_root, &path).map_err(|_| format!("Page not found: {path}"))?;
            Ok(json!({ "path": path, "action": "delete", "summary": format!("Delete {path}") }))
        }
        "rename_page" => {
            let path = norm_md_path(&str_arg("path"));
            vault::read_page(&vault_root, &path).map_err(|_| format!("Page not found: {path}"))?;
            let to = rename_target(&path, &str_arg("new_name"))?;
            let n = backlink_count(ctx, &vault_root, &path);
            let links = if n == 0 {
                String::new()
            } else {
                format!(" and rewrite {n} link{}", if n == 1 { "" } else { "s" })
            };
            Ok(json!({
                "path": path, "to": to, "action": "rename",
                "summary": format!("Rename {} → {}{}", stem(&path), stem(&to), links),
            }))
        }
        "move_page" => {
            let path = norm_md_path(&str_arg("path"));
            vault::read_page(&vault_root, &path).map_err(|_| format!("Page not found: {path}"))?;
            let to = move_target(&path, &str_arg("new_folder"));
            Ok(json!({
                "path": path, "to": to, "action": "move",
                "summary": format!("Move {path} → {to}"),
            }))
        }
        other => Err(format!("not a structural tool: {other}")),
    }
}

fn stem(p: &str) -> String {
    std::path::Path::new(p)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// New vault path for a rename: same folder, new filename from `new_name`.
fn rename_target(path: &str, new_name: &str) -> Result<String, String> {
    let new_name = new_name.trim().trim_matches('/');
    if new_name.is_empty() {
        return Err("missing 'new_name'".into());
    }
    let file = norm_md_path(new_name);
    Ok(
        match std::path::Path::new(path)
            .parent()
            .and_then(|p| p.to_str())
            .filter(|s| !s.is_empty())
        {
            Some(dir) => format!("{dir}/{file}"),
            None => file,
        },
    )
}

/// New vault path for a move: new folder, same filename.
fn move_target(path: &str, new_folder: &str) -> String {
    let folder = new_folder.trim().trim_matches('/');
    let file = std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path);
    if folder.is_empty() {
        file.to_string()
    } else {
        format!("{folder}/{file}")
    }
}

fn backlink_count(ctx: &ToolCtx<'_>, vault_root: &std::path::Path, path: &str) -> usize {
    ctx.state
        .with_index(vault_root, |conn| index::sources_linking_to(conn, path))
        .ok()
        .and_then(|r| r.ok())
        .map(|v| v.iter().filter(|(src, _)| src != path).count())
        .unwrap_or(0)
}

/// Run one read-tier tool. `Err` content goes back to the model as a
/// `ToolResult { is_error: true }` — it is conversational, not an HTTP error.
pub fn dispatch(ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Result<String, String> {
    let str_arg = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
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
                    format!(
                        "- {} ({}){summary}\n  …{}…",
                        h.path,
                        h.title,
                        strip_b(&h.snippet)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "read_page" => {
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let path = str_arg("path");
            match vault::read_page(&vault_root, &path) {
                Ok(page) => Ok(page.content),
                Err(_) => Err(not_found_with_suggestions(&vault_root, &path)),
            }
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
                    let kind = if kind.is_empty() {
                        String::new()
                    } else {
                        format!(" [{kind}]")
                    };
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
                    let title = if title.is_empty() {
                        String::new()
                    } else {
                        format!(" — {title}")
                    };
                    let date = if date.is_empty() {
                        String::new()
                    } else {
                        format!(" ({date})")
                    };
                    format!("- Session {n}{title}{date}")
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "read_summary" => {
            let n = int_arg("session").ok_or("missing 'session'")?;
            let dir = session_dir(ctx, n)?;
            let path = session_files::summary_md_path(&dir);
            std::fs::read_to_string(&path).map_err(|_| format!("Session {n} has no summary yet."))
        }
        "search_summaries" => {
            let query = str_arg("query").to_lowercase();
            if query.trim().is_empty() {
                return Err("empty query".into());
            }
            let mut sessions = super::context::session_entries(ctx.world_root);
            sessions.sort_by_key(|(n, _, _)| std::cmp::Reverse(*n));
            let tokens: Vec<String> = query.split_whitespace().map(str::to_string).collect();
            let mut out: Vec<String> = Vec::new();
            // Whole-phrase pass, then any-token fallback (same shape as transcripts).
            for pass in 0..2 {
                for (n, title, _) in &sessions {
                    let Some(dir) = session_dir(ctx, *n).ok() else {
                        continue;
                    };
                    let Ok(text) = std::fs::read_to_string(session_files::summary_md_path(&dir))
                    else {
                        continue;
                    };
                    let lower = text.to_lowercase();
                    let hit = if pass == 0 {
                        lower.find(&query)
                    } else {
                        tokens.iter().find_map(|t| lower.find(t))
                    };
                    if let Some(at) = hit {
                        let title = if title.is_empty() {
                            String::new()
                        } else {
                            format!(" — {title}")
                        };
                        out.push(format!(
                            "- session {n}{title}: …{}…",
                            snippet_around(&text, at, 220)
                        ));
                        if out.len() >= MAX_SEARCH_HITS {
                            break;
                        }
                    }
                }
                if !out.is_empty() || tokens.len() < 2 {
                    break;
                }
            }
            if out.is_empty() {
                return Ok("No summary matches. The raw transcripts (search_transcripts) may still have it.".into());
            }
            Ok(out.join("\n"))
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
                    let Ok(turns) = transcript_of(ctx, n) else {
                        continue;
                    };
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
        "vault_diagnostics" => {
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let d = ctx
                .state
                .with_index(&vault_root, |conn| index::diagnostics(conn, &vault_root))
                .map_err(app_err)?
                .map_err(app_err)?;
            let mut out = String::new();
            if !d.broken_links.is_empty() {
                out.push_str(&format!("Broken wikilinks ({}):\n", d.broken_links.len()));
                for b in d.broken_links.iter().take(40) {
                    out.push_str(&format!(
                        "- {} links to [[{}]] (no such page)\n",
                        b.source_path, b.link_text
                    ));
                }
            }
            if !d.orphans.is_empty() {
                out.push_str(&format!(
                    "\nOrphan pages — no backlinks ({}):\n",
                    d.orphans.len()
                ));
                for o in d.orphans.iter().take(40) {
                    out.push_str(&format!("- {}\n", o.path));
                }
            }
            if !d.broken_media.is_empty() {
                out.push_str(&format!(
                    "\nBroken media embeds ({}):\n",
                    d.broken_media.len()
                ));
                for m in d.broken_media.iter().take(40) {
                    out.push_str(&format!(
                        "- {} embeds {} (missing)\n",
                        m.source_path, m.target
                    ));
                }
            }
            if !d.scan_errors.is_empty() {
                out.push_str(&format!("\nUnreadable files ({}):\n", d.scan_errors.len()));
                for e in d.scan_errors.iter().take(40) {
                    out.push_str(&format!("- {}: {}\n", e.path, e.error));
                }
            }
            if !d.conflicts.is_empty() {
                out.push_str(&format!("\nSync-conflict files ({}):\n", d.conflicts.len()));
                for c in d.conflicts.iter().take(40) {
                    out.push_str(&format!("- {c}\n"));
                }
            }
            if out.is_empty() {
                return Ok(
                    "The Codex is clean — no broken links, orphans, media, errors or conflicts."
                        .into(),
                );
            }
            Ok(out.trim_end().to_string())
        }
        "tags" => {
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let want = str_arg("tag");
            let want = want.trim().trim_start_matches('#').to_lowercase();
            if want.is_empty() {
                let tags = ctx
                    .state
                    .with_index(&vault_root, index::tag_counts)
                    .map_err(app_err)?
                    .map_err(app_err)?;
                if tags.is_empty() {
                    return Ok("No tags yet.".into());
                }
                return Ok(tags
                    .iter()
                    .map(|(t, n)| format!("- #{t} ({n})"))
                    .collect::<Vec<_>>()
                    .join("\n"));
            }
            let meta = ctx
                .state
                .with_index(&vault_root, index::page_meta)
                .map_err(app_err)?
                .map_err(app_err)?;
            let mut hits: Vec<&String> = meta
                .iter()
                .filter(|(_, (_, tags))| tags.iter().any(|t| t.to_lowercase() == want))
                .map(|(path, _)| path)
                .collect();
            hits.sort();
            if hits.is_empty() {
                return Ok(format!("No pages tagged #{want}."));
            }
            Ok(hits
                .iter()
                .map(|p| format!("- {p}"))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "query_world" => {
            let q = str_arg("query");
            if q.trim().is_empty() {
                return Err("missing 'query'".into());
            }
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let result = ctx
                .state
                .with_index(&vault_root, |conn| index::run_query(conn, &q))
                .map_err(app_err)?
                .map_err(app_err)?;
            let hits = result?;
            if hits.is_empty() {
                return Ok("No pages match.".into());
            }
            Ok(hits
                .iter()
                .map(|h| {
                    let kind = h.kind.as_deref().unwrap_or("");
                    let kind = if kind.is_empty() {
                        String::new()
                    } else {
                        format!(" [{kind}]")
                    };
                    let summary = if h.summary.trim().is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", h.summary.trim())
                    };
                    format!("- {}{kind}{summary}", h.path)
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "page_kinds" => {
            let schemas = ctx.cfg.kind_schemas();
            Ok(schemas
                .iter()
                .map(|(kind, fields)| {
                    let f = if fields.is_empty() {
                        "(no infobox fields)".into()
                    } else {
                        fields
                            .iter()
                            .map(|f| format!("{}:{}", f.name, f.ftype))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    format!("- {kind}: {f}")
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "read_recap" => {
            let (body, _) = crate::world_config::read_recap(ctx.world_root);
            if body.trim().is_empty() {
                Err("No recap has been generated yet.".into())
            } else {
                Ok(body)
            }
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
            let op = EditOp {
                old: str_arg("old_str"),
                new: str_arg("new_str"),
                all: args
                    .get("replace_all")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            let content = apply_edits(&page.content, std::slice::from_ref(&op))?;
            vault::write_page(&vault_root, &path, &content).map_err(app_err)?;
            reindex(ctx, &vault_root, &path);
            Ok(format!("Edited {path}."))
        }
        "multi_edit_page" => {
            let path = norm_md_path(&str_arg("path"));
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let page = vault::read_page(&vault_root, &path).map_err(app_err)?;
            let edits = parse_edits(args);
            let content = apply_edits(&page.content, &edits)?;
            vault::write_page(&vault_root, &path, &content).map_err(app_err)?;
            reindex(ctx, &vault_root, &path);
            Ok(format!("Applied {} edits to {path}.", edits.len()))
        }
        "insert_into_page" => {
            let path = norm_md_path(&str_arg("path"));
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            let page = vault::read_page(&vault_root, &path).map_err(app_err)?;
            let heading = str_arg("heading");
            let text = str_arg("text");
            let content = if heading.trim().is_empty() {
                format!("{}\n\n{}\n", page.content.trim_end(), text.trim_end())
            } else {
                vault::append_under_heading(&page.content, &heading, &text)
            };
            vault::write_page(&vault_root, &path, &content).map_err(app_err)?;
            reindex(ctx, &vault_root, &path);
            Ok(format!("Updated {path}."))
        }
        "write_page" => {
            let path = norm_md_path(&str_arg("path"));
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            vault::write_page(&vault_root, &path, &str_arg("content")).map_err(app_err)?;
            reindex(ctx, &vault_root, &path);
            Ok(format!("Wrote {path}."))
        }
        "create_folder" => {
            let p = str_arg("path");
            let p = p.trim().trim_matches('/');
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            vault::create_folder(&vault_root, p).map_err(app_err)?;
            Ok(format!("Created folder {p}/."))
        }
        "delete_page" => {
            let path = norm_md_path(&str_arg("path"));
            let vault_root = ctx.cfg.codex_dir(ctx.world_root);
            crate::trash::trash_paths(ctx.world_root, &vault_root, &[(path.clone(), false)])
                .map_err(app_err)?;
            ctx.state.note_vault_write(&vault_root, &path);
            let _ = ctx.state.with_index(&vault_root, |conn| {
                let _ = index::remove_path(conn, &path);
            });
            Ok(format!(
                "Moved {path} to the world trash (restorable from the Trash view)."
            ))
        }
        "rename_page" => {
            let path = norm_md_path(&str_arg("path"));
            let to = rename_target(&path, &str_arg("new_name"))?;
            move_with_links(ctx, &path, &to)?;
            Ok(format!("Renamed {} → {}.", stem(&path), stem(&to)))
        }
        "move_page" => {
            let path = norm_md_path(&str_arg("path"));
            let to = move_target(&path, &str_arg("new_folder"));
            move_with_links(ctx, &path, &to)?;
            Ok(format!("Moved {path} → {to}."))
        }
        "use_skill" => {
            super::skills::read(&super::skills::skills_root(ctx.state), &str_arg("name"))
        }
        "read_memory" => super::memory::read_memory(ctx.world_root, &str_arg("name")),
        "write_memory" => super::memory::write_memory(
            ctx.world_root,
            &str_arg("name"),
            &str_arg("description"),
            &str_arg("type"),
            &str_arg("content"),
        ),
        "delete_memory" => super::memory::delete_memory(ctx.world_root, &str_arg("name")),
        "run_command" => run_command(ctx, &str_arg("command")),
        other => Err(format!("unknown tool: {other}")),
    }
}

/// Move a page and rewrite the [[wikilinks]] that pointed at it (mirrors the
/// HTTP move handler's cascade). Reindexes the old/new paths + every rewrite.
fn move_with_links(ctx: &ToolCtx<'_>, from: &str, to: &str) -> Result<(), String> {
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    let sources = ctx
        .state
        .with_index(&vault_root, |conn| index::sources_linking_to(conn, from))
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    vault::move_entry(&vault_root, from, to).map_err(app_err)?;
    crate::history::move_history(ctx.world_root, from, to);

    let (old_title, new_title) = (stem(from), stem(to));
    if !old_title.is_empty() && !old_title.eq_ignore_ascii_case(&new_title) {
        let mut seen = std::collections::HashSet::new();
        for (src, _) in sources {
            let src = if src == from { to.to_string() } else { src };
            if !seen.insert(src.clone()) {
                continue;
            }
            let Ok(page) = vault::read_page(&vault_root, &src) else {
                continue;
            };
            if let Some(updated) = index::rewrite_link_names(&page.content, &old_title, &new_title)
            {
                let _ = crate::history::record(ctx.world_root, &vault_root, &src, "keeper");
                if vault::write_page(&vault_root, &src, &updated).is_ok() {
                    reindex(ctx, &vault_root, &src);
                }
            }
        }
    }
    ctx.state.note_vault_write(&vault_root, from);
    let _ = ctx.state.with_index(&vault_root, |conn| {
        let _ = index::remove_path(conn, from);
    });
    reindex(ctx, &vault_root, to);
    Ok(())
}

/// Run a shell command with cwd = world root, a wall-clock cap and a combined
/// stdout+stderr cap. Minimal env (`PATH`/`HOME`/`LANG`) — no app secrets live
/// in env anyway. Output is data, not instructions (the loop wraps it).
fn run_command(ctx: &ToolCtx<'_>, command: &str) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let command = command.trim();
    if command.is_empty() {
        return Err("empty command".into());
    }
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("/bin/sh");
        c.args(["-c", command]);
        c
    };
    cmd.current_dir(ctx.world_root)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in ["PATH", "HOME", "LANG"] {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }

    let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;
    // Drain the pipes on threads so a chatty command can't deadlock on a full
    // pipe buffer while we poll for exit.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let read_all = |p: Option<std::process::ChildStdout>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = p {
                let _ = r.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out_t = read_all(out_pipe.take());
    let err_h = {
        let mut e = err_pipe.take();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(ref mut r) = e {
                let _ = r.read_to_end(&mut buf);
            }
            buf
        })
    };

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if start.elapsed() > SHELL_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(e) => return Err(format!("wait failed: {e}")),
        }
    };

    let stdout = String::from_utf8_lossy(&out_t.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_h.join().unwrap_or_default()).into_owned();

    let mut body = String::new();
    if status.is_none() {
        body.push_str(&format!(
            "[timed out after {}s — killed]\n",
            SHELL_TIMEOUT.as_secs()
        ));
    } else if let Some(s) = status {
        if !s.success() {
            body.push_str(&format!(
                "[exit status: {}]\n",
                s.code().map_or("signal".into(), |c| c.to_string())
            ));
        }
    }
    if !stdout.is_empty() {
        body.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !stdout.is_empty() {
            body.push('\n');
        }
        body.push_str("[stderr]\n");
        body.push_str(&stderr);
    }
    if body.is_empty() {
        body.push_str("[no output]");
    }
    if body.len() > SHELL_OUTPUT_CAP {
        let mut end = SHELL_OUTPUT_CAP;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push_str("\n[output truncated]");
    }
    Ok(body)
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

/// A ~`window`-char window of `text` centred on byte offset `at`, on char
/// boundaries, newlines flattened. `at` comes from a lowercased copy — fine for
/// the ASCII/Latin we index; clamped defensively.
fn snippet_around(text: &str, at: usize, window: usize) -> String {
    let at = at.min(text.len());
    let half = window / 2;
    let mut start = at.saturating_sub(half);
    let mut end = (at + half).min(text.len());
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    text[start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
        let ctx = ToolCtx {
            state: &state,
            world_root: &root,
            cfg: &cfg,
        };

        let pages = call(&ctx, "list_pages", json!({})).unwrap();
        assert!(pages.contains("Thornhold.md [place] — A fortified town."));
        let scoped = call(&ctx, "list_pages", json!({ "folder": "NPCs" })).unwrap();
        assert!(scoped.contains("Baron Aldric"));
        assert!(!scoped.contains("Thornhold.md"));

        let page = call(&ctx, "read_page", json!({ "path": "Thornhold.md" })).unwrap();
        assert!(page.contains("Ruled by [[Baron Aldric]]."));

        let hits = call(&ctx, "search_pages", json!({ "query": "fortified" })).unwrap();
        assert!(hits.contains("Thornhold.md"));

        let back = call(
            &ctx,
            "get_backlinks",
            json!({ "path": "NPCs/Baron Aldric.md" }),
        )
        .unwrap();
        assert!(back.contains("Thornhold.md"));

        let sessions = call(&ctx, "list_sessions", json!({})).unwrap();
        assert!(sessions.contains("Session 1 — Arrival (2026-05-01)"));

        let summary = call(&ctx, "read_summary", json!({ "session": 1 })).unwrap();
        assert!(summary.contains("reached Thornhold"));

        let found = call(&ctx, "search_transcripts", json!({ "query": "knock" })).unwrap();
        assert!(found.contains("session 1, turn 3: Lyra: I knock loudly."));

        let slice = call(
            &ctx,
            "read_transcript",
            json!({ "session": 1, "from_turn": 1, "to_turn": 2 }),
        )
        .unwrap();
        assert!(slice.contains("1: GM: You arrive at Thornhold."));
        assert!(!slice.contains("knock"));

        let kinds = call(&ctx, "page_kinds", json!({})).unwrap();
        assert!(kinds.contains("npc:") && kinds.contains("race:text"));

        let sum = call(&ctx, "search_summaries", json!({ "query": "Thornhold" })).unwrap();
        assert!(sum.contains("session 1") && sum.contains("Thornhold"));
        assert!(
            call(&ctx, "search_summaries", json!({ "query": "dragons" }))
                .unwrap()
                .contains("No summary matches")
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn diagnostics_and_tag_tools() {
        let (state, root, cfg) = fixture_world("diag");
        let vault_root = cfg.codex_dir(&root);
        // A page with a tag and a dangling link, indexed.
        std::fs::write(
            root.join("Codex/NPCs/Reeve.md"),
            "---\nkind: npc\ntags: [crown]\nsummary: The reeve.\n---\n\nServes [[Nobody Here]].\n",
        )
        .unwrap();
        state
            .with_index(&vault_root, |conn| {
                index::rebuild(conn, &vault_root).ok();
            })
            .unwrap();
        let ctx = ToolCtx {
            state: &state,
            world_root: &root,
            cfg: &cfg,
        };

        let diag = call(&ctx, "vault_diagnostics", json!({})).unwrap();
        assert!(diag.contains("[[Nobody Here]]"));

        let tags = call(&ctx, "tags", json!({})).unwrap();
        assert!(tags.contains("#crown"));

        let tagged = call(&ctx, "tags", json!({ "tag": "#crown" })).unwrap();
        assert!(tagged.contains("NPCs/Reeve.md"));
        assert!(call(&ctx, "tags", json!({ "tag": "nope" }))
            .unwrap()
            .contains("No pages tagged"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn write_tools_replace_all_multi_append_insert() {
        let (state, root, cfg) = fixture_world("writes");
        let ctx = ToolCtx {
            state: &state,
            world_root: &root,
            cfg: &cfg,
        };
        std::fs::write(
            root.join("Codex/Argent.md"),
            "---\nkind: place\nsummary: A city.\n---\n\nThe [[Argent]] gate. Visit [[Argent]] again.\n\n## Notes\n\nStub.\n",
        )
        .unwrap();

        // replace_all: the n>1 trap that used to force write_page.
        assert!(gate_preview(
            &ctx,
            "edit_page",
            &json!({ "path": "Argent.md", "old_str": "[[Argent]]", "new_str": "[[Argent City]]" })
        )
        .is_err());
        call(&ctx, "edit_page", json!({ "path": "Argent.md", "old_str": "[[Argent]]", "new_str": "[[Argent City]]", "replace_all": true })).unwrap();
        let c = std::fs::read_to_string(root.join("Codex/Argent.md")).unwrap();
        assert_eq!(c.matches("[[Argent City]]").count(), 2);

        // multi_edit: all-or-nothing — a bad later edit reverts nothing.
        assert!(call(
            &ctx,
            "multi_edit_page",
            json!({ "path": "Argent.md", "edits": [
            { "old_str": "A city.", "new_str": "A silver city." },
            { "old_str": "NOPE", "new_str": "x" }
        ] })
        )
        .is_err());
        let c = std::fs::read_to_string(root.join("Codex/Argent.md")).unwrap();
        assert!(c.contains("A city.")); // first edit not committed
        call(
            &ctx,
            "multi_edit_page",
            json!({ "path": "Argent.md", "edits": [
            { "old_str": "A city.", "new_str": "A silver city." },
            { "old_str": "Stub.", "new_str": "Founded long ago." }
        ] }),
        )
        .unwrap();
        let c = std::fs::read_to_string(root.join("Codex/Argent.md")).unwrap();
        assert!(c.contains("A silver city.") && c.contains("Founded long ago."));

        // append (no heading) + insert under heading.
        call(
            &ctx,
            "insert_into_page",
            json!({ "path": "Argent.md", "text": "Tail line." }),
        )
        .unwrap();
        call(
            &ctx,
            "insert_into_page",
            json!({ "path": "Argent.md", "heading": "## Notes", "text": "A fresh note." }),
        )
        .unwrap();
        let c = std::fs::read_to_string(root.join("Codex/Argent.md")).unwrap();
        assert!(c.contains("Tail line."));
        let notes = c.split("## Notes").nth(1).unwrap();
        assert!(notes.contains("A fresh note."));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn structural_tools_move_rename_delete() {
        let (state, root, cfg) = fixture_world("struct");
        let ctx = ToolCtx {
            state: &state,
            world_root: &root,
            cfg: &cfg,
        };
        // Seed the index so backlink rewrite has something to follow.
        let vault_root = cfg.codex_dir(&root);
        state
            .with_index(&vault_root, |conn| {
                index::rebuild(conn, &vault_root).ok();
            })
            .unwrap();

        // Rename rewrites the [[Baron Aldric]] link in Thornhold.md.
        let prev = gate_preview(
            &ctx,
            "rename_page",
            &json!({ "path": "NPCs/Baron Aldric.md", "new_name": "Baroness Mira" }),
        )
        .unwrap();
        assert_eq!(prev["to"], "NPCs/Baroness Mira.md");
        assert!(prev["summary"].as_str().unwrap().contains("rewrite 1 link"));
        call(
            &ctx,
            "rename_page",
            json!({ "path": "NPCs/Baron Aldric.md", "new_name": "Baroness Mira" }),
        )
        .unwrap();
        assert!(root.join("Codex/NPCs/Baroness Mira.md").is_file());
        assert!(!root.join("Codex/NPCs/Baron Aldric.md").exists());
        let thornhold = std::fs::read_to_string(root.join("Codex/Thornhold.md")).unwrap();
        assert!(thornhold.contains("[[Baroness Mira]]"));

        // Move into a new folder.
        call(&ctx, "create_folder", json!({ "path": "Rulers" })).unwrap();
        assert!(root.join("Codex/Rulers").is_dir());
        call(
            &ctx,
            "move_page",
            json!({ "path": "NPCs/Baroness Mira.md", "new_folder": "Rulers" }),
        )
        .unwrap();
        assert!(root.join("Codex/Rulers/Baroness Mira.md").is_file());

        // Delete.
        call(
            &ctx,
            "delete_page",
            json!({ "path": "Rulers/Baroness Mira.md" }),
        )
        .unwrap();
        assert!(!root.join("Codex/Rulers/Baroness Mira.md").exists());
        // Deleting a missing page is a conversational error.
        assert!(call(&ctx, "delete_page", json!({ "path": "Ghost.md" })).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn shell_runs_in_world_folder_and_caps() {
        let (state, root, cfg) = fixture_world("shell");
        let ctx = ToolCtx {
            state: &state,
            world_root: &root,
            cfg: &cfg,
        };
        if cfg!(windows) {
            return;
        }
        let out = call(&ctx, "run_command", json!({ "command": "ls Codex" })).unwrap();
        assert!(out.contains("Thornhold.md"));
        let fail = call(&ctx, "run_command", json!({ "command": "exit 3" })).unwrap();
        assert!(fail.contains("exit status: 3"));
        let stderr = call(&ctx, "run_command", json!({ "command": "echo oops 1>&2" })).unwrap();
        assert!(stderr.contains("[stderr]") && stderr.contains("oops"));
        assert!(gate_preview(&ctx, "run_command", &json!({ "command": "  " })).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn foundry_tool_gates_and_needs_config() {
        let (state, root, cfg) = fixture_world("foundry");
        let ctx = ToolCtx {
            state: &state,
            world_root: &root,
            cfg: &cfg,
        };
        for t in [
            "sync_foundry",
            "foundry_create_actor",
            "foundry_create_scene",
            "foundry_create_rolltable",
            "foundry_post_chat",
        ] {
            assert_eq!(tier_of(t), Tier::Foundry);
            assert!(is_foundry_async(t));
            assert!(gate_preview(&ctx, t, &json!({})).is_err());
        }
        // Live-play reads are ungated (Tier::Read) but still async + bridge-gated.
        for t in [
            "foundry_list_actors",
            "foundry_get_actor",
            "foundry_scene_state",
            "foundry_lookup",
            "foundry_system_info",
        ] {
            assert_eq!(tier_of(t), Tier::Read);
            assert!(is_foundry_async(t));
        }
        // Unconfigured bridge: every runner refuses before touching the network.
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt
            .block_on(run_foundry_sync(&ctx))
            .unwrap_err()
            .contains("not configured"));
        for t in [
            "foundry_create_actor",
            "foundry_create_scene",
            "foundry_create_rolltable",
            "foundry_post_chat",
            "foundry_list_actors",
            "foundry_get_actor",
            "foundry_scene_state",
            "foundry_lookup",
            "foundry_system_info",
        ] {
            let err = rt.block_on(run_foundry_tool(&ctx, t, &json!({}))).unwrap_err();
            assert!(err.contains("not configured"), "{t}: {err}");
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn errors_are_conversational() {
        let (state, root, cfg) = fixture_world("err");
        let ctx = ToolCtx {
            state: &state,
            world_root: &root,
            cfg: &cfg,
        };
        assert!(call(&ctx, "read_summary", json!({ "session": 99 })).is_err());
        assert!(call(&ctx, "nope", json!({})).is_err());
        assert!(call(&ctx, "read_page", json!({ "path": "../../etc/passwd" })).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn edit_tolerates_whitespace_variants() {
        // Exact match still wins and is untouched.
        let op = |old: &str, new: &str| EditOp {
            old: old.into(),
            new: new.into(),
            all: false,
        };
        let body = "## Notes\n\nThe   guard    stands\n\tat the gate.\n";
        // Model copied with single spaces / different indentation.
        let out = apply_edits(body, &[op("The guard stands at the gate.", "The guard left.")])
            .unwrap();
        assert!(out.contains("The guard left."));
        assert!(!out.contains("stands"));

        // Genuinely absent text still errors.
        assert!(apply_edits(body, &[op("nonexistent line", "x")]).is_err());

        // Ambiguous normalized match without replace_all errors (neither line
        // matches "a b" exactly, both match once whitespace-normalized).
        let dup = "a  b\na   b\n";
        assert!(apply_edits(dup, &[op("a b", "X")]).is_err());
        assert_eq!(
            apply_edits(
                dup,
                &[EditOp {
                    old: "a b".into(),
                    new: "X".into(),
                    all: true
                }]
            )
            .unwrap(),
            "X\nX\n"
        );
    }

    #[test]
    fn read_page_miss_suggests_neighbours() {
        let (state, root, cfg) = fixture_world("suggest");
        std::fs::create_dir_all(root.join("Codex/NPCs")).unwrap();
        std::fs::write(
            root.join("Codex/NPCs/Baron Aldric.md"),
            "---\nkind: npc\n---\n\nx\n",
        )
        .unwrap();
        let ctx = ToolCtx {
            state: &state,
            world_root: &root,
            cfg: &cfg,
        };
        let err = call(&ctx, "read_page", json!({ "path": "People/Baron Aldric.md" }))
            .unwrap_err();
        assert!(err.contains("Did you mean"));
        assert!(err.contains("NPCs/Baron Aldric.md"));
        std::fs::remove_dir_all(&root).ok();
    }
}
