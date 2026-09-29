//! World packs: a shareable zip of a world's pages, media, templates, kind
//! schemas and Atlas maps. Sessions, transcripts, audio, players and every
//! secret stay out — the pack holds world content only.
//!
//! Zip layout: `manifest.json` + `files/<pack path>`, pack paths being
//! `Codex/…`, `_templates/…`, `Atlas/…` or `config/fragment.toml`.
//!
//! Import is a pure three-way plan (base = what the last import of this pack
//! installed, mine = the world now, theirs = the pack); nothing is written
//! until `apply`, which journals every file it replaces so `rollback` can put
//! the world back exactly.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::atlas::{self, MapDoc};
use crate::error::{AppError, AppResult};
use crate::world_config::{self, CalendarConfig, KindOverride, WorldConfig};

pub const FORMAT: u32 = 1;
const MAX_FILES: usize = 50_000;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const PACKS_DIR: &str = "Packs";
const FRAGMENT_PATH: &str = "config/fragment.toml";
/// Plan/journal key of the kind-schema + calendar fragment inside `.ck/config.toml`.
const FRAGMENT_KEY: &str = ".ck/config.toml#fragment";
const CONFIG_KEY: &str = ".ck/config.toml";

fn bad(msg: impl Into<String>) -> AppError {
    AppError::BadRequest(msg.into())
}

fn internal(ctx: &str, e: impl std::fmt::Display) -> AppError {
    AppError::Internal(anyhow::anyhow!("{ctx}: {e}"))
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// ── Format ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Page,
    Asset,
    Template,
    AtlasMap,
    AtlasArt,
    KindConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub role: Role,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub created_at: String,
    pub files: Vec<ManifestFile>,
}

pub struct Pack {
    pub manifest: Manifest,
    pub files: HashMap<String, Vec<u8>>,
}

fn safe_components(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 300
        && !path.contains('\\')
        && !path.starts_with('/')
        && path.split('/').all(|c| {
            !c.is_empty()
                && c != "."
                && c != ".."
                && !c.starts_with('.')
                && !c.contains(':')
                && !c.contains('\0')
        })
}

fn is_md(p: &str) -> bool {
    p.to_lowercase().ends_with(".md")
}

fn is_atlas_image(p: &str) -> bool {
    Path::new(p)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| atlas::IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
}

/// Whether `path` is an allowed pack path for `role`. Everything else — `..`,
/// dot-components (so `.ck/`), other roots, wrong extensions — is refused.
pub fn valid_entry(path: &str, role: Role) -> bool {
    if !safe_components(path) {
        return false;
    }
    let Some((root, rest)) = path.split_once('/') else {
        return false;
    };
    match (root, role) {
        ("Codex", Role::Page) => is_md(path),
        ("Codex", Role::Asset) => !is_md(path) && crate::vault::is_asset(Path::new(path)),
        ("_templates", Role::Template) => !rest.contains('/') && is_md(rest),
        ("Atlas", Role::AtlasMap) => {
            !rest.contains('/') && rest.strip_suffix(".json").is_some_and(atlas::valid_id)
        }
        ("Atlas", Role::AtlasArt) => !rest.contains('/') && is_atlas_image(rest),
        ("config", Role::KindConfig) => path == FRAGMENT_PATH,
        _ => false,
    }
}

fn valid_pack_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    out.chars().take(48).collect()
}

// ── Reading a pack ───────────────────────────────────────────────

pub fn read_pack(path: &Path) -> AppResult<Pack> {
    let file = std::fs::File::open(path)
        .map_err(|e| bad(format!("Cannot open pack {}: {e}", path.display())))?;
    let mut ar = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| bad(format!("Not a world pack (bad zip): {e}")))?;
    if ar.len() > MAX_FILES + 1 {
        return Err(bad("Pack has too many files"));
    }
    let mut manifest: Option<Manifest> = None;
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut total = 0u64;
    for i in 0..ar.len() {
        let mut entry = ar
            .by_index(i)
            .map_err(|e| bad(format!("Unreadable pack entry: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        entry
            .by_ref()
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| bad(format!("Cannot read {name}: {e}")))?;
        if buf.len() as u64 > MAX_FILE_BYTES {
            return Err(bad(format!("{name} is too large for a pack")));
        }
        total += buf.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(bad("Pack is too large"));
        }
        if name == "manifest.json" {
            manifest = Some(
                serde_json::from_slice(&buf)
                    .map_err(|e| bad(format!("manifest.json is not valid: {e}")))?,
            );
        } else if let Some(rest) = name.strip_prefix("files/") {
            if files.insert(rest.to_string(), buf).is_some() {
                return Err(bad(format!("Duplicate entry {name}")));
            }
        } else {
            return Err(bad(format!("Unexpected entry in pack: {name}")));
        }
    }
    let manifest = manifest.ok_or_else(|| bad("Pack has no manifest.json"))?;
    validate(&manifest, &files)?;
    Ok(Pack { manifest, files })
}

fn validate(m: &Manifest, files: &HashMap<String, Vec<u8>>) -> AppResult<()> {
    if m.format == 0 || m.format > FORMAT {
        return Err(bad(format!(
            "Pack format {} is newer than this app understands ({FORMAT})",
            m.format
        )));
    }
    if !valid_pack_id(&m.id) {
        return Err(bad("Pack id is not valid"));
    }
    if m.name.trim().is_empty() {
        return Err(bad("Pack has no name"));
    }
    let mut seen = std::collections::HashSet::new();
    for f in &m.files {
        if !valid_entry(&f.path, f.role) {
            return Err(bad(format!("Pack path not allowed: {}", f.path)));
        }
        if !seen.insert(f.path.as_str()) {
            return Err(bad(format!("Duplicate pack path: {}", f.path)));
        }
        let bytes = files
            .get(&f.path)
            .ok_or_else(|| bad(format!("Pack is missing {}", f.path)))?;
        if sha_hex(bytes) != f.sha256 {
            return Err(bad(format!("Checksum mismatch for {}", f.path)));
        }
    }
    if let Some(extra) = files.keys().find(|k| !seen.contains(k.as_str())) {
        return Err(bad(format!("Pack has an unlisted file: {extra}")));
    }
    Ok(())
}

