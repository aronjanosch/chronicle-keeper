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

// ── Carry: copy cards / possibilities into a later session's prep ─────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarryRequest {
    pub base_revision: String,
    pub request_id: String,
    pub source_session_id: String,
    pub item_ids: Vec<String>,
}

/// Destination-side dedup receipt. Server-owned: `put` round-trips the raw
/// `handoffs` list untouched, so a client cannot forge or drop one.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Handoff {
    request_id: String,
    source_session_id: String,
    item_ids: Vec<String>,
    created_at: String,
}

impl Handoff {
    /// Same request, same work. Order of `item_ids` is not part of the payload.
    fn matches(&self, req: &CarryRequest) -> bool {
        if self.source_session_id != req.source_session_id {
            return false;
        }
        let mine: HashSet<&str> = self.item_ids.iter().map(String::as_str).collect();
        let theirs: HashSet<&str> = req.item_ids.iter().map(String::as_str).collect();
        mine == theirs
    }
}

/// Copy the named source cards/possibilities into `dest_dir`'s preparation.
///
/// The receipt is checked *before* the revision so a lost response can be
/// retried with the same `request_id` and produce one copy, not two. Source
/// preparation is never modified; a carried possibility is marked on the source
/// review afterwards, best effort, and reconciled from the receipt if that
/// write fails — the destination is not undone.
pub fn carry(
    dest_dir: &Path,
    dest_session_id: &str,
    source_dir: &Path,
    req: &CarryRequest,
) -> AppResult<PrepResponse> {
    Uuid::parse_str(&req.request_id)
        .map_err(|_| AppError::Unprocessable(format!("Invalid request id: {}", req.request_id)))?;
    if req.item_ids.is_empty() {
        return invalid("Choose at least one item to carry".into());
    }
    if req.item_ids.len() > MAX_CARDS {
        return invalid(format!("Carry is limited to {MAX_CARDS} items"));
    }
    if dest_dir == source_dir {
        return invalid("Choose a different session to carry into".into());
    }

    let loaded = load(dest_dir)?;
    // Receipt before revision: a lost response must be retryable with the same
    // request id even though the destination has moved on since.
    let replay = loaded
        .document
        .handoffs
        .iter()
        .filter_map(|raw| serde_yaml::from_value::<Handoff>(raw.clone()).ok())
        .find(|receipt| receipt.request_id == req.request_id)
        .map(|receipt| receipt.matches(req));
    match replay {
        Some(true) => return Ok(response(loaded)),
        Some(false) => {
            return Err(AppError::Conflict(
                "This carry id was already used for different items".into(),
            ))
        }
        None => {}
    }
    if req.base_revision != loaded.revision {
        return Err(AppError::Conflict(
            "Preparation changed since it was loaded".into(),
        ));
    }

    let copies = resolve_carry_items(source_dir, &req.source_session_id, &req.item_ids)?;
    if copies.iter().any(|c| c.section == PrepSection::Opening)
        && loaded
            .document
            .cards
            .iter()
            .any(|c| c.section == PrepSection::Opening)
    {
        return invalid(
            "This session already has an opening — replace it explicitly instead".into(),
        );
    }

    let mut cards = loaded.document.cards.clone();
    cards.extend(copies);
    merge_and_assign_card_fields(&loaded.document.cards, &mut cards)?;
    validate(&cards, &loaded.document.selected_threads, &loaded.notes)?;

    let mut handoffs = loaded.document.handoffs.clone();
    let receipt = Handoff {
        request_id: req.request_id.clone(),
        source_session_id: req.source_session_id.clone(),
        item_ids: req.item_ids.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    handoffs.push(
        serde_yaml::to_value(&receipt)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("encode carry receipt: {e}")))?,
    );

    let document = PrepDocument {
        ck_prep_version: SCHEMA_VERSION,
        cards,
        selected_threads: loaded.document.selected_threads,
        handoffs,
        extra: loaded.document.extra,
    };
    let bytes = render(&document, &loaded.notes)?;
    if current_revision(&prep_path(dest_dir))? != loaded.revision {
        return Err(AppError::Conflict(
            "Preparation changed while it was being saved".into(),
        ));
    }
    atomic_write(&prep_path(dest_dir), &bytes)?;

    mark_possibilities_carried(source_dir, dest_session_id, &req.item_ids);

    Ok(PrepResponse {
        revision: revision(&bytes),
        cards: document.cards,
        selected_threads: document.selected_threads,
        notes: loaded.notes,
    })
}

