//! `.ck/config.toml` — world identity + settings. TRUTH for world discovery:
//! a folder is a world iff this file exists. Unknown keys/tables are preserved
//! across read-modify-write (future infobox schemas live here too).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerEntry {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub player_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub character_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pronouns: String,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldConfig {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub system: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub setting: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gm: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gm_pronouns: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_language: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub extra_info: String,
    #[serde(default = "default_start", skip_serializing_if = "is_default_start")]
    pub start_session_number: i64,
    /// Where Codex pages live, relative to the world root. `""`/`"Codex"` =
    /// canonical layout; `"."` = pages anywhere (adopted foreign vault).
    #[serde(default, skip_serializing_if = "is_default_codex_root")]
    pub codex_root: String,
    #[serde(default, rename = "player", skip_serializing_if = "Vec::is_empty")]
    pub players: Vec<PlayerEntry>,
    /// Per-kind infobox field overrides (`[kinds.npc] fields = ["race", "affiliation:list"]`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub kinds: BTreeMap<String, KindOverride>,
    /// Custom fantasy calendar for the timeline (`[calendar]`).
    #[serde(default, skip_serializing_if = "is_default_calendar")]
    pub calendar: CalendarConfig,
    #[serde(flatten)]
    pub extra: toml::Table,
}

/// `[calendar]`: month names map `date:` month numbers for display; eras are
/// ordered suffixes (`1374-08-12 DR`), earliest first.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub months: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eras: Vec<String>,
}

fn is_default_calendar(c: &CalendarConfig) -> bool {
    c.months.is_empty() && c.eras.is_empty()
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct KindOverride {
    #[serde(default)]
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindField {
    pub name: String,
    #[serde(rename = "type")]
    pub ftype: String,
}

const FIELD_TYPES: [&str; 6] = ["text", "list", "number", "checkbox", "date", "datetime"];

/// Built-in infobox schemas (page-data-model-spec.md). `name:type` — type
/// defaults to `text`.
const DEFAULT_KINDS: &[(&str, &[&str])] = &[
    ("pc", &["player", "race", "class", "affiliation:list"]),
    ("npc", &["race", "affiliation:list", "status", "location"]),
    (
        "place",
        &["region", "type", "population", "ruler", "part_of"],
    ),
    ("faction", &["type", "leader", "headquarters", "alignment"]),
    ("item", &["type", "owner", "location", "magical:checkbox"]),
    ("event", &["date:date", "location", "participants:list"]),
    ("lore", &[]),
];

fn parse_field(spec: &str) -> Option<KindField> {
    let (name, ftype) = match spec.split_once(':') {
        Some((n, t)) => (n.trim(), t.trim()),
        None => (spec.trim(), "text"),
    };
    if name.is_empty() {
        return None;
    }
    let ftype = if FIELD_TYPES.contains(&ftype) {
        ftype
    } else {
        "text"
    };
    Some(KindField {
        name: name.to_string(),
        ftype: ftype.to_string(),
    })
}

impl WorldConfig {
    /// Built-in schemas merged with this world's `[kinds.*]` overrides;
    /// custom kinds from the config are appended after the built-ins.
    pub fn kind_schemas(&self) -> Vec<(String, Vec<KindField>)> {
        let parse = |specs: &[String]| specs.iter().filter_map(|s| parse_field(s)).collect();
        let mut out: Vec<(String, Vec<KindField>)> = DEFAULT_KINDS
            .iter()
            .map(|(kind, fields)| {
                let fields = match self.kinds.get(*kind) {
                    Some(o) => parse(&o.fields),
                    None => fields.iter().filter_map(|s| parse_field(s)).collect(),
                };
                (kind.to_string(), fields)
            })
            .collect();
        for (kind, o) in &self.kinds {
            if !DEFAULT_KINDS.iter().any(|(k, _)| k == kind) {
                out.push((kind.clone(), parse(&o.fields)));
            }
        }
        out
    }
}

fn default_start() -> i64 {
    1
}

fn is_default_start(n: &i64) -> bool {
    *n == 1
}

fn is_default_codex_root(s: &str) -> bool {
    s.is_empty() || s == "Codex"
}

impl WorldConfig {
    /// Absolute Codex folder for this world.
    pub fn codex_dir(&self, world_root: &Path) -> PathBuf {
        match self.codex_root.trim() {
            "" | "Codex" => world_root.join("Codex"),
            "." => world_root.to_path_buf(),
            other => world_root.join(other),
        }
    }
}

pub fn config_path(world_root: &Path) -> PathBuf {
    world_root.join(".ck").join("config.toml")
}

/// `Ok(None)` when the marker file doesn't exist (folder is not a world).
pub fn read(world_root: &Path) -> AppResult<Option<WorldConfig>> {
    let path = config_path(world_root);
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(AppError::Internal(anyhow::anyhow!(
                "read {}: {e}",
                path.display()
            )))
        }
    };
    let cfg = toml::from_str(&raw)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("parse {}: {e}", path.display())))?;
    Ok(Some(cfg))
}

pub fn write(world_root: &Path, cfg: &WorldConfig) -> AppResult<()> {
    let path = config_path(world_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create .ck: {e}")))?;
    }
    let body = toml::to_string_pretty(cfg)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize config.toml: {e}")))?;
    std::fs::write(&path, body)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write {}: {e}", path.display())))
}

