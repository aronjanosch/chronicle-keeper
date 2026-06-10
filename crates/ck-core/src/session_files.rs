//! Session file I/O: write and sync the canonical 1.7 session files
//! (`session.toml`, `transcript.md`, `summary.md`) that sit alongside audio
//! inside `Sessions/<NNN>/`. These are TRUTH for vault worlds; SQLite is cache.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

// ── session.toml model ────────────────────────────────────────────

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl SessionMetadata {
    pub fn is_empty(&self) -> bool {
        self.characters.is_empty()
            && self.locations.is_empty()
            && self.events.is_empty()
            && self.items.is_empty()
            && self.tags.is_empty()
    }
}

/// Provenance of the generated transcript (summary's lives in summary.md
/// frontmatter; transcript.md stays raw text, so its meta sits here).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactMeta {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub generated_at: String,
}

impl ArtifactMeta {
    pub fn is_empty(&self) -> bool {
        self.provider.is_empty() && self.model.is_empty() && self.generated_at.is_empty()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackEntry {
    pub filename: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub speaker: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub character: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pronouns: String,
}

/// TRUTH for one session's metadata (vault worlds).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionToml {
    /// Stable session id (the HTTP `:id`). Back-filled by the cache scanner
    /// for files that predate the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// In-world date (Phase 11.5G): plots the session on the World timeline
    /// lane; same `year[-month[-day]] [ERA]` syntax as page `date:` frontmatter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_date_end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "SessionMetadata::is_empty")]
    pub metadata: SessionMetadata,
    #[serde(default, skip_serializing_if = "ArtifactMeta::is_empty")]
    pub transcript: ArtifactMeta,
    #[serde(default, rename = "track", skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<TrackEntry>,
}

impl SessionToml {
    /// Build from the DB-shaped `tracks_json` / `speakers_json` values.
    /// Track id = filename stem, so the speaker map folds into each track.
    pub fn from_json_parts(
        number: Option<i64>,
        title: Option<&str>,
        date: Option<&str>,
        language: &str,
        tracks: &Value,
        speakers: &Value,
    ) -> Self {
        let speaker_by_track: std::collections::HashMap<&str, &Value> = speakers
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.get("track_id").and_then(Value::as_str).map(|t| (t, s)))
                    .collect()
            })
            .unwrap_or_default();
        let tracks = tracks
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|track| {
                        let filename = track.get("filename").and_then(Value::as_str)?;
                        let track_id = track.get("id").and_then(Value::as_str).unwrap_or(filename);
                        let s = speaker_by_track.get(track_id);
                        let field = |k: &str| {
                            s.and_then(|s| s.get(k))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string()
                        };
                        Some(TrackEntry {
                            filename: filename.to_string(),
                            speaker: field("player_name"),
                            character: field("character_name"),
                            pronouns: field("pronouns"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        SessionToml {
            number,
            date: date.filter(|s| !s.is_empty()).map(str::to_string),
            title: title.filter(|s| !s.is_empty()).map(str::to_string),
            language: language.to_string(),
            tracks,
            ..Default::default()
        }
    }
}

/// `Ok(None)` when no session.toml exists.
pub fn read_session_toml(session_path: &Path) -> AppResult<Option<SessionToml>> {
    let path = session_toml_path(session_path);
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Internal(anyhow::anyhow!("read {}: {e}", path.display()))),
    };
    let parsed = toml::from_str(&raw)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("parse {}: {e}", path.display())))?;
    Ok(Some(parsed))
}

pub fn write_session_toml_file(session_path: &Path, st: &SessionToml) -> std::io::Result<()> {
    let body = toml::to_string_pretty(st)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(session_toml_path(session_path), body.as_bytes())
}

// ── Path helpers ──────────────────────────────────────────────────

pub fn audio_dir(session_path: &Path) -> PathBuf {
    session_path.join("audio")
}

pub fn session_toml_path(session_path: &Path) -> PathBuf {
    session_path.join("session.toml")
}

pub fn transcript_md_path(session_path: &Path) -> PathBuf {
    session_path.join("transcript.md")
}

