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
            "gemma4:e2b",
            "gemma4:e4b",
            "gemma4",
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
        default_model: "gemma4:e2b",
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

/// Remember the last model used for a provider (Keeper chats seed new sessions
/// from this). Touches only `default_model`, preserving any saved key/base.
pub fn set_last_model(conn: &Connection, id: &str, model: &str) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO provider_keys (provider_id, default_model, updated_at) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(provider_id) DO UPDATE SET \
            default_model = excluded.default_model, updated_at = excluded.updated_at",
        params![id, model, now],
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
    /// Extra attempts after a transient failure (rate limit / overload). 0 = fail fast.
    pub retries: u32,
}

impl Resolved {
    /// A one-shot chat request against this target. Callers only supply the prompt.
    pub fn chat_req<'a>(&'a self, prompt: &'a str) -> ChatRequest<'a> {
        ChatRequest {
            transport: self.transport,
            api_base: &self.api_base,
            api_key: &self.api_key,
            model: &self.model,
            prompt,
            timeout_secs: self.timeout,
            num_ctx_max: self.num_ctx_max,
            retries: self.retries,
        }
    }
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

    let retries = cfg
        .get("llm_retry_attempts")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3u32)
        .min(10);

    Ok(Resolved {
        provider,
        transport: p.transport,
        api_base,
        api_key,
        model,
        timeout,
        needs_key: p.needs_key,
        num_ctx_max,
        retries,
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
    pub retries: u32,
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
        retries,
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
            let resp = send_retrying(req, *retries).await?;
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
            let resp = send_retrying(req, *retries).await?;
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
            let req = client
                .post(url)
                .header("x-api-key", *api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body);
            let resp = send_retrying(req, *retries).await?;
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
        retries,
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

    let resp = send_retrying(req, *retries).await?;

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

/// Turn a raw transport error into something the user can act on. Provider 400s
/// are JSON blobs; the common ones (no image support, missing model) become a
/// plain instruction instead of leaking HTTP noise into the chat.
pub fn friendly_llm_error(raw: &str) -> String {
    let m = raw.to_lowercase();
    if m.contains("image") && (m.contains("support") || m.contains("not allowed")) {
        "This model can't accept images. Remove the image, or pick a vision-capable \
         model in Settings, then try again."
            .into()
    } else if m.contains("not found") && m.contains("model") {
        // Ollama's 404 body looks like: {"error":"model 'llama3.2' not found"} —
        // pull the quoted name out so the message can name the exact pull command.
        let name = raw.split('\'').nth(1);
        match name {
            Some(name) => format!(
                "Model \"{name}\" isn't pulled in Ollama yet. Pull it from Settings → \
                 LLM providers, or run \"ollama pull {name}\" in a terminal."
            ),
            None => {
                "That model wasn't found at the provider. Check the model name in Settings.".into()
            }
        }
    } else {
        raw.to_string()
    }
}

/// Pull an Ollama model, reporting progress into `progress` (mirrors the
/// transcription-model download so the frontend can reuse the same poll +
/// progress-bar UI). Ollama streams NDJSON lines like
/// `{"status":"pulling ...","total":123,"completed":45}`, ending with
/// `{"status":"success"}`.
pub async fn pull_model(
    api_base: &str,
    model: &str,
    progress: &std::sync::Arc<std::sync::Mutex<crate::state::ModelProgress>>,
) -> Result<(), LlmError> {
    use crate::state::ModelProgress;

    // No overall .timeout(): a large model pull can run for many minutes; only
    // bound the initial connect so an unreachable daemon fails fast.
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| LlmError(e.to_string()))?;
    let url = format!("{}/api/pull", api_base.trim_end_matches('/'));
    let body = json!({ "model": model, "stream": true });
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| LlmError(e.to_string()))?;
    let resp = error_for_status(resp).await?;

    ModelProgress::set(progress, "pulling", 0, 0);
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut succeeded = false;
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
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(err) = v.get("error").and_then(Value::as_str) {
                let msg = err.to_string();
                ModelProgress::set_error(progress, msg.clone());
                return Err(LlmError(msg));
            }
            let status = v.get("status").and_then(Value::as_str).unwrap_or("");
            let total = v.get("total").and_then(Value::as_u64).unwrap_or(0);
            let completed = v.get("completed").and_then(Value::as_u64).unwrap_or(0);
            if total > 0 {
                ModelProgress::set(progress, "pulling", completed, total);
            }
            if status == "success" {
                succeeded = true;
                break 'outer;
            }
        }
    }
    if succeeded {
        ModelProgress::set(progress, "ready", 0, 0);
        Ok(())
    } else {
        let msg = "Ollama closed the connection before confirming the pull finished.".to_string();
        ModelProgress::set_error(progress, msg.clone());
        Err(LlmError(msg))
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

// ---- transient-failure retry ----

/// Longest single wait we'll honour. A provider hint above this means the budget
/// is gone for minutes, not seconds — surface it instead of hanging the run.
const MAX_WAIT_SECS: f64 = 60.0;

/// A rate limit (429) or provider overload is a wait-and-retry condition, not a
/// user error. Everything else (bad key, unknown model, malformed request) is
/// deterministic and must fail on the first try.
fn is_transient(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 502 | 503 | 504 | 529)
}

