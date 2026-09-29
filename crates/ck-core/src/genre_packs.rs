//! Genre packs: a starting vocabulary for a world — extra kinds and infobox
//! fields, page-template headings, suggested tags and an optional calendar.
//! Built-ins ship in the binary under stable `builtin:<name>` ids; users can
//! drop their own `*.json` files into `<app data>/genre-packs/` (ids become
//! `user:<slug>`). Applying is additive and never overwrites a user's edits:
//! existing fields, edited templates and a set calendar are left alone.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::vault;
use crate::world_config::{self, CalendarConfig, KindField, KindOverride};

pub const BUILTIN_PREFIX: &str = "builtin:";
const USER_PREFIX: &str = "user:";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackKind {
    /// `name` or `name:type` (text, list, number, checkbox, date, datetime).
    #[serde(default)]
    pub fields: Vec<String>,
    /// `## ` headings of the kind's starter template.
    #[serde(default)]
    pub headings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GenrePack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub kinds: BTreeMap<String, PackKind>,
    /// Suggestions shown in the picker; tags stay derived from pages.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<CalendarConfig>,
}

// ── Built-ins ─────────────────────────────────────────────────────

type KindSpec = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

struct Builtin {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    kinds: &'static [KindSpec],
    tags: &'static [&'static str],
    calendar: Option<(&'static [&'static str], &'static [&'static str])>,
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "builtin:fantasy",
        name: "Fantasy",
        description: "Gods, creatures and ancient ruins; a made-up calendar.",
        kinds: &[
            (
                "npc",
                &["occupation"],
                &[
                    "Appearance",
                    "Motivation",
                    "Secrets",
                    "Relationships",
                    "History",
                ],
            ),
            (
                "creature",
                &["habitat", "danger", "diet"],
                &["Appearance", "Behaviour", "Lair", "Lore"],
            ),
            (
                "deity",
                &["domain", "symbol", "alignment", "worshippers:list"],
                &["Portfolio", "Worship", "Myths"],
            ),
        ],
        tags: &["magic", "ancient", "ruin", "guild", "prophecy"],
        calendar: Some((
            &[
                "Frostmoot",
                "Thawrise",
                "Seedfall",
                "Greenwake",
                "Highsun",
                "Emberturn",
                "Harvestide",
                "Leaffall",
                "Mistwane",
                "Coldset",
                "Deepnight",
                "Yearsend",
            ],
            &["Age of Embers", "Age of Crowns"],
        )),
    },
    Builtin {
        id: "builtin:scifi",
        name: "Science fiction",
        description: "Ships, technology and species; before and after the jump.",
        kinds: &[
            (
                "ship",
                &["class", "operator", "crew:number", "status", "location"],
                &["Overview", "Systems", "Crew", "History"],
            ),
            (
                "technology",
                &["type", "origin", "tech_level:number", "status"],
                &["Function", "Limits", "Implications"],
            ),
            (
                "species",
                &["homeworld", "lifespan", "traits:list"],
                &["Biology", "Culture", "First contact"],
            ),
            ("npc", &["species", "allegiance"], &[]),
            ("place", &["system", "orbit"], &[]),
        ],
        tags: &["ftl", "corporate", "ai", "frontier", "derelict"],
        calendar: Some((&[], &["Pre-Jump", "Post-Jump"])),
    },
    Builtin {
        id: "builtin:horror",
        name: "Horror & mystery",
        description: "Clues, monsters and dread; who knows what, and at what cost.",
        kinds: &[
            (
                "clue",
                &["found_at", "points_to", "status"],
                &["What it shows", "Where it is found", "What it costs"],
            ),
            (
                "monster",
                &["threat", "weakness", "lair"],
                &["Signs", "Behaviour", "Weakness"],
            ),
            ("npc", &["secret", "fear"], &[]),
            ("place", &["dread"], &[]),
        ],
        tags: &["omen", "cult", "haunting", "mystery", "sanity"],
        calendar: None,
    },
    Builtin {
        id: "builtin:historical",
        name: "Historical",
        description: "Sourced people, places and documents on the real calendar.",
        kinds: &[
            ("npc", &["born:date", "died:date", "occupation"], &[]),
            ("faction", &["founded:date"], &[]),
            ("place", &["founded:date"], &[]),
            (
                "event",
                &[],
                &["What happened", "Causes", "Consequences", "Sources"],
            ),
            (
                "document",
                &["author", "date:date", "archive"],
                &["Summary", "Context", "Excerpts"],
            ),
        ],
        tags: &["treaty", "campaign", "dynasty", "trade", "sourced"],
        calendar: Some((
            &[
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
            &["BC", "AD"],
        )),
    },
];

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

