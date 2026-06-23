//! LLM provider registry + clients. Replaces the Python litellm layer with
//! native transports: Ollama (`/api/chat`), a generic OpenAI-compatible
//! `/chat/completions` client (covers openai/groq/deepseek/mistral/together/
//! perplexity/minimax + Gemini's OpenAI-compat endpoint), and Anthropic's
//! native Messages API (`/v1/messages`). Cohere is still a follow-up.

pub mod agent;

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use futures_util::StreamExt;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Transport {
    Ollama,
    OpenAiCompat,
    /// Anthropic native Messages API (`/v1/messages`).
    Anthropic,
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
        models: &[
            "llama3.3",
            "llama3.2",
            "llama3.1",
            "mistral",
            "mixtral",
            "gemma3",
            "gemma2",
            "phi4",
            "qwen3",
            "qwen2.5",
            "deepseek-r1",
            "command-r",
        ],
        default_model: "llama3.2",
        transport: Transport::Ollama,
    },
    Provider {
        id: "ollama-cloud",
        name: "Ollama Cloud",
        needs_key: true,
        default_api_base: Some("https://ollama.com"),
        // No baked-in suggestions: the cloud catalogue changes often, so the
        // model is a free-text field (type the exact id from ollama.com).
        models: &[],
        default_model: "",
        transport: Transport::Ollama,
    },
    Provider {
        id: "openai",
        name: "OpenAI",
        needs_key: true,
        default_api_base: Some("https://api.openai.com/v1"),
        models: &[
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "gpt-4o",
            "gpt-4o-mini",
            "o3",
            "o3-mini",
            "o4-mini",
        ],
        default_model: "gpt-4.1-mini",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "anthropic",
        name: "Anthropic",
        needs_key: true,
        default_api_base: Some("https://api.anthropic.com"),
        models: &[
            "claude-opus-4-8",
            "claude-sonnet-4-6",
            "claude-haiku-4-5-20251001",
        ],
        default_model: "claude-sonnet-4-6",
        transport: Transport::Anthropic,
    },
    Provider {
        id: "openrouter",
        name: "OpenRouter",
        needs_key: true,
        default_api_base: Some("https://openrouter.ai/api/v1"),
        // OpenRouter proxies the entire vendor-prefixed catalogue and adds new
        // models constantly, so no baked-in list: type any id from
        // openrouter.ai/models (free-text field).
        models: &[],
        default_model: "",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "gemini",
        name: "Google Gemini",
        needs_key: true,
        default_api_base: Some("https://generativelanguage.googleapis.com/v1beta/openai"),
        models: &[
            "gemini-2.5-flash",
            "gemini-2.5-pro",
            "gemini-2.0-flash",
            "gemini-2.0-flash-lite",
        ],
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
        models: &[
            "llama-3.3-70b-versatile",
            "llama-3.1-8b-instant",
            "gemma2-9b-it",
            "mixtral-8x7b-32768",
        ],
        default_model: "llama-3.3-70b-versatile",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "mistral",
        name: "Mistral",
        needs_key: true,
        default_api_base: Some("https://api.mistral.ai/v1"),
        models: &[
            "mistral-large-latest",
            "mistral-small-latest",
            "mistral-medium-latest",
            "codestral-latest",
        ],
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
        models: &[
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "Qwen/Qwen2.5-72B-Instruct-Turbo",
            "mistralai/Mixtral-8x7B-Instruct-v0.1",
        ],
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        transport: Transport::OpenAiCompat,
    },
    Provider {
        id: "perplexity",
        name: "Perplexity",
        needs_key: true,
        default_api_base: Some("https://api.perplexity.ai"),
        models: &[
            "sonar-pro",
            "sonar",
            "sonar-reasoning-pro",
            "sonar-reasoning",
        ],
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
            |r| {
                Ok(SavedKey {
                    api_key: r.get(0)?,
                    api_base: r.get(1)?,
                    default_model: r.get(2)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

pub fn upsert_key(
    conn: &Connection,
    id: &str,
    api_key: &str,
    api_base: &str,
    default_model: &str,
) -> AppResult<()> {
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
    let mut stmt =
        conn.prepare("SELECT provider_id, api_key, api_base, default_model FROM provider_keys")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            SavedKey {
                api_key: r.get(1)?,
                api_base: r.get(2)?,
                default_model: r.get(3)?,
            },
        ))
    })?;
    let mut map = HashMap::new();
    for r in rows {
        let (id, k) = r?;
        map.insert(id, k);
    }
    Ok(map)
}