/// Parse a wait hint into seconds: `Retry-After` is bare seconds, OpenAI's
/// headers and prose use Go-style durations (`2.192s`, `20ms`, `1m30s`).
fn parse_wait_secs(raw: &str) -> Option<f64> {
    let s = raw.trim().to_ascii_lowercase();
    let mut rest = s.as_str();
    let mut total = 0.0;
    let mut matched = false;
    while !rest.is_empty() {
        let after_digits = rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
        let split = rest.len() - after_digits.len();
        if split == 0 {
            break;
        }
        // Trailing punctuation ("2.192s.") lands here as an unparseable chunk.
        let Ok(value) = rest[..split].parse::<f64>() else {
            break;
        };
        rest = after_digits;
        let (mult, unit_len) = if rest.starts_with("ms") {
            (0.001, 2)
        } else if rest.starts_with('s') {
            (1.0, 1)
        } else if rest.starts_with('m') {
            (60.0, 1)
        } else if rest.starts_with('h') {
            (3600.0, 1)
        } else {
            (1.0, 0)
        };
        total += value * mult;
        matched = true;
        rest = &rest[unit_len..];
    }
    matched.then_some(total)
}

/// How long the provider wants us to wait, in its own words. Headers first
/// (`Retry-After`, then OpenAI's per-bucket reset), else the message body, where
/// OpenAI puts the only precise figure: "Please try again in 2.192s."
fn wait_hint(headers: &reqwest::header::HeaderMap, body: &str) -> Option<f64> {
    for name in [
        "retry-after",
        "x-ratelimit-reset-tokens",
        "x-ratelimit-reset-requests",
    ] {
        if let Some(secs) = headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_wait_secs)
        {
            return Some(secs);
        }
    }
    let lower = body.to_ascii_lowercase();
    let at = lower.find("try again in ")? + "try again in ".len();
    let token: String = lower[at..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.')
        .collect();
    parse_wait_secs(&token)
}

