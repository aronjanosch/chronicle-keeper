//! Vault pages: a folder of `.md` files, files-as-truth. Direct file I/O.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};

/// Page kinds the world schema knows about (frontmatter `kind:`).
pub const KINDS: &[&str] = &["pc", "npc", "place", "faction", "item", "event", "lore"];

#[derive(Debug, Serialize)]
pub struct PageInfo {
    pub path: String,
    pub title: String,
    pub kind: Option<String>,
    pub summary: String,
    pub modified: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct Page {
    pub path: String,
    pub title: String,
    pub kind: Option<String>,
    pub summary: String,
    pub content: String,
}

// Never part of the page tree, regardless of layout: matters when
// `codex_root = "."` puts the vault at the world root (adopted foreign vault),
// where `Sessions/` holds `.md` session artifacts. Dot-dirs (`.ck`,
// `.obsidian`, `.trash`) are excluded separately.
pub(crate) fn is_reserved_dir(name: &str) -> bool {
    name == "Sessions" || name == "_templates"
}

fn require_dir(vault: &Path) -> AppResult<()> {
    if vault.is_dir() {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "Vault folder does not exist: {}",
            vault.display()
        )))
    }
}

// Rejects `..`, absolute paths, and empty/dotfile components — result stays
// under `vault`. Used for folders and move targets (no extension requirement).
fn resolve_rel(vault: &Path, rel: &str) -> AppResult<PathBuf> {
    let candidate = Path::new(rel);
    let mut any = false;
    for comp in candidate.components() {
        match comp {
            Component::Normal(s) => {
                any = true;
                let s = s.to_string_lossy();
                if s.starts_with('.') || is_reserved_dir(&s) {
                    return Err(AppError::BadRequest("invalid path".into()));
                }
            }
            _ => return Err(AppError::BadRequest("invalid path".into())),
        }
    }
    if !any {
        return Err(AppError::BadRequest("empty path".into()));
    }
    Ok(vault.join(candidate))
}

// As `resolve_rel`, but also requires a `.md` extension (page files).
fn resolve(vault: &Path, rel: &str) -> AppResult<PathBuf> {
    if Path::new(rel).extension().and_then(|e| e.to_str()) != Some("md") {
        return Err(AppError::BadRequest("page path must end in .md".into()));
    }
    resolve_rel(vault, rel)
}

fn rel_of(vault: &Path, abs: &Path) -> String {
    abs.strip_prefix(vault)
        .unwrap_or(abs)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

// Flat YAML: scalars, inline `[a, b]` lists, block `- item` lists. Nested
// mappings are skipped. Each key maps to its values (scalar = one value).
pub(crate) fn split_frontmatter(content: &str) -> (Vec<(String, Vec<String>)>, &str) {
    let rest = match content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))
    {
        Some(r) => r,
        None => return (Vec::new(), content),
    };
    let Some(end) = rest.find("\n---") else {
        return (Vec::new(), content);
    };
    let fm = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
    let mut map: Vec<(String, Vec<String>)> = Vec::new();
    for line in fm.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix('-').filter(|s| !s.is_empty()))
        {
            let v = item.trim().trim_matches(['"', '\'']);
            if !v.is_empty() {
                if let Some((_, vals)) = map.last_mut() {
                    vals.push(v.to_string());
                }
            }
            continue;
        }
        if line.starts_with([' ', '\t']) {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim();
            let vals = match v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                Some(inner) => inner
                    .split(',')
                    .map(|s| s.trim().trim_matches(['"', '\'']).to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                None => {
                    let v = v.trim_matches(['"', '\'']);
                    if v.is_empty() {
                        Vec::new()
                    } else {
                        vec![v.to_string()]
                    }
                }
            };
            map.push((k.trim().to_string(), vals));
        }
    }
    (map, body)
}

pub(crate) fn fm_get<'a>(fm: &'a [(String, Vec<String>)], key: &str) -> Option<&'a str> {
    fm.iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.first())
        .map(String::as_str)
}

pub(crate) fn fm_list<'a>(fm: &'a [(String, Vec<String>)], key: &str) -> &'a [String] {
    fm.iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_slice())
        .unwrap_or(&[])
}

fn title_of(abs: &Path) -> String {
    abs.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

fn page_from(vault: &Path, abs: &Path, content: String) -> Page {
    let (fm, _body) = split_frontmatter(&content);
    // No `summary:` frontmatter means no summary — never fabricate one from the
    // body. Plain notes (prep, scratch) legitimately have none.
    let summary = fm_get(&fm, "summary")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_default();
    Page {
        path: rel_of(vault, abs),
        title: title_of(abs),
        kind: fm_get(&fm, "kind")
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        summary,
        content,
    }
}

fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) {
    collect_files(dir, out, &|p| {
        p.extension().and_then(|e| e.to_str()) == Some("md")
    });
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>, want: &dyn Fn(&Path) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if !is_reserved_dir(&name) {
                collect_files(&path, out, want);
            }
        } else if want(&path) {
            out.push(path);
        }
    }
}

fn mtime_secs(abs: &Path) -> Option<u64> {
    abs.metadata()
        .and_then(|m| m.modified())
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn collect_dirs(base: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || is_reserved_dir(&name) {
            continue;
        }
        out.push(rel_of(base, &path));
        collect_dirs(base, &path, out);
    }
}

// Every folder under the vault (incl. empty ones), as slash-joined relative
// paths. Excludes `.ck` and other dotfiles. Sorted.
pub fn list_folders(vault: &Path) -> AppResult<Vec<String>> {
    require_dir(vault)?;
    let mut dirs = Vec::new();
    collect_dirs(vault, vault, &mut dirs);
    dirs.sort();
    Ok(dirs)
}

pub fn create_folder(vault: &Path, rel: &str) -> AppResult<()> {
    require_dir(vault)?;
    let abs = resolve_rel(vault, rel)?;
    if abs.is_file() {
        return Err(AppError::BadRequest("A file with that name exists".into()));
    }
    std::fs::create_dir_all(&abs)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create folder: {e}")))
}

// Rename or move a page or folder. `from` must exist; `to` must not.
pub fn move_entry(vault: &Path, from: &str, to: &str) -> AppResult<()> {
    require_dir(vault)?;
    let src = resolve_rel(vault, from)?;
    let dst = resolve_rel(vault, to)?;
    if !src.exists() {
        return Err(AppError::NotFound(format!("Not found: {from}")));
    }
    if dst.exists() {
        return Err(AppError::BadRequest(format!("Already exists: {to}")));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create dir: {e}")))?;
    }
    std::fs::rename(&src, &dst).map_err(|e| AppError::Internal(anyhow::anyhow!("move: {e}")))
}

