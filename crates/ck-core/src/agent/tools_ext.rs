//! Keeper tools added after the core set: loop-handled helpers (`todo_write`,
//! `ask_user`, `delegate`), timeline / relations / history / trash reads,
//! `restore_page`, `edit_map`, and the post-write link lint.

use serde_json::{json, Value};

use super::tools::{app_err, cap_preview, norm_md_path, pct, reindex, resolve_map, ToolCtx};
use crate::llm::agent::ToolDef;
use crate::store::index;
use crate::{atlas, history, timeline, trash, vault};

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

/// Tools the loop answers itself (no dispatch): offered in every mode.
pub fn interactive_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "todo_write".into(),
            description: "Keep a visible checklist for a multi-step job (3+ steps). Send the whole list each time; exactly one item `in_progress`, mark items `completed` as you finish them. Skip it for simple requests.".into(),
            schema: obj(
                json!({
                    "todos": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string" },
                                "status": { "type": "string", "description": "pending, in_progress or completed" }
                            },
                            "required": ["content", "status"]
                        }
                    }
                }),
                &["todos"],
            ),
        },
        ToolDef {
            name: "ask_user".into(),
            description: "Ask the user one question and wait for the answer — use it when a choice is genuinely theirs (which of several directions, which page is meant, a preference you can't infer). Offer 2–6 short `options` when the answers are enumerable; the user can always type their own. Don't use it for things you can look up or decide.".into(),
            schema: obj(
                json!({
                    "question": { "type": "string" },
                    "options": { "type": "array", "items": { "type": "string" } }
                }),
                &["question"],
            ),
        },
    ]
}

pub fn delegate_tool() -> ToolDef {
    ToolDef {
        name: "delegate".into(),
        description: "Hand a self-contained read-only research job to a fresh worker with its own context and get back a short report — use it for sweeps that would flood your context (audit many pages, collect every mention of X, compare a faction across sessions). Write the task so it needs no other context and say what shape the report should take. The worker can read and search the world but cannot edit anything or ask the user.".into(),
        schema: obj(json!({ "task": { "type": "string" } }), &["task"]),
    }
}

pub fn is_loop_tool(name: &str) -> bool {
    matches!(name, "todo_write" | "ask_user" | "delegate")
}

pub fn read_ext_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "read_timeline".into(),
            description: "The world timeline: every dated page (and session with a world date) ordered on the world's own calendar, with display dates. Use it for 'what happened before/after X', 'when was Y' and era questions instead of sorting dates yourself. `query` filters by title/summary text.".into(),
            schema: obj(
                json!({
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "description": "default 60" }
                }),
                &[],
            ),
        },
        ToolDef {
            name: "read_relations".into(),
            description: "Typed relations (frontmatter [[links]], predicate = key) from the index. Filter by `path` (relations from or to that page) and/or `predicate` (e.g. allies, location, part_of).".into(),
            schema: obj(
                json!({
                    "path": { "type": "string" },
                    "predicate": { "type": "string" }
                }),
                &[],
            ),
        },
        ToolDef {
            name: "page_history".into(),
            description: "Saved versions of pages. No args: the newest changes world-wide (who: user or keeper). `path`: that page's versions. `path` + `ts`: the page text as it was in that version. Use it before restore_page.".into(),
            schema: obj(
                json!({
                    "path": { "type": "string" },
                    "ts": { "type": "integer" }
                }),
                &[],
            ),
        },
        ToolDef {
            name: "list_trash".into(),
            description: "Deleted pages and folders still in the 30-day trash (id, items). Restore a single page with restore_page.".into(),
            schema: obj(json!({}), &[]),
        },
    ]
}

