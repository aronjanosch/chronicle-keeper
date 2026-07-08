use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::{ProjectDirs, UserDirs};

/// Resolved on-disk locations for the core.
#[derive(Clone, Debug)]
pub struct Paths {
    /// App-private data dir (holds the SQLite DB and downloaded models).
    pub data_dir: PathBuf,
}

impl Paths {
    /// Resolve from the platform app-data dir, overridable with `CK_DATA_DIR`
    /// (used in dev and tests).
    pub fn resolve() -> Result<Self> {
        let data_dir = match std::env::var_os("CK_DATA_DIR") {
            Some(v) => PathBuf::from(v),
            None => ProjectDirs::from("com", "aron", "chronicle-keeper")
                .context("could not determine app data dir")?
                .data_dir()
                .to_path_buf(),
        };
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("create data dir {}", data_dir.display()))?;
        Ok(Self { data_dir })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("chronicle_keeper_v1.db")
    }

    pub fn models_dir(&self) -> PathBuf {
        match std::env::var_os("CK_MODELS_DIR") {
            Some(v) => PathBuf::from(v),
            None => self.data_dir.join("models"),
        }
    }
}

/// Move a file or folder to the OS trash. In unit tests this hard-deletes
/// instead, so `cargo test` never fills the user's real trash.
pub fn move_to_trash(path: &Path) -> Result<()> {
    #[cfg(test)]
    {
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
    #[cfg(not(test))]
    {
        trash::delete(path).map_err(|e| anyhow::anyhow!(e))
    }
}

/// Default `output_root` — a plain, findable folder in the user's Documents:
/// `~/Documents/Chronicle Keeper`. Each campaign gets a subfolder, each session
/// a numbered folder inside it, so the recordings are browsable like any docs.
pub fn default_output_root() -> PathBuf {
    const FOLDER: &str = "Chronicle Keeper";
    if let Some(dirs) = UserDirs::new() {
        if let Some(docs) = dirs.document_dir() {
            return docs.join(FOLDER);
        }
        return dirs.home_dir().join("Documents").join(FOLDER);
    }
    PathBuf::from(FOLDER)
}
