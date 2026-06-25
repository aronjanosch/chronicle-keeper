//! System introspection for the Keeper: what document types this world's game
//! system defines (Actor/Item) and the default `system` data model per type — so
//! the Keeper can stat a new actor on a **fresh, empty world** without a sample
//! actor to copy. Schema sources, in order of preference:
//!   1. the world snapshot's `model`/`documentTypes` (Foundry ships the merged
//!      template to every client — already flattened, no work);
//!   2. the system's `template.json` (template-based systems), flattened here;
//!   3. nothing — modern **DataModel** systems define their schema in JS classes
//!      with no static file, so we say so and point back to sampling.

use crate::foundry::read::collection;
use serde_json::{Map, Value};

/// Recursively merges `src` into `target` (objects deep-merge; other values
/// overwrite). Mirrors how Foundry layers `templates` partials onto a type.
fn deep_merge(target: &mut Map<String, Value>, src: &Map<String, Value>) {
    for (k, v) in src {
        match (target.get_mut(k), v) {
            (Some(Value::Object(t)), Value::Object(s)) => deep_merge(t, s),
            _ => {
                target.insert(k.clone(), v.clone());
            }
        }
    }
}

/// Flattens one `template.json` document type: merge each `templates: [...]`
/// partial it references, then overlay the type's own fields (minus the meta
/// `templates` key). `class` is the document-class object (e.g. the `Actor`
/// value, carrying `templates` + a key per type).
pub fn flatten_type(class: &Value, ty: &str) -> Value {
    let mut out: Map<String, Value> = Map::new();
    let Some(type_obj) = class.get(ty).and_then(Value::as_object) else {
        return Value::Object(out);
    };
    if let Some(names) = type_obj.get("templates").and_then(Value::as_array) {
        for name in names.iter().filter_map(Value::as_str) {
            if let Some(tmpl) = class
                .get("templates")
                .and_then(|t| t.get(name))
                .and_then(Value::as_object)
            {
                deep_merge(&mut out, tmpl);
            }
        }
    }
    for (k, v) in type_obj {
        if k != "templates" {
            deep_merge(&mut out, &{
                let mut m = Map::new();
                m.insert(k.clone(), v.clone());
                m
            });
        }
    }
    Value::Object(out)
}

/// The type names a document class defines (`Actor` → its actor types). Prefers
/// the snapshot's `documentTypes`, then `template.json` `types`, then the keys of
/// the snapshot `model`, and finally the distinct types of existing documents.
fn types_for(world: &Value, template: Option<&Value>, doc: &str) -> Vec<String> {
    if let Some(arr) = world
        .get("documentTypes")
        .and_then(|d| d.get(doc))
        .and_then(Value::as_array)
    {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
    }
    if let Some(arr) = template
        .and_then(|t| t.get(doc))
        .and_then(|c| c.get("types"))
        .and_then(Value::as_array)
    {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
    }
    if let Some(obj) = world
        .get("model")
        .and_then(|m| m.get(doc))
        .and_then(Value::as_object)
    {
        return obj.keys().cloned().collect();
    }
    // Last resort: scan the live documents for distinct `type`s.
    let key = match doc {
        "Actor" => "actors",
        "Item" => "items",
        _ => return Vec::new(),
    };
    let mut seen: Vec<String> = Vec::new();
    for d in collection(world, key) {
        if let Some(t) = d.get("type").and_then(Value::as_str) {
            if !t.is_empty() && !seen.iter().any(|s| s == t) {
                seen.push(t.to_string());
            }
        }
    }
    seen
}

/// The default `system` block for one `doc`/`type`: snapshot `model` first
/// (already flattened by Foundry), else the flattened `template.json` type. An
/// empty object (DataModel system, no static fields) is reported as `None`.
fn default_system(world: &Value, template: Option<&Value>, doc: &str, ty: &str) -> Option<Value> {
    if let Some(m) = world
        .get("model")
        .and_then(|m| m.get(doc))
        .and_then(|d| d.get(ty))
    {
        if m.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
            return Some(m.clone());
        }
    }
    let flat = template.and_then(|t| t.get(doc)).map(|c| flatten_type(c, ty))?;
    match &flat {
        Value::Object(o) if !o.is_empty() => Some(flat),
        _ => None,
    }
}

const MAX_SCHEMA: usize = 6000;

