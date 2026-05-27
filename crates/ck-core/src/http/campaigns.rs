use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::models::{
    CampaignSessionInfo, CampaignUpdateRequest, CampaignsResponse, CreateCampaignRequest,
    CreateCampaignSessionRequest, NextSessionNumberResponse,
};
use crate::state::AppState;
use crate::store::{campaigns, sessions};

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
        let campaign = campaigns::create_campaign(conn, &req.campaign_id, &req.name, req.start_session_number)?;
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
