use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use futures_util::StreamExt;

use crate::paths::Paths;

pub const MODEL_DIR_NAME: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2";
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
/// missing. Returns the model directory.
pub async fn ensure(paths: &Paths) -> Result<PathBuf> {
    let dir = model_dir(paths);
    if is_present(&dir) {
        return Ok(dir);
    }
    let models_root = paths.models_dir();
    std::fs::create_dir_all(&models_root).context("create models dir")?;

    tracing::info!("downloading Parakeet model (~465MB, one time)…");
    let archive = models_root.join("parakeet-v3.tar.bz2");
    download(MODEL_URL, &archive).await.context("download model")?;

    tracing::info!("extracting model archive…");
    extract_tar_bz2(&archive, &models_root).context("extract model")?;
    let _ = std::fs::remove_file(&archive);

    if !is_present(&dir) {
        anyhow::bail!("model archive extracted but expected files missing in {}", dir.display());
    }
    tracing::info!("model ready at {}", dir.display());
    Ok(dir)
}

async fn download(url: &str, dest: &Path) -> Result<()> {
    let resp = reqwest::get(url).await?.error_for_status()?;
    let mut stream = resp.bytes_stream();
    let mut file = std::fs::File::create(dest)?;
    use std::io::Write;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
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
