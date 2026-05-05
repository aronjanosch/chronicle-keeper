"""Transcription service for sessions."""

from __future__ import annotations

from app.logging_config import get_logger
from app.services.sessions import get_session_path, load_session, save_session
from app.services.transcription import PROVIDERS, get_provider, resolve_transcription_provider
from app.services.transcription.formatting import save_transcription_result
from app.storage.artifacts import insert_artifact
from app.storage.config import get_transcription_config

log = get_logger("transcribe")


def transcribe_session(
    session_id: str,
    language: str | None = None,
    model: str | None = None,
    provider: str | None = None,
) -> dict:
    log.info("transcribe session=%s provider=%s model=%s lang=%s", session_id, provider, model, language)
    session = load_session(session_id)
    session_path = get_session_path(session_id)
    tracks = session.get("tracks") or []
    if not tracks:
        raise FileNotFoundError("No tracks found for session.")

    config = get_transcription_config()
    raw_lang = (language if language is not None else config.default_language) or "en"
    language = str(raw_lang).strip() or "en"
    provider_name = provider or resolve_transcription_provider(config.transcription_provider)
    provider_meta = PROVIDERS[provider_name]
    valid_ids = {m["id"] for m in provider_meta["models"]}
    fallback_model = provider_meta["default_model"]
    stored = (config.whisperx_model or "").strip()
    if model:
        model_name = model
    elif stored in valid_ids:
        model_name = stored
    else:
        model_name = fallback_model

    provider = get_provider(provider_name, model_name=model_name)
    result = provider.transcribe_session(
        session_path=session_path,
        tracks=tracks,
        speakers=session.get("speakers"),
        language=language,
    )

    provider_model = model_name
    segments, json_path, text_path = save_transcription_result(
        result=result,
        session_path=session_path,
        provider_model=provider_model,
    )

    insert_artifact(session_id, "transcript", result.provider, provider_model, text_path)

    session["transcription"] = {
        "language": result.language,
        "json_path": json_path,
        "text_path": text_path,
        "provider": result.provider,
        "model": provider_model,
        "segments": segments,
    }
    save_session(session_id, session)

    return {
        "language": result.language,
        "json_path": json_path,
        "text_path": text_path,
    }
