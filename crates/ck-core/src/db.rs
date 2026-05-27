use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS campaigns (
    campaign_id         TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    next_session_number INTEGER NOT NULL DEFAULT 1,
    system              TEXT,
    gm                  TEXT,
    setting             TEXT,
    default_language    TEXT,
    players_json        TEXT NOT NULL DEFAULT '[]',
    extra_info          TEXT
);

CREATE TABLE IF NOT EXISTS sessions (
    session_id     TEXT PRIMARY KEY,
    campaign_id    TEXT,
    session_number INTEGER,
    title          TEXT,
    date           TEXT,
    metadata_json  TEXT NOT NULL DEFAULT '{}',
    notes          TEXT,
    session_path   TEXT NOT NULL DEFAULT '',
    tracks_json    TEXT NOT NULL DEFAULT '[]',
    speakers_json  TEXT NOT NULL DEFAULT '[]',
    FOREIGN KEY (campaign_id) REFERENCES campaigns(campaign_id)
);
CREATE INDEX IF NOT EXISTS idx_sessions_campaign_id ON sessions(campaign_id);

CREATE TABLE IF NOT EXISTS artifacts (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    kind       TEXT NOT NULL,
    provider   TEXT NOT NULL,
    model      TEXT NOT NULL,
    file_path  TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(session_id)
);
CREATE INDEX IF NOT EXISTS idx_artifacts_session ON artifacts(session_id, kind);

CREATE TABLE IF NOT EXISTS provider_keys (
    provider_id   TEXT PRIMARY KEY,
    api_key       TEXT NOT NULL DEFAULT '',
    api_base      TEXT NOT NULL DEFAULT '',
    default_model TEXT NOT NULL DEFAULT '',
    updated_at    TEXT NOT NULL DEFAULT ''
);
";

/// Open the database and ensure the schema exists.
///
/// Storage is simplified vs. the Python backend: the `sessions` table is the
/// source of truth (tracks/speakers/metadata live in columns here), so there
/// is no scattered `session.json` discovery or campaign-folder relocation.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).with_context(|| format!("open db {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA).context("init schema")?;
    Ok(conn)
}
