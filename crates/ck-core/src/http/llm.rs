use std::convert::Infallible;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::Stream;
use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::llm::{self, SavedKey};
use crate::models::{
    ExportRequest, ExportResponse, ProviderInfo, ProviderKeyUpdate, ProviderTestRequest,
    ProviderTestResult, SummarizeRequest, SummarizeResponse,
};
use crate::state::AppState;
use crate::summarize::SummaryProgress;
use crate::{export, summarize};

fn provider_info(p: &llm::Provider, saved: Option<&SavedKey>) -> ProviderInfo {
    ProviderInfo {
        id: p.id.to_string(),
        name: p.name.to_string(),
        models: p.models.iter().map(|m| m.to_string()).collect(),
        default_model: p.default_model.to_string(),
        needs_key: p.needs_key,
        default_api_base: p.default_api_base.map(str::to_string),
        has_key: saved.map(|s| !s.api_key.is_empty()).unwrap_or(false),
        has_custom_base: saved.map(|s| !s.api_base.is_empty()).unwrap_or(false),
        saved_model: saved
            .map(|s| s.default_model.clone())
            .filter(|m| !m.is_empty()),
    }
}

pub async fn list_providers(State(state): State<AppState>) -> AppResult<Json<Vec<ProviderInfo>>> {
    state.with_db(|conn| {
        let saved = llm::list_keys(conn)?;
        Ok(Json(
            llm::REGISTRY
                .iter()
                .map(|p| provider_info(p, saved.get(p.id)))
                .collect(),
        ))
    })
}

pub async fn put_provider(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
    Json(req): Json<ProviderKeyUpdate>,
) -> AppResult<Json<ProviderInfo>> {
    let p = llm::get(&provider_id)
        .ok_or_else(|| AppError::NotFound(format!("Unknown provider: {provider_id}")))?;
    state.with_db(|conn| {
        llm::upsert_key(
            conn,
            &provider_id,
            req.api_key.as_deref().unwrap_or(""),
            req.api_base.as_deref().unwrap_or(""),
            req.default_model.as_deref().unwrap_or(""),
        )?;
        let saved = llm::get_key(conn, &provider_id)?;
        Ok(Json(provider_info(p, saved.as_ref())))
    })
}

pub async fn test_provider(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
    Json(req): Json<ProviderTestRequest>,
) -> AppResult<Json<ProviderTestResult>> {
    let p = llm::get(&provider_id)
        .ok_or_else(|| AppError::NotFound(format!("Unknown provider: {provider_id}")))?;
    let saved = state
        .with_db(|conn| llm::get_key(conn, &provider_id))?
        .unwrap_or_default();

    let api_base = if !saved.api_base.is_empty() {
        saved.api_base.clone()
    } else {
        p.default_api_base.unwrap_or("").to_string()
    };
    let model = req
        .model
        .filter(|s| !s.is_empty())
        .or_else(|| Some(saved.default_model.clone()).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| p.default_model.to_string());

    let start = Instant::now();
    let result = llm::chat(
        p.transport,
        &api_base,
        &saved.api_key,
        &model,
        "Hi",
        15,
        false,
        None,
    )
    .await;
    let latency_ms = start.elapsed().as_millis() as i64;
    Ok(Json(match result {
        Ok(_) => ProviderTestResult {
            ok: true,
            latency_ms,
            error: None,
        },
        Err(e) => ProviderTestResult {
            ok: false,
            latency_ms,
            error: Some(e.0),
        },
    }))
}

/// Cheap reachability check used by the sidebar status badge. Unlike
/// `test_provider` it does not generate (no model load) — for Ollama it just
/// lists tags; for keyed providers it returns ok and relies on the key check.
pub async fn ping_provider(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> AppResult<Json<ProviderTestResult>> {
    let p = llm::get(&provider_id)
        .ok_or_else(|| AppError::NotFound(format!("Unknown provider: {provider_id}")))?;
    let saved = state
        .with_db(|conn| llm::get_key(conn, &provider_id))?
        .unwrap_or_default();
    let api_base = if !saved.api_base.is_empty() {
        saved.api_base.clone()
    } else {
        p.default_api_base.unwrap_or("").to_string()
    };
    let result = llm::ping(p.transport, &api_base, &saved.api_key, 4).await;
    Ok(Json(match result {
        Ok(()) => ProviderTestResult {
            ok: true,
            latency_ms: 0,
            error: None,
        },
        Err(e) => ProviderTestResult {
            ok: false,
            latency_ms: 0,
            error: Some(e.0),
        },
    }))
}

pub async fn summarize(
    State(state): State<AppState>,
    Json(req): Json<SummarizeRequest>,
) -> AppResult<Json<SummarizeResponse>> {
    Ok(Json(summarize::summarize_session(&state, &req).await?))
}

/// Streaming summarize over Server-Sent Events. Typed `data:` frames:
///   {stage:"reading"}                          prefill / waiting on first token
///   {stage:"writing", token:"…"}               one prose chunk
///   {stage:"metadata"}                          summary done, tag extraction running
///   {stage:"done", summary, metadata, …}        authoritative + already persisted
///   {stage:"error", message}                    failure mid-run
/// The done payload matches the blocking `/summarize` response; the frontend
/// swaps its live-built text for it (and gains the parsed metadata/tags).
pub async fn summarize_stream(
    State(state): State<AppState>,
    Json(req): Json<SummarizeRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    tokio::spawn(async move {
        let send = |val: serde_json::Value| {
            let ev = Event::default()
                .json_data(&val)
                .unwrap_or_else(|_| Event::default());
            let _ = tx.send(ev);
        };
        let result = summarize::summarize_session_streamed(&state, &req, |p| match p {
            SummaryProgress::Reading => send(json!({ "stage": "reading" })),
            SummaryProgress::Token(t) => send(json!({ "stage": "writing", "token": t })),
            SummaryProgress::Metadata => send(json!({ "stage": "metadata" })),
        })
        .await;
        match result {
            Ok(r) => send(json!({
                "stage": "done",
                "summary": r.summary,
                "metadata": r.metadata,
                "provider": r.provider,
                "model": r.model,
            })),
            Err(e) => send(json!({ "stage": "error", "message": e.to_string() })),
        }
    });

    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|ev| (Ok(ev), rx))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub async fn export_notes(
    State(state): State<AppState>,
    Json(req): Json<ExportRequest>,
) -> AppResult<Json<ExportResponse>> {
    Ok(Json(export::export_session(&state, &req)?))
}
