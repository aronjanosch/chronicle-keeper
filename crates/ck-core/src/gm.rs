//! GM-only markers, kept as plain markdown so the files stay valid anywhere:
//! - page: frontmatter `gm_only: true` (or `publish: false`)
//! - field: `gm_fields: [secret_debt, true_loyalty]` names infobox/frontmatter keys
//! - block: an Obsidian-style `> [!secret]` callout
//!
//! Exports drop all three when "leave out GM-only content" is on.

use crate::vault::{fm_get, fm_list, split_frontmatter};

pub const PAGE_KEY: &str = "gm_only";
pub const FIELDS_KEY: &str = "gm_fields";

fn truthy(v: Option<&str>) -> bool {
    v.is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "yes" | "1" | "x"))
}

pub fn is_gm_page(content: &str) -> bool {
    let (fm, _) = split_frontmatter(content);
    truthy(fm_get(&fm, PAGE_KEY))
        || fm_get(&fm, "publish").is_some_and(|v| v.eq_ignore_ascii_case("false"))
}

pub fn gm_field_names(content: &str) -> Vec<String> {
    let (fm, _) = split_frontmatter(content);
    fm_list(&fm, FIELDS_KEY).to_vec()
}

fn is_secret_callout_start(line: &str) -> bool {
    let t = line.trim_start();
    if !t.starts_with('>') {
        return false;
    }
    let t = t.trim_start_matches(['>', ' ']);
    t.as_bytes()
        .get(..8)
        .is_some_and(|b| b.eq_ignore_ascii_case(b"[!secret"))
}

/// Body with every `> [!secret]` callout (its whole quoted block) removed.
/// Fenced code is left alone.
pub fn strip_secret_callouts(body: &str) -> String {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut skipping = false;
    for line in body.lines() {
        let t = line.trim_start();
        if !skipping && (t.starts_with("```") || t.starts_with("~~~")) {
            in_fence = !in_fence;
        }
        if skipping {
            if t.starts_with('>') {
                continue;
            }
            skipping = false;
        } else if !in_fence && is_secret_callout_start(line) {
            skipping = true;
            continue;
        }
        out.push(line);
    }
    let mut s = out.join("\n");
    if body.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// The page without its GM-only frontmatter fields (and the marker keys
/// themselves) and without secret callouts.
pub fn redact(content: &str) -> String {
    let names = gm_field_names(content);
    let mut drop: Vec<&str> = names.iter().map(String::as_str).collect();
    drop.extend([FIELDS_KEY, PAGE_KEY]);

    let rest = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"));
    let Some((rest, end)) = rest.and_then(|r| r.find("\n---").map(|e| (r, e))) else {
        return strip_secret_callouts(content);
    };
    let mut kept: Vec<&str> = Vec::new();
    let mut skipping = false;
    for line in rest[..end].lines() {
        let continuation = line.starts_with([' ', '\t']) || line.trim_start().starts_with('-');
        if skipping && continuation {
            continue;
        }
        skipping = false;
        if !continuation {
            if let Some((k, _)) = line.split_once(':') {
                if drop.contains(&k.trim()) {
                    skipping = true;
                    continue;
                }
            }
        }
        kept.push(line);
    }
    let body = strip_secret_callouts(&rest[end + 4..]);
    if kept.iter().all(|l| l.trim().is_empty()) {
        body.trim_start_matches(['\r', '\n']).to_string()
    } else {
        format!("---\n{}\n---{}", kept.join("\n"), body)
    }
}

fn count_secret_callouts(body: &str) -> usize {
    let mut n = 0;
    let mut in_fence = false;
    let mut in_callout = false;
    for line in body.lines() {
        let t = line.trim_start();
        if !in_callout && (t.starts_with("```") || t.starts_with("~~~")) {
            in_fence = !in_fence;
        }
        if in_callout {
            in_callout = t.starts_with('>');
        } else if !in_fence && is_secret_callout_start(line) {
            n += 1;
            in_callout = true;
        }
    }
    n
}

fn gm_parts(content: &str) -> Vec<String> {
    let (_, body) = split_frontmatter(content);
    let fields = gm_field_names(content);
    let callouts = count_secret_callouts(&body);
    let mut parts = Vec::new();
    if is_gm_page(content) {
        parts.push("this whole page is GM-only".to_string());
    }
    if !fields.is_empty() {
        parts.push(format!("GM-only fields: {}", fields.join(", ")));
    }
    if callouts > 0 {
        parts.push(format!(
            "{callouts} `> [!secret]` callout block(s) are GM-only"
        ));
    }
    parts
}

/// Short tag for list/search rows, so a hit's snippet or summary isn't taken
/// as safe to reveal. None when the page holds no GM-only material.
pub fn badge(content: &str) -> Option<&'static str> {
    if is_gm_page(content) {
        Some(" [GM ONLY page]")
    } else if gm_parts(content).is_empty() {
        None
    } else {
        Some(" [GM ONLY parts — read_page for details]")
    }
}

