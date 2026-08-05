//! `/compact`: summarize a chat's history into a single boundary so the replayed
//! context resets. The summary is carried forward; the full log stays on disk.

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::llm::agent::Msg;
use crate::llm::{self, Resolved};

/// Cap each rendered message so the summarization prompt stays bounded even for
/// a chat full of large tool results.
const PER_MSG_CAP: usize = 1200;

const PROMPT: &str = "You are compacting a conversation between a user and the Keeper — an AI \
worldbuilding assistant for a tabletop campaign — so it can continue with the earlier turns \
dropped from context. Write a summary that captures everything needed to carry on seamlessly:\n\
- What the user is trying to do: the current task or goal and where it stands.\n\
- Key facts, decisions, and conventions established in the conversation.\n\
- World entities discussed (NPCs, places, factions, items, sessions) and any pages created or edited.\n\
- Open threads and the next steps still to do.\n\
- Any user preferences about how to work that came up.\n\
Be thorough on substance but concise. Do not invent anything that isn't in the conversation. \
Output only the summary.";

fn render(msgs: &[Msg]) -> String {
    let mut out = String::new();
    for m in msgs {
        let (role, body) = match m {
            Msg::System(_) => continue,
            Msg::User(s) => ("User", s.as_str()),
            Msg::UserImages { text, .. } => ("User", text.as_str()),
            Msg::Assistant { text, .. } => ("Keeper", text.as_str()),
            Msg::ToolResult { content, .. } => ("Tool result", content.as_str()),
        };
        if body.trim().is_empty() {
            continue;
        }
        let body = if body.len() > PER_MSG_CAP {
            let mut end = PER_MSG_CAP;
            while !body.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}…", &body[..end])
        } else {
            body.to_string()
        };
        out.push_str(role);
        out.push_str(": ");
        out.push_str(&body);
        out.push_str("\n\n");
    }
    out
}

/// Summarize the chat's effective context (respecting any prior compaction) and
/// append a compact boundary. Returns the summary text.
pub async fn run_compact(
    world_root: &Path,
    chat_id: &str,
    resolved: &Resolved,
) -> AppResult<String> {
    let events = super::chats::load_chat(world_root, chat_id)?;
    let msgs = super::chats::events_to_msgs(&events);
    if !msgs.iter().any(|m| matches!(m, Msg::Assistant { .. })) {
        return Err(AppError::BadRequest("Nothing to compact yet.".into()));
    }
    let prompt = format!("{PROMPT}\n\n---\nConversation:\n\n{}", render(&msgs));
    let summary = llm::chat(&resolved.chat_req(&prompt), false)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Compaction failed: {}", e.0)))?;
    let summary = summary.trim().to_string();
    if summary.is_empty() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "The model returned an empty summary."
        )));
    }
    super::chats::append(world_root, chat_id, &super::chats::compact_event(&summary))?;
    Ok(summary)
}
