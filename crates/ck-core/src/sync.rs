//! Multi-device sync client (Sprint 2).
//!
//! Offline-first: every local write sets a `dirty` flag (see `store`). A sync
//! cycle pushes all dirty records to the server in one `POST /sync`, receives
//! the server's changes since the last cursor, applies them locally, then
//! clears the `dirty` flags it pushed. The server is authoritative — pulled
//! records overwrite local copies and are marked clean. The other end of the
//! wire is the open-source reference server `chronicle-keeper-sync-server`.
//!
//! Covers campaigns, sessions, artifacts (transcripts + summaries), and codex
//! entries. Deletions propagate too: campaigns/sessions/codex soft-delete via a
//! `deleted` flag; hard-deleted artifacts push a tombstone (see
//! `store::artifacts`). Audio is never synced — `tracks_json`/`session_path` are
//! device-local. The Tauri shell drives [`sync_once`] on a background interval.

use chrono::Local;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const SYNC_PATH: &str = "/sync";

// Wire DTOs — mirror the sync server's Pydantic models (`app/models.py`).

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Campaign {
    pub campaign_id: String,
    pub name: String,
    pub next_session_number: i64,
    pub system: String,
    pub gm: String,
    #[serde(default)]
    pub gm_pronouns: String,
    pub setting: String,
    pub default_language: String,
    #[serde(default)]
    pub players: Value,
    pub extra_info: String,
    #[serde(default)]
    pub codex: String,
    #[serde(default)]
    pub codex_notes: String,
    #[serde(default)]
    pub recap: String,
    #[serde(default)]
    pub recap_updated_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub campaign_id: Option<String>,
    pub session_number: Option<i64>,
    pub title: Option<String>,
    pub date: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub notes: String,
    #[serde(default)]
    pub speakers: Value,
    pub updated_at: String,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Artifact {
    pub artifact_id: String,
    pub session_id: String,
    pub kind: String,
    pub provider: String,
    pub model: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CodexEntry {
    pub entry_id: String,
    pub campaign_id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub source: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SyncPayload {
    #[serde(default)]
    pub campaigns: Vec<Campaign>,
    #[serde(default)]
    pub sessions: Vec<Session>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub deleted_artifact_ids: Vec<String>,
    #[serde(default)]
    pub codex_entries: Vec<CodexEntry>,
}

#[derive(Debug, Serialize)]
pub struct SyncRequest {
    pub client_id: String,
    pub since: Option<String>,
    /// `"merge"` (normal) or `"mirror"` (see [`force_mirror_sync`]).
    pub mode: &'static str,
    pub push: SyncPayload,
}

#[derive(Debug, Deserialize)]
pub struct SyncResponse {
    pub synced_at: String,
    pub pull: SyncPayload,
}

/// Gather every locally-dirty record into a push payload (normal sync).
pub fn collect_dirty(conn: &Connection) -> AppResult<SyncPayload> {
    collect(conn, true)
}

/// Gather the device's full live state for a mirror push. See [`force_mirror_sync`].
pub fn collect_all(conn: &Connection) -> AppResult<SyncPayload> {
    collect(conn, false)
}

/// Body of [`collect_dirty`]/[`collect_all`]: dirty rows only, or all live rows.
fn collect(conn: &Connection, dirty_only: bool) -> AppResult<SyncPayload> {
    let mut payload = SyncPayload::default();

    let row_filter = if dirty_only {
        "WHERE dirty = 1"
    } else {
        "WHERE deleted = 0"
    };

    let mut stmt = conn.prepare(&format!(
        "SELECT campaign_id, name, next_session_number, system, gm, gm_pronouns, setting, \
         default_language, players_json, extra_info, codex, codex_notes, recap, recap_updated_at, \
         updated_at, deleted \
         FROM campaigns {row_filter}",
    ))?;
    let rows = stmt.query_map([], |r| {
        let players_json: String = r.get("players_json")?;
        Ok(Campaign {
            campaign_id: r.get("campaign_id")?,
            name: r.get("name")?,
            next_session_number: r.get("next_session_number")?,
            system: r.get::<_, Option<String>>("system")?.unwrap_or_default(),
            gm: r.get::<_, Option<String>>("gm")?.unwrap_or_default(),
            gm_pronouns: r
                .get::<_, Option<String>>("gm_pronouns")?
                .unwrap_or_default(),
            setting: r.get::<_, Option<String>>("setting")?.unwrap_or_default(),
            default_language: r
                .get::<_, Option<String>>("default_language")?
                .unwrap_or_default(),
            players: serde_json::from_str(&players_json).unwrap_or_else(|_| json!([])),
            extra_info: r
                .get::<_, Option<String>>("extra_info")?
                .unwrap_or_default(),
            codex: r.get::<_, Option<String>>("codex")?.unwrap_or_default(),
            codex_notes: r
                .get::<_, Option<String>>("codex_notes")?
                .unwrap_or_default(),
            recap: r.get::<_, Option<String>>("recap")?.unwrap_or_default(),
            recap_updated_at: r
                .get::<_, Option<String>>("recap_updated_at")?
                .unwrap_or_default(),
            updated_at: r.get("updated_at")?,
            deleted: r.get::<_, i64>("deleted")? != 0,
        })
    })?;
    for r in rows {
        payload.campaigns.push(r?);
    }
    drop(stmt);

    let mut stmt = conn.prepare(&format!(
        "SELECT session_id, campaign_id, session_number, title, date, metadata_json, \
         notes, speakers_json, updated_at, deleted FROM sessions {row_filter}",
    ))?;
    let rows = stmt.query_map([], |r| {
        let metadata_json: String = r.get("metadata_json")?;
        let speakers_json: String = r.get("speakers_json")?;
        Ok(Session {
            session_id: r.get("session_id")?,
            campaign_id: r.get("campaign_id")?,
            session_number: r.get("session_number")?,
            title: r.get("title")?,
            date: r.get("date")?,
            metadata: serde_json::from_str(&metadata_json).unwrap_or_else(|_| json!({})),
            notes: r.get::<_, Option<String>>("notes")?.unwrap_or_default(),
            speakers: serde_json::from_str(&speakers_json).unwrap_or_else(|_| json!([])),
            updated_at: r.get("updated_at")?,
            deleted: r.get::<_, i64>("deleted")? != 0,
        })
    })?;
    for r in rows {
        payload.sessions.push(r?);
    }
    drop(stmt);

    // Artifacts have no `deleted` column (hard-deleted): mirror sends them all.
    let artifact_filter = if dirty_only { "WHERE dirty = 1" } else { "" };
    let mut stmt = conn.prepare(&format!(
        "SELECT artifact_id, session_id, kind, provider, model, content, created_at \
         FROM artifacts {artifact_filter}",
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok(Artifact {
            artifact_id: r.get("artifact_id")?,
            session_id: r.get("session_id")?,
            kind: r.get("kind")?,
            provider: r.get("provider")?,
            model: r.get("model")?,
            content: r.get("content")?,
            created_at: r.get("created_at")?,
        })
    })?;
    for r in rows {
        payload.artifacts.push(r?);
    }
    drop(stmt);

    // A mirror push prunes by absence, so it sends no deletion tombstones.
    if dirty_only {
        payload.deleted_artifact_ids = crate::store::artifacts::collect_deleted_dirty(conn)?;
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT entry_id, campaign_id, name, kind, body, detail, source, updated_at, deleted \
         FROM codex_entries {row_filter}",
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok(CodexEntry {
            entry_id: r.get("entry_id")?,
            campaign_id: r.get("campaign_id")?,
            name: r.get("name")?,
            kind: r.get("kind")?,
            body: r.get::<_, Option<String>>("body")?.unwrap_or_default(),
            detail: r.get::<_, Option<String>>("detail")?.unwrap_or_default(),
            source: r
                .get::<_, Option<String>>("source")?
                .unwrap_or_else(|| "manual".into()),
            updated_at: r
                .get::<_, Option<String>>("updated_at")?
                .unwrap_or_default(),
            deleted: r.get::<_, i64>("deleted")? != 0,
        })
    })?;
    for r in rows {
        payload.codex_entries.push(r?);
    }
    drop(stmt);

    Ok(payload)
}

/// Clear the `dirty` flag on the records we just pushed (by key).
///
/// A local write between collect and clear could re-dirty a record and have it
/// wrongly cleared here; the window is tiny for a single-user app and the next
/// cycle would re-push anyway. Revisit if this proves to drop edits.
fn clear_dirty(conn: &Connection, payload: &SyncPayload) -> AppResult<()> {
    for c in &payload.campaigns {
        conn.execute(
            "UPDATE campaigns SET dirty = 0 WHERE campaign_id = ?1",
            params![c.campaign_id],
        )?;
    }
    for s in &payload.sessions {
        conn.execute(
            "UPDATE sessions SET dirty = 0 WHERE session_id = ?1",
            params![s.session_id],
        )?;
    }
    for a in &payload.artifacts {
        conn.execute(
            "UPDATE artifacts SET dirty = 0 WHERE artifact_id = ?1",
            params![a.artifact_id],
        )?;
    }
    for e in &payload.codex_entries {
        conn.execute(
            "UPDATE codex_entries SET dirty = 0 WHERE entry_id = ?1",
            params![e.entry_id],
        )?;
    }
    crate::store::artifacts::clear_deleted_dirty(conn, &payload.deleted_artifact_ids)?;
    Ok(())
}

/// Apply the server's changes locally. The server is authoritative: pulled
/// records overwrite local copies and are marked clean (`dirty = 0`).
pub fn apply_pull(conn: &Connection, pull: &SyncPayload) -> AppResult<()> {
    for c in &pull.campaigns {
        let players = if c.players.is_null() {
            json!([])
        } else {
            c.players.clone()
        };
        conn.execute(
            "INSERT INTO campaigns \
             (campaign_id, name, next_session_number, system, gm, gm_pronouns, setting, default_language, \
              players_json, extra_info, codex, codex_notes, recap, recap_updated_at, updated_at, deleted, dirty) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 0) \
             ON CONFLICT(campaign_id) DO UPDATE SET \
              name = excluded.name, next_session_number = excluded.next_session_number, \
              system = excluded.system, gm = excluded.gm, gm_pronouns = excluded.gm_pronouns, \
              setting = excluded.setting, \
              default_language = excluded.default_language, players_json = excluded.players_json, \
              extra_info = excluded.extra_info, codex = excluded.codex, codex_notes = excluded.codex_notes, \
              recap = excluded.recap, recap_updated_at = excluded.recap_updated_at, \
              updated_at = excluded.updated_at, deleted = excluded.deleted, dirty = 0",
            params![
                c.campaign_id, c.name, c.next_session_number, c.system, c.gm, c.gm_pronouns, c.setting,
                c.default_language, players.to_string(), c.extra_info, c.codex, c.codex_notes,
                c.recap, c.recap_updated_at, c.updated_at, c.deleted as i64,
            ],
        )?;
    }

    for s in &pull.sessions {
        let metadata = if s.metadata.is_null() {
            json!({})
        } else {
            s.metadata.clone()
        };
        let speakers = if s.speakers.is_null() {
            json!([])
        } else {
            s.speakers.clone()
        };
        // tracks_json and session_path are device-local (audio lives only on the
        // device that recorded it) — never overwritten from the server.
        conn.execute(
            "INSERT INTO sessions \
             (session_id, campaign_id, session_number, title, date, metadata_json, notes, \
              speakers_json, session_path, updated_at, deleted, dirty) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '', ?9, ?10, 0) \
             ON CONFLICT(session_id) DO UPDATE SET \
              campaign_id = excluded.campaign_id, session_number = excluded.session_number, \
              title = excluded.title, date = excluded.date, metadata_json = excluded.metadata_json, \
              notes = excluded.notes, speakers_json = excluded.speakers_json, \
              updated_at = excluded.updated_at, deleted = excluded.deleted, dirty = 0",
            params![
                s.session_id, s.campaign_id, s.session_number, s.title, s.date,
                metadata.to_string(), s.notes, speakers.to_string(), s.updated_at, s.deleted as i64,
            ],
        )?;
    }

    // Artifacts are immutable and content-stable; first writer wins.
    for a in &pull.artifacts {
        conn.execute(
            "INSERT OR IGNORE INTO artifacts \
             (artifact_id, session_id, kind, provider, model, content, created_at, dirty) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                a.artifact_id,
                a.session_id,
                a.kind,
                a.provider,
                a.model,
                a.content,
                a.created_at
            ],
        )?;
    }

    for aid in &pull.deleted_artifact_ids {
        // Remote-initiated: delete without tombstoning (must not echo back).
        crate::store::artifacts::apply_remote_deletion(conn, aid)?;
    }

    for e in &pull.codex_entries {
        // Local-only UNIQUE(campaign, lower(name), kind) over live rows: a live
        // pulled entry can collide with a different local id (same NPC on two
        // devices). Server wins — drop the local duplicate before upserting.
        if !e.deleted {
            conn.execute(
                "DELETE FROM codex_entries \
                 WHERE campaign_id = ?1 AND lower(name) = lower(?2) AND kind = ?3 \
                   AND deleted = 0 AND entry_id != ?4",
                params![e.campaign_id, e.name, e.kind, e.entry_id],
            )?;
        }
        conn.execute(
            "INSERT INTO codex_entries \
             (entry_id, campaign_id, name, kind, body, detail, source, updated_at, deleted, dirty) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0) \
             ON CONFLICT(entry_id) DO UPDATE SET \
              campaign_id = excluded.campaign_id, name = excluded.name, kind = excluded.kind, \
              body = excluded.body, detail = excluded.detail, source = excluded.source, \
              updated_at = excluded.updated_at, deleted = excluded.deleted, dirty = 0",
            params![
                e.entry_id,
                e.campaign_id,
                e.name,
                e.kind,
                e.body,
                e.detail,
                if e.source.is_empty() {
                    "manual"
                } else {
                    &e.source
                },
                e.updated_at,
                e.deleted as i64,
            ],
        )?;
    }

    Ok(())
}

/// Get this device's stable sync id, generating and persisting one on first use.
fn ensure_client_id(conn: &Connection) -> AppResult<String> {
    if let Some(id) = config::get_value(conn, "ck_client_id")? {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    config::set_value(conn, "ck_client_id", &id)?;
    Ok(id)
}

/// Run one full sync cycle. No-op (Ok) if no `sync_url`/`sync_token` is configured.
pub async fn sync_once(state: &AppState) -> AppResult<()> {
    let prep = state.with_db(|conn| -> AppResult<Option<(String, String, SyncRequest)>> {
        let (Some(url), Some(token)) = (
            config::get_value(conn, "sync_url")?,
            config::get_value(conn, "sync_token")?,
        ) else {
            return Ok(None); // sync not configured
        };
        let client_id = ensure_client_id(conn)?;
        let since = config::get_value(conn, "last_sync_at")?;
        let push = collect_dirty(conn)?;
        Ok(Some((
            url,
            token,
            SyncRequest {
                client_id,
                since,
                mode: "merge",
                push,
            },
        )))
    })?;

    let Some((url, token, req)) = prep else {
        return Ok(());
    };

    let endpoint = format!("{}{SYNC_PATH}", url.trim_end_matches('/'));
    let resp: SyncResponse = reqwest::Client::new()
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&req)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sync request failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sync server error: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sync response decode failed: {e}")))?;

    state.with_db(|conn| -> AppResult<()> {
        apply_pull(conn, &resp.pull)?;
        clear_dirty(conn, &req.push)?;
        config::set_value(conn, "last_sync_at", &resp.synced_at)?;
        Ok(())
    })?;

    Ok(())
}

/// Push every live local record with `mode = "mirror"` so the server deletes
/// anything the push omits. Destructive and irreversible — the wipe propagates
/// to every other device. User-triggered only, never the background loop.
/// `BadRequest` if sync is not configured.
pub async fn force_mirror_sync(state: &AppState) -> AppResult<()> {
    let prep = state.with_db(|conn| -> AppResult<Option<(String, String, SyncRequest)>> {
        let (Some(url), Some(token)) = (
            config::get_value(conn, "sync_url")?,
            config::get_value(conn, "sync_token")?,
        ) else {
            return Ok(None); // sync not configured
        };
        let client_id = ensure_client_id(conn)?;
        let since = config::get_value(conn, "last_sync_at")?;
        let push = collect_all(conn)?;
        Ok(Some((
            url,
            token,
            SyncRequest {
                client_id,
                since,
                mode: "mirror",
                push,
            },
        )))
    })?;

    let Some((url, token, req)) = prep else {
        return Err(AppError::BadRequest(
            "Sync is not configured — set a sync URL and token first.".into(),
        ));
    };

    let endpoint = format!("{}{SYNC_PATH}", url.trim_end_matches('/'));
    let resp: SyncResponse = reqwest::Client::new()
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&req)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("mirror sync request failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("mirror sync server error: {e}")))?
        .json()
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("mirror sync response decode failed: {e}"))
        })?;

    state.with_db(|conn| -> AppResult<()> {
        apply_pull(conn, &resp.pull)?;
        clear_dirty(conn, &req.push)?;
        config::set_value(conn, "last_sync_at", &resp.synced_at)?;
        Ok(())
    })?;

    Ok(())
}

