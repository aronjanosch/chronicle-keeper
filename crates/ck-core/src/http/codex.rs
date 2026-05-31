//! Codex entry endpoints (Phase 2). Pattern mirrors `campaigns.rs`.

use axum::extract::{Path, State};
use axum::Json;

use crate::codex_import;
use crate::error::AppResult;
use crate::models::{
    CodexCommitRequest, CodexEntry, CodexEntryCreate, CodexEntryUpdate, CodexImportRequest,
};
use crate::state::AppState;
use crate::store::codex;

pub async fn list(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Vec<CodexEntry>>> {
    state.with_db(|conn| Ok(Json(codex::list_entries(conn, &campaign_id)?)))
}

pub async fn create(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<CodexEntryCreate>,
) -> AppResult<Json<CodexEntry>> {
    state.with_db(|conn| Ok(Json(codex::create_entry(conn, &campaign_id, &req)?)))
}

pub async fn update(
    State(state): State<AppState>,
    Path((_campaign_id, entry_id)): Path<(String, String)>,
    Json(req): Json<CodexEntryUpdate>,
) -> AppResult<Json<CodexEntry>> {
    state.with_db(|conn| Ok(Json(codex::update_entry(conn, &entry_id, &req)?)))
}

pub async fn delete(
    State(state): State<AppState>,
    Path((_campaign_id, entry_id)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    state.with_db(|conn| {
        codex::delete_entry(conn, &entry_id)?;
        Ok(Json(serde_json::json!({ "status": "ok" })))
    })
}

/// Distill pasted notes into proposed entries (not saved — the user reviews first).
/// Each entry is annotated with `exists`: true when an entry of the same name+kind
/// already lives in the codex (e.g. a faction first picked up from an NPC note),
/// so the review UI can flag it and let the user choose to replace it.
pub async fn import(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<CodexImportRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let entries = codex_import::import(&state, &campaign_id, &req.text).await?;
    let annotated = state.with_db(|conn| {
        entries
            .iter()
            .map(|e| {
                let exists = codex::exists(conn, &campaign_id, &e.name, &e.kind)?;
                Ok(serde_json::json!({
                    "name": e.name, "kind": e.kind, "body": e.body, "detail": e.detail, "exists": exists,
                }))
            })
            .collect::<AppResult<Vec<_>>>()
    })?;
    Ok(Json(serde_json::json!({ "entries": annotated })))
}

/// Save the reviewed entries. Upserts by natural key: a new name+kind is created,
/// an existing one has its body replaced (the user explicitly checked it to win).
/// A bad entry is skipped rather than failing the whole batch.
pub async fn commit(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<CodexCommitRequest>,
) -> AppResult<Json<serde_json::Value>> {
    state.with_db(|conn| {
        let mut created = 0;
        let mut updated = 0;
        let mut skipped = 0;
        for e in &req.entries {
            match codex::upsert_manual(conn, &campaign_id, e) {
                Ok(true) => created += 1,
                Ok(false) => updated += 1,
                Err(_) => skipped += 1,
            }
        }
        Ok(Json(
            serde_json::json!({ "created": created, "updated": updated, "skipped": skipped }),
        ))
    })
}
