//! Atlas maps: explorable map images with pins that reference codex pages.
//! Files-as-truth — one JSON document per map in `<world>/Atlas/`, with the
//! map art copied alongside it, portable with the world folder. Pins carry
//! normalised (0..1) coordinates over the image; a pin's `page` is a
//! Codex-relative `.md` path, its `to` a child map id.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const ATLAS_DIR: &str = "Atlas";

pub(crate) const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pin {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Glyph override; unset = the kind's seal glyph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Number/letter shown in the seal instead of a glyph ("1", "B").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Real-world size of a map: its full image width spans `width` × `unit`
/// (height follows from the image's aspect ratio).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MapScale {
    pub width: f64,
    pub unit: String,
}

/// One annotation shape in normalised (0..1) map coordinates. `points` holds
/// the freehand path (pen), two endpoints (line), two corners (rect), centre +
/// edge (circle) or a single anchor (stamp).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Drawing {
    pub id: String,
    /// pen | line | rect | circle | stamp
    pub kind: String,
    pub points: Vec<[f64; 2]>,
    pub color: String,
    /// Screen px at zoom 1 (stroke width; stamp size).
    pub width: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// A free-placed label; `x`/`y` is its centre, `rotation` in degrees.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MapText {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub size: f64,
    pub color: String,
    #[serde(default)]
    pub rotation: f64,
}

/// A drawn polygon that stands for a place (a kingdom, a forest). `page` is the
/// Codex-relative page it owns, same reference form as a pin's.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub points: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A preview card pinned open over the map: screen-space px inside the stage.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PinnedPreview {
    pub pin_id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MapDoc {
    pub id: String,
    pub name: String,
    /// Map art filename inside `Atlas/` (e.g. `aethric-reach.png`).
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// This map's own codex entry (Codex-relative `.md` path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<MapScale>,
    #[serde(default)]
    pub pins: Vec<Pin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawings: Vec<Drawing>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<MapText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<Region>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_previews: Vec<PinnedPreview>,
}

const MAX_OBJECTS: usize = 2000;
const MAX_REGIONS: usize = 500;
const MAX_REGION_POINTS: usize = 1000;
const MAX_STROKE_POINTS: usize = 5000;
const MAX_PINNED_PREVIEWS: usize = 8;
const DRAWING_KINDS: &[&str] = &["pen", "line", "rect", "circle", "stamp"];

fn bad(msg: &str) -> AppError {
    AppError::BadRequest(msg.into())
}

fn valid_color(c: &str) -> bool {
    let hex = c.strip_prefix('#').unwrap_or("");
    matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn unit(v: f64) -> bool {
    v.is_finite() && (0.0..=1.0).contains(&v)
}

fn valid_obj_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64
}

