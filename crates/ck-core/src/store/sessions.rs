use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::get_config_map;
use crate::error::{AppError, AppResult};
use crate::models::{CampaignSessionInfo, SessionInfo, SessionMetadataRequest};
use crate::normalize::{normalize_metadata, sanitize_folder_name};
use crate::store::{artifacts, campaigns, now};

fn output_root(conn: &Connection) -> AppResult<PathBuf> {
    let map = get_config_map(conn)?;
    let root = map.get("output_root").cloned().unwrap_or_default();
    Ok(shellexpand_home(&root))
}

/// Expand a leading `~` to the home dir (config may store a literal tilde).
fn shellexpand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~") {
        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            return home.join(rest.trim_start_matches(['/', '\\']));
        }
    }
    PathBuf::from(path)
}

fn campaign_name(conn: &Connection, campaign_id: &str) -> Option<String> {
    campaigns::get_campaign(conn, campaign_id)
        .ok()
        .flatten()
        .map(|c| c.name)
}

fn number_in_use(conn: &Connection, campaign_id: &str, number: i64) -> AppResult<bool> {
    let n: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sessions WHERE campaign_id = ?1 AND session_number = ?2 LIMIT 1",
            params![campaign_id, number],
            |r| r.get(0),
        )
        .optional()?;
    Ok(n.is_some())
}

pub fn create_campaign_session(
    conn: &Connection,
    campaign_id: &str,
    session_number: Option<i64>,
    title: Option<&str>,
    date: Option<&str>,
) -> AppResult<CampaignSessionInfo> {
    let Some(campaign) = campaigns::get_campaign(conn, campaign_id)? else {
        return Err(AppError::NotFound(format!(
            "Campaign not found: {campaign_id}"
        )));
    };
    let current_next = campaign.next_session_number;

    let number = match session_number {
        Some(n) => {
            if number_in_use(conn, campaign_id, n)? {
                return Err(AppError::Conflict(format!(
                    "Session number already exists for campaign {campaign_id}: {n}"
                )));
            }
            n
        }
        None => {
            let mut n = current_next;
            while number_in_use(conn, campaign_id, n)? {
                n += 1;
            }
            n
        }
    };

    let session_id = Uuid::new_v4().to_string();
    let safe = sanitize_folder_name(&campaign.name);
    let session_path = output_root(conn)?.join(safe).join(number.to_string());
    std::fs::create_dir_all(&session_path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create session dir: {e}")))?;

    let metadata = normalize_metadata(&Value::Null);
    conn.execute(
        "INSERT INTO sessions \
         (session_id, campaign_id, session_number, title, date, metadata_json, notes, session_path, tracks_json, speakers_json, updated_at, dirty) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, '', ?7, '[]', '[]', ?8, 1)",
        params![
            session_id,
            campaign_id,
            number,
            title,
            date,
            metadata.to_string(),
            session_path.to_string_lossy(),
            now()
        ],
    )?;

    if number >= current_next {
        conn.execute(
            "UPDATE campaigns SET next_session_number = ?1 WHERE campaign_id = ?2",
            params![number + 1, campaign_id],
        )?;
    }

    Ok(CampaignSessionInfo {
        session_id,
        session_number: Some(number),
        title: title.map(str::to_string),
        date: date.map(str::to_string),
        metadata,
        has_tracks: false,
        has_transcription: false,
        has_summary: false,
    })
}

pub fn set_campaign_metadata(conn: &Connection, req: &SessionMetadataRequest) -> AppResult<Value> {
    if !session_exists(conn, &req.session_id)? {
        return Err(AppError::NotFound(format!(
            "Session not found: {}",
            req.session_id
        )));
    }

    let mut session_number = req.session_number;
    let mut should_increment = false;
    if let Some(cid) = &req.campaign_id {
        if session_number.is_none() {
            session_number = Some(campaigns::next_session_number(conn, Some(cid))?);
            should_increment = true;
        }
    }

    let metadata = normalize_metadata(req.metadata.as_ref().unwrap_or(&Value::Null));
    let notes = req.notes.clone().unwrap_or_default();
    conn.execute(
        "UPDATE sessions SET campaign_id = ?1, session_number = ?2, title = ?3, date = ?4, \
         metadata_json = ?5, notes = ?6, updated_at = ?7, dirty = 1 WHERE session_id = ?8",
        params![
            req.campaign_id,
            session_number,
            req.title,
            req.date,
            metadata.to_string(),
            notes,
            now(),
            req.session_id
        ],
    )?;

    if should_increment {
        if let Some(cid) = &req.campaign_id {
            conn.execute(
                "UPDATE campaigns SET next_session_number = next_session_number + 1 WHERE campaign_id = ?1",
                params![cid],
            )?;
        }
    }

    let campaign_name = req
        .campaign_id
        .as_deref()
        .and_then(|c| campaign_name(conn, c));
    Ok(json!({
        "campaign_id": req.campaign_id,
        "campaign_name": campaign_name,
        "session_number": session_number,
        "title": req.title,
        "date": req.date,
        "notes": notes,
        "metadata": metadata,
    }))
}

fn session_exists(conn: &Connection, session_id: &str) -> AppResult<bool> {
    let n: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sessions WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(n.is_some())
}

/// Full `/session/{id}` object the frontend consumes.
pub fn get_session_object(conn: &Connection, session_id: &str) -> AppResult<Value> {
    let row = conn
        .query_row(
            "SELECT campaign_id, session_number, title, date, metadata_json, notes, session_path, \
             tracks_json, speakers_json FROM sessions WHERE session_id = ?1",
            params![session_id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                ))
            },
        )
        .optional()?;
    let Some((
        campaign_id,
        number,
        title,
        date,
        metadata_json,
        notes,
        session_path,
        tracks_json,
        speakers_json,
    )) = row
    else {
        return Err(AppError::NotFound(format!(
            "Session not found: {session_id}"
        )));
    };

    let campaign_name = campaign_id.as_deref().and_then(|c| campaign_name(conn, c));
    let metadata: Value =
        serde_json::from_str(&metadata_json).unwrap_or_else(|_| normalize_metadata(&Value::Null));
    let tracks: Value = serde_json::from_str(&tracks_json).unwrap_or_else(|_| json!([]));
    let speakers: Value = serde_json::from_str(&speakers_json).unwrap_or_else(|_| json!([]));

    Ok(json!({
        "session_id": session_id,
        "session_path": session_path,
        "tracks": tracks,
        "speakers": speakers,
        "metadata": metadata,
        "transcription": {},
        "summary": {},
        "campaign": {
            "campaign_id": campaign_id,
            "campaign_name": campaign_name,
            "session_number": number,
            "title": title,
            "date": date,
            "notes": notes.unwrap_or_default(),
        }
    }))
}

