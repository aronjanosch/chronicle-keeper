//! Native transcription engine (Parakeet TDT v3 via sherpa-onnx). Compiled
//! only with the `transcription` feature; the server build omits it.

mod decode;
pub mod model;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
use sherpa_onnx::{
    LinearResampler, OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig,
    SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};

use crate::models::Segment;
use crate::state::ModelProgress;

/// Sample rate the engine + VAD operate at. Audio is resampled here once; the
/// recognizer no longer resamples internally because we feed it 16k directly.
const TARGET_SR: i32 = 16_000;

/// Fallback window length in seconds when no VAD model is available. The int8
/// ONNX encoder has a fixed max sequence (~50s); 30s stays safely under it.
const CHUNK_SECS: u32 = 30;

/// Cap on a single VAD speech segment (seconds). Keeps even long monologues
/// under the encoder's max sequence; VAD splits anything longer at this bound.
const VAD_MAX_SPEECH_SECS: f32 = 28.0;

/// Silero VAD processing window (samples) at 16kHz — the model's native frame.
const VAD_WINDOW: usize = 512;

fn build_recognizer(model_dir: &Path, accelerator: &str) -> Result<OfflineRecognizer> {
    match create_recognizer(model_dir, accelerator) {
        Some(r) => Ok(r),
        None if accelerator != "cpu" => {
            tracing::warn!(
                "failed to create recognizer with provider '{accelerator}'; falling back to cpu \
                 (the bundled onnxruntime may lack that execution provider)"
            );
            create_recognizer(model_dir, "cpu")
                .ok_or_else(|| anyhow::anyhow!("failed to create recognizer (cpu)"))
        }
        None => Err(anyhow::anyhow!("failed to create recognizer")),
    }
}

fn create_recognizer(model_dir: &Path, provider: &str) -> Option<OfflineRecognizer> {
    let p = |name: &str| -> Option<String> {
        let path = model_dir.join(name);
        path.exists().then(|| path.to_string_lossy().into_owned())
    };
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer = OfflineTransducerModelConfig {
        encoder: p("encoder.int8.onnx"),
        decoder: p("decoder.int8.onnx"),
        joiner: p("joiner.int8.onnx"),
    };
    config.model_config.tokens = p("tokens.txt");
    config.model_config.provider = Some(provider.to_string());
    config.model_config.num_threads = num_threads();
    // Sherpa's own stderr logging follows RUST_LOG: off at info, on at debug.
    config.model_config.debug = tracing::enabled!(tracing::Level::DEBUG);
    OfflineRecognizer::create(&config)
}

fn build_vad(vad_model: &Path) -> Option<VoiceActivityDetector> {
    let mut config = VadModelConfig {
        sample_rate: TARGET_SR,
        num_threads: 1,
        provider: Some("cpu".to_string()),
        ..Default::default()
    };
    config.silero_vad = SileroVadModelConfig {
        model: Some(vad_model.to_string_lossy().into_owned()),
        threshold: 0.5,
        min_silence_duration: 0.5,
        min_speech_duration: 0.25,
        window_size: VAD_WINDOW as i32,
        max_speech_duration: VAD_MAX_SPEECH_SECS,
    };
    VoiceActivityDetector::create(&config, 30.0)
}

fn num_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(2)
        .clamp(1, 8)
}

/// Resample mono samples to [`TARGET_SR`]. A no-op clone if already at rate.
fn to_target_sr(samples: &[f32], src_sr: u32) -> Vec<f32> {
    if src_sr as i32 == TARGET_SR {
        return samples.to_vec();
    }
    match LinearResampler::create(src_sr as i32, TARGET_SR) {
        Some(rs) => rs.resample(samples, true),
        None => samples.to_vec(),
    }
}

/// Transcribe one 16kHz mono buffer into text (single recognizer pass).
fn decode_text(recognizer: &OfflineRecognizer, samples: &[f32]) -> String {
    let stream = recognizer.create_stream();
    stream.accept_waveform(TARGET_SR, samples);
    recognizer.decode(&stream);
    stream
        .get_result()
        .map(|r| r.text.trim().to_string())
        .unwrap_or_default()
}

