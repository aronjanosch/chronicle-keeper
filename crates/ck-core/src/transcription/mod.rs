//! Native transcription engine (Parakeet TDT v3 via sherpa-onnx). Compiled
//! only with the `transcription` feature; the server build omits it.

mod decode;
pub mod model;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

/// Shared between the HTTP handler and the blocking worker. The worker bumps
/// `ticks` as it makes progress (per decoded packet / VAD window) so the
/// handler's watchdog can tell a stalled job from a long one; flipping `cancel`
/// makes the worker bail at its next boundary (blocking threads can't be
/// force-aborted, so cancellation is cooperative).
#[derive(Default)]
pub struct Watch {
    cancel: AtomicBool,
    ticks: AtomicU64,
}

impl Watch {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn tick(&self) {
        self.ticks.fetch_add(1, Ordering::Relaxed);
    }
    pub fn ticks(&self) -> u64 {
        self.ticks.load(Ordering::Relaxed)
    }
}

/// Result of a transcription run. `complete` is `false` when the run was
/// cancelled mid-way — `segments` then holds whatever finished before the stop.
pub struct Transcribed {
    pub segments: Vec<Segment>,
    pub complete: bool,
}

fn build_recognizer(
    model_dir: &Path,
    accelerator: &str,
    threads: i32,
) -> Result<OfflineRecognizer> {
    match create_recognizer(model_dir, accelerator, threads) {
        Some(r) => Ok(r),
        None if accelerator != "cpu" => {
            tracing::warn!(
                "failed to create recognizer with provider '{accelerator}'; falling back to cpu \
                 (the bundled onnxruntime may lack that execution provider)"
            );
            create_recognizer(model_dir, "cpu", threads)
                .ok_or_else(|| anyhow::anyhow!("failed to create recognizer (cpu)"))
        }
        None => Err(anyhow::anyhow!("failed to create recognizer")),
    }
}