/// Reject annotations a client bug could otherwise persist forever: non-finite
/// numbers, out-of-frame points, unbounded lists.
fn validate_annotations(doc: &MapDoc) -> AppResult<()> {
    if doc.drawings.len() > MAX_OBJECTS || doc.texts.len() > MAX_OBJECTS {
        return Err(bad("Too many annotations on one map"));
    }
    for d in &doc.drawings {
        let ok_shape = match d.kind.as_str() {
            "pen" => (1..=MAX_STROKE_POINTS).contains(&d.points.len()),
            "line" | "rect" | "circle" => d.points.len() == 2,
            "stamp" => d.points.len() == 1,
            _ => false,
        };
        if !DRAWING_KINDS.contains(&d.kind.as_str())
            || !ok_shape
            || !valid_obj_id(&d.id)
            || !valid_color(&d.color)
            || !(d.width.is_finite() && d.width > 0.0 && d.width <= 200.0)
            || d.points.iter().any(|p| !unit(p[0]) || !unit(p[1]))
            || d.icon
                .as_deref()
                .is_some_and(|i| i.is_empty() || i.len() > 32)
        {
            return Err(bad("A drawing on this map is not valid"));
        }
    }
    for t in &doc.texts {
        if !valid_obj_id(&t.id)
            || t.text.chars().count() > 500
            || !unit(t.x)
            || !unit(t.y)
            || !valid_color(&t.color)
            || !(t.size.is_finite() && (4.0..=400.0).contains(&t.size))
            || !t.rotation.is_finite()
        {
            return Err(bad("A text label on this map is not valid"));
        }
    }
    if doc.regions.len() > MAX_REGIONS {
        return Err(bad("Too many regions on one map"));
    }
    for r in &doc.regions {
        if !valid_obj_id(&r.id)
            || r.name.trim().is_empty()
            || r.name.chars().count() > 200
            || !(3..=MAX_REGION_POINTS).contains(&r.points.len())
            || r.points.iter().any(|p| !unit(p[0]) || !unit(p[1]))
            || r.color.as_deref().is_some_and(|c| !valid_color(c))
            || r.page
                .as_deref()
                .is_some_and(|p| p.is_empty() || p.len() > 500)
        {
            return Err(bad("A region on this map is not valid"));
        }
    }
    if doc.pinned_previews.len() > MAX_PINNED_PREVIEWS {
        return Err(bad("Too many pinned previews on one map"));
    }
    let mut seen = std::collections::HashSet::new();
    for v in &doc.pinned_previews {
        let px = |n: f64, lo: f64| n.is_finite() && (lo..=20000.0).contains(&n);
        if !valid_obj_id(&v.pin_id)
            || !seen.insert(v.pin_id.as_str())
            || !px(v.x, -20000.0)
            || !px(v.y, -20000.0)
            || !px(v.w, 100.0)
            || !px(v.h, 100.0)
        {
            return Err(bad("A pinned preview on this map is not valid"));
        }
    }
    Ok(())
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn map_path(world_root: &Path, id: &str) -> AppResult<PathBuf> {
    if !valid_id(id) {
        return Err(AppError::BadRequest("invalid map id".into()));
    }
    Ok(world_root.join(ATLAS_DIR).join(format!("{id}.json")))
}

/// Absolute path of a map's image file, validated to stay inside `Atlas/`.
pub fn image_path(world_root: &Path, doc: &MapDoc) -> AppResult<PathBuf> {
    let name = &doc.image;
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(AppError::BadRequest("invalid map image".into()));
    }
    Ok(world_root.join(ATLAS_DIR).join(name))
}

pub fn list_maps(world_root: &Path) -> AppResult<Vec<MapDoc>> {
    let dir = world_root.join(ATLAS_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut maps = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(anyhow::Error::from)? {
        let path = entry.map_err(anyhow::Error::from)?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(doc) = serde_json::from_str::<MapDoc>(&text) {
            maps.push(doc);
        }
    }
    maps.sort_by_key(|m| m.name.to_lowercase());
    Ok(maps)
}

pub fn read_map(world_root: &Path, id: &str) -> AppResult<MapDoc> {
    let path = map_path(world_root, id)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|_| AppError::NotFound(format!("Map not found: {id}")))?;
    serde_json::from_str(&text).map_err(|e| {
        keep_corrupt_copy(&path, id, &text);
        AppError::BadRequest(format!(
            "Map file {id}.json is not valid ({e}); a copy was kept in Atlas/.corrupt/"
        ))
    })
}

/// Keep what an unparseable map file held, so a later save can't lose it.
/// Named by the file's mtime, so repeated reads of one broken file add nothing.
fn keep_corrupt_copy(path: &Path, id: &str, text: &str) {
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let dir = path.parent().unwrap().join(".corrupt");
    let copy = dir.join(format!("{id}-{mtime}.json"));
    if !copy.exists() && std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(copy, text);
    }
}

pub fn write_map(world_root: &Path, doc: &MapDoc) -> AppResult<()> {
    write_map_as(world_root, doc, "user")
}

/// Save a map, first snapshotting the file it replaces into map history.
pub fn write_map_as(world_root: &Path, doc: &MapDoc, origin: &str) -> AppResult<()> {
    let path = map_path(world_root, &doc.id)?;
    if let Some(s) = &doc.scale {
        if !(s.width.is_finite() && s.width > 0.0) || s.unit.trim().is_empty() {
            return Err(AppError::BadRequest(
                "Map scale needs a positive width and a unit".into(),
            ));
        }
    }
    validate_annotations(doc)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(anyhow::Error::from)?;
    if let Ok(before) = std::fs::read_to_string(&path) {
        snapshot(world_root, &doc.id, &before, origin);
    }
    let text = serde_json::to_string_pretty(doc).map_err(anyhow::Error::from)?;
    std::fs::write(&path, text).map_err(anyhow::Error::from)?;
    Ok(())
}

