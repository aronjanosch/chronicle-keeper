"""Export helpers for session summaries."""

from __future__ import annotations

from pathlib import Path

from app.services.sessions import get_session_path, load_session


def _sanitize_filename(name: str) -> str:
    return name.replace("/", "_").replace("\\", "_").replace(":", "_")


def _format_frontmatter(metadata: dict) -> str:
    lines = ["---"]
    for key, value in metadata.items():
        if value is None or value == "":
            continue
        if isinstance(value, list):
            lines.append(f"{key}:")
            for item in value:
                lines.append(f"  - {item}")
        else:
            lines.append(f"{key}: {value}")
    lines.append("---")
    return "\n".join(lines)


def export_session(
    session_id: str,
    use_obsidian_format: bool = True,
    custom_filename: str | None = None,
) -> dict:
    session = load_session(session_id)
    session_path = get_session_path(session_id)

    summary_path = session.get("summary", {}).get("summary_path")
    summary_text = ""
    if summary_path:
        summary_text = Path(summary_path).read_text(encoding="utf-8")

    if not summary_text:
        summary_text = session.get("summary", {}).get("summary", "")
    if not summary_text:
        raise ValueError("No summary available for export.")

    campaign_info = session.get("campaign") or {}
    extracted_metadata = session.get("metadata") or {}

    if use_obsidian_format:
        frontmatter = _format_frontmatter(
            {
                "campaign": campaign_info.get("campaign_id"),
                "session_number": campaign_info.get("session_number"),
                "session_title": campaign_info.get("title"),
                "session_date": campaign_info.get("date"),
                "characters": extracted_metadata.get("characters"),
                "locations": extracted_metadata.get("locations"),
                "items": extracted_metadata.get("items"),
                "tags": extracted_metadata.get("tags"),
            }
        )
        content = f"{frontmatter}\n\n{summary_text.strip()}\n"
    else:
        content = f"{summary_text.strip()}\n"

    if custom_filename:
        filename = _sanitize_filename(custom_filename)
    else:
        campaign_id = campaign_info.get("campaign_id")
        session_number = campaign_info.get("session_number")
        if campaign_id and session_number:
            filename = f"{campaign_id}_session_{session_number}.md"
        else:
            filename = "session_notes.md"

    export_dir = session_path / "exports"
    export_dir.mkdir(parents=True, exist_ok=True)
    export_path = export_dir / filename
    export_path.write_text(content, encoding="utf-8")

    return {
        "content": content,
        "filename": filename,
        "use_obsidian_format": use_obsidian_format,
        "export_path": str(export_path),
    }
