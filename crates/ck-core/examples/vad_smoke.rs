//! Manual smoke test for VAD-driven transcription on real Craig FLAC tracks.
//! Not a CI test (needs the downloaded model + sample audio).
//!
//! Run from repo root:
//!   cargo run --example vad_smoke -p ck-core --features transcription -- \
//!     example-recordings/craig-*.flac/1-aronjanosch.flac
//!
//! With no args it tries both tracks in the bundled example recording.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use ck_core::paths::Paths;
use ck_core::state::ModelProgress;
use ck_core::transcription::{model, transcribe_tracks};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let files: Vec<PathBuf> = if args.is_empty() {
        let base = "example-recordings/craig-yNq4gbpXrgTL-qFkw6S-QOHiQEiiRKNdeRWF35Tsaqx.flac";
        vec![
            PathBuf::from(format!("{base}/1-aronjanosch.flac")),
            PathBuf::from(format!("{base}/2-elestea.flac")),
        ]
    } else {
        args.iter().map(PathBuf::from).collect()
    };

    let paths = Paths::resolve()?;
    let model_dir = model::model_dir(&paths);
    anyhow::ensure!(
        model::is_present(&model_dir),
        "Parakeet model not found at {} — run the app once to download it",
        model_dir.display()
    );

    let vad = model::ensure_vad(&paths).await;
    println!(
        "VAD model: {}",
        vad.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "NONE (fixed-window fallback)".into())
    );

    let tracks: Vec<(String, PathBuf, String)> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (format!("t{i}"), f.clone(), format!("Speaker{i}")))
        .collect();

    let cancel = AtomicBool::new(false);
    let progress = Arc::new(Mutex::new(ModelProgress::default()));
    let t0 = std::time::Instant::now();
    let segments = transcribe_tracks(
        &model_dir,
        "cpu",
        vad.as_deref(),
        &tracks,
        &cancel,
        &progress,
    )?;
    let elapsed = t0.elapsed();

    println!(
        "\n== {} segments in {:.1}s ==",
        segments.len(),
        elapsed.as_secs_f64()
    );
    for s in &segments {
        println!(
            "[{:>7.2}–{:>7.2}] {}: {}",
            s.start,
            s.end,
            s.speaker.as_deref().unwrap_or("?"),
            s.text.chars().take(90).collect::<String>()
        );
    }
    Ok(())
}
