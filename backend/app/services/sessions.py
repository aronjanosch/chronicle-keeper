"""Session folder utilities."""

from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Any
from uuid import uuid4

from app.storage.campaigns import (
    get_campaign,
    get_session_metadata,
    list_sessions_for_campaign,
    upsert_session_metadata,
    update_campaign,
)
from app.storage.artifacts import (
    delete_artifact,
    delete_artifacts_for_session,
    get_artifact,
    list_artifacts,
)
from app.storage.config import get_config

STAGING_DIR_NAME = "_sessions"


class OutputRootError(Exception):
    """Configured output_root cannot be created or is not writable on this machine."""


def _suggested_output_root() -> Path:
    return Path.home() / "Documents" / "chronicle-keeper"


def _mkdir_output_root(path: Path) -> Path:
    """Create output root (and parents). Raise OutputRootError on failure."""
    expanded = path.expanduser()
    try:
        expanded.mkdir(parents=True, exist_ok=True)
    except OSError as exc:
        suggested = _suggested_output_root()
        raise OutputRootError(
            f"Cannot create session data folder {expanded}: {exc}. "
            f"Update Settings → session data folder (or CK_OUTPUT_ROOT) to a path on this computer "
            f"(for example: {suggested})."
        ) from exc
    return expanded


def validate_output_root(path_str: str) -> None:
    """Raise OutputRootError if the given path cannot be used as output_root."""
    if not (path_str or "").strip():
        raise OutputRootError("Session data folder path cannot be empty.")
    _mkdir_output_root(Path(path_str))


def get_output_root() -> Path:
    """Return output root folder."""
    config = get_config()
    return Path(config["output_root"]).expanduser()


def ensure_output_root() -> Path:
    """Ensure output root exists."""
    return _mkdir_output_root(get_output_root())


def get_staging_root() -> Path:
    """Return staging root for new sessions."""
    output_root = ensure_output_root()
    staging_root = output_root / STAGING_DIR_NAME
    staging_root.mkdir(parents=True, exist_ok=True)
    return staging_root


def _sanitize_folder_name(name: str) -> str:
    return name.replace("/", "_").replace("\\", "_").replace(":", "_").replace(" ", "_")


def create_session(session_id: str | None = None) -> dict[str, Any]:
    """Create a new session folder and session.json."""
    output_root = ensure_output_root()
    session_id = session_id or str(uuid4())
    session_path = get_staging_root() / session_id
    session_path.mkdir(parents=True, exist_ok=True)

    session_data = {
        "session_id": session_id,
        "session_path": str(session_path),
        "tracks": [],
        "speakers": [],
        "transcription": {},
        "summary": {},
        "metadata": {},
    }
    _write_session_file(session_path, session_data)
    return session_data


def create_campaign_session(
    campaign_id: str,
    session_number: int | None = None,
    title: str | None = None,
    date: str | None = None,
) -> dict[str, Any]:
    """Create a session under a campaign folder."""
    campaign = get_campaign(campaign_id)
    if not campaign:
        raise FileNotFoundError(f"Campaign not found: {campaign_id}")

    current_next = int(campaign.get("next_session_number", 1))
    campaign_name = campaign.get("name") or campaign_id
    safe_campaign = _sanitize_folder_name(campaign_name)

    def _session_number_in_use(number: int) -> bool:
        if any(
            item.get("session_number") == number
            for item in list_sessions_for_campaign(campaign_id)
        ):
            return True
        target_path = ensure_output_root() / safe_campaign / str(number)
        return target_path.exists()

    if session_number is None:
        session_number = current_next
        while _session_number_in_use(session_number):
            session_number += 1
    elif _session_number_in_use(session_number):
        raise FileExistsError(
            f"Session number already exists for campaign {campaign_id}: {session_number}"
        )

    session = create_session()
    set_campaign_metadata(
        session_id=session["session_id"],
        campaign_id=campaign_id,
        session_number=session_number,
        title=title,
        date=date,
    )
    if session_number >= current_next:
        update_campaign(
            campaign_id,
            {"next_session_number": int(session_number) + 1},
        )
    return load_session(session["session_id"])


def _session_file_path(session_path: Path) -> Path:
    return session_path / "session.json"


def _write_session_file(session_path: Path, data: dict[str, Any]) -> None:
    session_file = _session_file_path(session_path)
    session_file.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")


def load_session(session_id: str) -> dict[str, Any]:
    """Load session data from session.json."""
    session_path = get_session_path(session_id)
    session_file = _session_file_path(session_path)
    if not session_file.exists():
        raise FileNotFoundError(f"Session not found: {session_id}")
    data = json.loads(session_file.read_text(encoding="utf-8"))
    _sync_session_metadata(session_id, data)
    return data


def save_session(session_id: str, data: dict[str, Any]) -> None:
    """Persist session data."""
    session_path = get_session_path(session_id)
    _write_session_file(session_path, data)


def update_session(session_id: str, updates: dict[str, Any]) -> dict[str, Any]:
    """Update session data and return it."""
    data = load_session(session_id)
    data.update(updates)
    save_session(session_id, data)
    return data