pub fn summary_md_path(session_path: &Path) -> PathBuf {
    session_path.join("summary.md")
}

/// True when the session_path is in the 1.7 vault layout (parent named "Sessions").
/// Used to gate file writes so non-vault / old-layout sessions are left alone.
pub fn is_vault_session_path(session_path: &str) -> bool {
    Path::new(session_path)
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n == "Sessions")
        .unwrap_or(false)
}

// ── Writers ───────────────────────────────────────────────────────

/// Write (or overwrite) `session.toml` with the canonical metadata + speaker
/// map, preserving existing `metadata`/`notes` (read-modify-write — they are
/// not part of the DB-shaped inputs).
pub fn write_session_toml(
    session_path: &Path,
    number: Option<i64>,
    title: Option<&str>,
    date: Option<&str>,
    language: &str,
    tracks: &Value,
    speakers: &Value,
) -> std::io::Result<()> {
    let mut st = SessionToml::from_json_parts(number, title, date, language, tracks, speakers);
    if let Ok(Some(existing)) = read_session_toml(session_path) {
        st.id = existing.id;
        st.metadata = existing.metadata;
        st.notes = existing.notes;
        st.transcript = existing.transcript;
        st.world_date = existing.world_date;
        st.world_date_end = existing.world_date_end;
    }
    write_session_toml_file(session_path, &st)
}

pub fn write_transcript_md(session_path: &Path, text: &str) -> std::io::Result<()> {
    std::fs::write(transcript_md_path(session_path), text.as_bytes())
}

/// Write `summary.md` with Obsidian-compatible frontmatter.
pub fn write_summary_md(
    session_path: &Path,
    text: &str,
    number: Option<i64>,
    date: Option<&str>,
    title: Option<&str>,
    provider: &str,
    model: &str,
    generated_at: &str,
) -> std::io::Result<()> {
    let mut content = String::from("---\n");
    if let Some(n) = number {
        content.push_str(&format!("session: {n}\n"));
    }
    if let Some(d) = date.filter(|s| !s.is_empty()) {
        content.push_str(&format!("date: \"{}\"\n", yaml_escape(d)));
    }
    if let Some(t) = title.filter(|s| !s.is_empty()) {
        content.push_str(&format!("title: \"{}\"\n", yaml_escape(t)));
    }
    content.push_str(&format!("provider: {provider}\nmodel: {model}\ngenerated_at: {generated_at}\n---\n\n"));
    content.push_str(text.trim());
    content.push('\n');
    std::fs::write(summary_md_path(session_path), content.as_bytes())
}

// ── Migration helpers ─────────────────────────────────────────────

/// Copy audio from `src` into `dst/` and verify each copy by size. Copy-only,
/// never deletes. A missing `src` is Ok(0) — originals may be long gone while
/// transcript/summary still exist.
pub fn copy_audio_files(src: &Path, dst: &Path) -> std::io::Result<usize> {
    if !src.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(dst)?;
    let mut copied: Vec<(PathBuf, PathBuf)> = Vec::new();
    collect_and_copy_audio(src, dst, &mut copied)?;
    for (from, to) in &copied {
        if from.metadata()?.len() != to.metadata()?.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("size mismatch after copy: {}", to.display()),
            ));
        }
    }
    Ok(copied.len())
}

fn collect_and_copy_audio(
    src: &Path,
    dst: &Path,
    copied: &mut Vec<(PathBuf, PathBuf)>,
) -> std::io::Result<()> {
    const AUDIO_EXTS: &[&str] = &["flac", "wav", "mp3", "m4a", "ogg"];
    for entry in std::fs::read_dir(src)?.flatten() {
        let from = entry.path();
        if from.is_dir() {
            let sub = dst.join(entry.file_name());
            std::fs::create_dir_all(&sub)?;
            collect_and_copy_audio(&from, &sub, copied)?;
        } else if from
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| AUDIO_EXTS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false)
        {
            let to = dst.join(entry.file_name());
            std::fs::copy(&from, &to)?;
            copied.push((from, to));
        }
    }
    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────

fn yaml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Zero-pad a session number to 3 digits (expands for 4+ digit numbers).
pub fn padded_number(n: i64) -> String {
    format!("{n:03}")
}

