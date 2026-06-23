//! Phase 25 live-play reads: parse the world snapshot (`FoundryClient::fetch_world`)
//! into answers a GM asks mid-session. **System-agnostic by design** — we read only
//! the core document fields every system shares (`name`/`type`/`img`/`items`/`folder`,
//! the active scene, its tokens). Mechanical stats live in the per-system `system.*`
//! blob, which we never parse: `get_actor` hands that raw JSON to the LLM to interpret
//! (5e `system.attributes.hp` vs Daggerheart's shape), so no per-system schema library.

use serde_json::Value;

/// Top-level world array, tolerating either a bare array or a `{ contents: [...] }`
/// wrapper (Foundry collections serialize both ways across majors).
pub(crate) fn collection<'a>(world: &'a Value, key: &str) -> Vec<&'a Value> {
    match world.get(key) {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(Value::Object(o)) => o
            .get("contents")
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A flat "Name (type)" listing of every actor in the world.
pub fn list_actors(world: &Value) -> String {
    let actors = collection(world, "actors");
    if actors.is_empty() {
        return "No actors found in the connected Foundry world.".into();
    }
    let mut lines: Vec<String> = actors
        .iter()
        .map(|a| {
            let name = str_field(a, "name");
            let ty = str_field(a, "type");
            if ty.is_empty() {
                name.to_string()
            } else {
                format!("{name} ({ty})")
            }
        })
        .filter(|l| !l.is_empty())
        .collect();
    lines.sort_unstable();
    format!("{} actor(s):\n{}", lines.len(), lines.join("\n"))
}

const MAX_SYSTEM_BLOB: usize = 4000;

/// Core fields for one actor (case-insensitive name match) plus its raw `system`
/// JSON — the LLM interprets the system-specific stats itself.
pub fn get_actor(world: &Value, name: &str) -> String {
    let want = name.trim().to_lowercase();
    let actors = collection(world, "actors");
    let found = actors
        .iter()
        .find(|a| str_field(a, "name").to_lowercase() == want)
        .or_else(|| {
            actors
                .iter()
                .find(|a| str_field(a, "name").to_lowercase().contains(&want))
        });
    let Some(actor) = found else {
        return format!("No actor named “{name}” found. Try foundry_list_actors to see what exists.");
    };

    let mut out = format!("Actor: {}\n", str_field(actor, "name"));
    let ty = str_field(actor, "type");
    if !ty.is_empty() {
        out.push_str(&format!("Type: {ty}\n"));
    }
    let img = str_field(actor, "img");
    if !img.is_empty() {
        out.push_str(&format!("Image: {img}\n"));
    }

    let items = actor
        .get("items")
        .map(|i| match i {
            Value::Array(a) => a.iter().collect::<Vec<_>>(),
            Value::Object(o) => o
                .get("contents")
                .and_then(Value::as_array)
                .map(|a| a.iter().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        })
        .unwrap_or_default();
    if !items.is_empty() {
        out.push_str(&format!("Items ({}):\n", items.len()));
        for it in &items {
            let n = str_field(it, "name");
            let t = str_field(it, "type");
            out.push_str(&format!("  - {n}{}\n", if t.is_empty() { String::new() } else { format!(" ({t})") }));
        }
    }

    match actor.get("system") {
        Some(sys) if !sys.is_null() => {
            let blob = serde_json::to_string_pretty(sys).unwrap_or_default();
            out.push_str("\nRaw system stats (game-system specific — interpret for the user):\n");
            if blob.len() > MAX_SYSTEM_BLOB {
                out.push_str(&blob[..MAX_SYSTEM_BLOB]);
                out.push_str("\n… (truncated)");
            } else {
                out.push_str(&blob);
            }
        }
        _ => out.push_str("\n(No system stat block on this actor.)"),
    }
    out
}

/// The active scene: name, dimensions, and the tokens placed on it (token name +
/// linked actor id, so the LLM can cross-reference with get_actor).
pub fn scene_state(world: &Value) -> String {
    let scenes = collection(world, "scenes");
    if scenes.is_empty() {
        return "No scenes in the connected Foundry world.".into();
    }
    let active = scenes
        .iter()
        .find(|s| s.get("active").and_then(Value::as_bool).unwrap_or(false))
        .copied()
        .or_else(|| scenes.first().copied());
    let Some(scene) = active else {
        return "No active scene.".into();
    };

    let mut out = format!("Active scene: {}", str_field(scene, "name"));
    if let (Some(w), Some(h)) = (
        scene.get("width").and_then(Value::as_u64),
        scene.get("height").and_then(Value::as_u64),
    ) {
        out.push_str(&format!(" ({w}×{h})"));
    }
    out.push('\n');

    let tokens = match scene.get("tokens") {
        Some(Value::Array(a)) => a.iter().collect::<Vec<_>>(),
        Some(Value::Object(o)) => o
            .get("contents")
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    if tokens.is_empty() {
        out.push_str("No tokens placed.");
        return out;
    }
    out.push_str(&format!("{} token(s) on scene:\n", tokens.len()));
    for t in &tokens {
        let n = str_field(t, "name");
        let hidden = t.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        let actor_id = str_field(t, "actorId");
        let mut line = format!("  - {n}");
        if !actor_id.is_empty() {
            line.push_str(&format!(" [actor {actor_id}]"));
        }
        if hidden {
            line.push_str(" (hidden)");
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

const MAX_LOOKUP_HITS: usize = 40;

/// Substring search over the installed system's **compendium pack indices** — the
/// GM's own rules/skills/items, matching their system + version (Option A in the
/// Phase 25 spec; beats a web search). Each pack carries `metadata` + an `index`
/// array of `{ name, type }`. Packs without a loaded index are skipped (Foundry
/// lazy-loads some); we report when nothing matched so the caller knows.
pub fn lookup(world: &Value, query: &str) -> String {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return "Empty lookup query.".into();
    }
    let packs = collection(world, "packs");
    let mut hits: Vec<String> = Vec::new();
    let mut indexed_packs = 0usize;
    for pack in &packs {
        let meta = pack.get("metadata").unwrap_or(pack);
        let label = {
            let l = str_field(meta, "label");
            if l.is_empty() {
                str_field(meta, "name")
            } else {
                l
            }
        };
        let index = pack.get("index").and_then(Value::as_array);
        if index.is_some() {
            indexed_packs += 1;
        }
        for entry in index.map(|a| a.iter().collect::<Vec<_>>()).unwrap_or_default() {
            let name = str_field(entry, "name");
            if name.to_lowercase().contains(&q) {
                let ty = str_field(entry, "type");
                hits.push(format!(
                    "  - {name}{} — {label}",
                    if ty.is_empty() { String::new() } else { format!(" ({ty})") }
                ));
                if hits.len() >= MAX_LOOKUP_HITS {
                    break;
                }
            }
        }
        if hits.len() >= MAX_LOOKUP_HITS {
            break;
        }
    }
    if hits.is_empty() {
        if indexed_packs == 0 {
            return format!(
                "No compendium index was available in the world snapshot to search for “{query}”. \
                 The system's packs may load their index lazily; ask the user to open the relevant \
                 compendium in Foundry, or answer from the codex instead."
            );
        }
        return format!("No compendium entries matched “{query}” across {indexed_packs} pack(s).");
    }
    format!(
        "{} compendium match(es) for “{query}”:\n{}",
        hits.len(),
        hits.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn world() -> Value {
        json!({
            "actors": [
                { "_id": "a1", "name": "Goblin Boss", "type": "npc", "img": "x.png",
                  "items": [{ "name": "Scimitar", "type": "weapon" }],
                  "system": { "attributes": { "hp": { "value": 21, "max": 21 } } } },
                { "_id": "a2", "name": "Aria", "type": "character", "items": [] }
            ],
            "scenes": [
                { "_id": "s1", "name": "Tavern", "active": false, "width": 1000, "height": 800, "tokens": [] },
                { "_id": "s2", "name": "Ambush", "active": true, "width": 2000, "height": 2000,
                  "tokens": [
                    { "name": "Goblin Boss", "actorId": "a1", "hidden": false },
                    { "name": "Scout", "actorId": "a3", "hidden": true }
                  ] }
            ],
            "packs": [
                { "metadata": { "label": "SRD Skills", "type": "JournalEntry" },
                  "index": [{ "name": "Stealth", "type": "skill" }, { "name": "Perception" }] }
            ]
        })
    }

    #[test]
    fn lists_actors_sorted() {
        let out = list_actors(&world());
        assert!(out.contains("2 actor(s):"));
        assert!(out.contains("Aria (character)"));
        assert!(out.contains("Goblin Boss (npc)"));
        assert!(out.find("Aria").unwrap() < out.find("Goblin Boss").unwrap());
    }

    #[test]
    fn gets_actor_with_raw_system() {
        let out = get_actor(&world(), "goblin boss");
        assert!(out.contains("Actor: Goblin Boss"));
        assert!(out.contains("Type: npc"));
        assert!(out.contains("Scimitar (weapon)"));
        assert!(out.contains("\"hp\""));
    }

    #[test]
    fn actor_partial_match_and_miss() {
        assert!(get_actor(&world(), "aria").contains("Actor: Aria"));
        assert!(get_actor(&world(), "nobody").contains("No actor named"));
    }

    #[test]
    fn scene_state_prefers_active() {
        let out = scene_state(&world());
        assert!(out.contains("Active scene: Ambush (2000×2000)"));
        assert!(out.contains("Goblin Boss [actor a1]"));
        assert!(out.contains("Scout [actor a3] (hidden)"));
    }

    #[test]
    fn lookup_matches_compendium_index() {
        let out = lookup(&world(), "stealth");
        assert!(out.contains("Stealth (skill) — SRD Skills"));
        assert!(lookup(&world(), "fireball").contains("No compendium entries matched"));
    }

    #[test]
    fn empty_world_is_graceful() {
        let empty = json!({});
        assert!(list_actors(&empty).contains("No actors"));
        assert!(scene_state(&empty).contains("No scenes"));
        assert!(get_actor(&empty, "x").contains("No actor named"));
    }
}