pub fn delete_page(vault: &Path, rel: &str) -> AppResult<()> {
    let abs = resolve(vault, rel)?;
    if !abs.is_file() {
        return Err(AppError::NotFound(format!("Page not found: {rel}")));
    }
    crate::paths::move_to_trash(&abs)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("move page to trash: {e}")))
}

/// Relative paths of every `.md` page under `folder_rel` (incl. nested folders).
pub fn page_paths_in_folder(vault: &Path, folder_rel: &str) -> AppResult<Vec<String>> {
    let abs = resolve_rel(vault, folder_rel)?;
    if !abs.is_dir() {
        return Err(AppError::NotFound(format!(
            "Folder not found: {folder_rel}"
        )));
    }
    let mut files = Vec::new();
    collect_md(&abs, &mut files);
    Ok(files.iter().map(|p| rel_of(vault, p)).collect())
}

/// Move a folder and all contents to the OS trash.
pub fn delete_folder(vault: &Path, rel: &str) -> AppResult<()> {
    let abs = resolve_rel(vault, rel)?;
    if !abs.is_dir() {
        return Err(AppError::NotFound(format!("Folder not found: {rel}")));
    }
    crate::paths::move_to_trash(&abs)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("move folder to trash: {e}")))
}

/// World root for a vault (codex) dir: `.ck/` marks it — either the vault
/// itself (`codex_root = "."`) or its parent (canonical `Codex/` layout).
pub fn world_root_of(vault: &Path) -> Option<PathBuf> {
    if vault.join(".ck").is_dir() {
        return Some(vault.to_path_buf());
    }
    match vault.parent() {
        Some(p) if p.join(".ck").is_dir() => Some(p.to_path_buf()),
        _ => None,
    }
}

pub fn ensure_ck_dir(vault: &Path) -> AppResult<()> {
    require_dir(vault)?;
    let ck = vault.join(".ck");
    std::fs::create_dir_all(&ck)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create .ck: {e}")))?;
    let gitignore = ck.join(".gitignore");
    if !gitignore.exists() {
        let _ = std::fs::write(&gitignore, "index.db\nindex.db-*\n");
    }
    write_default_templates(vault)?;
    write_default_snippets(vault)
}

pub fn list_pages(vault: &Path) -> AppResult<Vec<PageInfo>> {
    require_dir(vault)?;
    let mut files = Vec::new();
    collect_md(vault, &mut files);
    let mut pages: Vec<PageInfo> = files
        .iter()
        .filter_map(|abs| {
            let content = std::fs::read_to_string(abs).ok()?;
            let p = page_from(vault, abs, content);
            Some(PageInfo {
                path: p.path,
                title: p.title,
                kind: p.kind,
                summary: p.summary,
                modified: mtime_secs(abs),
            })
        })
        .collect();
    pages.sort_by_key(|p| p.title.to_lowercase());
    Ok(pages)
}

pub fn read_page(vault: &Path, rel: &str) -> AppResult<Page> {
    let abs = resolve(vault, rel)?;
    let content = std::fs::read_to_string(&abs)
        .map_err(|_| AppError::NotFound(format!("Page not found: {rel}")))?;
    Ok(page_from(vault, &abs, content))
}

pub fn write_page(vault: &Path, rel: &str, content: &str) -> AppResult<Page> {
    let abs = resolve(vault, rel)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create dir: {e}")))?;
    }
    std::fs::write(&abs, content)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write page: {e}")))?;
    Ok(page_from(vault, &abs, content.to_string()))
}

pub fn create_page(vault: &Path, title: &str, kind: &str, folder: Option<&str>) -> AppResult<Page> {
    let content = page_file_content(title.trim(), kind, "", "");
    create_page_with(vault, title, folder, &content)
}

/// Create a page with explicit initial content (template-driven create).
pub fn create_page_with(
    vault: &Path,
    title: &str,
    folder: Option<&str>,
    content: &str,
) -> AppResult<Page> {
    require_dir(vault)?;
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    if title.contains(['/', '\\']) {
        return Err(AppError::BadRequest("title cannot contain slashes".into()));
    }
    let folder = folder.map(str::trim).filter(|s| !s.is_empty());
    let rel = match folder {
        Some(f) => format!("{f}/{title}.md"),
        None => format!("{title}.md"),
    };
    let abs = resolve(vault, &rel)?;
    if abs.exists() {
        return Err(AppError::BadRequest(format!(
            "A page named \"{title}\" already exists here"
        )));
    }
    write_page(vault, &rel, content)
}

// ── Page templates (_templates/<name>.md) ────────────────────────
// User-editable, user-creatable starter files surfaced in the Explorer's
// Templates section; `{{title}}` is replaced on create. `kind:` frontmatter
// picks the page kind. A missing template falls back to a schema-derived
// default. `_templates` is a reserved dir (out of the page tree/index/links).

pub fn templates_dir(world_root: &Path) -> PathBuf {
    world_root.join("_templates")
}

/// Standard body headings per kind (Phase 16); `lore` stays free-form.
pub const DEFAULT_HEADINGS: &[(&str, &[&str])] = &[
    ("pc", &["Background", "Goals", "Relationships", "Notes"]),
    (
        "npc",
        &["Appearance", "Motivation", "Relationships", "History"],
    ),
    ("place", &["At a glance", "Notable people", "Hooks"]),
    (
        "faction",
        &["Goals", "Members", "Allies & enemies", "History"],
    ),
    ("item", &["Description", "Properties", "History"]),
    ("event", &["What happened", "Consequences"]),
];

pub fn default_headings(kind: &str) -> &'static [&'static str] {
    DEFAULT_HEADINGS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, h)| *h)
        .unwrap_or(&[])
}

// Frontmatter + H1 only — the pre-Phase-16 default, kept so untouched seeded
// templates can be recognized and upgraded.
fn template_base(kind: &str, fields: &[crate::world_config::KindField]) -> String {
    let mut out = format!("---\nkind: {kind}\nsummary:\n");
    for f in fields {
        match f.ftype.as_str() {
            "list" => out.push_str(&format!("{}: []\n", f.name)),
            "checkbox" => out.push_str(&format!("{}: false\n", f.name)),
            _ => out.push_str(&format!("{}:\n", f.name)),
        }
    }
    out.push_str("---\n\n# {{title}}\n\n");
    out
}

/// Built-in starter content for a kind: frontmatter with the kind's infobox
/// fields left blank (list → `[]`, checkbox → `false`), a `{{title}}` H1, and
/// the kind's standard headings.
pub fn template_content(kind: &str, fields: &[crate::world_config::KindField]) -> String {
    let mut out = template_base(kind, fields);
    for h in default_headings(kind) {
        out.push_str(&format!("## {h}\n\n"));
    }
    out
}

