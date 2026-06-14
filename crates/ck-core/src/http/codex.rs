//! Codex paste-import: distill pasted notes into vault pages (files-as-truth).

use axum::extract::{Path, State};
use axum::Json;

use crate::codex_import;
use crate::error::AppResult;
use crate::models::{CodexCommitRequest, CodexImportRequest};
use crate::state::AppState;

/// Distill pasted notes into proposed entries (not saved — the user reviews first).
/// Each entry is annotated with `exists`: true when a vault page of the same
/// title already exists, so the review UI can flag it (commit then writes a
/// suffixed page rather than overwriting).
pub async fn import(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<CodexImportRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let entries = codex_import::import(&state, &campaign_id, &req.text).await?;
    let root = super::vault::vault_root(&state, &campaign_id)?;
    let annotated: Vec<_> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "name": e.name, "kind": e.kind, "body": e.body, "detail": e.detail,
                "exists": crate::vault::page_exists(&root, &e.name),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "entries": annotated })))
}

/// Save the reviewed entries as vault pages (files-as-truth): one-liner →
/// `summary:` frontmatter, detail → page body. Never overwrites — a taken
/// title gets a numeric suffix. A bad entry is skipped rather than failing
/// the whole batch.
pub async fn commit(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<CodexCommitRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let root = super::vault::vault_root(&state, &campaign_id)?;
    let mut created = 0;
    let mut skipped = 0;
    for e in &req.entries {
        let name = e.name.trim();
        if name.is_empty()
            || crate::vault::write_migrated_entry(&root, name, &e.kind, &e.body, &e.detail).is_err()
        {
            skipped += 1;
        } else {
            created += 1;
        }
    }
    let _ = state.with_index(&root, |conn| {
        let _ = crate::store::index::rebuild(conn, &root);
    });
    Ok(Json(
        serde_json::json!({ "created": created, "updated": 0, "skipped": skipped }),
    ))
}