/// `/session/{id}/metadata` — campaign metadata view.
pub fn get_campaign_metadata(conn: &Connection, session_id: &str) -> AppResult<Value> {
    let obj = get_session_object(conn, session_id)?;
    let campaign = obj.get("campaign").cloned().unwrap_or_else(|| json!({}));
    let mut out = campaign;
    if let Value::Object(map) = &mut out {
        map.insert(
            "metadata".into(),
            obj.get("metadata").cloned().unwrap_or_else(|| json!({})),
        );
    }
    Ok(out)
}

/// True when the stored `tracks_json` holds at least one track.
fn tracks_present(tracks_json: &str) -> bool {
    serde_json::from_str::<Value>(tracks_json)
        .ok()
        .and_then(|v| v.as_array().map(|a| !a.is_empty()))
        .unwrap_or(false)
}

pub fn list_sessions(conn: &Connection) -> AppResult<Vec<SessionInfo>> {
    let mut stmt = conn.prepare("SELECT session_id, session_path, tracks_json FROM sessions WHERE deleted = 0 ORDER BY session_id DESC")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let with_transcript = artifacts::session_ids_with_kind(conn, "transcript")?;
    let with_summary = artifacts::session_ids_with_kind(conn, "summary")?;
    let out = rows
        .into_iter()
        .map(|(sid, path, tracks_json)| SessionInfo {
            has_tracks: tracks_present(&tracks_json),
            has_transcription: with_transcript.contains(&sid),
            has_summary: with_summary.contains(&sid),
            // Artifacts live inline in SQLite now; no file paths to surface.
            transcript_path: None,
            summary_path: None,
            session_id: sid,
            session_path: path,
        })
        .collect();
    Ok(out)
}

