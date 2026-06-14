//! Campaigns (worlds) — files are truth. A world is any folder holding
//! `.ck/config.toml`; the list is discovered by scanning the data root (plus
//! the `world_dirs` registry for worlds living elsewhere). The global DB keeps
//! only app settings; the `campaigns` table is an inert FK shim until the
//! one-app merge drops it.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::config::get_value;
use crate::error::{AppError, AppResult};
use crate::models::{CampaignDetail, CampaignInfo, CampaignUpdateRequest};
use crate::normalize::normalize_players;
use crate::store::now;
use crate::world_config::{self, PlayerEntry, WorldConfig};

pub(crate) fn default_language(conn: &Connection) -> String {
    get_value(conn, "default_language")
        .ok()
        .flatten()
        .unwrap_or_else(|| "en".to_string())
}

// ── Discovery ─────────────────────────────────────────────────────

const WORLD_DIRS_KEY: &str = "world_dirs";

/// Extra world roots living outside the data root (user-picked locations).
fn extra_world_dirs(conn: &Connection) -> Vec<PathBuf> {
    get_value(conn, WORLD_DIRS_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

/// Register a world root outside the data root so discovery finds it.
pub fn register_world_dir(conn: &Connection, root: &Path) -> AppResult<()> {
    let data_root = crate::store::sessions::output_root(conn)?;
    if root.parent() == Some(&data_root) {
        return Ok(()); // inside the data root — the scan finds it anyway
    }
    let mut dirs: Vec<String> = extra_world_dirs(conn)
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let s = root.to_string_lossy().to_string();
    if !dirs.contains(&s) {
        dirs.push(s);
        crate::config::set_value(conn, WORLD_DIRS_KEY, &serde_json::to_string(&dirs).unwrap())?;
    }
    Ok(())
}

fn unregister_world_dir(conn: &Connection, root: &Path) -> AppResult<()> {
    let dirs: Vec<String> = extra_world_dirs(conn)
        .iter()
        .filter(|p| p.as_path() != root)
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    crate::config::set_value(conn, WORLD_DIRS_KEY, &serde_json::to_string(&dirs).unwrap())?;
    Ok(())
}

/// Every world: scan the data root one level deep for `.ck/config.toml`,
/// plus the registered extra roots. Duplicate ids: first wins, warn.
pub(crate) fn worlds(conn: &Connection) -> AppResult<Vec<(PathBuf, WorldConfig)>> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let data_root = crate::store::sessions::output_root(conn)?;
    if let Ok(entries) = std::fs::read_dir(&data_root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
                roots.push(p);
            }
        }
    }
    roots.extend(extra_world_dirs(conn));

    let mut out: Vec<(PathBuf, WorldConfig)> = Vec::new();
    for root in roots {
        match world_config::read(&root) {
            Ok(Some(cfg)) => {
                if out.iter().any(|(_, c)| c.id == cfg.id) {
                    tracing::warn!(
                        "duplicate world id {} at {} — ignored",
                        cfg.id,
                        root.display()
                    );
                } else {
                    out.push((root, cfg));
                }
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("unreadable world config at {}: {e}", root.display()),
        }
    }
    out.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()));
    Ok(out)
}

pub fn world_root_for_id(conn: &Connection, campaign_id: &str) -> AppResult<Option<PathBuf>> {
    Ok(worlds(conn)?
        .into_iter()
        .find(|(_, c)| c.id == campaign_id)
        .map(|(root, _)| root))
}

// ── Config ↔ DTO mapping ──────────────────────────────────────────

fn players_value(players: &[PlayerEntry]) -> Value {
    json!(players
        .iter()
        .map(|p| json!({
            "player_name": p.player_name,
            "character_name": p.character_name,
            "pronouns": p.pronouns,
        }))
        .collect::<Vec<_>>())
}

