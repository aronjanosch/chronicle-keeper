//! Artifacts — files are truth. One `transcript.md` + one `summary.md` per
//! session folder; the old DB row history collapsed to "the current file".
//! Synthetic stable ids (transcript = 1, summary = 2, scoped to the session
//! routes) keep the HTTP contract intact. Provenance: transcript meta lives in
//! `session.toml [transcript]`, summary meta in summary.md frontmatter.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::models::ArtifactInfo;
use crate::session_files::{self, ArtifactMeta};
use crate::store::sessions;

pub const TRANSCRIPT_ID: i64 = 1;
pub const SUMMARY_ID: i64 = 2;

fn id_of(kind: &str) -> i64 {
    if kind == "summary" {
        SUMMARY_ID
    } else {
        TRANSCRIPT_ID
    }
}

fn kind_of(id: i64) -> Option<&'static str> {
    match id {
        TRANSCRIPT_ID => Some("transcript"),
        SUMMARY_ID => Some("summary"),
        _ => None,
    }
}

fn file_of(dir: &Path, kind: &str) -> PathBuf {
    if kind == "summary" {
        session_files::summary_md_path(dir)
    } else {
        session_files::transcript_md_path(dir)
    }
}

fn session_dir(conn: &Connection, session_id: &str) -> AppResult<PathBuf> {
    sessions::session_path_of(conn, session_id)?
        .map(PathBuf::from)
        .ok_or_else(|| AppError::NotFound(format!("Session not found: {session_id}")))
}

fn mtime_stamp(p: &Path) -> String {
    p.metadata()
        .and_then(|m| m.modified())
        .ok()
        .map(|t| {
            chrono::DateTime::<chrono::Local>::from(t)
                .naive_local()
                .format("%Y-%m-%dT%H:%M:%S%.6f")
                .to_string()
        })
        .unwrap_or_default()
}

// Provenance for one kind: transcript from session.toml, summary from
// summary.md frontmatter.
fn meta_for(dir: &Path, kind: &str) -> ArtifactMeta {
    if kind == "transcript" {
        return session_files::read_session_toml(dir)
            .ok()
            .flatten()
            .map(|st| st.transcript)
            .unwrap_or_default();
    }
    let Ok(raw) = std::fs::read_to_string(file_of(dir, kind)) else {
        return ArtifactMeta::default();
    };
    let (fm, _) = crate::vault::split_frontmatter(&raw);
    ArtifactMeta {
        provider: crate::vault::fm_get(&fm, "provider")
            .unwrap_or("")
            .to_string(),
        model: crate::vault::fm_get(&fm, "model").unwrap_or("").to_string(),
        generated_at: crate::vault::fm_get(&fm, "generated_at")
            .unwrap_or("")
            .to_string(),
    }
}

fn info_for(dir: &Path, session_id: &str, kind: &str) -> Option<ArtifactInfo> {
    let path = file_of(dir, kind);
    if !path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return None;
    }
    let meta = meta_for(dir, kind);
    Some(ArtifactInfo {
        id: id_of(kind),
        artifact_id: format!("{session_id}:{kind}"),
        session_id: session_id.to_string(),
        kind: kind.to_string(),
        provider: meta.provider,
        model: meta.model,
        created_at: if meta.generated_at.is_empty() {
            mtime_stamp(&path)
        } else {
            meta.generated_at
        },
    })
}

/// Write the artifact file (+ provenance). Returns the synthetic info.
pub fn insert_artifact(
    conn: &Connection,
    session_id: &str,
    kind: &str,
    provider: &str,
    model: &str,
    content: &str,
) -> AppResult<ArtifactInfo> {
    let dir = session_dir(conn, session_id)?;
    let generated_at = chrono::Local::now()
        .naive_local()
        .format("%Y-%m-%dT%H:%M:%S%.6f")
        .to_string();
    if kind == "summary" {
        let st = session_files::read_session_toml(&dir)
            .ok()
            .flatten()
            .unwrap_or_default();
        session_files::write_summary_md(
            &dir,
            content,
            st.number,
            st.date.as_deref(),
            st.title.as_deref(),
            provider,
            model,
            &generated_at,
        )
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write summary.md: {e}")))?;
    } else {
        session_files::write_transcript_md(&dir, content)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("write transcript.md: {e}")))?;
        if let Ok(Some(mut st)) = session_files::read_session_toml(&dir) {
            st.transcript = ArtifactMeta {
                provider: provider.to_string(),
                model: model.to_string(),
                generated_at: generated_at.clone(),
            };
            let _ = session_files::write_session_toml_file(&dir, &st);
        }
    }
    Ok(ArtifactInfo {
        id: id_of(kind),
        artifact_id: format!("{session_id}:{kind}"),
        session_id: session_id.to_string(),
        kind: kind.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        created_at: generated_at,
    })
}

