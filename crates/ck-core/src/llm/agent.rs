//! Tool-calling chat layer for the Keeper agent loop (Phase 6).
//! Parallel API to `chat`/`chat_stream` — those stay single-prompt and keep
//! serving summarize/import. Text deltas stream; tool calls are buffered and
//! surfaced only when complete.

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{LlmError, Resolved, Transport};

/// A pasted image, base64-encoded. `media_type` is an image MIME (e.g.
/// "image/png") — the shape every vision transport wants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Msg {
    System(String),
    User(String),
    /// User turn carrying pasted images alongside (optional) text.
    UserImages {
        text: String,
        images: Vec<Image>,
    },
    Assistant {
        text: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Other,
}

#[derive(Debug)]
pub struct AssistantTurn {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: StopReason,
}

#[derive(Debug)]
pub enum AgentDelta {
    Text(String),
}

// ---- per-transport request bodies ----

/// Anthropic: system messages concatenate into the top-level `system` param;
/// tool results ride as `tool_result` blocks in user messages. Consecutive
/// same-role messages merge (tool_result blocks must directly follow the
/// assistant tool_use turn).
fn anthropic_body(msgs: &[Msg], tools: &[ToolDef], model: &str, stream: bool) -> Value {
    fn push(out: &mut Vec<Value>, role: &str, block: Value) {
        if let Some(last) = out.last_mut() {
            if last["role"] == role {
                last["content"].as_array_mut().unwrap().push(block);
                return;
            }
        }
        out.push(json!({ "role": role, "content": [block] }));
    }
    let mut system = String::new();
    let mut messages: Vec<Value> = Vec::new();
    for m in msgs {
        match m {
            Msg::System(s) => {
                if !system.is_empty() {
                    system.push_str("\n\n");
                }
                system.push_str(s);
            }
            Msg::User(s) => push(&mut messages, "user", json!({ "type": "text", "text": s })),
            Msg::UserImages { text, images } => {
                if !text.is_empty() {
                    push(
                        &mut messages,
                        "user",
                        json!({ "type": "text", "text": text }),
                    );
                }
                for img in images {
                    push(
                        &mut messages,
                        "user",
                        json!({ "type": "image", "source": { "type": "base64", "media_type": img.media_type, "data": img.data } }),
                    );
                }
            }
            Msg::Assistant { text, tool_calls } => {
                if !text.is_empty() {
                    push(
                        &mut messages,
                        "assistant",
                        json!({ "type": "text", "text": text }),
                    );
                }
                for c in tool_calls {
                    push(
                        &mut messages,
                        "assistant",
                        json!({ "type": "tool_use", "id": c.id, "name": c.name, "input": c.arguments }),
                    );
                }
            }
            Msg::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => push(
                &mut messages,
                "user",
                json!({ "type": "tool_result", "tool_use_id": call_id, "content": content, "is_error": is_error }),
            ),
        }
    }
    // Prompt caching (Anthropic only — Ollama/Gemini ignore cache_control or
    // cache automatically). Render order is tools → system → messages: one
    // breakpoint at the end of the stable prefix (system, which also caches the
    // tools before it — identical every turn and every agent-loop iteration),
    // plus one on the latest message so the growing conversation is cached too.
    // Reads cost ~0.1x; the diagnostic log surfaces cache_read_input_tokens.
    if let Some(blocks) = messages
        .last_mut()
        .and_then(|m| m["content"].as_array_mut())
    {
        if let Some(last) = blocks.last_mut() {
            last["cache_control"] = json!({ "type": "ephemeral" });
        }
    }
    let mut body = json!({
        "model": model,
        "max_tokens": 8192,
        "stream": stream,
        "messages": messages,
    });
    if !tools.is_empty() {
        let mut defs: Vec<Value> = tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.schema }))
            .collect();
        // With no system block, the last tool is the end of the stable prefix.
        if system.is_empty() {
            if let Some(last) = defs.last_mut() {
                last["cache_control"] = json!({ "type": "ephemeral" });
            }
        }
        body["tools"] = json!(defs);
    }
    if !system.is_empty() {
        // Text-block form so cache_control can ride the prefix end.
        body["system"] = json!([{
            "type": "text",
            "text": system,
            "cache_control": { "type": "ephemeral" },
        }]);
    }
    body
}