def set_campaign_metadata(
    session_id: str,
    campaign_id: str | None = None,
    session_number: int | None = None,
    title: str | None = None,
    date: str | None = None,
    metadata: dict | None = None,
    notes: str | None = None,
) -> dict[str, Any]:
    """Set campaign/session metadata for a session."""
    data = load_session(session_id)
    campaign_name = None
    if campaign_id:
        campaign = get_campaign(campaign_id)
        if campaign:
            campaign_name = campaign.get("name")

    data["campaign"] = {
        "campaign_id": campaign_id,
        "campaign_name": campaign_name,
        "session_number": session_number,
        "title": title,
        "date": date,
        "notes": notes or "",
    }
    data["metadata"] = metadata or data.get("metadata") or {}
    upsert_session_metadata(
        session_id=session_id,
        campaign_id=campaign_id,
        session_number=session_number,
        title=title,
        date=date,
        metadata=metadata,
        notes=notes,
    )
    data = _relocate_session_folder(session_id, data)
    return {**data["campaign"], "metadata": data.get("metadata", {})}


def get_session_path(session_id: str) -> Path:
    """Get session path from output root."""
    session_path = _find_session_path(session_id)
    if not session_path:
        raise FileNotFoundError(f"Session not found: {session_id}")
    return session_path


