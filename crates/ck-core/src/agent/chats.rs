//! Chat persistence: `.ck/chats/<uuid>.jsonl`, one JSON event per line.
//! Append-only; reload = replay. Travels with the world folder, holds no keys.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::llm::agent::{Msg, ToolCall};

const TITLE_MAX: usize = 60;

#[derive(Debug, serde::Serialize)]
pub struct ChatMeta {
    pub id: String,
    pub title: String,
    pub updated_at: Option<String>,
    pub message_count: usize,
}

fn chats_dir(world_root: &Path) -> PathBuf {
    world_root.join(".ck").join("chats")
}

/// Reject anything that isn't a bare uuid-ish file stem (traversal guard).
fn chat_path(world_root: &Path, chat_id: &str) -> AppResult<PathBuf> {
    let ok = !chat_id.is_empty()
        && chat_id.len() <= 64
        && chat_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !ok {
        return Err(AppError::BadRequest(format!("Invalid chat id: {chat_id}")));
    }
    Ok(chats_dir(world_root).join(format!("{chat_id}.jsonl")))
}

pub fn create_chat(world_root: &Path) -> AppResult<ChatMeta> {
    let dir = chats_dir(world_root);
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create .ck/chats: {e}")))?;
    let id = uuid::Uuid::new_v4().to_string();
    std::fs::write(dir.join(format!("{id}.jsonl")), "")
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create chat: {e}")))?;
    Ok(ChatMeta {
        id,
        title: "New chat".into(),
        updated_at: None,
        message_count: 0,
    })
}

pub fn list_chats(world_root: &Path) -> AppResult<Vec<ChatMeta>> {
    let dir = chats_dir(world_root);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<(std::time::SystemTime, ChatMeta)> = Vec::new();
    for e in rd.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let events = load_events(&path);
        let mtime = e
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        out.push((mtime, meta_of(id, &events)));
    }
    out.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
    Ok(out.into_iter().map(|(_, m)| m).collect())
}

pub fn load_chat(world_root: &Path, chat_id: &str) -> AppResult<Vec<Value>> {
    let path = chat_path(world_root, chat_id)?;
    if !path.exists() {
        return Err(AppError::NotFound(format!("Chat not found: {chat_id}")));
    }
    Ok(load_events(&path))
}

pub fn append(world_root: &Path, chat_id: &str, event: &Value) -> AppResult<()> {
    use std::io::Write;
    let path = chat_path(world_root, chat_id)?;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("open chat {chat_id}: {e}")))?;
    let line = serde_json::to_string(event)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize chat event: {e}")))?;
    writeln!(f, "{line}").map_err(|e| AppError::Internal(anyhow::anyhow!("append chat: {e}")))
}

pub fn delete_chat(world_root: &Path, chat_id: &str) -> AppResult<()> {
    let path = chat_path(world_root, chat_id)?;
    if !path.exists() {
        return Err(AppError::NotFound(format!("Chat not found: {chat_id}")));
    }
    std::fs::remove_file(&path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("delete chat: {e}")))?;
    super::checkpoints::delete_for_chat(world_root, chat_id);
    super::attachments::delete_for_chat(world_root, chat_id);
    Ok(())
}