/// Write the default per-kind starter files. User edits and deletions are
/// never overwritten — only a file still byte-equal to the pre-Phase-16
/// default is upgraded to the current one.
pub fn write_default_templates(world_root: &Path) -> AppResult<()> {
    let dir = templates_dir(world_root);
    let fresh = !dir.exists();
    if fresh {
        std::fs::create_dir_all(&dir)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create _templates: {e}")))?;
        migrate_legacy_templates(world_root, &dir);
    }
    for (kind, fields) in crate::world_config::WorldConfig::default().kind_schemas() {
        let path = dir.join(format!("{kind}.md"));
        let untouched_seed = std::fs::read_to_string(&path)
            .map(|c| c == template_base(&kind, &fields))
            .unwrap_or(false);
        if (fresh && !path.exists()) || untouched_seed {
            let _ = std::fs::write(path, template_content(&kind, &fields));
        }
    }
    Ok(())
}

// One-time move of pre-`_templates` files from the old `.ck/templates/<kind>.md`
// home. Only `*.md` files move; the `snippets` subdir stays where it is.
fn migrate_legacy_templates(world_root: &Path, dest: &Path) {
    let legacy = world_root.join(".ck").join("templates");
    let Ok(entries) = std::fs::read_dir(&legacy) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(name) = path.file_name() {
            let _ = std::fs::rename(&path, dest.join(name));
        }
    }
}

/// List every template file as `(name, content)`, sorted by name. Skips
/// dotfiles and the `snippets` subdir.
pub fn list_templates(world_root: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(templates_dir(world_root)) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("md") {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            Some((name, std::fs::read_to_string(&path).ok()?))
        })
        .collect();
    out.sort_by_key(|(name, _)| name.to_lowercase());
    out
}

fn template_path(world_root: &Path, name: &str) -> AppResult<PathBuf> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', '.']) {
        return Err(AppError::BadRequest("invalid template name".into()));
    }
    Ok(templates_dir(world_root).join(format!("{name}.md")))
}

pub fn write_template(world_root: &Path, name: &str, content: &str) -> AppResult<()> {
    let path = template_path(world_root, name)?;
    let dir = templates_dir(world_root);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create _templates: {e}")))?;
    }
    std::fs::write(&path, content)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write template: {e}")))
}

pub fn delete_template(world_root: &Path, name: &str) -> AppResult<()> {
    let path = template_path(world_root, name)?;
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("delete template: {e}")))?;
    }
    Ok(())
}

// ── Snippets (.ck/templates/snippets/<name>.md) — caret inserts, Phase 8C ──

pub fn snippets_dir(world_root: &Path) -> PathBuf {
    world_root.join(".ck").join("templates").join("snippets")
}

const DEFAULT_SNIPPETS: &[(&str, &str)] = &[
    (
        "Statblock",
        "> [!note] Statblock\n> **AC** — · **HP** — · **Speed** —\n>\n> | STR | DEX | CON | INT | WIS | CHA |\n> | --- | --- | --- | --- | --- | --- |\n> | — | — | — | — | — | — |\n>\n> **Traits** —\n> **Actions** —\n",
    ),
    (
        "Location skeleton",
        "## At a glance\n\n\n## Notable people\n\n- [[ ]]\n\n## Hooks\n\n- \n\n> [!secret] GM only\n> \n",
    ),
    (
        "Plot hook",
        "> [!note] Hook\n> **Who** — \n> **Wants** — \n> **Obstacle** — \n> **Twist** — \n",
    ),
];

/// Seed starter snippets once; an existing folder is the user's (edits and
/// deletions sacred), exactly like the kind templates.
pub fn write_default_snippets(world_root: &Path) -> AppResult<()> {
    let dir = snippets_dir(world_root);
    if dir.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create snippets dir: {e}")))?;
    for (name, content) in DEFAULT_SNIPPETS {
        let _ = std::fs::write(dir.join(format!("{name}.md")), content);
    }
    Ok(())
}

/// (name, content) for every snippet file, sorted by name.
pub fn list_snippets(world_root: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(snippets_dir(world_root)) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("md") {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            let content = std::fs::read_to_string(&path).ok()?;
            Some((name, content))
        })
        .collect();
    out.sort_by_key(|(name, _)| name.to_lowercase());
    out
}

pub fn read_template(world_root: &Path, kind: &str) -> Option<String> {
    let kind = kind.trim();
    if kind.is_empty() || kind.contains(['/', '\\', '.']) {
        return None;
    }
    std::fs::read_to_string(templates_dir(world_root).join(format!("{kind}.md"))).ok()
}

/// Starter content for a new page: the kind's template file if present,
/// otherwise the schema-derived default. `{{title}}` is substituted.
pub fn new_page_content(
    world_root: &Path,
    kind: &str,
    title: &str,
    fields: &[crate::world_config::KindField],
) -> String {
    let tpl = read_template(world_root, kind).unwrap_or_else(|| template_content(kind, fields));
    tpl.replace("{{title}}", title.trim())
}

/// Starter content + frontmatter kind for a named template. `{{title}}` is
/// substituted; the kind is read from the template's own `kind:` frontmatter
/// (empty when absent — the caller decides the fallback). Returns None when no
/// template of that name exists.
pub fn new_page_from_template(
    world_root: &Path,
    name: &str,
    title: &str,
) -> Option<(String, String)> {
    let path = template_path(world_root, name).ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let (fm, _) = split_frontmatter(&raw);
    let kind = fm
        .iter()
        .find(|(k, _)| k == "kind")
        .and_then(|(_, v)| v.first())
        .cloned()
        .unwrap_or_default();
    Some((raw.replace("{{title}}", title.trim()), kind))
}

/// `## ` headings of the kind's template (world override or built-in default).
pub fn template_headings(world_root: &Path, kind: &str) -> Vec<String> {
    match read_template(world_root, kind) {
        Some(tpl) => tpl
            .lines()
            .filter_map(|l| l.strip_prefix("## "))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => default_headings(kind)
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

// Raw frontmatter block (without fences) + body, or None when absent.
fn split_raw_frontmatter(content: &str) -> Option<(&str, &str)> {
    let rest = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))?;
    let end = rest.find("\n---")?;
    Some((
        &rest[..end],
        rest[end + 4..].trim_start_matches(['\r', '\n']),
    ))
}

