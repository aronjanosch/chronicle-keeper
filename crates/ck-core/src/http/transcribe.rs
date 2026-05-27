use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::models::{TranscribeRequest, TranscribeResponse};
use crate::state::AppState;

const MODEL_ID: &str = "nemo-parakeet-tdt-0.6b-v3";

/// `/providers` — single native engine. Shape matches what the frontend's
/// transcribe modal consumes (name, display_name, models[{id,name}], default_model).
pub async fn providers() -> Json<Value> {
    Json(json!([
        {
            "name": "sherpa",
            "display_name": "Parakeet (native)",
            "description": "On-device Parakeet TDT v3 via sherpa-onnx (CPU). 25 EU languages.",
            "supports_diarization": false,
            "default_model": MODEL_ID,
            "models": [
                { "id": MODEL_ID, "name": "Parakeet TDT 0.6B v3", "description": "NVIDIA's fast & accurate, 25 EU languages (recommended)" }
            ]
        }
    ]))
}

#[cfg(feature = "transcription")]
pub async fn transcribe(
    State(state): State<AppState>,
    Json(req): Json<TranscribeRequest>,
) -> AppResult<Json<TranscribeResponse>> {
    use std::path::PathBuf;

    use crate::error::AppError;
    use crate::store::sessions;
    use crate::transcript_format::{speaker_label, write_transcription};
    use crate::transcription::{model, transcribe_tracks};

    // Gather session inputs.
    let (tracks_val, speakers_val, session_path, default_lang) = state.with_db(|conn| {
        let tracks = sessions::get_tracks(conn, &req.session_id)?;
        let speakers = sessions::get_speakers(conn, &req.session_id)?;
        let path = sessions::session_path_of(conn, &req.session_id)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {}", req.session_id)))?;
        let lang = crate::config::get_config_map(conn)?
            .get("default_language").cloned().unwrap_or_else(|| "en".into());
        Ok::<_, AppError>((tracks, speakers, path, lang))
    })?;

    let track_list = tracks_val.as_array().cloned().unwrap_or_default();
    if track_list.is_empty() {
        return Err(AppError::BadRequest("No tracks found for session.".into()));
    }
    let language = req
        .language
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(default_lang);
    let language = if language.trim().is_empty() { "en".to_string() } else { language };

    // Map track_id -> speaker entry.
    let speaker_map: std::collections::HashMap<String, Value> = speakers_val
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("track_id").and_then(Value::as_str).map(|t| (t.to_string(), s.clone())))
                .collect()
        })
        .unwrap_or_default();

    let tracks: Vec<(String, PathBuf, String)> = track_list
        .iter()
        .filter_map(|t| {
            let id = t.get("id").and_then(Value::as_str)?.to_string();
            let path = t.get("file_path").and_then(Value::as_str)?.to_string();
            let label = speaker_label(speaker_map.get(&id), &id);
            Some((id, PathBuf::from(path), label))
        })
        .collect();

    // Download model if needed, then run the CPU-heavy transcription off-thread.
    let model_dir = model::ensure(&state.paths).await.map_err(AppError::Internal)?;
    let segments = tokio::task::spawn_blocking(move || transcribe_tracks(&model_dir, &tracks))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("transcription task: {e}")))?
        .map_err(AppError::Internal)?;

    let session_path = PathBuf::from(session_path);
    let (json_path, text_path) =
        write_transcription(&session_path, "sherpa", MODEL_ID, &language, &segments)
            .map_err(AppError::Internal)?;

    state.with_db(|conn| {
        crate::store::artifacts::insert_artifact(conn, &req.session_id, "transcript", "sherpa", MODEL_ID, &text_path)
    })?;

    Ok(Json(TranscribeResponse {
        language,
        json_path: Some(json_path),
        text_path: Some(text_path),
    }))
}

#[cfg(not(feature = "transcription"))]
pub async fn transcribe(
    State(_state): State<AppState>,
    Json(_req): Json<TranscribeRequest>,
) -> AppResult<Json<TranscribeResponse>> {
    Err(crate::error::AppError::BadRequest(
        "Transcription is not available in this build.".into(),
    ))
}