/// OpenAI-compat and Ollama share message shape; OpenAI wants tool-call
/// arguments as a JSON *string*, Ollama as an object, and Ollama's tool role
/// uses `tool_name` instead of `tool_call_id`.
fn openai_style_body(
    msgs: &[Msg],
    tools: &[ToolDef],
    model: &str,
    stream: bool,
    ollama: bool,
) -> Value {
    let messages: Vec<Value> = msgs
        .iter()
        .map(|m| match m {
            Msg::System(s) => json!({ "role": "system", "content": s }),
            Msg::User(s) => json!({ "role": "user", "content": s }),
            Msg::UserImages { text, images } => {
                if ollama {
                    // Ollama: raw base64 (no data: prefix) in a sibling `images` array.
                    let imgs: Vec<Value> = images.iter().map(|i| json!(i.data)).collect();
                    json!({ "role": "user", "content": text, "images": imgs })
                } else {
                    let mut parts = vec![json!({ "type": "text", "text": text })];
                    for i in images {
                        let url = format!("data:{};base64,{}", i.media_type, i.data);
                        parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
                    }
                    json!({ "role": "user", "content": parts })
                }
            }
            Msg::Assistant { text, tool_calls } => {
                let mut v = json!({ "role": "assistant", "content": text });
                if !tool_calls.is_empty() {
                    v["tool_calls"] = tool_calls
                        .iter()
                        .map(|c| {
                            let args = if ollama {
                                c.arguments.clone()
                            } else {
                                Value::String(c.arguments.to_string())
                            };
                            json!({
                                "id": c.id,
                                "type": "function",
                                "function": { "name": c.name, "arguments": args },
                            })
                        })
                        .collect();
                }
                v
            }
            Msg::ToolResult {
                call_id,
                name,
                content,
                is_error,
            } => {
                // No native error flag outside Anthropic — prefix instead.
                let content = if *is_error {
                    format!("ERROR: {content}")
                } else {
                    content.clone()
                };
                if ollama {
                    json!({ "role": "tool", "tool_name": name, "content": content })
                } else {
                    json!({ "role": "tool", "tool_call_id": call_id, "content": content })
                }
            }
        })
        .collect();
    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": stream,
    });
    // OpenAI-compat only emits a usage block when asked; Ollama always sends
    // counts on its final chunk and rejects unknown keys.
    if stream && !ollama {
        body["stream_options"] = json!({ "include_usage": true });
    }
    if !tools.is_empty() {
        body["tools"] = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": { "name": t.name, "description": t.description, "parameters": t.schema },
                })
            })
            .collect();
    }
    body
}

// ---- stream assembly ----

fn parse_stop(s: &str) -> StopReason {
    match s {
        "end_turn" | "stop" => StopReason::EndTurn,
        "tool_use" | "tool_calls" => StopReason::ToolUse,
        "max_tokens" | "length" => StopReason::MaxTokens,
        _ => StopReason::Other,
    }
}

fn parse_args(buf: &str) -> Value {
    if buf.trim().is_empty() {
        return json!({});
    }
    serde_json::from_str(buf).unwrap_or_else(|_| json!({}))
}

#[derive(Default)]
struct ToolBuf {
    id: String,
    name: String,
    args: String,
}

#[derive(Default)]
struct StreamState {
    text: String,
    tools: BTreeMap<u64, ToolBuf>,
    stop_reason: Option<StopReason>,
    done: bool,
    /// Provider usage block accumulated from the stream (token counts incl.
    /// any cache-hit fields). Logged for caching diagnostics, not used in flow.
    usage: Option<Value>,
}

impl StreamState {
    /// Merge a provider `usage` object into the accumulator (non-null wins).
    fn merge_usage(&mut self, u: &Value) {
        let Some(src) = u.as_object() else { return };
        let dst = self.usage.get_or_insert_with(|| json!({}));
        let dst = dst.as_object_mut().unwrap();
        for (k, val) in src {
            if !val.is_null() {
                dst.insert(k.clone(), val.clone());
            }
        }
    }

    fn finish(self) -> AssistantTurn {
        let tool_calls: Vec<ToolCall> = self
            .tools
            .into_values()
            .map(|t| ToolCall {
                id: t.id,
                name: t.name,
                arguments: parse_args(&t.args),
            })
            .collect();
        // Tool calls are authoritative: Ollama streams done_reason "stop" even
        // with tool_calls attached, so trusting stop_reason would skip the loop.
        let stop_reason = if tool_calls.is_empty() {
            self.stop_reason.unwrap_or(StopReason::EndTurn)
        } else {
            StopReason::ToolUse
        };
        AssistantTurn {
            text: self.text.trim().to_string(),
            tool_calls,
            stop_reason,
        }
    }