/// Overwrite the summary body, preserving its frontmatter provenance. For
/// manual edits — the next re-summary still wins (it calls `insert_artifact`).
pub fn update_summary(
    conn: &Connection,
    session_id: &str,
    content: &str,
) -> AppResult<ArtifactInfo> {
    let dir = session_dir(conn, session_id)?;
    if !file_of(&dir, "summary")
        .metadata()
        .map(|m| m.len() > 0)
        .unwrap_or(false)
    {
        return Err(AppError::NotFound(format!(
            "No summary to edit: {session_id}"
        )));
    }
    let meta = meta_for(&dir, "summary");
    let st = session_files::read_session_toml(&dir)
        .ok()
        .flatten()
        .unwrap_or_default();
    session_files::write_summary_md(
        &dir,
        content,
        st.number,
        st.date.as_deref(),
        st.title.as_deref(),
        &meta.provider,
        &meta.model,
        &meta.generated_at,
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("write summary.md: {e}")))?;
    info_for(&dir, session_id, "summary")
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("summary vanished after write")))
}

/// 0 or 1 synthetic artifacts per kind (the file either exists or doesn't).
pub fn list_artifacts(
    conn: &Connection,
    session_id: &str,
    kind: Option<&str>,
) -> AppResult<Vec<ArtifactInfo>> {
    let dir = session_dir(conn, session_id)?;
    let kinds: &[&str] = match kind {
        Some(k) => {
            if k == "summary" {
                &["summary"]
            } else {
                &["transcript"]
            }
        }
        None => &["transcript", "summary"],
    };
    Ok(kinds
        .iter()
        .filter_map(|k| info_for(&dir, session_id, k))
        .collect())
}

pub fn get_artifact(
    conn: &Connection,
    session_id: &str,
    id: i64,
) -> AppResult<Option<ArtifactInfo>> {
    let Some(kind) = kind_of(id) else {
        return Ok(None);
    };
    let dir = session_dir(conn, session_id)?;
    Ok(info_for(&dir, session_id, kind))
}

/// Artifact text by synthetic id. Summary content is the body below the
/// frontmatter (the DB rows never carried frontmatter either).
pub fn get_content(conn: &Connection, session_id: &str, id: i64) -> AppResult<Option<String>> {
    match kind_of(id) {
        Some(kind) => content_of(&session_dir(conn, session_id)?, kind),
        None => Ok(None),
    }
}

fn content_of(dir: &Path, kind: &str) -> AppResult<Option<String>> {
    let raw = match std::fs::read_to_string(file_of(dir, kind)) {
        Ok(r) if !r.is_empty() => r,
        _ => return Ok(None),
    };
    if kind == "summary" {
        let (_, body) = crate::vault::split_frontmatter(&raw);
        Ok(Some(body.trim_end().to_string()))
    } else {
        Ok(Some(raw))
    }
}

/// Latest (= the one) artifact text of a kind.
pub fn latest_content(
    conn: &Connection,
    session_id: &str,
    kind: &str,
) -> AppResult<Option<String>> {
    content_of(&session_dir(conn, session_id)?, kind)
}

/// Delete an artifact: the file moves to the OS trash.
pub fn delete_artifact(conn: &Connection, session_id: &str, id: i64) -> AppResult<()> {
    let Some(kind) = kind_of(id) else {
        return Err(AppError::NotFound(format!("Artifact not found: {id}")));
    };
    let path = file_of(&session_dir(conn, session_id)?, kind);
    if !path.is_file() {
        return Err(AppError::NotFound(format!("Artifact not found: {id}")));
    }
    crate::paths::move_to_trash(&path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("move artifact to trash: {e}")))
}

pub fn has_kind(conn: &Connection, session_id: &str, kind: &str) -> AppResult<bool> {
    let dir = session_dir(conn, session_id)?;
    Ok(file_of(&dir, kind)
        .metadata()
        .map(|m| m.len() > 0)
        .unwrap_or(false))
}