// ---- provider resolution ----

/// A fully-resolved LLM target for one generation call.
pub struct Resolved {
    pub provider: String,
    pub transport: Transport,
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub timeout: u64,
    pub needs_key: bool,
    pub num_ctx_max: Option<u32>,
}

/// Resolve provider/model/base/timeout for a call, layering per-request overrides
/// over the saved provider key over config defaults. Shared by summarization,
/// recap, and codex import so they all pick the same target the same way.
pub fn resolve(
    conn: &Connection,
    cfg: &HashMap<String, String>,
    provider_override: Option<&str>,
    model_override: Option<&str>,
    base_override: Option<&str>,
) -> AppResult<Resolved> {
    let provider = provider_override
        .map(str::to_string)
        .unwrap_or_else(|| {
            cfg.get("summary_provider")
                .cloned()
                .unwrap_or_else(|| "ollama".into())
        })
        .to_lowercase();
    let p = get(&provider)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown provider: {provider}")))?;
    let saved = get_key(conn, &provider)?.unwrap_or_default();

    let api_key = saved.api_key.clone();
    if p.needs_key && api_key.is_empty() {
        return Err(AppError::BadRequest(format!(
            "No API key saved for {}. Add it in Settings → LLM providers.",
            p.name
        )));
    }

    let api_base = base_override
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| Some(saved.api_base.clone()).filter(|s| !s.is_empty()))
        // Legacy config key points at the local daemon — it must not hijack
        // other Ollama-transport providers (ollama-cloud has its own base).
        .or_else(|| {
            (p.id == "ollama")
                .then(|| cfg.get("ollama_base_url").cloned())
                .flatten()
        })
        .or_else(|| p.default_api_base.map(str::to_string))
        .unwrap_or_default();

    let model = model_override
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| Some(saved.default_model.clone()).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| p.default_model.to_string());

    let timeout_key = if p.transport == Transport::Ollama {
        "ollama_timeout_seconds"
    } else {
        "litellm_timeout_seconds"
    };
    let timeout = cfg
        .get(timeout_key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);

    let num_ctx_max = (p.transport == Transport::Ollama)
        .then(|| cfg.get("ollama_num_ctx_max").and_then(|s| s.parse().ok()))
        .flatten();

    Ok(Resolved {
        provider,
        transport: p.transport,
        api_base,
        api_key,
        model,
        timeout,
        needs_key: p.needs_key,
        num_ctx_max,
    })
}

/// Rough token estimate. ~3 chars/token is conservative for German + lots of
/// proper names (English averages ~4); undershooting here silently truncates.
fn approx_tokens(chars: usize) -> usize {
    chars / 3
}

/// Size the Ollama context window to the prompt: enough to hold the whole
/// prompt plus room to generate, rounded up to a common bucket, clamped to the
/// caller's memory ceiling. Below 2048 Ollama silently truncates long prompts.
fn fit_num_ctx(prompt_chars: usize, max: u32) -> u32 {
    let needed = approx_tokens(prompt_chars) as u32 + 2048;
    for bucket in [4096u32, 8192, 16384, 32768, 65536, 131072] {
        if bucket >= needed {
            return bucket.min(max);
        }
    }
    max
}

// ---- chat client ----

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct LlmError(pub String);

pub struct ChatRequest<'a> {
    pub transport: Transport,
    pub api_base: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub prompt: &'a str,
    pub timeout_secs: u64,
    pub num_ctx_max: Option<u32>,
}

