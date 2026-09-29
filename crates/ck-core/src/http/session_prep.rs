use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::Json;

use crate::error::{AppError, AppResult};
use crate::session_prep::{self, OpsRequest, PrepCtx, PrepResponse};
use crate::state::AppState;
use crate::store::sessions;

pub async fn get(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> AppResult<Json<PrepResponse>> {
    let ctx = prep_ctx(&state, &session_id)?;
    let response = session_prep::read(&ctx)?;
    reindex(&state, &ctx, &response);
    Ok(Json(response))
}

pub async fn ops(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    payload: Result<Json<OpsRequest>, JsonRejection>,
) -> AppResult<Json<PrepResponse>> {
    let Json(request) = payload.map_err(|error| AppError::Unprocessable(error.body_text()))?;
    let ctx = prep_ctx(&state, &session_id)?;
    let lock = state.world_write_lock(&ctx.world_root).await;
    let _guard = lock.lock().await;
    let response = session_prep::apply(&ctx, &request)?;
    reindex(&state, &ctx, &response);
    Ok(Json(response))
}

pub(crate) fn prep_ctx(state: &AppState, session_id: &str) -> AppResult<PrepCtx> {
    let sid = session_id.to_string();
    state.with_db(move |conn| {
        let loc = sessions::locate(conn, &sid)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {sid}")))?;
        PrepCtx::of(conn, &loc)
    })
}

/// The prep page is an ordinary Codex page: keep the index current and tell
/// the watcher the write was ours. Reads can write too (legacy migration).
fn reindex(state: &AppState, ctx: &PrepCtx, response: &PrepResponse) {
    let Some(rel) = &response.page else { return };
    state.note_vault_write(&ctx.vault, rel);
    let _ = state.with_index(&ctx.vault, |conn| {
        let _ = crate::store::index::upsert_path(conn, &ctx.vault, rel);
    });
}