fn players_entries(v: &Value) -> Vec<PlayerEntry> {
    normalize_players(v)
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|p| PlayerEntry {
                    player_name: p
                        .get("player_name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    character_name: p
                        .get("character_name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    pronouns: p
                        .get("pronouns")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn detail_from(root: &Path, cfg: &WorldConfig, fallback_lang: &str) -> CampaignDetail {
    let (recap, recap_updated_at) = world_config::read_recap(root);
    CampaignDetail {
        campaign_id: cfg.id.clone(),
        name: cfg.name.clone(),
        next_session_number: next_number_for(root, cfg),
        system: cfg.system.clone(),
        gm: cfg.gm.clone(),
        gm_pronouns: cfg.gm_pronouns.clone(),
        setting: cfg.setting.clone(),
        default_language: if cfg.default_language.is_empty() {
            fallback_lang.to_string()
        } else {
            cfg.default_language.clone()
        },
        players: players_value(&cfg.players),
        extra_info: cfg.extra_info.clone(),
        recap,
        recap_updated_at,
        vault_path: Some(cfg.codex_dir(root).to_string_lossy().to_string()),
    }
}

/// `max(Sessions/<NNN>) + 1`, falling back to the configured start number for
/// a world with no sessions yet.
fn next_number_for(root: &Path, cfg: &WorldConfig) -> i64 {
    let mut max: Option<i64> = None;
    if let Ok(entries) = std::fs::read_dir(root.join("Sessions")) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            if let Ok(n) = entry.file_name().to_string_lossy().parse::<i64>() {
                max = Some(max.map_or(n, |m| m.max(n)));
            }
        }
    }
    max.map(|m| m + 1).unwrap_or(cfg.start_session_number)
}

// ── Reads ─────────────────────────────────────────────────────────

pub fn get_campaigns(conn: &Connection) -> AppResult<Vec<CampaignDetail>> {
    let lang = default_language(conn);
    Ok(worlds(conn)?
        .iter()
        .map(|(root, cfg)| detail_from(root, cfg, &lang))
        .collect())
}

pub fn get_campaign(conn: &Connection, campaign_id: &str) -> AppResult<Option<CampaignDetail>> {
    let lang = default_language(conn);
    Ok(worlds(conn)?
        .iter()
        .find(|(_, c)| c.id == campaign_id)
        .map(|(root, cfg)| detail_from(root, cfg, &lang)))
}

pub fn campaign_infos(conn: &Connection) -> AppResult<Vec<CampaignInfo>> {
    Ok(get_campaigns(conn)?
        .into_iter()
        .map(|c| CampaignInfo {
            campaign_id: c.campaign_id,
            name: c.name,
            next_session_number: c.next_session_number,
        })
        .collect())
}

// ── Writes ────────────────────────────────────────────────────────

/// Create a world: provision the canonical folder layout and write the full
/// identity into `.ck/config.toml`. No campaign data lands in the DB (only an
/// inert FK-shim row — see below).
pub fn create_campaign(
    conn: &Connection,
    campaign_id: &str,
    name: &str,
    start_session_number: i64,
    world_path: Option<&str>,
    scaffold: bool,
    adopt: bool,
) -> AppResult<CampaignDetail> {
    if let Some(existing) = get_campaign(conn, campaign_id)? {
        // FK shim row may be missing when the world folder predates this DB.
        conn.execute(
            "INSERT OR IGNORE INTO campaigns (campaign_id, name) VALUES (?1, ?2)",
            params![existing.campaign_id, existing.name],
        )?;
        return Ok(existing);
    }
    let world_root = if let Some(p) = world_path.map(str::trim).filter(|s| !s.is_empty()) {
        PathBuf::from(p)
    } else {
        let root = crate::store::sessions::output_root(conn)?;
        if root.as_os_str().is_empty() {
            return Err(AppError::BadRequest("No data root configured".into()));
        }
        root.join(crate::normalize::sanitize_folder_name(name))
    };
    let cfg = match world_config::read(&world_root)? {
        // Open-existing: the folder already is a world — adopt it as-is.
        Some(cfg) => cfg,
        None => {
            // Open-existing on a foreign vault writes only additive artifacts
            // and points codex_root at the pages where they are.
            let codex_root = if adopt {
                crate::vault::adopt_vault_layout(&world_root)?
            } else {
                crate::vault::provision_vault_layout(&world_root, scaffold)?;
                "Codex".to_string()
            };
            let cfg = WorldConfig {
                id: campaign_id.to_string(),
                name: name.to_string(),
                default_language: default_language(conn),
                start_session_number,
                codex_root,
                ..Default::default()
            };
            world_config::write(&world_root, &cfg)?;
            cfg
        }
    };
    register_world_dir(conn, &world_root)?;
    // FK shim: sessions.campaign_id REFERENCES campaigns. Row is inert (never
    // read); dropped with the table at the one-app merge.
    conn.execute(
        "INSERT OR IGNORE INTO campaigns (campaign_id, name) VALUES (?1, ?2)",
        params![cfg.id, cfg.name],
    )?;
    set_current_campaign_id(conn, &cfg.id)?;
    let lang = default_language(conn);
    Ok(detail_from(&world_root, &cfg, &lang))
}

pub fn update_campaign(
    conn: &Connection,
    campaign_id: &str,
    req: &CampaignUpdateRequest,
) -> AppResult<CampaignDetail> {
    let root = world_root_for_id(conn, campaign_id)?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
    let mut cfg = world_config::read(&root)?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
    if let Some(v) = &req.name {
        cfg.name = v.clone();
    }
    if let Some(v) = &req.system {
        cfg.system = v.clone();
    }
    if let Some(v) = &req.gm {
        cfg.gm = v.clone();
    }
    if let Some(v) = &req.gm_pronouns {
        cfg.gm_pronouns = v.clone();
    }
    if let Some(v) = &req.setting {
        cfg.setting = v.clone();
    }
    if let Some(v) = &req.default_language {
        cfg.default_language = v.clone();
    }
    if let Some(v) = &req.extra_info {
        cfg.extra_info = v.clone();
    }
    if let Some(n) = req.next_session_number {
        cfg.start_session_number = n;
    }
    if let Some(players) = &req.players {
        cfg.players = players_entries(players);
    }
    world_config::write(&root, &cfg)?;
    let lang = default_language(conn);
    Ok(detail_from(&root, &cfg, &lang))
}

/// Delete a world: move its folder to the OS trash (recoverable), then clear
/// the inert DB rows + registry entries that referenced it.
pub fn delete_campaign(conn: &Connection, campaign_id: &str) -> AppResult<()> {
    let root = world_root_for_id(conn, campaign_id)?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
    crate::paths::move_to_trash(&root)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("move world to trash: {e}")))?;
    unregister_world_dir(conn, &root)?;
    // Inert-row cleanup (FK shim). sessions/artifacts/codex_entries are
    // cleared globally on open (db.rs migrate). No filesystem touches here —
    // the folder is already in the trash.
    conn.execute(
        "DELETE FROM campaigns WHERE campaign_id = ?1",
        params![campaign_id],
    )?;
    if current_campaign_id(conn)?.as_deref() == Some(campaign_id) {
        set_current_campaign_id(conn, "")?;
    }
    Ok(())
}

