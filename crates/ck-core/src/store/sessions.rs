//! Sessions — files are truth. A session is a `Sessions/<NNN>/` folder inside
//! its world (audio/ + transcript.md + summary.md + session.toml); bare
//! uploads live under `<data-root>/_sessions/<id>/` until assigned to a world.
//! The global DB is not consulted; lookups scan the (small) world list.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::get_config_map;
use crate::error::{AppError, AppResult};
use crate::models::{CampaignSessionInfo, SessionInfo, SessionMetadataRequest};
use crate::normalize::normalize_metadata;
use crate::session_files::{self, SessionMetadata, SessionToml};
use crate::store::campaigns;
use crate::world_config::WorldConfig;

pub(crate) fn output_root(conn: &Connection) -> AppResult<PathBuf> {
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

// ── Locating sessions on disk ─────────────────────────────────────

pub(crate) struct SessionLoc {
    pub dir: PathBuf,
    pub st: SessionToml,
    /// The owning world; `None` for bare (campaign-less) upload sessions.
    pub world: Option<(PathBuf, WorldConfig)>,
}

pub(crate) fn session_dirs(world_root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(world_root.join("Sessions")) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
                out.push(p);
            }
        }
    }
    out
}

/// session.toml of a dir; tolerates a missing file (bare upload before any write).
fn toml_of(dir: &Path) -> SessionToml {
    session_files::read_session_toml(dir).ok().flatten().unwrap_or_default()
}

/// Back-fill a missing `id` (pre-Phase-2 session.toml) so the session is addressable.
fn ensure_id(dir: &Path, st: &mut SessionToml) {
    if st.id.is_none() {
        st.id = Some(Uuid::new_v4().to_string());
        let _ = session_files::write_session_toml_file(dir, st);
    }
}

pub(crate) fn locate(conn: &Connection, session_id: &str) -> AppResult<Option<SessionLoc>> {
    if session_id.is_empty() || session_id.contains(['/', '\\', '.']) && session_id.contains("..") {
        return Ok(None);
    }
    // Bare uploads: dir name == session id.
    let bare = output_root(conn)?.join("_sessions").join(session_id);
    if bare.is_dir() {
        let mut st = toml_of(&bare);
        if st.id.is_none() {
            st.id = Some(session_id.to_string());
            let _ = session_files::write_session_toml_file(&bare, &st);
        }
        return Ok(Some(SessionLoc { dir: bare, st, world: None }));
    }
    for (root, cfg) in campaigns::worlds(conn)? {
        for dir in session_dirs(&root) {
            let mut st = toml_of(&dir);
            ensure_id(&dir, &mut st);
            if st.id.as_deref() == Some(session_id) {
                return Ok(Some(SessionLoc { dir, st, world: Some((root, cfg)) }));
            }
        }
    }
    Ok(None)
}

fn require(conn: &Connection, session_id: &str) -> AppResult<SessionLoc> {
    locate(conn, session_id)?
        .ok_or_else(|| AppError::NotFound(format!("Session not found: {session_id}")))
}

// ── session.toml ↔ DTO shapes ─────────────────────────────────────

// Recursively find an audio file by name under `audio/` (Craig zips can nest).
fn find_audio(dir: &Path, filename: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, filename: &str) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Some(found) = walk(&p, filename) {
                    return Some(found);
                }
            } else if entry.file_name().to_string_lossy() == filename {
                return Some(p);
            }
        }
        None
    }
    walk(&session_files::audio_dir(dir), filename)
}

fn stem_of(filename: &str) -> String {
    Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename)
        .to_string()
}

fn tracks_value(dir: &Path, st: &SessionToml) -> Value {
    json!(st
        .tracks
        .iter()
        .map(|t| {
            let file_path = find_audio(dir, &t.filename)
                .unwrap_or_else(|| session_files::audio_dir(dir).join(&t.filename));
            json!({
                "id": stem_of(&t.filename),
                "filename": t.filename,
                "file_path": file_path.to_string_lossy(),
                "duration": Value::Null,
            })
        })
        .collect::<Vec<_>>())
}

