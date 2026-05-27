use std::path::PathBuf;

use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{ExportRequest, ExportResponse};
use crate::state::AppState;
use crate::store::{artifacts, sessions};

pub fn export_session(state: &AppState, req: &ExportRequest) -> AppResult<ExportResponse> {
    let (session, summary_text) = state.with_db(|conn| -> AppResult<_> {
        let session = sessions::get_session_object(conn, &req.session_id)?;
        let summary_text = resolve_summary_text(conn, &req.session_id, req.summary_id)?;
        Ok((session, summary_text))
    })?;

    if summary_text.trim().is_empty() {
        return Err(AppError::BadRequest("No summary available for export.".into()));
    }

    let campaign = session.get("campaign").cloned().unwrap_or_default();
    let metadata = session.get("metadata").cloned().unwrap_or_default();
    let campaign_id = campaign.get("campaign_id").and_then(Value::as_str);
    let session_number = campaign.get("session_number").and_then(Value::as_i64);

    let content = if req.use_obsidian_format {
        let frontmatter = format_frontmatter(&[
            ("campaign", campaign.get("campaign_id").cloned()),
            ("session_number", campaign.get("session_number").cloned()),
            ("session_title", campaign.get("title").cloned()),
            ("session_date", campaign.get("date").cloned()),
            ("characters", metadata.get("characters").cloned()),
            ("locations", metadata.get("locations").cloned()),
            ("items", metadata.get("items").cloned()),
            ("tags", metadata.get("tags").cloned()),
        ]);
        format!("{frontmatter}\n\n{}\n", summary_text.trim())
    } else {
        format!("{}\n", summary_text.trim())
    };

    let filename = match &req.custom_filename {
        Some(f) if !f.is_empty() => sanitize_filename(f),
        _ => match (campaign_id, session_number) {
            (Some(cid), Some(num)) => format!("{cid}_session_{num}.md"),
            _ => "session_notes.md".to_string(),
        },
    };

    if let Some(path) = session.get("session_path").and_then(Value::as_str) {
        let export_dir = PathBuf::from(path).join("exports");
        if std::fs::create_dir_all(&export_dir).is_ok() {
            let _ = std::fs::write(export_dir.join(&filename), &content);
        }
    }

    Ok(ExportResponse {
        content,
        filename,
        use_obsidian_format: req.use_obsidian_format,
    })
}

fn resolve_summary_text(conn: &rusqlite::Connection, session_id: &str, summary_id: Option<i64>) -> AppResult<String> {
    if let Some(id) = summary_id {
        let art = artifacts::get_artifact(conn, id)?
            .filter(|a| a.session_id == session_id)
            .ok_or_else(|| AppError::BadRequest("Selected summary was not found for this session.".into()))?;
        if art.kind != "summary" {
            return Err(AppError::BadRequest("Selected artifact is not a summary.".into()));
        }
        return std::fs::read_to_string(&art.file_path)
            .map_err(|_| AppError::BadRequest("Selected summary file does not exist.".into()));
    }
    if let Some(path) = artifacts::latest_path(conn, session_id, "summary")? {
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Ok(text);
        }
    }
    Ok(String::new())
}

fn format_frontmatter(fields: &[(&str, Option<Value>)]) -> String {
    let mut lines = vec!["---".to_string()];
    for (key, value) in fields {
        let Some(value) = value else { continue };
        match value {
            Value::Null => continue,
            Value::String(s) if s.is_empty() => continue,
            Value::Array(items) => {
                if items.is_empty() {
                    continue;
                }
                lines.push(format!("{key}:"));
                for item in items {
                    lines.push(format!("  - {}", plain(item)));
                }
            }
            other => lines.push(format!("{key}: {}", plain(other))),
        }
    }
    lines.push("---".to_string());
    lines.join("\n")
}

fn plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn sanitize_filename(name: &str) -> String {
    name.replace(['/', '\\', ':'], "_")
}