pub fn write_ext_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "restore_page".into(),
            description: "Bring a page back: `source: history` writes an earlier version of `path` (get `ts` from page_history) over the current text; `source: trash` restores the single page in trash group `id` (from list_trash). Needs approval like any edit and is undoable.".into(),
            schema: obj(
                json!({
                    "source": { "type": "string", "description": "history or trash" },
                    "path": { "type": "string", "description": "page path (history)" },
                    "ts": { "type": "integer", "description": "version timestamp (history)" },
                    "id": { "type": "string", "description": "trash group id (trash)" }
                }),
                &["source"],
            ),
        },
        ToolDef {
            name: "edit_map".into(),
            description: "Change an Atlas map. `action`: move_pin (pin + x,y or near), update_pin (pin; name/kind/page/label — page \"\" unlinks), delete_pin (pin), add_region (name, points = [[x,y],…] ≥3 corners in 0–1, optional page/color), update_region (region; name/page/color), delete_region (region), set_scale (width, unit). Pins and regions are found by name or id — read_map first. Every change is undoable and kept in the map's history.".into(),
            schema: obj(
                json!({
                    "map": { "type": "string", "description": "map id or name" },
                    "action": { "type": "string" },
                    "pin": { "type": "string" },
                    "region": { "type": "string" },
                    "name": { "type": "string" },
                    "kind": { "type": "string" },
                    "label": { "type": "string" },
                    "page": { "type": "string" },
                    "color": { "type": "string", "description": "#rgb or #rrggbb" },
                    "x": { "type": "number" },
                    "y": { "type": "number" },
                    "near": { "type": "string" },
                    "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" } } },
                    "width": { "type": "number" },
                    "unit": { "type": "string" }
                }),
                &["map", "action"],
            ),
        },
    ]
}

pub fn is_ext_read(name: &str) -> bool {
    matches!(
        name,
        "read_timeline" | "read_relations" | "page_history" | "list_trash"
    )
}

pub fn is_ext_write(name: &str) -> bool {
    matches!(name, "restore_page" | "edit_map")
}

pub fn preview(ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "edit_map" => {
            let (doc, summary) = plan_edit_map(ctx, args)?;
            Ok(json!({
                "path": format!("Atlas/{}.json", doc.id),
                "action": "edit_map",
                "summary": summary,
                "map": doc.id,
            }))
        }
        "restore_page" => restore_preview(ctx, args),
        other => Err(format!("not an extension write tool: {other}")),
    }
}

pub fn dispatch(ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Option<Result<String, String>> {
    Some(match name {
        "read_timeline" => read_timeline(ctx, args),
        "read_relations" => read_relations(ctx, args),
        "page_history" => page_history(ctx, args),
        "list_trash" => Ok(list_trash(ctx)),
        "restore_page" => restore_page(ctx, args),
        "edit_map" => plan_edit_map(ctx, args).and_then(|(doc, summary)| {
            atlas::write_map_as(ctx.world_root, &doc, "keeper").map_err(app_err)?;
            Ok(format!("{summary}."))
        }),
        "todo_write" | "ask_user" | "delegate" => {
            Err("This tool is answered by the agent loop.".into())
        }
        _ => return None,
    })
}

fn s<'a>(args: &'a Value, k: &str) -> &'a str {
    args.get(k).and_then(Value::as_str).unwrap_or("").trim()
}

// ── Reads ─────────────────────────────────────────────────────────

fn read_timeline(ctx: &ToolCtx<'_>, args: &Value) -> Result<String, String> {
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    let mut rows = ctx
        .state
        .with_index(&vault_root, index::all_frontmatter)
        .map_err(app_err)?
        .map_err(app_err)?;
    if let Ok(sessions) = ctx
        .state
        .with_db(|conn| crate::store::sessions::world_dated_session_rows(conn, &ctx.cfg.id))
    {
        rows.extend(sessions);
    }
    let events = timeline::world_events(rows, &ctx.cfg.calendar);
    let q = s(args, "query").to_lowercase();
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(60)
        .clamp(1, 200) as usize;
    let mut lines = Vec::new();
    let mut total = 0usize;
    for ev in &events {
        let title = ev["title"].as_str().unwrap_or("");
        let summary = ev["summary"].as_str().unwrap_or("");
        if !q.is_empty()
            && !title.to_lowercase().contains(&q)
            && !summary.to_lowercase().contains(&q)
        {
            continue;
        }
        total += 1;
        if lines.len() >= limit {
            continue;
        }
        let when = match ev["display"].as_str() {
            Some(d) => match ev["end_display"].as_str() {
                Some(e) => format!("{d} → {e}"),
                None => d.to_string(),
            },
            None => "undated (relative order)".into(),
        };
        let gm = if ev["gm_only"] == true {
            " [GM-only]"
        } else {
            ""
        };
        let path = ev["path"].as_str().unwrap_or("");
        let sum = if summary.is_empty() {
            String::new()
        } else {
            format!(" — {summary}")
        };
        lines.push(format!("- {when}: {title} ({path}){gm}{sum}"));
    }
    if lines.is_empty() {
        return Ok("No dated pages. Pages get on the timeline with a `date:` frontmatter field (see writing-codex-syntax).".into());
    }
    let mut out = lines.join("\n");
    if total > lines.len() {
        out.push_str(&format!(
            "\n… {} more — narrow with `query` or raise `limit`.",
            total - lines.len()
        ));
    }
    Ok(out)
}

