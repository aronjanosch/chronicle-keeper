"""SQLite database helpers for configuration storage."""

from __future__ import annotations

import os
from pathlib import Path
import sqlite3


DEFAULT_DB_FILENAME = "chronicle_keeper.db"

SCHEMA_STATEMENTS = [
    """
    CREATE TABLE IF NOT EXISTS config (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS campaigns (
        campaign_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        next_session_number INTEGER NOT NULL DEFAULT 1,
        system TEXT,
        gm TEXT,
        setting TEXT,
        default_language TEXT,
        players_json TEXT NOT NULL DEFAULT '[]',
        extra_info TEXT
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS sessions (
        session_id TEXT PRIMARY KEY,
        campaign_id TEXT,
        session_number INTEGER,
        title TEXT,
        date TEXT,
        metadata_json TEXT NOT NULL DEFAULT '{}',
        notes TEXT,
        FOREIGN KEY (campaign_id) REFERENCES campaigns(campaign_id)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS idx_sessions_campaign_id
    ON sessions(campaign_id)
    """,
    """
    CREATE TABLE IF NOT EXISTS artifacts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        session_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        provider TEXT NOT NULL,
        model TEXT NOT NULL,
        file_path TEXT NOT NULL,
        created_at TEXT NOT NULL,
        FOREIGN KEY (session_id) REFERENCES sessions(session_id)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS idx_artifacts_session
    ON artifacts(session_id, kind)
    """,
    """
    CREATE TABLE IF NOT EXISTS provider_keys (
        provider_id  TEXT PRIMARY KEY,
        api_key      TEXT NOT NULL DEFAULT '',
        api_base     TEXT NOT NULL DEFAULT '',
        default_model TEXT NOT NULL DEFAULT '',
        updated_at   TEXT NOT NULL DEFAULT ''
    )
    """,
]


def get_db_path() -> Path:
    """Return the database path for persistence."""
    default_path = Path.cwd() / DEFAULT_DB_FILENAME
    return Path(os.getenv("CK_DB_PATH", str(default_path)))


def get_connection() -> sqlite3.Connection:
    """Open a SQLite connection and initialize schema if needed."""
    db_path = get_db_path()
    db_path.parent.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(db_path)
    connection.row_factory = sqlite3.Row
    ensure_schema(connection)
    return connection


def ensure_schema(connection: sqlite3.Connection) -> None:
    """Ensure required tables exist."""
    for statement in SCHEMA_STATEMENTS:
        connection.execute(statement)
    connection.commit()

    # Migration: add default_model column if not present
    try:
        connection.execute(
            "ALTER TABLE provider_keys ADD COLUMN default_model TEXT NOT NULL DEFAULT ''"
        )
        connection.commit()
    except sqlite3.OperationalError:
        pass  # column already exists
