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
        return Err(AppError::BadRequest(
            "No summary available for export.".into(),
        ));
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

    // Write the note into the session's own folder (next to its audio), so it
    // lands in the user-visible data folder ready for Obsidian. SQLite stays the
    // source of truth; this is just a convenience output the user asked to keep.
    let session_path = session
        .get("session_path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut path = None;
    if !session_path.is_empty() {
        let dir = std::path::Path::new(session_path);
        std::fs::create_dir_all(dir)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create export dir: {e}")))?;
        let full = dir.join(&filename);
        std::fs::write(&full, content.as_bytes())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("write export: {e}")))?;
        path = Some(full.to_string_lossy().into_owned());
    }

    Ok(ExportResponse {
        content,
        filename,
        path,
        use_obsidian_format: req.use_obsidian_format,
    })
}

/// Zip the whole world folder — files are truth, so export is honesty, not
/// transformation. `.ck/index.db*` (rebuildable cache) is excluded;
/// `.ck/config.toml` and everything else ships. `include_audio: false` drops
/// `Sessions/*/audio/` (often GBs).
pub fn export_world(world_root: &std::path::Path, include_audio: bool) -> AppResult<String> {
    use std::io::Write;

    let name = world_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("world");
    let out_path = world_root
        .parent()
        .unwrap_or(world_root)
        .join(format!("{}.zip", sanitize_filename(name)));

    let mut files = Vec::new();
    collect_files(world_root, &mut files);

    let file = std::fs::File::create(&out_path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create zip: {e}")))?;
    let mut w = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);

    for abs in files {
        let rel = abs
            .strip_prefix(world_root)
            .unwrap_or(&abs)
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with(".ck/index.db") {
            continue;
        }
        if !include_audio && is_session_audio(&rel) {
            continue;
        }
        let bytes = std::fs::read(&abs)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("read {rel}: {e}")))?;
        w.start_file(format!("{name}/{rel}"), opts)
            .and_then(|()| w.write_all(&bytes).map_err(Into::into))
            .map_err(|e| AppError::Internal(anyhow::anyhow!("zip {rel}: {e}")))?;
    }
    w.finish()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("finish zip: {e}")))?;
    Ok(out_path.to_string_lossy().into_owned())
}

fn is_session_audio(rel: &str) -> bool {
    let mut parts = rel.split('/');
    parts.next() == Some("Sessions") && parts.next().is_some() && parts.next() == Some("audio")
}

fn collect_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn resolve_summary_text(
    conn: &rusqlite::Connection,
    session_id: &str,
    summary_id: Option<i64>,
) -> AppResult<String> {
    if let Some(id) = summary_id {
        let art = artifacts::get_artifact(conn, session_id, id)?.ok_or_else(|| {
            AppError::BadRequest("Selected summary was not found for this session.".into())
        })?;
        if art.kind != "summary" {
            return Err(AppError::BadRequest(
                "Selected artifact is not a summary.".into(),
            ));
        }
        return artifacts::get_content(conn, session_id, art.id)?.ok_or_else(|| {
            AppError::BadRequest("Selected summary was not found for this session.".into())
        });
    }
    Ok(artifacts::latest_content(conn, session_id, "summary")?.unwrap_or_default())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_world_excludes_cache_and_optionally_audio() {
        let dir = std::env::temp_dir().join(format!("ck-export-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let world = dir.join("My World");
        std::fs::create_dir_all(world.join(".ck")).unwrap();
        std::fs::create_dir_all(world.join("Codex")).unwrap();
        std::fs::create_dir_all(world.join("Sessions/001/audio")).unwrap();
        std::fs::write(world.join(".ck/config.toml"), "id = \"w\"").unwrap();
        std::fs::write(world.join(".ck/index.db"), "cache").unwrap();
        std::fs::write(world.join("Codex/Page.md"), "hello").unwrap();
        std::fs::write(world.join("Sessions/001/session.toml"), "n = 1").unwrap();
        std::fs::write(world.join("Sessions/001/audio/track.flac"), "audio").unwrap();

        let path = export_world(&world, false).unwrap();
        let names = zip_names(&path);
        assert!(names.contains(&"My World/.ck/config.toml".to_string()));
        assert!(names.contains(&"My World/Codex/Page.md".to_string()));
        assert!(names.contains(&"My World/Sessions/001/session.toml".to_string()));
        assert!(!names.iter().any(|n| n.contains("index.db")));
        assert!(!names.iter().any(|n| n.contains("/audio/")));

        let path = export_world(&world, true).unwrap();
        assert!(zip_names(&path).contains(&"My World/Sessions/001/audio/track.flac".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    fn zip_names(path: &str) -> Vec<String> {
        let f = std::fs::File::open(path).unwrap();
        let mut z = zip::ZipArchive::new(f).unwrap();
        (0..z.len())
            .map(|i| z.by_index(i).unwrap().name().to_string())
            .collect()
    }
}