def _find_session_path(session_id: str) -> Path | None:
    staging_path = get_staging_root() / session_id
    session_file = _session_file_path(staging_path)
    if session_file.exists():
        try:
            data = json.loads(session_file.read_text(encoding="utf-8"))
            if data.get("session_id") == session_id:
                return staging_path
        except json.JSONDecodeError:
            pass

    output_root = ensure_output_root()
    for candidate in output_root.rglob("session.json"):
        try:
            data = json.loads(candidate.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        if data.get("session_id") == session_id:
            return candidate.parent
    return None


def _rewrite_session_paths(
    data: dict[str, Any], old_path: Path, new_path: Path
) -> dict[str, Any]:
    def _rewrite(value: str | None) -> str | None:
        if not value:
            return value
        path_value = Path(value)
        if path_value.is_relative_to(old_path):
            return str(new_path / path_value.relative_to(old_path))
        return value

    for track in data.get("tracks", []) or []:
        track["file_path"] = _rewrite(track.get("file_path"))

    transcription = data.get("transcription") or {}
    transcription["json_path"] = _rewrite(transcription.get("json_path"))
    transcription["text_path"] = _rewrite(transcription.get("text_path"))
    data["transcription"] = transcription

    summary = data.get("summary") or {}
    summary["summary_path"] = _rewrite(summary.get("summary_path"))
    data["summary"] = summary

    data["session_path"] = str(new_path)
    return data


def _update_transcription_metadata(data: dict[str, Any], new_path: Path) -> None:
    transcription = data.get("transcription") or {}
    json_path = transcription.get("json_path")
    if not json_path:
        return
    json_file = Path(json_path)
    if not json_file.exists():
        return
    try:
        payload = json.loads(json_file.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return
    metadata = payload.get("metadata") or {}
    metadata["session_path"] = str(new_path)
    payload["metadata"] = metadata
    json_file.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")


def _relocate_session_folder(session_id: str, data: dict[str, Any]) -> dict[str, Any]:
    campaign = data.get("campaign") or {}
    campaign_id = campaign.get("campaign_id")
    session_number = campaign.get("session_number")
    if not campaign_id or not session_number:
        save_session(session_id, data)
        return data

    campaign_name = campaign.get("campaign_name") or campaign_id
    safe_campaign = _sanitize_folder_name(campaign_name)
    output_root = ensure_output_root()
    target_path = output_root / safe_campaign / str(session_number)

    current_path = get_session_path(session_id)
    if current_path == target_path:
        data["session_path"] = str(target_path)
        save_session(session_id, data)
        return data

    if target_path.exists():
        raise FileExistsError(
            f"Target session folder already exists: {target_path}"
        )

    target_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(current_path), str(target_path))

    data = _rewrite_session_paths(data, current_path, target_path)
    _write_session_file(target_path, data)
    _update_transcription_metadata(data, target_path)
    return data


def list_sessions() -> list[dict[str, Any]]:
    """List all sessions with basic status info."""
    sessions = []
    output_root = ensure_output_root()
    for session_file in output_root.rglob("session.json"):
        if session_file.parent.name == "transcriptions":
            continue
        try:
            data = json.loads(session_file.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        sid = data.get("session_id", session_file.parent.name)
        transcripts = list_artifacts(sid, "transcript")
        summaries = list_artifacts(sid, "summary")
        sessions.append(
            {
                "session_id": sid,
                "session_path": str(session_file.parent),
                "has_transcription": bool(transcripts),
                "has_summary": bool(summaries),
                "transcript_path": transcripts[0]["file_path"] if transcripts else None,
                "summary_path": summaries[0]["file_path"] if summaries else None,
            }
        )
    sessions.sort(key=lambda item: item["session_id"], reverse=True)
    return sessions


def list_campaign_sessions(campaign_id: str) -> list[dict[str, Any]]:
    """List sessions for a campaign."""
    db_sessions = list_sessions_for_campaign(campaign_id)
    if db_sessions:
        sessions: list[dict[str, Any]] = []
        for item in db_sessions:
            session_id = item.get("session_id")
            transcripts = list_artifacts(session_id, "transcript")
            summaries = list_artifacts(session_id, "summary")
            sessions.append(
                {
                    "session_id": session_id,
                    "session_number": item.get("session_number"),
                    "title": item.get("title"),
                    "date": item.get("date"),
                    "metadata": item.get("metadata") or {},
                    "has_transcription": bool(transcripts),
                    "has_summary": bool(summaries),
                }
            )
        return sessions

    sessions = []
    output_root = ensure_output_root()
    for session_file in output_root.rglob("session.json"):
        if session_file.parent.name == "transcriptions":
            continue
        try:
            data = json.loads(session_file.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        campaign = data.get("campaign") or {}
        if campaign.get("campaign_id") != campaign_id:
            continue
        sid = data.get("session_id", session_file.parent.name)
        transcripts = list_artifacts(sid, "transcript")
        summaries = list_artifacts(sid, "summary")
        sessions.append(
            {
                "session_id": sid,
                "session_number": campaign.get("session_number"),
                "title": campaign.get("title"),
                "date": campaign.get("date"),
                "has_transcription": bool(transcripts),
                "has_summary": bool(summaries),
            }
        )
    sessions.sort(key=lambda item: item.get("session_number") or 0, reverse=True)
    return sessions


def get_campaign_metadata(session_id: str) -> dict[str, Any]:
    """Return campaign metadata from the database or session file."""
    db_metadata = get_session_metadata(session_id)
    if db_metadata:
        campaign_name = None
        if db_metadata.get("campaign_id"):
            campaign = get_campaign(db_metadata["campaign_id"])
            if campaign:
                campaign_name = campaign.get("name")
        return {
            "campaign_id": db_metadata.get("campaign_id"),
            "campaign_name": campaign_name,
            "session_number": db_metadata.get("session_number"),
            "title": db_metadata.get("title"),
            "date": db_metadata.get("date"),
            "metadata": db_metadata.get("metadata") or {},
            "notes": db_metadata.get("notes") or "",
        }
    session = load_session(session_id)
    result = dict(session.get("campaign", {}))
    result["metadata"] = session.get("metadata", {})
    return result


def _sync_session_metadata(session_id: str, data: dict[str, Any]) -> None:
    campaign = data.get("campaign") or {}
    if not campaign:
        return
    upsert_session_metadata(
        session_id=session_id,
        campaign_id=campaign.get("campaign_id"),
        session_number=campaign.get("session_number"),
        title=campaign.get("title"),
        date=campaign.get("date"),
        metadata=data.get("metadata"),
        notes=campaign.get("notes"),
    )


def delete_transcript(session_id: str, artifact_id: int) -> None:
    """Delete a transcript artifact and its files."""
    artifact = get_artifact(artifact_id)
    if not artifact or artifact["session_id"] != session_id:
        raise FileNotFoundError(f"Transcript artifact not found: {artifact_id}")

    file_path = Path(artifact["file_path"])
    transcript_dir = file_path.parent
    if transcript_dir.exists():
        shutil.rmtree(transcript_dir)

    delete_artifact(artifact_id)

    # If the session's active transcription pointed into this folder, clear it
    data = load_session(session_id)
    transcription = data.get("transcription") or {}
    text_path = transcription.get("text_path") or ""
    if text_path and Path(text_path).is_relative_to(transcript_dir):
        data["transcription"] = {}
        save_session(session_id, data)


def delete_summary(session_id: str, artifact_id: int) -> None:
    """Delete a summary artifact and its file."""
    artifact = get_artifact(artifact_id)
    if not artifact or artifact["session_id"] != session_id:
        raise FileNotFoundError(f"Summary artifact not found: {artifact_id}")

    file_path = Path(artifact["file_path"])
    summary_dir = file_path.parent
    if summary_dir.exists():
        shutil.rmtree(summary_dir)

    delete_artifact(artifact_id)


def read_artifact_content(session_id: str, artifact_id: int) -> str:
    """Read and return the text content of an artifact."""
    artifact = get_artifact(artifact_id)
    if not artifact or artifact["session_id"] != session_id:
        raise FileNotFoundError(f"Artifact not found: {artifact_id}")
    file_path = Path(artifact["file_path"])
    if not file_path.exists():
        raise FileNotFoundError(f"Artifact file not found: {file_path}")
    return file_path.read_text(encoding="utf-8")


def delete_session(session_id: str) -> None:
    """Delete a session folder, data, and artifact rows."""
    session_path = get_session_path(session_id)
    delete_artifacts_for_session(session_id)
    shutil.rmtree(session_path)
