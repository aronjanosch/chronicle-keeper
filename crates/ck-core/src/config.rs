use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::paths::default_output_root;

/// Config keys persisted in the `config` table. Kept identical to the Python
/// backend so the existing frontend's settings screen works unchanged. Some
/// keys (`litellm_*`, `whisperx_model`) are legacy names retained for contract
/// compatibility.
fn default_config() -> Vec<(&'static str, String)> {
    vec![
        ("output_root", default_output_root().to_string_lossy().into_owned()),
        ("transcription_provider", "auto".into()),
        ("summary_provider", "ollama".into()),
        ("ollama_base_url", "http://localhost:11434".into()),
        ("ollama_model", "llama3.2".into()),
        ("ollama_timeout_seconds", "120".into()),
        ("litellm_model", "gemini/gemini-2.5-flash".into()),
        ("litellm_api_key", "".into()),
        ("litellm_api_base", "".into()),
        ("litellm_timeout_seconds", "120".into()),
        ("default_language", "en".into()),
        ("whisperx_model", "nemo-parakeet-tdt-0.6b-v3".into()),
        ("current_campaign_id", "".into()),
    ]
}

/// Native transcription provider id (replaces the old mlx-audio/onnx-asr split).
pub const NATIVE_TRANSCRIPTION_PROVIDER: &str = "sherpa";

pub fn resolve_transcription_provider(pref: &str) -> String {
    match pref.trim() {
        "" | "auto" => NATIVE_TRANSCRIPTION_PROVIDER.to_string(),
        other => other.to_string(),
    }
}

/// Response for GET/PUT /config — mirrors the Python `ConfigResponse`.
#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub output_root: String,
    pub summary_provider: String,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub ollama_timeout_seconds: i64,
    pub litellm_model: String,
    pub litellm_api_base: String,
    pub litellm_timeout_seconds: i64,
    pub default_language: String,
    pub whisperx_model: String,
    pub transcription_provider: String,
    pub transcription_provider_effective: String,
    pub has_litellm_key: bool,
}

/// Partial update payload — mirrors the Python `UpdateConfigRequest`.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateConfigRequest {
    pub output_root: Option<String>,
    pub summary_provider: Option<String>,
    pub ollama_base_url: Option<String>,
    pub ollama_model: Option<String>,
    pub ollama_timeout_seconds: Option<i64>,
    pub litellm_model: Option<String>,
    pub litellm_api_key: Option<String>,
    pub litellm_api_base: Option<String>,
    pub litellm_timeout_seconds: Option<i64>,
    pub default_language: Option<String>,
    pub whisperx_model: Option<String>,
    pub transcription_provider: Option<String>,
}

fn ensure_defaults(conn: &Connection) -> AppResult<()> {
    for (key, value) in default_config() {
        conn.execute(
            "INSERT OR IGNORE INTO config (key, value) VALUES (?1, ?2)",
            (key, value),
        )?;
    }
    Ok(())
}

pub fn get_config_map(conn: &Connection) -> AppResult<HashMap<String, String>> {
    ensure_defaults(conn)?;
    let mut stmt = conn.prepare("SELECT key, value FROM config")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (k, v) = row?;
        map.insert(k, v);
    }
    Ok(map)
}

fn get_str(map: &HashMap<String, String>, key: &str) -> String {
    map.get(key).cloned().unwrap_or_default()
}

fn get_int(map: &HashMap<String, String>, key: &str) -> i64 {
    map.get(key).and_then(|v| v.parse().ok()).unwrap_or(0)
}

pub fn to_response(map: &HashMap<String, String>) -> ConfigResponse {
    let pref = get_str(map, "transcription_provider");
    ConfigResponse {
        output_root: get_str(map, "output_root"),
        summary_provider: get_str(map, "summary_provider"),
        ollama_base_url: get_str(map, "ollama_base_url"),
        ollama_model: get_str(map, "ollama_model"),
        ollama_timeout_seconds: get_int(map, "ollama_timeout_seconds"),
        litellm_model: get_str(map, "litellm_model"),
        litellm_api_base: get_str(map, "litellm_api_base"),
        litellm_timeout_seconds: get_int(map, "litellm_timeout_seconds"),
        default_language: get_str(map, "default_language"),
        whisperx_model: get_str(map, "whisperx_model"),
        transcription_provider_effective: resolve_transcription_provider(&pref),
        transcription_provider: pref,
        has_litellm_key: !get_str(map, "litellm_api_key").is_empty(),
    }
}

pub fn apply_update(conn: &Connection, req: &UpdateConfigRequest) -> AppResult<()> {
    if let Some(v) = &req.transcription_provider {
        let v = v.trim().to_lowercase();
        if !matches!(v.as_str(), "auto" | NATIVE_TRANSCRIPTION_PROVIDER) {
            return Err(AppError::BadRequest(format!(
                "transcription_provider must be one of: auto, {NATIVE_TRANSCRIPTION_PROVIDER}"
            )));
        }
    }
    let set = |key: &str, val: Option<String>| -> AppResult<()> {
        if let Some(v) = val {
            conn.execute(
                "INSERT INTO config (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                (key, v),
            )?;
        }
        Ok(())
    };
    set("output_root", req.output_root.clone())?;
    set("summary_provider", req.summary_provider.as_ref().map(|s| s.trim().to_lowercase()))?;
    set("ollama_base_url", req.ollama_base_url.clone())?;
    set("ollama_model", req.ollama_model.clone())?;
    set("ollama_timeout_seconds", req.ollama_timeout_seconds.map(|n| n.to_string()))?;
    set("litellm_model", req.litellm_model.clone())?;
    set("litellm_api_key", req.litellm_api_key.clone())?;
    set("litellm_api_base", req.litellm_api_base.clone())?;
    set("litellm_timeout_seconds", req.litellm_timeout_seconds.map(|n| n.to_string()))?;
    set("default_language", req.default_language.clone())?;
    set("whisperx_model", req.whisperx_model.clone())?;
    set("transcription_provider", req.transcription_provider.as_ref().map(|s| s.trim().to_lowercase()))?;
    Ok(())
}
