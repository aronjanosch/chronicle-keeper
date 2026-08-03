use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::models::{TranscribeRequest, TranscribeResponse};
use crate::state::AppState;

/// `/providers` — the native engine plus any cloud endpoints whose LLM key is
/// already saved. Shape matches what the frontend's transcribe modal consumes
/// (name, display_name, models[{id,name}], default_model).
pub async fn providers(State(state): State<AppState>) -> Json<Value> {
    use crate::asr_models;

    let models: Vec<Value> = asr_models::MODELS
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "description": m.description,
                "precision": m.precision,
                "languages": m.languages,
                "download_mb": m.download_mb,
                "downloaded": downloaded(&state, m),
            })
        })
        .collect();

    let mut out = vec![json!({
        "name": crate::config::NATIVE_TRANSCRIPTION_PROVIDER,
        "display_name": "On-device (sherpa-onnx)",
        "description": "Runs entirely on this machine. Parakeet, Canary or Whisper.",
        "supports_diarization": false,
        "default_model": asr_models::DEFAULT_MODEL_ID,
        "models": models,
    })];

    let keyed = state
        .with_db(|conn| {
            Ok::<_, crate::error::AppError>(
                asr_models::CLOUD_PROVIDERS
                    .iter()
                    .filter(|p| {
                        crate::llm::get_key(conn, p.id)
                            .ok()
                            .flatten()
                            .is_some_and(|k| !k.api_key.trim().is_empty())
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    for p in keyed {
        out.push(json!({
            "name": p.id,
            "display_name": format!("{} (cloud)", p.name),
            "description": "Uploads audio to this provider. Uses the API key saved under LLM providers.",
            "supports_diarization": false,
            "cloud": true,
            "default_model": p.default_model,
            "models": p.models.iter().map(|m| json!({ "id": m, "name": m })).collect::<Vec<_>>(),
        }));
    }
    Json(Value::Array(out))
}

#[cfg(feature = "transcription")]
fn downloaded(state: &AppState, model: &crate::asr_models::AsrModel) -> bool {
    let dir = crate::transcription::model::model_dir(&state.paths, model);
    crate::transcription::model::is_present(&dir, model)
}

#[cfg(not(feature = "transcription"))]
fn downloaded(_state: &AppState, _model: &crate::asr_models::AsrModel) -> bool {
    false
}

#[derive(serde::Deserialize)]
pub struct ImportTranscriptRequest {
    pub session_id: String,
    pub content: String,
    /// Only used to break format ties (`.srt` vs `.vtt`); detection is by content.
    pub filename: Option<String>,
}

/// `POST /transcript-import` — take a transcript produced by another tool and
/// store it as this session's transcript, so the summarize → codex pipeline runs
/// on it unchanged. No ASR involved, hence no `transcription` feature needed.
pub async fn import_transcript(
    State(state): State<AppState>,
    Json(req): Json<ImportTranscriptRequest>,
) -> AppResult<Json<Value>> {
    use crate::transcript_format::segments_to_plain_text;

    let segments = crate::transcript_import::parse(&req.content, req.filename.as_deref())?;
    let text = segments_to_plain_text(&segments);
    let speakers = segments
        .iter()
        .filter_map(|s| s.speaker.as_deref())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    state.with_db(|conn| {
        crate::store::artifacts::insert_artifact(
            conn,
            &req.session_id,
            "transcript",
            "import",
            req.filename.as_deref().unwrap_or("external"),
            &text,
        )
    })?;
    Ok(Json(json!({
        "segments": segments.len(),
        "speakers": speakers,
        "characters": text.chars().count(),
    })))
}

/// Stall watchdog: the worker ticks `watch` per decoded packet / VAD window;
/// cancel only when the tick counter hasn't moved for `timeout_secs`. After
/// cancelling, grace-wait for the cooperative stop so the tracks that did finish
/// come back — a thread wedged inside onnx can't be aborted, so give up on it
/// after a minute.
#[cfg(feature = "transcription")]
async fn watch_native(
    handle: &mut tokio::task::JoinHandle<anyhow::Result<crate::transcription::Transcribed>>,
    watch: &std::sync::Arc<crate::transcription::Watch>,
    timeout_secs: u64,
) -> AppResult<crate::transcription::Transcribed> {
    use std::time::{Duration, Instant};

    use crate::error::AppError;

    let mut last = (watch.ticks(), Instant::now());
    let joined = loop {
        match tokio::time::timeout(Duration::from_secs(5), &mut *handle).await {
            Ok(joined) => break Some(joined),
            Err(_) => {
                let ticks = watch.ticks();
                if ticks != last.0 {
                    last = (ticks, Instant::now());
                } else if last.1.elapsed().as_secs() >= timeout_secs {
                    watch.cancel();
                    break tokio::time::timeout(Duration::from_secs(60), &mut *handle)
                        .await
                        .ok();
                }
            }
        }
    };
    match joined {
        Some(joined) => joined
            .map_err(|e| AppError::Internal(anyhow::anyhow!("transcription task: {e}")))?
            .map_err(AppError::Internal),
        None => Err(AppError::Internal(anyhow::anyhow!(
            "Transcription stalled (no progress for {timeout_secs}s) and the worker did not stop \
             — likely stuck on a corrupt audio file. Nothing was saved."
        ))),
    }
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
    let (tracks_val, speakers_val, _session_path, default_lang, cfg) = state.with_db(|conn| {
        let tracks = sessions::get_tracks(conn, &req.session_id)?;
        let speakers = sessions::get_speakers(conn, &req.session_id)?;
        let path = sessions::session_path_of(conn, &req.session_id)?
            .ok_or_else(|| AppError::NotFound(format!("Session not found: {}", req.session_id)))?;
        let lang = sessions::get_session_object(conn, &req.session_id)
            .ok()
            .as_ref()
            .and_then(|s| s.get("campaign"))
            .and_then(|c| c.get("campaign_id"))
            .and_then(Value::as_str)
            .and_then(|cid| campaigns::get_campaign(conn, cid).ok().flatten())
            .map(|c| c.default_language)
            .unwrap_or_else(|| "en".into());
        let cfg = crate::config::get_config_map(conn)?;
        Ok::<_, AppError>((tracks, speakers, path, lang, cfg))
    })?;
    let pick = |key: &str, over: Option<&String>| -> String {
        over.map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| cfg.get(key).cloned().filter(|s| !s.is_empty()))
            .unwrap_or_default()
    };
    let accelerator = {
        let a = pick("transcription_accelerator", None);
        if a.is_empty() {
            "auto".to_string()
        } else {
            a
        }
    };
    // Per-request overrides win over the saved defaults; the engine is either the
    // native one or a cloud provider id.
    let engine = crate::config::resolve_transcription_provider(&pick(
        "transcription_provider",
        req.provider.as_ref(),
    ));

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

    // A lone unlabelled track is a mixed recording of the whole table — per-track
    // speaker attribution would be wrong, so leave the segments speakerless.
    let single_track = track_list.len() == 1;
    let tracks: Vec<(String, PathBuf, String)> = track_list
        .iter()
        .filter_map(|t| {
            let id = t.get("id").and_then(Value::as_str)?.to_string();
            let path = t.get("file_path").and_then(Value::as_str)?.to_string();
            let mut label = speaker_label(speaker_map.get(&id), &id);
            if single_track && label == id {
                label = String::new(); // no real name assigned, only the fallback
            }
            Some((id, PathBuf::from(path), label))
        })
        .collect();

    use std::sync::Arc;

    use crate::transcription::Watch;

    // "Timeout" means stall, not wall clock: a multi-hour session legitimately
    // transcribes for hours, so the only thing worth killing is a job that has
    // stopped making progress (e.g. a corrupt file pinning the decoder).
    let timeout_secs: u64 = cfg
        .get("transcription_timeout_seconds")
        .and_then(|s| s.parse().ok())
        .filter(|&s: &u64| s > 0)
        .unwrap_or(600);

    let watch = Arc::new(Watch::default());
    let progress = state.model_progress.clone();

    // Cloud runs in-task (it's IO-bound and already async); the native engine is
    // CPU-heavy and goes to a blocking thread under the stall watchdog below.
    let (used_provider, used_model, cloud_outcome) =
        if let Some(cloud) = crate::asr_models::cloud_provider(&engine) {
            let model = pick("transcription_cloud_model", req.model.as_ref());
            let model = if model.is_empty() {
                cloud.default_model.to_string()
            } else {
                model
            };
            let saved = state
                .with_db(|conn| crate::llm::get_key(conn, cloud.id))?
                .unwrap_or_default();
            if saved.api_key.trim().is_empty() {
                return Err(AppError::BadRequest(format!(
                    "No API key saved for {} — add one under Settings → LLM providers.",
                    cloud.name
                )));
            }
            let api_base = if saved.api_base.trim().is_empty() {
                crate::llm::get(cloud.id)
                    .and_then(|p| p.default_api_base)
                    .unwrap_or_default()
                    .to_string()
            } else {
                saved.api_base.trim().to_string()
            };
            let cfg_cloud = crate::transcription::cloud::CloudAsr {
                api_base,
                api_key: saved.api_key.trim().to_string(),
                model: model.clone(),
            };
            let outcome = crate::transcription::cloud::transcribe_tracks(
                &cfg_cloud, &language, &tracks, &watch, &progress,
            )
            .await
            .map_err(AppError::Internal)?;
            (cloud.id.to_string(), model, Some(outcome))
        } else {
            (engine.clone(), String::new(), None)
        };

    let (used_provider, used_model, outcome) = match cloud_outcome {
        Some(outcome) => (used_provider, used_model, outcome),
        None => {
            let asr = crate::asr_models::resolve(&pick("transcription_model", req.model.as_ref()));
            // Download the model if needed, then run it off-thread.
            let model_dir = match model::ensure(&state.paths, asr, &state.model_progress).await {
                Ok(dir) => dir,
                Err(e) => {
                    crate::state::ModelProgress::set_error(&state.model_progress, e.to_string());
                    return Err(AppError::Internal(e));
                }
            };
            // Best-effort VAD fetch (None → fixed-window fallback in the engine).
            let vad_model = model::ensure_vad(&state.paths).await;
            // Resolve the accelerator preference to a concrete provider for this
            // OS; the engine still falls back to CPU if it isn't linked in.
            let accelerator = crate::config::resolve_accelerator(&accelerator);
            let watch_worker = watch.clone();
            let progress = progress.clone();
            let lang = language.clone();
            let mut handle = tokio::task::spawn_blocking(move || {
                transcribe_tracks(
                    &model_dir,
                    asr,
                    &lang,
                    accelerator,
                    vad_model.as_deref(),
                    &tracks,
                    &watch_worker,
                    &progress,
                )
            });
            let outcome = watch_native(&mut handle, &watch, timeout_secs).await?;
            (used_provider, asr.id.to_string(), outcome)
        }
    };

    if !outcome.complete && outcome.segments.is_empty() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "Transcription stalled (no progress for {timeout_secs}s) before any speech was \
             transcribed. Nothing was saved."
        )));
    }

    let transcript_text = segments_to_plain_text(&outcome.segments);

    // Writes transcript.md + provenance into session.toml (files are truth).
    state.with_db(|conn| {
        crate::store::artifacts::insert_artifact(
            conn,
            &req.session_id,
            "transcript",
            &used_provider,
            &used_model,
            &transcript_text,
        )
    })?;

    if !outcome.complete {
        return Err(AppError::Internal(anyhow::anyhow!(
            "Transcription stalled (no progress for {timeout_secs}s) and was cancelled. A partial \
             transcript ({} segments) was saved — re-run after checking the audio files.",
            outcome.segments.len()
        )));
    }

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