    /// Anthropic typed SSE events. Tool-use JSON arrives as `input_json_delta`
    /// fragments buffered per block index. Returns a text delta to emit.
    fn anthropic_line(&mut self, line: &str) -> Result<Option<String>, LlmError> {
        let Some(data) = line.strip_prefix("data:") else {
            return Ok(None);
        };
        let Ok(v) = serde_json::from_str::<Value>(data.trim()) else {
            return Ok(None);
        };
        match v.get("type").and_then(Value::as_str) {
            Some("error") => {
                let msg = v
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("Anthropic stream error");
                Err(LlmError(msg.to_string()))
            }
            Some("content_block_start") => {
                let idx = v.get("index").and_then(Value::as_u64).unwrap_or(0);
                let block = &v["content_block"];
                if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                    self.tools.insert(
                        idx,
                        ToolBuf {
                            id: block
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            name: block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            args: String::new(),
                        },
                    );
                }
                Ok(None)
            }
            Some("content_block_delta") => {
                let idx = v.get("index").and_then(Value::as_u64).unwrap_or(0);
                let delta = &v["delta"];
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let t = delta.get("text").and_then(Value::as_str).unwrap_or("");
                        if t.is_empty() {
                            return Ok(None);
                        }
                        self.text.push_str(t);
                        Ok(Some(t.to_string()))
                    }
                    Some("input_json_delta") => {
                        if let Some(buf) = self.tools.get_mut(&idx) {
                            buf.args.push_str(
                                delta
                                    .get("partial_json")
                                    .and_then(Value::as_str)
                                    .unwrap_or(""),
                            );
                        }
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }
            Some("message_start") => {
                // cache_read/creation + input_tokens land here on the opening event.
                self.merge_usage(&v["message"]["usage"]);
                Ok(None)
            }
            Some("message_delta") => {
                if let Some(s) = v
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(Value::as_str)
                {
                    self.stop_reason = Some(parse_stop(s));
                }
                // output_tokens (and refreshed cache fields) ride the closing delta.
                self.merge_usage(&v["usage"]);
                Ok(None)
            }
            Some("message_stop") => {
                self.done = true;
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// OpenAI-compat SSE chunks. Tool-call fragments are indexed by
    /// `tool_calls[].index` and assembled across chunks.
    fn openai_line(&mut self, line: &str) -> Result<Option<String>, LlmError> {
        let Some(data) = line.strip_prefix("data:") else {
            return Ok(None);
        };
        let data = data.trim();
        if data == "[DONE]" {
            self.done = true;
            return Ok(None);
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return Ok(None);
        };
        // The usage chunk (include_usage) carries an empty choices array, so
        // grab it before the choices guard. cached_tokens lives under
        // prompt_tokens_details.
        self.merge_usage(&v["usage"]);
        let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
            return Ok(None);
        };
        if let Some(s) = choice.get("finish_reason").and_then(Value::as_str) {
            self.stop_reason = Some(parse_stop(s));
        }
        let delta = &choice["delta"];
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for c in calls {
                let idx = c.get("index").and_then(Value::as_u64).unwrap_or(0);
                let buf = self.tools.entry(idx).or_default();
                if let Some(id) = c.get("id").and_then(Value::as_str) {
                    buf.id = id.to_string();
                }
                if let Some(f) = c.get("function") {
                    if let Some(n) = f.get("name").and_then(Value::as_str) {
                        buf.name.push_str(n);
                    }
                    if let Some(a) = f.get("arguments").and_then(Value::as_str) {
                        buf.args.push_str(a);
                    }
                }
            }
        }
        let token = delta
            .get("content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        if let Some(t) = &token {
            self.text.push_str(t);
        }
        Ok(token)
    }