pub fn builtin_packs() -> Vec<GenrePack> {
    BUILTINS
        .iter()
        .map(|b| GenrePack {
            id: b.id.into(),
            name: b.name.into(),
            description: b.description.into(),
            kinds: b
                .kinds
                .iter()
                .map(|(kind, fields, headings)| {
                    (
                        kind.to_string(),
                        PackKind {
                            fields: strings(fields),
                            headings: strings(headings),
                        },
                    )
                })
                .collect(),
            tags: strings(b.tags),
            calendar: b.calendar.map(|(months, eras)| CalendarConfig {
                months: strings(months),
                eras: strings(eras),
            }),
        })
        .collect()
}

// ── User packs (lenient) ──────────────────────────────────────────

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn clean_strings(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && !s.contains('\n'))
                .collect()
        })
        .unwrap_or_default()
}

fn valid_field_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Parse one user pack, dropping whatever is unusable instead of failing:
/// a bad kind, field or heading disappears, the rest survives. `None` only
/// when the pack itself is unusable (not an object, no id, or an id that
/// tries to claim the built-in prefix).
pub fn validate_user_pack(v: &serde_json::Value) -> Option<GenrePack> {
    let obj = v.as_object()?;
    let raw_id = obj.get("id")?.as_str()?.trim().to_lowercase();
    if raw_id.starts_with(BUILTIN_PREFIX) {
        return None;
    }
    let id_slug = slug(raw_id.strip_prefix(USER_PREFIX).unwrap_or(&raw_id));
    if id_slug.is_empty() {
        return None;
    }
    let name = obj
        .get("name")
        .and_then(|n| n.as_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(&id_slug)
        .to_string();

    let mut kinds = BTreeMap::new();
    if let Some(map) = obj.get("kinds").and_then(|k| k.as_object()) {
        for (kind, body) in map {
            let kind = kind.trim().to_lowercase();
            if !valid_field_name(&kind) {
                continue;
            }
            let fields: Vec<String> = clean_strings(body.get("fields"))
                .into_iter()
                .filter_map(|spec| world_config::parse_field(&spec))
                .filter(|f| valid_field_name(&f.name))
                .map(|f| f.spec())
                .collect();
            let headings = clean_strings(body.get("headings"));
            if !fields.is_empty() || !headings.is_empty() {
                kinds.insert(kind, PackKind { fields, headings });
            }
        }
    }

    let mut seen = HashSet::new();
    let tags = clean_strings(obj.get("tags"))
        .into_iter()
        .map(|t| slug(&t))
        .filter(|t| !t.is_empty() && seen.insert(t.clone()))
        .collect();

    let calendar = obj
        .get("calendar")
        .map(|c| CalendarConfig {
            months: clean_strings(c.get("months")),
            eras: clean_strings(c.get("eras")),
        })
        .filter(|c| !c.months.is_empty() || !c.eras.is_empty());

    Some(GenrePack {
        id: format!("{USER_PREFIX}{id_slug}"),
        name,
        description: obj
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        kinds,
        tags,
        calendar,
    })
}

/// Every `*.json` in `dir`, validated, first id wins. A missing dir is fine.
pub fn load_user_packs(dir: &Path) -> Vec<GenrePack> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    files.sort();
    let mut seen = HashSet::new();
    files
        .iter()
        .filter_map(|p| {
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(p).ok()?).ok()
        })
        .filter_map(|v| validate_user_pack(&v))
        .filter(|p| seen.insert(p.id.clone()))
        .collect()
}

