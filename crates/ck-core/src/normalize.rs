use serde_json::{json, Map, Value};

pub const METADATA_CATEGORIES: [&str; 5] = ["characters", "locations", "events", "items", "tags"];

/// Normalize arbitrary player input (list of objects, list of strings, or a
/// comma string) into `[{player_name, character_name}]`. Mirrors the Python
/// `_normalize_players`.
pub fn normalize_players(value: &Value) -> Value {
    let raw: Vec<Value> = match value {
        Value::Array(items) => items.clone(),
        Value::String(s) => s.split(',').map(|p| json!(p.trim())).collect(),
        _ => return json!([]),
    };
    let mut out = Vec::new();
    for item in raw {
        let (player, character) = match &item {
            Value::Object(o) => (
                o.get("player_name").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                o.get("character_name").and_then(Value::as_str).unwrap_or("").trim().to_string(),
            ),
            Value::String(s) => (s.trim().to_string(), String::new()),
            _ => continue,
        };
        if player.is_empty() && character.is_empty() {
            continue;
        }
        out.push(json!({ "player_name": player, "character_name": character }));
    }
    Value::Array(out)
}

/// Normalize metadata into `{category: [str, ...]}` for all five categories.
/// Accepts an object, a legacy plain list (treated as tags), or null. Mirrors
/// the Python `_normalize_metadata`.
pub fn normalize_metadata(value: &Value) -> Value {
    let mut out = Map::new();
    let to_list = |v: &Value| -> Vec<Value> {
        match v {
            Value::Array(items) => items
                .iter()
                .filter_map(|x| x.as_str().map(str::trim))
                .filter(|s| !s.is_empty())
                .map(|s| json!(s))
                .collect(),
            Value::String(s) => s
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| json!(s))
                .collect(),
            _ => Vec::new(),
        }
    };

    match value {
        Value::Object(obj) => {
            for cat in METADATA_CATEGORIES {
                out.insert(cat.to_string(), Value::Array(obj.get(cat).map(to_list).unwrap_or_default()));
            }
        }
        Value::Array(_) => {
            for cat in METADATA_CATEGORIES {
                out.insert(cat.to_string(), json!([]));
            }
            out.insert("tags".to_string(), Value::Array(to_list(value)));
        }
        _ => {
            for cat in METADATA_CATEGORIES {
                out.insert(cat.to_string(), json!([]));
            }
        }
    }
    Value::Object(out)
}

pub fn sanitize_folder_name(name: &str) -> String {
    name.replace(['/', '\\', ':', ' '], "_")
}