    /// Ollama NDJSON chunks (text-only streaming; tools go non-streaming).
    fn ollama_line(&mut self, line: &str) -> Result<Option<String>, LlmError> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Ok(None);
        };
        if v.get("done").and_then(Value::as_bool).unwrap_or(false) {
            self.done = true;
            if let Some(s) = v.get("done_reason").and_then(Value::as_str) {
                self.stop_reason = Some(parse_stop(s));
            }
            // Counts (prompt_eval_count, eval_count, …) ride the final chunk.
            // Capture every scalar top-level key so Ollama Cloud cache fields,
            // whatever they're named, surface in the log too.
            if let Some(obj) = v.as_object() {
                let counts: serde_json::Map<String, Value> = obj
                    .iter()
                    .filter(|(k, val)| {
                        k.as_str() != "message" && !val.is_object() && !val.is_array()
                    })
                    .map(|(k, val)| (k.clone(), val.clone()))
                    .collect();
                self.merge_usage(&Value::Object(counts));
            }
        }
        self.absorb_ollama_tool_calls(&v);
        let token = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        if let Some(t) = &token {
            self.text.push_str(t);
        }
        Ok(token)
    }

    /// Ollama returns tool calls without ids — synthesize stable ones.
    fn absorb_ollama_tool_calls(&mut self, v: &Value) {
        let Some(calls) = v
            .get("message")
            .and_then(|m| m.get("tool_calls"))
            .and_then(Value::as_array)
        else {
            return;
        };
        for c in calls {
            let f = &c["function"];
            let idx = self.tools.len() as u64;
            self.tools.insert(
                idx,
                ToolBuf {
                    id: format!("call_{idx}"),
                    name: f
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    args: f
                        .get("arguments")
                        .map(|a| {
                            if a.is_string() {
                                a.as_str().unwrap_or_default().to_string()
                            } else {
                                a.to_string()
                            }
                        })
                        .unwrap_or_default(),
                },
            );
        }
    }
}

// ---- the call ----

/// One assistant turn with tools attached. Streams text via `on_delta`,
/// buffers tool calls, returns the complete turn. All transports stream;
/// Ollama assembles tool_calls from the streamed chunks via `ollama_line`.
pub async fn agent_chat_stream<F: FnMut(AgentDelta)>(
    resolved: &Resolved,
    msgs: &[Msg],
    tools: &[ToolDef],
    mut on_delta: F,
) -> Result<AssistantTurn, LlmError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(resolved.timeout))
        .build()
        .map_err(|e| LlmError(e.to_string()))?;

    let req = match resolved.transport {
        Transport::Ollama => {
            let mut body = openai_style_body(msgs, tools, &resolved.model, true, true);
            let is_local = !resolved.model.to_lowercase().contains("cloud");
            if is_local {
                if let Some(max) = resolved.num_ctx_max {
                    let chars: usize = msgs.iter().map(msg_chars).sum();
                    body["options"] = json!({ "num_ctx": super::fit_num_ctx(chars, max) });
                }
            }
            let url = format!("{}/api/chat", resolved.api_base.trim_end_matches('/'));
            let mut req = client.post(url).json(&body);
            if !resolved.api_key.is_empty() {
                req = req.bearer_auth(&resolved.api_key);
            }
            req
        }
        Transport::OpenAiCompat => {
            let body = openai_style_body(msgs, tools, &resolved.model, true, false);
            let url = format!(
                "{}/chat/completions",
                resolved.api_base.trim_end_matches('/')
            );
            let mut req = client.post(url).json(&body);
            if !resolved.api_key.is_empty() {
                req = req.bearer_auth(&resolved.api_key);
            }
            req
        }
        Transport::Anthropic => {
            let base = if resolved.api_base.is_empty() {
                "https://api.anthropic.com"
            } else {
                &resolved.api_base
            };
            let body = anthropic_body(msgs, tools, &resolved.model, true);
            let url = format!("{}/v1/messages", base.trim_end_matches('/'));
            client
                .post(url)
                .header("x-api-key", &resolved.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
        }
        Transport::Unsupported => {
            return Err(LlmError(
                "This provider's native client is not yet available in this build.".into(),
            ))
        }
    };

    let resp = req.send().await.map_err(|e| LlmError(e.to_string()))?;
    let resp = super::error_for_status(resp).await?;

    let mut state = StreamState::default();
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
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
            let token = match resolved.transport {
                Transport::Anthropic => state.anthropic_line(line)?,
                Transport::OpenAiCompat => state.openai_line(line)?,
                Transport::Ollama => state.ollama_line(line)?,
                Transport::Unsupported => unreachable!(),
            };
            if let Some(t) = token {
                on_delta(AgentDelta::Text(t));
            }
            if state.done {
                break 'outer;
            }
        }
    }
    if let Some(usage) = &state.usage {
        tracing::info!(
            transport = ?resolved.transport,
            model = %resolved.model,
            usage = %usage,
            "agent turn usage (cache diagnostics)"
        );
    }
    Ok(state.finish())
}