/// VAD-driven segmentation: emit one transcript segment per detected speech run,
/// with timestamps derived from the sample offset. Cleaner boundaries than fixed
/// windows and skips silence. Returns `None` if the VAD model can't be built so
/// the caller can fall back to fixed windows.
fn transcribe_vad(
    recognizer: &OfflineRecognizer,
    vad_model: &Path,
    samples: &[f32],
    track_id: &str,
    label: &str,
    cancel: &AtomicBool,
) -> Option<Vec<Segment>> {
    let vad = build_vad(vad_model)?;
    let mut segments = Vec::new();
    let drain = |vad: &VoiceActivityDetector, segments: &mut Vec<Segment>| {
        while let Some(seg) = vad.front() {
            let text = decode_text(recognizer, seg.samples());
            if !text.is_empty() {
                let start = seg.start() as f64 / TARGET_SR as f64;
                let end = start + seg.n() as f64 / TARGET_SR as f64;
                segments.push(make_segment(text, start, end, track_id, label));
            }
            vad.pop();
        }
    };
    for window in samples.chunks(VAD_WINDOW) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        vad.accept_waveform(window);
        drain(&vad, &mut segments);
    }
    vad.flush();
    drain(&vad, &mut segments);
    Some(segments)
}

/// Fixed ~30s windows on 16kHz samples — the fallback when no VAD model exists.
fn transcribe_fixed(
    recognizer: &OfflineRecognizer,
    samples: &[f32],
    track_id: &str,
    label: &str,
    cancel: &AtomicBool,
) -> Vec<Segment> {
    let chunk_len = (CHUNK_SECS as i32 * TARGET_SR) as usize;
    let mut segments = Vec::new();
    for (idx, chunk) in samples.chunks(chunk_len).enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let text = decode_text(recognizer, chunk);
        if text.is_empty() {
            continue;
        }
        let start = (idx as u32 * CHUNK_SECS) as f64;
        let end = start + chunk.len() as f64 / TARGET_SR as f64;
        segments.push(make_segment(text, start, end, track_id, label));
    }
    segments
}

fn make_segment(text: String, start: f64, end: f64, track_id: &str, label: &str) -> Segment {
    Segment {
        text,
        start,
        end,
        speaker: Some(label.to_string()),
        source: Some(track_id.to_string()),
        words: None,
    }
}

/// Transcribe every track and return speaker-labelled segments sorted by start.
/// `tracks` is `(track_id, file_path, speaker_label)`. `accelerator` is the
/// onnxruntime execution provider (cpu/coreml/cuda/directml); falls back to cpu
/// if unsupported. `vad_model`, when present, enables Silero-VAD segmentation.
/// `cancel`, when flipped to `true` by another thread (e.g. the HTTP handler on
/// timeout), makes the loop bail at the next track/chunk boundary — a blocking
/// `spawn_blocking` thread can't be aborted, so cancellation is cooperative.
pub fn transcribe_tracks(
    model_dir: &Path,
    accelerator: &str,
    vad_model: Option<&Path>,
    tracks: &[(String, PathBuf, String)],
    cancel: &AtomicBool,
    progress: &Arc<Mutex<ModelProgress>>,
) -> Result<Vec<Segment>> {
    let started = Instant::now();
    let recognizer = build_recognizer(model_dir, accelerator)?;
    tracing::info!(
        "transcribing {} track(s) [provider={accelerator}, vad={}]",
        tracks.len(),
        vad_model.is_some()
    );
    let total = tracks.len() as u64;
    let mut all = Vec::new();
    for (idx, (track_id, path, label)) in tracks.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("Transcription cancelled (timed out).");
        }
        ModelProgress::set_transcribe(progress, idx as u64, total, label);
        if !path.exists() {
            tracing::warn!("track file missing, skipping: {}", path.display());
            continue;
        }
        let t0 = Instant::now();
        let (samples, sr) =
            decode::decode_to_mono(path).with_context(|| format!("decode {}", path.display()))?;
        let samples = to_target_sr(&samples, sr);
        let secs = samples.len() as f64 / TARGET_SR as f64;
        tracing::info!(
            "track {}/{} '{label}' ({track_id}): {secs:.0}s audio, decoding…",
            idx + 1,
            total
        );

        let segs = vad_model
            .and_then(|m| transcribe_vad(&recognizer, m, &samples, track_id, label, cancel))
            .unwrap_or_else(|| transcribe_fixed(&recognizer, &samples, track_id, label, cancel));
        tracing::info!(
            "track {}/{} '{label}': {} segment(s) in {:.1}s",
            idx + 1,
            total,
            segs.len(),
            t0.elapsed().as_secs_f64()
        );
        all.extend(segs);
    }
    ModelProgress::set_transcribe(progress, total, total, "");
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("Transcription cancelled (timed out).");
    }
    all.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    tracing::info!(
        "transcription done: {} segment(s) across {total} track(s) in {:.1}s",
        all.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(all)
}
