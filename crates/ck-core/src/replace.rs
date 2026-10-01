//! World-wide find & replace (Phase 33B): Codex pages plus session
//! `summary.md` / `transcript.md`. A dry run lists hits; apply journals every
//! touched file's original under `.ck/replace/<id>.json` so one call undoes it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::{history, vault};

const KEEP_JOURNALS: usize = 10;
const MAX_SAMPLES: usize = 3;
const SAMPLE_CHARS: usize = 160;

#[derive(Deserialize, Clone)]
pub struct Options {
    pub find: String,
    #[serde(default)]
    pub replace: String,
    #[serde(default = "yes")]
    pub whole_word: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default = "yes")]
    pub pages: bool,
    #[serde(default = "yes")]
    pub sessions: bool,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Debug, PartialEq)]
pub struct FileHit {
    /// "page" | "transcript" | "summary"
    pub kind: String,
    /// Codex-relative path for pages, world-relative for session files.
    pub path: String,
    pub count: usize,
    pub samples: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct Plan {
    pub total: usize,
    pub files: Vec<FileHit>,
}

#[derive(Serialize, Deserialize)]
struct JournalFile {
    abs: PathBuf,
    /// Codex-relative path when the file is a page (reindex after undo).
    page: Option<String>,
    original: String,
}

#[derive(Serialize, Deserialize)]
struct Journal {
    id: String,
    find: String,
    replace: String,
    files: Vec<JournalFile>,
}

#[derive(Serialize)]
pub struct Applied {
    pub id: String,
    pub files: usize,
    pub replaced: usize,
    pub pages: Vec<String>,
}

fn same_char(a: char, b: char, case_sensitive: bool) -> bool {
    a == b || (!case_sensitive && a.to_lowercase().eq(b.to_lowercase()))
}

/// Byte ranges of every match, skipping `[Speaker]` label lines.
fn find_matches(text: &str, o: &Options) -> Vec<(usize, usize)> {
    let needle: Vec<char> = o.find.chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut label_ranges = Vec::new();
    let mut off = 0;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            label_ranges.push((off, off + line.len()));
        }
        off += line.len();
    }
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= chars.len() {
        let hit = (0..needle.len()).all(|j| same_char(chars[i + j].1, needle[j], o.case_sensitive));
        let start = chars[i].0;
        let end = chars.get(i + needle.len()).map_or(text.len(), |(b, _)| *b);
        let bounded = !o.whole_word
            || ((i == 0 || !word(chars[i - 1].1))
                && chars.get(i + needle.len()).is_none_or(|(_, c)| !word(*c)));
        if hit && bounded && !label_ranges.iter().any(|(s, e)| start >= *s && start < *e) {
            out.push((start, end));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

fn sample(text: &str, (start, end): (usize, usize)) -> String {
    let line_start = text[..start].rfind('\n').map_or(0, |p| p + 1);
    let line_end = text[end..].find('\n').map_or(text.len(), |p| end + p);
    let line = &text[line_start..line_end];
    if line.chars().count() <= SAMPLE_CHARS {
        return line.trim().to_string();
    }
    let rel = start - line_start;
    let from = line[..rel]
        .char_indices()
        .rev()
        .nth(SAMPLE_CHARS / 2)
        .map_or(0, |(b, _)| b);
    let to = line[rel..]
        .char_indices()
        .nth(SAMPLE_CHARS / 2)
        .map_or(line.len(), |(b, _)| rel + b);
    format!("…{}…", line[from..to].trim())
}

fn apply_to(text: &str, matches: &[(usize, usize)], with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for &(s, e) in matches {
        out.push_str(&text[cursor..s]);
        out.push_str(with);
        cursor = e;
    }
    out.push_str(&text[cursor..]);
    out
}

struct Candidate {
    kind: &'static str,
    path: String,
    abs: PathBuf,
    page: Option<String>,
}

fn candidates(world_root: &Path, codex_root: &Path, o: &Options) -> Vec<Candidate> {
    let mut out = Vec::new();
    if o.pages {
        for p in vault::list_pages(codex_root).unwrap_or_default() {
            out.push(Candidate {
                kind: "page",
                abs: codex_root.join(&p.path),
                page: Some(p.path.clone()),
                path: p.path,
            });
        }
    }
    if o.sessions {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(world_root.join("Sessions"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let name = dir.file_name().map(|n| n.to_string_lossy().into_owned());
            for (kind, file) in [("transcript", "transcript.md"), ("summary", "summary.md")] {
                let abs = dir.join(file);
                if abs.is_file() {
                    out.push(Candidate {
                        kind,
                        path: format!("Sessions/{}/{file}", name.clone().unwrap_or_default()),
                        abs,
                        page: None,
                    });
                }
            }
        }
    }
    out
}

fn check(o: &Options) -> AppResult<()> {
    if o.find.is_empty() {
        return Err(AppError::BadRequest("find text is required".into()));
    }
    if o.find == o.replace {
        return Err(AppError::BadRequest(
            "find and replace are identical".into(),
        ));
    }
    Ok(())
}

pub fn plan(world_root: &Path, codex_root: &Path, o: &Options) -> AppResult<Plan> {
    check(o)?;
    let mut files = Vec::new();
    let mut total = 0;
    for c in candidates(world_root, codex_root, o) {
        let Ok(text) = std::fs::read_to_string(&c.abs) else {
            continue;
        };
        let m = find_matches(&text, o);
        if m.is_empty() {
            continue;
        }
        total += m.len();
        files.push(FileHit {
            kind: c.kind.into(),
            path: c.path,
            count: m.len(),
            samples: m
                .iter()
                .take(MAX_SAMPLES)
                .map(|r| sample(&text, *r))
                .collect(),
        });
    }
    Ok(Plan { total, files })
}

fn journal_dir(world_root: &Path) -> PathBuf {
    world_root.join(".ck").join("replace")
}

fn io_err(what: &str) -> impl Fn(std::io::Error) -> AppError + '_ {
    move |e| AppError::Internal(anyhow::anyhow!("{what}: {e}"))
}

pub fn apply(world_root: &Path, codex_root: &Path, o: &Options) -> AppResult<Applied> {
    check(o)?;
    let mut journal = Vec::new();
    let mut staged = Vec::new();
    let mut replaced = 0;
    for c in candidates(world_root, codex_root, o) {
        let Ok(text) = std::fs::read_to_string(&c.abs) else {
            continue;
        };
        let m = find_matches(&text, o);
        if m.is_empty() {
            continue;
        }
        replaced += m.len();
        staged.push((apply_to(&text, &m, &o.replace), c.abs.clone()));
        journal.push(JournalFile {
            abs: c.abs,
            page: c.page,
            original: text,
        });
    }
    if journal.is_empty() {
        return Ok(Applied {
            id: String::new(),
            files: 0,
            replaced: 0,
            pages: Vec::new(),
        });
    }
    // Journal first: a crash mid-apply must still be undoable.
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_default();
    let dir = journal_dir(world_root);
    std::fs::create_dir_all(&dir).map_err(io_err("create replace journal dir"))?;
    let pages: Vec<String> = journal.iter().filter_map(|f| f.page.clone()).collect();
    let files = journal.len();
    let j = Journal {
        id: id.clone(),
        find: o.find.clone(),
        replace: o.replace.clone(),
        files: journal,
    };
    std::fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_string(&j).map_err(|e| AppError::Internal(e.into()))?,
    )
    .map_err(io_err("write replace journal"))?;
    for page in &pages {
        let _ = history::record_now(world_root, codex_root, page, "user");
    }
    for (text, abs) in staged {
        std::fs::write(&abs, text).map_err(io_err("write file"))?;
    }
    prune(&dir);
    Ok(Applied {
        id,
        files,
        replaced,
        pages,
    })
}

fn prune(dir: &Path) {
    let mut ids: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".json")
                .map(String::from)
        })
        .collect();
    ids.sort_by_key(|i| i.parse::<u128>().unwrap_or(0));
    while ids.len() > KEEP_JOURNALS {
        let _ = std::fs::remove_file(dir.join(format!("{}.json", ids.remove(0))));
    }
}

