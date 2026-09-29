//! Session preparation. The prep is a Codex page (`kind: prep`, see
//! [`crate::prep_page`]) that `session.toml` points to with `prep = "<path>"`.
//! Sessions prepared before that stored YAML cards in `Sessions/<NNN>/prep.md`;
//! those are rendered into a page on first access and the old file is left
//! untouched.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::prep_page::PrepPage;
use crate::world_config::WorldConfig;

const PREP_FOLDER: &str = "Prep";

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
    /// Page paths the card links to, derived from its `[[wikilinks]]`.
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

#[derive(Debug, Clone, Serialize)]
pub struct PrepResponse {
    pub revision: String,
    /// Codex-relative path of the prep page; `None` until the session is prepared.
    pub page: Option<String>,
    pub cards: Vec<PrepCard>,
    pub selected_threads: Vec<String>,
    pub notes: String,
}

/// Where one session's prep lives and how new pages are written.
pub struct PrepCtx {
    pub session_id: String,
    pub session_dir: PathBuf,
    pub world_root: PathBuf,
    pub vault: PathBuf,
    pub lang: String,
    pub number: Option<i64>,
    pub title: Option<String>,
}

impl PrepCtx {
    pub(crate) fn of(
        conn: &Connection,
        loc: &crate::store::sessions::SessionLoc,
    ) -> AppResult<Self> {
        let Some((root, cfg)) = &loc.world else {
            return Err(AppError::Unprocessable(
                "Session preparation requires a world session".into(),
            ));
        };
        Ok(Self {
            session_id: loc.st.id.clone().unwrap_or_default(),
            session_dir: loc.dir.clone(),
            world_root: root.clone(),
            vault: cfg.codex_dir(root),
            lang: world_language(conn, cfg),
            number: loc.st.number,
            title: loc.st.title.clone(),
        })
    }

    /// For callers that already know the world (Keeper tools).
    pub fn for_dir(world_root: &Path, cfg: &WorldConfig, session_dir: &Path, lang: &str) -> Self {
        let st = crate::session_files::read_session_toml(session_dir)
            .ok()
            .flatten()
            .unwrap_or_default();
        Self {
            session_id: st.id.unwrap_or_default(),
            session_dir: session_dir.to_path_buf(),
            world_root: world_root.to_path_buf(),
            vault: cfg.codex_dir(world_root),
            lang: lang.to_string(),
            number: st.number,
            title: st.title,
        }
    }
}

