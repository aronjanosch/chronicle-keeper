//! Durable session preparation stored as human-editable `prep.md`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

const SCHEMA_VERSION: u32 = 1;
const MAX_CARDS: usize = 200;
const MAX_CARD_CHARS: usize = 20_000;
const MAX_LINKS: usize = 200;
const MAX_NOTES_CHARS: usize = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrepSection {
    Opening,
    Scene,
    Reminder,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrepOutcome {
    #[default]
    Unmarked,
    Happened,
    Changed,
    Unused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepOrigin {
    pub session_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepCard {
    pub id: Option<String>,
    pub section: PrepSection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub text: String,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub outcome: PrepOutcome,
    #[serde(default)]
    pub outcome_note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<PrepOrigin>,
    #[serde(flatten)]
    extra: BTreeMap<String, YamlValue>,
}

impl PrepCard {
    pub fn new(section: PrepSection, text: impl Into<String>) -> Self {
        Self {
            id: None,
            section,
            title: None,
            text: text.into(),
            links: Vec::new(),
            outcome: PrepOutcome::Unmarked,
            outcome_note: String::new(),
            origin: None,
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrepDocument {
    ck_prep_version: u32,
    #[serde(default)]
    cards: Vec<PrepCard>,
    #[serde(default)]
    selected_threads: Vec<String>,
    #[serde(default)]
    handoffs: Vec<YamlValue>,
    #[serde(flatten)]
    extra: BTreeMap<String, YamlValue>,
}

impl Default for PrepDocument {
    fn default() -> Self {
        Self {
            ck_prep_version: SCHEMA_VERSION,
            cards: Vec::new(),
            selected_threads: Vec::new(),
            handoffs: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepResponse {
    pub revision: String,
    pub cards: Vec<PrepCard>,
    pub selected_threads: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutPrepRequest {
    pub base_revision: String,
    pub cards: Vec<PrepCard>,
    #[serde(default)]
    pub selected_threads: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

struct LoadedPrep {
    revision: String,
    document: PrepDocument,
    notes: String,
}

pub fn prep_path(session_dir: &Path) -> std::path::PathBuf {
    session_dir.join("prep.md")
}

pub fn read(session_dir: &Path) -> AppResult<PrepResponse> {
    let loaded = load(session_dir)?;
    Ok(response(loaded))
}

pub fn put(session_dir: &Path, mut request: PutPrepRequest) -> AppResult<PrepResponse> {
    let loaded = load(session_dir)?;
    if request.base_revision != loaded.revision {
        return Err(AppError::Conflict(
            "Preparation changed since it was loaded".into(),
        ));
    }

    merge_and_assign_card_fields(&loaded.document.cards, &mut request.cards)?;
    validate(&request.cards, &request.selected_threads, &request.notes)?;

    let document = PrepDocument {
        ck_prep_version: SCHEMA_VERSION,
        cards: request.cards,
        selected_threads: request.selected_threads,
        handoffs: loaded.document.handoffs,
        extra: loaded.document.extra,
    };
    let bytes = render(&document, &request.notes)?;
    if current_revision(&prep_path(session_dir))? != loaded.revision {
        return Err(AppError::Conflict(
            "Preparation changed while it was being saved".into(),
        ));
    }
    atomic_write(&prep_path(session_dir), &bytes)?;
    Ok(PrepResponse {
        revision: revision(&bytes),
        cards: document.cards,
        selected_threads: document.selected_threads,
        notes: request.notes,
    })
}

fn load(session_dir: &Path) -> AppResult<LoadedPrep> {
    let path = prep_path(session_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LoadedPrep {
                revision: "absent".into(),
                document: PrepDocument::default(),
                notes: String::new(),
            })
        }
        Err(e) => return Err(io_error("read", &path, e)),
    };
    let raw = std::str::from_utf8(&bytes)
        .map_err(|e| AppError::Unprocessable(format!("prep.md is not valid UTF-8: {e}")))?;
    let (yaml, notes) = split_frontmatter(raw)?;
    let document: PrepDocument = serde_yaml::from_str(yaml)
        .map_err(|e| AppError::Unprocessable(format!("Invalid prep.md frontmatter: {e}")))?;
    if document.ck_prep_version != SCHEMA_VERSION {
        return Err(AppError::Unprocessable(format!(
            "Unsupported preparation version: {}",
            document.ck_prep_version
        )));
    }
    validate(&document.cards, &document.selected_threads, notes)?;
    Ok(LoadedPrep {
        revision: revision(&bytes),
        document,
        notes: notes.to_string(),
    })
}

fn response(loaded: LoadedPrep) -> PrepResponse {
    PrepResponse {
        revision: loaded.revision,
        cards: loaded.document.cards,
        selected_threads: loaded.document.selected_threads,
        notes: loaded.notes,
    }
}

/// Best-effort rewrite of prep page references after an app-driven page move or
/// rename. Exact `from`/`to` vault-relative paths only; a folder move rewrites
/// every path with the `from` prefix. Fail-soft: a prep file that cannot be
/// loaded (invalid YAML, unknown enums) is left byte-identical, and no error
/// escapes to abort the move.
pub fn rewrite_page_references(world_root: &Path, from: &str, to: &str) {
    rewrite_references(world_root, from, to, false);
}

/// Folder-move variant: rewrite every reference under the `from` folder prefix.
pub fn rewrite_page_references_prefix(world_root: &Path, from: &str, to: &str) {
    rewrite_references(world_root, from, to, true);
}

fn rewrite_references(world_root: &Path, from: &str, to: &str, prefix: bool) {
    let from = from.trim_matches('/');
    let to = to.trim_matches('/');
    if from.is_empty() || from == to {
        return;
    }
    for dir in crate::store::sessions::session_dirs(world_root) {
        let path = prep_path(&dir);
        if !path.is_file() {
            continue;
        }
        let Ok(loaded) = load(&dir) else {
            continue;
        };
        let mut document = loaded.document;
        let mut changed = false;
        for card in &mut document.cards {
            for link in &mut card.links {
                if let Some(updated) = rewritten(link, from, to, prefix) {
                    *link = updated;
                    changed = true;
                }
            }
        }
        for thread in &mut document.selected_threads {
            if let Some(updated) = rewritten(thread, from, to, prefix) {
                *thread = updated;
                changed = true;
            }
        }
        if !changed {
            continue;
        }
        let Ok(bytes) = render(&document, &loaded.notes) else {
            continue;
        };
        // Re-check the exact bytes so a concurrent editor is never clobbered.
        if current_revision(&path).ok().as_deref() != Some(loaded.revision.as_str()) {
            continue;
        }
        let _ = atomic_write(&path, &bytes);
    }
}

fn rewritten(reference: &str, from: &str, to: &str, prefix: bool) -> Option<String> {
    if prefix {
        let rest = reference.strip_prefix(from)?.strip_prefix('/')?;
        return Some(if to.is_empty() {
            rest.to_string()
        } else {
            format!("{to}/{rest}")
        });
    }
    (reference == from).then(|| to.to_string())
}

fn split_frontmatter(raw: &str) -> AppResult<(&str, &str)> {
    let (rest, delimiter) = if let Some(rest) = raw.strip_prefix("---\n") {
        (rest, "\n---\n")
    } else if let Some(rest) = raw.strip_prefix("---\r\n") {
        (rest, "\r\n---\r\n")
    } else {
        return Err(AppError::Unprocessable(
            "prep.md must begin with YAML frontmatter".into(),
        ));
    };
    let end = rest
        .find(delimiter)
        .ok_or_else(|| AppError::Unprocessable("prep.md frontmatter is not terminated".into()))?;
    Ok((&rest[..end], &rest[end + delimiter.len()..]))
}

fn merge_and_assign_card_fields(existing: &[PrepCard], incoming: &mut [PrepCard]) -> AppResult<()> {
    let by_id: HashMap<&str, &PrepCard> = existing
        .iter()
        .filter_map(|card| card.id.as_deref().map(|id| (id, card)))
        .collect();
    for card in incoming {
        match card.id.as_deref() {
            None => card.id = Some(Uuid::new_v4().to_string()),
            Some(id) => {
                Uuid::parse_str(id).map_err(|_| {
                    AppError::Unprocessable(format!("Invalid preparation card id: {id}"))
                })?;
                let persisted = by_id.get(id).ok_or_else(|| {
                    AppError::Unprocessable(format!("Unknown preparation card id: {id}"))
                })?;
                for (key, value) in &persisted.extra {
                    card.extra
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
            }
        }
    }
    Ok(())
}

fn validate(cards: &[PrepCard], selected_threads: &[String], notes: &str) -> AppResult<()> {
    if cards.len() > MAX_CARDS {
        return invalid(format!("Preparation is limited to {MAX_CARDS} cards"));
    }
    if notes.chars().count() > MAX_NOTES_CHARS {
        return invalid("Preparation notes are too large".into());
    }
    let mut ids = HashSet::new();
    let mut openings = 0;
    let mut link_count = selected_threads.len();
    for card in cards {
        let id = card
            .id
            .as_deref()
            .ok_or_else(|| AppError::Unprocessable("Stored card is missing an id".into()))?;
        Uuid::parse_str(id)
            .map_err(|_| AppError::Unprocessable(format!("Invalid preparation card id: {id}")))?;
        if !ids.insert(id) {
            return invalid(format!("Duplicate preparation card id: {id}"));
        }
        if card.section == PrepSection::Opening {
            openings += 1;
        }
        if card.text.trim().is_empty() {
            return invalid("Preparation card text cannot be empty".into());
        }
        if card.text.chars().count() > MAX_CARD_CHARS
            || card.title.as_deref().unwrap_or_default().chars().count() > MAX_CARD_CHARS
            || card.outcome_note.chars().count() > MAX_CARD_CHARS
        {
            return invalid(format!(
                "Preparation cards are limited to {MAX_CARD_CHARS} characters per field"
            ));
        }
        link_count += card.links.len();
        for link in &card.links {
            validate_world_path(link)?;
        }
    }
    if openings > 1 {
        return invalid("Preparation can contain only one opening card".into());
    }
    if link_count > MAX_LINKS {
        return invalid(format!("Preparation is limited to {MAX_LINKS} page links"));
    }
    for thread in selected_threads {
        validate_world_path(thread)?;
    }
    Ok(())
}

fn validate_world_path(value: &str) -> AppResult<()> {
    let path = Path::new(value);
    let valid = !value.trim().is_empty()
        && !value.contains(['\\', ':'])
        && !path.is_absolute()
        && path.extension().and_then(|s| s.to_str()) == Some("md")
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if !valid {
        return invalid(format!("Invalid world page path: {value}"));
    }
    Ok(())
}

fn render(document: &PrepDocument, notes: &str) -> AppResult<Vec<u8>> {
    let yaml = serde_yaml::to_string(document)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize prep.md: {e}")))?;
    Ok(format!("---\n{}---\n{}", yaml, notes).into_bytes())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("prep.md has no parent directory")))?;
    let temp = parent.join(format!(".prep-{}.tmp", Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        if let Ok(dir) = OpenOptions::new().read(true).open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(io_error("write", path, error));
    }
    Ok(())
}

fn revision(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn current_revision(path: &Path) -> AppResult<String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(revision(&bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("absent".into()),
        Err(error) => Err(io_error("read", path, error)),
    }
}

fn invalid<T>(message: String) -> AppResult<T> {
    Err(AppError::Unprocessable(message))
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> AppError {
    AppError::Internal(anyhow::anyhow!("{action} {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_session() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("ck-prep-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn new_card(section: PrepSection, text: &str) -> PrepCard {
        PrepCard::new(section, text)
    }

    #[test]
    fn missing_prep_is_empty_without_creating_a_file() {
        let dir = temp_session();
        let prep = read(&dir).unwrap();
        assert_eq!(prep.revision, "absent");
        assert!(prep.cards.is_empty());
        assert!(!prep_path(&dir).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn round_trip_preserves_unknown_yaml_and_notes() {
        let dir = temp_session();
        let id = Uuid::new_v4();
        let raw = format!(
            "---\nck_prep_version: 1\ncustom_root: yes\ncards:\n  - id: {id}\n    section: scene\n    text: Old\n    custom_card: 42\nselected_threads: []\nhandoffs: []\n---\nFreeform notes.\n"
        );
        std::fs::write(prep_path(&dir), raw).unwrap();
        let loaded = read(&dir).unwrap();
        let mut card = loaded.cards[0].clone();
        card.text = "New".into();
        let saved = put(
            &dir,
            PutPrepRequest {
                base_revision: loaded.revision,
                cards: vec![card],
                selected_threads: Vec::new(),
                notes: "Changed notes.\n".into(),
            },
        )
        .unwrap();
        assert_ne!(saved.revision, "absent");
        let written = std::fs::read_to_string(prep_path(&dir)).unwrap();
        assert!(written.contains("custom_root: yes"));
        assert!(written.contains("custom_card: 42"));
        assert!(written.ends_with("Changed notes.\n"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stale_write_conflicts_and_does_not_replace_file() {
        let dir = temp_session();
        let first = put(
            &dir,
            PutPrepRequest {
                base_revision: "absent".into(),
                cards: vec![new_card(PrepSection::Scene, "First")],
                selected_threads: Vec::new(),
                notes: String::new(),
            },
        )
        .unwrap();
        let before = std::fs::read(prep_path(&dir)).unwrap();
        let error = put(
            &dir,
            PutPrepRequest {
                base_revision: "absent".into(),
                cards: vec![new_card(PrepSection::Scene, "Second")],
                selected_threads: Vec::new(),
                notes: String::new(),
            },
        )
        .unwrap_err();
        assert!(matches!(error, AppError::Conflict(_)));
        assert_eq!(before, std::fs::read(prep_path(&dir)).unwrap());
        assert!(!first.cards[0].id.as_deref().unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_existing_yaml_is_never_erased() {
        let dir = temp_session();
        let bytes = b"---\ncards: [broken\n---\nnotes";
        std::fs::write(prep_path(&dir), bytes).unwrap();
        assert!(matches!(read(&dir), Err(AppError::Unprocessable(_))));
        assert_eq!(std::fs::read(prep_path(&dir)).unwrap(), bytes);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_duplicate_ids() {
        let id = Uuid::new_v4().to_string();
        let mut one = new_card(PrepSection::Scene, "One");
        one.id = Some(id.clone());
        let mut two = new_card(PrepSection::Scene, "Two");
        two.id = Some(id);
        assert!(validate(&[one, two], &[], "").is_err());
    }

    #[test]
    fn rejects_multiple_openings() {
        let dir = temp_session();
        let error = put(
            &dir,
            PutPrepRequest {
                base_revision: "absent".into(),
                cards: vec![
                    new_card(PrepSection::Opening, "One"),
                    new_card(PrepSection::Opening, "Two"),
                ],
                selected_threads: Vec::new(),
                notes: String::new(),
            },
        )
        .unwrap_err();
        assert!(matches!(error, AppError::Unprocessable(_)));
        assert!(!prep_path(&dir).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_traversal_and_windows_paths() {
        for path in ["../secret.md", "..\\secret.md", "C:\\secret.md"] {
            assert!(validate_world_path(path).is_err(), "accepted {path}");
        }
    }

    #[test]
    fn rejects_oversize_cards() {
        let dir = temp_session();
        let error = put(
            &dir,
            PutPrepRequest {
                base_revision: "absent".into(),
                cards: vec![new_card(
                    PrepSection::Scene,
                    &"x".repeat(MAX_CARD_CHARS + 1),
                )],
                selected_threads: Vec::new(),
                notes: String::new(),
            },
        )
        .unwrap_err();
        assert!(matches!(error, AppError::Unprocessable(_)));
        assert!(!prep_path(&dir).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_unknown_yaml_enums() {
        let dir = temp_session();
        let id = Uuid::new_v4();
        let raw = format!(
            "---\nck_prep_version: 1\ncards:\n  - id: {id}\n    section: encounter\n    text: Wrong enum\n---\n"
        );
        std::fs::write(prep_path(&dir), raw.as_bytes()).unwrap();
        assert!(matches!(read(&dir), Err(AppError::Unprocessable(_))));
        assert_eq!(std::fs::read(prep_path(&dir)).unwrap(), raw.as_bytes());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn temp_world(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("ck-prep-world-{tag}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("Sessions/001")).unwrap();
        root
    }

    fn seed_prep(dir: &Path, raw: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(prep_path(dir), raw).unwrap();
    }

    #[test]
    fn rewrite_updates_exact_links_and_selected_threads() {
        let root = temp_world("exact");
        let dir = root.join("Sessions/001");
        let id = Uuid::new_v4();
        seed_prep(
            &dir,
            &format!(
                "---\nck_prep_version: 1\ncards:\n  - id: {id}\n    section: scene\n    text: Courier\n    links:\n      - Codex/Threads/Missing courier.md\n      - NPCs/Other.md\nselected_threads:\n  - Codex/Threads/Missing courier.md\n---\nNotes.\n"
            ),
        );
        rewrite_page_references(
            &root,
            "Codex/Threads/Missing courier.md",
            "Codex/Threads/Found courier.md",
        );
        let loaded = read(&dir).unwrap();
        assert_eq!(
            loaded.cards[0].links,
            vec![
                "Codex/Threads/Found courier.md".to_string(),
                "NPCs/Other.md".to_string()
            ]
        );
        assert_eq!(
            loaded.selected_threads,
            vec!["Codex/Threads/Found courier.md".to_string()]
        );
        assert_eq!(loaded.notes, "Notes.\n");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rewrite_leaves_unrelated_paths_and_absent_files_untouched() {
        let root = temp_world("unrelated");
        let dir = root.join("Sessions/001");
        let id = Uuid::new_v4();
        let raw = format!(
            "---\nck_prep_version: 1\ncards:\n  - id: {id}\n    section: scene\n    text: X\n    links:\n      - Codex/Threads/Other.md\nselected_threads: []\n---\n"
        );
        seed_prep(&dir, &raw);
        rewrite_page_references(&root, "Codex/Threads/Missing.md", "Codex/Threads/New.md");
        assert_eq!(std::fs::read_to_string(prep_path(&dir)).unwrap(), raw);
        // No session dirs at all: a no-op, not an error.
        let empty = root.join("Empty");
        std::fs::create_dir_all(&empty).unwrap();
        rewrite_page_references(&empty, "a.md", "b.md");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rewrite_preserves_unknown_yaml_and_notes() {
        let root = temp_world("preserve");
        let dir = root.join("Sessions/001");
        let id = Uuid::new_v4();
        seed_prep(
            &dir,
            &format!(
                "---\nck_prep_version: 1\ncustom_root: yes\ncards:\n  - id: {id}\n    section: scene\n    text: X\n    custom_card: 42\n    links:\n      - A.md\nselected_threads: []\nhandoffs: []\n---\nFreeform notes.\n"
            ),
        );
        rewrite_page_references(&root, "A.md", "B.md");
        let written = std::fs::read_to_string(prep_path(&dir)).unwrap();
        assert!(written.contains("custom_root: yes"));
        assert!(written.contains("custom_card: 42"));
        assert!(written.ends_with("Freeform notes.\n"));
        assert_eq!(read(&dir).unwrap().cards[0].links, vec!["B.md".to_string()]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rewrite_is_idempotent_and_skips_invalid_prep_byte_identical() {
        let root = temp_world("idempotent");
        let dir = root.join("Sessions/001");
        seed_prep(&dir, "---\ncards: [broken\n---\nnotes");
        let bytes = std::fs::read(prep_path(&dir)).unwrap();
        rewrite_page_references(&root, "A.md", "B.md");
        assert_eq!(std::fs::read(prep_path(&dir)).unwrap(), bytes);

        let valid = root.join("Sessions/002");
        let id = Uuid::new_v4();
        seed_prep(
            &valid,
            &format!(
                "---\nck_prep_version: 1\ncards:\n  - id: {id}\n    section: scene\n    text: X\n    links:\n      - A.md\nselected_threads:\n  - A.md\n---\n"
            ),
        );
        rewrite_page_references(&root, "A.md", "B.md");
        let once = std::fs::read(prep_path(&valid)).unwrap();
        rewrite_page_references(&root, "A.md", "B.md");
        assert_eq!(std::fs::read(prep_path(&valid)).unwrap(), once);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_rewrite_reparents_prefixed_references() {
        let root = temp_world("prefix");
        let dir = root.join("Sessions/001");
        let id = Uuid::new_v4();
        seed_prep(
            &dir,
            &format!(
                "---\nck_prep_version: 1\ncards:\n  - id: {id}\n    section: scene\n    text: X\n    links:\n      - Codex/Threads/Missing courier.md\n      - NPCs/A.md\nselected_threads:\n  - Codex/Threads/Missing courier.md\n---\n"
            ),
        );
        rewrite_page_references_prefix(&root, "Codex/Threads", "Codex/Plots");
        let loaded = read(&dir).unwrap();
        assert_eq!(loaded.cards[0].links[0], "Codex/Plots/Missing courier.md");
        assert_eq!(loaded.cards[0].links[1], "NPCs/A.md");
        assert_eq!(
            loaded.selected_threads,
            vec!["Codex/Plots/Missing courier.md".to_string()]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
