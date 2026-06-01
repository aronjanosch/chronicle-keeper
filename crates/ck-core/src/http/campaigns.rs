use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::models::{
    CampaignSessionInfo, CampaignUpdateRequest, CampaignsResponse, CreateCampaignRequest,
    CreateCampaignSessionRequest, NextSessionNumberResponse, RecapRequest, RecapResponse,
};
use crate::state::AppState;
use crate::store::{campaigns, sessions, tags};
use crate::summarize;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<CampaignsResponse>> {
    state.with_db(|conn| {
        Ok(Json(CampaignsResponse {
            campaigns: campaigns::campaign_infos(conn)?,
            current_campaign_id: campaigns::current_campaign_id(conn)?,
        }))
    })
}

pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateCampaignRequest>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        let campaign = campaigns::create_campaign(
            conn,
            &req.campaign_id,
            &req.name,
            req.start_session_number,
        )?;
        campaigns::set_current_campaign_id(conn, &req.campaign_id)?;
        Ok(Json(json!({ "status": "success", "campaign": campaign })))
    })
}

pub async fn detail(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        let campaign = campaigns::get_campaign(conn, &campaign_id)?
            .ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))?;
        Ok(Json(serde_json::to_value(campaign).unwrap()))
    })
}

pub async fn update(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<CampaignUpdateRequest>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        let campaign = campaigns::update_campaign(conn, &campaign_id, &req)?;
        Ok(Json(serde_json::to_value(campaign).unwrap()))
    })
}

pub async fn delete(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        campaigns::delete_campaign(conn, &campaign_id)?;
        Ok(Json(
            json!({ "status": "deleted", "campaign_id": campaign_id }),
        ))
    })
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Vec<CampaignSessionInfo>>> {
    state.with_db(|conn| Ok(Json(sessions::list_campaign_sessions(conn, &campaign_id)?)))
}

pub async fn create_session(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<CreateCampaignSessionRequest>,
) -> AppResult<Json<CampaignSessionInfo>> {
    state.with_db(|conn| {
        Ok(Json(sessions::create_campaign_session(
            conn,
            &campaign_id,
            req.session_number,
            req.title.as_deref(),
            req.date.as_deref(),
        )?))
    })
}

/// Generate (or regenerate) the campaign "story so far" recap. Synchronous —
/// one LLM call over the existing session summaries — then stored on the campaign.
pub async fn generate_recap(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<RecapRequest>,
) -> AppResult<Json<RecapResponse>> {
    Ok(Json(
        summarize::generate_recap(&state, &campaign_id, &req).await?,
    ))
}

/// Campaign tag vocabulary with usage counts (the tag-manager UI).
pub async fn list_tags(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        let tags = tags::tag_counts(conn, &campaign_id)?
            .into_iter()
            .map(|(tag, count)| json!({ "tag": tag, "count": count }))
            .collect::<Vec<_>>();
        Ok(Json(json!({ "tags": tags })))
    })
}

#[derive(Debug, Deserialize)]
pub struct RenameTagRequest {
    pub from: String,
    /// Empty target deletes the tag (merge into nothing).
    pub to: String,
}

/// Rename/merge a tag across every session in the campaign.
pub async fn rename_tag(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<RenameTagRequest>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        let changed = tags::rename(conn, &campaign_id, &req.from, &req.to)?;
        Ok(Json(json!({ "status": "ok", "sessions_changed": changed })))
    })
}

#[derive(Debug, Deserialize)]
pub struct DeleteTagRequest {
    pub tag: String,
}

/// Drop a tag from every session in the campaign.
pub async fn delete_tag(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Json(req): Json<DeleteTagRequest>,
) -> AppResult<Json<Value>> {
    state.with_db(|conn| {
        let changed = tags::delete(conn, &campaign_id, &req.tag)?;
        Ok(Json(json!({ "status": "ok", "sessions_changed": changed })))
    })
}

#[derive(Debug, Deserialize)]
pub struct NextNumberQuery {
    pub campaign_id: Option<String>,
}

pub async fn next_session_number(
    State(state): State<AppState>,
    Query(q): Query<NextNumberQuery>,
) -> AppResult<Json<NextSessionNumberResponse>> {
    state.with_db(|conn| {
        Ok(Json(NextSessionNumberResponse {
            next_session_number: campaigns::next_session_number(conn, q.campaign_id.as_deref())?,
        }))
    })
}