pub fn user_packs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("genre-packs")
}

pub fn all_packs(data_dir: &Path) -> Vec<GenrePack> {
    let mut packs = builtin_packs();
    packs.extend(load_user_packs(&user_packs_dir(data_dir)));
    packs
}

pub fn find_pack(data_dir: &Path, id: &str) -> Option<GenrePack> {
    all_packs(data_dir).into_iter().find(|p| p.id == id)
}

// ── Apply ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, PartialEq)]
pub struct KindChange {
    pub kind: String,
    /// True when the pack introduced the kind.
    pub created: bool,
    pub fields_added: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TemplateChange {
    pub name: String,
    /// created | upgraded | unchanged | kept (the user edited it)
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ApplyReport {
    pub pack: String,
    pub dry_run: bool,
    pub kinds: Vec<KindChange>,
    pub templates: Vec<TemplateChange>,
    /// set | unchanged | kept (the world already has one) | none
    pub calendar: &'static str,
    pub tags: Vec<String>,
}

fn template_for(kind: &str, fields: &[KindField], headings: &[String]) -> String {
    let mut out = vault::template_base(kind, fields);
    for h in headings {
        out.push_str(&format!("## {h}\n\n"));
    }
    out
}

/// Merge `pack` into the world at `world_root`. With `dry_run` nothing is
/// written and the report says what would happen.
pub fn apply(world_root: &Path, pack: &GenrePack, dry_run: bool) -> AppResult<ApplyReport> {
    let mut cfg = world_config::read(world_root)?
        .ok_or_else(|| AppError::NotFound("World config not found".into()))?;
    let before = cfg.clone();
    if !dry_run {
        vault::write_default_templates(world_root)?;
    }
    let schemas = cfg.kind_schemas();
    let dir = vault::templates_dir(world_root);

    let mut kinds = Vec::new();
    let mut templates = Vec::new();
    for (kind, pk) in &pack.kinds {
        let known = schemas.iter().any(|(k, _)| k == kind);
        let current: Vec<KindField> = schemas
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, f)| f.clone())
            .unwrap_or_default();
        let mut merged = current.clone();
        let mut added = Vec::new();
        for spec in &pk.fields {
            if let Some(f) = world_config::parse_field(spec) {
                if !merged.iter().any(|m| m.name.eq_ignore_ascii_case(&f.name)) {
                    added.push(f.name.clone());
                    merged.push(f);
                }
            }
        }
        if !added.is_empty() || !known {
            cfg.kinds.insert(
                kind.clone(),
                KindOverride {
                    fields: merged.iter().map(KindField::spec).collect(),
                },
            );
            kinds.push(KindChange {
                kind: kind.clone(),
                created: !known,
                fields_added: added,
            });
        }

        let headings: Vec<String> = if pk.headings.is_empty() {
            vault::default_headings(kind)
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            pk.headings.clone()
        };
        let generated = template_for(kind, &merged, &headings);
        let path = dir.join(format!("{kind}.md"));
        let status = match std::fs::read_to_string(&path) {
            Err(_) => "created",
            Ok(c) if c == generated => "unchanged",
            Ok(c)
                if c == vault::template_content(kind, &current)
                    || c == vault::template_base(kind, &current) =>
            {
                "upgraded"
            }
            Ok(_) => "kept",
        };
        if !dry_run && matches!(status, "created" | "upgraded") {
            std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::write(&path, &generated))
                .map_err(|e| AppError::Internal(anyhow::anyhow!("write template {kind}: {e}")))?;
        }
        templates.push(TemplateChange {
            name: kind.clone(),
            status,
        });
    }

    let calendar = match &pack.calendar {
        None => "none",
        Some(c) if *c == cfg.calendar => "unchanged",
        Some(_) if !cfg.calendar.months.is_empty() || !cfg.calendar.eras.is_empty() => "kept",
        Some(c) => {
            cfg.calendar = c.clone();
            "set"
        }
    };

    if !dry_run {
        let mut applied: Vec<toml::Value> = cfg
            .extra
            .get("genre_packs")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        if !applied.iter().any(|v| v.as_str() == Some(pack.id.as_str())) {
            applied.push(toml::Value::String(pack.id.clone()));
        }
        cfg.extra
            .insert("genre_packs".into(), toml::Value::Array(applied));
        if cfg != before {
            world_config::write(world_root, &cfg)?;
        }
    }

    Ok(ApplyReport {
        pack: pack.id.clone(),
        dry_run,
        kinds,
        templates,
        calendar,
        tags: pack.tags.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_config::WorldConfig;
    use serde_json::json;

    fn temp_world(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-genre-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = WorldConfig {
            id: "w".into(),
            name: "W".into(),
            ..Default::default()
        };
        world_config::write(&dir, &cfg).unwrap();
        vault::write_default_templates(&dir).unwrap();
        dir
    }

    fn pack(id: &str) -> GenrePack {
        builtin_packs().into_iter().find(|p| p.id == id).unwrap()
    }

    fn fields_of(dir: &Path, kind: &str) -> Vec<String> {
        world_config::read(dir)
            .unwrap()
            .unwrap()
            .kind_schemas()
            .into_iter()
            .find(|(k, _)| k == kind)
            .map(|(_, f)| f.iter().map(KindField::spec).collect())
            .unwrap_or_default()
    }

    #[test]
    fn builtins_are_well_formed() {
        let packs = builtin_packs();
        assert_eq!(packs.len(), 4);
        let mut ids = HashSet::new();
        for p in &packs {
            assert!(p.id.starts_with(BUILTIN_PREFIX) && ids.insert(p.id.clone()));
            for (kind, pk) in &p.kinds {
                assert!(valid_field_name(kind));
                for spec in &pk.fields {
                    let f = world_config::parse_field(spec).unwrap();
                    assert!(valid_field_name(&f.name), "{spec}");
                    assert_eq!(&f.spec(), spec, "spec must be canonical");
                }
            }
        }
    }

    #[test]
    fn apply_adds_fields_kinds_templates_calendar_and_is_idempotent() {
        let dir = temp_world("apply");
        let p = pack("builtin:scifi");
        let r = apply(&dir, &p, false).unwrap();
        assert_eq!(r.calendar, "set");
        assert!(r.kinds.iter().any(|k| k.kind == "ship" && k.created));
        assert!(r
            .kinds
            .iter()
            .any(|k| k.kind == "npc" && k.fields_added == ["species", "allegiance"]));
        assert_eq!(
            fields_of(&dir, "npc"),
            [
                "race",
                "affiliation:list",
                "status",
                "location",
                "species",
                "allegiance"
            ]
        );
        assert_eq!(fields_of(&dir, "ship")[2], "crew:number");
        let ship = std::fs::read_to_string(vault::templates_dir(&dir).join("ship.md")).unwrap();
        assert!(ship.contains("crew:\n"));
        assert!(ship.contains("## Systems"));
        let cfg = world_config::read(&dir).unwrap().unwrap();
        assert_eq!(cfg.calendar.eras, ["Pre-Jump", "Post-Jump"]);

        let again = apply(&dir, &p, false).unwrap();
        assert!(again.kinds.is_empty());
        assert!(again.templates.iter().all(|t| t.status == "unchanged"));
        assert_eq!(again.calendar, "unchanged");
        assert_eq!(fields_of(&dir, "npc").len(), 6);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edited_template_calendar_and_fields_survive() {
        let dir = temp_world("keep");
        let npc = vault::templates_dir(&dir).join("npc.md");
        std::fs::write(&npc, "---\nkind: npc\n---\n\nMY OWN TEMPLATE {{title}}\n").unwrap();
        let mut cfg = world_config::read(&dir).unwrap().unwrap();
        cfg.calendar = CalendarConfig {
            months: vec!["Mine".into()],
            eras: vec![],
        };
        cfg.kinds.insert(
            "npc".into(),
            KindOverride {
                fields: vec!["race".into(), "occupation:list".into()],
            },
        );
        world_config::write(&dir, &cfg).unwrap();

        let r = apply(&dir, &pack("builtin:fantasy"), false).unwrap();
        assert_eq!(r.calendar, "kept");
        assert!(r
            .templates
            .iter()
            .any(|t| t.name == "npc" && t.status == "kept"));
        assert!(std::fs::read_to_string(&npc)
            .unwrap()
            .contains("MY OWN TEMPLATE"));
        // the user's own `occupation:list` wins over the pack's text field
        assert_eq!(fields_of(&dir, "npc"), ["race", "occupation:list"]);
        assert_eq!(
            world_config::read(&dir).unwrap().unwrap().calendar.months,
            ["Mine"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untouched_default_template_is_upgraded_and_dry_run_writes_nothing() {
        let dir = temp_world("upgrade");
        let before = std::fs::read_to_string(dir.join(".ck/config.toml")).unwrap();
        let r = apply(&dir, &pack("builtin:horror"), true).unwrap();
        assert!(r.dry_run);
        assert!(r
            .templates
            .iter()
            .any(|t| t.name == "npc" && t.status == "upgraded"));
        assert!(r
            .templates
            .iter()
            .any(|t| t.name == "clue" && t.status == "created"));
        assert_eq!(
            std::fs::read_to_string(dir.join(".ck/config.toml")).unwrap(),
            before
        );
        assert!(!vault::templates_dir(&dir).join("clue.md").exists());

        apply(&dir, &pack("builtin:horror"), false).unwrap();
        let npc = std::fs::read_to_string(vault::templates_dir(&dir).join("npc.md")).unwrap();
        assert!(npc.contains("secret:") && npc.contains("## Motivation"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn user_packs_are_validated_leniently() {
        assert!(validate_user_pack(&json!("nope")).is_none());
        assert!(validate_user_pack(&json!({ "name": "no id" })).is_none());
        assert!(validate_user_pack(&json!({ "id": "builtin:fantasy" })).is_none());
        assert!(validate_user_pack(&json!({ "id": "  !!!  " })).is_none());

        let p = validate_user_pack(&json!({
            "id": "My Pack!",
            "kinds": {
                "Ship": { "fields": ["hull:number", "", "bad key", "crew:list"], "headings": ["Deck", "", "two\nlines"] },
                "bad kind": { "fields": ["x"] },
                "empty": { "fields": [], "headings": [] },
                "junk": 7
            },
            "tags": ["Cult", "cult", "", "Old Gods"],
            "calendar": { "months": ["A", ""], "eras": [] }
        }))
        .unwrap();
        assert_eq!(p.id, "user:my-pack");
        assert_eq!(p.name, "my-pack");
        assert_eq!(p.kinds.keys().collect::<Vec<_>>(), ["ship"]);
        assert_eq!(p.kinds["ship"].fields, ["hull:number", "crew:list"]);
        assert_eq!(p.kinds["ship"].headings, ["Deck"]);
        assert_eq!(p.tags, ["cult", "old-gods"]);
        assert_eq!(p.calendar.unwrap().months, ["A"]);
    }

    #[test]
    fn user_pack_files_dedupe_and_skip_garbage() {
        let dir = std::env::temp_dir().join(format!("ck-genre-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.json"),
            r#"{"id":"dup","name":"First","kinds":{"x":{"fields":["f"]}}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("b.json"),
            r#"{"id":"user:dup","name":"Second","kinds":{"y":{"fields":["g"]}}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("c.json"), "{ not json").unwrap();
        std::fs::write(dir.join("d.json"), r#"{"id":"builtin:x"}"#).unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let packs = load_user_packs(&dir);
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].name, "First");
        assert!(load_user_packs(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