/// `/transcribe-dictation` — one-shot speech-to-text for the chatbox mic. The
/// frontend records mic audio, encodes it as a WAV blob, and POSTs the raw
/// bytes; we run it through the same engine as session transcription (single
/// unlabelled track) and return the plain text. No session, no DB, no files.
#[cfg(feature = "transcription")]
pub async fn dictate(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> AppResult<Json<Value>> {
    use std::sync::Arc;

    use crate::error::AppError;
    use crate::transcript_format::segments_to_plain_text;
    use crate::transcription::{model, transcribe_tracks, Watch};

    if body.is_empty() {
        return Err(AppError::BadRequest("Empty audio.".into()));
    }

    // Stage the upload as a temp .wav so symphonia can probe it; cleaned up
    // regardless of outcome below.
    let tmp = std::env::temp_dir().join(format!(
        "ck_dictate_{}_{}.wav",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&tmp, &body).map_err(|e| AppError::Internal(e.into()))?;

    let cfg = state.with_db(crate::config::get_config_map).ok();
    let pick = |key: &str, fallback: &str| -> String {
        cfg.as_ref()
            .and_then(|c| c.get(key).cloned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback.to_string())
    };
    // Dictation always runs on-device: it's a short clip and the round-trip to a
    // cloud endpoint would defeat the point.
    let asr = crate::asr_models::resolve(&pick("transcription_model", ""));
    let language = pick("default_language", "en");
    let model_dir = match model::ensure(&state.paths, asr, &state.model_progress).await {
        Ok(dir) => dir,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            crate::state::ModelProgress::set_error(&state.model_progress, e.to_string());
            return Err(AppError::Internal(e));
        }
    };
    let vad_model = model::ensure_vad(&state.paths).await;
    let accelerator =
        crate::config::resolve_accelerator(&pick("transcription_accelerator", "auto"));

    let watch = Arc::new(Watch::default());
    let progress = state.model_progress.clone();
    let tracks = vec![("mic".to_string(), tmp.clone(), String::new())];
    let result = tokio::task::spawn_blocking(move || {
        transcribe_tracks(
            &model_dir,
            asr,
            &language,
            accelerator,
            vad_model.as_deref(),
            &tracks,
            &watch,
            &progress,
        )
    })
    .await;
    let _ = std::fs::remove_file(&tmp);

    let outcome = result
        .map_err(|e| AppError::Internal(anyhow::anyhow!("dictation task: {e}")))?
        .map_err(AppError::Internal)?;
    Ok(Json(
        json!({ "text": segments_to_plain_text(&outcome.segments) }),
    ))
}

#[cfg(not(feature = "transcription"))]
pub async fn dictate(
    State(_state): State<AppState>,
    _body: axum::body::Bytes,
) -> AppResult<Json<Value>> {
    Err(crate::error::AppError::BadRequest(
        "Transcription is not available in this build.".into(),
    ))
}