/// Turn source ids into fresh cards. A card that happened is not carryable;
/// `changed` and `unused` are, because the GM chose them by id.
fn resolve_carry_items(
    source_dir: &Path,
    source_session_id: &str,
    item_ids: &[String],
) -> AppResult<Vec<PrepCard>> {
    let source = load(source_dir)?;
    let possibilities = crate::session_review::load(source_dir)?
        .map(|loaded| loaded.run.possibilities)
        .unwrap_or_default();

    let mut out = Vec::with_capacity(item_ids.len());
    let mut seen = HashSet::new();
    for id in item_ids {
        if !seen.insert(id.as_str()) {
            return invalid(format!("Item listed twice: {id}"));
        }
        if let Some(card) = source
            .document
            .cards
            .iter()
            .find(|c| c.id.as_deref() == Some(id.as_str()))
        {
            if card.outcome == PrepOutcome::Happened {
                return invalid(format!(
                    "\"{}\" happened in that session — it cannot carry forward",
                    card.title.clone().unwrap_or_else(|| card.text.clone())
                ));
            }
            out.push(copy_of(card, source_session_id, id));
            continue;
        }
        if let Some(p) = possibilities.iter().find(|p| &p.id == id) {
            out.push(possibility_card(p, source_session_id, id)?);
            continue;
        }
        return invalid(format!("Unknown item: {id}"));
    }
    Ok(out)
}

/// A copy is a new card: no id, unmarked, origin recorded, links kept.
fn copy_of(card: &PrepCard, source_session_id: &str, item_id: &str) -> PrepCard {
    PrepCard {
        id: None,
        section: card.section,
        title: card.title.clone(),
        text: card.text.clone(),
        links: card.links.clone(),
        outcome: PrepOutcome::Unmarked,
        outcome_note: String::new(),
        origin: Some(PrepOrigin {
            session_id: source_session_id.to_string(),
            item_id: item_id.to_string(),
        }),
        extra: BTreeMap::new(),
    }
}

/// A saved possibility becomes something to keep in mind, never an opening and
/// never a claim that it happened. Only source links that are real page paths
/// survive the copy.
fn possibility_card(
    possibility: &crate::session_review::Possibility,
    source_session_id: &str,
    item_id: &str,
) -> AppResult<PrepCard> {
    let links: Vec<String> = possibility
        .source_links
        .iter()
        .filter(|link| validate_world_path(link).is_ok())
        .cloned()
        .collect();
    let text = if possibility.text.trim().is_empty() {
        possibility.title.clone()
    } else {
        possibility.text.clone()
    };
    if text.trim().is_empty() {
        return invalid(format!("Possibility {item_id} has no text to carry"));
    }
    Ok(PrepCard {
        id: None,
        section: PrepSection::Reminder,
        title: Some(possibility.title.clone()).filter(|t| !t.trim().is_empty()),
        text,
        links,
        outcome: PrepOutcome::Unmarked,
        outcome_note: String::new(),
        origin: Some(PrepOrigin {
            session_id: source_session_id.to_string(),
            item_id: item_id.to_string(),
        }),
        extra: BTreeMap::new(),
    })
}