// ── Map history: `.ck/history-atlas/<id>/<millis>-<origin>.json` ──────
// The full pre-save file, like page history. Autosaves from a drag session
// coalesce: a snapshot is skipped while the latest is under COALESCE_SECS old.

const MAX_HISTORY: usize = 40;
const COALESCE_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MapVersion {
    pub ts: u64,
    pub origin: String,
}

fn history_dir(world_root: &Path, id: &str) -> PathBuf {
    world_root.join(".ck").join("history-atlas").join(id)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn versions(dir: &Path) -> Vec<(PathBuf, MapVersion)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?;
            let (ts, origin) = stem.split_once('-')?;
            let v = MapVersion {
                ts: ts.parse().ok()?,
                origin: origin.to_string(),
            };
            Some((p, v))
        })
        .collect();
    out.sort_by_key(|(_, v)| std::cmp::Reverse(v.ts));
    out
}

fn snapshot(world_root: &Path, id: &str, before: &str, origin: &str) {
    let dir = history_dir(world_root, id);
    let existing = versions(&dir);
    if let Some((path, latest)) = existing.first() {
        let fresh = now_ms().saturating_sub(latest.ts) < COALESCE_SECS * 1000;
        if (fresh && latest.origin == origin)
            || std::fs::read_to_string(path).is_ok_and(|t| t == before)
        {
            return;
        }
    }
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = std::fs::write(dir.join(format!("{}-{origin}.json", now_ms())), before);
    for (path, _) in versions(&dir).into_iter().skip(MAX_HISTORY) {
        let _ = std::fs::remove_file(path);
    }
}

/// Saved versions of a map, newest first.
pub fn list_history(world_root: &Path, id: &str) -> AppResult<Vec<MapVersion>> {
    map_path(world_root, id)?;
    Ok(versions(&history_dir(world_root, id))
        .into_iter()
        .map(|(_, v)| v)
        .collect())
}

/// Put a saved version back. The art file stays as it is now; the current
/// state is itself snapshotted first, so a restore can be undone.
pub fn restore_version(world_root: &Path, id: &str, ts: u64) -> AppResult<MapDoc> {
    let current = read_map(world_root, id)?;
    let (path, _) = versions(&history_dir(world_root, id))
        .into_iter()
        .find(|(_, v)| v.ts == ts)
        .ok_or_else(|| AppError::NotFound("No such map version".into()))?;
    let text = std::fs::read_to_string(path).map_err(anyhow::Error::from)?;
    let mut doc: MapDoc = serde_json::from_str(&text)
        .map_err(|e| AppError::BadRequest(format!("Saved version is not valid: {e}")))?;
    doc.id = current.id;
    doc.image = current.image;
    write_map_as(world_root, &doc, "user")?;
    Ok(doc)
}

/// Validate user-supplied map art and return its lowercase extension.
fn validate_art(image_src: &Path) -> AppResult<String> {
    let ext = image_src
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    if !IMAGE_EXTS.contains(&ext.as_str()) {
        return Err(AppError::BadRequest(
            "Map art must be an image (png, jpg, webp, gif)".into(),
        ));
    }
    if !image_src.is_file() {
        return Err(AppError::BadRequest(format!(
            "Image not found: {}",
            image_src.display()
        )));
    }
    Ok(ext)
}