/// Promote a page to `kind` (Phase 16, quick-capture → real page): set
/// `kind:`, add the schema fields it lacks, drop the `inbox` tag, and append
/// the kind's template headings the body doesn't already have. Existing
/// frontmatter values and body text are kept verbatim.
pub fn promote_content(
    content: &str,
    kind: &str,
    fields: &[crate::world_config::KindField],
    headings: &[String],
) -> String {
    let (fm_raw, body) = split_raw_frontmatter(content).unwrap_or(("", content));

    let mut fm: Vec<String> = Vec::new();
    let mut in_tags_block = false;
    for line in fm_raw.lines() {
        let trimmed = line.trim();
        if in_tags_block {
            if let Some(item) = trimmed.strip_prefix("- ") {
                if item.trim().trim_matches(['"', '\'']) != "inbox" {
                    fm.push(line.to_string());
                }
                continue;
            }
            in_tags_block = false;
        }
        let top_level = !line.starts_with([' ', '\t']);
        if top_level && trimmed.strip_prefix("kind:").is_some() {
            fm.push(format!("kind: {kind}"));
            continue;
        }
        if let Some(v) = trimmed.strip_prefix("tags:").filter(|_| top_level) {
            match v.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                Some(inner) => {
                    let kept: Vec<&str> = inner
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty() && s.trim_matches(['"', '\'']) != "inbox")
                        .collect();
                    if !kept.is_empty() {
                        fm.push(format!("tags: [{}]", kept.join(", ")));
                    }
                }
                None if v.trim().is_empty() => {
                    in_tags_block = true;
                    fm.push(line.to_string());
                }
                None => fm.push(line.to_string()),
            }
            continue;
        }
        fm.push(line.to_string());
    }
    // a block list whose items were all removed leaves a dangling `tags:`
    if fm.last().map(|l| l.trim() == "tags:").unwrap_or(false) {
        fm.pop();
    }

    let has_key = |fm: &[String], key: &str| {
        fm.iter().any(|l| {
            !l.starts_with([' ', '\t'])
                && l.split_once(':')
                    .map(|(k, _)| k.trim() == key)
                    .unwrap_or(false)
        })
    };
    if !has_key(&fm, "kind") {
        fm.insert(0, format!("kind: {kind}"));
    }
    if !has_key(&fm, "summary") {
        fm.push("summary:".to_string());
    }
    for f in fields {
        if !has_key(&fm, &f.name) {
            match f.ftype.as_str() {
                "list" => fm.push(format!("{}: []", f.name)),
                "checkbox" => fm.push(format!("{}: false", f.name)),
                _ => fm.push(format!("{}:", f.name)),
            }
        }
    }

    let existing: std::collections::HashSet<String> = body
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .map(|s| s.trim().to_lowercase())
        .collect();
    let mut out = format!("---\n{}\n---\n\n{}", fm.join("\n"), body.trim_end());
    out.push('\n');
    for h in headings {
        if !existing.contains(&h.to_lowercase()) {
            out.push_str(&format!("\n## {h}\n"));
        }
    }
    out
}

/// `.ck/config.toml` — its presence marks a folder as a world (discovery
/// scans for it). Never overwrites an existing one.
pub fn write_world_config(
    world_root: &Path,
    id: &str,
    name: &str,
    codex_root: &str,
) -> AppResult<()> {
    if crate::world_config::config_path(world_root).exists() {
        return Ok(());
    }
    let cfg = crate::world_config::WorldConfig {
        id: id.to_string(),
        name: name.to_string(),
        codex_root: codex_root.to_string(),
        ..Default::default()
    };
    crate::world_config::write(world_root, &cfg)
}

pub fn default_vault_path(output_root: &Path, campaign_name: &str) -> PathBuf {
    output_root
        .join(crate::normalize::sanitize_folder_name(campaign_name))
        .join("Codex")
}

/// Create the canonical 1.0 world layout: `Codex/`, `Sessions/`, `.ck/` under
/// `world_root`. Idempotent — safe to call on an existing folder. If `scaffold`
/// is true, also creates `Codex/NPCs`, `Places`, `Factions`, `Items`, `Lore`.
pub fn provision_vault_layout(world_root: &Path, scaffold: bool) -> AppResult<()> {
    let codex = world_root.join("Codex");
    let sessions = world_root.join("Sessions");
    std::fs::create_dir_all(&codex)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create Codex/: {e}")))?;
    std::fs::create_dir_all(&sessions)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create Sessions/: {e}")))?;
    ensure_ck_dir(world_root)?;
    if scaffold {
        for sub in ["NPCs", "Places", "Factions", "Items", "Lore"] {
            let _ = std::fs::create_dir_all(codex.join(sub));
        }
    }
    Ok(())
}

/// Adopt-in-place (Phase 3): make a user's folder a world while writing only
/// additive artifacts — `.ck/` and an empty `Sessions/`. Never moves, renames,
/// or rewrites a user file. Returns the `codex_root` for the config: `"."`
/// when pages already live anywhere in the folder (foreign vault), `"Codex"`
/// for canonical-layout or empty folders (which get the fresh layout).
pub fn adopt_vault_layout(world_root: &Path) -> AppResult<String> {
    if !world_root.join("Codex").is_dir() && has_pages(world_root) {
        std::fs::create_dir_all(world_root.join("Sessions"))
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create Sessions/: {e}")))?;
        ensure_ck_dir(world_root)?;
        return Ok(".".into());
    }
    provision_vault_layout(world_root, false)?;
    Ok("Codex".into())
}

pub fn has_pages(vault: &Path) -> bool {
    count_pages(vault) > 0
}

pub fn count_pages(vault: &Path) -> usize {
    let mut files = Vec::new();
    collect_md(vault, &mut files);
    files.len()
}

pub fn page_exists(vault: &Path, title: &str) -> bool {
    let want = crate::store::index::normalize_name(title.trim());
    let mut files = Vec::new();
    collect_md(vault, &mut files);
    files
        .iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .any(|s| crate::store::index::normalize_name(s) == want)
}

// Filesystem-safe filename; spaces kept (Obsidian-style), separators stripped.
pub(crate) fn safe_page_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        "Untitled".into()
    } else {
        cleaned.to_string()
    }
}