/// Best effort: the destination write is the commitment, so a failure here
/// leaves the receipt as the record and never rolls the copy back.
fn mark_possibilities_carried(source_dir: &Path, dest_session_id: &str, item_ids: &[String]) {
    let Ok(Some(loaded)) = crate::session_review::load(source_dir) else {
        return;
    };
    let mut run = loaded.run;
    let wanted: HashSet<&str> = item_ids.iter().map(String::as_str).collect();
    let mut changed = false;
    for p in &mut run.possibilities {
        if wanted.contains(p.id.as_str()) {
            p.decision = crate::session_review::PossibilityDecision::SavedToPrep;
            p.destination_session_id = Some(dest_session_id.to_string());
            changed = true;
        }
    }
    if changed {
        let _ = crate::session_review::save(source_dir, &run);
    }
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

    fn write_cards(dir: &Path, cards: Vec<PrepCard>) -> PrepResponse {
        put(
            dir,
            PutPrepRequest {
                base_revision: "absent".into(),
                cards,
                selected_threads: Vec::new(),
                notes: String::new(),
            },
        )
        .unwrap()
    }

    fn carry_req(revision: &str, source: &str, ids: &[&str]) -> CarryRequest {
        CarryRequest {
            base_revision: revision.into(),
            request_id: Uuid::new_v4().to_string(),
            source_session_id: source.into(),
            item_ids: ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn carry_copies_as_new_unmarked_cards_and_leaves_the_source_alone() {
        let src = temp_session();
        let dst = temp_session();
        let mut card = new_card(PrepSection::Scene, "The magistrate offers a bargain");
        card.title = Some("The bargain".into());
        card.links = vec!["NPCs/Magistrate.md".into()];
        let saved = write_cards(&src, vec![card]);
        let id = saved.cards[0].id.clone().unwrap();
        let before = read(&src).unwrap();

        let dest = carry(
            &dst,
            "dest-session",
            &src,
            &carry_req("absent", "src-session", &[&id]),
        )
        .unwrap();

        assert_eq!(dest.cards.len(), 1);
        let copy = &dest.cards[0];
        assert_ne!(copy.id.as_deref(), Some(id.as_str()));
        assert_eq!(copy.outcome, PrepOutcome::Unmarked);
        assert_eq!(copy.links, vec!["NPCs/Magistrate.md".to_string()]);
        assert_eq!(
            copy.origin,
            Some(PrepOrigin {
                session_id: "src-session".into(),
                item_id: id.clone(),
            })
        );
        assert_eq!(read(&src).unwrap().revision, before.revision);
        std::fs::remove_dir_all(src).unwrap();
        std::fs::remove_dir_all(dst).unwrap();
    }

    #[test]
    fn carry_retried_with_the_same_request_makes_one_copy() {
        let src = temp_session();
        let dst = temp_session();
        let saved = write_cards(
            &src,
            vec![new_card(PrepSection::Scene, "A nervous courier")],
        );
        let id = saved.cards[0].id.clone().unwrap();
        let req = carry_req("absent", "src-session", &[&id]);

        let first = carry(&dst, "dest", &src, &req).unwrap();
        // The retry sends the stale revision too — that is the point of the receipt.
        let replay = carry(&dst, "dest", &src, &req).unwrap();

        assert_eq!(first.cards.len(), 1);
        assert_eq!(replay.cards.len(), 1);
        assert_eq!(first.revision, replay.revision);
        std::fs::remove_dir_all(src).unwrap();
        std::fs::remove_dir_all(dst).unwrap();
    }

    #[test]
    fn carry_reusing_a_request_id_for_other_items_conflicts() {
        let src = temp_session();
        let dst = temp_session();
        let saved = write_cards(
            &src,
            vec![
                new_card(PrepSection::Scene, "First"),
                new_card(PrepSection::Scene, "Second"),
            ],
        );
        let first = saved.cards[0].id.clone().unwrap();
        let second = saved.cards[1].id.clone().unwrap();
        let mut req = carry_req("absent", "src-session", &[&first]);
        carry(&dst, "dest", &src, &req).unwrap();

        req.item_ids = vec![second];
        let err = carry(&dst, "dest", &src, &req).unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        std::fs::remove_dir_all(src).unwrap();
        std::fs::remove_dir_all(dst).unwrap();
    }

    #[test]
    fn carry_refuses_a_card_that_happened() {
        let src = temp_session();
        let dst = temp_session();
        let mut card = new_card(PrepSection::Scene, "The accusation at the docks");
        card.outcome = PrepOutcome::Happened;
        let saved = write_cards(&src, vec![card]);
        let id = saved.cards[0].id.clone().unwrap();

        let err = carry(&dst, "dest", &src, &carry_req("absent", "src", &[&id])).unwrap_err();
        assert!(matches!(err, AppError::Unprocessable(_)));
        assert!(!prep_path(&dst).exists());
        std::fs::remove_dir_all(src).unwrap();
        std::fs::remove_dir_all(dst).unwrap();
    }

    #[test]
    fn carry_never_silently_replaces_an_existing_opening() {
        let src = temp_session();
        let dst = temp_session();
        let saved = write_cards(
            &src,
            vec![new_card(PrepSection::Opening, "At the east gate")],
        );
        let id = saved.cards[0].id.clone().unwrap();
        let dest = write_cards(
            &dst,
            vec![new_card(PrepSection::Opening, "In the warehouse")],
        );

        let err = carry(
            &dst,
            "dest",
            &src,
            &carry_req(&dest.revision, "src", &[&id]),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Unprocessable(_)));
        assert_eq!(read(&dst).unwrap().cards.len(), 1);
        std::fs::remove_dir_all(src).unwrap();
        std::fs::remove_dir_all(dst).unwrap();
    }

    #[test]
    fn carry_rejects_unknown_items_and_a_stale_destination() {
        let src = temp_session();
        let dst = temp_session();
        write_cards(&src, vec![new_card(PrepSection::Scene, "Something")]);

        let unknown = Uuid::new_v4().to_string();
        let err = carry(&dst, "dest", &src, &carry_req("absent", "src", &[&unknown])).unwrap_err();
        assert!(matches!(err, AppError::Unprocessable(_)));

        let saved = write_cards(&dst, vec![new_card(PrepSection::Scene, "Existing")]);
        let id = read(&src).unwrap().cards[0].id.clone().unwrap();
        let _ = saved;
        let err = carry(&dst, "dest", &src, &carry_req("absent", "src", &[&id])).unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        std::fs::remove_dir_all(src).unwrap();
        std::fs::remove_dir_all(dst).unwrap();
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
