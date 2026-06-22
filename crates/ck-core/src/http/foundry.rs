//! HTTP surface for the Foundry bridge (Phase 23 B): bridge settings, a
//! connection test, and the one-way codex → Journals push.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use super::vault::{vault_root, world_cfg};
use crate::config;
use crate::error::{AppError, AppResult};
use crate::foundry::{self, FoundrySettings};
use crate::state::AppState;

fn load_settings(state: &AppState) -> AppResult<FoundrySettings> {
    state.with_db(|conn| {
        Ok(FoundrySettings {
            server_url: config::get_value(conn, "foundry_server_url")?.unwrap_or_default(),
            user_id: config::get_value(conn, "foundry_user_id")?.unwrap_or_default(),
            password: config::get_value(conn, "foundry_password")?.unwrap_or_default(),
        })
    })
}

/// GET — current bridge settings; the password is never echoed, only its presence.
pub async fn get_settings(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let s = load_settings(&state)?;
    Ok(Json(json!({
        "server_url": s.server_url,
        "user_id": s.user_id,
        "password_set": !s.password.is_empty(),
    })))
}

#[derive(Debug, Default, Deserialize)]
pub struct SettingsRequest {
    pub server_url: Option<String>,
    pub user_id: Option<String>,
    /// Omit to keep the stored password; empty string clears it.
    pub password: Option<String>,
}

/// PUT — update bridge settings (only the fields present are written).
pub async fn put_settings(
    State(state): State<AppState>,
    Json(req): Json<SettingsRequest>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        if let Some(v) = &req.server_url {
            config::set_value(conn, "foundry_server_url", v.trim())?;
        }
        if let Some(v) = &req.user_id {
            config::set_value(conn, "foundry_user_id", v.trim())?;
        }
        if let Some(v) = &req.password {
            config::set_value(conn, "foundry_password", v)?;
        }
        Ok::<_, AppError>(())
    })?;
    Ok(Json(json!({ "status": "ok" })))
}

/// POST — verify the bridge can authenticate against the live world.
pub async fn test_connection(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let s = load_settings(&state)?;
    if !s.is_complete() {
        return Err(AppError::BadRequest(
            "Foundry bridge is not fully configured (server URL, user id, password).".into(),
        ));
    }
    let client = foundry::FoundryClient::connect(&s.server_url, &s.user_id, &s.password).await?;
    client.close().await;
    Ok(Json(json!({ "connected": true })))
}

/// POST — push every vault page to Foundry as a Journal entry.
pub async fn sync(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let s = load_settings(&state)?;
    if !s.is_complete() {
        return Err(AppError::BadRequest(
            "Foundry bridge is not fully configured (server URL, user id, password).".into(),
        ));
    }
    let (world_root, _) = world_cfg(&state, &campaign_id)?;
    let vault = vault_root(&state, &campaign_id)?;

    let report = foundry::sync::sync_world(&s, &world_root, &vault).await?;
    Ok(Json(json!({
        "created": report.created,
        "updated": report.updated,
        "deleted": report.deleted,
        "errors": report.errors,
    })))
}
