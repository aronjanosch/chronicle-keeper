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

pub async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<Json<UploadResponse>> {
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
        return Err(AppError::BadRequest(
            "No audio files found in ZIP archive".into(),
        ));
    }

    state.with_db(|conn| sessions::set_tracks(conn, &sid, &tracks))?;

    Ok(Json(UploadResponse {
        session_id: sid,
        session_path: session_path.to_string_lossy().into_owned(),
        tracks,
    }))
}

/// Replace the session's audio with the zip's contents and return the new track
/// list. Extraction is staged in a sibling temp dir and only swapped in once it
/// succeeds and contains audio — a corrupt/empty ZIP or a mid-extract failure
/// leaves the existing recording untouched instead of half-deleting it.
fn extract_and_list(zip_bytes: &[u8], session_path: &Path) -> AppResult<Value> {
    let staging = session_path.with_extension("upload.tmp");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| AppError::Internal(e.into()))?;

    let staged = extract_to(zip_bytes, &staging).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&staging);
    })?;
    if staged.is_empty() {
        let _ = std::fs::remove_dir_all(&staging);
        return Ok(Value::Array(vec![])); // caller reports "no audio"; old audio intact
    }

    // Commit: drop the prior audio, then move the staged tree into place.
    if let Ok(entries) = std::fs::read_dir(session_path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if is_audio(&p) {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    move_tree(&staging, session_path)?;
    let _ = std::fs::remove_dir_all(&staging);

    let mut tracks: Vec<Value> = Vec::new();
    collect_audio(session_path, &mut tracks)?;
    tracks.sort_by(|a, b| {
        a["filename"]
            .as_str()
            .unwrap_or("")
            .cmp(b["filename"].as_str().unwrap_or(""))
    });
    Ok(Value::Array(tracks))
}

/// Extract every file into `dest` (zip-slip guarded). Returns the audio files found.
fn extract_to(zip_bytes: &[u8], dest: &Path) -> AppResult<Vec<PathBuf>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|_| AppError::BadRequest("Invalid ZIP file".into()))?;
    let mut audio: Vec<PathBuf> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::Internal(e.into()))?;
        if entry.is_dir() {
            continue;
        }
        // zip-slip guard: only keep the sanitized relative path.
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = safe_join(dest, &rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Internal(e.into()))?;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| AppError::Internal(e.into()))?;
        std::fs::write(&out, &buf).map_err(|e| AppError::Internal(e.into()))?;
        if is_audio(&out) {
            audio.push(out);
        }
    }
    Ok(audio)
}

/// Move every file under `src` into `dst` at the same relative path (overwriting).
fn move_tree(src: &Path, dst: &Path) -> AppResult<()> {
    for entry in std::fs::read_dir(src)
        .map_err(|e| AppError::Internal(e.into()))?
        .flatten()
    {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to).map_err(|e| AppError::Internal(e.into()))?;
            move_tree(&from, &to)?;
        } else {
            let _ = std::fs::remove_file(&to);
            if std::fs::rename(&from, &to).is_err() {
                std::fs::copy(&from, &to).map_err(|e| AppError::Internal(e.into()))?;
            }
        }
    }
    Ok(())
}

fn collect_audio(dir: &Path, out: &mut Vec<Value>) -> AppResult<()> {
    for entry in std::fs::read_dir(dir)
        .map_err(|e| AppError::Internal(e.into()))?
        .flatten()
    {
        let p = entry.path();
        if p.is_dir() {
            collect_audio(&p, out)?;
        } else if is_audio(&p) {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let filename = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
        buf
    }

    #[test]
    fn extract_swaps_atomically_and_preserves_on_failure() {
        let base = std::env::temp_dir().join(format!("ck_upload_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        // First upload lands one track.
        let tracks = extract_and_list(&make_zip(&[("a.flac", b"aaa")]), &base).unwrap();
        assert_eq!(tracks.as_array().unwrap().len(), 1);
        assert!(base.join("a.flac").exists());

        // A corrupt ZIP must not destroy the existing recording.
        assert!(extract_and_list(b"not a zip", &base).is_err());
        assert!(base.join("a.flac").exists());

        // A ZIP with no audio returns empty and leaves the old audio intact.
        let tracks = extract_and_list(&make_zip(&[("readme.txt", b"hi")]), &base).unwrap();
        assert!(tracks.as_array().unwrap().is_empty());
        assert!(base.join("a.flac").exists());

        // A valid ZIP replaces the prior audio wholesale.
        let tracks = extract_and_list(&make_zip(&[("b.flac", b"bbb")]), &base).unwrap();
        let arr = tracks.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["filename"], "b.flac");
        assert!(base.join("b.flac").exists());
        assert!(!base.join("a.flac").exists());

        let _ = std::fs::remove_dir_all(&base);
    }
}

pub async fn label_speakers(
    State(state): State<AppState>,
    Json(req): Json<LabelSpeakersRequest>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| sessions::set_speakers(conn, &req.session_id, &req.speakers))?;
    Ok(Json(
        json!({ "session_id": req.session_id, "speakers": req.speakers }),
    ))
}
