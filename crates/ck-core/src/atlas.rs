//! Atlas maps: explorable map images with pins that reference codex pages.
//! Files-as-truth — one JSON document per map in `<world>/Atlas/`, with the
//! map art copied alongside it, portable with the world folder. Pins carry
//! normalised (0..1) coordinates over the image; a pin's `page` is a
//! Codex-relative `.md` path, its `to` a child map id.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const ATLAS_DIR: &str = "Atlas";

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];

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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    #[serde(default)]
    pub pins: Vec<Pin>,
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

fn valid_id(id: &str) -> bool {
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
    maps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(maps)
}

pub fn read_map(world_root: &Path, id: &str) -> AppResult<MapDoc> {
    let path = map_path(world_root, id)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|_| AppError::NotFound(format!("Map not found: {id}")))?;
    serde_json::from_str(&text)
        .map_err(|e| AppError::BadRequest(format!("Map file {id}.json is not valid: {e}")))
}

pub fn write_map(world_root: &Path, doc: &MapDoc) -> AppResult<()> {
    let path = map_path(world_root, &doc.id)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(anyhow::Error::from)?;
    let text = serde_json::to_string_pretty(doc).map_err(anyhow::Error::from)?;
    std::fs::write(&path, text).map_err(anyhow::Error::from)?;
    Ok(())
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
        pins: Vec::new(),
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
            page: None,
            pins: vec![],
        };
        assert!(image_path(&dir, &doc).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
