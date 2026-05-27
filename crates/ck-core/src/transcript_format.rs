use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::Local;
use serde_json::{json, Value};

use crate::models::Segment;
use crate::normalize::sanitize_folder_name;

/// Build a display label from a speaker mapping entry (mirrors the Python
/// `speaker_label`): "Character (Player)" / Character / Player / fallback.
pub fn speaker_label(speaker: Option<&Value>, fallback: &str) -> String {
    let Some(s) = speaker else { return fallback.to_string() };
    let character = s.get("character_name").and_then(Value::as_str).unwrap_or("").trim();
    let player = s.get("player_name").and_then(Value::as_str).unwrap_or("").trim();
    match (character.is_empty(), player.is_empty()) {
        (false, false) => format!("{character} ({player})"),
        (false, true) => character.to_string(),
        (true, false) => player.to_string(),
        (true, true) => fallback.to_string(),
    }
}

/// Group segments into speaker-blocked plain text (mirrors `segments_to_plain_text`).
pub fn segments_to_plain_text(segments: &[Segment]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current: Option<&str> = None;
    for seg in segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        let speaker = seg.speaker.as_deref();
        if speaker != current {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            if let Some(sp) = speaker {
                lines.push(format!("[{sp}]"));
            }
            current = speaker;
        }
        lines.push(text.to_string());
    }
    lines.join("\n")
}

/// Persist transcription to `transcriptions/{provider}_{model}/` as JSON + txt.
/// Returns `(json_path, text_path)`.
pub fn write_transcription(
    session_path: &Path,
    provider: &str,
    provider_model: &str,
    language: &str,
    segments: &[Segment],
) -> Result<(String, String)> {
    let subfolder = format!("{provider}_{}", sanitize_folder_name(provider_model));
    let dir: PathBuf = session_path.join("transcriptions").join(subfolder);
    std::fs::create_dir_all(&dir)?;

    let segments_json: Vec<Value> = segments
        .iter()
        .map(|s| {
            json!({
                "text": s.text,
                "start": s.start,
                "end": s.end,
                "speaker": s.speaker,
                "source": s.source,
                "words": s.words,
            })
        })
        .collect();

    let data = json!({
        "segments": segments_json,
        "language": language,
        "provider": provider,
        "metadata": {
            "provider_model": provider_model,
            "transcribed_at": Local::now().naive_local().format("%Y-%m-%dT%H:%M:%S%.6f").to_string(),
            "session_path": session_path.to_string_lossy(),
        }
    });

    let json_path = dir.join("transcription.json");
    std::fs::write(&json_path, serde_json::to_vec_pretty(&data)?)?;

    let text_path = dir.join("transcript.txt");
    std::fs::write(&text_path, segments_to_plain_text(segments))?;

    Ok((json_path.to_string_lossy().into_owned(), text_path.to_string_lossy().into_owned()))
}
