//! LLM provider registry + clients. Replaces the Python litellm layer with two
//! native transports: Ollama (`/api/chat`) and a generic OpenAI-compatible
//! `/chat/completions` client (covers openai/groq/deepseek/mistral/together/
//! perplexity/minimax + Gemini's OpenAI-compat endpoint). Anthropic's native
//! Messages API is a planned follow-up.

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::error::AppResult;

#[derive(Clone, Copy, PartialEq)]
pub enum Transport {
    Ollama,
    OpenAiCompat,
    /// Listed for parity but not yet wired (native client pending).
    Unsupported,
}

pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub needs_key: bool,
    pub default_api_base: Option<&'static str>,
    pub models: &'static [&'static str],
    pub default_model: &'static str,
    pub transport: Transport,
}

pub static REGISTRY: &[Provider] = &[
    Provider {
        id: "ollama",
        name: "Ollama (local)",
        needs_key: false,
        default_api_base: Some("http://localhost:11434"),
        models: &["llama3.3", "llama3.2", "llama3.1", "mistral", "mixtral", "gemma3", "gemma2", "phi4", "qwen3", "qwen2.5", "deepseek-r1", "command-r"],
        default_model: "llama3.2",
        transport: Transport::Ollama,
    },
    Provider {
        id: "openai",
        name: "OpenAI",
        needs_key: true,
        default_api_base: Some("https://api.openai.com/v1"),
        models: &["gpt-4.1", "gpt-4.1-mini", "gpt-4.1-nano", "gpt-4o", "gpt-4o-mini", "o3", "o3-mini", "o4-mini"],
        default_model: "gpt-4.1-mini",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "anthropic",
        name: "Anthropic",
        needs_key: true,
        default_api_base: None,
        models: &["claude-opus-4-7", "claude-sonnet-4-6", "claude-haiku-4-5-20251001"],
        default_model: "claude-sonnet-4-6",
        transport: Transport::Unsupported,
    },
    Provider {
        id: "gemini",
        name: "Google Gemini",
        needs_key: true,
        default_api_base: Some("https://generativelanguage.googleapis.com/v1beta/openai"),
        models: &["gemini-2.5-flash", "gemini-2.5-pro", "gemini-2.0-flash", "gemini-2.0-flash-lite"],
        default_model: "gemini-2.5-flash",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "minimax",
        name: "MiniMax",
        needs_key: true,
        default_api_base: Some("https://api.minimax.io/v1"),
        models: &["MiniMax-M1", "MiniMax-Text-01"],
        default_model: "MiniMax-M1",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "groq",
        name: "Groq",
        needs_key: true,
        default_api_base: Some("https://api.groq.com/openai/v1"),
        models: &["llama-3.3-70b-versatile", "llama-3.1-8b-instant", "gemma2-9b-it", "mixtral-8x7b-32768"],
        default_model: "llama-3.3-70b-versatile",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "mistral",
        name: "Mistral",
        needs_key: true,
        default_api_base: Some("https://api.mistral.ai/v1"),
        models: &["mistral-large-latest", "mistral-small-latest", "mistral-medium-latest", "codestral-latest"],
        default_model: "mistral-large-latest",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "deepseek",
        name: "DeepSeek",
        needs_key: true,
        default_api_base: Some("https://api.deepseek.com/v1"),
        models: &["deepseek-chat", "deepseek-reasoner"],
        default_model: "deepseek-chat",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "together",
        name: "Together AI",
        needs_key: true,
        default_api_base: Some("https://api.together.xyz/v1"),
        models: &["meta-llama/Llama-3.3-70B-Instruct-Turbo", "Qwen/Qwen2.5-72B-Instruct-Turbo", "mistralai/Mixtral-8x7B-Instruct-v0.1"],
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "perplexity",
        name: "Perplexity",
        needs_key: true,
        default_api_base: Some("https://api.perplexity.ai"),
        models: &["sonar-pro", "sonar", "sonar-reasoning-pro", "sonar-reasoning"],
        default_model: "sonar-pro",
        transport: Transport::OpenAiCompat,
    },
];