/// Set (or add) `kind` and `summary` frontmatter fields without touching other fields.
/// Existing values take priority — only blank/absent fields are filled from the arguments.
/// Non-`kind`/`summary` fields are preserved (reformatted to scalar or block-list YAML).
pub(crate) fn set_frontmatter_fields(content: &str, kind: &str, summary: &str) -> String {
    let (fm, body) = split_frontmatter(content);
    let k = fm_get(&fm, "kind")
        .filter(|s| !s.is_empty())
        .unwrap_or(kind);
    let s = fm_get(&fm, "summary")
        .filter(|s| !s.is_empty())
        .unwrap_or(summary);
    let mut out = String::from("---\n");
    out.push_str(&format!("kind: {k}\n"));
    let sq = yaml_quoted(s);
    if sq.is_empty() {
        out.push_str("summary:\n");
    } else {
        out.push_str(&format!("summary: {sq}\n"));
    }
    for (key, vals) in &fm {
        if key == "kind" || key == "summary" {
            continue;
        }
        push_fm_field(&mut out, key, vals);
    }
    out.push_str("---\n\n");
    out.push_str(body);
    if !body.is_empty() && !body.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn push_fm_field(out: &mut String, key: &str, vals: &[String]) {
    if vals.is_empty() {
        out.push_str(&format!("{key}:\n"));
    } else if vals.len() == 1 {
        let v = if key == "summary" {
            yaml_quoted(&vals[0])
        } else {
            yaml_scalar(&vals[0])
        };
        out.push_str(&format!("{key}: {v}\n"));
    } else {
        out.push_str(&format!("{key}:\n"));
        for v in vals {
            out.push_str(&format!("  - {v}\n"));
        }
    }
}

fn rebuild_with_fm(fm: &[(String, Vec<String>)], body: &str) -> String {
    let mut out = String::from("---\n");
    for (key, vals) in fm {
        push_fm_field(&mut out, key, vals);
    }
    out.push_str("---\n\n");
    out.push_str(body);
    if !body.is_empty() && !body.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Overwrite the `summary:` frontmatter field (unlike `set_frontmatter_fields`,
/// which only fills blanks). Creates the frontmatter block if missing.
pub(crate) fn overwrite_summary(content: &str, summary: &str) -> String {
    let (mut fm, body) = split_frontmatter(content);
    let val = vec![summary.trim().to_string()];
    match fm.iter_mut().find(|(k, _)| k == "summary") {
        Some((_, vals)) => *vals = val,
        None => fm.push(("summary".into(), val)),
    }
    rebuild_with_fm(&fm, body)
}

/// Append a value to a frontmatter list field, creating the field if absent.
/// No-op when the value is already present (case-insensitive).
pub(crate) fn fm_append_list_value(content: &str, field: &str, value: &str) -> String {
    let (mut fm, body) = split_frontmatter(content);
    let value = value.trim();
    match fm.iter_mut().find(|(k, _)| k == field) {
        Some((_, vals)) => {
            if vals.iter().any(|v| v.eq_ignore_ascii_case(value)) {
                return content.to_string();
            }
            vals.push(value.to_string());
        }
        None => fm.push((field.to_string(), vec![value.to_string()])),
    }
    rebuild_with_fm(&fm, body)
}

/// Append `text` at the end of the `anchor` heading's section (before the next
/// heading of the same or higher level). A missing anchor is created at the end
/// of the file.
pub(crate) fn append_under_heading(content: &str, anchor: &str, text: &str) -> String {
    let anchor = anchor.trim();
    let text = text.trim();
    let level = anchor.chars().take_while(|c| *c == '#').count().max(1);
    let lines: Vec<&str> = content.lines().collect();
    let Some(start) = lines.iter().position(|l| l.trim() == anchor) else {
        let mut out = content.trim_end().to_string();
        out.push_str(&format!("\n\n{anchor}\n\n{text}\n"));
        return out;
    };
    // End of section = next heading at the same or a higher level.
    let end = lines[start + 1..]
        .iter()
        .position(|l| {
            let h = l.trim_start();
            let n = h.chars().take_while(|c| *c == '#').count();
            n >= 1 && n <= level && h[n..].starts_with(' ')
        })
        .map(|i| start + 1 + i)
        .unwrap_or(lines.len());
    let mut out: Vec<String> = lines[..end].iter().map(|s| s.to_string()).collect();
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.push(String::new());
    out.push(text.to_string());
    if end < lines.len() {
        out.push(String::new());
        out.extend(lines[end..].iter().map(|s| s.to_string()));
    }
    let mut joined = out.join("\n");
    if !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// Returns true when the page content lacks an explicit `kind` or `summary` frontmatter field.
pub(crate) fn needs_frontmatter_enhance(content: &str) -> bool {
    let (fm, _) = split_frontmatter(content);
    fm_get(&fm, "kind").filter(|s| !s.is_empty()).is_none()
        || fm_get(&fm, "summary").filter(|s| !s.is_empty()).is_none()
}

/// Always double-quote a scalar (empty stays empty). Used for `summary:` so the
/// emitted frontmatter is unambiguous for Obsidian's YAML parser.
fn yaml_quoted(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn yaml_scalar(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    let needs_quote = s.contains([':', '#', '"', '\''])
        || s.starts_with(['-', ' ', '[', '{', '*', '&', '!', '|', '>', '@', '`'])
        || s.ends_with(' ');
    if needs_quote {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

pub(crate) fn page_file_content(name: &str, kind: &str, summary: &str, body: &str) -> String {
    let mut out = format!("---\nkind: {kind}\n");
    let s = yaml_quoted(summary);
    if s.is_empty() {
        out.push_str("summary:\n");
    } else {
        out.push_str(&format!("summary: {s}\n"));
    }
    out.push_str(&format!("---\n\n# {name}\n\n"));
    let body = body.trim();
    if !body.is_empty() {
        out.push_str(body);
        out.push('\n');
    }
    out
}

fn unique_md_path(dir: &Path, stem: &str) -> PathBuf {
    unique_path(dir, stem, "md")
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    candidate
}

// Embeddable media an Obsidian vault commonly carries alongside its notes —
// imported with the pages so `![[image.png]]` embeds keep resolving.
const ASSET_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif", "heic", "pdf", "mp3", "wav", "m4a",
    "ogg", "flac", "mp4", "webm", "mov",
];

fn is_asset(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| ASSET_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn count_assets(dir: &Path) -> usize {
    let mut files = Vec::new();
    collect_files(dir, &mut files, &is_asset);
    files.len()
}

#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub imported: usize,
    pub renamed: usize,
    pub assets: usize,
}

/// Copy-in import (Phase 3): copy every `.md` page — plus the media assets the
/// pages may embed — from `src` into the vault, preserving folder structure.
/// The source folder is never touched; name collisions get a numeric suffix
/// (never overwrite). Reserved and dot dirs (`.obsidian/`, `Sessions/`, …) are
/// skipped on the source side too.
pub fn import_folder(vault: &Path, src: &Path) -> AppResult<ImportReport> {
    require_dir(vault)?;
    if !src.is_dir() {
        return Err(AppError::BadRequest("Folder does not exist".into()));
    }
    let vc = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    let sc = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
    if sc.starts_with(&vc) || vc.starts_with(&sc) {
        return Err(AppError::BadRequest(
            "That folder overlaps this world's Codex — pick a folder outside it".into(),
        ));
    }
    let mut files = Vec::new();
    collect_md(&sc, &mut files);
    if files.is_empty() {
        return Err(AppError::BadRequest(
            "No markdown pages found in that folder".into(),
        ));
    }
    collect_files(&sc, &mut files, &is_asset);
    let mut report = ImportReport {
        imported: 0,
        renamed: 0,
        assets: 0,
    };
    for abs in &files {
        let rel = abs.strip_prefix(&sc).unwrap_or(abs);
        let mut dest = vault.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("create dir: {e}")))?;
        }
        let ext = dest
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("md")
            .to_lowercase();
        if dest.exists() {
            let stem = dest
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string();
            dest = unique_path(dest.parent().unwrap_or(vault), &stem, &ext);
            report.renamed += 1;
        }
        std::fs::copy(abs, &dest)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("copy {}: {e}", rel.display())))?;
        if ext == "md" {
            report.imported += 1;
        } else {
            report.assets += 1;
        }
    }
    Ok(report)
}

// One-time migration: write a codex entry as a page (body one-liner → summary,
// detail → page body). Best-effort; collisions get a numeric suffix.
pub fn write_migrated_entry(
    vault: &Path,
    name: &str,
    kind: &str,
    summary: &str,
    body: &str,
) -> std::io::Result<()> {
    let path = unique_md_path(vault, &safe_page_filename(name));
    std::fs::write(path, page_file_content(name, kind, summary, body))
}

// ── Assets (pasted/dropped editor media) ──────────────────────────

/// Save pasted/dropped bytes under `Assets/`, de-duplicating the filename.
/// Returns the saved file's vault-relative path.
pub fn save_asset(vault: &Path, name: &str, bytes: &[u8]) -> AppResult<String> {
    require_dir(vault)?;
    let raw = Path::new(name.trim());
    let ext = raw
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .ok_or_else(|| AppError::BadRequest("asset name needs a file extension".into()))?;
    let stem = raw
        .file_stem()
        .and_then(|s| s.to_str())
        .map(safe_page_filename)
        .unwrap_or_else(|| "Pasted".into());
    let dir = vault.join("Assets");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create Assets dir: {e}")))?;
    let mut filename = format!("{stem}.{ext}");
    let mut n = 1;
    while dir.join(&filename).exists() {
        n += 1;
        filename = format!("{stem} {n}.{ext}");
    }
    std::fs::write(dir.join(&filename), bytes)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write asset: {e}")))?;
    Ok(format!("Assets/{filename}"))
}

/// Resolve a `![[target]]` embed like Obsidian (and the diagnostics scan):
/// exact vault-relative path first, else filename match anywhere in the vault.
pub fn find_asset(vault: &Path, target: &str) -> AppResult<PathBuf> {
    use unicode_normalization::UnicodeNormalization;
    let norm = |s: &str| -> String { s.to_lowercase().nfc().collect() };
    if let Ok(abs) = resolve_rel(vault, target) {
        if abs.is_file() {
            return Ok(abs);
        }
    }
    let want = norm(target.rsplit('/').next().unwrap_or(target));
    fn walk(dir: &Path, want: &str, norm: &dyn Fn(&str) -> String) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if !is_reserved_dir(&name) {
                    if let Some(hit) = walk(&path, want, norm) {
                        return Some(hit);
                    }
                }
            } else if norm(&name) == want {
                return Some(path);
            }
        }
        None
    }
    walk(vault, &want, &norm)
        .ok_or_else(|| AppError::NotFound(format!("Asset not found: {target}")))
}