/// Run one sync cycle and persist the outcome to config so the UI can display
/// last-synced time and any error. Swallows the error after recording it.
pub async fn sync_once_recording_error(state: &AppState) {
    match sync_once(state).await {
        Ok(()) => {
            let ts = Local::now()
                .naive_local()
                .format("%Y-%m-%dT%H:%M:%S")
                .to_string();
            state
                .with_db(|conn| -> AppResult<()> {
                    config::set_value(conn, "last_sync_ts", &ts)?;
                    config::set_value(conn, "last_sync_error", "")?;
                    Ok(())
                })
                .ok();
        }
        Err(e) => {
            let msg = e.to_string();
            tracing::warn!("sync failed: {msg}");
            state
                .with_db(|conn| config::set_value(conn, "last_sync_error", &msg))
                .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{artifacts, campaigns};

    #[test]
    fn collect_dirty_picks_up_local_writes() {
        let conn = crate::db::open_in_memory().unwrap();
        // Seed the session that artifacts reference (FK target).
        conn.execute("INSERT INTO sessions (session_id) VALUES ('s1')", [])
            .unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp One", 1).unwrap();
        artifacts::insert_artifact(&conn, "s1", "transcript", "sherpa", "m", "hello").unwrap();

        let push = collect_dirty(&conn).unwrap();
        assert_eq!(push.campaigns.len(), 1, "new campaign should be dirty");
        assert_eq!(push.campaigns[0].campaign_id, "c1");
        assert_eq!(push.artifacts.len(), 1, "new artifact should be dirty");
        assert_eq!(push.artifacts[0].content, "hello");
        assert!(
            !push.artifacts[0].artifact_id.is_empty(),
            "artifact gets a sync uuid"
        );

        // After clearing, nothing is dirty.
        clear_dirty(&conn, &push).unwrap();
        let empty = collect_dirty(&conn).unwrap();
        assert!(empty.campaigns.is_empty() && empty.artifacts.is_empty());
    }

    #[test]
    fn collect_all_returns_live_rows_not_just_dirty() {
        use crate::store::sessions;
        let conn = crate::db::open_in_memory().unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp", 1).unwrap();
        // A clean (already-synced) row: nothing dirty, but mirror still sends it.
        conn.execute("UPDATE campaigns SET dirty = 0", []).unwrap();
        assert!(
            collect_dirty(&conn).unwrap().campaigns.is_empty(),
            "clean row is not dirty"
        );
        let all = collect_all(&conn).unwrap();
        assert_eq!(all.campaigns.len(), 1);
        assert_eq!(all.campaigns[0].campaign_id, "c1");
        assert!(
            all.deleted_artifact_ids.is_empty(),
            "mirror push sends no tombstones"
        );

        // A soft-deleted campaign is omitted from a mirror push (server prunes it).
        campaigns::create_campaign(&conn, "c2", "Doomed", 1).unwrap();
        conn.execute(
            "INSERT INTO sessions (session_id, campaign_id, session_number) VALUES ('s1','c2',1)",
            [],
        )
        .unwrap();
        sessions::delete_session(&conn, "s1").unwrap();
        conn.execute(
            "UPDATE campaigns SET deleted = 1 WHERE campaign_id = 'c2'",
            [],
        )
        .unwrap();
        let all = collect_all(&conn).unwrap();
        assert_eq!(all.campaigns.len(), 1, "soft-deleted campaign omitted");
        assert_eq!(all.campaigns[0].campaign_id, "c1");
        assert!(
            all.sessions.iter().all(|s| s.session_id != "s1"),
            "soft-deleted session omitted"
        );
    }

    #[test]
    fn apply_pull_upserts_and_marks_clean() {
        let conn = crate::db::open_in_memory().unwrap();
        // Seed the session that artifacts reference (FK target).
        conn.execute("INSERT INTO sessions (session_id) VALUES ('s1')", [])
            .unwrap();
        let pull = SyncPayload {
            campaigns: vec![Campaign {
                campaign_id: "remote".into(),
                name: "Remote Camp".into(),
                next_session_number: 3,
                default_language: "de".into(),
                players: serde_json::json!([{ "player_name": "Ann", "character_name": "Elf" }]),
                updated_at: "2026-05-27T10:00:00Z".into(),
                ..Default::default()
            }],
            artifacts: vec![Artifact {
                artifact_id: "a-uuid".into(),
                session_id: "s1".into(),
                kind: "summary".into(),
                provider: "ollama".into(),
                model: "llama".into(),
                content: "notes".into(),
                created_at: "2026-05-27T10:00:00Z".into(),
            }],
            ..Default::default()
        };
        apply_pull(&conn, &pull).unwrap();

        let got = campaigns::get_campaign(&conn, "remote")
            .unwrap()
            .expect("campaign applied");
        assert_eq!(got.name, "Remote Camp");
        assert_eq!(got.next_session_number, 3);

        // Pulled records must not be dirty (they came from the server).
        let push = collect_dirty(&conn).unwrap();
        assert!(push.campaigns.is_empty(), "pulled campaign must be clean");
        assert!(push.artifacts.is_empty(), "pulled artifact must be clean");

        // Re-applying the same artifact is ignored (immutable / push-once).
        apply_pull(&conn, &pull).unwrap();
        let arts = artifacts::list_artifacts(&conn, "s1", None).unwrap();
        assert_eq!(arts.len(), 1, "duplicate artifact_id ignored");
    }

    #[test]
    fn recap_round_trips_on_the_campaign() {
        let conn = crate::db::open_in_memory().unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp", 1).unwrap();

        // A generated recap is dirty and pushed with its timestamp.
        campaigns::set_recap(&conn, "c1", "The party rose from nothing.").unwrap();
        let push = collect_dirty(&conn).unwrap();
        let pushed = push
            .campaigns
            .iter()
            .find(|c| c.campaign_id == "c1")
            .expect("campaign pushed");
        assert_eq!(pushed.recap, "The party rose from nothing.");
        assert!(
            !pushed.recap_updated_at.is_empty(),
            "recap timestamp pushed"
        );
        clear_dirty(&conn, &push).unwrap();

        // A recap arriving from another device overwrites the local one.
        let pull = SyncPayload {
            campaigns: vec![Campaign {
                campaign_id: "c1".into(),
                name: "Camp".into(),
                recap: "A darker chapter began.".into(),
                recap_updated_at: "2026-05-29T00:00:00Z".into(),
                updated_at: "2026-05-29T00:00:00Z".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        apply_pull(&conn, &pull).unwrap();
        let got = campaigns::get_campaign(&conn, "c1")
            .unwrap()
            .expect("campaign present");
        assert_eq!(got.recap, "A darker chapter began.");
        assert_eq!(got.recap_updated_at, "2026-05-29T00:00:00Z");
        assert!(
            collect_dirty(&conn).unwrap().campaigns.is_empty(),
            "pulled campaign clean"
        );
    }

    #[test]
    fn deletions_are_collected_and_cleared() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute("INSERT INTO sessions (session_id) VALUES ('s1')", [])
            .unwrap();
        let art = artifacts::insert_artifact(&conn, "s1", "summary", "p", "m", "x").unwrap();

        // Deleting an artifact tombstones it for the next push.
        artifacts::delete_artifact(&conn, art.id).unwrap();
        let push = collect_dirty(&conn).unwrap();
        assert_eq!(push.deleted_artifact_ids, vec![art.artifact_id.clone()]);

        // After clearing, the tombstone is no longer pushed.
        clear_dirty(&conn, &push).unwrap();
        let again = collect_dirty(&conn).unwrap();
        assert!(again.deleted_artifact_ids.is_empty());
    }

    #[test]
    fn codex_entries_round_trip() {
        use crate::models::CodexEntryCreate;
        use crate::store::codex as codex_store;

        let conn = crate::db::open_in_memory().unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp", 1).unwrap();
        let e = codex_store::create_entry(
            &conn,
            "c1",
            &CodexEntryCreate {
                name: "Aragorn".into(),
                kind: "npc".into(),
                body: "Ranger".into(),
                detail: "A weathered ranger of the North.".into(),
            },
        )
        .unwrap();

        // Push picks up the new entry.
        let push = collect_dirty(&conn).unwrap();
        let pushed = push
            .codex_entries
            .iter()
            .find(|x| x.entry_id == e.entry_id)
            .expect("entry pushed");
        assert_eq!(pushed.name, "Aragorn");
        assert_eq!(pushed.detail, "A weathered ranger of the North.");
        assert!(!pushed.deleted);
        clear_dirty(&conn, &push).unwrap();
        assert!(collect_dirty(&conn).unwrap().codex_entries.is_empty());

        // Pull from another device with an updated body — should overwrite.
        let pull = SyncPayload {
            codex_entries: vec![CodexEntry {
                entry_id: e.entry_id.clone(),
                campaign_id: "c1".into(),
                name: "Aragorn".into(),
                kind: "npc".into(),
                body: "Heir of Isildur".into(),
                detail: "Currently in Rivendell seeking counsel.".into(),
                source: "manual".into(),
                updated_at: "2026-05-28T00:00:00Z".into(),
                deleted: false,
            }],
            ..Default::default()
        };
        apply_pull(&conn, &pull).unwrap();
        let entries = codex_store::list_entries(&conn, "c1").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].body, "Heir of Isildur");
        assert_eq!(entries[0].detail, "Currently in Rivendell seeking counsel.");
        // Pulled record clean.
        assert!(collect_dirty(&conn).unwrap().codex_entries.is_empty());

        // Soft-delete arriving from another device hides the entry.
        let pull_del = SyncPayload {
            codex_entries: vec![CodexEntry {
                entry_id: e.entry_id.clone(),
                campaign_id: "c1".into(),
                name: "Aragorn".into(),
                kind: "npc".into(),
                deleted: true,
                updated_at: "2026-05-28T01:00:00Z".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        apply_pull(&conn, &pull_del).unwrap();
        assert!(codex_store::list_entries(&conn, "c1").unwrap().is_empty());
    }

    #[test]
    fn apply_pull_resolves_codex_dedup_collision() {
        use crate::models::CodexEntryCreate;
        use crate::store::codex as codex_store;

        let conn = crate::db::open_in_memory().unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp", 1).unwrap();
        // Local "Aragorn (npc)" with its own entry_id.
        let local = codex_store::create_entry(
            &conn,
            "c1",
            &CodexEntryCreate {
                name: "Aragorn".into(),
                kind: "npc".into(),
                body: "local".into(),
                detail: String::new(),
            },
        )
        .unwrap();

        // Same NPC from another device, different entry_id — collides on the
        // natural key. Server copy must win without tripping the dedup index.
        let pull = SyncPayload {
            codex_entries: vec![CodexEntry {
                entry_id: "remote-id".into(),
                campaign_id: "c1".into(),
                name: "Aragorn".into(),
                kind: "npc".into(),
                body: "remote".into(),
                source: "manual".into(),
                updated_at: "2026-06-02T00:00:00Z".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        apply_pull(&conn, &pull).unwrap();

        let entries = codex_store::list_entries(&conn, "c1").unwrap();
        assert_eq!(entries.len(), 1, "the duplicate was resolved, not doubled");
        assert_eq!(entries[0].entry_id, "remote-id", "server copy wins");
        assert_eq!(entries[0].body, "remote");
        assert_ne!(
            entries[0].entry_id, local.entry_id,
            "local duplicate dropped"
        );
    }

    #[test]
    fn deleting_a_session_soft_deletes_and_tombstones_artifacts() {
        use crate::store::{campaigns, sessions};
        let conn = crate::db::open_in_memory().unwrap();
        campaigns::create_campaign(&conn, "c1", "Camp", 1).unwrap();
        // Direct insert (avoids create_campaign_session's filesystem side effects).
        conn.execute(
            "INSERT INTO sessions (session_id, campaign_id, session_number) VALUES ('s1', 'c1', 1)",
            [],
        )
        .unwrap();
        artifacts::insert_artifact(&conn, "s1", "transcript", "sherpa", "m", "hi").unwrap();

        sessions::delete_session(&conn, "s1").unwrap();

        // Hidden from listings…
        let listed = sessions::list_campaign_sessions(&conn, "c1").unwrap();
        assert!(
            listed.iter().all(|x| x.session_id != "s1"),
            "deleted session hidden"
        );

        // …but pushed as a soft-deleted record + its artifact tombstoned.
        let push = collect_dirty(&conn).unwrap();
        let pushed = push
            .sessions
            .iter()
            .find(|x| x.session_id == "s1")
            .expect("soft-deleted session pushed");
        assert!(pushed.deleted, "session pushed with deleted=true");
        assert_eq!(
            push.deleted_artifact_ids.len(),
            1,
            "session's artifact tombstoned"
        );
    }
}
