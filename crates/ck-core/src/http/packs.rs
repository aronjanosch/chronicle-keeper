//! World pack endpoints: export a shareable zip, plan/apply an import, roll it back.

use std::collections::HashMap;
use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use super::vault::{vault_root, world_cfg};
use crate::error::AppResult;
use crate::state::AppState;
use crate::worldpack::{self, ExportOpts};

fn reindex(state: &AppState, campaign_id: &str) {
    if let Ok(root) = vault_root(state, campaign_id) {
        let _ = state.with_index(&root, |conn| {
            let _ = crate::store::index::rebuild(conn, &root);
        });
    }
}

fn read_pack(path: &str) -> AppResult<worldpack::Pack> {
    worldpack::read_pack(&PathBuf::from(path.trim()))
}

pub async fn export(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(opts): Json<ExportOpts>,
) -> AppResult<Json<Value>> {
    let (root, cfg) = world_cfg(&state, &campaign_id)?;
    let (path, m) = worldpack::export_pack(&root, &cfg, &opts)?;
    Ok(Json(json!({
        "path": path.to_string_lossy(),
        "id": m.id,
        "name": m.name,
        "files": m.files.len(),
    })))
}

#[derive(Deserialize)]
pub struct PlanRequest {
    pub pack_path: String,
    #[serde(default)]
    pub dest: String,
}

pub async fn plan(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<PlanRequest>,
) -> AppResult<Json<Value>> {
    let (root, cfg) = world_cfg(&state, &campaign_id)?;
    let pack = read_pack(&req.pack_path)?;
    let prepared = worldpack::prepare(&pack, &root, &cfg, &req.dest)?;
    let m = &pack.manifest;
    Ok(Json(json!({
        "pack": {
            "id": m.id,
            "name": m.name,
            "description": m.description,
            "created_at": m.created_at,
            "files": m.files.len(),
        },
        "dest": prepared.dest,
        "installed_at": prepared.base.as_ref().map(|b| b.installed_at.clone()),
        "items": prepared.items,
    })))
}

#[derive(Deserialize)]
pub struct ApplyRequest {
    pub pack_path: String,
    #[serde(default)]
    pub dest: String,
    /// target → apply? Anything absent follows the plan's default.
    #[serde(default)]
    pub overrides: HashMap<String, bool>,
}

pub async fn apply(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<ApplyRequest>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    let pack = read_pack(&req.pack_path)?;
    let r = worldpack::apply(&pack, &root, &req.dest, &req.overrides)?;
    reindex(&state, &campaign_id);
    Ok(Json(json!({
        "pack_id": pack.manifest.id,
        "applied": r.applied,
        "skipped": r.skipped,
        "journal": r.journal,
    })))
}

pub async fn list(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    let packs: Vec<Value> = worldpack::list_installed(&root)
        .into_iter()
        .map(|p| {
            json!({
                "pack_id": p.pack_id,
                "name": p.name,
                "installed_at": p.installed_at,
                "dest": p.dest,
                "files": p.files,
                "can_rollback": p.can_rollback,
            })
        })
        .collect();
    Ok(Json(json!({ "packs": packs })))
}

pub async fn rollback(
    State(state): State<AppState>,
    Path((campaign_id, pack_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let (root, _) = world_cfg(&state, &campaign_id)?;
    let r = worldpack::rollback(&root, &pack_id)?;
    reindex(&state, &campaign_id);
    Ok(Json(json!({
        "restored": r.restored,
        "skipped_modified": r.skipped_modified,
    })))
}