/// One chat completion. Returns the assistant message text.
pub async fn chat(req: &ChatRequest<'_>, json_mode: bool) -> Result<String, LlmError> {
    let ChatRequest {
        transport,
        api_base,
        api_key,
        model,
        prompt,
        timeout_secs,
        num_ctx_max,
    } = req;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(*timeout_secs))
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
            // num_ctx is a local-inference parameter — cloud-backed Ollama models
            // (e.g. "gemma4:31b-cloud") proxy to an upstream API that ignores or
            // rejects it, so skip for any model whose name contains "cloud".
            let is_local = !model.to_lowercase().contains("cloud");
            let num_ctx = if is_local {
                num_ctx_max.map(|max| fit_num_ctx(prompt.len(), max))
            } else {
                None
            };
            if let Some(n) = num_ctx {
                body["options"] = json!({ "num_ctx": n });
            }
            tracing::info!(
                model,
                prompt_chars = prompt.len(),
                approx_prompt_tokens = approx_tokens(prompt.len()),
                num_ctx,
                num_ctx_max,
                json_mode,
                "ollama chat request"
            );
            let url = format!("{}/api/chat", api_base.trim_end_matches('/'));
            // Local Ollama needs no auth (empty key → no header); Ollama Cloud
            // (ollama.com) authenticates with a Bearer key.
            let mut req = client.post(url).json(&body);
            if !api_key.is_empty() {
                req = req.bearer_auth(api_key);
            }
            let resp = req.send().await.map_err(|e| LlmError(e.to_string()))?;
            let resp = error_for_status(resp).await?;
            let v: Value = resp.json().await.map_err(|e| LlmError(e.to_string()))?;
            let prompt_eval_count = v.get("prompt_eval_count").and_then(Value::as_u64);
            let eval_count = v.get("eval_count").and_then(Value::as_u64);
            let done_reason = v.get("done_reason").and_then(Value::as_str);
            tracing::info!(
                prompt_eval_count,
                eval_count,
                done_reason,
                "ollama chat response"
            );
            if let (Some(sent), Some(n)) = (prompt_eval_count, num_ctx) {
                if sent >= u64::from(n) {
                    tracing::warn!(
                        prompt_eval_count = sent,
                        num_ctx = n,
                        "prompt hit the context ceiling — transcript truncated; raise ollama_num_ctx_max if VRAM allows"
                    );
                }
            }
            Ok(v.get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string())
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
        Transport::Anthropic => {
            // Native Messages API. There is no `response_format`. We can't prefill
            // the assistant turn to force JSON — newer models reject prefill
            // ("does not support assistant message prefill") — so we instruct via
            // the user turn and let callers parse leniently.
            let base = if api_base.is_empty() {
                "https://api.anthropic.com"
            } else {
                api_base
            };
            let content = if json_mode {
                format!("{prompt}\n\nRespond with only the raw JSON, no prose or code fences.")
            } else {
                prompt.to_string()
            };
            let body = json!({
                "model": model,
                "max_tokens": 8192,
                "messages": [{ "role": "user", "content": content }],
            });
            let url = format!("{}/v1/messages", base.trim_end_matches('/'));
            let resp = client
                .post(url)
                .header("x-api-key", *api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await
                .map_err(|e| LlmError(e.to_string()))?;
            let resp = error_for_status(resp).await?;
            let v: Value = resp.json().await.map_err(|e| LlmError(e.to_string()))?;
            Ok(extract_anthropic_content(&v))
        }
        Transport::Unsupported => Err(LlmError(
            "This provider's native client is not yet available in this build.".into(),
        )),
    }
}

/// One streamed line yields zero or more text chunks plus an end-of-stream flag.
struct LineOutcome {
    token: Option<String>,
    done: bool,
}