fn speakers_value(st: &SessionToml) -> Value {
    json!(st
        .tracks
        .iter()
        .filter(|t| !t.speaker.is_empty() || !t.character.is_empty() || !t.pronouns.is_empty())
        .map(|t| {
            json!({
                "track_id": stem_of(&t.filename),
                "player_name": t.speaker,
                "character_name": t.character,
                "pronouns": t.pronouns,
            })
        })
        .collect::<Vec<_>>())
}

fn metadata_value(st: &SessionToml) -> Value {
    json!({
        "characters": st.metadata.characters,
        "locations": st.metadata.locations,
        "events": st.metadata.events,
        "items": st.metadata.items,
        "tags": st.metadata.tags,
    })
}

pub(crate) fn metadata_from_value(v: &Value) -> SessionMetadata {
    let list = |key: &str| {
        v.get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    SessionMetadata {
        characters: list("characters"),
        locations: list("locations"),
        events: list("events"),
        items: list("items"),
        tags: list("tags"),
    }
}

fn has_tracks(st: &SessionToml) -> bool {
    !st.tracks.is_empty()
}

fn file_nonempty(p: &Path) -> bool {
    p.metadata().map(|m| m.len() > 0).unwrap_or(false)
}

pub(crate) fn has_transcript(dir: &Path) -> bool {
    file_nonempty(&session_files::transcript_md_path(dir))
}

pub(crate) fn has_summary(dir: &Path) -> bool {
    file_nonempty(&session_files::summary_md_path(dir))
}

fn campaign_session_info(dir: &Path, st: &SessionToml) -> CampaignSessionInfo {
    CampaignSessionInfo {
        session_id: st.id.clone().unwrap_or_default(),
        session_number: st.number,
        title: st.title.clone(),
        date: st.date.clone(),
        metadata: metadata_value(st),
        has_tracks: has_tracks(st),
        has_transcription: has_transcript(dir),
        has_summary: has_summary(dir),
    }
}

// ── Reads ─────────────────────────────────────────────────────────

pub fn list_campaign_sessions(
    conn: &Connection,
    campaign_id: &str,
) -> AppResult<Vec<CampaignSessionInfo>> {
    let Some(root) = campaigns::world_root_for_id(conn, campaign_id)? else {
        return Ok(Vec::new());
    };
    let mut out: Vec<CampaignSessionInfo> = session_dirs(&root)
        .iter()
        .map(|dir| {
            let mut st = toml_of(dir);
            ensure_id(dir, &mut st);
            campaign_session_info(dir, &st)
        })
        .collect();
    out.sort_by(|a, b| b.session_number.cmp(&a.session_number));
    Ok(out)
}

/// Sessions carrying an in-world `world_date` (Phase 11.5G), shaped as
/// synthetic timeline rows: (`session:<id>` path, title, "session" kind,
/// frontmatter json with date/end_date).
pub fn world_dated_session_rows(
    conn: &Connection,
    campaign_id: &str,
) -> AppResult<Vec<crate::store::index::PageFrontmatter>> {
    let Some(root) = campaigns::world_root_for_id(conn, campaign_id)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for dir in session_dirs(&root) {
        let mut st = toml_of(&dir);
        let Some(wd) = st.world_date.clone().filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        ensure_id(&dir, &mut st);
        let title = st.title.clone().unwrap_or_else(|| {
            format!("Session {:02}", st.number.unwrap_or(0))
        });
        let fm = serde_json::json!({ "date": wd, "end_date": st.world_date_end });
        out.push((
            format!("session:{}", st.id.clone().unwrap_or_default()),
            title,
            Some("session".to_string()),
            fm.to_string(),
        ));
    }
    Ok(out)
}

pub fn list_sessions(conn: &Connection) -> AppResult<Vec<SessionInfo>> {
    let mut out = Vec::new();
    let mut push = |dir: &Path, st: &SessionToml| {
        out.push(SessionInfo {
            session_id: st.id.clone().unwrap_or_default(),
            session_path: dir.to_string_lossy().to_string(),
            has_tracks: has_tracks(st),
            has_transcription: has_transcript(dir),
            has_summary: has_summary(dir),
            transcript_path: None,
            summary_path: None,
        });
    };
    for (root, _) in campaigns::worlds(conn)? {
        for dir in session_dirs(&root) {
            let mut st = toml_of(&dir);
            ensure_id(&dir, &mut st);
            push(&dir, &st);
        }
    }
    // Bare uploads (not yet assigned to a world).
    if let Ok(entries) = std::fs::read_dir(output_root(conn)?.join("_sessions")) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let mut st = toml_of(&dir);
            if st.id.is_none() {
                st.id = Some(entry.file_name().to_string_lossy().to_string());
            }
            push(&dir, &st);
        }
    }
    Ok(out)
}

