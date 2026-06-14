//! The Keeper's markdown memory (keeper-memory-spec.md). One fact per file in
//! `.ck/keeper/memory/<slug>.md`; `MEMORY.md` is a one-line-per-fact index
//! injected into every chat system prompt. Files are truth — read fresh each
//! turn, no cache. World facts belong in Codex pages, never here.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// Beyond this the Keeper must consolidate or delete before adding more.
const MAX_MEMORIES: usize = 50;
const VALID_TYPES: [&str; 4] = ["preference", "task", "style", "correction"];

fn keeper_dir(world_root: &Path) -> PathBuf {
    world_root.join(".ck").join("keeper")
}

fn memory_dir(world_root: &Path) -> PathBuf {
    keeper_dir(world_root).join("memory")
}

fn index_path(world_root: &Path) -> PathBuf {
    keeper_dir(world_root).join("MEMORY.md")
}

/// Lowercase `[a-z0-9-]`, collapsed dashes — model-supplied names never reach
/// the filesystem verbatim (no traversal, no surprises).
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

struct Memory {
    slug: String,
    description: String,
    mtype: String,
    body: String,
}

fn parse(slug: &str, raw: &str) -> Memory {
    let mut description = String::new();
    let mut mtype = String::new();
    let body;
    if let Some(rest) = raw.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                if let Some(v) = line.strip_prefix("description:") {
                    description = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("type:") {
                    mtype = v.trim().to_string();
                }
            }
            body = rest[end + 4..].trim_start_matches('\n').to_string();
            return Memory {
                slug: slug.to_string(),
                description,
                mtype,
                body,
            };
        }
    }
    body = raw.to_string();
    Memory {
        slug: slug.to_string(),
        description,
        mtype,
        body,
    }
}

fn load_all(world_root: &Path) -> Vec<Memory> {
    let dir = memory_dir(world_root);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<Memory> = rd
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                return None;
            }
            let slug = path.file_stem()?.to_str()?.to_string();
            let raw = std::fs::read_to_string(&path).ok()?;
            Some(parse(&slug, &raw))
        })
        .collect();
    out.sort_by(|a, b| a.slug.cmp(&b.slug));
    out
}

/// Rewrite `MEMORY.md` from the current fact files — derived, never edited by
/// the model directly.
fn rebuild_index(world_root: &Path) {
    let mems = load_all(world_root);
    let mut body = String::from("# The Keeper's memory\n\n");
    if mems.is_empty() {
        body.push_str("_(empty)_\n");
    } else {
        for m in &mems {
            let desc = if m.description.is_empty() {
                "(no description)"
            } else {
                &m.description
            };
            body.push_str(&format!("- **{}** — {}\n", m.slug, desc));
        }
    }
    let _ = std::fs::write(index_path(world_root), body);
}

/// Layer 4: the index lines for the chat system prompt. Empty when there are
/// no memories.
pub fn index_block(world_root: &Path) -> String {
    let mems = load_all(world_root);
    if mems.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        "\n## Your memory — facts you have saved across chats\n\n\
         Call read_memory to read one in full when an entry looks relevant.\n\n",
    );
    for m in &mems {
        let desc = if m.description.is_empty() {
            "(no description)"
        } else {
            &m.description
        };
        s.push_str(&format!("- {} — {}\n", m.slug, desc));
    }
    s
}

pub fn read_memory(world_root: &Path, name: &str) -> Result<String, String> {
    let slug = slugify(name);
    if slug.is_empty() {
        return Err("invalid memory name".into());
    }
    std::fs::read_to_string(memory_dir(world_root).join(format!("{slug}.md")))
        .map_err(|_| format!("No memory named {slug}."))
}

pub fn write_memory(
    world_root: &Path,
    name: &str,
    description: &str,
    mtype: &str,
    content: &str,
) -> Result<String, String> {
    let slug = slugify(name);
    if slug.is_empty() {
        return Err("invalid memory name — use a short kebab-case label".into());
    }
    if content.trim().is_empty() {
        return Err("memory content is empty".into());
    }
    let dir = memory_dir(world_root);
    let path = dir.join(format!("{slug}.md"));
    if !path.exists() && load_all(world_root).len() >= MAX_MEMORIES {
        return Err(format!(
            "Memory is full ({MAX_MEMORIES}) — consolidate or delete_memory before adding more."
        ));
    }
    let mtype = if VALID_TYPES.contains(&mtype) {
        mtype
    } else {
        "preference"
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("create memory dir: {e}"))?;
    let file = format!(
        "---\nname: {slug}\ndescription: {}\ntype: {mtype}\n---\n\n{}\n",
        description.trim(),
        content.trim(),
    );
    std::fs::write(&path, file).map_err(|e| format!("write memory: {e}"))?;
    rebuild_index(world_root);
    Ok(format!("Remembered {slug}."))
}

pub fn delete_memory(world_root: &Path, name: &str) -> Result<String, String> {
    let slug = slugify(name);
    let path = memory_dir(world_root).join(format!("{slug}.md"));
    if !path.exists() {
        return Err(format!("No memory named {slug}."));
    }
    std::fs::remove_file(&path).map_err(|e| format!("delete memory: {e}"))?;
    rebuild_index(world_root);
    Ok(format!("Forgot {slug}."))
}

/// For the Keeper screen's Memory list.
pub fn list_json(world_root: &Path) -> Value {
    let items: Vec<Value> = load_all(world_root)
        .iter()
        .map(|m| {
            json!({
                "name": m.slug,
                "description": m.description,
                "type": m.mtype,
                "body": m.body,
            })
        })
        .collect();
    json!({ "memories": items })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-mem-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn slugify_strips_to_kebab() {
        assert_eq!(slugify("Terse Summaries!"), "terse-summaries");
        assert_eq!(slugify("  ../etc/passwd "), "etc-passwd");
        assert_eq!(slugify("a__b  c"), "a-b-c");
    }

    #[test]
    fn write_read_delete_roundtrip() {
        let root = tmp("rt");
        write_memory(
            &root,
            "Terse summaries",
            "Keep summaries short",
            "preference",
            "User likes short scene-structured summaries.",
        )
        .unwrap();
        let body = read_memory(&root, "terse-summaries").unwrap();
        assert!(body.contains("description: Keep summaries short"));
        assert!(body.contains("scene-structured"));

        let block = index_block(&root);
        assert!(block.contains("terse-summaries — Keep summaries short"));
        let index = std::fs::read_to_string(index_path(&root)).unwrap();
        assert!(index.contains("- **terse-summaries**"));

        delete_memory(&root, "terse-summaries").unwrap();
        assert!(read_memory(&root, "terse-summaries").is_err());
        assert!(index_block(&root).is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unknown_type_falls_back_and_cap_enforced() {
        let root = tmp("cap");
        write_memory(&root, "x", "d", "bogus", "c").unwrap();
        assert!(read_memory(&root, "x")
            .unwrap()
            .contains("type: preference"));
        for i in 0..MAX_MEMORIES {
            let _ = write_memory(&root, &format!("m{i}"), "d", "task", "c");
        }
        let err = write_memory(&root, "overflow", "d", "task", "c").unwrap_err();
        assert!(err.contains("full"));
        // Overwriting an existing one still works at the cap.
        assert!(write_memory(&root, "m0", "d2", "task", "c2").is_ok());
        std::fs::remove_dir_all(&root).ok();
    }
}