/// Parse a single decoded transport line into a text chunk / done signal.
/// Ollama emits bare JSON objects per line; OpenAI-compat and Anthropic use SSE
/// `data:` framing. Returns `Err` on an explicit error event in the stream.
fn parse_stream_line(transport: Transport, line: &str) -> Result<LineOutcome, LlmError> {
    let none = LineOutcome {
        token: None,
        done: false,
    };
    match transport {
        Transport::Ollama => {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                return Ok(none);
            };
            let token = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let done = v.get("done").and_then(Value::as_bool).unwrap_or(false);
            Ok(LineOutcome { token, done })
        }
        Transport::OpenAiCompat | Transport::Anthropic => {
            // SSE: ignore everything but `data:` lines; `event:`/comment/blank skipped.
            let Some(data) = line.strip_prefix("data:") else {
                return Ok(none);
            };
            let data = data.trim();
            // OpenAI terminates the stream with a literal `[DONE]` sentinel.
            if data == "[DONE]" {
                return Ok(LineOutcome {
                    token: None,
                    done: true,
                });
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else {
                return Ok(none);
            };
            if transport == Transport::OpenAiCompat {
                let token = v
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"))
                    .and_then(|d| d.get("content"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                Ok(LineOutcome { token, done: false })
            } else {
                // Anthropic typed events. Text arrives as content_block_delta /
                // text_delta; message_stop ends it; an error event aborts.
                match v.get("type").and_then(Value::as_str) {
                    Some("error") => {
                        let msg = v
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(Value::as_str)
                            .unwrap_or("Anthropic stream error");
                        Err(LlmError(msg.to_string()))
                    }
                    Some("message_stop") => Ok(LineOutcome {
                        token: None,
                        done: true,
                    }),
                    Some("content_block_delta") => {
                        let token = v
                            .get("delta")
                            .filter(|d| d.get("type").and_then(Value::as_str) == Some("text_delta"))
                            .and_then(|d| d.get("text"))
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string);
                        Ok(LineOutcome { token, done: false })
                    }
                    _ => Ok(none),
                }
            }
        }
        Transport::Unsupported => Ok(none),
    }
}

/// Streaming chat completion. Calls `on_token` with each text chunk as it
/// arrives and returns the full accumulated text. All transports stream
/// incrementally: Ollama via NDJSON (`stream:true`), OpenAI-compat and Anthropic
/// via their native SSE deltas. Never used for JSON-mode calls — partial JSON is
/// unparseable, so the metadata pass stays on the blocking `chat`.
pub async fn chat_stream<F: FnMut(&str)>(
    req: &ChatRequest<'_>,
    mut on_token: F,
) -> Result<String, LlmError> {
    let ChatRequest {
        transport,
        api_base,
        api_key,
        model,
        prompt,
        timeout_secs,
        num_ctx_max,
    } = req;
    if *transport == Transport::Unsupported {
        return Err(LlmError(
            "This provider's native client is not yet available in this build.".into(),
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(*timeout_secs))
        .build()
        .map_err(|e| LlmError(e.to_string()))?;

    // Build the per-transport streaming request.
    let is_local_ollama =
        *transport == Transport::Ollama && !model.to_lowercase().contains("cloud");
    let num_ctx = if is_local_ollama {
        num_ctx_max.map(|max| fit_num_ctx(prompt.len(), max))
    } else {
        None
    };
    let req = match transport {
        Transport::Ollama => {
            let mut body = json!({
                "model": model,
                "messages": [{ "role": "user", "content": prompt }],
                "stream": true,
            });
            if let Some(n) = num_ctx {
                body["options"] = json!({ "num_ctx": n });
            }
            tracing::info!(
                model,
                prompt_chars = prompt.len(),
                approx_prompt_tokens = approx_tokens(prompt.len()),
                num_ctx,
                num_ctx_max,
                "ollama chat stream request"
            );
            let url = format!("{}/api/chat", api_base.trim_end_matches('/'));
            let mut req = client.post(url).json(&body);
            if !api_key.is_empty() {
                req = req.bearer_auth(api_key);
            }
            req
        }
        Transport::OpenAiCompat => {
            let body = json!({
                "model": model,
                "messages": [{ "role": "user", "content": prompt }],
                "stream": true,
            });
            let url = format!("{}/chat/completions", api_base.trim_end_matches('/'));
            let mut req = client.post(url).json(&body);
            if !api_key.is_empty() {
                req = req.bearer_auth(api_key);
            }
            req
        }
        Transport::Anthropic => {
            let base = if api_base.is_empty() {
                "https://api.anthropic.com"
            } else {
                api_base
            };
            let body = json!({
                "model": model,
                "max_tokens": 8192,
                "stream": true,
                "messages": [{ "role": "user", "content": prompt }],
            });
            let url = format!("{}/v1/messages", base.trim_end_matches('/'));
            client
                .post(url)
                .header("x-api-key", *api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
        }
        Transport::Unsupported => unreachable!(),
    };

    let resp = req.send().await.map_err(|e| LlmError(e.to_string()))?;
    let resp = error_for_status(resp).await?;

    let mut stream = resp.bytes_stream();
    // Chunks split anywhere, so buffer raw bytes and only decode a line once it's
    // complete (keeps multibyte UTF-8 intact across chunk boundaries). Both NDJSON
    // and SSE are newline-delimited, so a line-oriented reader serves all three.
    let mut buf: Vec<u8> = Vec::new();
    let mut full = String::new();
    'outer: while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| LlmError(e.to_string()))?;
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let outcome = parse_stream_line(*transport, line)?;
            if let Some(tok) = outcome.token {
                full.push_str(&tok);
                on_token(&tok);
            }
            if outcome.done {
                break 'outer;
            }
        }
    }
    Ok(full.trim().to_string())
}

/// Cheap reachability probe. For Ollama we hit `/api/tags` (instant, no model
/// load or generation). Other transports have no keyless probe, so we report
/// reachable and lean on the saved-key check instead.
pub async fn ping(
    transport: Transport,
    api_base: &str,
    api_key: &str,
    timeout_secs: u64,
) -> Result<(), LlmError> {
    if transport != Transport::Ollama {
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| LlmError(e.to_string()))?;
    let base = if api_base.is_empty() {
        "http://localhost:11434"
    } else {
        api_base
    };
    let url = format!("{}/api/tags", base.trim_end_matches('/'));
    let mut req = client.get(url);
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let resp = req.send().await.map_err(|e| LlmError(e.to_string()))?;
    error_for_status(resp).await?;
    Ok(())
}

/// Installed models, live from the provider. Only Ollama exposes a keyless
/// listing (`/api/tags`); other transports return empty and the UI falls back
/// to the static suggestions.
pub async fn list_models(
    transport: Transport,
    api_base: &str,
    api_key: &str,
    timeout_secs: u64,
) -> Result<Vec<String>, LlmError> {
    if transport != Transport::Ollama {
        return Ok(Vec::new());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| LlmError(e.to_string()))?;
    let base = if api_base.is_empty() {
        "http://localhost:11434"
    } else {
        api_base
    };
    let url = format!("{}/api/tags", base.trim_end_matches('/'));
    let mut req = client.get(url);
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let resp = req.send().await.map_err(|e| LlmError(e.to_string()))?;
    let v: Value = error_for_status(resp)
        .await?
        .json()
        .await
        .map_err(|e| LlmError(e.to_string()))?;
    let mut models: Vec<String> = v
        .get("models")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    Ok(models)
}

async fn error_for_status(resp: reqwest::Response) -> Result<reqwest::Response, LlmError> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(LlmError(format!("HTTP {status}: {}", body.trim())))
}

fn extract_anthropic_content(v: &Value) -> String {
    let Some(blocks) = v.get("content").and_then(Value::as_array) else {
        return String::new();
    };
    blocks
        .iter()
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string()
}

fn extract_openai_content(v: &Value) -> String {
    let Some(content) = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
    else {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(transport: Transport, line: &str) -> Option<String> {
        parse_stream_line(transport, line).unwrap().token
    }
    fn done(transport: Transport, line: &str) -> bool {
        parse_stream_line(transport, line).unwrap().done
    }

    #[test]
    fn ollama_stream_line() {
        let mid = r#"{"message":{"role":"assistant","content":"Hallo"},"done":false}"#;
        assert_eq!(tok(Transport::Ollama, mid).as_deref(), Some("Hallo"));
        assert!(!done(Transport::Ollama, mid));
        let end = r#"{"message":{"content":""},"done":true,"done_reason":"stop"}"#;
        assert_eq!(tok(Transport::Ollama, end), None);
        assert!(done(Transport::Ollama, end));
    }

    #[test]
    fn openai_stream_line() {
        let chunk = r#"data: {"choices":[{"delta":{"content":"Hi"}}]}"#;
        assert_eq!(tok(Transport::OpenAiCompat, chunk).as_deref(), Some("Hi"));
        // Role-only opening chunk carries no content.
        let role = r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(tok(Transport::OpenAiCompat, role), None);
        // Terminator.
        assert!(done(Transport::OpenAiCompat, "data: [DONE]"));
        assert!(!done(Transport::OpenAiCompat, chunk));
    }

    #[test]
    fn anthropic_stream_line() {
        let delta = r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        assert_eq!(tok(Transport::Anthropic, delta).as_deref(), Some("Hello"));
        // Non-text deltas (e.g. input_json_delta) are not summary text.
        let json_delta = r#"data: {"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"{"}}"#;
        assert_eq!(tok(Transport::Anthropic, json_delta), None);
        // message_stop ends the stream; event:/ping/blank lines are inert.
        assert!(done(
            Transport::Anthropic,
            r#"data: {"type":"message_stop"}"#
        ));
        assert!(!done(Transport::Anthropic, "event: message_stop"));
        assert_eq!(tok(Transport::Anthropic, r#"data: {"type":"ping"}"#), None);
    }

    #[test]
    fn anthropic_stream_error_propagates() {
        let err =
            r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let res = parse_stream_line(Transport::Anthropic, err);
        assert!(matches!(res, Err(LlmError(m)) if m == "Overloaded"));
    }
}
