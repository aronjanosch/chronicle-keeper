use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use futures_util::StreamExt;

use crate::paths::Paths;
use crate::state::ModelProgress;

pub const MODEL_DIR_NAME: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2";
const VAD_MODEL_NAME: &str = "silero_vad.onnx";
const VAD_MODEL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
const REQUIRED: [&str; 4] = [
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

pub fn model_dir(paths: &Paths) -> PathBuf {
    paths.models_dir().join(MODEL_DIR_NAME)
}

pub fn is_present(dir: &Path) -> bool {
    REQUIRED.iter().all(|f| dir.join(f).exists())
}

/// Ensure the Parakeet model is available, downloading + extracting it once if
/// missing. Returns the model directory. Reports download/extract progress into
/// `progress` so the frontend can render a bar via `GET /model-status`.
pub async fn ensure(paths: &Paths, progress: &Arc<Mutex<ModelProgress>>) -> Result<PathBuf> {
    let dir = model_dir(paths);
    if is_present(&dir) {
        ModelProgress::set(progress, "ready", 0, 0);
        return Ok(dir);
    }
    let models_root = paths.models_dir();
    std::fs::create_dir_all(&models_root).context("create models dir")?;

    tracing::info!("downloading Parakeet model (~465MB, one time)…");
    let archive = models_root.join("parakeet-v3.tar.bz2");
    download(MODEL_URL, &archive, progress)
        .await
        .context("download model")?;

    tracing::info!("extracting model archive…");
    ModelProgress::set(progress, "extracting", 0, 0);
    extract_tar_bz2(&archive, &models_root).context("extract model")?;
    let _ = std::fs::remove_file(&archive);

    if !is_present(&dir) {
        anyhow::bail!(
            "model archive extracted but expected files missing in {}",
            dir.display()
        );
    }
    tracing::info!("model ready at {}", dir.display());
    ModelProgress::set(progress, "ready", 0, 0);
    Ok(dir)
}

/// Ensure the Silero VAD model (~2MB) is available, downloading it once if
/// missing. Best-effort: returns `None` (logged) on any failure so transcription
/// degrades to fixed-window chunking rather than aborting.
pub async fn ensure_vad(paths: &Paths) -> Option<PathBuf> {
    let dest = paths.models_dir().join(VAD_MODEL_NAME);
    if dest.exists() {
        return Some(dest);
    }
    if let Err(e) = std::fs::create_dir_all(paths.models_dir()) {
        tracing::warn!("VAD: create models dir failed: {e}");
        return None;
    }
    tracing::info!("downloading Silero VAD model (~2MB, one time)…");
    match download_simple(VAD_MODEL_URL, &dest).await {
        Ok(()) => Some(dest),
        Err(e) => {
            tracing::warn!("VAD model download failed ({e}); using fixed-window chunking");
            let _ = std::fs::remove_file(&dest);
            None
        }
    }
}

async fn download_simple(url: &str, dest: &Path) -> Result<()> {
    let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
    std::fs::write(dest, &bytes)?;
    Ok(())
}

async fn download(url: &str, dest: &Path, progress: &Arc<Mutex<ModelProgress>>) -> Result<()> {
    let resp = reqwest::get(url).await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();
    let mut file = std::fs::File::create(dest)?;
    use std::io::Write;
    let mut downloaded: u64 = 0;
    ModelProgress::set(progress, "downloading", 0, total);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        ModelProgress::set(progress, "downloading", downloaded, total);
    }
    file.flush()?;
    Ok(())
}

fn extract_tar_bz2(archive: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let decompressed = bzip2::read::BzDecoder::new(file);
    let mut tar = tar::Archive::new(decompressed);
    tar.unpack(dest)?;
    Ok(())
}