// ── Export ───────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct ExportOpts {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub id: Option<String>,
    /// Codex folders to include (`None` = the whole Codex).
    #[serde(default)]
    pub folders: Option<Vec<String>>,
    #[serde(default)]
    pub include_atlas: bool,
}

/// The shareable slice of a world's config: kind schemas + calendar. Players,
/// GM and every other key stay behind.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Fragment {
    #[serde(default)]
    kinds: BTreeMap<String, KindOverride>,
    #[serde(default)]
    calendar: CalendarConfig,
}

fn fragment_text(cfg: &WorldConfig) -> Option<String> {
    if cfg.kinds.is_empty() && cfg.calendar == CalendarConfig::default() {
        return None;
    }
    toml::to_string_pretty(&Fragment {
        kinds: cfg.kinds.clone(),
        calendar: cfg.calendar.clone(),
    })
    .ok()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || crate::vault::is_reserved_dir(&name) {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn rel_of(base: &Path, abs: &Path) -> String {
    abs.strip_prefix(base)
        .unwrap_or(abs)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Write a pack zip to `<world>/Packs/` and return its path.
pub fn export_pack(
    world_root: &Path,
    cfg: &WorldConfig,
    opts: &ExportOpts,
) -> AppResult<(PathBuf, Manifest)> {
    let name = opts.name.trim();
    if name.is_empty() {
        return Err(bad("A pack name is required"));
    }
    let id = match opts.id.as_deref().map(slug).filter(|s| !s.is_empty()) {
        Some(id) => id,
        None => match slug(name) {
            s if s.is_empty() => "world-pack".to_string(),
            s => s,
        },
    };
    let codex = cfg.codex_dir(world_root);
    let in_folders = |rel: &str| match &opts.folders {
        None => true,
        Some(fs) => fs.iter().any(|f| {
            let f = f.trim().trim_matches('/');
            f.is_empty() || rel.starts_with(&format!("{f}/"))
        }),
    };

    let mut entries: Vec<(String, Role, Vec<u8>)> = Vec::new();
    let mut page_text = String::new();
    for page in crate::vault::list_pages(&codex)? {
        if page.kind.as_deref() == Some(crate::prep_page::KIND) || !in_folders(&page.path) {
            continue;
        }
        let bytes = std::fs::read(codex.join(&page.path))
            .map_err(|e| internal(&format!("read {}", page.path), e))?;
        page_text.push_str(&String::from_utf8_lossy(&bytes));
        entries.push((format!("Codex/{}", page.path), Role::Page, bytes));
    }
    // media travels only when an exported page mentions it by file name
    let mut media = Vec::new();
    walk(&codex, &mut media);
    for abs in media {
        let rel = rel_of(&codex, &abs);
        let fname = abs.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_md(&rel)
            || !crate::vault::is_asset(&abs)
            || !in_folders(&rel) && !rel.starts_with("Assets/")
        {
            continue;
        }
        if !fname.is_empty() && page_text.contains(fname) {
            let bytes = std::fs::read(&abs).map_err(|e| internal(&format!("read {rel}"), e))?;
            entries.push((format!("Codex/{rel}"), Role::Asset, bytes));
        }
    }
    if let Ok(rd) = std::fs::read_dir(crate::vault::templates_dir(world_root)) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if is_md(&n) && !n.starts_with('.') && e.path().is_file() {
                let bytes = std::fs::read(e.path()).map_err(|e| internal("read template", e))?;
                entries.push((format!("_templates/{n}"), Role::Template, bytes));
            }
        }
    }
    if opts.include_atlas {
        for doc in atlas::list_maps(world_root)? {
            let Ok(img) = atlas::image_path(world_root, &doc) else {
                continue;
            };
            let Ok(art) = std::fs::read(&img) else {
                continue;
            };
            let json = serde_json::to_vec_pretty(&doc).map_err(|e| internal("map json", e))?;
            entries.push((format!("Atlas/{}.json", doc.id), Role::AtlasMap, json));
            entries.push((format!("Atlas/{}", doc.image), Role::AtlasArt, art));
        }
    }
    if let Some(frag) = fragment_text(cfg) {
        entries.push((
            FRAGMENT_PATH.to_string(),
            Role::KindConfig,
            frag.into_bytes(),
        ));
    }
    entries.retain(|(p, r, _)| valid_entry(p, *r));
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if entries.is_empty() {
        return Err(bad("Nothing to put in the pack"));
    }

    let manifest = Manifest {
        format: FORMAT,
        id: id.clone(),
        name: name.to_string(),
        description: opts.description.trim().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        files: entries
            .iter()
            .map(|(p, r, b)| ManifestFile {
                path: p.clone(),
                role: *r,
                sha256: sha_hex(b),
            })
            .collect(),
    };

    let dir = world_root.join(PACKS_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| internal("create Packs/", e))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut out = dir.join(format!("{id}-{stamp}.zip"));
    let mut n = 2;
    while out.exists() {
        out = dir.join(format!("{id}-{stamp}-{n}.zip"));
        n += 1;
    }
    let mut w = zip::ZipWriter::new(
        std::fs::File::create(&out).map_err(|e| internal("create pack zip", e))?,
    );
    let zopts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    let mut put = |name: String, bytes: &[u8]| -> AppResult<()> {
        w.start_file(name.clone(), zopts)
            .and_then(|()| w.write_all(bytes).map_err(Into::into))
            .map_err(|e| internal(&format!("zip {name}"), e))
    };
    put(
        "manifest.json".into(),
        &serde_json::to_vec_pretty(&manifest).map_err(|e| internal("manifest", e))?,
    )?;
    for (p, _, b) in &entries {
        put(format!("files/{p}"), b)?;
    }
    w.finish().map_err(|e| internal("finish pack zip", e))?;
    Ok((out, manifest))
}

// ── Install set: what the pack becomes inside a target world ─────

#[derive(Debug, Clone)]
pub struct InstallFile {
    pub target: String,
    pub role: Role,
    pub bytes: Vec<u8>,
    pub sha: String,
}

fn clean_dest(dest: &str) -> AppResult<String> {
    let d = dest.trim().trim_matches('/');
    if d.is_empty() {
        return Ok(String::new());
    }
    if safe_components(d) {
        Ok(d.to_string())
    } else {
        Err(bad("Destination folder is not valid"))
    }
}

fn join_rel(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("/")
}

fn prefix_page_ref(reference: &str, dest: &str) -> String {
    if dest.is_empty() || reference.is_empty() {
        reference.to_string()
    } else {
        format!("{dest}/{reference}")
    }
}

fn existing_map_ids(world_root: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(world_root.join(atlas::ATLAS_DIR)) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.strip_suffix(".json").map(str::to_string)
        })
        .collect()
}

