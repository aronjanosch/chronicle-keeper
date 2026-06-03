//! Index-backed endpoints: link graph (backlinks panel + diagnostics),
//! full-text search, page tags. All read the per-world `.ck/index.db` cache.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::AppState;
use crate::store::index;

use super::vault::vault_root;

/// Change counter for the vault — moves when files change outside CK
/// (Obsidian, Finder). The frontend polls this and refreshes on change.
pub async fn seq(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    Ok(Json(json!({ "seq": state.vault_seq(&root)? })))
}

pub async fn links(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    state.with_index(&root, |conn| {
        Ok(Json(json!({
            "links": index::all_links(conn)?,
            "unresolved": index::unresolved_count(conn)?,
            "orphans": index::orphan_count(conn)?,
        })))
    })?
}

/// Grouped vault diagnostics for the Explorer panel (Phase 3).
pub async fn diagnostics(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    state.with_index(&root, |conn| {
        Ok(Json(serde_json::to_value(index::diagnostics(conn, &root)?).unwrap()))
    })?
}

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
}

pub async fn search(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Query(query): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    state.with_index(&root, |conn| {
        Ok(Json(json!({ "results": index::search(conn, &query.q)? })))
    })?
}

pub async fn tags(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    state.with_index(&root, |conn| {
        let tags: Vec<Value> = index::tag_counts(conn)?
            .into_iter()
            .map(|(tag, count)| json!({ "tag": tag, "count": count }))
            .collect();
        Ok(Json(json!({ "tags": tags })))
    })?
}
