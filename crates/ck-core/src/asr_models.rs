//! Selectable on-device ASR models (sherpa-onnx prebuilts).
//!
//! Data only — no sherpa types — so `/providers` and config validation still
//! work in the headless build, which omits the `transcription` feature. The
//! mapping from [`Family`] onto sherpa's per-family config slots lives in
//! `transcription::create_recognizer`.
//!
//! k2-fsa publishes each model at one precision only, so "run fp16/fp32 instead
//! of int8" means picking a different entry here, not a flag: Parakeet v3 exists
//! solely as int8, v2 solely as fp16, Canary solely as fp32.

use serde::Serialize;

const RELEASE_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models";

/// Which sherpa `OfflineModelConfig` slot a model fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Transducer,
    Whisper,
    Canary,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct AsrModel {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Archive stem, which is also the directory it extracts to.
    pub dir: &'static str,
    pub family: Family,
    pub encoder: &'static str,
    pub decoder: &'static str,
    /// Transducer-only; empty for the other families.
    pub joiner: &'static str,
    pub tokens: &'static str,
    pub precision: &'static str,
    pub download_mb: u32,
    /// Rough resident size of one recognizer, used to cap parallel workers.
    pub ram_mb: u32,
    /// Longest audio the encoder accepts; chunking must stay under it.
    pub max_chunk_secs: f32,
    pub languages: &'static str,
}

impl AsrModel {
    pub fn url(&self) -> String {
        format!("{RELEASE_BASE}/{}.tar.bz2", self.dir)
    }

    /// Files that must exist for the model to count as downloaded.
    pub fn required_files(&self) -> Vec<&'static str> {
        [self.encoder, self.decoder, self.joiner, self.tokens]
            .into_iter()
            .filter(|f| !f.is_empty())
            .collect()
    }
}

pub const DEFAULT_MODEL_ID: &str = "parakeet-tdt-0.6b-v3-int8";

pub const MODELS: &[AsrModel] = &[
    AsrModel {
        id: DEFAULT_MODEL_ID,
        name: "Parakeet TDT 0.6B v3",
        description: "Fast and accurate across 25 European languages (recommended default).",
        dir: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
        family: Family::Transducer,
        encoder: "encoder.int8.onnx",
        decoder: "decoder.int8.onnx",
        joiner: "joiner.int8.onnx",
        tokens: "tokens.txt",
        precision: "int8",
        download_mb: 465,
        ram_mb: 650,
        // Empirical: the int8 encoder's max sequence is ~50s.
        max_chunk_secs: 28.0,
        languages: "25 European languages",
    },
    AsrModel {
        id: "parakeet-tdt-0.6b-v2-fp16",
        name: "Parakeet TDT 0.6B v2 (fp16)",
        description: "English only, half-precision weights — no quantisation loss. Needs ~2 GB RAM per worker.",
        dir: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-fp16",
        family: Family::Transducer,
        encoder: "encoder.fp16.onnx",
        decoder: "decoder.fp16.onnx",
        joiner: "joiner.fp16.onnx",
        tokens: "tokens.txt",
        precision: "fp16",
        download_mb: 1069,
        ram_mb: 1800,
        max_chunk_secs: 28.0,
        languages: "English",
    },
    AsrModel {
        id: "canary-180m-flash-fp32",
        name: "Canary 180M Flash (fp32)",
        description: "Full-precision weights with built-in punctuation and capitalisation. English, German, Spanish, French.",
        dir: "sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr",
        family: Family::Canary,
        encoder: "encoder.onnx",
        decoder: "decoder.onnx",
        joiner: "",
        tokens: "tokens.txt",
        precision: "fp32",
        download_mb: 684,
        ram_mb: 1100,
        max_chunk_secs: 28.0,
        languages: "English, German, Spanish, French",
    },
    AsrModel {
        id: "whisper-turbo-int8",
        name: "Whisper Turbo",
        description: "OpenAI Whisper, ~99 languages. Slower than Parakeet; better on mixed-language speech.",
        dir: "sherpa-onnx-whisper-turbo",
        family: Family::Whisper,
        encoder: "turbo-encoder.int8.onnx",
        decoder: "turbo-decoder.int8.onnx",
        joiner: "",
        tokens: "turbo-tokens.txt",
        precision: "int8",
        download_mb: 538,
        ram_mb: 1400,
        // Whisper's encoder is hard-wired to 30s windows.
        max_chunk_secs: 28.0,
        languages: "~99 languages",
    },
    AsrModel {
        id: "whisper-large-v3-int8",
        name: "Whisper large-v3",
        description: "The largest Whisper. Slowest option by a wide margin; needs ~2.5 GB RAM per worker.",
        dir: "sherpa-onnx-whisper-large-v3",
        family: Family::Whisper,
        encoder: "large-v3-encoder.int8.onnx",
        decoder: "large-v3-decoder.int8.onnx",
        joiner: "",
        tokens: "large-v3-tokens.txt",
        precision: "int8",
        download_mb: 1019,
        ram_mb: 2400,
        max_chunk_secs: 28.0,
        languages: "~99 languages",
    },
];

/// LLM providers that also expose an OpenAI-compatible
/// `/audio/transcriptions` endpoint, so their saved key doubles as an ASR key.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CloudAsrProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub models: &'static [&'static str],
    pub default_model: &'static str,
}

pub const CLOUD_PROVIDERS: &[CloudAsrProvider] = &[
    CloudAsrProvider {
        id: "openai",
        name: "OpenAI",
        models: &["whisper-1", "gpt-4o-transcribe", "gpt-4o-mini-transcribe"],
        // whisper-1 is the only one that returns per-segment timestamps.
        default_model: "whisper-1",
    },
    CloudAsrProvider {
        id: "groq",
        name: "Groq",
        models: &["whisper-large-v3-turbo", "whisper-large-v3"],
        default_model: "whisper-large-v3-turbo",
    },
];

pub fn cloud_provider(id: &str) -> Option<&'static CloudAsrProvider> {
    let id = id.trim();
    CLOUD_PROVIDERS.iter().find(|p| p.id == id)
}

pub fn find(id: &str) -> Option<&'static AsrModel> {
    let id = id.trim();
    MODELS.iter().find(|m| m.id == id)
}

/// Look up a model id, falling back to the default for empty/unknown values so
/// a stale config can never leave transcription unusable.
pub fn resolve(id: &str) -> &'static AsrModel {
    find(id).unwrap_or_else(|| find(DEFAULT_MODEL_ID).expect("default model is in MODELS"))
}

pub fn ids() -> Vec<&'static str> {
    MODELS.iter().map(|m| m.id).collect()
}