/// Put every file back to its journalled original. Returns the page paths
/// touched, so the caller can reindex. Refuses files outside the world.
pub fn undo(world_root: &Path, codex_root: &Path, id: &str) -> AppResult<Vec<String>> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::BadRequest("invalid undo id".into()));
    }
    let path = journal_dir(world_root).join(format!("{id}.json"));
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| AppError::NotFound("Nothing to undo for that id".into()))?;
    let j: Journal = serde_json::from_str(&raw).map_err(|e| AppError::Internal(e.into()))?;
    let inside = |p: &Path| p.starts_with(world_root) || p.starts_with(codex_root);
    if j.files
        .iter()
        .any(|f| !inside(&f.abs) || f.abs.components().any(|c| c.as_os_str() == ".."))
    {
        return Err(AppError::BadRequest(
            "journal points outside the world".into(),
        ));
    }
    let mut pages = Vec::new();
    for f in &j.files {
        if let Some(page) = &f.page {
            let _ = history::record_now(world_root, codex_root, page, "user");
            pages.push(page.clone());
        }
        std::fs::write(&f.abs, &f.original).map_err(io_err("restore file"))?;
    }
    let _ = std::fs::remove_file(path);
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(find: &str, replace: &str) -> Options {
        Options {
            find: find.into(),
            replace: replace.into(),
            whole_word: true,
            case_sensitive: false,
            pages: true,
            sessions: true,
        }
    }

    #[test]
    fn whole_word_and_case() {
        let t = "Brannick and brannick, Brannicks.";
        assert_eq!(find_matches(t, &opts("Brannick", "x")).len(), 2);
        let mut o = opts("Brannick", "x");
        o.whole_word = false;
        assert_eq!(find_matches(t, &o).len(), 3);
        o.case_sensitive = true;
        assert_eq!(find_matches(t, &o).len(), 2);
    }

    #[test]
    fn speaker_labels_are_protected() {
        let t = "[Bran]\nBran sprach.\n";
        assert_eq!(find_matches(t, &opts("Bran", "Brannik")).len(), 1);
    }

    #[test]
    fn unicode_boundaries_and_replace() {
        let t = "Über Ælfric, ælfric!";
        let m = find_matches(t, &opts("ælfric", "Ælfrik"));
        assert_eq!(m.len(), 2);
        assert_eq!(apply_to(t, &m, "Ælfrik"), "Über Ælfrik, Ælfrik!");
    }

    fn world(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("ck-repl-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let codex = root.join("Codex");
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::create_dir_all(root.join("Sessions/001")).unwrap();
        (root, codex)
    }

    #[test]
    fn plan_apply_undo_roundtrip() {
        let (root, codex) = world("rt");
        std::fs::write(
            codex.join("Hof.md"),
            "Brannick lebt hier.\nZweimal Brannick.\n",
        )
        .unwrap();
        std::fs::write(codex.join("Rest.md"), "nichts\n").unwrap();
        std::fs::write(
            root.join("Sessions/001/transcript.md"),
            "[Spieler]\nBrannick rief.\n",
        )
        .unwrap();
        std::fs::write(root.join("Sessions/001/summary.md"), "Brannick half.").unwrap();
        let o = opts("Brannick", "Brannik");

        let p = plan(&root, &codex, &o).unwrap();
        assert_eq!(p.total, 4);
        assert_eq!(p.files.len(), 3);
        assert_eq!(p.files[0].samples[0], "Brannick lebt hier.");
        // dry run touched nothing
        assert!(std::fs::read_to_string(codex.join("Hof.md"))
            .unwrap()
            .contains("Brannick"));

        let a = apply(&root, &codex, &o).unwrap();
        assert_eq!((a.files, a.replaced), (3, 4));
        assert_eq!(
            std::fs::read_to_string(codex.join("Hof.md")).unwrap(),
            "Brannik lebt hier.\nZweimal Brannik.\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("Sessions/001/transcript.md")).unwrap(),
            "[Spieler]\nBrannik rief.\n"
        );
        assert_eq!(
            std::fs::read_to_string(codex.join("Rest.md")).unwrap(),
            "nichts\n"
        );

        let pages = undo(&root, &codex, &a.id).unwrap();
        assert_eq!(pages, vec!["Hof.md".to_string()]);
        assert!(std::fs::read_to_string(codex.join("Hof.md"))
            .unwrap()
            .contains("Brannick"));
        assert_eq!(
            std::fs::read_to_string(root.join("Sessions/001/summary.md")).unwrap(),
            "Brannick half."
        );
        assert!(undo(&root, &codex, &a.id).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scope_flags_and_identical_input() {
        let (root, codex) = world("scope");
        std::fs::write(codex.join("A.md"), "Bran\n").unwrap();
        std::fs::write(root.join("Sessions/001/summary.md"), "Bran").unwrap();
        let mut o = opts("Bran", "Brannik");
        o.sessions = false;
        assert_eq!(plan(&root, &codex, &o).unwrap().files.len(), 1);
        assert!(plan(&root, &codex, &opts("Bran", "Bran")).is_err());
        assert!(plan(&root, &codex, &opts("", "x")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn undo_refuses_bad_ids() {
        let (root, codex) = world("bad");
        assert!(undo(&root, &codex, "../x").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