pub fn resolve(
    pack: &Pack,
    world_root: &Path,
    cfg: &WorldConfig,
    dest: &str,
    prev_ids: &BTreeMap<String, String>,
) -> AppResult<(Vec<InstallFile>, BTreeMap<String, String>)> {
    let dest = clean_dest(dest)?;
    let codex_rel = rel_of(world_root, &cfg.codex_dir(world_root));
    let codex_rel = if codex_rel == "." {
        String::new()
    } else {
        codex_rel
    };

    // pack map ids → free ids in the target; a re-import keeps the ids it owns
    let existing = existing_map_ids(world_root);
    let mut taken: Vec<String> = existing.clone();
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut art_map: HashMap<String, String> = HashMap::new();
    for f in pack
        .manifest
        .files
        .iter()
        .filter(|f| f.role == Role::AtlasMap)
    {
        let id = f.path["Atlas/".len()..f.path.len() - ".json".len()].to_string();
        // a re-import keeps the id this pack's map got the first time
        let mut new_id = prev_ids.get(&id).cloned().unwrap_or_else(|| id.clone());
        let mut n = 2;
        while !prev_ids.contains_key(&id) && taken.contains(&new_id)
            || id_map.values().any(|v| *v == new_id)
        {
            new_id = format!("{id}-{n}");
            n += 1;
        }
        taken.push(new_id.clone());
        id_map.insert(id, new_id);
    }

    let mut out = Vec::new();
    let mut push = |target: String, role: Role, bytes: Vec<u8>| {
        let sha = sha_hex(&bytes);
        out.push(InstallFile {
            target,
            role,
            bytes,
            sha,
        });
    };
    // maps first, so art can follow the (possibly renamed) map that uses it
    for f in pack
        .manifest
        .files
        .iter()
        .filter(|f| f.role == Role::AtlasMap)
    {
        let raw = &pack.files[&f.path];
        let mut doc: MapDoc = serde_json::from_slice(raw)
            .map_err(|e| bad(format!("Map {} is not valid: {e}", f.path)))?;
        let old_id = doc.id.clone();
        let new_id = id_map
            .get(&old_id)
            .cloned()
            .unwrap_or_else(|| old_id.clone());
        if new_id != old_id {
            let ext = Path::new(&doc.image)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("png");
            let new_image = format!("{new_id}.{ext}");
            art_map.insert(doc.image.clone(), new_image.clone());
            doc.image = new_image;
            doc.id = new_id.clone();
        }
        doc.parent = doc.parent.map(|p| id_map.get(&p).cloned().unwrap_or(p));
        doc.page = doc.page.map(|p| prefix_page_ref(&p, &dest));
        for pin in &mut doc.pins {
            pin.to = pin.to.take().map(|t| id_map.get(&t).cloned().unwrap_or(t));
            pin.page = pin.page.take().map(|p| prefix_page_ref(&p, &dest));
        }
        let json = serde_json::to_vec_pretty(&doc).map_err(|e| internal("map json", e))?;
        push(format!("Atlas/{new_id}.json"), Role::AtlasMap, json);
    }
    for f in &pack.manifest.files {
        let bytes = pack.files[&f.path].clone();
        match f.role {
            Role::Page | Role::Asset => {
                let rest = &f.path["Codex/".len()..];
                push(join_rel(&[&codex_rel, &dest, rest]), f.role, bytes);
            }
            Role::Template => push(f.path.clone(), f.role, bytes),
            Role::AtlasArt => {
                let name = &f.path["Atlas/".len()..];
                let name = art_map.get(name).map(String::as_str).unwrap_or(name);
                push(format!("Atlas/{name}"), f.role, bytes);
            }
            Role::KindConfig => push(FRAGMENT_KEY.to_string(), f.role, bytes),
            Role::AtlasMap => {}
        }
    }
    Ok((out, id_map.into_iter().collect()))
}

/// Sha of what the world holds at `target` now.
fn current_sha(world_root: &Path, cfg: &WorldConfig, target: &str) -> Option<String> {
    if target == FRAGMENT_KEY {
        return fragment_text(cfg).map(|t| sha_hex(t.as_bytes()));
    }
    std::fs::read(world_root.join(target))
        .ok()
        .map(|b| sha_hex(&b))
}

