//! HTTP surface for the Foundry bridge (Phase 23 B): bridge settings, a
//! connection test, and the one-way codex → Journals push.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use super::vault::{vault_root, world_cfg};
use crate::error::{AppError, AppResult};
use crate::foundry::{self, load_settings_for};
use crate::state::AppState;
use crate::store::campaigns;

/// 404 on an id no world answers to, so a typo can't quietly stash settings
/// under a scope nothing will ever read.
fn known_campaign(state: &AppState, campaign_id: &str) -> AppResult<()> {
    state
        .with_db(|conn| campaigns::world_root_for_id(conn, campaign_id))?
        .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
    Ok(())
}

/// Settings for one scope: `campaign_id: None` is the app-wide default, `Some`
/// one campaign's own bridge. `own` says which of the two the values came from,
/// so the UI can show "using the app default" without a second request.
fn settings_json(state: &AppState, campaign_id: Option<&str>) -> AppResult<Value> {
    let (s, own) = state.with_db(|conn| {
        let own = match campaign_id {
            Some(id) => foundry::has_own_settings(conn, id)?,
            None => true,
        };
        Ok::<_, AppError>((foundry::read_settings(conn, campaign_id)?, own))
    })?;
    Ok(json!({
        "server_url": s.server_url,
        "user_id": s.user_id,
        "password_set": !s.password.is_empty(),
        "own": own,
    }))
}

/// GET — the app-wide default bridge settings; the password is never echoed,
/// only its presence.
pub async fn get_settings(State(state): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(settings_json(&state, None)?))
}

/// GET — the settings in effect for one campaign (its own, else the default).
pub async fn get_campaign_settings(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    known_campaign(&state, &campaign_id)?;
    Ok(Json(settings_json(&state, Some(&campaign_id))?))
}

#[derive(Debug, Default, Deserialize)]
pub struct SettingsRequest {
    pub server_url: Option<String>,
    pub user_id: Option<String>,
    /// Omit to keep the stored password; empty string clears it.
    pub password: Option<String>,
    /// Campaign scope only: drop this world's own bridge and fall back to the
    /// app-wide default. Any other field in the same request is ignored.
    pub use_default: Option<bool>,
}

/// PUT — update the app-wide default (only the fields present are written).
pub async fn put_settings(
    State(state): State<AppState>,
    Json(req): Json<SettingsRequest>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| write_scope(conn, None, &req))?;
    Ok(Json(json!({ "status": "ok" })))
}

/// PUT — give this campaign its own bridge, or (with `use_default`) take it away.
pub async fn put_campaign_settings(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<SettingsRequest>,
) -> AppResult<Json<Value>> {
    known_campaign(&state, &campaign_id)?;
    state.with_db(|conn| {
        if req.use_default == Some(true) {
            return foundry::clear_own_settings(conn, &campaign_id);
        }
        write_scope(conn, Some(campaign_id.as_str()), &req)
    })?;
    Ok(Json(json!({ "status": "ok" })))
}

fn write_scope(
    conn: &rusqlite::Connection,
    campaign_id: Option<&str>,
    req: &SettingsRequest,
) -> AppResult<()> {
    foundry::write_settings(
        conn,
        campaign_id,
        req.server_url.as_deref().map(str::trim),
        req.user_id.as_deref().map(str::trim),
        req.password.as_deref(),
    )
}

/// POST — verify the app-wide default can authenticate against the live world.
pub async fn test_connection(State(state): State<AppState>) -> AppResult<Json<Value>> {
    test_scope(&state, None).await
}

/// POST — same test, against the bridge this campaign actually uses.
pub async fn test_campaign_connection(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    known_campaign(&state, &campaign_id)?;
    test_scope(&state, Some(&campaign_id)).await
}

async fn test_scope(state: &AppState, campaign_id: Option<&str>) -> AppResult<Json<Value>> {
    let s = load_settings_for(state, campaign_id)?;
    if !s.is_complete() {
        return Err(AppError::BadRequest(
            "Foundry bridge is not fully configured (server URL, user id, password).".into(),
        ));
    }
    let client = foundry::FoundryClient::connect(&s.server_url, &s.user_id, &s.password).await?;
    client.close().await;

    // Best-effort version probe: report it and flag a known-major mismatch, but
    // never fail the test on it (the schema-drift early-warning, not a gate).
    let status = foundry::fetch_status(&s.server_url).await;
    let version = status
        .as_ref()
        .and_then(|v| v.get("version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let world = status
        .as_ref()
        .and_then(|v| v.get("world"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let compatible = version
        .as_deref()
        .map(foundry::version_compatible)
        .unwrap_or(true);
    Ok(Json(json!({
        "connected": true,
        "version": version,
        "world": world,
        "compatible": compatible,
        "supported_major": foundry::SUPPORTED_FOUNDRY_MAJOR,
    })))
}

/// POST — push every vault page to Foundry as a Journal entry.
pub async fn sync(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let s = load_settings_for(&state, Some(&campaign_id))?;
    if !s.is_complete() {
        return Err(AppError::BadRequest(
            "Foundry bridge is not fully configured (server URL, user id, password).".into(),
        ));
    }
    let (world_root, cfg) = world_cfg(&state, &campaign_id)?;
    let vault = vault_root(&state, &campaign_id)?;

    let report = foundry::sync::sync_world(&s, &world_root, &vault, &cfg.name).await?;
    Ok(Json(json!({
        "created": report.created,
        "updated": report.updated,
        "deleted": report.deleted,
        "scenes": report.scenes,
        "errors": report.errors,
    })))
}
