//! Cloud ASR via the OpenAI-compatible `/audio/transcriptions` endpoint
//! (OpenAI, Groq). Keys come from the existing `provider_keys` rows, so BYO-key
//! setup is shared with the LLM providers.
//!
//! Audio is decoded locally and re-encoded as 16kHz mono WAV before upload:
//! both providers cap a request at 25 MB, which is ~13 minutes at that rate, so
//! long sessions are cut into chunks and the returned timings are shifted back
//! onto the original timeline.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{decode, make_segment, to_target_sr, Transcribed, Watch, TARGET_SR};
use crate::state::ModelProgress;

/// Upload chunk length. 10 min of 16kHz mono 16-bit PCM is ~19 MB, safely under
/// the 25 MB request cap with room for the WAV header and multipart framing.
const CHUNK_SECS: f64 = 600.0;

/// How far either side of a chunk boundary to hunt for a quiet spot, so cuts
/// land in silence instead of mid-word.
const SEEK_SECS: f64 = 20.0;

/// Window used when scoring "how quiet is it here".
const QUIET_WIN_SECS: f64 = 0.4;

pub struct CloudAsr {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Deserialize)]
struct VerboseJson {
    #[serde(default)]
    text: String,
    #[serde(default)]
    segments: Vec<CloudSegment>,
}

#[derive(Deserialize)]
struct CloudSegment {
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
    #[serde(default)]
    text: String,
}

/// Encode 16kHz mono f32 samples as a 16-bit PCM WAV file in memory.
fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&(TARGET_SR as u32).to_le_bytes());
    out.extend_from_slice(&((TARGET_SR as u32) * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Chunk boundaries (sample indices) that prefer the quietest point near each
/// nominal cut, so a chunk rarely ends mid-word.
fn chunk_bounds(samples: &[f32]) -> Vec<(usize, usize)> {
    let chunk = (CHUNK_SECS * TARGET_SR as f64) as usize;
    let seek = (SEEK_SECS * TARGET_SR as f64) as usize;
    let win = (QUIET_WIN_SECS * TARGET_SR as f64) as usize;
    let mut bounds = Vec::new();
    let mut start = 0usize;
    while start < samples.len() {
        let nominal = start + chunk;
        if nominal >= samples.len() {
            bounds.push((start, samples.len()));
            break;
        }
        let lo = nominal.saturating_sub(seek).max(start + win);
        let hi = (nominal + seek).min(samples.len().saturating_sub(win));
        let mut best = (nominal, f32::MAX);
        let mut at = lo;
        while at < hi {
            let energy: f32 = samples[at..at + win].iter().map(|s| s.abs()).sum();
            if energy < best.1 {
                best = (at, energy);
            }
            at += win;
        }
        let end = best.0.max(start + win);
        bounds.push((start, end));
        start = end;
    }
    bounds
}

struct SendErr {
    err: anyhow::Error,
    retry_plain: bool,
}

async fn send(
    client: &reqwest::Client,
    cfg: &CloudAsr,
    language: &str,
    wav: &[u8],
    response_format: &str,
) -> std::result::Result<String, SendErr> {
    let plain = |e: anyhow::Error| SendErr {
        err: e,
        retry_plain: false,
    };
    let part = reqwest::multipart::Part::bytes(wav.to_vec())
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| plain(e.into()))?;
    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", cfg.model.clone())
        .text("response_format", response_format.to_string());
    if !language.is_empty() {
        form = form.text("language", language.to_string());
    }
    let url = format!(
        "{}/audio/transcriptions",
        cfg.api_base.trim_end_matches('/')
    );
    let resp = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .multipart(form)
        .send()
        .await
        .context("cloud transcription request failed")
        .map_err(plain)?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        return Ok(body);
    }
    Err(SendErr {
        retry_plain: status == reqwest::StatusCode::BAD_REQUEST
            && response_format == "verbose_json",
        err: anyhow::anyhow!("cloud transcription failed ({status}): {}", body.trim()),
    })
}

