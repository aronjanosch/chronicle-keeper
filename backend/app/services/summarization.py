"""Summarization service for session transcripts."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
import json
from pathlib import Path
from typing import Any

import litellm

from app.logging_config import get_logger
from app.prompts import build_metadata_prompt, build_summary_prompt
from app.services.llm_providers import REGISTRY_BY_ID, REGISTRY_IDS, get_provider_key
from app.services.sessions import get_session_path, load_session, save_session
from app.storage.artifacts import insert_artifact
from app.storage.campaigns import get_campaign
from app.storage.config import SummarizationConfig, get_summarization_config

litellm.suppress_debug_info = True
if hasattr(litellm, "turn_off_message_logging"):
    litellm.turn_off_message_logging = True

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


def _coerce_litellm_model_id(raw: str) -> str:
    """Add provider prefix for bare Google model ids; pass through routes that already include a slash."""
    m = raw.strip()
    if not m:
        return "gemini/gemini-2.5-flash"
    if "/" in m:
        return m
    if m.startswith("gemini") or m.startswith("models/"):
        return f"gemini/{m.removeprefix('models/')}"
    return m


def _resolve_litellm(
    config: SummarizationConfig,
    *,
    model_override: str | None,
    base_url_override: str | None,
) -> tuple[str, str, str | None, int]:
    """Return (litellm_model, api_key, api_base_or_none, timeout_seconds)."""
    timeout = config.litellm_timeout_seconds
    base = (base_url_override or config.litellm_api_base or "").strip() or None
    raw = (model_override or config.litellm_model or "").strip()
    litellm_model = _coerce_litellm_model_id(raw)
    api_key = (config.litellm_api_key or "").strip()
    return litellm_model, api_key, base, timeout


def _extract_completion_text(response: Any) -> str:
    if not response or not getattr(response, "choices", None):
        return ""
    msg = response.choices[0].message
    content = getattr(msg, "content", None)
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        parts: list[str] = []
        for block in content:
            if isinstance(block, dict):
                t = block.get("text")
                if t:
                    parts.append(str(t))
            else:
                t = getattr(block, "text", None)
                if t:
                    parts.append(str(t))
        return "".join(parts).strip()
    return (str(content) if content else "").strip()


def _litellm_error_message(exc: Exception) -> str:
    msg = str(exc).strip()
    if msg:
        return f"Cloud LLM request failed: {msg}"
    return "Cloud LLM request failed."


def _call_litellm(
    prompt: str,
    *,
    model: str,
    api_key: str,
    api_base: str | None,
    timeout: int,
    log_label: str,
) -> str:
    log.info("%s request  model=%s api_base=%s", log_label, model, api_base or "(default)")
    _log_prompt(log_label, prompt)
    kwargs: dict[str, Any] = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "timeout": float(timeout),
    }
    if api_key:
        kwargs["api_key"] = api_key
    if api_base:
        kwargs["api_base"] = api_base
    try:
        response = litellm.completion(**kwargs)
    except Exception as exc:
        log.exception("LiteLLM request failed")
        raise SummarizationError(_litellm_error_message(exc)) from exc
    text = _extract_completion_text(response)
    _log_response(log_label, text)
    return text


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
        session_context=session_context
        if any(v for k, v in session_context.items() if k != "speakers" and v) or session_context.get("speakers")
        else None,
    )

    provider = (provider or config.summary_provider).lower()
    if provider in REGISTRY_IDS:
        reg = REGISTRY_BY_ID[provider]
        saved = get_provider_key(provider) or {}
        api_key = saved.get("api_key", "")
        if not api_key and reg["needs_key"]:
            raise SummarizationError(
                f"No API key saved for {reg['name']}. Add it in Settings → LLM providers."
            )
        resolved_base = base_url or saved.get("api_base") or reg.get("default_api_base")
        model_id = model or saved.get("default_model") or reg["default_model"]
        litellm_model = f"{reg['litellm_prefix']}/{model_id}"
        summary_text = _call_litellm(
            summary_prompt,
            model=litellm_model,
            api_key=api_key,
            api_base=resolved_base,
            timeout=config.litellm_timeout_seconds,
            log_label=provider,
        )
        metadata_text = _call_litellm(
            build_metadata_prompt(summary_text, language=language),
            model=litellm_model,
            api_key=api_key,
            api_base=resolved_base,
            timeout=config.litellm_timeout_seconds,
            log_label=provider,
        )
        model_name = litellm_model
    elif provider == "litellm":
        lm_model, api_key, api_base, lm_timeout = _resolve_litellm(
            config, model_override=model, base_url_override=base_url
        )
        if not api_key:
            raise SummarizationError(
                "API key is required for custom LiteLLM. Set it in Settings → LLM providers."
            )
        summary_text = _call_litellm(
            summary_prompt,
            model=lm_model,
            api_key=api_key,
            api_base=api_base,
            timeout=lm_timeout,
            log_label="litellm",
        )
        metadata_text = _call_litellm(
            build_metadata_prompt(summary_text, language=language),
            model=lm_model,
            api_key=api_key,
            api_base=api_base,
            timeout=lm_timeout,
            log_label="litellm",
        )
        model_name = lm_model
    else:
        raise SummarizationError(f"Unknown provider: {provider!r}")

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
