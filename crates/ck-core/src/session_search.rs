//! Substring search over session summaries and raw transcripts (Phase 7d).
//! These records live as files under `Sessions/<NNN>/`, not in the page FTS
//! index, so this is a plain two-pass scan (whole phrase, then any token) —
//! the same shape the Keeper's `search_summaries`/`search_transcripts` agent
//! tools use, lifted to a structured result the search screen can render.

use std::path::Path;

use crate::agent::context::session_entries;
use crate::codex_update::transcript_turns;
use crate::session_files;

const MAX_HITS: usize = 50;
const SNIPPET_WINDOW: usize = 240;

#[derive(Debug, serde::Serialize)]
pub struct SessionHit {
    pub session: i64,
    pub title: String,
    /// 1-based transcript turn for transcript hits; `None` for summaries.
    pub turn: Option<usize>,
    /// HTML-escaped excerpt with the match wrapped in `<b>…</b>`.
    pub snippet: String,
}

fn tokens(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// Sessions newest-first, the order both screens want.
fn sessions_desc(world_root: &Path) -> Vec<(i64, String)> {
    let mut s: Vec<(i64, String)> = session_entries(world_root)
        .into_iter()
        .map(|(n, t, _)| (n, t))
        .collect();
    s.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    s
}

fn session_dir(world_root: &Path, number: i64) -> Option<std::path::PathBuf> {
    let rd = std::fs::read_dir(world_root.join("Sessions")).ok()?;
    for e in rd.flatten() {
        let dir = e.path();
        if let Ok(Some(st)) = session_files::read_session_toml(&dir) {
            if st.number == Some(number) {
                return Some(dir);
            }
        }
    }
    None
}

pub fn search_summaries(world_root: &Path, query: &str) -> Vec<SessionHit> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let toks = tokens(&query);
    let sessions = sessions_desc(world_root);
    let mut out = Vec::new();
    for pass in 0..2 {
        for (n, title) in &sessions {
            let Some(dir) = session_dir(world_root, *n) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(session_files::summary_md_path(&dir)) else {
                continue;
            };
            let lower = text.to_lowercase();
            let hit = if pass == 0 {
                lower.find(&query)
            } else {
                toks.iter().find_map(|t| lower.find(t))
            };
            if let Some(at) = hit {
                out.push(SessionHit {
                    session: *n,
                    title: title.clone(),
                    turn: None,
                    snippet: snippet_html(&text, at),
                });
                if out.len() >= MAX_HITS {
                    return out;
                }
            }
        }
        if !out.is_empty() || toks.len() < 2 {
            break;
        }
    }
    out
}

pub fn search_transcripts(world_root: &Path, query: &str) -> Vec<SessionHit> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let toks = tokens(&query);
    let sessions = sessions_desc(world_root);
    let mut out = Vec::new();
    for pass in 0..2 {
        for (n, title) in &sessions {
            let Some(dir) = session_dir(world_root, *n) else {
                continue;
            };
            let Ok(raw) = std::fs::read_to_string(session_files::transcript_md_path(&dir)) else {
                continue;
            };
            for (i, t) in transcript_turns(&raw).iter().enumerate() {
                let lower = t.to_lowercase();
                let hit = if pass == 0 {
                    lower.find(&query)
                } else {
                    toks.iter().find_map(|tok| lower.find(tok))
                };
                if let Some(at) = hit {
                    out.push(SessionHit {
                        session: *n,
                        title: title.clone(),
                        turn: Some(i + 1),
                        snippet: snippet_html(t, at),
                    });
                    if out.len() >= MAX_HITS {
                        return out;
                    }
                }
            }
        }
        if !out.is_empty() || toks.len() < 2 {
            break;
        }
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Excerpt centered on a byte offset, HTML-escaped, leading/trailing ellipses,
/// matched word bolded by re-finding it inside the (smaller) window.
fn snippet_html(text: &str, at: usize) -> String {
    let at = at.min(text.len());
    let half = SNIPPET_WINDOW / 2;
    let mut start = at.saturating_sub(half);
    let mut end = (at + half).min(text.len());
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    let slice = text[start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut html = escape(&slice);
    // Bold the word the match landed on (best-effort; cosmetic only).
    if let Some(word) = text[at..].split_whitespace().next() {
        let esc = escape(word);
        if !esc.is_empty() {
            html = html.replacen(&esc, &format!("<b>{esc}</b>"), 1);
        }
    }
    let lead = if start > 0 { "…" } else { "" };
    let trail = if end < text.len() { "…" } else { "" };
    format!("{lead}{html}{trail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_world(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-ssearch-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        dir
    }

    fn write_session(world: &Path, n: i64, title: &str, summary: &str, transcript: &str) {
        let dir = world.join(format!("Sessions/{n:03}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("session.toml"),
            format!("number = {n}\ntitle = \"{title}\"\n"),
        )
        .unwrap();
        std::fs::write(session_files::summary_md_path(&dir), summary).unwrap();
        std::fs::write(session_files::transcript_md_path(&dir), transcript).unwrap();
    }

    #[test]
    fn finds_summary_and_transcript() {
        let world = tmp_world("basic");
        write_session(
            &world,
            1,
            "Arrival",
            "The party reached Thornhold at dusk.",
            "[Mara]\nWe should ask the baron.\n",
        );
        write_session(
            &world,
            2,
            "Cellars",
            "They explored the cellars.",
            "[GM]\nThe baron glares at you.\n",
        );

        let s = search_summaries(&world, "Thornhold");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].session, 1);
        assert!(s[0].snippet.contains("<b>Thornhold</b>"));

        let t = search_transcripts(&world, "baron");
        assert_eq!(t.len(), 2);
        // newest-first
        assert_eq!(t[0].session, 2);
        assert!(t[0].turn.is_some());

        assert!(search_summaries(&world, "dragon").is_empty());
        std::fs::remove_dir_all(&world).ok();
    }
}