/// Create a map from user-supplied art: the image is copied into `Atlas/`
/// under the map's id so the world folder stays self-contained.
pub fn create_map(
    world_root: &Path,
    name: &str,
    image_src: &Path,
    parent: Option<String>,
    page: Option<String>,
) -> AppResult<MapDoc> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("A map name is required".into()));
    }
    let ext = validate_art(image_src)?;
    let base = {
        let s = slugify(name);
        if s.is_empty() {
            "map".to_string()
        } else {
            s
        }
    };
    let mut id = base.clone();
    let mut n = 2;
    while map_path(world_root, &id)?.exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    let image = format!("{id}.{ext}");
    let dir = world_root.join(ATLAS_DIR);
    std::fs::create_dir_all(&dir).map_err(anyhow::Error::from)?;
    std::fs::copy(image_src, dir.join(&image))
        .map_err(|e| AppError::BadRequest(format!("Cannot copy map art: {e}")))?;
    let doc = MapDoc {
        id,
        name: name.to_string(),
        image,
        parent,
        page,
        ..Default::default()
    };
    write_map(world_root, &doc)?;
    Ok(doc)
}

/// Swap a map's art for a new image, trashing the old file. Pins keep their
/// normalised coordinates — they land where they land on the new art.
pub fn replace_image(world_root: &Path, id: &str, image_src: &Path) -> AppResult<MapDoc> {
    let mut doc = read_map(world_root, id)?;
    let ext = validate_art(image_src)?;
    let new_image = format!("{id}.{ext}");
    let old = image_path(world_root, &doc).ok().filter(|p| p.is_file());
    let dir = world_root.join(ATLAS_DIR);
    std::fs::copy(image_src, dir.join(&new_image))
        .map_err(|e| AppError::BadRequest(format!("Cannot copy map art: {e}")))?;
    if doc.image != new_image {
        if let Some(old) = old {
            let _ = crate::paths::move_to_trash(&old);
        }
        doc.image = new_image;
        write_map(world_root, &doc)?;
    }
    Ok(doc)
}

pub fn delete_map(world_root: &Path, id: &str) -> AppResult<()> {
    let doc = read_map(world_root, id)?;
    let path = map_path(world_root, id)?;
    crate::paths::move_to_trash(&path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("move map to trash: {e}")))?;
    if let Ok(img) = image_path(world_root, &doc) {
        if img.is_file() {
            let _ = crate::paths::move_to_trash(&img);
        }
    }
    // heal references: children move up to the deleted map's parent, pins
    // pointing at it lose their gateway
    for mut m in list_maps(world_root)? {
        let mut changed = false;
        if m.parent.as_deref() == Some(id) {
            m.parent = doc.parent.clone();
            changed = true;
        }
        for p in &mut m.pins {
            if p.to.as_deref() == Some(id) {
                p.to = None;
                changed = true;
            }
        }
        if changed {
            write_map(world_root, &m)?;
        }
    }
    Ok(())
}

/// Repoint pins and map pages at a moved page. A reference may carry a
/// `#Heading` suffix, which survives the move.
pub fn rewrite_page_references(world_root: &Path, from: &str, to: &str) {
    rewrite_refs(world_root, from, to, false);
}

/// Folder-move variant: re-parent every reference under the `from` prefix.
pub fn rewrite_page_references_prefix(world_root: &Path, from: &str, to: &str) {
    rewrite_refs(world_root, from, to, true);
}

fn rewrite_refs(world_root: &Path, from: &str, to: &str, prefix: bool) {
    let from = from.trim_matches('/');
    let to = to.trim_matches('/');
    if from.is_empty() || from == to {
        return;
    }
    let Ok(maps) = list_maps(world_root) else {
        return;
    };
    for mut m in maps {
        let mut changed = false;
        let refs = m
            .page
            .iter_mut()
            .chain(m.pins.iter_mut().filter_map(|p| p.page.as_mut()))
            .chain(m.regions.iter_mut().filter_map(|r| r.page.as_mut()));
        for r in refs {
            if let Some(updated) = rewritten(r, from, to, prefix) {
                *r = updated;
                changed = true;
            }
        }
        if changed {
            let _ = write_map(world_root, &m);
        }
    }
}

