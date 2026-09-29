//! Genre pack endpoints: list the packs, apply one to a world.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::genre_packs;
use crate::state::AppState;

use super::vault::world_cfg;

pub async fn list(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "packs": genre_packs::all_packs(&state.paths.data_dir) }))
}

#[derive(Deserialize)]
pub struct ApplyRequest {
    pub id: String,
    /// "preview" reports what would change without writing; default applies.
    #[serde(default)]
    pub mode: Option<String>,
}

pub async fn apply(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<ApplyRequest>,
) -> AppResult<Json<Value>> {
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    let pack = genre_packs::find_pack(&state.paths.data_dir, &req.id)
        .ok_or_else(|| AppError::NotFound(format!("Genre pack not found: {}", req.id)))?;
    let dry_run = req.mode.as_deref() == Some("preview");
    let report = genre_packs::apply(&world_root, &pack, dry_run)?;
    Ok(Json(
        serde_json::to_value(report).map_err(anyhow::Error::from)?,
    ))
}
