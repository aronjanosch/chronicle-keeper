use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use axum::extract::{Multipart, State};
use axum::Json;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::models::{LabelSpeakersRequest, UploadResponse};
use crate::state::AppState;
use crate::store::sessions;

const AUDIO_EXTS: [&str; 5] = ["flac", "wav", "mp3", "m4a", "ogg"];

pub async fn upload(State(state): State<AppState>, mut multipart: Multipart) -> AppResult<Json<UploadResponse>> {
    let mut zip_bytes: Option<Vec<u8>> = None;
    let mut session_id: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("malformed upload: {e}")))?
    {
        match field.name() {
            Some("file") => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("read upload: {e}")))?;
                zip_bytes = Some(data.to_vec());
            }
            Some("session_id") => {
                let v = field.text().await.unwrap_or_default();
                if !v.is_empty() {
                    session_id = Some(v);
                }
            }
            _ => {}
        }
    }

    let zip_bytes = zip_bytes.ok_or_else(|| AppError::BadRequest("No file uploaded".into()))?;

    let (sid, session_path) =
        state.with_db(|conn| sessions::resolve_for_upload(conn, session_id.as_deref()))?;

    let tracks = extract_and_list(&zip_bytes, &session_path)?;
    if tracks.as_array().map(|a| a.is_empty()).unwrap_or(true) {
        return Err(AppError::BadRequest("No audio files found in ZIP archive".into()));
    }

    state.with_db(|conn| sessions::set_tracks(conn, &sid, &tracks))?;

    Ok(Json(UploadResponse {
        session_id: sid,
        session_path: session_path.to_string_lossy().into_owned(),
        tracks,
    }))
}

/// Remove existing audio files, extract the zip, and return the new track list.
fn extract_and_list(zip_bytes: &[u8], session_path: &Path) -> AppResult<Value> {
    // Clear any prior audio so re-upload replaces rather than accumulates.
    if let Ok(entries) = std::fs::read_dir(session_path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if is_audio(&p) {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|_| AppError::BadRequest("Invalid ZIP file".into()))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| AppError::Internal(e.into()))?;
        if entry.is_dir() {
            continue;
        }
        // zip-slip guard: only keep the sanitized relative path.
        let Some(rel) = entry.enclosed_name() else { continue };
        let dest = safe_join(session_path, &rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Internal(e.into()))?;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| AppError::Internal(e.into()))?;
        std::fs::write(&dest, &buf).map_err(|e| AppError::Internal(e.into()))?;
    }

    let mut tracks: Vec<Value> = Vec::new();
    collect_audio(session_path, &mut tracks)?;
    tracks.sort_by(|a, b| {
        a["filename"].as_str().unwrap_or("").cmp(b["filename"].as_str().unwrap_or(""))
    });
    Ok(Value::Array(tracks))
}

fn collect_audio(dir: &Path, out: &mut Vec<Value>) -> AppResult<()> {
    for entry in std::fs::read_dir(dir).map_err(|e| AppError::Internal(e.into()))?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_audio(&p, out)?;
        } else if is_audio(&p) {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let filename = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
            out.push(json!({
                "id": stem,
                "filename": filename,
                "file_path": p.to_string_lossy(),
                "duration": Value::Null,
            }));
        }
    }
    Ok(())
}

fn is_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn safe_join(base: &Path, rel: &Path) -> PathBuf {
    let mut out = base.to_path_buf();
    for comp in rel.components() {
        if let std::path::Component::Normal(c) = comp {
            out.push(c);
        }
    }
    out
}

pub async fn label_speakers(
    State(state): State<AppState>,
    Json(req): Json<LabelSpeakersRequest>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| sessions::set_speakers(conn, &req.session_id, &req.speakers))?;
    Ok(Json(json!({ "session_id": req.session_id, "speakers": req.speakers })))
}