/// Full `/session/{id}` object the frontend consumes.
pub fn get_session_object(conn: &Connection, session_id: &str) -> AppResult<Value> {
    let loc = require(conn, session_id)?;
    let (campaign_id, campaign_name) = match &loc.world {
        Some((_, cfg)) => (Some(cfg.id.clone()), Some(cfg.name.clone())),
        None => (None, None),
    };
    Ok(json!({
        "session_id": session_id,
        "session_path": loc.dir.to_string_lossy(),
        "tracks": tracks_value(&loc.dir, &loc.st),
        "speakers": speakers_value(&loc.st),
        "metadata": metadata_value(&loc.st),
        "transcription": {},
        "summary": {},
        "campaign": {
            "campaign_id": campaign_id,
            "campaign_name": campaign_name,
            "session_number": loc.st.number,
            "title": loc.st.title,
            "date": loc.st.date,
            "notes": loc.st.notes,
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

pub fn session_path_of(conn: &Connection, session_id: &str) -> AppResult<Option<String>> {
    Ok(locate(conn, session_id)?.map(|l| l.dir.to_string_lossy().to_string()))
}

pub fn get_tracks(conn: &Connection, session_id: &str) -> AppResult<Value> {
    let loc = require(conn, session_id)?;
    Ok(tracks_value(&loc.dir, &loc.st))
}

pub fn get_speakers(conn: &Connection, session_id: &str) -> AppResult<Value> {
    Ok(locate(conn, session_id)?
        .map(|l| speakers_value(&l.st))
        .unwrap_or_else(|| json!([])))
}

// ── Writes ────────────────────────────────────────────────────────

fn number_dir(world_root: &Path, number: i64) -> PathBuf {
    world_root.join("Sessions").join(session_files::padded_number(number))
}

pub fn create_campaign_session(
    conn: &Connection,
    campaign_id: &str,
    session_number: Option<i64>,
    title: Option<&str>,
    date: Option<&str>,
) -> AppResult<CampaignSessionInfo> {
    let Some(campaign) = campaigns::get_campaign(conn, campaign_id)? else {
        return Err(AppError::NotFound(format!("Campaign not found: {campaign_id}")));
    };
    let root = campaigns::world_root_for_id(conn, campaign_id)?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;

    let number = match session_number {
        Some(n) => {
            if number_dir(&root, n).exists() {
                return Err(AppError::Conflict(format!(
                    "Session number already exists for campaign {campaign_id}: {n}"
                )));
            }
            n
        }
        None => {
            let mut n = campaign.next_session_number;
            while number_dir(&root, n).exists() {
                n += 1;
            }
            n
        }
    };

    let dir = number_dir(&root, number);
    std::fs::create_dir_all(session_files::audio_dir(&dir))
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create session dir: {e}")))?;
    let st = SessionToml {
        id: Some(Uuid::new_v4().to_string()),
        number: Some(number),
        title: title.filter(|s| !s.is_empty()).map(str::to_string),
        date: date.filter(|s| !s.is_empty()).map(str::to_string),
        language: campaign.default_language.clone(),
        ..Default::default()
    };
    session_files::write_session_toml_file(&dir, &st)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write session.toml: {e}")))?;
    Ok(campaign_session_info(&dir, &st))
}

/// Resolve the session dir for an upload, creating a bare (campaign-less)
/// session folder if the id is unknown. Returns `(session_id, session_path)`.
pub fn resolve_for_upload(
    conn: &Connection,
    session_id: Option<&str>,
) -> AppResult<(String, PathBuf)> {
    if let Some(sid) = session_id {
        if let Some(loc) = locate(conn, sid)? {
            return Ok((sid.to_string(), loc.dir));
        }
    }
    let sid = session_id
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let dir = output_root(conn)?.join("_sessions").join(&sid);
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create session dir: {e}")))?;
    let st = SessionToml { id: Some(sid.clone()), ..Default::default() };
    let _ = session_files::write_session_toml_file(&dir, &st);
    Ok((sid, dir))
}

pub fn set_tracks(conn: &Connection, session_id: &str, tracks: &Value) -> AppResult<()> {
    let loc = require(conn, session_id)?;
    let mut st = loc.st;
    // Rebuild the track list from the upload, carrying over speaker labels for
    // filenames that survive.
    let old = std::mem::take(&mut st.tracks);
    st.tracks = tracks
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let filename = t.get("filename").and_then(Value::as_str)?.to_string();
                    let prev = old.iter().find(|o| o.filename == filename);
                    Some(crate::session_files::TrackEntry {
                        filename,
                        speaker: prev.map(|p| p.speaker.clone()).unwrap_or_default(),
                        character: prev.map(|p| p.character.clone()).unwrap_or_default(),
                        pronouns: prev.map(|p| p.pronouns.clone()).unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    session_files::write_session_toml_file(&loc.dir, &st)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write session.toml: {e}")))
}

pub fn set_speakers(conn: &Connection, session_id: &str, speakers: &Value) -> AppResult<()> {
    let loc = require(conn, session_id)?;
    let mut st = loc.st;
    let by_track: std::collections::HashMap<&str, &Value> = speakers
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("track_id").and_then(Value::as_str).map(|t| (t, s)))
                .collect()
        })
        .unwrap_or_default();
    for t in &mut st.tracks {
        if let Some(s) = by_track.get(stem_of(&t.filename).as_str()) {
            let field = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            t.speaker = field("player_name");
            t.character = field("character_name");
            t.pronouns = field("pronouns");
        }
    }
    session_files::write_session_toml_file(&loc.dir, &st)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write session.toml: {e}")))
}

pub fn set_campaign_metadata(conn: &Connection, req: &SessionMetadataRequest) -> AppResult<Value> {
    let loc = require(conn, &req.session_id)?;
    let mut st = loc.st;
    let mut dir = loc.dir;

    // Resolve the target world: an explicit campaign assignment moves a bare
    // session into that world's Sessions/.
    let target_world = match &req.campaign_id {
        Some(cid) => {
            let root = campaigns::world_root_for_id(conn, cid)?
                .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {cid}")))?;
            Some(root)
        }
        None => loc.world.as_ref().map(|(root, _)| root.clone()),
    };

    let mut number = req.session_number.or(st.number);
    if number.is_none() {
        if let Some(cid) = &req.campaign_id {
            number = Some(campaigns::next_session_number(conn, Some(cid))?);
        }
    }

    // Move/renumber the folder when its canonical location changed.
    if let (Some(root), Some(n)) = (&target_world, number) {
        let want = number_dir(root, n);
        if want != dir {
            if want.exists() {
                return Err(AppError::Conflict(format!("Session number already exists: {n}")));
            }
            if let Some(parent) = want.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("create Sessions/: {e}")))?;
            }
            std::fs::rename(&dir, &want)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("move session dir: {e}")))?;
            dir = want;
        }
    }

    st.number = number;
    st.title = req.title.clone().filter(|s| !s.is_empty());
    st.date = req.date.clone().filter(|s| !s.is_empty());
    let metadata = normalize_metadata(req.metadata.as_ref().unwrap_or(&Value::Null));
    st.metadata = metadata_from_value(&metadata);
    st.notes = req.notes.clone().unwrap_or_default();
    session_files::write_session_toml_file(&dir, &st)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write session.toml: {e}")))?;

    let campaign_name = req
        .campaign_id
        .as_deref()
        .and_then(|c| campaigns::get_campaign(conn, c).ok().flatten())
        .map(|c| c.name);
    Ok(json!({
        "campaign_id": req.campaign_id,
        "campaign_name": campaign_name,
        "session_number": number,
        "title": req.title,
        "date": req.date,
        "notes": st.notes,
        "metadata": metadata,
    }))
}

/// Delete a session: its folder (audio included) moves to the OS trash.
pub fn delete_session(conn: &Connection, session_id: &str) -> AppResult<()> {
    let loc = require(conn, session_id)?;
    crate::paths::move_to_trash(&loc.dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("move session to trash: {e}")))?;
    Ok(())
}
