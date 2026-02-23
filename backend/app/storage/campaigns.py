"""SQLite-backed campaign and session metadata storage."""

from __future__ import annotations

import json
from typing import Any

from app.storage.config import get_config, update_config
from app.storage.db import get_connection


CAMPAIGN_FIELDS = {
    "campaign_id",
    "name",
    "next_session_number",
    "system",
    "gm",
    "setting",
    "default_language",
    "players",
    "extra_info",
}


def _normalize_players(players: Any) -> list[dict[str, str]]:
    if not players:
        return []
    if isinstance(players, list):
        raw = players
    elif isinstance(players, str):
        raw = [item.strip() for item in players.split(",")]
    else:
        return []
    normalized: list[dict[str, str]] = []
    for item in raw:
        if isinstance(item, dict):
            player_name = str(item.get("player_name", "")).strip()
            character_name = str(item.get("character_name", "")).strip()
        else:
            player_name = str(item).strip()
            character_name = ""
        if not player_name and not character_name:
            continue
        normalized.append(
            {
                "player_name": player_name,
                "character_name": character_name,
            }
        )
    return normalized


def _normalize_campaign(row: dict[str, Any], config: dict[str, Any]) -> dict[str, Any]:
    try:
        players = json.loads(row.get("players_json") or "[]")
    except json.JSONDecodeError:
        players = []
    return {
        "campaign_id": row.get("campaign_id", ""),
        "name": row.get("name", ""),
        "next_session_number": int(row.get("next_session_number", 1)),
        "system": row.get("system") or "",
        "gm": row.get("gm") or "",
        "setting": row.get("setting") or "",
        "default_language": row.get("default_language")
        or config.get("default_language", "en"),
        "players": _normalize_players(players),
        "extra_info": row.get("extra_info") or "",
    }


def get_current_campaign_id() -> str | None:
    config = get_config()
    return config.get("current_campaign_id") or None


def set_current_campaign_id(campaign_id: str) -> None:
    update_config({"current_campaign_id": campaign_id})


def get_campaigns() -> list[dict[str, Any]]:
    config = get_config()
    with get_connection() as connection:
        rows = connection.execute(
            "SELECT * FROM campaigns ORDER BY name"
        ).fetchall()
    return [_normalize_campaign(dict(row), config) for row in rows]


def get_campaign(campaign_id: str) -> dict[str, Any] | None:
    config = get_config()
    with get_connection() as connection:
        row = connection.execute(
            "SELECT * FROM campaigns WHERE campaign_id = ?",
            (campaign_id,),
        ).fetchone()
    if not row:
        return None
    return _normalize_campaign(dict(row), config)