/// The world's default language code, falling back to the app default, then `en`.
pub fn world_language(conn: &Connection, cfg: &WorldConfig) -> String {
    let app = crate::config::get_config_map(conn).unwrap_or_default();
    crate::store::campaigns::get_campaign(conn, &cfg.id)
        .ok()
        .flatten()
        .map(|c| c.default_language)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| app.get("default_language").cloned())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "en".into())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrepOp {
    /// Start an empty prep page for this session.
    Create,
    /// Use an existing Codex page as this session's prep.
    Adopt {
        page: String,
    },
    AddCard {
        section: PrepSection,
        #[serde(default)]
        title: Option<String>,
        text: String,
        /// Page paths to mention as wikilinks when the text does not already.
        #[serde(default)]
        links: Vec<String>,
    },
    ReplaceOpening {
        text: String,
    },
    SetOutcome {
        id: String,
        outcome: PrepOutcome,
        #[serde(default)]
        note: String,
    },
    AddThread {
        page: String,
    },
    Link {
        id: String,
        page: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpsRequest {
    pub base_revision: String,
    pub ops: Vec<PrepOp>,
}

// ── Reading ───────────────────────────────────────────────────────

struct Loaded {
    rel: Option<String>,
    content: String,
    revision: String,
}

/// Absolute path of the file that holds this session's prep (page or legacy).
/// Used for review freshness; works from the session folder alone.
pub fn source_file(session_dir: &Path) -> PathBuf {
    let pointer = crate::session_files::read_session_toml(session_dir)
        .ok()
        .flatten()
        .and_then(|st| st.prep);
    let world_root = session_dir.parent().and_then(Path::parent);
    if let (Some(rel), Some(root)) = (pointer, world_root) {
        let cfg = crate::world_config::read(root)
            .ok()
            .flatten()
            .unwrap_or_default();
        return cfg.codex_dir(root).join(rel);
    }
    legacy_path(session_dir)
}

/// True when the session has any prep (a page or a not-yet-migrated file).
pub fn exists(ctx: &PrepCtx) -> bool {
    pointer(ctx).is_some() || legacy_path(&ctx.session_dir).is_file()
}

pub fn read(ctx: &PrepCtx) -> AppResult<PrepResponse> {
    Ok(response(ctx, load(ctx)?))
}

fn load(ctx: &PrepCtx) -> AppResult<Loaded> {
    let rel = match pointer(ctx) {
        Some(rel) => Some(rel),
        None => match find_by_session(ctx) {
            Some(rel) => Some(rel),
            None => migrate_legacy(ctx)?,
        },
    };
    let Some(rel) = rel else {
        return Ok(Loaded {
            rel: None,
            content: String::new(),
            revision: "absent".into(),
        });
    };
    let bytes = std::fs::read(ctx.vault.join(&rel))
        .map_err(|e| AppError::Internal(anyhow::anyhow!("read prep page {rel}: {e}")))?;
    Ok(Loaded {
        revision: revision(&bytes),
        content: String::from_utf8_lossy(&bytes).into_owned(),
        rel: Some(rel),
    })
}

fn response(ctx: &PrepCtx, loaded: Loaded) -> PrepResponse {
    let parsed = PrepPage::parse(&loaded.content).read();
    let names = PageNames::of(&ctx.vault);
    let cards = parsed
        .cards
        .into_iter()
        .map(|mut card| {
            card.links = card.links.iter().map(|l| names.resolve(l)).collect();
            card
        })
        .collect();
    PrepResponse {
        revision: loaded.revision,
        page: loaded.rel,
        cards,
        selected_threads: parsed.threads.iter().map(|t| names.resolve(t)).collect(),
        notes: parsed.notes,
    }
}

/// `session.toml`'s `prep`, when that page still exists.
fn pointer(ctx: &PrepCtx) -> Option<String> {
    let rel = crate::session_files::read_session_toml(&ctx.session_dir)
        .ok()
        .flatten()?
        .prep?;
    ctx.vault.join(&rel).is_file().then_some(rel)
}

/// No live pointer (never set, or the page was renamed outside CK): find the
/// `kind: prep` page that names this session and point at it. This is how a
/// page the Keeper or the GM wrote by hand becomes the session's prep.
fn find_by_session(ctx: &PrepCtx) -> Option<String> {
    let number = ctx.number?;
    let pages = crate::vault::list_pages(&ctx.vault).ok()?;
    let is_prep = |p: &&crate::vault::PageInfo| p.kind.as_deref() == Some(crate::prep_page::KIND);
    let by_frontmatter = pages.iter().filter(is_prep).find(|p| {
        crate::vault::read_page(&ctx.vault, &p.path).is_ok_and(|page| {
            let (fm, _) = crate::vault::split_frontmatter(&page.content);
            crate::vault::fm_get(&fm, "session") == Some(number.to_string().as_str())
        })
    });
    let hit = match by_frontmatter {
        Some(p) => p.path.clone(),
        None => by_file_name(&pages, number)?,
    };
    let _ = set_pointer(ctx, &hit);
    Some(hit)
}

/// Hand-written prep that predates CK: `Session 14 - Windhalle.md`. Only an
/// unambiguous match counts — a `kind: prep` page wins over untyped ones.
fn by_file_name(pages: &[crate::vault::PageInfo], number: i64) -> Option<String> {
    let named: Vec<_> = pages
        .iter()
        .filter(|p| names_session(stem(&p.path), number))
        .collect();
    let prep: Vec<_> = named
        .iter()
        .filter(|p| p.kind.as_deref() == Some(crate::prep_page::KIND))
        .collect();
    match (prep.as_slice(), named.as_slice()) {
        ([one], _) => Some(one.path.clone()),
        ([], [one]) if one.kind.is_none() => Some(one.path.clone()),
        _ => None,
    }
}

fn names_session(stem: &str, number: i64) -> bool {
    let lower = stem.to_lowercase();
    let Some(rest) = lower.strip_prefix("session") else {
        return false;
    };
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    !digits.is_empty() && digits.parse::<i64>().ok() == Some(number)
}

fn set_pointer(ctx: &PrepCtx, rel: &str) -> AppResult<()> {
    let mut st = crate::session_files::read_session_toml(&ctx.session_dir)?.unwrap_or_default();
    st.prep = Some(rel.to_string());
    crate::session_files::write_session_toml_file(&ctx.session_dir, &st)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write session.toml: {e}")))
}

// ── Writing ───────────────────────────────────────────────────────

/// Apply surgical edits to the prep page. Returns the new state; the caller
/// reindexes `page`.
pub fn apply(ctx: &PrepCtx, req: &OpsRequest) -> AppResult<PrepResponse> {
    let loaded = load(ctx)?;
    if req.base_revision != loaded.revision {
        return Err(AppError::Conflict(
            "Preparation changed since it was loaded".into(),
        ));
    }
    let mut rel = loaded.rel.clone();
    let mut page = rel.as_ref().map(|_| PrepPage::parse(&loaded.content));
    for op in &req.ops {
        if let PrepOp::Adopt { page: path } = op {
            if rel.is_some() {
                return Err(AppError::Conflict(
                    "This session is already prepared on another page".into(),
                ));
            }
            let content = crate::vault::read_page(&ctx.vault, path)?.content;
            let mut adopted = PrepPage::parse(&content);
            adopted.ensure_kind();
            page = Some(adopted);
            rel = Some(path.clone());
            continue;
        }
        let p = page.get_or_insert_with(|| new_page(ctx));
        match op {
            PrepOp::Create | PrepOp::Adopt { .. } => {}
            PrepOp::AddCard {
                section,
                title,
                text,
                links,
            } => {
                let text = with_links(&ctx.vault, text, links);
                p.add_card(*section, title.as_deref(), &text, None, &ctx.lang)?;
            }
            PrepOp::ReplaceOpening { text } => {
                p.replace_opening(text, &ctx.lang)?;
            }
            PrepOp::SetOutcome { id, outcome, note } => p.set_outcome(id, *outcome, note)?,
            PrepOp::AddThread { page: path } => p.add_thread(&link_for(&ctx.vault, path)),
            PrepOp::Link { id, page: path } => p.link(id, &link_for(&ctx.vault, path))?,
        }
    }
    let Some(page) = page else {
        return Ok(response(ctx, loaded));
    };
    let rel = match rel {
        Some(rel) => rel,
        None => fresh_path(ctx),
    };
    let content = page.render();
    if loaded.rel.as_deref() == Some(rel.as_str()) && content == loaded.content {
        return Ok(response(ctx, loaded));
    }
    write(
        ctx,
        &rel,
        &content,
        loaded.rel.as_ref().map(|_| loaded.revision.as_str()),
    )?;
    if loaded.rel.as_deref() != Some(rel.as_str()) {
        set_pointer(ctx, &rel)?;
    }
    read(ctx)
}

fn new_page(ctx: &PrepCtx) -> PrepPage {
    PrepPage::new(
        &ctx.lang,
        ctx.number,
        ctx.title.as_deref().unwrap_or_default(),
    )
}

/// `Prep/Session 012.md`, or the first free `… (2).md`.
fn fresh_path(ctx: &PrepCtx) -> String {
    let stem = match ctx.number {
        Some(n) => format!("Session {}", crate::session_files::padded_number(n)),
        None => ctx
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "Session".into()),
    };
    let stem = stem.replace(['/', '\\', ':'], "-");
    (1..)
        .map(|i| match i {
            1 => format!("{PREP_FOLDER}/{stem}.md"),
            i => format!("{PREP_FOLDER}/{stem} ({i}).md"),
        })
        .find(|rel| !ctx.vault.join(rel).exists())
        .expect("unbounded")
}

/// Write the page with a history snapshot. `expected` guards an existing page
/// against a concurrent editor.
fn write(ctx: &PrepCtx, rel: &str, content: &str, expected: Option<&str>) -> AppResult<()> {
    let abs = ctx.vault.join(rel);
    if let Some(expected) = expected {
        let now = std::fs::read(&abs)
            .map(|b| revision(&b))
            .unwrap_or_default();
        if now != expected {
            return Err(AppError::Conflict(
                "Preparation changed while it was being saved".into(),
            ));
        }
    }
    let existed = abs.is_file();
    if existed {
        let _ = crate::history::record(&ctx.world_root, &ctx.vault, rel, "user");
    }
    crate::vault::write_page(&ctx.vault, rel, content)?;
    if !existed {
        let _ = crate::history::record_create(&ctx.world_root, rel, "user");
    }
    Ok(())
}

// ── Page moves ────────────────────────────────────────────────────

/// Keep `session.toml` pointers on a prep page that CK moved or renamed.
/// Links inside the page are wikilinks, which the rename cascade handles.
pub fn rewrite_page_references(world_root: &Path, from: &str, to: &str) {
    rewrite_pointers(world_root, from, to, false);
}

/// Folder-move variant: re-parent every pointer under the `from` prefix.
pub fn rewrite_page_references_prefix(world_root: &Path, from: &str, to: &str) {
    rewrite_pointers(world_root, from, to, true);
}

fn rewrite_pointers(world_root: &Path, from: &str, to: &str, prefix: bool) {
    let from = from.trim_matches('/');
    let to = to.trim_matches('/');
    if from.is_empty() || from == to {
        return;
    }
    for dir in crate::store::sessions::session_dirs(world_root) {
        let Ok(Some(mut st)) = crate::session_files::read_session_toml(&dir) else {
            continue;
        };
        let Some(updated) = st
            .prep
            .as_deref()
            .and_then(|p| rewritten(p, from, to, prefix))
        else {
            continue;
        };
        st.prep = Some(updated);
        let _ = crate::session_files::write_session_toml_file(&dir, &st);
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

// ── Legacy prep.md ────────────────────────────────────────────────

fn legacy_path(session_dir: &Path) -> PathBuf {
    session_dir.join("prep.md")
}

#[derive(Deserialize)]
struct LegacyDocument {
    #[serde(default)]
    cards: Vec<PrepCard>,
    #[serde(default)]
    selected_threads: Vec<String>,
    #[serde(default)]
    handoffs: Vec<YamlValue>,
}

/// Render a legacy `prep.md` into a new prep page and point the session at it.
/// The old file stays on disk; an unreadable one is left alone and ignored.
fn migrate_legacy(ctx: &PrepCtx) -> AppResult<Option<String>> {
    let Ok(raw) = std::fs::read_to_string(legacy_path(&ctx.session_dir)) else {
        return Ok(None);
    };
    let raw = raw.replace("\r\n", "\n");
    let Some((yaml, notes)) = raw
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
    else {
        return Ok(None);
    };
    let Ok(doc) = serde_yaml::from_str::<LegacyDocument>(yaml) else {
        tracing::warn!(
            "legacy prep.md in {} is unreadable; not migrated",
            ctx.session_dir.display()
        );
        return Ok(None);
    };
    if doc.cards.is_empty() && doc.selected_threads.is_empty() && notes.trim().is_empty() {
        return Ok(None);
    }
    let mut page = new_page(ctx);
    for card in &doc.cards {
        let text = with_links(&ctx.vault, &card.text, &card.links);
        let id = page.add_card(
            card.section,
            card.title.as_deref(),
            &text,
            card.origin.as_ref(),
            &ctx.lang,
        )?;
        if card.outcome != PrepOutcome::Unmarked {
            page.set_outcome(&id, card.outcome, &card.outcome_note)?;
        }
    }
    for thread in &doc.selected_threads {
        page.add_thread(&link_for(&ctx.vault, thread));
    }
    for receipt in doc.handoffs {
        page.push_handoff(receipt);
    }
    page.append_notes(notes, &ctx.lang);
    let rel = fresh_path(ctx);
    write(ctx, &rel, &page.render(), None)?;
    set_pointer(ctx, &rel)?;
    tracing::info!(
        "migrated {} → {rel}",
        legacy_path(&ctx.session_dir).display()
    );
    Ok(Some(rel))
}

// ── Link names ────────────────────────────────────────────────────

/// Page stems and paths of a vault, for turning link targets into paths.
struct PageNames {
    paths: Vec<String>,
}

impl PageNames {
    fn of(vault: &Path) -> Self {
        let paths = crate::vault::list_pages(vault)
            .map(|pages| pages.into_iter().map(|p| p.path).collect())
            .unwrap_or_default();
        Self { paths }
    }

    /// Wikilink target → page path; an unresolved target becomes `target.md`.
    fn resolve(&self, target: &str) -> String {
        let bare = target.trim().trim_end_matches(".md");
        let exact = format!("{bare}.md");
        if self.paths.iter().any(|p| p.eq_ignore_ascii_case(&exact)) {
            return exact;
        }
        let want = crate::store::index::normalize_name(stem(bare));
        self.paths
            .iter()
            .find(|p| crate::store::index::normalize_name(stem(p.trim_end_matches(".md"))) == want)
            .cloned()
            .unwrap_or(exact)
    }
}

fn stem(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Wikilink target for a page path: the bare name when it is unique in the
/// vault (so rename cascades find it), else the folder-qualified path.
fn link_for(vault: &Path, path: &str) -> String {
    let bare = path.trim().trim_end_matches(".md");
    let name = stem(bare);
    let want = crate::store::index::normalize_name(name);
    let same_name = PageNames::of(vault)
        .paths
        .iter()
        .filter(|p| crate::store::index::normalize_name(stem(p.trim_end_matches(".md"))) == want)
        .count();
    if same_name > 1 {
        bare.to_string()
    } else {
        name.to_string()
    }
}

/// `text` plus a `[[wikilink]]` for each page path it does not mention yet.
fn with_links(vault: &Path, text: &str, paths: &[String]) -> String {
    let have = crate::prep_page::wikilinks(text);
    let missing: Vec<String> = paths
        .iter()
        .filter(|p| p.ends_with(".md") && !p.contains(".."))
        .map(|p| link_for(vault, p))
        .filter(|l| !have.iter().any(|h| h.eq_ignore_ascii_case(l)))
        .map(|l| format!("[[{l}]]"))
        .collect();
    if missing.is_empty() {
        text.to_string()
    } else {
        format!("{} {}", text.trim_end(), missing.join(" "))
    }
}

fn revision(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        root: PathBuf,
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn world() -> World {
        let root = std::env::temp_dir().join(format!("ck-prep-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("Codex/NPCs")).unwrap();
        std::fs::write(root.join("Codex/NPCs/Mara Voss.md"), "# Mara\n").unwrap();
        World { root }
    }

    fn session(w: &World, n: i64) -> PrepCtx {
        let dir = w.root.join(format!("Sessions/{n:03}"));
        std::fs::create_dir_all(&dir).unwrap();
        let st = crate::session_files::SessionToml {
            id: Some(format!("s{n}")),
            number: Some(n),
            ..Default::default()
        };
        crate::session_files::write_session_toml_file(&dir, &st).unwrap();
        PrepCtx {
            session_id: format!("s{n}"),
            session_dir: dir,
            world_root: w.root.clone(),
            vault: w.root.join("Codex"),
            lang: "en".into(),
            number: Some(n),
            title: None,
        }
    }

    fn ops(ctx: &PrepCtx, base: &str, ops: Vec<PrepOp>) -> AppResult<PrepResponse> {
        apply(
            ctx,
            &OpsRequest {
                base_revision: base.into(),
                ops,
            },
        )
    }

    fn add(section: PrepSection, text: &str) -> PrepOp {
        PrepOp::AddCard {
            section,
            title: None,
            text: text.into(),
            links: Vec::new(),
        }
    }

    #[test]
    fn unprepared_session_is_empty_and_writes_nothing() {
        let w = world();
        let s = session(&w, 1);
        let r = read(&s).unwrap();
        assert_eq!((r.revision.as_str(), r.page.as_deref()), ("absent", None));
        assert!(!w.root.join("Codex/Prep").exists());
        assert!(!exists(&s));
    }

    #[test]
    fn first_op_creates_a_prep_page_and_points_the_session_at_it() {
        let w = world();
        let s = session(&w, 12);
        let r = ops(
            &s,
            "absent",
            vec![add(PrepSection::Scene, "Ambush [[Mara Voss]]")],
        )
        .unwrap();
        assert_eq!(r.page.as_deref(), Some("Prep/Session 012.md"));
        assert_eq!(r.cards[0].links, vec!["NPCs/Mara Voss.md"]);
        let text = std::fs::read_to_string(w.root.join("Codex/Prep/Session 012.md")).unwrap();
        assert!(text.starts_with("---\nkind: prep\nsession: 12\n---\n"));
        let st = crate::session_files::read_session_toml(&s.session_dir)
            .unwrap()
            .unwrap();
        assert_eq!(st.prep.as_deref(), Some("Prep/Session 012.md"));
        assert_eq!(
            source_file(&s.session_dir),
            w.root.join("Codex/Prep/Session 012.md")
        );
    }

    #[test]
    fn stale_revision_conflicts_and_hand_edits_survive_ops() {
        let w = world();
        let s = session(&w, 2);
        let r = ops(
            &s,
            "absent",
            vec![add(PrepSection::Reminder, "Roll weather")],
        )
        .unwrap();
        let path = w.root.join("Codex").join(r.page.as_ref().unwrap());
        let edited = std::fs::read_to_string(&path).unwrap() + "\nMy own *prose*.\n";
        std::fs::write(&path, &edited).unwrap();
        assert!(matches!(
            ops(&s, &r.revision, vec![add(PrepSection::Reminder, "x")]),
            Err(AppError::Conflict(_))
        ));
        let fresh = read(&s).unwrap();
        let id = fresh.cards[0].id.clone().unwrap();
        ops(
            &s,
            &fresh.revision,
            vec![PrepOp::SetOutcome {
                id,
                outcome: PrepOutcome::Happened,
                note: String::new(),
            }],
        )
        .unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("- [x] Roll weather ^"));
        assert!(after.contains("My own *prose*."));
    }

    #[test]
    fn adopting_an_existing_page_marks_it_prep() {
        let w = world();
        let s = session(&w, 3);
        std::fs::write(
            w.root.join("Codex/Omas prep.md"),
            "---\ntags: [prep]\n---\n## Szenen\n### Nebel\nText\n",
        )
        .unwrap();
        let r = ops(
            &s,
            "absent",
            vec![PrepOp::Adopt {
                page: "Omas prep.md".into(),
            }],
        )
        .unwrap();
        assert_eq!(r.page.as_deref(), Some("Omas prep.md"));
        assert_eq!(r.cards[0].title.as_deref(), Some("Nebel"));
        let text = std::fs::read_to_string(w.root.join("Codex/Omas prep.md")).unwrap();
        assert!(text.contains("kind: prep") && text.contains("### Nebel\nText"));
    }

    #[test]
    fn legacy_prep_is_migrated_once_and_left_on_disk() {
        let w = world();
        let s = session(&w, 4);
        let legacy = "---\nck_prep_version: 1\ncards:\n- id: 7b1c9d62-0000-4000-8000-000000000001\n  section: scene\n  title: Ambush\n  text: Crossbows\n  links: [NPCs/Mara Voss.md]\n  outcome: changed\n  outcome_note: they ran\nselected_threads: [NPCs/Mara Voss.md]\n---\nOld notes\n";
        std::fs::write(s.session_dir.join("prep.md"), legacy).unwrap();
        let r = read(&s).unwrap();
        assert_eq!(r.page.as_deref(), Some("Prep/Session 004.md"));
        assert_eq!(r.cards[0].title.as_deref(), Some("Ambush"));
        assert_eq!(r.cards[0].outcome, PrepOutcome::Changed);
        assert_eq!(r.cards[0].outcome_note, "they ran");
        assert_eq!(r.cards[0].links, vec!["NPCs/Mara Voss.md"]);
        assert_eq!(r.selected_threads, vec!["NPCs/Mara Voss.md"]);
        assert!(r.notes.contains("Old notes"));
        assert_eq!(
            std::fs::read_to_string(s.session_dir.join("prep.md")).unwrap(),
            legacy
        );
        let again = read(&s).unwrap();
        assert_eq!(again.revision, r.revision);
        assert!(!w.root.join("Codex/Prep/Session 004 (2).md").exists());
    }

    #[test]
    fn a_page_renamed_outside_ck_is_found_by_session_number() {
        let w = world();
        let s = session(&w, 5);
        ops(&s, "absent", vec![add(PrepSection::Reminder, "One")]).unwrap();
        std::fs::rename(
            w.root.join("Codex/Prep/Session 005.md"),
            w.root.join("Codex/Prep/Docks.md"),
        )
        .unwrap();
        assert_eq!(read(&s).unwrap().page.as_deref(), Some("Prep/Docks.md"));
    }

    #[test]
    fn hand_written_prep_is_found_by_file_name() {
        let w = world();
        let s = session(&w, 14);
        let dir = w.root.join("Codex/Prep Notes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Session 14 - Windhalle.md"),
            "## Plan\nThe hall.\n",
        )
        .unwrap();
        std::fs::write(dir.join("Session 140 - Later.md"), "Later.\n").unwrap();
        assert_eq!(
            read(&s).unwrap().page.as_deref(),
            Some("Prep Notes/Session 14 - Windhalle.md")
        );
        assert!(!names_session("Session 140 - Later", 14));
        assert!(names_session("Session 014", 14));
    }

    #[test]
    fn moves_rewrite_session_pointers() {
        let w = world();
        let s = session(&w, 6);
        ops(&s, "absent", vec![add(PrepSection::Reminder, "One")]).unwrap();
        rewrite_page_references(&w.root, "Prep/Session 006.md", "Prep/Six.md");
        let st = |s: &PrepCtx| {
            crate::session_files::read_session_toml(&s.session_dir)
                .unwrap()
                .unwrap()
                .prep
        };
        assert_eq!(st(&s).as_deref(), Some("Prep/Six.md"));
        rewrite_page_references_prefix(&w.root, "Prep", "Archive/Prep");
        assert_eq!(st(&s).as_deref(), Some("Archive/Prep/Six.md"));
    }
}
