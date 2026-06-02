use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::paths::default_output_root;

/// Config keys persisted in the `config` table. Kept identical to the Python
/// backend so the existing frontend's settings screen works unchanged. Some
/// keys (`litellm_*`, `whisperx_model`) are legacy names retained for contract
/// compatibility.
fn default_config() -> Vec<(&'static str, String)> {
    vec![
        (
            "output_root",
            default_output_root().to_string_lossy().into_owned(),
        ),
        ("transcription_provider", "auto".into()),
        ("summary_provider", "ollama".into()),
        ("ollama_base_url", "http://localhost:11434".into()),
        ("ollama_model", "llama3.2".into()),
        ("ollama_timeout_seconds", "600".into()),
        // Ollama defaults num_ctx to 2048 and silently truncates longer prompts;
        // we auto-size the window per request and clamp it to this VRAM ceiling.
        ("ollama_num_ctx_max", "65536".into()),
        ("litellm_model", "gemini/gemini-2.5-flash".into()),
        ("litellm_api_key", "".into()),
        ("litellm_api_base", "".into()),
        ("litellm_timeout_seconds", "120".into()),
        ("default_language", "en".into()),
        ("whisperx_model", "nemo-parakeet-tdt-0.6b-v3".into()),
        ("transcription_accelerator", "auto".into()),
        ("transcription_timeout_seconds", "3600".into()),
        ("current_campaign_id", "".into()),
    ]
}

/// Native transcription provider id (replaces the old mlx-audio/onnx-asr split).
pub const NATIVE_TRANSCRIPTION_PROVIDER: &str = "sherpa";

/// Hardware backends accepted for `transcription_accelerator`. `auto` is the
/// default and picks the best provider for the host OS (see
/// [`resolve_accelerator`]); the explicit values are opt-in overrides and only
/// effective if the bundled onnxruntime was built with that execution provider
/// — otherwise the engine falls back to CPU at recognizer-create time (see
/// `transcription::mod`).
pub const ACCELERATORS: [&str; 5] = ["auto", "cpu", "coreml", "cuda", "directml"];

pub fn resolve_transcription_provider(pref: &str) -> String {
    match pref.trim() {
        "" | "auto" => NATIVE_TRANSCRIPTION_PROVIDER.to_string(),
        other => other.to_string(),
    }
}

/// Resolve `transcription_accelerator` into a concrete onnxruntime execution
/// provider. `auto` (and empty/unknown) maps to the best provider for the host
/// OS; explicit values pass through. The engine still degrades to CPU at
/// recognizer-create time if the linked onnxruntime lacks the chosen provider.
pub fn resolve_accelerator(pref: &str) -> &'static str {
    match pref.trim() {
        "cpu" => "cpu",
        "coreml" => "coreml",
        "cuda" => "cuda",
        "directml" => "directml",
        // "auto" + anything unexpected: pick per-OS. macOS keeps CPU — CoreML is
        // measurably slower than CPU for transducer ASR and ships int8-only.
        // Windows → DirectML (all DX12 GPUs). Linux → CUDA (NVIDIA). Each falls
        // back to CPU if that provider isn't in the linked onnxruntime.
        _ => {
            if cfg!(target_os = "windows") {
                "directml"
            } else if cfg!(target_os = "linux") {
                "cuda"
            } else {
                "cpu"
            }
        }
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
    pub transcription_accelerator: String,
    /// Hard cap (seconds) on a single transcription run before it's aborted.
    pub transcription_timeout_seconds: i64,
    pub has_litellm_key: bool,
    /// Multi-device sync server base URL (empty = sync disabled).
    pub sync_url: String,
    /// Whether a sync bearer token is saved (the token itself is never echoed).
    pub has_sync_token: bool,
    /// ISO timestamp of the last successful sync (local time), empty if never synced.
    pub last_sync_ts: String,
    /// Error message from the most recent failed sync attempt, empty if last sync succeeded.
    pub last_sync_error: String,
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
    pub transcription_accelerator: Option<String>,
    pub transcription_timeout_seconds: Option<i64>,
    pub sync_url: Option<String>,
    pub sync_token: Option<String>,
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

/// Read a single config value by key (None if unset/empty).
pub fn get_value(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let v: Option<String> = conn
        .query_row("SELECT value FROM config WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(v.filter(|s| !s.is_empty()))
}

/// Upsert a single config value by key.
pub fn set_value(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )?;
    Ok(())
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
        transcription_accelerator: {
            let a = get_str(map, "transcription_accelerator");
            if a.is_empty() {
                "cpu".into()
            } else {
                a
            }
        },
        transcription_timeout_seconds: {
            let t = get_int(map, "transcription_timeout_seconds");
            if t > 0 {
                t
            } else {
                3600
            }
        },
        has_litellm_key: !get_str(map, "litellm_api_key").is_empty(),
        sync_url: get_str(map, "sync_url"),
        has_sync_token: !get_str(map, "sync_token").is_empty(),
        last_sync_ts: get_str(map, "last_sync_ts"),
        last_sync_error: get_str(map, "last_sync_error"),
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
    if let Some(v) = &req.transcription_accelerator {
        let v = v.trim().to_lowercase();
        if !ACCELERATORS.contains(&v.as_str()) {
            return Err(AppError::BadRequest(format!(
                "transcription_accelerator must be one of: {}",
                ACCELERATORS.join(", ")
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
    set(
        "summary_provider",
        req.summary_provider
            .as_ref()
            .map(|s| s.trim().to_lowercase()),
    )?;
    set("ollama_base_url", req.ollama_base_url.clone())?;
    set("ollama_model", req.ollama_model.clone())?;
    set(
        "ollama_timeout_seconds",
        req.ollama_timeout_seconds.map(|n| n.to_string()),
    )?;
    set("litellm_model", req.litellm_model.clone())?;
    set("litellm_api_key", req.litellm_api_key.clone())?;
    set("litellm_api_base", req.litellm_api_base.clone())?;
    set(
        "litellm_timeout_seconds",
        req.litellm_timeout_seconds.map(|n| n.to_string()),
    )?;
    set("default_language", req.default_language.clone())?;
    set("whisperx_model", req.whisperx_model.clone())?;
    set(
        "transcription_provider",
        req.transcription_provider
            .as_ref()
            .map(|s| s.trim().to_lowercase()),
    )?;
    set(
        "transcription_accelerator",
        req.transcription_accelerator
            .as_ref()
            .map(|s| s.trim().to_lowercase()),
    )?;
    set(
        "transcription_timeout_seconds",
        req.transcription_timeout_seconds.map(|n| n.to_string()),
    )?;
    set(
        "sync_url",
        req.sync_url
            .as_ref()
            .map(|s| s.trim().trim_end_matches('/').to_string()),
    )?;
    set("sync_token", req.sync_token.clone())?;
    Ok(())
}