// ── Plan ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Unchanged,
    Added,
    Updated,
    Removed,
    /// The world's copy was edited and the pack didn't change: left alone.
    Kept,
    /// Both sides changed (or the file is unknown to this pack): mine wins unless overridden.
    Conflict,
    /// Installed before, deleted since, still in the pack.
    Restored,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanItem {
    pub target: String,
    pub role: Role,
    pub status: Status,
    pub default_apply: bool,
    pub theirs: Option<String>,
    pub mine: Option<String>,
    pub base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledFile {
    pub sha: String,
    pub role: Role,
}

/// Pure three-way plan. `theirs` = target → (role, sha) of the pack's files,
/// `base` = what the last import installed, `mine` = current sha per target.
pub fn plan(
    theirs: &BTreeMap<String, (Role, String)>,
    base: &BTreeMap<String, InstalledFile>,
    mine: &dyn Fn(&str) -> Option<String>,
) -> Vec<PlanItem> {
    let mut items = Vec::new();
    let mut item = |target: &str, role, status: Status, t: Option<&String>, m: Option<String>| {
        let default_apply = matches!(status, Status::Added | Status::Updated | Status::Removed);
        items.push(PlanItem {
            target: target.to_string(),
            role,
            status,
            default_apply,
            theirs: t.cloned(),
            mine: m,
            base: base.get(target).map(|b| b.sha.clone()),
        });
    };
    for (target, (role, t)) in theirs {
        let m = mine(target);
        let b = base.get(target).map(|b| &b.sha);
        let status = match (&m, b) {
            (None, None) => Status::Added,
            (None, Some(_)) => Status::Restored,
            (Some(m), _) if m == t => Status::Unchanged,
            (Some(_), None) => Status::Conflict,
            (Some(m), Some(b)) if m == b => Status::Updated,
            (Some(_), Some(b)) if b == t => Status::Kept,
            (Some(_), Some(_)) => Status::Conflict,
        };
        item(target, *role, status, Some(t), m);
    }
    for (target, b) in base {
        if theirs.contains_key(target) {
            continue;
        }
        match mine(target) {
            None => {}
            Some(m) if m == b.sha => item(target, b.role, Status::Removed, None, Some(m)),
            Some(m) => item(target, b.role, Status::Kept, None, Some(m)),
        }
    }
    items.sort_by(|a, b| a.target.cmp(&b.target));
    items
}

// ── Installed records + journal ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    pub pack_id: String,
    pub name: String,
    pub created_at: String,
    pub installed_at: String,
    pub dest: String,
    pub files: BTreeMap<String, InstalledFile>,
    /// Pack map id → the id it was installed under (renamed on collision).
    #[serde(default)]
    pub map_ids: BTreeMap<String, String>,
}

fn pack_dir(world_root: &Path, pack_id: &str) -> AppResult<PathBuf> {
    if !valid_pack_id(pack_id) {
        return Err(bad("Invalid pack id"));
    }
    Ok(world_root.join(".ck").join("packs").join(pack_id))
}

fn read_installed(world_root: &Path, pack_id: &str) -> AppResult<Option<Installed>> {
    let path = pack_dir(world_root, pack_id)?.join("installed.json");
    match std::fs::read(&path) {
        Ok(b) => Ok(Some(
            serde_json::from_slice(&b).map_err(|e| internal("installed.json", e))?,
        )),
        Err(_) => Ok(None),
    }
}

fn write_installed(world_root: &Path, inst: &Installed) -> AppResult<()> {
    let dir = pack_dir(world_root, &inst.pack_id)?;
    std::fs::create_dir_all(&dir).map_err(|e| internal("create pack dir", e))?;
    std::fs::write(
        dir.join("installed.json"),
        serde_json::to_vec_pretty(inst).map_err(|e| internal("installed.json", e))?,
    )
    .map_err(|e| internal("write installed.json", e))
}

#[derive(Debug, Serialize, Deserialize)]
struct JournalEntry {
    target: String,
    existed_before: bool,
    /// Sha the import left behind (`None` = the import deleted the file).
    after_sha: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Journal {
    created_at: String,
    entries: Vec<JournalEntry>,
    installed_before: Option<Installed>,
}

fn latest_journal(world_root: &Path, pack_id: &str) -> AppResult<Option<PathBuf>> {
    let dir = pack_dir(world_root, pack_id)?.join("journal");
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(None);
    };
    let mut names: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".undone"))
        })
        .collect();
    names.sort();
    Ok(names.pop())
}

pub struct InstalledSummary {
    pub pack_id: String,
    pub name: String,
    pub installed_at: String,
    pub dest: String,
    pub files: usize,
    pub can_rollback: bool,
}

pub fn list_installed(world_root: &Path) -> Vec<InstalledSummary> {
    let Ok(rd) = std::fs::read_dir(world_root.join(".ck").join("packs")) else {
        return Vec::new();
    };
    let mut out: Vec<InstalledSummary> = rd
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().to_string();
            let inst = read_installed(world_root, &id).ok().flatten();
            let can_rollback = latest_journal(world_root, &id).ok().flatten().is_some();
            if inst.is_none() && !can_rollback {
                return None;
            }
            Some(match inst {
                Some(i) => InstalledSummary {
                    pack_id: id,
                    name: i.name,
                    installed_at: i.installed_at,
                    dest: i.dest,
                    files: i.files.len(),
                    can_rollback,
                },
                None => InstalledSummary {
                    pack_id: id.clone(),
                    name: id,
                    installed_at: String::new(),
                    dest: String::new(),
                    files: 0,
                    can_rollback,
                },
            })
        })
        .collect();
    out.sort_by(|a, b| a.pack_id.cmp(&b.pack_id));
    out
}

// ── Plan for a pack + target world ───────────────────────────────

pub struct Prepared {
    pub install: Vec<InstallFile>,
    pub items: Vec<PlanItem>,
    pub base: Option<Installed>,
    pub dest: String,
    pub map_ids: BTreeMap<String, String>,
}

pub fn prepare(
    pack: &Pack,
    world_root: &Path,
    cfg: &WorldConfig,
    dest: &str,
) -> AppResult<Prepared> {
    let dest = clean_dest(dest)?;
    let base = read_installed(world_root, &pack.manifest.id)?;
    let base_files = base.as_ref().map(|b| b.files.clone()).unwrap_or_default();
    let prev_ids = base.as_ref().map(|b| b.map_ids.clone()).unwrap_or_default();
    let (install, map_ids) = resolve(pack, world_root, cfg, &dest, &prev_ids)?;
    let theirs: BTreeMap<String, (Role, String)> = install
        .iter()
        .map(|f| (f.target.clone(), (f.role, f.sha.clone())))
        .collect();
    let items = plan(&theirs, &base_files, &|t| current_sha(world_root, cfg, t));
    Ok(Prepared {
        install,
        items,
        base,
        dest,
        map_ids,
    })
}

// ── Apply ────────────────────────────────────────────────────────

pub struct ApplyResult {
    pub applied: usize,
    pub skipped: usize,
    pub journal: String,
}

fn merge_fragment(cfg: &mut WorldConfig, bytes: &[u8]) -> AppResult<()> {
    let frag: Fragment = toml::from_str(&String::from_utf8_lossy(bytes))
        .map_err(|e| bad(format!("Kind schema fragment is not valid: {e}")))?;
    cfg.kinds.extend(frag.kinds);
    if frag.calendar != CalendarConfig::default() {
        cfg.calendar = frag.calendar;
    }
    Ok(())
}