// Stub page for an auto-extracted name. No-op if a page of that title exists.
pub fn create_stub(vault: &Path, name: &str, kind: &str) -> bool {
    let name = name.trim();
    if name.is_empty() || !KINDS.contains(&kind) || page_exists(vault, name) {
        return false;
    }
    let path = unique_md_path(vault, &safe_page_filename(name));
    std::fs::write(path, page_file_content(name, kind, "", "")).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rejects_traversal() {
        let v = Path::new("/tmp/vault");
        assert!(resolve(v, "../escape.md").is_err());
        assert!(resolve(v, "/etc/passwd.md").is_err());
        assert!(resolve(v, "a/../../b.md").is_err());
        assert!(resolve(v, "notes.txt").is_err());
        assert_eq!(
            resolve(v, "Characters/Aragorn.md").unwrap(),
            v.join("Characters/Aragorn.md")
        );
    }

    #[test]
    fn snippets_seed_once_and_list() {
        let dir = std::env::temp_dir().join(format!("ck-vault-snip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_default_snippets(&dir).unwrap();
        let names: Vec<String> = list_snippets(&dir).into_iter().map(|(n, _)| n).collect();
        assert!(names.contains(&"Statblock".to_string()));
        // user edits are sacred: a second seed run must not restore deletions
        std::fs::remove_file(snippets_dir(&dir).join("Statblock.md")).unwrap();
        write_default_snippets(&dir).unwrap();
        assert!(!list_snippets(&dir).iter().any(|(n, _)| n == "Statblock"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn asset_save_and_find() {
        let dir = std::env::temp_dir().join(format!("ck-vault-assets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rel = save_asset(&dir, "shot.PNG", b"abc").unwrap();
        assert_eq!(rel, "Assets/shot.png");
        let rel2 = save_asset(&dir, "shot.png", b"def").unwrap();
        assert_eq!(rel2, "Assets/shot 2.png");
        assert!(save_asset(&dir, "noext", b"x").is_err());
        assert!(find_asset(&dir, "Assets/shot.png").is_ok());
        assert!(find_asset(&dir, "Shot 2.png").is_ok()); // bare name, case-insensitive
        assert!(find_asset(&dir, "missing.png").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn frontmatter_split_and_summary() {
        let raw = "---\nkind: npc\nsummary: A ranger.\naliases:\n  - Strider\n  - \"Elessar\"\ntags: [character/ranger, fellowship]\n---\n\n# Aragorn\n\nBody text.";
        let (fm, body) = split_frontmatter(raw);
        assert_eq!(fm_get(&fm, "kind"), Some("npc"));
        assert_eq!(fm_get(&fm, "summary"), Some("A ranger."));
        assert_eq!(fm_list(&fm, "aliases"), ["Strider", "Elessar"]);
        assert_eq!(fm_list(&fm, "tags"), ["character/ranger", "fellowship"]);
        assert!(fm_get(&fm, "missing").is_none());
        assert!(fm_list(&fm, "missing").is_empty());
        assert!(body.starts_with("# Aragorn"));
    }

    #[test]
    fn summary_empty_when_frontmatter_absent() {
        let dir = std::env::temp_dir().join(format!("ck-vault-sum-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let abs = dir.join("Note.md");
        std::fs::write(
            &abs,
            "---\nkind: lore\n---\n\n# Title\n\nFirst real paragraph here.",
        )
        .unwrap();
        let p = page_from(&dir, &abs, std::fs::read_to_string(&abs).unwrap());
        assert_eq!(p.summary, "");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn page_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ck-vault-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let created = create_page(&dir, "Rivendell", "place", None).unwrap();
        assert_eq!(created.path, "Rivendell.md");
        assert_eq!(created.kind.as_deref(), Some("place"));
        assert!(create_page(&dir, "Rivendell", "place", None).is_err());
        write_page(
            &dir,
            "Rivendell.md",
            "---\nkind: place\nsummary: Elf haven.\n---\n\nBody",
        )
        .unwrap();
        let read = read_page(&dir, "Rivendell.md").unwrap();
        assert_eq!(read.summary, "Elf haven.");
        assert_eq!(list_pages(&dir).unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn tmp_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-vault-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn folders_create_list_and_nested_page() {
        let dir = tmp_vault("folders");
        create_folder(&dir, "NPCs/Neverwinter").unwrap();
        let folders = list_folders(&dir).unwrap();
        assert!(folders.contains(&"NPCs".to_string()));
        assert!(folders.contains(&"NPCs/Neverwinter".to_string()));

        let page = create_page(&dir, "Lord Ulric", "npc", Some("NPCs/Neverwinter")).unwrap();
        assert_eq!(page.path, "NPCs/Neverwinter/Lord Ulric.md");
        assert_eq!(list_pages(&dir).unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn move_file_and_folder() {
        let dir = tmp_vault("move");
        create_page(&dir, "Aragorn", "npc", None).unwrap();
        // rename a page
        move_entry(&dir, "Aragorn.md", "Strider.md").unwrap();
        assert!(read_page(&dir, "Aragorn.md").is_err());
        assert!(read_page(&dir, "Strider.md").is_ok());
        // moving onto an existing path fails
        create_page(&dir, "Gandalf", "npc", None).unwrap();
        assert!(move_entry(&dir, "Strider.md", "Gandalf.md").is_err());
        // move a folder with children
        create_folder(&dir, "Old").unwrap();
        create_page(&dir, "Note", "lore", Some("Old")).unwrap();
        move_entry(&dir, "Old", "New").unwrap();
        assert!(read_page(&dir, "New/Note.md").is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_copies_structure_and_renames_collisions() {
        let vault = tmp_vault("import-dst");
        let src = tmp_vault("import-src");
        std::fs::create_dir_all(src.join("People")).unwrap();
        std::fs::create_dir_all(src.join(".obsidian")).unwrap();
        std::fs::create_dir_all(src.join("Sessions")).unwrap();
        std::fs::write(src.join("People/Aragorn.md"), "# Aragorn\n").unwrap();
        std::fs::write(src.join("Rivendell.md"), "# Rivendell\n").unwrap();
        std::fs::write(src.join(".obsidian/app.json"), "{}").unwrap();
        std::fs::write(src.join("Sessions/notes.md"), "skip\n").unwrap();
        std::fs::write(src.join("People/aragorn.PNG"), b"img").unwrap();
        std::fs::write(src.join("map.pdf"), b"pdf").unwrap();
        std::fs::write(src.join("notes.txt"), "not an asset\n").unwrap();
        create_page(&vault, "Rivendell", "place", None).unwrap();

        let r = import_folder(&vault, &src).unwrap();
        assert_eq!((r.imported, r.renamed, r.assets), (2, 1, 2));
        assert!(vault.join("People/Aragorn.md").is_file());
        assert!(vault.join("Rivendell-2.md").is_file()); // collision suffixed
        assert!(!vault.join("Sessions/notes.md").exists()); // reserved skipped
        assert!(vault.join("People/aragorn.PNG").is_file()); // media imported
        assert!(vault.join("map.pdf").is_file());
        assert!(!vault.join("notes.txt").exists()); // unknown extension skipped
                                                    // source untouched
        assert!(src.join("People/Aragorn.md").is_file());

        // overlap + empty-source guards
        assert!(import_folder(&vault, &vault).is_err());
        let empty = tmp_vault("import-empty");
        assert!(import_folder(&vault, &empty).is_err());
        for d in [&vault, &src, &empty] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn adopt_foreign_vault_is_additive() {
        let dir = tmp_vault("adopt");
        std::fs::create_dir_all(dir.join("People")).unwrap();
        std::fs::write(dir.join("People/Aragorn.md"), "# Aragorn\n").unwrap();
        assert_eq!(adopt_vault_layout(&dir).unwrap(), ".");
        assert!(!dir.join("Codex").exists());
        assert!(dir.join("Sessions").is_dir());
        assert!(dir.join(".ck").is_dir());
        // reserved dirs stay out of the page tree
        std::fs::write(dir.join("Sessions/transcript.md"), "x\n").unwrap();
        let pages = list_pages(&dir).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].path, "People/Aragorn.md");
        assert!(create_page(&dir, "Note", "lore", Some("Sessions")).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn adopt_empty_or_canonical_gets_fresh_layout() {
        let dir = tmp_vault("adopt-empty");
        assert_eq!(adopt_vault_layout(&dir).unwrap(), "Codex");
        assert!(dir.join("Codex").is_dir());
        // canonical layout (Codex/ present) stays canonical even with pages
        std::fs::write(dir.join("Codex/Note.md"), "x\n").unwrap();
        assert_eq!(adopt_vault_layout(&dir).unwrap(), "Codex");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn templates_default_and_create_from_template() {
        let dir = tmp_vault("templates");
        write_default_templates(&dir).unwrap();
        let npc = std::fs::read_to_string(templates_dir(&dir).join("npc.md")).unwrap();
        assert!(npc.contains("kind: npc"));
        assert!(npc.contains("affiliation: []"));
        assert!(npc.contains("# {{title}}"));
        assert!(npc.contains("## Appearance"));
        assert!(npc.contains("## History"));
        let item = std::fs::read_to_string(templates_dir(&dir).join("item.md")).unwrap();
        assert!(item.contains("magical: false"));
        let lore = std::fs::read_to_string(templates_dir(&dir).join("lore.md")).unwrap();
        assert!(!lore.contains("## "));

        // user edit survives a re-provision
        std::fs::write(
            templates_dir(&dir).join("npc.md"),
            "---\nkind: npc\nsummary:\nvoice:\n---\n\n# {{title}}\n",
        )
        .unwrap();
        write_default_templates(&dir).unwrap();
        let kept = std::fs::read_to_string(templates_dir(&dir).join("npc.md")).unwrap();
        assert!(kept.contains("voice:"));

        // create uses the template, substituting the title
        let fields = crate::world_config::WorldConfig::default()
            .kind_schemas()
            .into_iter()
            .find(|(k, _)| k == "npc")
            .map(|(_, f)| f)
            .unwrap();
        let content = new_page_content(&dir, "npc", "Lord Ulric", &fields);
        assert!(content.contains("voice:"));
        assert!(content.contains("# Lord Ulric"));
        let page = create_page_with(&dir, "Lord Ulric", None, &content).unwrap();
        assert_eq!(page.kind.as_deref(), Some("npc"));

        // missing template → schema-derived default
        std::fs::remove_file(templates_dir(&dir).join("place.md")).unwrap();
        let pf = crate::world_config::WorldConfig::default()
            .kind_schemas()
            .into_iter()
            .find(|(k, _)| k == "place")
            .map(|(_, f)| f)
            .unwrap();
        let content = new_page_content(&dir, "place", "Rivendell", &pf);
        assert!(content.contains("region:"));
        assert!(content.contains("# Rivendell"));
        assert!(content.contains("## At a glance"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn templates_list_write_delete_and_from_template() {
        let dir = tmp_vault("templates-crud");
        write_default_templates(&dir).unwrap();

        // a user-created custom template targeting an existing kind
        write_template(
            &dir,
            "villain",
            "---\nkind: npc\nsummary:\n---\n\n# {{title}}\n\n## Scheme\n",
        )
        .unwrap();
        let names: Vec<String> = list_templates(&dir).into_iter().map(|(n, _)| n).collect();
        assert!(names.contains(&"villain".to_string()));
        assert!(names.contains(&"npc".to_string()));

        let (body, kind) = new_page_from_template(&dir, "villain", "Sauron").unwrap();
        assert_eq!(kind, "npc");
        assert!(body.contains("# Sauron"));
        assert!(body.contains("## Scheme"));
        assert!(!body.contains("{{title}}"));

        delete_template(&dir, "villain").unwrap();
        assert!(new_page_from_template(&dir, "villain", "x").is_none());

        // name guards
        assert!(write_template(&dir, "../evil", "x").is_err());
        assert!(template_path(&dir, "a/b").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_templates_migrate_to_new_dir() {
        let dir = tmp_vault("templates-migrate");
        let legacy = dir.join(".ck").join("templates");
        std::fs::create_dir_all(legacy.join("snippets")).unwrap();
        std::fs::write(
            legacy.join("npc.md"),
            "---\nkind: npc\nvoice:\n---\n\n# {{title}}\n",
        )
        .unwrap();
        std::fs::write(legacy.join("snippets").join("Statblock.md"), "snip\n").unwrap();

        write_default_templates(&dir).unwrap();
        // the .md moved into _templates, the user edit preserved
        let npc = std::fs::read_to_string(templates_dir(&dir).join("npc.md")).unwrap();
        assert!(npc.contains("voice:"));
        assert!(!legacy.join("npc.md").exists());
        // snippets subdir untouched in its old home
        assert!(legacy.join("snippets").join("Statblock.md").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn promote_sets_kind_fills_fields_drops_inbox_appends_headings() {
        let fields = crate::world_config::WorldConfig::default()
            .kind_schemas()
            .into_iter()
            .find(|(k, _)| k == "npc")
            .map(|(_, f)| f)
            .unwrap();
        let headings: Vec<String> = default_headings("npc")
            .iter()
            .map(|s| s.to_string())
            .collect();

        let capture = "---\nkind: lore\nsummary: a stranger\ntags: [inbox, town]\nrace: human\n---\n\n# Stranger\n\nMet at the gate.\n\n## History\n\nUnknown.\n";
        let out = promote_content(capture, "npc", &fields, &headings);
        assert!(out.contains("kind: npc"));
        assert!(out.contains("tags: [town]"));
        assert!(out.contains("summary: a stranger"));
        assert!(out.contains("race: human")); // existing value kept, not blanked
        assert!(out.contains("affiliation: []"));
        assert!(out.contains("status:"));
        assert!(out.contains("Met at the gate."));
        assert!(out.contains("## Appearance"));
        assert_eq!(out.matches("## History").count(), 1); // already present → not duplicated

        // block-list tags, inbox only → tags key dropped entirely
        let block = "---\nkind: lore\ntags:\n  - inbox\n---\n\nBody.\n";
        let out = promote_content(block, "npc", &fields, &[]);
        assert!(!out.contains("tags"));
        assert!(out.contains("kind: npc"));

        // no frontmatter at all → one is created
        let bare = "# Loose note\n\nJust text.\n";
        let out = promote_content(bare, "place", &[], &[]);
        assert!(out.starts_with("---\nkind: place\nsummary:\n---\n"));
        assert!(out.contains("Just text."));
    }

    #[test]
    fn untouched_legacy_template_upgrades_edited_kept() {
        let dir = tmp_vault("tpl-upgrade");
        std::fs::create_dir_all(templates_dir(&dir)).unwrap();
        let fields = crate::world_config::WorldConfig::default()
            .kind_schemas()
            .into_iter()
            .find(|(k, _)| k == "npc")
            .map(|(_, f)| f)
            .unwrap();
        // pre-Phase-16 seed (no headings) → upgraded
        std::fs::write(
            templates_dir(&dir).join("npc.md"),
            template_base("npc", &fields),
        )
        .unwrap();
        // user-edited file → untouched; deleted file → not recreated
        std::fs::write(
            templates_dir(&dir).join("item.md"),
            "---\nkind: item\n---\n\n# {{title}}\nMine\n",
        )
        .unwrap();
        write_default_templates(&dir).unwrap();
        let npc = std::fs::read_to_string(templates_dir(&dir).join("npc.md")).unwrap();
        assert!(npc.contains("## Appearance"));
        let item = std::fs::read_to_string(templates_dir(&dir).join("item.md")).unwrap();
        assert!(item.contains("Mine"));
        assert!(!templates_dir(&dir).join("place.md").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_and_traversal_guards() {
        let dir = tmp_vault("delete");
        create_page(&dir, "Doomed", "lore", None).unwrap();
        delete_page(&dir, "Doomed.md").unwrap();
        assert!(read_page(&dir, "Doomed.md").is_err());
        assert!(delete_page(&dir, "Doomed.md").is_err());
        create_folder(&dir, "Archive").unwrap();
        create_page(&dir, "Kept", "lore", Some("Archive")).unwrap();
        create_page(&dir, "Gone", "lore", Some("Archive")).unwrap();
        assert_eq!(page_paths_in_folder(&dir, "Archive").unwrap().len(), 2);
        delete_folder(&dir, "Archive").unwrap();
        assert!(!dir.join("Archive").exists());
        assert!(read_page(&dir, "Kept.md").is_err());
        // folder + move resolvers reject traversal / dotfiles
        assert!(create_folder(&dir, "../escape").is_err());
        assert!(create_folder(&dir, ".ck").is_err());
        assert!(move_entry(&dir, "a.md", "../b.md").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