pub fn list_campaign_sessions(
    conn: &Connection,
    campaign_id: &str,
) -> AppResult<Vec<CampaignSessionInfo>> {
    let mut stmt = conn.prepare(
        "SELECT session_id, session_number, title, date, metadata_json, tracks_json FROM sessions \
         WHERE campaign_id = ?1 AND deleted = 0 ORDER BY session_number DESC",
    )?;
    let rows = stmt
        .query_map(params![campaign_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let with_transcript = artifacts::session_ids_with_kind(conn, "transcript")?;
    let with_summary = artifacts::session_ids_with_kind(conn, "summary")?;
    let out = rows
        .into_iter()
        .map(|(sid, number, title, date, metadata_json, tracks_json)| {
            let metadata: Value = serde_json::from_str(&metadata_json)
                .unwrap_or_else(|_| normalize_metadata(&Value::Null));
            CampaignSessionInfo {
                has_tracks: tracks_present(&tracks_json),
                has_transcription: with_transcript.contains(&sid),
                has_summary: with_summary.contains(&sid),
                session_id: sid,
                session_number: number,
                title,
                date,
                metadata,
            }
        })
        .collect();
    Ok(out)
}

pub fn session_path_of(conn: &Connection, session_id: &str) -> AppResult<Option<String>> {
    let p: Option<String> = conn
        .query_row(
            "SELECT session_path FROM sessions WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(p)
}

/// Resolve the session dir for an upload, creating a bare (campaign-less)
/// session row if the id is unknown. Returns `(session_id, session_path)`.
pub fn resolve_for_upload(
    conn: &Connection,
    session_id: Option<&str>,
) -> AppResult<(String, PathBuf)> {
    if let Some(sid) = session_id {
        if let Some(path) = session_path_of(conn, sid)? {
            return Ok((sid.to_string(), PathBuf::from(path)));
        }
    }
    let sid = session_id
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let path = output_root(conn)?.join("_sessions").join(&sid);
    std::fs::create_dir_all(&path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create session dir: {e}")))?;
    let metadata = normalize_metadata(&Value::Null);
    conn.execute(
        "INSERT INTO sessions (session_id, metadata_json, notes, session_path, tracks_json, speakers_json, updated_at, dirty) \
         VALUES (?1, ?2, '', ?3, '[]', '[]', ?4, 1)",
        params![sid, metadata.to_string(), path.to_string_lossy(), now()],
    )?;
    Ok((sid, path))
}

pub fn set_tracks(conn: &Connection, session_id: &str, tracks: &Value) -> AppResult<()> {
    conn.execute(
        "UPDATE sessions SET tracks_json = ?1, updated_at = ?2, dirty = 1 WHERE session_id = ?3",
        params![tracks.to_string(), now(), session_id],
    )?;
    Ok(())
}

pub fn set_speakers(conn: &Connection, session_id: &str, speakers: &Value) -> AppResult<()> {
    if !session_exists(conn, session_id)? {
        return Err(AppError::NotFound(format!(
            "Session not found: {session_id}"
        )));
    }
    conn.execute(
        "UPDATE sessions SET speakers_json = ?1, updated_at = ?2, dirty = 1 WHERE session_id = ?3",
        params![speakers.to_string(), now(), session_id],
    )?;
    Ok(())
}

pub fn get_tracks(conn: &Connection, session_id: &str) -> AppResult<Value> {
    let tj: Option<String> = conn
        .query_row(
            "SELECT tracks_json FROM sessions WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .optional()?;
    match tj {
        Some(s) => Ok(serde_json::from_str(&s).unwrap_or_else(|_| json!([]))),
        None => Err(AppError::NotFound(format!(
            "Session not found: {session_id}"
        ))),
    }
}

pub fn get_speakers(conn: &Connection, session_id: &str) -> AppResult<Value> {
    let sj: Option<String> = conn
        .query_row(
            "SELECT speakers_json FROM sessions WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(sj
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!([])))
}

pub fn delete_session(conn: &Connection, session_id: &str) -> AppResult<()> {
    let path: Option<String> = conn
        .query_row(
            "SELECT session_path FROM sessions WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(path) = path else {
        return Err(AppError::NotFound(format!(
            "Session not found: {session_id}"
        )));
    };
    // Artifacts are hard-deleted + tombstoned (so the deletion syncs). The
    // session row is *soft*-deleted: kept with deleted=1 + dirty=1 so the
    // tombstone propagates to other devices; list queries filter it out.
    artifacts::delete_artifacts_for_session(conn, session_id)?;
    conn.execute(
        "UPDATE sessions SET deleted = 1, dirty = 1, updated_at = ?1 WHERE session_id = ?2",
        params![now(), session_id],
    )?;
    // Audio is device-local and now orphaned — reclaim the disk.
    if !path.is_empty() {
        let _ = std::fs::remove_dir_all(&path);
    }
    Ok(())
}