/// Apply the plan. `overrides` maps target → apply?; anything absent follows
/// the item's default (conflicts keep the world's copy).
pub fn apply(
    pack: &Pack,
    world_root: &Path,
    dest: &str,
    overrides: &HashMap<String, bool>,
) -> AppResult<ApplyResult> {
    let mut cfg = world_config::read(world_root)?.unwrap_or_default();
    let prepared = prepare(pack, world_root, &cfg, dest)?;
    let id = &pack.manifest.id;
    let by_target: HashMap<&str, &InstallFile> = prepared
        .install
        .iter()
        .map(|f| (f.target.as_str(), f))
        .collect();

    let chosen: Vec<&PlanItem> = prepared
        .items
        .iter()
        .filter(|i| {
            !matches!(i.status, Status::Unchanged | Status::Kept)
                && overrides.get(&i.target).copied().unwrap_or(i.default_apply)
        })
        .collect();

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%3f").to_string();
    let jdir = pack_dir(world_root, id)?.join("journal").join(&stamp);
    let blobs = jdir.join("files");
    std::fs::create_dir_all(&blobs).map_err(|e| internal("create journal", e))?;

    let mut journal = Journal {
        created_at: chrono::Utc::now().to_rfc3339(),
        entries: Vec::new(),
        installed_before: prepared.base.clone(),
    };
    let save_blob = |target: &str| -> AppResult<bool> {
        let src = world_root.join(target);
        if !src.is_file() {
            return Ok(false);
        }
        let dst = blobs.join(target);
        std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| internal("journal dir", e))?;
        std::fs::copy(&src, &dst).map_err(|e| internal(&format!("journal {target}"), e))?;
        Ok(true)
    };
    let write_journal = |j: &Journal| -> AppResult<()> {
        std::fs::write(
            jdir.join("journal.json"),
            serde_json::to_vec_pretty(j).map_err(|e| internal("journal", e))?,
        )
        .map_err(|e| internal("write journal", e))
    };

    // journal every touched file before writing any of them
    let mut config_touched = false;
    for item in &chosen {
        if item.target == FRAGMENT_KEY {
            config_touched = true;
            continue;
        }
        let existed = save_blob(&item.target)?;
        journal.entries.push(JournalEntry {
            target: item.target.clone(),
            existed_before: existed,
            after_sha: by_target.get(item.target.as_str()).map(|f| f.sha.clone()),
        });
    }
    if config_touched {
        let existed = save_blob(CONFIG_KEY)?;
        journal.entries.push(JournalEntry {
            target: CONFIG_KEY.to_string(),
            existed_before: existed,
            after_sha: None,
        });
    }
    write_journal(&journal)?;

    for item in &chosen {
        if item.target == FRAGMENT_KEY {
            merge_fragment(&mut cfg, &by_target[FRAGMENT_KEY].bytes)?;
            world_config::write(world_root, &cfg)?;
            let sha = std::fs::read(world_config::config_path(world_root))
                .ok()
                .map(|b| sha_hex(&b));
            if let Some(e) = journal.entries.iter_mut().find(|e| e.target == CONFIG_KEY) {
                e.after_sha = sha;
            }
            continue;
        }
        let path = world_root.join(&item.target);
        match by_target.get(item.target.as_str()) {
            Some(f) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| internal("create dir", e))?;
                }
                std::fs::write(&path, &f.bytes)
                    .map_err(|e| internal(&format!("write {}", item.target), e))?;
            }
            None => {
                std::fs::remove_file(&path)
                    .map_err(|e| internal(&format!("remove {}", item.target), e))?;
            }
        }
    }
    write_journal(&journal)?;

    let applied: std::collections::HashSet<&str> =
        chosen.iter().map(|i| i.target.as_str()).collect();
    let mut files = prepared
        .base
        .as_ref()
        .map(|b| b.files.clone())
        .unwrap_or_default();
    for item in &prepared.items {
        let done = applied.contains(item.target.as_str());
        match (item.status, by_target.get(item.target.as_str())) {
            (Status::Removed, None) if done => {
                files.remove(&item.target);
            }
            (Status::Unchanged, Some(f)) => {
                files.insert(
                    f.target.clone(),
                    InstalledFile {
                        sha: f.sha.clone(),
                        role: f.role,
                    },
                );
            }
            (_, Some(f)) if done => {
                files.insert(
                    f.target.clone(),
                    InstalledFile {
                        sha: f.sha.clone(),
                        role: f.role,
                    },
                );
            }
            _ => {}
        }
    }
    write_installed(
        world_root,
        &Installed {
            pack_id: id.clone(),
            name: pack.manifest.name.clone(),
            created_at: pack.manifest.created_at.clone(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            dest: prepared.dest.clone(),
            files,
            map_ids: prepared.map_ids.clone(),
        },
    )?;
    Ok(ApplyResult {
        applied: chosen.len(),
        skipped: prepared
            .items
            .iter()
            .filter(|i| {
                !matches!(i.status, Status::Unchanged) && !applied.contains(i.target.as_str())
            })
            .count(),
        journal: stamp,
    })
}

// ── Rollback ─────────────────────────────────────────────────────

pub struct RollbackResult {
    pub restored: usize,
    /// Files changed after the import; left as they are.
    pub skipped_modified: Vec<String>,
}

