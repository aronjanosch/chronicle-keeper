use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::Connection;

use crate::paths::Paths;

/// Shared application state handed to every axum handler.
///
/// SQLite is single-writer; a `Mutex<Connection>` is the simplest correct
/// choice for a local single-user app and avoids a pool dependency. DB calls
/// are short, so brief lock contention is a non-issue here.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub paths: Paths,
}

impl AppState {
    pub fn new(paths: Paths) -> Result<Self> {
        let conn = crate::db::open(&paths.db_path())?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            paths,
        })
    }

    /// Run a closure with the locked DB connection.
    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        let conn = self.db.lock().expect("db mutex poisoned");
        f(&conn)
    }
}
