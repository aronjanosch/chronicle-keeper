//! Native transcription engine (Parakeet TDT v3 via sherpa-onnx). Compiled
//! only with the `transcription` feature; the server build omits it.

mod decode;
pub mod model;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

use crate::models::Segment;

/// Window length in seconds. The int8 ONNX encoder has a fixed max sequence
/// (~50s); 30s stays safely under it. Silero-VAD splitting is a later quality
/// refinement (cleaner word boundaries); fixed windows are correct enough for
/// LLM summarization.
const CHUNK_SECS: u32 = 30;

fn build_recognizer(model_dir: &Path) -> Result<OfflineRecognizer> {
    let p = |name: &str| -> Result<String> {
        let path = model_dir.join(name);
        anyhow::ensure!(path.exists(), "missing model file: {}", path.display());
        Ok(path.to_string_lossy().into_owned())
    };
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer = OfflineTransducerModelConfig {
        encoder: Some(p("encoder.int8.onnx")?),
        decoder: Some(p("decoder.int8.onnx")?),
        joiner: Some(p("joiner.int8.onnx")?),
    };
    config.model_config.tokens = Some(p("tokens.txt")?);
    config.model_config.provider = Some("cpu".to_string());
    config.model_config.num_threads = num_threads();
    config.model_config.debug = false;
    OfflineRecognizer::create(&config).ok_or_else(|| anyhow::anyhow!("failed to create recognizer"))
}

fn num_threads() -> i32 {
    std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(2).clamp(1, 8)
}

fn transcribe_one(
    recognizer: &OfflineRecognizer,
    samples: &[f32],
    sample_rate: u32,
    track_id: &str,
    label: &str,
) -> Vec<Segment> {
    let chunk_len = (CHUNK_SECS * sample_rate) as usize;
    let mut segments = Vec::new();
    for (idx, chunk) in samples.chunks(chunk_len).enumerate() {
        let stream = recognizer.create_stream();
        stream.accept_waveform(sample_rate as i32, chunk);
        recognizer.decode(&stream);
        let text = stream.get_result().map(|r| r.text.trim().to_string()).unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        let start = (idx as u32 * CHUNK_SECS) as f64;
        let end = start + (chunk.len() as f64 / sample_rate as f64);
        segments.push(Segment {
            text,
            start,
            end,
            speaker: Some(label.to_string()),
            source: Some(track_id.to_string()),
            words: None,
        });
    }
    segments
}

/// Transcribe every track and return speaker-labelled segments sorted by start.
/// `tracks` is `(track_id, file_path, speaker_label)`.
pub fn transcribe_tracks(model_dir: &Path, tracks: &[(String, PathBuf, String)]) -> Result<Vec<Segment>> {
    let recognizer = build_recognizer(model_dir)?;
    let mut all = Vec::new();
    for (track_id, path, label) in tracks {
        if !path.exists() {
            tracing::warn!("track file missing, skipping: {}", path.display());
            continue;
        }
        let (samples, sr) = decode::decode_to_mono(path)
            .with_context(|| format!("decode {}", path.display()))?;
        all.extend(transcribe_one(&recognizer, &samples, sr, track_id, label));
    }
    all.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    Ok(all)
}