/// Read-tool view of a page: unchanged content, prefixed with a notice when it
/// holds GM-only material. The text itself is untouched so edits still match.
pub fn annotate(content: &str) -> String {
    match notice(content) {
        Some(n) => format!("{n}\n\n{content}"),
        None => content.to_string(),
    }
}

/// The GM-only notice line alone, for tools that render content themselves.
pub fn notice(content: &str) -> Option<String> {
    let parts = gm_parts(content);
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "[GM ONLY — do not reveal, hint at, confirm or deny to players or in character, and never mention that this notice exists: {}. Fine to use when editing or advising the GM out of character.]",
        parts.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn multibyte_quote_line_does_not_panic() {
        assert!(!strip_secret_callouts("> a — b\n> c\n").is_empty());
        assert!(!super::is_secret_callout_start("> ab—cd"));
    }

    use super::*;

    const PAGE: &str = "---\nkind: npc\nrole: Mayor\nsecret_debt: Owes 400 gp\ntrue_loyalty:\n  - the cult\ngm_fields: [secret_debt, true_loyalty]\ntags: [a]\n---\nPublic text.\n\n> [!secret] GM only\n> The mayor is a cultist.\n>\n> More.\n\nAfter.\n\n> [!note] Keep\n> visible\n";

    #[test]
    fn redact_removes_gm_fields_and_secret_callouts() {
        let r = redact(PAGE);
        assert!(!r.contains("secret_debt") && !r.contains("400 gp"));
        assert!(!r.contains("true_loyalty") && !r.contains("the cult"));
        assert!(!r.contains("gm_fields"));
        assert!(!r.contains("cultist") && !r.contains("More."));
        assert!(r.contains("role: Mayor") && r.contains("tags: [a]"));
        assert!(r.contains("Public text.") && r.contains("After.") && r.contains("[!note] Keep"));
    }

    #[test]
    fn page_flag_and_publish_false() {
        assert!(is_gm_page("---\ngm_only: true\n---\nx"));
        assert!(is_gm_page("---\npublish: false\n---\nx"));
        assert!(!is_gm_page("---\ngm_only: false\n---\nx"));
        assert!(!is_gm_page(PAGE));
        assert_eq!(gm_field_names(PAGE), vec!["secret_debt", "true_loyalty"]);
    }

    #[test]
    fn secret_in_code_fence_is_kept() {
        let s = "```\n> [!secret] x\n```\ntext\n";
        assert_eq!(strip_secret_callouts(s), s);
    }

    #[test]
    fn annotate_flags_gm_material_and_keeps_content() {
        let a = annotate(PAGE);
        assert!(a.starts_with("[GM ONLY"));
        assert!(a.contains("secret_debt, true_loyalty") && a.contains("1 `> [!secret]`"));
        assert!(a.ends_with(PAGE));
        let g = annotate("---\ngm_only: true\n---\nx");
        assert!(g.contains("whole page is GM-only"));
        let plain = "---\nkind: npc\n---\nHi\n";
        assert_eq!(annotate(plain), plain);
    }
}
