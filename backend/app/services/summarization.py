"""Summarization service for session transcripts."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
import json
from pathlib import Path
from typing import Any

import requests
from google import genai

from app.logging_config import get_logger
from app.prompts import build_metadata_prompt, build_summary_prompt
from app.services.sessions import get_session_path, load_session, save_session
from app.storage.artifacts import insert_artifact
from app.storage.campaigns import get_campaign
from app.storage.config import get_summarization_config

log = get_logger("summarization")


class SummarizationError(Exception):
    """Raised when summarization fails."""


@dataclass(frozen=True)
class SummarizeResult:
    summary: str
    provider: str
    model: str
    summary_path: str | None
    metadata: dict[str, Any] | None


_MAX_LOG_CHARS = 2000


def _truncate(text: str) -> str:
    if len(text) <= _MAX_LOG_CHARS:
        return text
    half = _MAX_LOG_CHARS // 2
    return f"{text[:half]}\n\n... ({len(text) - _MAX_LOG_CHARS} chars truncated) ...\n\n{text[-half:]}"


def _log_prompt(provider: str, prompt: str) -> None:
    log.debug("[%s] prompt (%d chars):\n%s", provider, len(prompt), _truncate(prompt))


def _log_response(provider: str, text: str) -> None:
    log.debug("[%s] response (%d chars):\n%s", provider, len(text), _truncate(text))


def _call_ollama(prompt: str, *, model: str, base_url: str, timeout: int) -> str:
    url = f"{base_url.rstrip('/')}/api/generate"
    log.info("Ollama request  model=%s url=%s", model, url)
    _log_prompt("ollama", prompt)
    response = requests.post(
        url,
        json={"model": model, "prompt": prompt, "stream": False},
        timeout=timeout,
    )
    response.raise_for_status()
    result = (response.json().get("response") or "").strip()
    _log_response("ollama", result)
    return result


def _call_gemini(prompt: str, *, api_key: str, model: str) -> str:
    if not api_key:
        raise SummarizationError("Gemini API key is required.")
    log.info("Gemini request  model=%s", model)
    _log_prompt("gemini", prompt)
    client = genai.Client(api_key=api_key)
    response = client.models.generate_content(model=model, contents=prompt)
    result = (response.text or "").strip()
    _log_response("gemini", result)
    return result


def _parse_metadata(raw_text: str) -> dict[str, Any] | None:
    try:
        return json.loads(raw_text)
    except json.JSONDecodeError:
        return None


def summarize_session(
    session_id: str,
    transcript_path: str | None = None,
    output_path: str | None = None,
    title: str | None = None,
    context: str | None = None,
    provider: str | None = None,
    model: str | None = None,
    base_url: str | None = None,
    system_prompt: str | None = None,
) -> SummarizeResult:
    """Summarize a session transcript and persist results."""
    log.info("summarize session=%s provider=%s model=%s", session_id, provider, model)
    session = load_session(session_id)
    config = get_summarization_config()

    transcript_path = transcript_path or session.get("transcription", {}).get("text_path")
    if not transcript_path:
        raise FileNotFoundError("Transcript not found for session.")

    transcript_text = Path(transcript_path).read_text(encoding="utf-8")
    language = config.default_language

    # Gather session & campaign metadata for the prompt
    session_context: dict[str, Any] = {}
    campaign_data = session.get("campaign") or {}
    if campaign_data.get("campaign_id"):
        campaign = get_campaign(campaign_data["campaign_id"])
        if campaign:
            session_context["campaign_name"] = campaign.get("name")
            session_context["system"] = campaign.get("system")
            session_context["setting"] = campaign.get("setting")
            session_context["gm"] = campaign.get("gm")
            session_context["extra_info"] = campaign.get("extra_info")
        session_context["campaign_name"] = session_context.get("campaign_name") or campaign_data.get("campaign_name")
    session_context["session_number"] = campaign_data.get("session_number")
    session_context["title"] = campaign_data.get("title") or title
    session_context["date"] = campaign_data.get("date")
    session_context["speakers"] = session.get("speakers") or []

    summary_prompt = build_summary_prompt(
        transcript_text,
        title=title,
        context=context,
        language=language,
        system_prompt=system_prompt,
        session_context=session_context if any(
            v for k, v in session_context.items() if k != "speakers" and v
        ) or session_context.get("speakers") else None,
    )

    provider = (provider or config.summary_provider).lower()
    if provider == "ollama":
        model_name = model or config.ollama_model
        base_url = base_url or config.ollama_base_url
        summary_text = _call_ollama(
            summary_prompt,
            model=model_name,
            base_url=base_url,
            timeout=config.ollama_timeout_seconds,
        )
        metadata_text = _call_ollama(
            build_metadata_prompt(summary_text, language=language),
            model=model_name,
            base_url=base_url,
            timeout=config.ollama_timeout_seconds,
        )
    elif provider == "gemini":
        model_name = model or config.gemini_model
        summary_text = _call_gemini(summary_prompt, api_key=config.gemini_api_key, model=model_name)
        metadata_text = _call_gemini(
            build_metadata_prompt(summary_text, language=language),
            api_key=config.gemini_api_key,
            model=model_name,
        )
    else:
        raise SummarizationError(f"Unknown provider: {provider}")

    metadata = _parse_metadata(metadata_text)

    session_path = get_session_path(session_id)
    if output_path:
        summary_path = Path(output_path)
    else:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        safe_provider = provider.replace("/", "_")
        safe_model = model_name.replace("/", "_")
        summary_dir = session_path / "summaries" / f"{safe_provider}_{safe_model}_{timestamp}"
        summary_dir.mkdir(parents=True, exist_ok=True)
        summary_path = summary_dir / "summary.md"

    summary_path.write_text(summary_text, encoding="utf-8")

    insert_artifact(session_id, "summary", provider, model_name, str(summary_path))

    session["summary"] = {
        "summary_path": str(summary_path),
        "provider": provider,
        "model": model_name,
    }
    # Merge LLM-extracted metadata into existing metadata (don't overwrite user edits)
    existing_metadata = session.get("metadata") or {}
    if metadata:
        for key, values in metadata.items():
            if not isinstance(values, list):
                continue
            existing_values = existing_metadata.get(key, [])
            if not isinstance(existing_values, list):
                existing_values = []
            merged = list(existing_values)
            for v in values:
                if v and v not in merged:
                    merged.append(v)
            existing_metadata[key] = merged
    session["metadata"] = existing_metadata
    save_session(session_id, session)

    return SummarizeResult(
        summary=summary_text,
        provider=provider,
        model=model_name,
        summary_path=str(summary_path),
        metadata=metadata,
    )
