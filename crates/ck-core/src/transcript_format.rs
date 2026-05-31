use serde_json::Value;

use crate::models::Segment;

/// Build a display label from a speaker mapping entry (mirrors the Python
/// `speaker_label`): "Character (Player)" / Character / Player / fallback.
pub fn speaker_label(speaker: Option<&Value>, fallback: &str) -> String {
    let Some(s) = speaker else {
        return fallback.to_string();
    };
    let character = s
        .get("character_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let player = s
        .get("player_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
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
