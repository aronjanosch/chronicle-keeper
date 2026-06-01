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
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    artifact_id TEXT NOT NULL DEFAULT '',
    session_id  TEXT NOT NULL,
    kind        TEXT NOT NULL,
    provider    TEXT NOT NULL,
    model       TEXT NOT NULL,
    file_path   TEXT NOT NULL DEFAULT '',
    content     TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
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

CREATE TABLE IF NOT EXISTS codex_entries (
    entry_id    TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    body        TEXT NOT NULL DEFAULT '',
    source      TEXT NOT NULL DEFAULT 'manual',
    updated_at  TEXT NOT NULL DEFAULT '',
    deleted     INTEGER NOT NULL DEFAULT 0,
    dirty       INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (campaign_id) REFERENCES campaigns(campaign_id)
);
CREATE INDEX IF NOT EXISTS idx_codex_entries_campaign ON codex_entries(campaign_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_codex_entries_dedup
    ON codex_entries(campaign_id, lower(name), kind) WHERE deleted = 0;
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
    migrate(&conn).context("migrate schema")?;
    Ok(conn)
}

/// In-memory database with the full schema applied — for unit tests.
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA).context("init schema")?;
    migrate(&conn).context("migrate schema")?;
    Ok(conn)
}

/// Bring an existing database up to the current schema. SQLite has no
/// `ADD COLUMN IF NOT EXISTS`, so each ALTER is best-effort: a "duplicate
/// column" error means the column is already there, which is fine.
fn migrate(conn: &Connection) -> Result<()> {
    // Sync columns (Sprint 2): last-write tracking + soft-delete propagation.
    let add_columns = [
        "ALTER TABLE campaigns ADD COLUMN updated_at TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE campaigns ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0",
        // `dirty` = local change not yet pushed to the sync server (clock-free
        // tracking — set on every write, cleared after a successful push).
        "ALTER TABLE campaigns ADD COLUMN dirty INTEGER NOT NULL DEFAULT 1",
        "ALTER TABLE sessions  ADD COLUMN updated_at TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE sessions  ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE sessions  ADD COLUMN dirty INTEGER NOT NULL DEFAULT 1",
        // Artifacts move from loose files to inline DB content (core principle #1).
        "ALTER TABLE artifacts ADD COLUMN artifact_id TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE artifacts ADD COLUMN content TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE artifacts ADD COLUMN dirty INTEGER NOT NULL DEFAULT 1",
        // Codex (Phase 1): a per-campaign freeform glossary of known names & lore,
        // injected verbatim into every summary's prompt context.
        "ALTER TABLE campaigns ADD COLUMN codex TEXT NOT NULL DEFAULT ''",
        // Codex (Phase 3): distilled longer write-up per entry, shown in the entry
        // inspector. Not fed to summaries — the one-line `body` still is.
        "ALTER TABLE codex_entries ADD COLUMN detail TEXT NOT NULL DEFAULT ''",
        // Freeform notes: a JSON array [{title, body}] of campaign-wide notes, all
        // injected verbatim into every summary. Supersedes the single `codex` string
        // (kept for back-compat; shown as the first note until the user saves).
        "ALTER TABLE campaigns ADD COLUMN codex_notes TEXT NOT NULL DEFAULT ''",
        // "Story so far" recap: an LLM-generated narrative rollup of every session
        // summary, regenerated on demand. Read-only for the GM. Synced as part of
        // the campaign row (last-write-wins, like every other campaign field) so
        // devices stay consistent. NOT fed back into per-session summaries (codex
        // covers that; a recursive recap would compound drift — see ROADMAP).
        "ALTER TABLE campaigns ADD COLUMN recap TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE campaigns ADD COLUMN recap_updated_at TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE campaigns ADD COLUMN gm_pronouns TEXT NOT NULL DEFAULT ''",
    ];
    for sql in add_columns {
        if let Err(e) = conn.execute(sql, []) {
            // Tolerate "duplicate column name"; surface anything else.
            if !e.to_string().contains("duplicate column name") {
                return Err(e).context(sql.to_string());
            }
        }
    }

    // Backfill: existing artifacts still point at loose files. Pull their text
    // into `content` so reads can drop the filesystem. Best-effort — a missing
    // file just leaves an empty artifact rather than aborting startup.
    let mut stmt =
        conn.prepare("SELECT id, file_path FROM artifacts WHERE content = '' AND file_path <> ''")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    for (id, path) in rows {
        if let Ok(text) = std::fs::read_to_string(&path) {
            conn.execute(
                "UPDATE artifacts SET content = ?1 WHERE id = ?2",
                rusqlite::params![text, id],
            )?;
        }
    }

    // Give existing artifacts a stable sync UUID if they lack one.
    let mut stmt = conn.prepare("SELECT id FROM artifacts WHERE artifact_id = ''")?;
    let ids: Vec<i64> = stmt
        .query_map([], |r| r.get::<_, i64>(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    for id in ids {
        let uuid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "UPDATE artifacts SET artifact_id = ?1 WHERE id = ?2",
            rusqlite::params![uuid, id],
        )?;
    }

    // Sync relies on `artifact_id` being unique (push-once / INSERT OR IGNORE).
    // Created after the backfill above so every row already has a UUID.
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_artifacts_artifact_id ON artifacts(artifact_id)",
        [],
    )?;

    // Tombstones for locally hard-deleted artifacts, so the deletion propagates
    // to other devices on the next sync. `dirty` = not yet pushed.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS deleted_artifacts (
            artifact_id TEXT PRIMARY KEY,
            dirty       INTEGER NOT NULL DEFAULT 1
        )",
        [],
    )?;

    // Codex (Phase 2): structured per-campaign glossary of NPCs/places/factions/items/lore.
    // Idempotent — `CREATE TABLE IF NOT EXISTS` covers fresh installs via SCHEMA; this
    // re-run is a no-op there and creates the table for older DBs.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS codex_entries (
            entry_id    TEXT PRIMARY KEY,
            campaign_id TEXT NOT NULL,
            name        TEXT NOT NULL,
            kind        TEXT NOT NULL,
            body        TEXT NOT NULL DEFAULT '',
            source      TEXT NOT NULL DEFAULT 'manual',
            updated_at  TEXT NOT NULL DEFAULT '',
            deleted     INTEGER NOT NULL DEFAULT 0,
            dirty       INTEGER NOT NULL DEFAULT 1,
            FOREIGN KEY (campaign_id) REFERENCES campaigns(campaign_id)
        )",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_codex_entries_campaign ON codex_entries(campaign_id)",
        [],
    )?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_codex_entries_dedup \
         ON codex_entries(campaign_id, lower(name), kind) WHERE deleted = 0",
        [],
    )?;

    Ok(())
}