async fn post_chunk(
    client: &reqwest::Client,
    cfg: &CloudAsr,
    language: &str,
    wav: Vec<u8>,
) -> Result<Vec<(f64, f64, String)>> {
    // `verbose_json` carries per-segment timestamps but the gpt-4o transcribe
    // models reject it, so fall back to plain `json` on a 400.
    let mut body = send(client, cfg, language, &wav, "verbose_json").await;
    if matches!(&body, Err(e) if e.retry_plain) {
        body = send(client, cfg, language, &wav, "json").await;
    }
    let body = body.map_err(|e| e.err)?;
    let parsed: VerboseJson =
        serde_json::from_str(&body).context("unexpected cloud transcription response")?;
    if parsed.segments.is_empty() {
        let text = parsed.text.trim().to_string();
        return Ok(if text.is_empty() {
            Vec::new()
        } else {
            vec![(0.0, 0.0, text)]
        });
    }
    Ok(parsed
        .segments
        .into_iter()
        .filter(|s| !s.text.trim().is_empty())
        .map(|s| (s.start, s.end, s.text.trim().to_string()))
        .collect())
}

/// Transcribe every track through the cloud endpoint. Mirrors
/// [`super::transcribe_tracks`]: a failed track is skipped rather than aborting
/// the run, and cancelling returns the segments finished so far.
pub async fn transcribe_tracks(
    cfg: &CloudAsr,
    language: &str,
    tracks: &[(String, PathBuf, String)],
    watch: &Arc<Watch>,
    progress: &Arc<Mutex<ModelProgress>>,
) -> Result<Transcribed> {
    let client = reqwest::Client::builder()
        // A 10-minute chunk on a slow uplink plus server-side decode.
        .timeout(std::time::Duration::from_secs(900))
        .build()?;
    let total = tracks.len() as u64;
    let mut all: Vec<crate::models::Segment> = Vec::new();
    ModelProgress::set_transcribe(progress, 0, total, "");

    for (idx, (track_id, path, label)) in tracks.iter().enumerate() {
        if watch.cancelled() {
            break;
        }
        if !path.exists() {
            tracing::warn!("track file missing, skipping: {}", path.display());
            continue;
        }
        let shown = if label.is_empty() { track_id } else { label };
        ModelProgress::set_transcribe(progress, idx as u64, total, shown);

        let (p, w) = (path.clone(), watch.clone());
        let decoded = tokio::task::spawn_blocking(move || {
            decode::decode_to_mono(&p, &w).map(|(samples, sr)| to_target_sr(&samples, sr))
        })
        .await
        .map_err(|e| anyhow::anyhow!("decode task: {e}"))?;
        let samples = match decoded {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("track {}/{total}: decode failed, skipping: {e:#}", idx + 1);
                continue;
            }
        };

        let bounds = chunk_bounds(&samples);
        tracing::info!(
            "track {}/{total} '{shown}': {:.0}s audio, {} cloud chunk(s) via {}",
            idx + 1,
            samples.len() as f64 / TARGET_SR as f64,
            bounds.len(),
            cfg.model
        );
        for (start, end) in bounds {
            if watch.cancelled() {
                break;
            }
            watch.tick();
            let offset = start as f64 / TARGET_SR as f64;
            let wav = wav_bytes(&samples[start..end]);
            match post_chunk(&client, cfg, language, wav).await {
                Ok(segs) => {
                    for (s, e, text) in segs {
                        all.push(make_segment(text, offset + s, offset + e, track_id, label));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "track {}/{total} chunk at {offset:.0}s failed, skipping: {e:#}",
                        idx + 1
                    );
                }
            }
        }
    }

    ModelProgress::set_transcribe(progress, total, total, "");
    all.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(Transcribed {
        segments: all,
        complete: !watch.cancelled(),
    })
}
