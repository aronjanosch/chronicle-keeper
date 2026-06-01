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
    use crate::store::{campaigns, sessions};
    use crate::transcript_format::{segments_to_plain_text, speaker_label};
    use crate::transcription::{model, transcribe_tracks};

    // Gather session inputs. Language comes from the session's campaign — it's
    // fixed per campaign (a multilingual GM uses one campaign per language), so
    // there's no per-transcription language choice.
    let (tracks_val, speakers_val, session_path, default_lang, accelerator) =
        state.with_db(|conn| {
            let tracks = sessions::get_tracks(conn, &req.session_id)?;
            let speakers = sessions::get_speakers(conn, &req.session_id)?;
            let path = sessions::session_path_of(conn, &req.session_id)?.ok_or_else(|| {
                AppError::NotFound(format!("Session not found: {}", req.session_id))
            })?;
            let lang = sessions::get_session_object(conn, &req.session_id)
                .ok()
                .as_ref()
                .and_then(|s| s.get("campaign"))
                .and_then(|c| c.get("campaign_id"))
                .and_then(Value::as_str)
                .and_then(|cid| campaigns::get_campaign(conn, cid).ok().flatten())
                .map(|c| c.default_language)
                .unwrap_or_else(|| "en".into());
            let accel = crate::config::get_config_map(conn)?
                .get("transcription_accelerator")
                .cloned()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "auto".into());
            Ok::<_, AppError>((tracks, speakers, path, lang, accel))
        })?;

    let track_list = tracks_val.as_array().cloned().unwrap_or_default();
    if track_list.is_empty() {
        return Err(AppError::BadRequest("No tracks found for session.".into()));
    }
    let language = req
        .language
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(default_lang);
    let language = if language.trim().is_empty() {
        "en".to_string()
    } else {
        language
    };

    // Map track_id -> speaker entry.
    let speaker_map: std::collections::HashMap<String, Value> = speakers_val
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    s.get("track_id")
                        .and_then(Value::as_str)
                        .map(|t| (t.to_string(), s.clone()))
                })
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
    let model_dir = match model::ensure(&state.paths, &state.model_progress).await {
        Ok(dir) => dir,
        Err(e) => {
            crate::state::ModelProgress::set_error(&state.model_progress, e.to_string());
            return Err(AppError::Internal(e));
        }
    };
    // Best-effort VAD model fetch (None → fixed-window fallback inside the engine).
    let vad_model = model::ensure_vad(&state.paths).await;

    // Cap how long a single transcription may run. A corrupt or huge file would
    // otherwise pin a worker forever and freeze the frontend op-banner. The flag
    // cooperatively stops the blocking thread at its next chunk boundary (a
    // blocking thread can't be force-aborted), so the leaked core is freed too.
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    let timeout_secs: u64 = state
        .with_db(|conn| crate::config::get_config_map(conn))
        .ok()
        .and_then(|cfg| {
            cfg.get("transcription_timeout_seconds")
                .and_then(|s| s.parse().ok())
        })
        .filter(|&s: &u64| s > 0)
        .unwrap_or(3600);

    // Resolve the accelerator preference (default "auto") to a concrete provider
    // for this OS; the engine still falls back to CPU if it isn't linked in.
    let accelerator = crate::config::resolve_accelerator(&accelerator);

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = cancel.clone();
    let progress = state.model_progress.clone();
    let handle = tokio::task::spawn_blocking(move || {
        transcribe_tracks(
            &model_dir,
            accelerator,
            vad_model.as_deref(),
            &tracks,
            &cancel_worker,
            &progress,
        )
    });

    let segments = match tokio::time::timeout(Duration::from_secs(timeout_secs), handle).await {
        Ok(joined) => joined
            .map_err(|e| AppError::Internal(anyhow::anyhow!("transcription task: {e}")))?
            .map_err(AppError::Internal)?,
        Err(_) => {
            cancel.store(true, Ordering::Relaxed); // signal the blocking thread to bail
            return Err(AppError::Internal(anyhow::anyhow!(
                "Transcription timed out after {timeout_secs}s. The audio may be corrupt or too long; try splitting it or raise transcription_timeout_seconds."
            )));
        }
    };

    let _ = session_path; // session folder no longer holds transcript files
    let transcript_text = segments_to_plain_text(&segments);

    state.with_db(|conn| {
        crate::store::artifacts::insert_artifact(
            conn,
            &req.session_id,
            "transcript",
            "sherpa",
            MODEL_ID,
            &transcript_text,
        )
    })?;

    Ok(Json(TranscribeResponse {
        language,
        json_path: None,
        text_path: None,
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