def create_campaign(
    campaign_id: str, name: str, start_session_number: int = 1
) -> dict[str, Any]:
    config = get_config()
    existing = get_campaign(campaign_id)
    if existing:
        return existing
    with get_connection() as connection:
        connection.execute(
            """
            INSERT INTO campaigns (
                campaign_id,
                name,
                next_session_number,
                system,
                gm,
                setting,
                default_language,
                players_json,
                extra_info
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                campaign_id,
                name,
                int(start_session_number),
                "",
                "",
                "",
                config.get("default_language", "en"),
                "[]",
                "",
            ),
        )
        connection.commit()
    if not config.get("current_campaign_id"):
        set_current_campaign_id(campaign_id)
    return get_campaign(campaign_id) or {
        "campaign_id": campaign_id,
        "name": name,
        "next_session_number": int(start_session_number),
        "system": "",
        "gm": "",
        "setting": "",
        "default_language": config.get("default_language", "en"),
        "players": [],
        "extra_info": "",
    }


def update_campaign(campaign_id: str, updates: dict[str, Any]) -> dict[str, Any]:
    if not updates:
        campaign = get_campaign(campaign_id)
        if not campaign:
            raise KeyError(f"Campaign not found: {campaign_id}")
        return campaign

    fields: list[str] = []
    values: list[Any] = []
    for key, value in updates.items():
        if key not in CAMPAIGN_FIELDS or value is None:
            continue
        if key == "players":
            fields.append("players_json = ?")
            values.append(json.dumps(_normalize_players(value)))
        elif key == "next_session_number":
            fields.append("next_session_number = ?")
            values.append(int(value))
        else:
            fields.append(f"{key} = ?")
            values.append(value)

    if not fields:
        campaign = get_campaign(campaign_id)
        if not campaign:
            raise KeyError(f"Campaign not found: {campaign_id}")
        return campaign

    values.append(campaign_id)
    with get_connection() as connection:
        cursor = connection.execute(
            "SELECT campaign_id FROM campaigns WHERE campaign_id = ?",
            (campaign_id,),
        )
        if not cursor.fetchone():
            raise KeyError(f"Campaign not found: {campaign_id}")
        connection.execute(
            f"UPDATE campaigns SET {', '.join(fields)} WHERE campaign_id = ?",
            tuple(values),
        )
        connection.commit()
    return get_campaign(campaign_id) or {}


def get_next_session_number(campaign_id: str | None = None) -> int:
    target_id = campaign_id or get_current_campaign_id()
    if not target_id:
        return 1
    with get_connection() as connection:
        row = connection.execute(
            "SELECT next_session_number FROM campaigns WHERE campaign_id = ?",
            (target_id,),
        ).fetchone()
    if not row:
        return 1
    return int(row["next_session_number"])


def increment_session_number(campaign_id: str | None = None) -> int:
    target_id = campaign_id or get_current_campaign_id()
    if not target_id:
        return 1
    with get_connection() as connection:
        row = connection.execute(
            "SELECT next_session_number FROM campaigns WHERE campaign_id = ?",
            (target_id,),
        ).fetchone()
        if not row:
            return 1
        next_number = int(row["next_session_number"]) + 1
        connection.execute(
            "UPDATE campaigns SET next_session_number = ? WHERE campaign_id = ?",
            (next_number, target_id),
        )
        connection.commit()
    return next_number


METADATA_CATEGORIES = ("characters", "locations", "events", "items", "tags")


def _normalize_metadata(metadata: Any) -> dict[str, list[str]]:
    """Normalize metadata into {category: [str, ...]} format."""
    if not metadata:
        return {cat: [] for cat in METADATA_CATEGORIES}
    if isinstance(metadata, dict):
        result: dict[str, list[str]] = {}
        for cat in METADATA_CATEGORIES:
            raw = metadata.get(cat)
            if isinstance(raw, list):
                result[cat] = [str(v).strip() for v in raw if v and str(v).strip()]
            elif isinstance(raw, str):
                result[cat] = [v.strip() for v in raw.split(",") if v.strip()]
            else:
                result[cat] = []
        return result
    # Legacy: if it's a plain list, treat as tags
    if isinstance(metadata, list):
        normalized = {cat: [] for cat in METADATA_CATEGORIES}
        normalized["tags"] = [str(v).strip() for v in metadata if v and str(v).strip()]
        return normalized
    return {cat: [] for cat in METADATA_CATEGORIES}


def upsert_session_metadata(
    session_id: str,
    campaign_id: str | None,
    session_number: int | None,
    title: str | None,
    date: str | None,
    metadata: dict | list | None = None,
    notes: str | None = None,
) -> None:
    metadata_json = json.dumps(_normalize_metadata(metadata))
    with get_connection() as connection:
        connection.execute(
            """
            INSERT INTO sessions (
                session_id,
                campaign_id,
                session_number,
                title,
                date,
                metadata_json,
                notes
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET
                campaign_id=excluded.campaign_id,
                session_number=excluded.session_number,
                title=excluded.title,
                date=excluded.date,
                metadata_json=excluded.metadata_json,
                notes=excluded.notes
            """,
            (
                session_id,
                campaign_id,
                session_number,
                title,
                date,
                metadata_json,
                notes,
            ),
        )
        connection.commit()


def get_session_metadata(session_id: str) -> dict[str, Any] | None:
    with get_connection() as connection:
        row = connection.execute(
            "SELECT * FROM sessions WHERE session_id = ?",
            (session_id,),
        ).fetchone()
    if not row:
        return None
    row_dict = dict(row)
    # Support both old tags_json and new metadata_json columns
    raw_metadata: Any = None
    if "metadata_json" in row_dict:
        try:
            raw_metadata = json.loads(row_dict["metadata_json"] or "{}")
        except json.JSONDecodeError:
            raw_metadata = {}
    elif "tags_json" in row_dict:
        try:
            tags = json.loads(row_dict["tags_json"] or "[]")
        except json.JSONDecodeError:
            tags = []
        raw_metadata = tags  # will be normalized as legacy list
    return {
        "session_id": row_dict["session_id"],
        "campaign_id": row_dict["campaign_id"],
        "session_number": row_dict["session_number"],
        "title": row_dict["title"],
        "date": row_dict["date"],
        "metadata": _normalize_metadata(raw_metadata),
        "notes": row_dict["notes"] or "",
    }


def list_sessions_for_campaign(campaign_id: str) -> list[dict[str, Any]]:
    with get_connection() as connection:
        rows = connection.execute(
            """
            SELECT session_id, session_number, title, date, metadata_json, notes
            FROM sessions
            WHERE campaign_id = ?
            ORDER BY session_number DESC
            """,
            (campaign_id,),
        ).fetchall()
    sessions: list[dict[str, Any]] = []
    for row in rows:
        row_dict = dict(row)
        raw_metadata: Any = None
        if "metadata_json" in row_dict:
            try:
                raw_metadata = json.loads(row_dict["metadata_json"] or "{}")
            except json.JSONDecodeError:
                raw_metadata = {}
        elif "tags_json" in row_dict:
            try:
                raw_metadata = json.loads(row_dict["tags_json"] or "[]")
            except json.JSONDecodeError:
                raw_metadata = []
        sessions.append(
            {
                "session_id": row_dict["session_id"],
                "session_number": row_dict["session_number"],
                "title": row_dict["title"],
                "date": row_dict["date"],
                "metadata": _normalize_metadata(raw_metadata),
                "notes": row_dict["notes"] or "",
            }
        )
    return sessions