/// Store a freshly generated recap (`.ck/recap.md`), stamping `updated_at`.
pub fn set_recap(conn: &Connection, campaign_id: &str, recap: &str) -> AppResult<String> {
    let root = world_root_for_id(conn, campaign_id)?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
    let ts = now();
    world_config::write_recap(&root, recap, &ts)?;
    Ok(ts)
}

/// Next free session number, derived from the `Sessions/` folders (`MAX + 1`);
/// a world with no sessions yet starts at its configured start number.
pub fn next_session_number(conn: &Connection, campaign_id: Option<&str>) -> AppResult<i64> {
    let target = match campaign_id {
        Some(id) if !id.is_empty() => Some(id.to_string()),
        _ => current_campaign_id(conn)?,
    };
    let Some(target) = target else { return Ok(1) };
    let Some((root, cfg)) = worlds(conn)?.into_iter().find(|(_, c)| c.id == target) else {
        return Ok(1);
    };
    Ok(next_number_for(&root, &cfg))
}

pub fn current_campaign_id(conn: &Connection) -> AppResult<Option<String>> {
    use rusqlite::OptionalExtension;
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = 'current_campaign_id'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(v.filter(|s| !s.is_empty()))
}

pub fn set_current_campaign_id(conn: &Connection, campaign_id: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO config (key, value) VALUES ('current_campaign_id', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![campaign_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn(tag: &str) -> (Connection, PathBuf) {
        let conn = crate::db::open_in_memory().unwrap();
        let tmp = std::env::temp_dir().join(format!("ck-camp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        crate::config::set_value(&conn, "output_root", &tmp.to_string_lossy()).unwrap();
        (conn, tmp)
    }

    #[test]
    fn create_discover_update_roundtrip() {
        let (conn, tmp) = test_conn("crud");
        let c = create_campaign(&conn, "w1", "My World", 3, None, false, false).unwrap();
        assert_eq!(c.campaign_id, "w1");
        assert_eq!(c.next_session_number, 3);
        let vault = PathBuf::from(c.vault_path.unwrap());
        assert!(vault.ends_with("Codex") && vault.is_dir());
        assert!(vault.parent().unwrap().join(".ck/config.toml").is_file());

        // discovery sees it; idempotent create returns the same world
        assert_eq!(get_campaigns(&conn).unwrap().len(), 1);
        assert_eq!(
            create_campaign(&conn, "w1", "Renamed?", 1, None, false, false)
                .unwrap()
                .name,
            "My World"
        );

        // update rewrites config.toml
        let req = CampaignUpdateRequest {
            system: Some("D&D 5e".into()),
            players: Some(serde_json::json!([{ "player_name": "Aron", "character_name": "Lyra" }])),
            ..Default::default()
        };
        let updated = update_campaign(&conn, "w1", &req).unwrap();
        assert_eq!(updated.system, "D&D 5e");
        let cfg = world_config::read(vault.parent().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(cfg.system, "D&D 5e");
        assert_eq!(cfg.players[0].character_name, "Lyra");

        // sessions folders drive next number
        std::fs::create_dir_all(vault.parent().unwrap().join("Sessions/007")).unwrap();
        assert_eq!(next_session_number(&conn, Some("w1")).unwrap(), 8);

        // recap lives in .ck/recap.md
        set_recap(&conn, "w1", "So far…").unwrap();
        let detail = get_campaign(&conn, "w1").unwrap().unwrap();
        assert_eq!(detail.recap, "So far…");
        assert!(!detail.recap_updated_at.is_empty());

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn moved_world_found_by_scan() {
        let (conn, tmp) = test_conn("move");
        create_campaign(&conn, "w2", "Wanderer", 1, None, false, false).unwrap();
        let old = tmp.join("Wanderer");
        let new = tmp.join("Wanderer Renamed");
        std::fs::rename(&old, &new).unwrap();
        let found = get_campaign(&conn, "w2").unwrap().unwrap();
        assert!(found.vault_path.unwrap().contains("Wanderer Renamed"));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn adopt_foreign_vault_sets_codex_root() {
        let (conn, tmp) = test_conn("adopt");
        let foreign = tmp.join("Old Notes");
        std::fs::create_dir_all(foreign.join("People")).unwrap();
        std::fs::write(foreign.join("People/Aragorn.md"), "# Aragorn\n").unwrap();
        let c = create_campaign(
            &conn,
            "w4",
            "Old Notes",
            1,
            Some(&foreign.to_string_lossy()),
            false,
            true,
        )
        .unwrap();
        // vault root = world root: pages live anywhere
        assert_eq!(PathBuf::from(c.vault_path.unwrap()), foreign);
        assert!(!foreign.join("Codex").exists());
        let cfg = world_config::read(&foreign).unwrap().unwrap();
        assert_eq!(cfg.codex_root, ".");
        // discovery roundtrip keeps the layout
        let again = get_campaign(&conn, "w4").unwrap().unwrap();
        assert_eq!(PathBuf::from(again.vault_path.unwrap()), foreign);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn scaffold_creates_subfolders() {
        let (conn, tmp) = test_conn("scaffold");
        let c = create_campaign(&conn, "w3", "Scaffold World", 1, None, true, false).unwrap();
        let vault = PathBuf::from(c.vault_path.unwrap());
        assert!(vault.join("NPCs").is_dir());
        assert!(vault.join("Places").is_dir());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
