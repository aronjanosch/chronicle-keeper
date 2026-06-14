//! 0.X → 1.0 one-time migration (Phase 1.7-F): reads the legacy DB read-only
//! (never written — the old app can still open it) and writes world folders +
//! session files. Files are truth; no campaign/session/artifact rows land in
//! the v1 DB. Idempotent (existing file values win), so a partially failed run
//! is simply re-run. Legacy codex/codex_notes are NOT migrated — Phase 5
//! import reads them from the legacy DB instead.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

use crate::config;
use crate::error::{AppError, AppResult};
use crate::session_files;
use crate::store::campaigns;

const DONE_KEY: &str = "migrated_v1";

// ── Status ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CampaignStatus {
    pub campaign_id: String,
    pub name: String,
    pub session_count: usize,
}

#[derive(Debug, Serialize)]
pub struct MigrationStatus {
    pub needs_migration: bool,
    pub campaigns: Vec<CampaignStatus>,
    /// Sessions that won't migrate: no campaign or no session number.
    pub skipped_sessions: usize,
}

fn no_migration() -> MigrationStatus {
    MigrationStatus {
        needs_migration: false,
        campaigns: vec![],
        skipped_sessions: 0,
    }
}

/// Return migration status without running anything. `conn` is the v1 DB.
pub fn status(conn: &Connection, legacy_db: &Path) -> AppResult<MigrationStatus> {
    if config::get_value(conn, DONE_KEY)?.as_deref() == Some("done") || !legacy_db.exists() {
        return Ok(no_migration());
    }
    let Ok(legacy) = open_legacy(legacy_db) else {
        return Ok(no_migration());
    };

    let mut list: Vec<CampaignStatus> = Vec::new();
    let mut stmt = legacy.prepare(
        "SELECT c.campaign_id, c.name, \
                (SELECT COUNT(*) FROM sessions s \
                 WHERE s.campaign_id = c.campaign_id AND s.deleted = 0 \
                   AND s.session_number IS NOT NULL) \
         FROM campaigns c WHERE c.deleted = 0 ORDER BY c.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(CampaignStatus {
            campaign_id: r.get(0)?,
            name: r.get(1)?,
            session_count: r.get(2)?,
        })
    })?;
    for row in rows {
        list.push(row?);
    }

    let skipped = skipped_session_count(&legacy)?;
    let needs = !list.is_empty();
    Ok(MigrationStatus {
        needs_migration: needs,
        campaigns: list,
        skipped_sessions: skipped,
    })
}

// ── Result ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MigrationResult {
    pub ok: bool,
    pub campaigns_migrated: usize,
    pub sessions_migrated: usize,
    pub sessions_skipped: usize,
    pub errors: Vec<String>,
}

// ── Run ───────────────────────────────────────────────────────────

/// Run the full migration. `conn` is the v1 DB; the legacy DB is opened
/// read-only for the duration of the run.
pub fn run_all(conn: &Connection, legacy_db: &Path) -> AppResult<MigrationResult> {
    let legacy = open_legacy(legacy_db)?;
    let mut errors: Vec<String> = Vec::new();
    let mut campaigns_done = 0usize;
    let mut sessions_done = 0usize;

    // Settings first: provisioning below resolves output_root from v1 config.
    copy_settings(&legacy, conn)?;

    for c in legacy_campaigns(&legacy)? {
        if let Err(e) = campaigns::create_campaign(
            conn,
            &c.campaign_id,
            &c.name,
            c.next_session_number,
            None,
            false,
            false,
        ) {
            errors.push(format!("{}: provision world failed: {e}", c.name));
            continue;
        }
        if let Err(e) = carry_campaign_into_config(conn, &c) {
            errors.push(format!("{}: write config.toml failed: {e}", c.name));
            continue;
        }
        let vault_codex = match campaigns::get_campaign(conn, &c.campaign_id) {
            Ok(Some(detail)) => detail.vault_path,
            _ => None,
        };
        let Some(vault_codex) = vault_codex else {
            errors.push(format!("{}: no world folder after provisioning", c.name));
            continue;
        };

        let mut used_numbers: HashSet<i64> = HashSet::new();
        let mut camp_ok = true;
        for s in legacy_sessions(&legacy, &c.campaign_id)? {
            let Some(number) = s.number else { continue };
            if !used_numbers.insert(number) {
                errors.push(format!(
                    "{}: duplicate session number {number} — kept first",
                    c.name
                ));
                camp_ok = false;
                continue;
            }
            let Some(new_path) = session_files::vault_session_path(&vault_codex, number) else {
                continue;
            };
            match migrate_session(&legacy, &c, &s, &new_path) {
                Ok(_) => sessions_done += 1,
                Err(e) => {
                    errors.push(format!("{} session {number}: {e}", c.name));
                    camp_ok = false;
                }
            }
        }
        if camp_ok {
            campaigns_done += 1;
        }
    }

    let skipped = skipped_session_count(&legacy)?;
    if errors.is_empty() {
        config::set_value(conn, DONE_KEY, "done")?;
    }

    Ok(MigrationResult {
        ok: errors.is_empty(),
        campaigns_migrated: campaigns_done,
        sessions_migrated: sessions_done,
        sessions_skipped: skipped,
        errors,
    })
}