fn rewritten(reference: &str, from: &str, to: &str, prefix: bool) -> Option<String> {
    let (path, anchor) = match reference.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (reference, None),
    };
    let moved = if prefix {
        let rest = path.strip_prefix(from)?.strip_prefix('/')?;
        if to.is_empty() {
            rest.to_string()
        } else {
            format!("{to}/{rest}")
        }
    } else {
        (path == from).then(|| to.to_string())?
    };
    Some(match anchor {
        Some(a) => format!("{moved}#{a}"),
        None => moved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_world(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ck-atlas-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_png(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"\x89PNG\r\n\x1a\nfake").unwrap();
        p
    }

    #[test]
    fn create_copies_art_and_roundtrips() {
        let dir = temp_world("rt");
        let src = fake_png(&dir, "source-art.png");
        let doc = create_map(&dir, "Aethric Reach", &src, None, None).unwrap();
        assert_eq!(doc.id, "aethric-reach");
        assert_eq!(doc.image, "aethric-reach.png");
        assert!(dir.join(ATLAS_DIR).join("aethric-reach.png").is_file());

        let mut read = read_map(&dir, &doc.id).unwrap();
        read.pins.push(Pin {
            id: "p1".into(),
            name: "Neverwinter".into(),
            kind: "place".into(),
            x: 0.4,
            y: 0.5,
            page: Some("Places/Neverwinter.md".into()),
            to: None,
            icon: None,
            label: None,
        });
        write_map(&dir, &read).unwrap();
        assert_eq!(read_map(&dir, &doc.id).unwrap().pins.len(), 1);
        assert_eq!(list_maps(&dir).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_names_get_suffixed() {
        let dir = temp_world("dup");
        let src = fake_png(&dir, "a.png");
        create_map(&dir, "Vale", &src, None, None).unwrap();
        let second = create_map(&dir, "Vale", &src, None, None).unwrap();
        assert_eq!(second.id, "vale-2");
        assert_eq!(second.image, "vale-2.png");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_image_swaps_art_and_keeps_pins() {
        let dir = temp_world("swap");
        let src = fake_png(&dir, "a.png");
        let doc = create_map(&dir, "Vale", &src, None, None).unwrap();
        let mut with_pin = read_map(&dir, &doc.id).unwrap();
        with_pin.pins.push(Pin {
            id: "p1".into(),
            name: "X".into(),
            kind: "place".into(),
            x: 0.5,
            y: 0.5,
            page: None,
            to: None,
            icon: None,
            label: None,
        });
        write_map(&dir, &with_pin).unwrap();

        let jpg = dir.join("b.jpg");
        std::fs::write(&jpg, b"fake").unwrap();
        let updated = replace_image(&dir, &doc.id, &jpg).unwrap();
        assert_eq!(updated.image, "vale.jpg");
        assert!(dir.join(ATLAS_DIR).join("vale.jpg").is_file());
        assert!(!dir.join(ATLAS_DIR).join("vale.png").exists());
        assert_eq!(read_map(&dir, &doc.id).unwrap().pins.len(), 1);

        assert!(replace_image(&dir, &doc.id, &dir.join("missing.png")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_heals_children_and_pin_links() {
        let dir = temp_world("heal");
        let src = fake_png(&dir, "a.png");
        let root = create_map(&dir, "World", &src, None, None).unwrap();
        let mid = create_map(&dir, "Region", &src, Some(root.id.clone()), None).unwrap();
        let leaf = create_map(&dir, "Town", &src, Some(mid.id.clone()), None).unwrap();
        let mut r = read_map(&dir, &root.id).unwrap();
        r.pins.push(Pin {
            id: "p1".into(),
            name: "Region".into(),
            kind: "place".into(),
            x: 0.3,
            y: 0.3,
            page: None,
            to: Some(mid.id.clone()),
            icon: None,
            label: None,
        });
        write_map(&dir, &r).unwrap();

        delete_map(&dir, &mid.id).unwrap();
        assert!(read_map(&dir, &mid.id).is_err());
        assert_eq!(
            read_map(&dir, &leaf.id).unwrap().parent.as_deref(),
            Some(root.id.as_str())
        );
        assert!(read_map(&dir, &root.id).unwrap().pins[0].to.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_a_page_repoints_pins_and_keeps_anchor() {
        let dir = temp_world("mv");
        let src = fake_png(&dir, "a.png");
        let doc = create_map(&dir, "Vale", &src, None, Some("Places/Vale.md".into())).unwrap();
        let mut m = read_map(&dir, &doc.id).unwrap();
        let pin = |id: &str, page: &str| Pin {
            id: id.into(),
            name: id.into(),
            kind: "place".into(),
            x: 0.5,
            y: 0.5,
            page: Some(page.into()),
            to: None,
            icon: None,
            label: None,
        };
        m.pins = vec![
            pin("a", "Places/Vale.md"),
            pin("b", "Places/Vale.md#History"),
            pin("c", "Places/Other.md"),
            pin("d", "Places/Valey.md"),
        ];
        write_map(&dir, &m).unwrap();

        rewrite_page_references(&dir, "Places/Vale.md", "Realms/Vale.md");
        let m = read_map(&dir, &doc.id).unwrap();
        let pages: Vec<_> = m.pins.iter().map(|p| p.page.clone().unwrap()).collect();
        assert_eq!(
            pages,
            [
                "Realms/Vale.md",
                "Realms/Vale.md#History",
                "Places/Other.md",
                "Places/Valey.md"
            ]
        );
        assert_eq!(m.page.as_deref(), Some("Realms/Vale.md"));

        rewrite_page_references_prefix(&dir, "Realms", "World/Realms");
        let m = read_map(&dir, &doc.id).unwrap();
        assert_eq!(
            m.pins[1].page.as_deref(),
            Some("World/Realms/Vale.md#History")
        );
        assert_eq!(m.pins[2].page.as_deref(), Some("Places/Other.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_is_kept_once_and_reported() {
        let dir = temp_world("bad-json");
        let src = fake_png(&dir, "a.png");
        let doc = create_map(&dir, "Vale", &src, None, None).unwrap();
        let file = dir.join(ATLAS_DIR).join("vale.json");
        std::fs::write(&file, "{ not json").unwrap();
        assert!(read_map(&dir, &doc.id).is_err());
        assert!(read_map(&dir, &doc.id).is_err());
        let kept: Vec<_> = std::fs::read_dir(dir.join(ATLAS_DIR).join(".corrupt"))
            .unwrap()
            .collect();
        assert_eq!(kept.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_snapshots_restores_and_coalesces() {
        let dir = temp_world("hist");
        let src = fake_png(&dir, "a.png");
        let doc = create_map(&dir, "Vale", &src, None, None).unwrap();
        // create wrote the first file: nothing to snapshot yet
        assert!(list_history(&dir, &doc.id).unwrap().is_empty());

        let mut m = read_map(&dir, &doc.id).unwrap();
        m.name = "Vale 2".into();
        write_map_as(&dir, &m, "keeper").unwrap();
        let v = list_history(&dir, &doc.id).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].origin, "keeper");

        // same origin inside the coalesce window: no new version
        m.name = "Vale 3".into();
        write_map_as(&dir, &m, "keeper").unwrap();
        assert_eq!(list_history(&dir, &doc.id).unwrap().len(), 1);

        // a different origin is its own version
        m.name = "Vale 4".into();
        write_map_as(&dir, &m, "user").unwrap();
        assert_eq!(list_history(&dir, &doc.id).unwrap().len(), 2);

        // restoring the oldest brings the original name back, and is undoable
        let oldest = list_history(&dir, &doc.id).unwrap().pop().unwrap();
        let restored = restore_version(&dir, &doc.id, oldest.ts).unwrap();
        assert_eq!(restored.name, "Vale");
        assert_eq!(read_map(&dir, &doc.id).unwrap().name, "Vale");
        assert!(restore_version(&dir, &doc.id, 1).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scale_roundtrips_and_rejects_nonsense() {
        let dir = temp_world("scale");
        let src = fake_png(&dir, "a.png");
        let doc = create_map(&dir, "Vale", &src, None, None).unwrap();
        let mut m = read_map(&dir, &doc.id).unwrap();
        m.scale = Some(MapScale {
            width: 240.0,
            unit: "mi".into(),
        });
        write_map(&dir, &m).unwrap();
        let back = read_map(&dir, &doc.id).unwrap().scale.unwrap();
        assert_eq!((back.width, back.unit.as_str()), (240.0, "mi"));

        for (w, u) in [(0.0, "mi"), (-3.0, "mi"), (f64::NAN, "mi"), (10.0, "  ")] {
            m.scale = Some(MapScale {
                width: w,
                unit: u.into(),
            });
            assert!(write_map(&dir, &m).is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn drawing(kind: &str, pts: &[[f64; 2]]) -> Drawing {
        Drawing {
            id: "d1".into(),
            kind: kind.into(),
            points: pts.to_vec(),
            color: "#7a2e1f".into(),
            width: 3.0,
            icon: None,
        }
    }

    #[test]
    fn annotations_roundtrip_and_validate() {
        let dir = temp_world("annot");
        let src = fake_png(&dir, "a.png");
        let doc = create_map(&dir, "Vale", &src, None, None).unwrap();
        let raw = std::fs::read_to_string(dir.join(ATLAS_DIR).join("vale.json")).unwrap();
        assert!(!raw.contains("drawings") && !raw.contains("texts"));

        let mut m = read_map(&dir, &doc.id).unwrap();
        m.drawings = vec![
            drawing("pen", &[[0.1, 0.1], [0.2, 0.2], [0.3, 0.1]]),
            drawing("rect", &[[0.1, 0.1], [0.4, 0.5]]),
            Drawing {
                icon: Some("tower".into()),
                ..drawing("stamp", &[[0.5, 0.5]])
            },
        ];
        m.texts = vec![MapText {
            id: "t1".into(),
            x: 0.5,
            y: 0.4,
            text: "Old road".into(),
            size: 18.0,
            color: "#222".into(),
            rotation: -20.0,
        }];
        write_map(&dir, &m).unwrap();
        let back = read_map(&dir, &doc.id).unwrap();
        assert_eq!(back.drawings.len(), 3);
        assert_eq!(back.texts[0].rotation, -20.0);
        assert_eq!(back.drawings[2].icon.as_deref(), Some("tower"));

        let mut bad_docs = Vec::new();
        for d in [
            drawing("wave", &[[0.1, 0.1], [0.2, 0.2]]),
            drawing("line", &[[0.1, 0.1]]),
            drawing("pen", &[]),
            drawing("line", &[[0.1, 0.1], [1.5, 0.2]]),
            drawing("line", &[[0.1, f64::NAN], [0.2, 0.2]]),
            Drawing {
                color: "red".into(),
                ..drawing("line", &[[0.1, 0.1], [0.2, 0.2]])
            },
            Drawing {
                width: 0.0,
                ..drawing("line", &[[0.1, 0.1], [0.2, 0.2]])
            },
            drawing("pen", &vec![[0.5, 0.5]; MAX_STROKE_POINTS + 1]),
        ] {
            let mut b = back.clone();
            b.drawings = vec![d];
            bad_docs.push(b);
        }
        let mut b = back.clone();
        b.texts[0].size = f64::INFINITY;
        bad_docs.push(b);
        let mut b = back.clone();
        b.texts[0].x = -0.1;
        bad_docs.push(b);
        let mut b = back.clone();
        b.drawings = vec![drawing("line", &[[0.1, 0.1], [0.2, 0.2]]); MAX_OBJECTS + 1];
        bad_docs.push(b);
        for b in bad_docs {
            assert!(write_map(&dir, &b).is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pinned_previews_roundtrip_and_validate() {
        let dir = temp_world("previews");
        let src = fake_png(&dir, "a.png");
        let doc = create_map(&dir, "Vale", &src, None, None).unwrap();
        let card = |id: &str| PinnedPreview {
            pin_id: id.into(),
            x: 40.0,
            y: 60.0,
            w: 320.0,
            h: 340.0,
        };
        let mut m = read_map(&dir, &doc.id).unwrap();
        m.pinned_previews = vec![card("p1"), card("p2")];
        write_map(&dir, &m).unwrap();
        assert_eq!(
            read_map(&dir, &doc.id).unwrap().pinned_previews,
            m.pinned_previews
        );
        // an empty list is omitted from the file
        m.pinned_previews.clear();
        write_map(&dir, &m).unwrap();
        let text = std::fs::read_to_string(dir.join(ATLAS_DIR).join("vale.json")).unwrap();
        assert!(!text.contains("pinned_previews"));

        let mut bad_docs = Vec::new();
        for c in [
            PinnedPreview {
                x: f64::NAN,
                ..card("p")
            },
            PinnedPreview {
                w: 10.0,
                ..card("p")
            },
            PinnedPreview {
                h: 1e9,
                ..card("p")
            },
            PinnedPreview {
                pin_id: String::new(),
                ..card("p")
            },
        ] {
            let mut b = m.clone();
            b.pinned_previews = vec![c];
            bad_docs.push(b);
        }
        let mut b = m.clone();
        b.pinned_previews = vec![card("p1"), card("p1")];
        bad_docs.push(b);
        let mut b = m.clone();
        b.pinned_previews = (0..=MAX_PINNED_PREVIEWS)
            .map(|i| card(&format!("p{i}")))
            .collect();
        bad_docs.push(b);
        for b in bad_docs {
            assert!(write_map(&dir, &b).is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn region(id: &str, page: Option<&str>) -> Region {
        Region {
            id: id.into(),
            name: "Ashen Reach".into(),
            points: vec![[0.1, 0.1], [0.5, 0.1], [0.3, 0.6]],
            page: page.map(Into::into),
            color: None,
        }
    }

    #[test]
    fn regions_roundtrip_validate_and_follow_page_moves() {
        let dir = temp_world("regions");
        let src = fake_png(&dir, "a.png");
        let doc = create_map(&dir, "Vale", &src, None, None).unwrap();
        let mut m = read_map(&dir, &doc.id).unwrap();
        m.regions = vec![
            region("r1", Some("Places/Reach.md")),
            region("r2", Some("Places/Reach.md#Lore")),
            region("r3", None),
            Region {
                color: Some("#a87328".into()),
                ..region("r4", Some("Other.md"))
            },
        ];
        write_map(&dir, &m).unwrap();

        rewrite_page_references(&dir, "Places/Reach.md", "Realms/Reach.md");
        let m = read_map(&dir, &doc.id).unwrap();
        let pages: Vec<_> = m.regions.iter().map(|r| r.page.clone()).collect();
        assert_eq!(
            pages,
            [
                Some("Realms/Reach.md".into()),
                Some("Realms/Reach.md#Lore".into()),
                None,
                Some("Other.md".into())
            ]
        );
        rewrite_page_references_prefix(&dir, "Realms", "World/Realms");
        let m = read_map(&dir, &doc.id).unwrap();
        assert_eq!(
            m.regions[1].page.as_deref(),
            Some("World/Realms/Reach.md#Lore")
        );

        let bad_regions = [
            Region {
                points: vec![[0.1, 0.1], [0.5, 0.1]],
                ..region("x", None)
            },
            Region {
                points: vec![[0.1, 0.1], [0.5, 0.1], [1.2, 0.6]],
                ..region("x", None)
            },
            Region {
                name: "  ".into(),
                ..region("x", None)
            },
            Region {
                color: Some("blue".into()),
                ..region("x", None)
            },
            Region {
                points: vec![[0.5, 0.5]; MAX_REGION_POINTS + 1],
                ..region("x", None)
            },
            region("", None),
        ];
        for r in bad_regions {
            let mut b = m.clone();
            b.regions = vec![r];
            assert!(write_map(&dir, &b).is_err());
        }
        let mut b = m.clone();
        b.regions = vec![region("x", None); MAX_REGIONS + 1];
        assert!(write_map(&dir, &b).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_bad_input() {
        let dir = temp_world("bad");
        assert!(read_map(&dir, "../escape").is_err());
        assert!(read_map(&dir, "").is_err());
        let txt = dir.join("notes.txt");
        std::fs::write(&txt, "no").unwrap();
        assert!(create_map(&dir, "X", &txt, None, None).is_err());
        assert!(create_map(&dir, "X", &dir.join("missing.png"), None, None).is_err());
        let doc = MapDoc {
            id: "x".into(),
            name: "X".into(),
            image: "../../etc/passwd".into(),
            parent: None,
            ..Default::default()
        };
        assert!(image_path(&dir, &doc).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