pub fn get(id: &str) -> Option<&'static Provider> {
    REGISTRY.iter().find(|p| p.id == id)
}

pub fn registry_ids() -> Vec<&'static str> {
    REGISTRY.iter().map(|p| p.id).collect()
}

// ---- provider_keys storage ----

#[derive(Default, Clone)]
pub struct SavedKey {
    pub api_key: String,
    pub api_base: String,
    pub default_model: String,
}

pub fn get_key(conn: &Connection, id: &str) -> AppResult<Option<SavedKey>> {
    let row = conn
        .query_row(
            "SELECT api_key, api_base, default_model FROM provider_keys WHERE provider_id = ?1",
            params![id],
            |r| Ok(SavedKey { api_key: r.get(0)?, api_base: r.get(1)?, default_model: r.get(2)? }),
        )
        .optional()?;
    Ok(row)
}

pub fn upsert_key(conn: &Connection, id: &str, api_key: &str, api_base: &str, default_model: &str) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO provider_keys (provider_id, api_key, api_base, default_model, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(provider_id) DO UPDATE SET \
            api_key = excluded.api_key, api_base = excluded.api_base, \
            default_model = excluded.default_model, updated_at = excluded.updated_at",
        params![id, api_key, api_base, default_model, now],
    )?;
    Ok(())
}

pub fn list_keys(conn: &Connection) -> AppResult<HashMap<String, SavedKey>> {
    let mut stmt = conn.prepare("SELECT provider_id, api_key, api_base, default_model FROM provider_keys")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, SavedKey { api_key: r.get(1)?, api_base: r.get(2)?, default_model: r.get(3)? }))
    })?;
    let mut map = HashMap::new();
    for r in rows {
        let (id, k) = r?;
        map.insert(id, k);
    }
    Ok(map)
}

// ---- chat client ----

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct LlmError(pub String);

/// One chat completion. Returns the assistant message text.
pub async fn chat(
    transport: Transport,
    api_base: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
    timeout_secs: u64,
    json_mode: bool,
) -> Result<String, LlmError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| LlmError(e.to_string()))?;

    match transport {
        Transport::Ollama => {
            let mut body = json!({
                "model": model,
                "messages": [{ "role": "user", "content": prompt }],
                "stream": false,
            });
            if json_mode {
                body["format"] = json!("json");
            }
            let url = format!("{}/api/chat", api_base.trim_end_matches('/'));
            let resp = client.post(url).json(&body).send().await.map_err(|e| LlmError(e.to_string()))?;
            let resp = error_for_status(resp).await?;
            let v: Value = resp.json().await.map_err(|e| LlmError(e.to_string()))?;
            Ok(v.get("message").and_then(|m| m.get("content")).and_then(Value::as_str).unwrap_or("").trim().to_string())
        }
        Transport::OpenAiCompat => {
            let mut body = json!({
                "model": model,
                "messages": [{ "role": "user", "content": prompt }],
            });
            if json_mode {
                body["response_format"] = json!({ "type": "json_object" });
            }
            let url = format!("{}/chat/completions", api_base.trim_end_matches('/'));
            let mut req = client.post(url).json(&body);
            if !api_key.is_empty() {
                req = req.bearer_auth(api_key);
            }
            let resp = req.send().await.map_err(|e| LlmError(e.to_string()))?;
            let resp = error_for_status(resp).await?;
            let v: Value = resp.json().await.map_err(|e| LlmError(e.to_string()))?;
            Ok(extract_openai_content(&v))
        }
        Transport::Unsupported => Err(LlmError(
            "This provider's native client is not yet available in this build.".into(),
        )),
    }
}

async fn error_for_status(resp: reqwest::Response) -> Result<reqwest::Response, LlmError> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(LlmError(format!("HTTP {status}: {}", body.trim())))
}

fn extract_openai_content(v: &Value) -> String {
    let Some(content) = v.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("message")).and_then(|m| m.get("content")) else {
        return String::new();
    };
    match content {
        Value::String(s) => s.trim().to_string(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string(),
        _ => String::new(),
    }
}