/// Send a request, waiting out transient rate limits. Honours the provider's own
/// retry interval when it gives one, else backs off exponentially. Returns the
/// first successful response, or the last failure's status + body.
async fn send_retrying(
    req: reqwest::RequestBuilder,
    retries: u32,
) -> Result<reqwest::Response, LlmError> {
    let mut pending = req;
    let mut attempt = 0u32;
    loop {
        // Cloning must happen before send() consumes the builder; a streamed
        // body can't be cloned, and then there is nothing to retry with.
        let next = pending.try_clone();
        let resp = pending.send().await.map_err(|e| LlmError(e.to_string()))?;
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.text().await.unwrap_or_default();
        if attempt >= retries || !is_transient(status) || next.is_none() {
            return Err(LlmError(format!("HTTP {status}: {}", body.trim())));
        }
        let wait = wait_hint(&headers, &body);
        // Without a hint: 1s, 2s, 4s… A hint gets a small cushion, since the
        // provider's own figure is the earliest moment the budget frees up.
        let wait = match wait {
            Some(s) => s + 0.25,
            None => f64::from(1u32 << attempt.min(5)),
        };
        if wait > MAX_WAIT_SECS {
            return Err(LlmError(format!("HTTP {status}: {}", body.trim())));
        }
        attempt += 1;
        tracing::warn!(
            %status,
            attempt,
            retries,
            wait_secs = wait,
            "transient LLM failure — waiting and retrying"
        );
        tokio::time::sleep(Duration::from_secs_f64(wait)).await;
        pending = next.expect("checked above");
    }
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
    use std::sync::atomic::{AtomicUsize, Ordering};

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
    fn parses_wait_hints() {
        assert_eq!(parse_wait_secs("3"), Some(3.0));
        assert_eq!(parse_wait_secs("2.192s"), Some(2.192));
        assert_eq!(parse_wait_secs("20ms"), Some(0.02));
        assert_eq!(parse_wait_secs("1m30s"), Some(90.0));
        assert_eq!(parse_wait_secs("6m0s"), Some(360.0));
        assert_eq!(parse_wait_secs(""), None);
        assert_eq!(parse_wait_secs("soon"), None);
    }

    #[test]
    fn wait_hint_prefers_header_then_body() {
        use reqwest::header::{HeaderMap, HeaderValue};
        let openai_429 = "Rate limit reached for gpt-5 ... Limit 500000 TPM, Used 436092, \
                          Requested 82181. Please try again in 2.192s. Visit …";
        let empty = HeaderMap::new();
        assert_eq!(wait_hint(&empty, openai_429), Some(2.192));
        let mut h = HeaderMap::new();
        h.insert("x-ratelimit-reset-tokens", HeaderValue::from_static("1.5s"));
        assert_eq!(wait_hint(&h, openai_429), Some(1.5));
        h.insert("retry-after", HeaderValue::from_static("7"));
        assert_eq!(wait_hint(&h, openai_429), Some(7.0));
        assert_eq!(wait_hint(&empty, "invalid api key"), None);
    }

    #[test]
    fn only_rate_limits_and_overload_retry() {
        use reqwest::StatusCode;
        assert!(is_transient(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_transient(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_transient(StatusCode::UNAUTHORIZED));
        assert!(!is_transient(StatusCode::BAD_REQUEST));
        assert!(!is_transient(StatusCode::NOT_FOUND));
    }

    /// A fake OpenAI-compatible endpoint that rejects its first `fail` calls with
    /// 429, then answers. Returns (base url, hit counter).
    async fn rate_limited_server(
        fail: usize,
        status: u16,
    ) -> (String, std::sync::Arc<AtomicUsize>) {
        use axum::response::IntoResponse;
        let hits = std::sync::Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        let app = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(move || {
                let h = h.clone();
                async move {
                    if h.fetch_add(1, Ordering::SeqCst) < fail {
                        (
                            axum::http::StatusCode::from_u16(status).unwrap(),
                            [("retry-after", "0")],
                            "Rate limit reached for gpt-5. Please try again in 0.01s.",
                        )
                            .into_response()
                    } else {
                        axum::Json(json!({"choices":[{"message":{"content":"ok"}}]}))
                            .into_response()
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), hits)
    }

    fn req_to<'a>(base: &'a str, retries: u32) -> ChatRequest<'a> {
        ChatRequest {
            transport: Transport::OpenAiCompat,
            api_base: base,
            api_key: "",
            model: "m",
            prompt: "hi",
            timeout_secs: 5,
            num_ctx_max: None,
            retries,
        }
    }

    #[tokio::test]
    async fn retries_a_rate_limit_then_succeeds() {
        let (base, hits) = rate_limited_server(2, 429).await;
        assert_eq!(chat(&req_to(&base, 3), false).await.unwrap(), "ok");
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn gives_up_after_the_configured_attempts() {
        let (base, hits) = rate_limited_server(usize::MAX, 429).await;
        let err = chat(&req_to(&base, 1), false).await.unwrap_err();
        assert!(err.0.contains("429"), "{}", err.0);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn does_not_retry_a_permanent_error() {
        let (base, hits) = rate_limited_server(usize::MAX, 401).await;
        assert!(chat(&req_to(&base, 3), false).await.is_err());
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn anthropic_stream_error_propagates() {
        let err =
            r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let res = parse_stream_line(Transport::Anthropic, err);
        assert!(matches!(res, Err(LlmError(m)) if m == "Overloaded"));
    }
}
