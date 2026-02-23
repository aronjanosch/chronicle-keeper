"""Migrate sessions table from tags_json to metadata_json.

Run from the backend directory:
    uv run python -m scripts.migrate_metadata
"""

from __future__ import annotations

import json
import sqlite3
import sys
from pathlib import Path

# Add parent to path so we can import app modules
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from app.storage.db import get_connection


def migrate() -> None:
    conn = get_connection()
    cursor = conn.execute("PRAGMA table_info(sessions)")
    columns = {row[1] for row in cursor.fetchall()}

    if "metadata_json" in columns and "tags_json" not in columns:
        print("Already migrated — metadata_json exists, tags_json gone.")
        return

    if "tags_json" in columns and "metadata_json" not in columns:
        print("Adding metadata_json column...")
        conn.execute("ALTER TABLE sessions ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'")
        conn.commit()

        # Migrate data: convert tags list → metadata dict
        rows = conn.execute("SELECT session_id, tags_json FROM sessions").fetchall()
        for row in rows:
            session_id = row["session_id"]
            try:
                tags = json.loads(row["tags_json"] or "[]")
            except json.JSONDecodeError:
                tags = []
            if isinstance(tags, list):
                metadata = {
                    "characters": [],
                    "locations": [],
                    "events": [],
                    "items": [],
                    "tags": [str(t).strip() for t in tags if t and str(t).strip()],
                }
            else:
                metadata = tags  # already a dict somehow
            conn.execute(
                "UPDATE sessions SET metadata_json = ? WHERE session_id = ?",
                (json.dumps(metadata), session_id),
            )
        conn.commit()
        print(f"Migrated {len(rows)} session(s).")

        # Drop old column (SQLite doesn't support DROP COLUMN before 3.35)
        sqlite_version = tuple(int(x) for x in sqlite3.sqlite_version.split("."))
        if sqlite_version >= (3, 35, 0):
            print("Dropping old tags_json column...")
            conn.execute("ALTER TABLE sessions DROP COLUMN tags_json")
            conn.commit()
            print("Done.")
        else:
            print(
                f"SQLite {sqlite3.sqlite_version} doesn't support DROP COLUMN. "
                "tags_json column remains but is unused."
            )
    elif "tags_json" in columns and "metadata_json" in columns:
        print("Both columns exist — migration already partially done.")
        print("metadata_json is the active column; tags_json is unused.")
    else:
        print("No tags_json column found — nothing to migrate.")


if __name__ == "__main__":
    migrate()