fn msg_chars(m: &Msg) -> usize {
    match m {
        Msg::System(s) | Msg::User(s) => s.len(),
        // Image bytes are not text budget — count only the prose.
        Msg::UserImages { text, .. } => text.len(),
        Msg::Assistant { text, tool_calls } => {
            text.len()
                + tool_calls
                    .iter()
                    .map(|c| c.name.len() + c.arguments.to_string().len())
                    .sum::<usize>()
        }
        Msg::ToolResult { content, .. } => content.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_msgs() -> Vec<Msg> {
        vec![
            Msg::System("You are the Keeper.".into()),
            Msg::User("Who rules Thornhold?".into()),
            Msg::Assistant {
                text: "Let me check.".into(),
                tool_calls: vec![ToolCall {
                    id: "tc1".into(),
                    name: "search_pages".into(),
                    arguments: json!({ "query": "Thornhold" }),
                }],
            },
            Msg::ToolResult {
                call_id: "tc1".into(),
                name: "search_pages".into(),
                content: "Thornhold — ruled by Baron Aldric".into(),
                is_error: false,
            },
        ]
    }

    fn tool_defs() -> Vec<ToolDef> {
        vec![ToolDef {
            name: "search_pages".into(),
            description: "Full-text search over codex pages".into(),
            schema: json!({ "type": "object", "properties": { "query": { "type": "string" } } }),
        }]
    }

    #[test]
    fn anthropic_body_shape() {
        let body = anthropic_body(&sample_msgs(), &tool_defs(), "claude-sonnet-4-6", true);
        assert_eq!(body["system"][0]["text"], "You are the Keeper.");
        let msgs = body["messages"].as_array().unwrap();
        // user, assistant (text + tool_use merged), user (tool_result)
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[1]["role"], "assistant");
        let blocks = msgs[1]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1]["type"], "tool_use");
        assert_eq!(blocks[1]["input"]["query"], "Thornhold");
        assert_eq!(msgs[2]["content"][0]["type"], "tool_result");
        assert_eq!(msgs[2]["content"][0]["tool_use_id"], "tc1");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
    }

    #[test]
    fn anthropic_cache_breakpoints() {
        // System present → breakpoint on the system block (covers tools too),
        // not on the tools; plus one on the latest message.
        let body = anthropic_body(&sample_msgs(), &tool_defs(), "claude-sonnet-4-6", false);
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert!(body["tools"][0].get("cache_control").is_none());
        let msgs = body["messages"].as_array().unwrap();
        let last = msgs.last().unwrap()["content"].as_array().unwrap();
        assert_eq!(last.last().unwrap()["cache_control"]["type"], "ephemeral");

        // No system → the last tool carries the stable-prefix breakpoint.
        let no_sys = vec![Msg::User("hi".into())];
        let body = anthropic_body(&no_sys, &tool_defs(), "m", false);
        assert!(body.get("system").is_none());
        assert_eq!(body["tools"][0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn anthropic_consecutive_tool_results_merge() {
        let msgs = vec![
            Msg::User("q".into()),
            Msg::Assistant {
                text: String::new(),
                tool_calls: vec![
                    ToolCall {
                        id: "a".into(),
                        name: "t".into(),
                        arguments: json!({}),
                    },
                    ToolCall {
                        id: "b".into(),
                        name: "t".into(),
                        arguments: json!({}),
                    },
                ],
            },
            Msg::ToolResult {
                call_id: "a".into(),
                name: "t".into(),
                content: "1".into(),
                is_error: false,
            },
            Msg::ToolResult {
                call_id: "b".into(),
                name: "t".into(),
                content: "2".into(),
                is_error: true,
            },
        ];
        let body = anthropic_body(&msgs, &[], "m", false);
        let out = body["messages"].as_array().unwrap();
        assert_eq!(out.len(), 3);
        let results = out[2]["content"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[1]["is_error"], true);
    }

    #[test]
    fn openai_body_shape() {
        let body = openai_style_body(&sample_msgs(), &tool_defs(), "gpt-4.1-mini", true, false);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0]["role"], "system");
        // Arguments must be a JSON string for OpenAI.
        let args = &msgs[2]["tool_calls"][0]["function"]["arguments"];
        assert!(args.is_string());
        assert_eq!(msgs[3]["role"], "tool");
        assert_eq!(msgs[3]["tool_call_id"], "tc1");
        assert_eq!(body["tools"][0]["function"]["name"], "search_pages");
    }

    #[test]
    fn ollama_body_shape() {
        let body = openai_style_body(&sample_msgs(), &tool_defs(), "qwen3", false, true);
        let msgs = body["messages"].as_array().unwrap();
        // Arguments stay an object; tool result keyed by tool_name.
        assert!(msgs[2]["tool_calls"][0]["function"]["arguments"].is_object());
        assert_eq!(msgs[3]["tool_name"], "search_pages");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn images_shape_per_transport() {
        let msgs = vec![Msg::UserImages {
            text: "what is this".into(),
            images: vec![Image {
                media_type: "image/png".into(),
                data: "QUJD".into(),
            }],
        }];

        let a = anthropic_body(&msgs, &[], "claude", false);
        let blocks = a["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["source"]["media_type"], "image/png");
        assert_eq!(blocks[1]["source"]["data"], "QUJD");

        let o = openai_style_body(&msgs, &[], "gpt", false, false);
        let parts = o["messages"][0]["content"].as_array().unwrap();
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,QUJD");

        let ol = openai_style_body(&msgs, &[], "llava", false, true);
        assert_eq!(ol["messages"][0]["content"], "what is this");
        assert_eq!(ol["messages"][0]["images"][0], "QUJD");
    }

    #[test]
    fn error_tool_result_prefixed_outside_anthropic() {
        let msgs = vec![Msg::ToolResult {
            call_id: "x".into(),
            name: "t".into(),
            content: "boom".into(),
            is_error: true,
        }];
        let body = openai_style_body(&msgs, &[], "m", false, false);
        assert_eq!(body["messages"][0]["content"], "ERROR: boom");
    }

    #[test]
    fn anthropic_stream_assembles_tool_call() {
        let mut st = StreamState::default();
        let lines = [
            r#"data: {"type":"message_start","message":{}}"#,
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text"}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Checking"}}"#,
            r#"data: {"type":"content_block_stop","index":0}"#,
            r#"data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"search_pages"}}"#,
            r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"que"}}"#,
            r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"ry\":\"Thornhold\"}"}}"#,
            r#"data: {"type":"content_block_stop","index":1}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"tool_use"}}"#,
            r#"data: {"type":"message_stop"}"#,
        ];
        let mut text = String::new();
        for l in lines {
            if let Some(t) = st.anthropic_line(l).unwrap() {
                text.push_str(&t);
            }
        }
        assert!(st.done);
        assert_eq!(text, "Checking");
        let turn = st.finish();
        assert_eq!(turn.stop_reason, StopReason::ToolUse);
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].id, "toolu_1");
        assert_eq!(turn.tool_calls[0].arguments["query"], "Thornhold");
    }

    #[test]
    fn openai_stream_assembles_fragmented_tool_calls() {
        let mut st = StreamState::default();
        let lines = [
            r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","function":{"name":"search_pages","arguments":""}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"query\":"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"Thornhold\"}"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
            "data: [DONE]",
        ];
        for l in lines {
            st.openai_line(l).unwrap();
        }
        assert!(st.done);
        let turn = st.finish();
        assert_eq!(turn.stop_reason, StopReason::ToolUse);
        assert_eq!(turn.tool_calls[0].id, "call_a");
        assert_eq!(turn.tool_calls[0].arguments["query"], "Thornhold");
    }

    #[test]
    fn openai_stream_text_only() {
        let mut st = StreamState::default();
        let mut text = String::new();
        for l in [
            r#"data: {"choices":[{"delta":{"content":"Hi "}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"there"}}]}"#,
            r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            "data: [DONE]",
        ] {
            if let Some(t) = st.openai_line(l).unwrap() {
                text.push_str(&t);
            }
        }
        assert_eq!(text, "Hi there");
        let turn = st.finish();
        assert_eq!(turn.stop_reason, StopReason::EndTurn);
        assert!(turn.tool_calls.is_empty());
    }

    #[test]
    fn ollama_nonstream_tool_calls_get_ids() {
        let v: Value = serde_json::from_str(
            r#"{"message":{"role":"assistant","content":"","tool_calls":[
                {"function":{"name":"search_pages","arguments":{"query":"Thornhold"}}},
                {"function":{"name":"read_page","arguments":{"path":"Codex/Thornhold.md"}}}
            ]},"done":true,"done_reason":"stop"}"#,
        )
        .unwrap();
        let mut st = StreamState::default();
        st.absorb_ollama_tool_calls(&v);
        st.stop_reason = Some(StopReason::ToolUse);
        let turn = st.finish();
        assert_eq!(turn.tool_calls.len(), 2);
        assert_eq!(turn.tool_calls[0].id, "call_0");
        assert_eq!(turn.tool_calls[1].id, "call_1");
        assert_eq!(turn.tool_calls[0].arguments["query"], "Thornhold");
    }

    #[test]
    fn ollama_stream_text() {
        let mut st = StreamState::default();
        let mut text = String::new();
        for l in [
            r#"{"message":{"role":"assistant","content":"Hal"},"done":false}"#,
            r#"{"message":{"content":"lo"},"done":false}"#,
            r#"{"message":{"content":""},"done":true,"done_reason":"stop"}"#,
        ] {
            if let Some(t) = st.ollama_line(l).unwrap() {
                text.push_str(&t);
            }
        }
        assert!(st.done);
        assert_eq!(text, "Hallo");
        assert_eq!(st.finish().stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn ollama_stream_tool_calls_force_tool_use() {
        // Ollama streams done_reason "stop" alongside tool_calls; the loop must
        // still run, so finish() must report ToolUse.
        let mut st = StreamState::default();
        for l in [
            r#"{"message":{"content":"Let me check."},"done":false}"#,
            r#"{"message":{"content":"","tool_calls":[{"function":{"name":"search_pages","arguments":{"query":"Thornhold"}}}]},"done":true,"done_reason":"stop"}"#,
        ] {
            st.ollama_line(l).unwrap();
        }
        let turn = st.finish();
        assert_eq!(turn.stop_reason, StopReason::ToolUse);
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].id, "call_0");
    }

    #[test]
    fn anthropic_stream_captures_cache_usage() {
        let mut st = StreamState::default();
        for l in [
            r#"data: {"type":"message_start","message":{"usage":{"input_tokens":12,"cache_read_input_tokens":8000,"cache_creation_input_tokens":0}}}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}"#,
            r#"data: {"type":"message_stop"}"#,
        ] {
            st.anthropic_line(l).unwrap();
        }
        let u = st.usage.as_ref().unwrap();
        assert_eq!(u["cache_read_input_tokens"], 8000);
        assert_eq!(u["input_tokens"], 12);
        assert_eq!(u["output_tokens"], 42);
    }

    #[test]
    fn openai_stream_captures_usage_chunk() {
        let mut st = StreamState::default();
        for l in [
            r#"data: {"choices":[{"delta":{"content":"hi"}}]}"#,
            r#"data: {"choices":[],"usage":{"prompt_tokens":1000,"prompt_tokens_details":{"cached_tokens":768}}}"#,
            "data: [DONE]",
        ] {
            st.openai_line(l).unwrap();
        }
        let u = st.usage.as_ref().unwrap();
        assert_eq!(u["prompt_tokens"], 1000);
        assert_eq!(u["prompt_tokens_details"]["cached_tokens"], 768);
    }

    #[test]
    fn ollama_done_captures_counts_skips_nested() {
        let mut st = StreamState::default();
        st.ollama_line(
            r#"{"message":{"content":""},"done":true,"done_reason":"stop","prompt_eval_count":900,"eval_count":50}"#,
        )
        .unwrap();
        let u = st.usage.as_ref().unwrap();
        assert_eq!(u["prompt_eval_count"], 900);
        assert_eq!(u["eval_count"], 50);
        // message object must not leak into the usage block.
        assert!(u.get("message").is_none());
    }

    #[test]
    fn anthropic_stream_error_propagates() {
        let mut st = StreamState::default();
        let err = st.anthropic_line(
            r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        );
        assert!(matches!(err, Err(LlmError(m)) if m == "Overloaded"));
    }

    #[test]
    fn malformed_tool_args_fall_back_to_empty_object() {
        assert_eq!(parse_args("{\"a\": "), json!({}));
        assert_eq!(parse_args(""), json!({}));
        assert_eq!(parse_args("{\"a\":1}"), json!({ "a": 1 }));
    }
}