// ── Recap (.ck/recap.md) ──────────────────────────────────────────

pub fn recap_path(world_root: &Path) -> PathBuf {
    world_root.join(".ck").join("recap.md")
}

/// (recap body, updated_at) — both empty when no recap exists.
pub fn read_recap(world_root: &Path) -> (String, String) {
    let Ok(raw) = std::fs::read_to_string(recap_path(world_root)) else {
        return (String::new(), String::new());
    };
    let (fm, body) = crate::vault::split_frontmatter(&raw);
    let updated = crate::vault::fm_get(&fm, "updated_at")
        .unwrap_or("")
        .to_string();
    (body.trim().to_string(), updated)
}

pub fn write_recap(world_root: &Path, recap: &str, updated_at: &str) -> AppResult<()> {
    let path = recap_path(world_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create .ck: {e}")))?;
    }
    let body = format!("---\nupdated_at: {updated_at}\n---\n\n{}\n", recap.trim());
    std::fs::write(&path, body)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write recap.md: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-wc-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip_full() {
        let root = tmp_root("full");
        let cfg = WorldConfig {
            id: "w-1".into(),
            name: "Ashfall".into(),
            system: "D&D 5e".into(),
            setting: "Sword Coast".into(),
            gm: "Aron".into(),
            gm_pronouns: "he/him".into(),
            default_language: "de".into(),
            extra_info: "line one\nline \"two\"\n".into(),
            start_session_number: 5,
            players: vec![PlayerEntry {
                player_name: "Aron".into(),
                character_name: "Lyra".into(),
                pronouns: "she/her".into(),
            }],
            codex_root: String::new(),
            kinds: BTreeMap::new(),
            calendar: CalendarConfig::default(),
            extra: toml::Table::new(),
        };
        write(&root, &cfg).unwrap();
        let back = read(&root).unwrap().unwrap();
        assert_eq!(back, cfg);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_is_none_and_minimal_parses() {
        let root = tmp_root("min");
        assert!(read(&root).unwrap().is_none());
        std::fs::create_dir_all(root.join(".ck")).unwrap();
        std::fs::write(config_path(&root), "id = \"x\"\nname = \"Y\"\n").unwrap();
        let cfg = read(&root).unwrap().unwrap();
        assert_eq!(cfg.id, "x");
        assert_eq!(cfg.start_session_number, 1);
        assert!(cfg.players.is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recap_roundtrip() {
        let root = tmp_root("recap");
        assert_eq!(read_recap(&root), (String::new(), String::new()));
        write_recap(&root, "The story so far…", "2026-06-03T10:00:00Z").unwrap();
        let (body, at) = read_recap(&root);
        assert_eq!(body, "The story so far…");
        assert_eq!(at, "2026-06-03T10:00:00Z");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unknown_tables_survive_rewrite() {
        let root = tmp_root("extra");
        std::fs::create_dir_all(root.join(".ck")).unwrap();
        std::fs::write(
            config_path(&root),
            "id = \"x\"\nname = \"Y\"\ncustom_key = \"kept\"\n\n[kinds.npc]\nfields = [\"race\"]\n",
        )
        .unwrap();
        let mut cfg = read(&root).unwrap().unwrap();
        cfg.name = "Z".into();
        write(&root, &cfg).unwrap();
        let back = read(&root).unwrap().unwrap();
        assert_eq!(back.name, "Z");
        assert_eq!(
            back.extra.get("custom_key").and_then(|v| v.as_str()),
            Some("kept")
        );
        assert_eq!(
            back.kinds.get("npc").map(|o| o.fields.as_slice()),
            Some(["race".to_string()].as_slice())
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn kind_schemas_defaults_overrides_and_custom() {
        let mut cfg = WorldConfig::default();
        let schemas = cfg.kind_schemas();
        let npc = &schemas.iter().find(|(k, _)| k == "npc").unwrap().1;
        assert_eq!(
            npc[0],
            KindField {
                name: "race".into(),
                ftype: "text".into()
            }
        );
        assert_eq!(
            npc[1],
            KindField {
                name: "affiliation".into(),
                ftype: "list".into()
            }
        );
        let item = &schemas.iter().find(|(k, _)| k == "item").unwrap().1;
        assert_eq!(item[3].ftype, "checkbox");
        assert!(schemas
            .iter()
            .find(|(k, _)| k == "lore")
            .unwrap()
            .1
            .is_empty());

        cfg.kinds.insert(
            "npc".into(),
            KindOverride {
                fields: vec!["age:number".into(), "bogus:wat".into()],
            },
        );
        cfg.kinds.insert(
            "deity".into(),
            KindOverride {
                fields: vec!["domain".into()],
            },
        );
        let schemas = cfg.kind_schemas();
        let npc = &schemas.iter().find(|(k, _)| k == "npc").unwrap().1;
        assert_eq!(npc.len(), 2);
        assert_eq!(npc[0].ftype, "number");
        assert_eq!(npc[1].ftype, "text"); // unknown type → text
        assert!(schemas
            .iter()
            .any(|(k, f)| k == "deity" && f[0].name == "domain"));
    }
}