fn load_events(path: &Path) -> Vec<Value> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn meta_of(id: &str, events: &[Value]) -> ChatMeta {
    let first_user = events
        .iter()
        .find(|e| e["type"] == "user")
        .and_then(|e| e["text"].as_str());
    let title = first_user
        .map(|t| {
            let t = t.trim().replace('\n', " ");
            if t.chars().count() > TITLE_MAX {
                let cut: String = t.chars().take(TITLE_MAX).collect();
                format!("{}…", cut.trim_end())
            } else {
                t
            }
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "New chat".into());
    let updated_at = events
        .iter()
        .rev()
        .find_map(|e| e["at"].as_str())
        .map(str::to_string);
    let message_count = events
        .iter()
        .filter(|e| e["type"] == "user" || e["type"] == "assistant")
        .count();
    ChatMeta {
        id: id.to_string(),
        title,
        updated_at,
        message_count,
    }
}

// ── jsonl events ↔ LLM message history ───────────────────────────

pub fn user_event(text: &str, images: &[crate::llm::agent::Image]) -> Value {
    let mut ev = json!({ "type": "user", "text": text, "at": crate::store::now() });
    if !images.is_empty() {
        ev["images"] = serde_json::to_value(images).unwrap_or(Value::Null);
    }
    ev
}

pub fn assistant_event(text: &str, tool_calls: &[ToolCall]) -> Value {
    json!({
        "type": "assistant",
        "text": text,
        "tool_calls": tool_calls,
        "at": crate::store::now(),
    })
}

pub fn tool_result_event(
    call_id: &str,
    name: &str,
    content: &str,
    is_error: bool,
    diff: Option<&Value>,
) -> Value {
    let mut ev = json!({
        "type": "tool_result",
        "call_id": call_id,
        "name": name,
        "content": content,
        "is_error": is_error,
        "at": crate::store::now(),
    });
    if let Some(d) = diff {
        ev["diff"] = d.clone();
    }
    ev
}

pub fn permission_event(
    request_id: &str,
    name: &str,
    diff: &Value,
    decision: super::Decision,
) -> Value {
    let decision = match decision {
        super::Decision::AllowOnce => "allow_once",
        super::Decision::AllowChat => "allow_chat",
        super::Decision::Deny => "deny",
    };
    json!({
        "type": "permission",
        "request_id": request_id,
        "name": name,
        "diff": diff,
        "decision": decision,
        "at": crate::store::now(),
    })
}

pub fn error_event(message: &str) -> Value {
    json!({ "type": "error", "message": message, "at": crate::store::now() })
}

/// Mode-change marker (e.g. grounded fallback engaged). UI-only, skipped on replay.
pub fn notice_event(message: &str) -> Value {
    json!({ "type": "notice", "message": message, "at": crate::store::now() })
}

pub fn aborted_event() -> Value {
    json!({ "type": "aborted", "at": crate::store::now() })
}

/// Replay persisted events into LLM messages. Error/abort markers are
/// UI-only and skipped.
pub fn events_to_msgs(events: &[Value]) -> Vec<Msg> {
    events
        .iter()
        .filter_map(|e| match e["type"].as_str() {
            Some("user") => {
                let text = e["text"].as_str().unwrap_or("").to_string();
                let images: Vec<crate::llm::agent::Image> =
                    serde_json::from_value(e["images"].clone()).unwrap_or_default();
                Some(if images.is_empty() {
                    Msg::User(text)
                } else {
                    Msg::UserImages { text, images }
                })
            }
            Some("assistant") => Some(Msg::Assistant {
                text: e["text"].as_str().unwrap_or("").to_string(),
                tool_calls: serde_json::from_value(e["tool_calls"].clone()).unwrap_or_default(),
            }),
            Some("tool_result") => Some(Msg::ToolResult {
                call_id: e["call_id"].as_str().unwrap_or("").to_string(),
                name: e["name"].as_str().unwrap_or("").to_string(),
                content: e["content"].as_str().unwrap_or("").to_string(),
                is_error: e["is_error"].as_bool().unwrap_or(false),
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_world(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-chats-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn chat_lifecycle() {
        let root = tmp_world("life");
        let chat = create_chat(&root).unwrap();
        append(&root, &chat.id, &user_event("Who rules Thornhold and why does it matter for the party right now?", &[])).unwrap();
        append(&root, &chat.id, &assistant_event("Baron Aldric.", &[])).unwrap();

        let list = list_chats(&root).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].message_count, 2);
        assert!(list[0].title.starts_with("Who rules Thornhold"));
        assert!(list[0].title.ends_with('…'));
        assert!(list[0].title.chars().count() <= TITLE_MAX + 1);

        let events = load_chat(&root, &chat.id).unwrap();
        let msgs = events_to_msgs(&events);
        assert_eq!(msgs.len(), 2);
        assert!(matches!(&msgs[0], Msg::User(t) if t.starts_with("Who rules")));

        delete_chat(&root, &chat.id).unwrap();
        assert!(load_chat(&root, &chat.id).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn tool_events_replay_into_msgs() {
        let root = tmp_world("replay");
        let chat = create_chat(&root).unwrap();
        let call = ToolCall {
            id: "c1".into(),
            name: "search_pages".into(),
            arguments: json!({ "query": "x" }),
        };
        append(&root, &chat.id, &user_event("q", &[])).unwrap();
        append(&root, &chat.id, &assistant_event("", std::slice::from_ref(&call))).unwrap();
        append(&root, &chat.id, &tool_result_event("c1", "search_pages", "hit", false, None)).unwrap();
        append(&root, &chat.id, &error_event("boom")).unwrap();

        let msgs = events_to_msgs(&load_chat(&root, &chat.id).unwrap());
        assert_eq!(msgs.len(), 3); // error marker skipped
        assert!(matches!(&msgs[1], Msg::Assistant { tool_calls, .. } if tool_calls[0].id == "c1"));
        assert!(matches!(&msgs[2], Msg::ToolResult { call_id, .. } if call_id == "c1"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn bad_chat_ids_rejected() {
        let root = tmp_world("ids");
        assert!(load_chat(&root, "../evil").is_err());
        assert!(load_chat(&root, "a/b").is_err());
        assert!(load_chat(&root, "").is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}
