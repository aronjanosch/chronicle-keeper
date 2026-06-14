//! Atlas map endpoints (files-as-truth: `<world>/Atlas/<id>.json` + map art).

use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::atlas;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::campaigns;

fn world_root(state: &AppState, campaign_id: &str) -> AppResult<PathBuf> {
    state
        .with_db(|conn| campaigns::world_root_for_id(conn, campaign_id))?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))
}

pub async fn list_maps(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = world_root(&state, &campaign_id)?;
    Ok(Json(json!({ "maps": atlas::list_maps(&root)? })))
}

#[derive(Deserialize)]
pub struct CreateMapRequest {
    pub name: String,
    /// Absolute path of the map art on this machine; copied into `Atlas/`.
    pub image_path: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub page: Option<String>,
}

pub async fn create_map(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<CreateMapRequest>,
) -> AppResult<Json<atlas::MapDoc>> {
    let root = world_root(&state, &campaign_id)?;
    Ok(Json(atlas::create_map(
        &root,
        &req.name,
        std::path::Path::new(req.image_path.trim()),
        req.parent,
        req.page,
    )?))
}

pub async fn read_map(
    State(state): State<AppState>,
    Path((campaign_id, map_id)): Path<(String, String)>,
) -> AppResult<Json<atlas::MapDoc>> {
    let root = world_root(&state, &campaign_id)?;
    Ok(Json(atlas::read_map(&root, &map_id)?))
}

pub async fn write_map(
    State(state): State<AppState>,
    Path((campaign_id, map_id)): Path<(String, String)>,
    Json(mut doc): Json<atlas::MapDoc>,
) -> AppResult<Json<atlas::MapDoc>> {
    let root = world_root(&state, &campaign_id)?;
    doc.id = map_id; // the path segment is authoritative
                     // The image is managed by create; a PUT must not repoint it elsewhere.
    doc.image = atlas::read_map(&root, &doc.id)?.image;
    atlas::write_map(&root, &doc)?;
    Ok(Json(doc))
}

#[derive(Deserialize)]
pub struct ReplaceImageRequest {
    /// Absolute path of the new map art on this machine; copied into `Atlas/`.
    pub image_path: String,
}

pub async fn replace_image(
    State(state): State<AppState>,
    Path((campaign_id, map_id)): Path<(String, String)>,
    Json(req): Json<ReplaceImageRequest>,
) -> AppResult<Json<atlas::MapDoc>> {
    let root = world_root(&state, &campaign_id)?;
    Ok(Json(atlas::replace_image(
        &root,
        &map_id,
        std::path::Path::new(req.image_path.trim()),
    )?))
}

pub async fn delete_map(
    State(state): State<AppState>,
    Path((campaign_id, map_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let root = world_root(&state, &campaign_id)?;
    atlas::delete_map(&root, &map_id)?;
    Ok(Json(json!({ "ok": true })))
}

/// The map art bytes. The frontend fetches this with the auth header and
/// renders via an object URL (an <img src> can't carry `X-CK-Token`).
pub async fn map_image(
    State(state): State<AppState>,
    Path((campaign_id, map_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let root = world_root(&state, &campaign_id)?;
    let doc = atlas::read_map(&root, &map_id)?;
    let path = atlas::image_path(&root, &doc)?;
    let bytes = std::fs::read(&path)
        .map_err(|_| AppError::NotFound(format!("Map art missing: {}", doc.image)))?;
    let mime = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    };
    Ok(([(header::CONTENT_TYPE, mime)], bytes))
}
