//! The Overview "Unfinished" card: what the Keeper already reads as open —
//! stubs, `[?]` markers, unresolved links, cold threads — as one ranked list.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::vault::PageInfo;

pub const STALE_DAYS: u64 = 30;

#[derive(Debug, Serialize, PartialEq)]
pub struct GapRow {
    /// Stable id for dismissal: `page:<path>` or `link:<normalized name>`.
    pub key: String,
    /// `page` or `link`.
    pub row: &'static str,
    pub path: Option<String>,
    pub title: String,
    pub kind: Option<String>,
    pub stub: bool,
    pub open_questions: usize,
    pub stale_days: Option<u64>,
    pub from_count: usize,
    pub sources: Vec<String>,
}

fn link_name(link_text: &str) -> String {
    let t = link_text.split('|').next().unwrap_or(link_text);
    t.split('#').next().unwrap_or(t).trim().to_string()
}

/// `open_threads`: paths of `kind: thread` pages whose status is still open,
/// so a resolved or dormant thread going quiet is not a gap.
pub fn compute(
    pages: &[PageInfo],
    unresolved: &[(String, String)],
    open_threads: &HashSet<String>,
    now: u64,
) -> Vec<GapRow> {
    let mut rows = Vec::new();
    for p in pages {
        if !crate::vault::is_canon_kind(p.kind.as_deref()) {
            continue;
        }
        let stale_days = p
            .modified
            .filter(|m| now > *m)
            .map(|m| (now - m) / 86_400)
            .filter(|d| *d > STALE_DAYS);
        let cold_thread = stale_days.is_some() && open_threads.contains(&p.path);
        if !(p.is_stub || p.open_questions > 0 || cold_thread) {
            continue;
        }
        rows.push(GapRow {
            key: format!("page:{}", p.path),
            row: "page",
            path: Some(p.path.clone()),
            title: p.title.clone(),
            kind: p.kind.clone(),
            stub: p.is_stub,
            open_questions: p.open_questions,
            stale_days,
            from_count: 0,
            sources: Vec::new(),
        });
    }
    // Stubs and questions first, then colder pages.
    rows.sort_by(|a, b| {
        (b.stub as u8 + (b.open_questions > 0) as u8)
            .cmp(&(a.stub as u8 + (a.open_questions > 0) as u8))
            .then(b.open_questions.cmp(&a.open_questions))
            .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    let mut links: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for (src, text) in unresolved {
        let name = link_name(text);
        if name.is_empty() {
            continue;
        }
        let e = links
            .entry(crate::store::index::normalize_name(&name))
            .or_insert_with(|| (name, Vec::new()));
        if !e.1.contains(src) {
            e.1.push(src.clone());
        }
    }
    let mut link_rows: Vec<GapRow> = links
        .into_iter()
        .map(|(norm, (name, sources))| GapRow {
            key: format!("link:{norm}"),
            row: "link",
            path: None,
            title: name,
            kind: None,
            stub: false,
            open_questions: 0,
            stale_days: None,
            from_count: sources.len(),
            sources,
        })
        .collect();
    link_rows.sort_by(|a, b| b.from_count.cmp(&a.from_count).then(a.title.cmp(&b.title)));
    rows.extend(link_rows);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(path: &str, kind: &str, stub: bool, open: usize, age_days: u64) -> PageInfo {
        PageInfo {
            path: path.into(),
            title: path.trim_end_matches(".md").into(),
            kind: Some(kind.into()),
            summary: String::new(),
            modified: Some(1_000_000_000 - age_days * 86_400),
            open_questions: open,
            is_stub: stub,
        }
    }

    #[test]
    fn collects_stubs_questions_cold_threads_and_grouped_links() {
        let pages = vec![
            page("Stub.md", "npc", true, 0, 1),
            page("Q.md", "faction", false, 2, 40),
            page("Fine.md", "npc", false, 0, 90),
            page("Cold.md", "thread", false, 0, 45),
            page("Done.md", "thread", false, 0, 45),
            page("Prep.md", "prep", true, 0, 1),
        ];
        let open: HashSet<String> = ["Cold.md".to_string()].into();
        let links = vec![
            ("A.md".into(), "Cistern".into()),
            ("B.md".into(), "cistern|the well".into()),
            ("B.md".into(), "Cistern#Deep".into()),
        ];
        let rows = compute(&pages, &links, &open, 1_000_000_000);
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys,
            ["page:Q.md", "page:Stub.md", "page:Cold.md", "link:cistern"]
        );
        assert_eq!(rows[0].stale_days, Some(40));
        assert_eq!(rows[3].from_count, 2);
        assert_eq!(rows[3].title, "Cistern");
    }
}
