use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppResult;
use crate::models::ArtifactInfo;

pub fn insert_artifact(
    conn: &Connection,
    session_id: &str,
    kind: &str,
    provider: &str,
    model: &str,
    file_path: &str,
) -> AppResult<ArtifactInfo> {
    let created_at = Local::now().naive_local().format("%Y-%m-%dT%H:%M:%S%.6f").to_string();
    conn.execute(
        "INSERT INTO artifacts (session_id, kind, provider, model, file_path, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![session_id, kind, provider, model, file_path, created_at],
    )?;
    let id = conn.last_insert_rowid();
    Ok(ArtifactInfo {
        id,
        session_id: session_id.to_string(),
        kind: kind.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        file_path: file_path.to_string(),
        created_at,
    })
}

fn row_to_artifact(row: &rusqlite::Row) -> rusqlite::Result<ArtifactInfo> {
    Ok(ArtifactInfo {
        id: row.get("id")?,
        session_id: row.get("session_id")?,
        kind: row.get("kind")?,
        provider: row.get("provider")?,
        model: row.get("model")?,
        file_path: row.get("file_path")?,
        created_at: row.get("created_at")?,
    })
}

pub fn list_artifacts(conn: &Connection, session_id: &str, kind: Option<&str>) -> AppResult<Vec<ArtifactInfo>> {
    let mut out = Vec::new();
    match kind {
        Some(k) => {
            let mut stmt = conn.prepare(
                "SELECT * FROM artifacts WHERE session_id = ?1 AND kind = ?2 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![session_id, k], row_to_artifact)?;
            for r in rows {
                out.push(r?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT * FROM artifacts WHERE session_id = ?1 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![session_id], row_to_artifact)?;
            for r in rows {
                out.push(r?);
            }
        }
    }
    Ok(out)
}

pub fn get_artifact(conn: &Connection, id: i64) -> AppResult<Option<ArtifactInfo>> {
    let art = conn
        .query_row("SELECT * FROM artifacts WHERE id = ?1", params![id], row_to_artifact)
        .optional()?;
    Ok(art)
}

pub fn delete_artifact(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM artifacts WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn delete_artifacts_for_session(conn: &Connection, session_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM artifacts WHERE session_id = ?1", params![session_id])?;
    Ok(())
}

/// First (latest) artifact file path for a kind, if any.
pub fn latest_path(conn: &Connection, session_id: &str, kind: &str) -> AppResult<Option<String>> {
    Ok(list_artifacts(conn, session_id, Some(kind))?.into_iter().next().map(|a| a.file_path))
}

pub fn has_kind(conn: &Connection, session_id: &str, kind: &str) -> AppResult<bool> {
    Ok(latest_path(conn, session_id, kind)?.is_some())
}