/// Compute the vault session path: `<world_root>/Sessions/<NNN>/`.
/// `vault_codex_path` is the campaign's `vault_path` column (= `<world_root>/Codex`).
pub fn vault_session_path(vault_codex_path: &str, number: i64) -> Option<PathBuf> {
    let world_root = Path::new(vault_codex_path).parent()?;
    Some(world_root.join("Sessions").join(padded_number(number)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_vault_session_path_works() {
        assert!(is_vault_session_path("/home/aron/Chronicle Keeper/Ashfall/Sessions/001"));
        assert!(is_vault_session_path("/home/aron/Chronicle Keeper/Ashfall/Sessions/42"));
        assert!(!is_vault_session_path("/home/aron/ck/ashfall/1"));
        assert!(!is_vault_session_path("/home/aron/Sessions-backup/ashfall/1")); // parent not exactly "Sessions"
    }

    #[test]
    fn vault_session_path_rounds_correctly() {
        let p = vault_session_path("/home/aron/Ashfall/Codex", 1).unwrap();
        assert!(p.ends_with("Sessions/001"));
        let p2 = vault_session_path("/home/aron/Ashfall/Codex", 42).unwrap();
        assert!(p2.ends_with("Sessions/042"));
        let p3 = vault_session_path("/home/aron/Ashfall/Codex", 1000).unwrap();
        assert!(p3.ends_with("Sessions/1000"));
    }

    #[test]
    fn write_session_toml_format() {
        let dir = std::env::temp_dir().join(format!("ck-sf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tracks = serde_json::json!([{"id": "track-aria", "filename": "track-aria.flac"}]);
        let speakers = serde_json::json!([{"track_id": "track-aria", "player_name": "Aron", "character_name": "Lyra", "pronouns": "she/her"}]);
        write_session_toml(&dir, Some(1), Some("The Iron Crown"), Some("2025-01-15"), "en", &tracks, &speakers).unwrap();
        let content = std::fs::read_to_string(session_toml_path(&dir)).unwrap();
        assert!(content.contains("number = 1"));
        assert!(content.contains("title = \"The Iron Crown\""));
        assert!(content.contains("filename = \"track-aria.flac\""));
        assert!(content.contains("speaker = \"Aron\""));
        assert!(content.contains("character = \"Lyra\""));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn session_toml_roundtrip_preserves_metadata_and_notes() {
        let dir = std::env::temp_dir().join(format!("ck-sf-rt-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let st = SessionToml {
            id: Some("sess-7".into()),
            number: Some(7),
            date: Some("2026-06-03".into()),
            world_date: Some("1374-08 DR".into()),
            world_date_end: None,
            title: Some("Tomb of \"Aldric\"".into()),
            language: "de".into(),
            notes: "line one\nline two\n".into(),
            metadata: SessionMetadata {
                characters: vec!["Lyra".into()],
                tags: vec!["Kampf".into(), "Mysterium".into()],
                ..Default::default()
            },
            transcript: ArtifactMeta::default(),
            tracks: vec![TrackEntry {
                filename: "track-aria.flac".into(),
                speaker: "Aron".into(),
                character: "Lyra".into(),
                pronouns: "she/her".into(),
            }],
        };
        write_session_toml_file(&dir, &st).unwrap();
        let back = read_session_toml(&dir).unwrap().unwrap();
        assert_eq!(back, st);

        // DB-shaped rewrite keeps metadata + notes (read-modify-write).
        let tracks = serde_json::json!([{"id": "track-aria", "filename": "track-aria.flac"}]);
        write_session_toml(&dir, Some(7), Some("Tomb"), None, "de", &tracks, &serde_json::json!([])).unwrap();
        let back2 = read_session_toml(&dir).unwrap().unwrap();
        assert_eq!(back2.notes, st.notes);
        assert_eq!(back2.metadata, st.metadata);
        assert_eq!(back2.tracks[0].speaker, ""); // speakers cleared by empty map
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_session_toml_missing_is_none() {
        let dir = std::env::temp_dir().join(format!("ck-sf-none-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read_session_toml(&dir).unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