// ── Legacy reads ──────────────────────────────────────────────────

fn open_legacy(path: &Path) -> AppResult<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("open legacy db read-only: {e}")))
}

fn skipped_session_count(legacy: &Connection) -> AppResult<usize> {
    let n: i64 = legacy.query_row(
        "SELECT COUNT(*) FROM sessions \
         WHERE deleted = 0 AND (campaign_id IS NULL OR session_number IS NULL)",
        [],
        |r| r.get(0),
    )?;
    Ok(n as usize)
}

struct LegacyCampaign {
    campaign_id: String,
    name: String,
    next_session_number: i64,
    system: Option<String>,
    gm: Option<String>,
    setting: Option<String>,
    default_language: Option<String>,
    players_json: String,
    extra_info: Option<String>,
    recap: Option<String>,
    recap_updated_at: Option<String>,
    gm_pronouns: Option<String>,
}

fn legacy_campaigns(legacy: &Connection) -> AppResult<Vec<LegacyCampaign>> {
    let mut stmt = legacy.prepare(
        "SELECT campaign_id, name, next_session_number, system, gm, setting, \
                default_language, players_json, extra_info, \
                recap, recap_updated_at, gm_pronouns \
         FROM campaigns WHERE deleted = 0 ORDER BY name",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(LegacyCampaign {
                campaign_id: r.get(0)?,
                name: r.get(1)?,
                next_session_number: r.get(2)?,
                system: r.get(3)?,
                gm: r.get(4)?,
                setting: r.get(5)?,
                default_language: r.get(6)?,
                players_json: r.get(7)?,
                extra_info: r.get(8)?,
                recap: r.get(9)?,
                recap_updated_at: r.get(10)?,
                gm_pronouns: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

struct LegacySession {
    session_id: String,
    number: Option<i64>,
    title: Option<String>,
    date: Option<String>,
    metadata_json: String,
    notes: Option<String>,
    session_path: String,
    tracks_json: String,
    speakers_json: String,
}

fn legacy_sessions(legacy: &Connection, campaign_id: &str) -> AppResult<Vec<LegacySession>> {
    let mut stmt = legacy.prepare(
        "SELECT session_id, session_number, title, date, metadata_json, notes, \
                session_path, tracks_json, speakers_json \
         FROM sessions WHERE campaign_id = ?1 AND deleted = 0 \
         ORDER BY session_number",
    )?;
    let rows = stmt
        .query_map(params![campaign_id], |r| {
            Ok(LegacySession {
                session_id: r.get(0)?,
                number: r.get(1)?,
                title: r.get(2)?,
                date: r.get(3)?,
                metadata_json: r.get(4)?,
                notes: r.get(5)?,
                session_path: r.get(6)?,
                tracks_json: r.get(7)?,
                speakers_json: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

struct LegacyArtifact {
    provider: String,
    model: String,
    content: String,
    created_at: String,
}

// ── v1 writes ─────────────────────────────────────────────────────

/// OR IGNORE throughout: anything already set in the v1 DB wins.
fn copy_settings(legacy: &Connection, conn: &Connection) -> AppResult<()> {
    let mut stmt = legacy.prepare("SELECT key, value FROM config")?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (key, value) in rows {
        // output_root intentionally not carried over — 1.0 worlds get a fresh
        // root, the old tree stays an untouched artifact.
        if key == "world_format_v1" || key == DONE_KEY || key == "output_root" {
            continue;
        }
        conn.execute(
            "INSERT OR IGNORE INTO config (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
    }

    let mut stmt = legacy.prepare(
        "SELECT provider_id, api_key, api_base, default_model, updated_at FROM provider_keys",
    )?;
    let rows: Vec<(String, String, String, String, String)> = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, key, base, model, ts) in rows {
        conn.execute(
            "INSERT OR IGNORE INTO provider_keys (provider_id, api_key, api_base, default_model, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, key, base, model, ts],
        )?;
    }

    // Older legacy DBs may predate prompt_templates — best-effort.
    if let Ok(mut stmt) = legacy
        .prepare("SELECT id, label, text, builtin, sort_order, updated_at FROM prompt_templates")
    {
        let rows: Vec<(String, String, String, i64, i64, String)> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, label, text, builtin, sort, ts) in rows {
            conn.execute(
                "INSERT OR IGNORE INTO prompt_templates (id, label, text, builtin, sort_order, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, label, text, builtin, sort, ts],
            )?;
        }
    }
    Ok(())
}

/// Carry the legacy campaign identity into the freshly provisioned world's
/// `.ck/config.toml` (+ recap into `.ck/recap.md`).
fn carry_campaign_into_config(conn: &Connection, c: &LegacyCampaign) -> AppResult<()> {
    let players: Value = serde_json::from_str(&c.players_json).unwrap_or(Value::Array(vec![]));
    let req = crate::models::CampaignUpdateRequest {
        system: c.system.clone(),
        gm: c.gm.clone(),
        gm_pronouns: c.gm_pronouns.clone(),
        setting: c.setting.clone(),
        default_language: c.default_language.clone(),
        extra_info: c.extra_info.clone(),
        players: Some(players),
        ..Default::default()
    };
    campaigns::update_campaign(conn, &c.campaign_id, &req)?;
    if let Some(recap) = c.recap.as_deref().filter(|r| !r.trim().is_empty()) {
        let root = campaigns::world_root_for_id(conn, &c.campaign_id)?
            .ok_or_else(|| AppError::NotFound(format!("world vanished: {}", c.campaign_id)))?;
        let ts = c.recap_updated_at.as_deref().unwrap_or("");
        crate::world_config::write_recap(&root, recap, ts)?;
    }
    Ok(())
}

fn io_err(what: &str, e: std::io::Error) -> AppError {
    AppError::Internal(anyhow::anyhow!("{what}: {e}"))
}

fn migrate_session(
    legacy: &Connection,
    campaign: &LegacyCampaign,
    s: &LegacySession,
    new_path: &Path,
) -> AppResult<()> {
    std::fs::create_dir_all(new_path).map_err(|e| io_err("create session folder", e))?;

    // Audio failures abort before session.toml is written, so a re-run picks
    // the session up again. Fork-interim sessions already at the target path
    // are only registered — copying would nest audio/audio/.
    let old_path = Path::new(&s.session_path);
    let already_in_place = old_path.canonicalize().ok() == new_path.canonicalize().ok();
    if !already_in_place {
        session_files::copy_audio_files(old_path, &session_files::audio_dir(new_path))
            .map_err(|e| io_err("copy audio", e))?;
    }

    let language = campaign.default_language.as_deref().unwrap_or("en");
    let tracks = serde_json::from_str(&s.tracks_json).unwrap_or(Value::Array(vec![]));
    let speakers = serde_json::from_str(&s.speakers_json).unwrap_or(Value::Array(vec![]));
    let mut st = session_files::SessionToml::from_json_parts(
        s.number,
        s.title.as_deref(),
        s.date.as_deref(),
        language,
        &tracks,
        &speakers,
    );
    st.id = Some(s.session_id.clone());
    let metadata: Value = serde_json::from_str(&s.metadata_json).unwrap_or(Value::Null);
    st.metadata = crate::store::sessions::metadata_from_value(
        &crate::normalize::normalize_metadata(&metadata),
    );
    st.notes = s.notes.clone().unwrap_or_default();
    // Fork-interim sessions may carry post-0.X edits in their session.toml —
    // existing file values win over the legacy DB.
    if let Ok(Some(existing)) = session_files::read_session_toml(new_path) {
        if existing.id.is_some() {
            st.id = existing.id;
        }
        if !existing.metadata.is_empty() {
            st.metadata = existing.metadata;
        }
        if !existing.notes.is_empty() {
            st.notes = existing.notes;
        }
        st.transcript = existing.transcript;
    }
    session_files::write_session_toml_file(new_path, &st)
        .map_err(|e| io_err("write session.toml", e))?;

    if let Some(a) = latest_artifact(legacy, &s.session_id, "transcript")? {
        session_files::write_transcript_md(new_path, &a.content)
            .map_err(|e| io_err("write transcript.md", e))?;
    }
    if let Some(a) = latest_artifact(legacy, &s.session_id, "summary")? {
        session_files::write_summary_md(
            new_path,
            &a.content,
            s.number,
            s.date.as_deref(),
            s.title.as_deref(),
            &a.provider,
            &a.model,
            &a.created_at,
        )
        .map_err(|e| io_err("write summary.md", e))?;
    }

    Ok(())
}

fn latest_artifact(
    legacy: &Connection,
    session_id: &str,
    kind: &str,
) -> AppResult<Option<LegacyArtifact>> {
    let row = legacy
        .query_row(
            "SELECT provider, model, content, created_at \
             FROM artifacts WHERE session_id = ?1 AND kind = ?2 \
             ORDER BY created_at DESC LIMIT 1",
            params![session_id, kind],
            |r| {
                Ok(LegacyArtifact {
                    provider: r.get(0)?,
                    model: r.get(1)?,
                    content: r.get(2)?,
                    created_at: r.get(3)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-mig-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 0.X schema as shipped — includes the sync-era columns (`deleted`,
    /// `dirty`, `updated_at`) the legacy reads filter on.
    fn legacy_db(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("chronicle_keeper.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE campaigns (
                campaign_id TEXT PRIMARY KEY, name TEXT NOT NULL,
                next_session_number INTEGER NOT NULL DEFAULT 1,
                system TEXT, gm TEXT, setting TEXT, default_language TEXT,
                players_json TEXT NOT NULL DEFAULT '[]', extra_info TEXT,
                codex TEXT NOT NULL DEFAULT '', codex_notes TEXT NOT NULL DEFAULT '',
                recap TEXT NOT NULL DEFAULT '', recap_updated_at TEXT NOT NULL DEFAULT '',
                gm_pronouns TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT '',
                deleted INTEGER NOT NULL DEFAULT 0, dirty INTEGER NOT NULL DEFAULT 1);
             CREATE TABLE sessions (
                session_id TEXT PRIMARY KEY, campaign_id TEXT, session_number INTEGER,
                title TEXT, date TEXT, metadata_json TEXT NOT NULL DEFAULT '{}',
                notes TEXT, session_path TEXT NOT NULL DEFAULT '',
                tracks_json TEXT NOT NULL DEFAULT '[]', speakers_json TEXT NOT NULL DEFAULT '[]',
                updated_at TEXT NOT NULL DEFAULT '',
                deleted INTEGER NOT NULL DEFAULT 0, dirty INTEGER NOT NULL DEFAULT 1);
             CREATE TABLE artifacts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                artifact_id TEXT NOT NULL DEFAULT '', session_id TEXT NOT NULL,
                kind TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL,
                file_path TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL, dirty INTEGER NOT NULL DEFAULT 1);
             CREATE TABLE provider_keys (
                provider_id TEXT PRIMARY KEY, api_key TEXT NOT NULL DEFAULT '',
                api_base TEXT NOT NULL DEFAULT '', default_model TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT '');
             CREATE TABLE prompt_templates (
                id TEXT PRIMARY KEY, label TEXT NOT NULL, text TEXT NOT NULL,
                builtin INTEGER NOT NULL DEFAULT 0, sort_order INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT '');",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('output_root', ?1)",
            params![dir.join("legacy-out").to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO campaigns (campaign_id, name, default_language, system, gm, setting, \
                                    players_json, recap, recap_updated_at) \
             VALUES ('c1', 'Ashfall', 'de', 'D&D 5e', 'The Keeper', 'A frontier of ash.', \
                     '[{\"player_name\":\"Sam\",\"character_name\":\"Lyra\"}]', \
                     'Previously…', '2025-01-16T10:00:00')",
            [],
        )
        .unwrap();
        // Old-layout session with one audio file + a numberless one (skipped).
        let old_session = dir.join("old").join("Ashfall").join("1");
        std::fs::create_dir_all(&old_session).unwrap();
        std::fs::write(old_session.join("track-aria.flac"), b"FLACDATA").unwrap();
        conn.execute(
            "INSERT INTO sessions (session_id, campaign_id, session_number, title, date, notes, metadata_json, session_path, tracks_json) \
             VALUES ('s1', 'c1', 1, 'The Iron Crown', '2025-01-15', 'my notes', '{\"tags\":[\"Kampf\"],\"characters\":[\"Lyra\"]}', ?1, '[{\"id\":\"aria\",\"filename\":\"track-aria.flac\"}]')",
            params![old_session.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions (session_id, campaign_id, session_path) VALUES ('s2', 'c1', '')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO artifacts (artifact_id, session_id, kind, provider, model, content, created_at) \
             VALUES ('a1', 's1', 'summary', 'ollama', 'llama3', 'It began at dusk.', '2025-01-16T10:00:00')",
            [],
        )
        .unwrap();
        path
    }

    #[test]
    fn migrates_legacy_db_into_files_and_v1_cache() {
        let dir = tmp_dir("run");
        let legacy_path = legacy_db(&dir);
        let conn = crate::db::open_in_memory().unwrap();
        // The app's own root (legacy output_root is deliberately not carried over).
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('output_root', ?1)",
            params![dir.join("out").to_string_lossy()],
        )
        .unwrap();

        let st = status(&conn, &legacy_path).unwrap();
        assert!(st.needs_migration);
        assert_eq!(st.campaigns.len(), 1);
        assert_eq!(st.campaigns[0].session_count, 1);
        assert_eq!(st.skipped_sessions, 1);

        let res = run_all(&conn, &legacy_path).unwrap();
        assert!(res.ok, "errors: {:?}", res.errors);
        assert_eq!(res.campaigns_migrated, 1);
        assert_eq!(res.sessions_migrated, 1);
        assert_eq!(res.sessions_skipped, 1);

        // Files are truth: campaign identity in config.toml + recap.md…
        let world = dir.join("out").join("Ashfall");
        let cfg = crate::world_config::read(&world).unwrap().unwrap();
        assert_eq!(cfg.system, "D&D 5e");
        assert_eq!(cfg.gm, "The Keeper");
        assert_eq!(cfg.default_language, "de");
        assert_eq!(cfg.players[0].character_name, "Lyra");
        let (recap, recap_at) = crate::world_config::read_recap(&world);
        assert_eq!(recap, "Previously…");
        assert_eq!(recap_at, "2025-01-16T10:00:00");
        assert!(world.join("Codex").is_dir());

        // …session identity/metadata/notes in session.toml.
        let session = world.join("Sessions").join("001");
        assert!(session.join("audio").join("track-aria.flac").is_file());
        let st = session_files::read_session_toml(&session).unwrap().unwrap();
        assert_eq!(st.id.as_deref(), Some("s1"));
        assert_eq!(st.notes, "my notes");
        assert_eq!(st.metadata.tags, vec!["Kampf"]);
        assert_eq!(st.metadata.characters, vec!["Lyra"]);
        let summary = std::fs::read_to_string(session.join("summary.md")).unwrap();
        assert!(summary.contains("provider: ollama"));
        assert!(summary.contains("It began at dusk."));

        // No campaign/session/artifact data lands in the v1 DB (FK shim aside).
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);

        // Done flag set; status flips off. Re-run stays idempotent.
        assert!(!status(&conn, &legacy_path).unwrap().needs_migration);
        let res2 = run_all(&conn, &legacy_path).unwrap();
        assert!(res2.ok);
        let again = session_files::read_session_toml(&session).unwrap().unwrap();
        assert_eq!(again.id.as_deref(), Some("s1"));

        // Legacy DB untouched: still has its rows, no new columns flipped.
        let legacy = open_legacy(&legacy_path).unwrap();
        let old_path: String = legacy
            .query_row(
                "SELECT session_path FROM sessions WHERE session_id = 's1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!old_path.ends_with("Sessions/001"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn status_without_legacy_db_is_clean() {
        let dir = tmp_dir("none");
        let conn = crate::db::open_in_memory().unwrap();
        let st = status(&conn, &dir.join("missing.db")).unwrap();
        assert!(!st.needs_migration);
        std::fs::remove_dir_all(&dir).ok();
    }
}