fn create_recognizer(model_dir: &Path, provider: &str, threads: i32) -> Option<OfflineRecognizer> {
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
    config.model_config.num_threads = threads;
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

/// Pick (worker count, ONNX threads per worker) for parallel track ASR.
/// Measured on the int8 Parakeet encoder: a single stream tops out at 4
/// threads (4thr beat 8thr on a 10-core M-series), so the core budget —
/// everything minus two cores for the decode thread and the system — is split
/// into up-to-4-thread workers, each with its own recognizer (~650 MB RAM
/// apiece, hence the cap of 3).
fn plan_workers(track_count: usize) -> (usize, i32) {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    let budget = cores.saturating_sub(2).max(1);
    let workers = (budget / 3).clamp(1, 3).min(track_count.max(1));
    let threads = (budget / workers).clamp(2, 4) as i32;
    (workers, threads)
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
///
/// Segments are decoded one stream at a time on purpose: batched
/// `decode_multiple_streams` was measured to silently return empty text for
/// ~25% of segments with the int8 Parakeet model (padding contamination), so
/// batching is off the table.
fn transcribe_vad(
    recognizer: &OfflineRecognizer,
    vad_model: &Path,
    samples: &[f32],
    track_id: &str,
    label: &str,
    watch: &Watch,
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
        if watch.cancelled() {
            break;
        }
        watch.tick();
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
    watch: &Watch,
) -> Vec<Segment> {
    let chunk_len = (CHUNK_SECS as i32 * TARGET_SR) as usize;
    let mut segments = Vec::new();
    for (idx, chunk) in samples.chunks(chunk_len).enumerate() {
        if watch.cancelled() {
            break;
        }
        watch.tick();
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
        // Empty label = no speaker attribution (e.g. a single mixed-voices track);
        // downstream formatting then omits the `[Speaker]` headers entirely.
        speaker: (!label.is_empty()).then(|| label.to_string()),
        source: Some(track_id.to_string()),
        words: None,
    }
}

/// Transcribe every track and return speaker-labelled segments sorted by start.
/// `tracks` is `(track_id, file_path, speaker_label)`. `accelerator` is the
/// onnxruntime execution provider (cpu/coreml/cuda/directml); falls back to cpu
/// if unsupported. `vad_model`, when present, enables Silero-VAD segmentation.
///
/// Audio decode runs on its own thread feeding a pool of ASR workers (see
/// [`plan_workers`]), each with its own recognizer, pulling tracks off a shared
/// channel. The bounded channel keeps at most workers+1 decoded tracks in
/// memory. A failed decode skips that track instead of aborting the run. On
/// cancel (via `watch`) the finished segments are still returned, with
/// `complete: false`.
pub fn transcribe_tracks(
    model_dir: &Path,
    accelerator: &str,
    vad_model: Option<&Path>,
    tracks: &[(String, PathBuf, String)],
    watch: &Watch,
    progress: &Arc<Mutex<ModelProgress>>,
) -> Result<Transcribed> {
    let started = Instant::now();
    let total = tracks.len() as u64;
    let (workers, threads) = plan_workers(tracks.len());
    let recognizers = (0..workers)
        .map(|_| build_recognizer(model_dir, accelerator, threads))
        .collect::<Result<Vec<_>>>()?;
    tracing::info!(
        "transcribing {} track(s) [provider={accelerator}, vad={}, {workers} worker(s) × {threads} thread(s)]",
        tracks.len(),
        vad_model.is_some()
    );
    ModelProgress::set_transcribe(progress, 0, total, "");
    let all = Mutex::new(Vec::new());
    let done = AtomicU64::new(0);

    type Decoded<'a> = (usize, &'a str, &'a str, Vec<f32>);
    let (tx, rx) = std::sync::mpsc::sync_channel::<Decoded>(1);
    // Workers race on the receiver; whoever is free takes the next track.
    let rx = Mutex::new(rx);

    std::thread::scope(|s| {
        s.spawn(move || {
            for (idx, (track_id, path, label)) in tracks.iter().enumerate() {
                if watch.cancelled() {
                    return;
                }
                if !path.exists() {
                    tracing::warn!("track file missing, skipping: {}", path.display());
                    continue;
                }
                let t0 = Instant::now();
                let decoded = decode::decode_to_mono(path, watch)
                    .with_context(|| format!("decode {}", path.display()))
                    .map(|(samples, sr)| to_target_sr(&samples, sr));
                if watch.cancelled() {
                    return;
                }
                let samples = match decoded {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(
                            "track {}/{}: decode failed, skipping: {e:#}",
                            idx + 1,
                            total
                        );
                        continue;
                    }
                };
                tracing::info!(
                    "track {}/{}: decoded {:.0}s audio in {:.1}s",
                    idx + 1,
                    total,
                    samples.len() as f64 / TARGET_SR as f64,
                    t0.elapsed().as_secs_f64()
                );
                if tx.send((idx, track_id, label, samples)).is_err() {
                    return;
                }
            }
        });

        for recognizer in recognizers {
            let (rx, all, done) = (&rx, &all, &done);
            s.spawn(move || loop {
                // Holding the lock while recv() blocks is fine: only one idle
                // worker can take the next track anyway, the rest queue here.
                let received = rx.lock().unwrap().recv();
                let Ok((idx, track_id, label, samples)) = received else {
                    return;
                };
                if watch.cancelled() {
                    return;
                }
                let shown = if label.is_empty() { track_id } else { label };
                ModelProgress::set_transcribe(progress, done.load(Ordering::Relaxed), total, shown);
                let secs = samples.len() as f64 / TARGET_SR as f64;
                tracing::info!(
                    "track {}/{} '{shown}' ({track_id}): {secs:.0}s audio, transcribing…",
                    idx + 1,
                    total
                );
                let t0 = Instant::now();
                let segs = vad_model
                    .and_then(|m| transcribe_vad(&recognizer, m, &samples, track_id, label, watch))
                    .unwrap_or_else(|| {
                        transcribe_fixed(&recognizer, &samples, track_id, label, watch)
                    });
                tracing::info!(
                    "track {}/{} '{shown}': {} segment(s) in {:.1}s",
                    idx + 1,
                    total,
                    segs.len(),
                    t0.elapsed().as_secs_f64()
                );
                all.lock().unwrap().extend(segs);
                done.fetch_add(1, Ordering::Relaxed);
            });
        }
    });

    ModelProgress::set_transcribe(progress, total, total, "");
    let complete = !watch.cancelled();
    let mut all = all.into_inner().unwrap();
    all.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    tracing::info!(
        "transcription {}: {} segment(s) across {total} track(s) in {:.1}s",
        if complete { "done" } else { "cancelled (partial)" },
        all.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(Transcribed {
        segments: all,
        complete,
    })
}