pub fn rollback(world_root: &Path, pack_id: &str) -> AppResult<RollbackResult> {
    let jdir = latest_journal(world_root, pack_id)?
        .ok_or_else(|| bad("Nothing to roll back for this pack"))?;
    let journal: Journal = serde_json::from_slice(
        &std::fs::read(jdir.join("journal.json")).map_err(|e| internal("read journal", e))?,
    )
    .map_err(|e| internal("journal.json", e))?;
    let mut restored = 0;
    let mut skipped_modified = Vec::new();
    for e in journal.entries.iter().rev() {
        let path = world_root.join(&e.target);
        let now = std::fs::read(&path).ok().map(|b| sha_hex(&b));
        if now != e.after_sha {
            skipped_modified.push(e.target.clone());
            continue;
        }
        if e.existed_before {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| internal("create dir", e))?;
            }
            std::fs::copy(jdir.join("files").join(&e.target), &path)
                .map_err(|er| internal(&format!("restore {}", e.target), er))?;
        } else if path.is_file() {
            crate::paths::move_to_trash(&path).map_err(|er| internal("trash", er))?;
        }
        restored += 1;
    }
    match &journal.installed_before {
        Some(inst) => write_installed(world_root, inst)?,
        None => {
            let _ = std::fs::remove_file(pack_dir(world_root, pack_id)?.join("installed.json"));
        }
    }
    let done = jdir.with_file_name(format!(
        "{}.undone",
        jdir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("journal")
    ));
    std::fs::rename(&jdir, done).map_err(|e| internal("close journal", e))?;
    Ok(RollbackResult {
        restored,
        skipped_modified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::Pin;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ck-wp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn world(root: &Path, name: &str) -> PathBuf {
        let w = root.join(name);
        std::fs::create_dir_all(w.join("Codex")).unwrap();
        let cfg = WorldConfig {
            id: name.into(),
            name: name.into(),
            ..Default::default()
        };
        world_config::write(&w, &cfg).unwrap();
        w
    }

    fn put(w: &Path, rel: &str, body: &str) {
        let p = w.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn get(w: &Path, rel: &str) -> String {
        std::fs::read_to_string(w.join(rel)).unwrap()
    }

    fn cfg_of(w: &Path) -> WorldConfig {
        world_config::read(w).unwrap().unwrap()
    }

    fn opts(name: &str) -> ExportOpts {
        ExportOpts {
            name: name.into(),
            include_atlas: true,
            ..Default::default()
        }
    }

    // ── planner ──────────────────────────────────────────────

    #[test]
    fn planner_matrix() {
        let inst = |sha: &str| InstalledFile {
            sha: sha.into(),
            role: Role::Page,
        };
        type Case = (
            Option<&'static str>,
            Option<&'static str>,
            Option<&'static str>,
            Option<Status>,
        );
        let cases: &[Case] = &[
            (Some("t"), None, None, Some(Status::Added)),
            (Some("t"), Some("b"), None, Some(Status::Restored)),
            (Some("t"), None, Some("t"), Some(Status::Unchanged)),
            (Some("t"), Some("b"), Some("t"), Some(Status::Unchanged)),
            (Some("t"), None, Some("m"), Some(Status::Conflict)),
            (Some("t"), Some("b"), Some("b"), Some(Status::Updated)),
            (Some("t"), Some("t"), Some("m"), Some(Status::Kept)),
            (Some("t"), Some("b"), Some("m"), Some(Status::Conflict)),
            (None, Some("b"), Some("b"), Some(Status::Removed)),
            (None, Some("b"), Some("m"), Some(Status::Kept)),
            (None, Some("b"), None, None),
        ];
        for (t, b, m, want) in cases {
            let theirs: BTreeMap<_, _> = t
                .iter()
                .map(|t| ("f.md".to_string(), (Role::Page, t.to_string())))
                .collect();
            let base: BTreeMap<_, _> = b.iter().map(|b| ("f.md".to_string(), inst(b))).collect();
            let items = plan(&theirs, &base, &|_| m.map(str::to_string));
            assert_eq!(items.first().map(|i| i.status), *want, "{t:?} {b:?} {m:?}");
            if let Some(i) = items.first() {
                let auto = matches!(i.status, Status::Added | Status::Updated | Status::Removed);
                assert_eq!(i.default_apply, auto, "{:?}", i.status);
            }
        }
    }

    // ── validation ───────────────────────────────────────────

    #[test]
    fn entry_paths_are_confined() {
        let ok = [
            ("Codex/NPCs/Ada.md", Role::Page),
            ("Codex/Assets/map.png", Role::Asset),
            ("_templates/npc.md", Role::Template),
            ("Atlas/vale.json", Role::AtlasMap),
            ("Atlas/vale.png", Role::AtlasArt),
            ("config/fragment.toml", Role::KindConfig),
        ];
        for (p, r) in ok {
            assert!(valid_entry(p, r), "{p}");
        }
        let refused = [
            ("Codex/../evil.md", Role::Page),
            ("/Codex/a.md", Role::Page),
            ("Codex\\a.md", Role::Page),
            ("Codex/.ck/index.db", Role::Asset),
            (".ck/config.toml", Role::KindConfig),
            ("Codex/.hidden/a.md", Role::Page),
            ("Codex/a.md", Role::Asset),
            ("Codex/run.exe", Role::Asset),
            ("Sessions/001/transcript.md", Role::Page),
            ("Codex/a.txt", Role::Page),
            ("_templates/sub/a.md", Role::Template),
            ("Atlas/../x.json", Role::AtlasMap),
            ("Atlas/a.json", Role::AtlasArt),
            ("config/settings.toml", Role::KindConfig),
            ("Codex/C:/a.md", Role::Page),
        ];
        for (p, r) in refused {
            assert!(!valid_entry(p, r), "{p}");
        }
    }

    fn zip_of(path: &Path, entries: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (n, b) in entries {
            w.start_file(*n, o).unwrap();
            w.write_all(b).unwrap();
        }
        w.finish().unwrap();
    }

    fn manifest_json(files: &[(&str, Role, &[u8])]) -> Vec<u8> {
        serde_json::to_vec(&Manifest {
            format: FORMAT,
            id: "p".into(),
            name: "P".into(),
            description: String::new(),
            created_at: "now".into(),
            files: files
                .iter()
                .map(|(p, r, b)| ManifestFile {
                    path: p.to_string(),
                    role: *r,
                    sha256: sha_hex(b),
                })
                .collect(),
        })
        .unwrap()
    }

    #[test]
    fn hostile_packs_are_refused() {
        let d = tmp("hostile");
        let body: &[u8] = b"# A\n";
        let good = manifest_json(&[("Codex/a.md", Role::Page, body)]);
        zip_of(
            &d.join("ok.zip"),
            &[("manifest.json", &good), ("files/Codex/a.md", body)],
        );
        assert!(read_pack(&d.join("ok.zip")).is_ok());

        let evil = manifest_json(&[("Codex/../evil.md", Role::Page, body)]);
        zip_of(
            &d.join("t.zip"),
            &[("manifest.json", &evil), ("files/Codex/../evil.md", body)],
        );
        assert!(read_pack(&d.join("t.zip")).is_err());

        zip_of(
            &d.join("s.zip"),
            &[("manifest.json", &good), ("files/Codex/a.md", b"tampered")],
        );
        assert!(read_pack(&d.join("s.zip")).is_err());

        zip_of(
            &d.join("x.zip"),
            &[
                ("manifest.json", &good),
                ("files/Codex/a.md", body),
                ("files/Codex/b.md", body),
            ],
        );
        assert!(read_pack(&d.join("x.zip")).is_err());

        zip_of(
            &d.join("o.zip"),
            &[
                ("manifest.json", &good),
                ("files/Codex/a.md", body),
                ("../escape.md", body),
            ],
        );
        assert!(read_pack(&d.join("o.zip")).is_err());

        zip_of(&d.join("m.zip"), &[("manifest.json", &good)]);
        assert!(read_pack(&d.join("m.zip")).is_err());

        std::fs::write(d.join("junk.zip"), b"nope").unwrap();
        assert!(read_pack(&d.join("junk.zip")).is_err());

        let mut newer: Manifest = serde_json::from_slice(&good).unwrap();
        newer.format = FORMAT + 1;
        zip_of(
            &d.join("n.zip"),
            &[
                ("manifest.json", &serde_json::to_vec(&newer).unwrap()),
                ("files/Codex/a.md", body),
            ],
        );
        assert!(read_pack(&d.join("n.zip")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    // ── export / import ──────────────────────────────────────

    fn seed_source(src: &Path) {
        put(
            src,
            "Codex/NPCs/Ada.md",
            "---\nkind: npc\n---\nAda. ![[crest.png]]\n",
        );
        put(
            src,
            "Codex/Places/Vale.md",
            "---\nkind: place\n---\nVale.\n",
        );
        put(src, "Codex/Assets/crest.png", "PNGDATA");
        put(src, "Codex/Assets/unused.png", "NOPE");
        put(
            src,
            "Codex/Prep/Session 3.md",
            "---\nkind: prep\nsession: 3\n---\nsecret plans\n",
        );
        put(
            src,
            "_templates/npc.md",
            "---\nkind: npc\n---\n# {{title}}\n",
        );
        put(src, "Sessions/001/transcript.md", "PRIVATE TRANSCRIPT");
        put(src, ".ck/index.db", "cache");
        let mut cfg = cfg_of(src);
        cfg.gm = "Secret GM Name".into();
        cfg.kinds.insert(
            "npc".into(),
            KindOverride {
                fields: vec!["race".into(), "rank".into()],
            },
        );
        cfg.calendar.eras = vec!["AE".into()];
        world_config::write(src, &cfg).unwrap();
        for (id, parent, page, to) in [
            ("realm", None, "Places/Vale.md", None),
            ("vale", Some("realm"), "Places/Vale.md#Hooks", Some("vale")),
        ] {
            let doc = MapDoc {
                id: id.into(),
                name: id.into(),
                image: format!("{id}.png"),
                parent: parent.map(str::to_string),
                page: Some(page.into()),
                scale: None,
                drawings: vec![],
                texts: vec![],
                regions: vec![],
                pinned_previews: vec![],
                pins: vec![Pin {
                    id: "p".into(),
                    name: "Vale".into(),
                    kind: "place".into(),
                    x: 0.5,
                    y: 0.5,
                    page: Some(page.into()),
                    to: to.map(str::to_string),
                    icon: None,
                    label: None,
                }],
            };
            put(
                src,
                &format!("Atlas/{id}.json"),
                &serde_json::to_string_pretty(&doc).unwrap(),
            );
            put(src, &format!("Atlas/{id}.png"), &format!("ART-{id}"));
        }
    }

    #[test]
    fn export_leaves_personal_data_behind() {
        let root = tmp("export");
        let src = world(&root, "src");
        seed_source(&src);
        let (zip, m) = export_pack(&src, &cfg_of(&src), &opts("Vale Pack")).unwrap();
        assert_eq!(m.id, "vale-pack");
        let pack = read_pack(&zip).unwrap();
        let mut paths: Vec<_> = pack.files.keys().cloned().collect();
        paths.sort();
        assert_eq!(
            paths,
            [
                "Atlas/realm.json",
                "Atlas/realm.png",
                "Atlas/vale.json",
                "Atlas/vale.png",
                "Codex/Assets/crest.png",
                "Codex/NPCs/Ada.md",
                "Codex/Places/Vale.md",
                "_templates/npc.md",
                "config/fragment.toml"
            ]
        );
        let frag = String::from_utf8_lossy(&pack.files[FRAGMENT_PATH]).to_string();
        assert!(frag.contains("rank") && frag.contains("AE"));
        assert!(!frag.contains("Secret GM Name"));

        let (zip2, _) = export_pack(
            &src,
            &cfg_of(&src),
            &ExportOpts {
                name: "Just NPCs".into(),
                folders: Some(vec!["NPCs".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        let p2 = read_pack(&zip2).unwrap();
        assert!(p2.files.contains_key("Codex/NPCs/Ada.md"));
        assert!(!p2.files.contains_key("Codex/Places/Vale.md"));
        assert!(p2.files.contains_key("Codex/Assets/crest.png"));
        assert!(!p2.files.keys().any(|k| k.starts_with("Atlas/")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn roundtrip_remaps_atlas_and_merges_kinds() {
        let root = tmp("roundtrip");
        let src = world(&root, "src");
        seed_source(&src);
        let (zip, _) = export_pack(&src, &cfg_of(&src), &opts("Vale Pack")).unwrap();
        let pack = read_pack(&zip).unwrap();

        let dst = world(&root, "dst");
        // an unrelated map already owns the id `vale`
        put(
            &dst,
            "Atlas/vale.json",
            "{\"id\":\"vale\",\"name\":\"Mine\",\"image\":\"vale.png\"}",
        );
        put(&dst, "Atlas/vale.png", "MINE");
        let prep = prepare(&pack, &dst, &cfg_of(&dst), "Imported/Vale").unwrap();
        assert!(prep.items.iter().all(|i| i.status == Status::Added));
        assert!(!dst.join("Codex/Imported").exists());

        let r = apply(&pack, &dst, "Imported/Vale", &HashMap::new()).unwrap();
        assert_eq!(r.applied, prep.items.len());
        assert_eq!(
            get(&dst, "Codex/Imported/Vale/NPCs/Ada.md"),
            get(&src, "Codex/NPCs/Ada.md")
        );
        assert_eq!(get(&dst, "Codex/Imported/Vale/Assets/crest.png"), "PNGDATA");
        assert!(dst.join("_templates/npc.md").exists());
        assert!(!dst.join("Codex/Imported/Vale/Prep").exists());
        assert!(!dst.join("Sessions").exists());

        assert!(get(&dst, "Atlas/vale.json").contains("Mine"));
        assert_eq!(get(&dst, "Atlas/vale.png"), "MINE");
        let child = atlas::read_map(&dst, "vale-2").unwrap();
        assert_eq!(child.image, "vale-2.png");
        assert_eq!(get(&dst, "Atlas/vale-2.png"), "ART-vale");
        assert_eq!(child.parent.as_deref(), Some("realm"));
        assert_eq!(
            child.page.as_deref(),
            Some("Imported/Vale/Places/Vale.md#Hooks")
        );
        assert_eq!(child.pins[0].to.as_deref(), Some("vale-2"));
        assert_eq!(
            child.pins[0].page.as_deref(),
            Some("Imported/Vale/Places/Vale.md#Hooks")
        );
        let realm = atlas::read_map(&dst, "realm").unwrap();
        assert_eq!(realm.page.as_deref(), Some("Imported/Vale/Places/Vale.md"));

        let merged = cfg_of(&dst);
        assert_eq!(merged.kinds["npc"].fields, ["race", "rank"]);
        assert_eq!(merged.calendar.eras, ["AE"]);
        assert!(merged.gm.is_empty());

        let again = prepare(&pack, &dst, &merged, "Imported/Vale").unwrap();
        assert!(
            again.items.iter().all(|i| i.status == Status::Unchanged),
            "{:?}",
            again
                .items
                .iter()
                .map(|i| (&i.target, i.status))
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn update_flow_keeps_my_edits_and_flags_conflicts() {
        let root = tmp("update");
        let src = world(&root, "src");
        seed_source(&src);
        let (z1, _) = export_pack(&src, &cfg_of(&src), &opts("Vale Pack")).unwrap();
        let dst = world(&root, "dst");
        apply(&read_pack(&z1).unwrap(), &dst, "", &HashMap::new()).unwrap();

        // I edit Ada and Vale locally; the author later edits both and adds Bram
        put(&dst, "Codex/NPCs/Ada.md", "my Ada");
        put(&dst, "Codex/Places/Vale.md", "my Vale");
        put(&src, "Codex/Places/Vale.md", "author Vale v2");
        put(
            &src,
            "Codex/NPCs/Ada.md",
            "---\nkind: npc\n---\nAda (no crest)\n",
        );
        put(&src, "Codex/NPCs/Bram.md", "---\nkind: npc\n---\nBram\n");
        std::fs::remove_file(src.join("Codex/Assets/crest.png")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let (z2, _) = export_pack(&src, &cfg_of(&src), &opts("Vale Pack")).unwrap();
        let p2 = read_pack(&z2).unwrap();

        let st = |items: &[PlanItem], t: &str| items.iter().find(|i| i.target == t).unwrap().status;
        let items = prepare(&p2, &dst, &cfg_of(&dst), "").unwrap().items;
        assert_eq!(st(&items, "Codex/NPCs/Bram.md"), Status::Added);
        assert_eq!(st(&items, "Codex/Places/Vale.md"), Status::Conflict);
        assert_eq!(st(&items, "Codex/NPCs/Ada.md"), Status::Conflict);
        assert_eq!(st(&items, "Codex/Assets/crest.png"), Status::Removed);

        let mut ov = HashMap::new();
        ov.insert("Codex/Places/Vale.md".to_string(), true);
        apply(&p2, &dst, "", &ov).unwrap();
        assert_eq!(get(&dst, "Codex/Places/Vale.md"), "author Vale v2");
        assert_eq!(get(&dst, "Codex/NPCs/Ada.md"), "my Ada");
        assert!(dst.join("Codex/NPCs/Bram.md").exists());
        assert!(!dst.join("Codex/Assets/crest.png").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rollback_restores_exact_bytes() {
        let root = tmp("rollback");
        let src = world(&root, "src");
        seed_source(&src);
        let (z, _) = export_pack(&src, &cfg_of(&src), &opts("Vale Pack")).unwrap();
        let pack = read_pack(&z).unwrap();

        let dst = world(&root, "dst");
        put(&dst, "Codex/NPCs/Ada.md", "MY ADA\n");
        let before_cfg = std::fs::read(world_config::config_path(&dst)).unwrap();
        let mut ov = HashMap::new();
        ov.insert("Codex/NPCs/Ada.md".to_string(), true);
        apply(&pack, &dst, "", &ov).unwrap();
        assert_ne!(get(&dst, "Codex/NPCs/Ada.md"), "MY ADA\n");
        assert!(dst.join("Codex/Places/Vale.md").exists());
        assert!(list_installed(&dst)[0].can_rollback);

        // an edit made after the import is never clobbered
        put(&dst, "Codex/Places/Vale.md", "edited after import");
        let r = rollback(&dst, "vale-pack").unwrap();
        assert_eq!(r.skipped_modified, ["Codex/Places/Vale.md"]);
        assert_eq!(get(&dst, "Codex/NPCs/Ada.md"), "MY ADA\n");
        assert_eq!(get(&dst, "Codex/Places/Vale.md"), "edited after import");
        assert!(!dst.join("Atlas/realm.json").exists());
        assert!(!dst.join("_templates/npc.md").exists());
        assert_eq!(
            std::fs::read(world_config::config_path(&dst)).unwrap(),
            before_cfg
        );
        assert!(rollback(&dst, "vale-pack").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