/// Human/LLM-readable system overview. With `want_type`, appends that type's
/// default `system` schema (the fill scaffold). `status` is the unauthenticated
/// `/api/status` payload (system id + version), best-effort.
pub fn system_info(
    world: &Value,
    status: Option<&Value>,
    template: Option<&Value>,
    doc: &str,
    want_type: Option<&str>,
) -> String {
    let mut out = String::new();

    let sys_id = status
        .and_then(|s| s.get("system"))
        .and_then(Value::as_str)
        .unwrap_or("?");
    let ver = status
        .and_then(|s| s.get("version"))
        .and_then(Value::as_str)
        .unwrap_or("");
    out.push_str(&format!(
        "Game system: {sys_id}{}\n",
        if ver.is_empty() {
            String::new()
        } else {
            format!(" v{ver}")
        }
    ));

    let actor_types = types_for(world, template, "Actor");
    let item_types = types_for(world, template, "Item");
    out.push_str(&format!(
        "Actor types: {}\n",
        if actor_types.is_empty() {
            "(unknown)".into()
        } else {
            actor_types.join(", ")
        }
    ));
    out.push_str(&format!(
        "Item types: {}\n",
        if item_types.is_empty() {
            "(unknown)".into()
        } else {
            item_types.join(", ")
        }
    ));

    let Some(ty) = want_type else {
        out.push_str(
            "\nCall again with a type to get its default `system` schema to fill, \
             e.g. { \"doc\": \"Actor\", \"type\": \"npc\" }.",
        );
        return out;
    };

    out.push_str(&format!("\nDefault `system` schema for {doc}.{ty}:\n"));
    match default_system(world, template, doc, ty) {
        Some(schema) => {
            let blob = serde_json::to_string_pretty(&schema).unwrap_or_default();
            if blob.len() > MAX_SCHEMA {
                out.push_str(&blob[..MAX_SCHEMA]);
                out.push_str("\n… (truncated)");
            } else {
                out.push_str(&blob);
            }
            out.push_str(
                "\n\nFill these field paths with values for the NPC, then pass as `system` to \
                 foundry_create_actor. Defaults shown are the system's empty/zero values.",
            );
        }
        None => out.push_str(&format!(
            "(No static schema — this system defines {doc}.{ty} in code (a DataModel), so the \
             field shape isn't fetchable. Use foundry_get_actor on an existing {ty} to read its \
             real `system` shape, or foundry_lookup a compendium entry to base it on.)"
        )),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn template() -> Value {
        json!({
            "Actor": {
                "types": ["character", "npc"],
                "templates": {
                    "common": { "attributes": { "hp": { "value": 0, "max": 0 } } }
                },
                "character": { "templates": ["common"], "details": { "level": 1 } },
                "npc": { "templates": ["common"], "cr": 0 }
            },
            "Item": {
                "types": ["weapon", "spell"],
                "weapon": { "damage": "" }
            }
        })
    }

    #[test]
    fn flatten_merges_templates_then_own_fields() {
        let t = template();
        let npc = flatten_type(&t["Actor"], "npc");
        assert_eq!(npc["attributes"]["hp"]["max"], 0);
        assert_eq!(npc["cr"], 0);
        assert!(npc.get("templates").is_none());
        let pc = flatten_type(&t["Actor"], "character");
        assert_eq!(pc["details"]["level"], 1);
        assert_eq!(pc["attributes"]["hp"]["value"], 0);
    }

    #[test]
    fn info_lists_types_from_template_on_empty_world() {
        let world = json!({});
        let status = json!({ "system": "dnd5e", "version": "3.3.1" });
        let out = system_info(&world, Some(&status), Some(&template()), "Actor", None);
        assert!(out.contains("Game system: dnd5e v3.3.1"));
        assert!(out.contains("Actor types: character, npc"));
        assert!(out.contains("Item types: weapon, spell"));
    }

    #[test]
    fn info_returns_flattened_schema_for_type() {
        let world = json!({});
        let out = system_info(&world, None, Some(&template()), "Actor", Some("npc"));
        assert!(out.contains("Default `system` schema for Actor.npc"));
        assert!(out.contains("\"cr\""));
        assert!(out.contains("\"hp\""));
    }

    #[test]
    fn datamodel_system_reports_no_static_schema() {
        // template.json present but the type carries no fields (DataModel system).
        let tmpl = json!({ "Actor": { "types": ["adversary"], "adversary": {} } });
        let out = system_info(&json!({}), None, Some(&tmpl), "Actor", Some("adversary"));
        assert!(out.contains("No static schema"));
        assert!(out.contains("foundry_get_actor"));
    }

    #[test]
    fn prefers_snapshot_model_when_present() {
        let world = json!({
            "documentTypes": { "Actor": ["hero"], "Item": [] },
            "model": { "Actor": { "hero": { "grit": { "value": 5 } } } }
        });
        let out = system_info(&world, None, None, "Actor", Some("hero"));
        assert!(out.contains("Actor types: hero"));
        assert!(out.contains("\"grit\""));
        assert!(out.contains("\"value\": 5"));
    }
}