fn read_relations(ctx: &ToolCtx<'_>, args: &Value) -> Result<String, String> {
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    let rows = ctx
        .state
        .with_index(&vault_root, index::all_relations)
        .map_err(app_err)?
        .map_err(app_err)?;
    let path = norm_md_path(s(args, "path"));
    let has_path = !s(args, "path").is_empty();
    let pred = s(args, "predicate").to_lowercase();
    let lines: Vec<String> = rows
        .iter()
        .filter(|r| {
            (!has_path || r.source_path == path || r.target_path.as_deref() == Some(path.as_str()))
                && (pred.is_empty() || r.predicate.to_lowercase() == pred)
        })
        .map(|r| {
            let target = match &r.target_path {
                Some(t) => t.clone(),
                None => format!("{} (unresolved)", r.link_text),
            };
            format!("- {} —{}→ {}", r.source_path, r.predicate, target)
        })
        .collect();
    if lines.is_empty() {
        return Ok("No matching relations.".into());
    }
    let total = lines.len();
    let mut out = lines.into_iter().take(150).collect::<Vec<_>>().join("\n");
    if total > 150 {
        out.push_str(&format!(
            "\n… {} more — filter by path or predicate.",
            total - 150
        ));
    }
    Ok(out)
}

fn page_history(ctx: &ToolCtx<'_>, args: &Value) -> Result<String, String> {
    let raw = s(args, "path");
    if raw.is_empty() {
        let recent = history::recent(ctx.world_root, None, 30);
        if recent.is_empty() {
            return Ok("No saved versions yet.".into());
        }
        return Ok(recent
            .iter()
            .map(|r| format!("- {} @ {} ({})", r.path, r.ts, r.origin))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    let path = norm_md_path(raw);
    if let Some(ts) = args.get("ts").and_then(Value::as_u64) {
        let (meta, content) = history::read_version(ctx.world_root, &path, ts).map_err(app_err)?;
        return Ok(match content {
            Some(c) => format!(
                "{path} as of {} ({}):\n\n{}",
                meta.ts,
                meta.origin,
                crate::gm::annotate(&c)
            ),
            None => format!("{path} did not exist at version {ts}."),
        });
    }
    let versions = history::list_page(ctx.world_root, &path).map_err(app_err)?;
    if versions.is_empty() {
        return Ok(format!("No saved versions of {path}."));
    }
    Ok(versions
        .iter()
        .rev()
        .map(|v| format!("- ts {} ({}) — page text before that save", v.ts, v.origin))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn list_trash(ctx: &ToolCtx<'_>) -> String {
    let groups = trash::list(ctx.world_root);
    if groups.is_empty() {
        return "The trash is empty.".into();
    }
    groups
        .iter()
        .map(|g| {
            let items: Vec<String> = g
                .items
                .iter()
                .map(|i| format!("{} ({}, {} page(s))", i.rel, i.kind, i.pages))
                .collect();
            format!("- id {} — {}", g.id, items.join("; "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── restore_page ──────────────────────────────────────────────────

fn single_trash_page(ctx: &ToolCtx<'_>, id: &str) -> Result<String, String> {
    let group = trash::list(ctx.world_root)
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("No trash group “{id}” — call list_trash."))?;
    match group.items.as_slice() {
        [item] if item.kind == "page" => Ok(item.rel.clone()),
        _ => Err("Only a single deleted page can be restored here — restore folders from the Trash view in the Codex.".into()),
    }
}

fn restore_preview(ctx: &ToolCtx<'_>, args: &Value) -> Result<Value, String> {
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    match s(args, "source") {
        "history" => {
            let path = norm_md_path(s(args, "path"));
            let ts = args
                .get("ts")
                .and_then(Value::as_u64)
                .ok_or("give `ts` — see page_history")?;
            let (_, snapshot) =
                history::read_version(ctx.world_root, &path, ts).map_err(app_err)?;
            let snapshot =
                snapshot.ok_or("That version predates the page — nothing to restore to.")?;
            let old = vault::read_page(&vault_root, &path)
                .ok()
                .map_or(Value::Null, |p| Value::String(cap_preview(&p.content)));
            Ok(json!({ "path": path, "old": old, "new": cap_preview(&snapshot) }))
        }
        "trash" => {
            let rel = single_trash_page(ctx, s(args, "id"))?;
            Ok(
                json!({ "path": rel, "old": Value::Null, "new": format!("Restore {rel} from the trash") }),
            )
        }
        _ => Err("`source` must be history or trash".into()),
    }
}

fn restore_page(ctx: &ToolCtx<'_>, args: &Value) -> Result<String, String> {
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    match s(args, "source") {
        "history" => {
            let path = norm_md_path(s(args, "path"));
            let ts = args
                .get("ts")
                .and_then(Value::as_u64)
                .ok_or("give `ts` — see page_history")?;
            let (_, snapshot) =
                history::read_version(ctx.world_root, &path, ts).map_err(app_err)?;
            let content = snapshot.ok_or("That version predates the page.")?;
            vault::write_page(&vault_root, &path, &content).map_err(app_err)?;
            reindex(ctx, &vault_root, &path);
            Ok(format!("Restored {path} to version {ts}."))
        }
        "trash" => {
            let rel = single_trash_page(ctx, s(args, "id"))?;
            let restored =
                trash::restore(ctx.world_root, &vault_root, s(args, "id")).map_err(app_err)?;
            for r in &restored {
                reindex(ctx, &vault_root, r);
            }
            Ok(format!(
                "Restored {} from the trash.",
                restored.first().map_or(rel, Clone::clone)
            ))
        }
        _ => Err("`source` must be history or trash".into()),
    }
}

// ── edit_map ──────────────────────────────────────────────────────

fn find_idx<T>(items: &[T], key: &str, id: fn(&T) -> &str, name: fn(&T) -> &str) -> Option<usize> {
    let k = key.trim().to_lowercase();
    items
        .iter()
        .position(|i| id(i).to_lowercase() == k)
        .or_else(|| items.iter().position(|i| name(i).to_lowercase() == k))
}

fn unit_ok(v: f64) -> bool {
    v.is_finite() && (0.0..=1.0).contains(&v)
}

fn coords(doc: &atlas::MapDoc, args: &Value) -> Result<(f64, f64), String> {
    let num = |k: &str| args.get(k).and_then(Value::as_f64);
    let (x, y) = match (num("x"), num("y"), s(args, "near")) {
        (Some(x), Some(y), _) => (x, y),
        (_, _, near) if !near.is_empty() => {
            let (_, nx, ny) = atlas::find_endpoint(doc, near)
                .ok_or_else(|| format!("No pin or region “{near}” — check read_map."))?;
            ((nx + 0.03).min(1.0), (ny + 0.03).min(1.0))
        }
        _ => return Err("give both `x` and `y` (0–1 from the top-left) or `near`".into()),
    };
    if !(unit_ok(x) && unit_ok(y)) {
        return Err("`x` and `y` must be between 0 and 1".into());
    }
    Ok((x, y))
}

fn linked_page(ctx: &ToolCtx<'_>, raw: &str) -> Result<Option<String>, String> {
    if raw.is_empty() {
        return Ok(None);
    }
    let path = norm_md_path(raw);
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    vault::read_page(&vault_root, &path)
        .map_err(|_| format!("Page not found: {path} — create it first."))?;
    Ok(Some(path))
}

fn plan_edit_map(ctx: &ToolCtx<'_>, args: &Value) -> Result<(atlas::MapDoc, String), String> {
    let mut doc = resolve_map(ctx, s(args, "map"))?;
    let map_name = doc.name.clone();
    let summary = match s(args, "action") {
        "move_pin" => {
            let i = find_idx(&doc.pins, s(args, "pin"), |p| &p.id, |p| &p.name)
                .ok_or_else(|| format!("No pin “{}” on “{map_name}” — check read_map.", s(args, "pin")))?;
            let (x, y) = coords(&doc, args)?;
            let pin = &mut doc.pins[i];
            pin.x = x;
            pin.y = y;
            format!("move pin “{}” on “{map_name}” to {}, {}", pin.name, pct(x), pct(y))
        }
        "update_pin" => {
            let i = find_idx(&doc.pins, s(args, "pin"), |p| &p.id, |p| &p.name)
                .ok_or_else(|| format!("No pin “{}” on “{map_name}” — check read_map.", s(args, "pin")))?;
            let new_name = s(args, "name");
            if !new_name.is_empty()
                && doc
                    .pins
                    .iter()
                    .enumerate()
                    .any(|(j, p)| j != i && p.name.eq_ignore_ascii_case(new_name))
            {
                return Err(format!("“{new_name}” is already pinned on “{map_name}”."));
            }
            let kind = s(args, "kind");
            if !kind.is_empty() && !super::tools::PIN_KINDS.contains(&kind) {
                return Err(format!("`kind` must be one of: {}", super::tools::PIN_KINDS.join(", ")));
            }
            let page = if args.get("page").is_some() {
                Some(linked_page(ctx, s(args, "page"))?)
            } else {
                None
            };
            let pin = &mut doc.pins[i];
            let before = pin.name.clone();
            if !new_name.is_empty() {
                pin.name = new_name.to_string();
            }
            if !kind.is_empty() {
                pin.kind = kind.to_string();
            }
            if let Some(p) = page {
                pin.page = p;
            }
            if args.get("label").is_some() {
                pin.label = Some(s(args, "label").to_string()).filter(|l| !l.is_empty());
            }
            format!("update pin “{before}” on “{map_name}”")
        }
        "delete_pin" => {
            let i = find_idx(&doc.pins, s(args, "pin"), |p| &p.id, |p| &p.name)
                .ok_or_else(|| format!("No pin “{}” on “{map_name}” — check read_map.", s(args, "pin")))?;
            let pin = doc.pins.remove(i);
            doc.pinned_previews.retain(|p| p.pin_id != pin.id);
            format!("delete pin “{}” from “{map_name}”", pin.name)
        }
        "add_region" => {
            let name = s(args, "name");
            if name.is_empty() || name.chars().count() > 120 {
                return Err("`name` must be 1–120 characters".into());
            }
            if doc.regions.iter().any(|r| r.name.eq_ignore_ascii_case(name)) {
                return Err(format!("A region “{name}” already exists on “{map_name}”."));
            }
            let points: Vec<[f64; 2]> = args
                .get("points")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|p| {
                            let p = p.as_array()?;
                            Some([p.first()?.as_f64()?, p.get(1)?.as_f64()?])
                        })
                        .collect()
                })
                .unwrap_or_default();
            if points.len() < 3 || points.iter().any(|p| !(unit_ok(p[0]) && unit_ok(p[1]))) {
                return Err("`points` needs ≥3 [x,y] corners, each between 0 and 1".into());
            }
            let page = linked_page(ctx, s(args, "page"))?;
            let mut id = format!("r{:x}", chrono::Utc::now().timestamp_millis());
            while doc.regions.iter().any(|r| r.id == id) {
                id.push('x');
            }
            doc.regions.push(atlas::Region {
                id,
                name: name.to_string(),
                points,
                page,
                color: Some(s(args, "color").to_string()).filter(|c| !c.is_empty()),
            });
            format!("add region “{name}” to “{map_name}”")
        }
        "update_region" => {
            let i = find_idx(&doc.regions, s(args, "region"), |r| &r.id, |r| &r.name)
                .ok_or_else(|| format!("No region “{}” on “{map_name}” — check read_map.", s(args, "region")))?;
            let page = if args.get("page").is_some() {
                Some(linked_page(ctx, s(args, "page"))?)
            } else {
                None
            };
            let r = &mut doc.regions[i];
            let before = r.name.clone();
            if !s(args, "name").is_empty() {
                r.name = s(args, "name").to_string();
            }
            if let Some(p) = page {
                r.page = p;
            }
            if args.get("color").is_some() {
                r.color = Some(s(args, "color").to_string()).filter(|c| !c.is_empty());
            }
            format!("update region “{before}” on “{map_name}”")
        }
        "delete_region" => {
            let i = find_idx(&doc.regions, s(args, "region"), |r| &r.id, |r| &r.name)
                .ok_or_else(|| format!("No region “{}” on “{map_name}” — check read_map.", s(args, "region")))?;
            let r = doc.regions.remove(i);
            format!("delete region “{}” from “{map_name}”", r.name)
        }
        "set_scale" => {
            let width = args.get("width").and_then(Value::as_f64).unwrap_or(0.0);
            let unit = s(args, "unit");
            if !(width.is_finite() && width > 0.0) || unit.is_empty() {
                return Err("set_scale needs a positive `width` (the full image width) and a `unit`".into());
            }
            doc.scale = Some(atlas::MapScale {
                width,
                unit: unit.to_string(),
            });
            format!("set scale of “{map_name}” to {width} {unit} across the image")
        }
        other => {
            return Err(format!(
                "unknown action “{other}” — use move_pin, update_pin, delete_pin, add_region, update_region, delete_region or set_scale"
            ))
        }
    };
    Ok((doc, summary))
}

// ── Post-write lint / citation check ──────────────────────────────

/// After a Keeper page write: wikilinks in that page that resolve to nothing.
pub fn lint_written_page(ctx: &ToolCtx<'_>, path: &str) -> Option<String> {
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    let broken: Vec<String> = ctx
        .state
        .with_index(&vault_root, |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT link_text FROM page_links WHERE source_path = ?1 AND target_path IS NULL ORDER BY link_text",
                )
                .ok()?;
            let rows = stmt
                .query_map([path], |r| r.get::<_, String>(0))
                .ok()?;
            Some(rows.filter_map(Result::ok).collect::<Vec<_>>())
        })
        .ok()??;
    if broken.is_empty() {
        return None;
    }
    let list: Vec<String> = broken.iter().take(10).map(|b| format!("[[{b}]]")).collect();
    Some(format!(
        "\n\nLink check: {} link(s) in this page match no page yet: {}. Fix the name, or create the page if it should exist (they show as stubs until then).",
        broken.len(),
        list.join(", ")
    ))
}

/// `[[Title]]` citations in a reply that match no page title or alias.
pub fn unresolved_citations(ctx: &ToolCtx<'_>, text: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let inner = &after[..end];
        rest = &after[end + 2..];
        let target = inner.split('|').next().unwrap_or(inner);
        let target = target.split('#').next().unwrap_or(target).trim();
        if target.is_empty() || target.len() > 120 || target.contains('\n') {
            continue;
        }
        if !seen.iter().any(|t| t.eq_ignore_ascii_case(target)) {
            seen.push(target.to_string());
        }
    }
    let vault_root = ctx.cfg.codex_dir(ctx.world_root);
    seen.into_iter()
        .filter(|t| {
            let want = index::normalize_name(t);
            let found = ctx
                .state
                .with_index(&vault_root, |conn| {
                    conn.query_row(
                        "SELECT 1 FROM page_aliases WHERE alias = ?1 LIMIT 1",
                        [&want],
                        |_| Ok(()),
                    )
                    .is_ok()
                })
                .unwrap_or(true);
            !found
        })
        .collect()
}
