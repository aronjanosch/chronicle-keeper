"""Chronicle Keeper API - simplified backend."""

from __future__ import annotations

import tempfile
from pathlib import Path

import fastapi
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse

from app.logging_config import get_logger, setup_logging

setup_logging()
log = get_logger("api")

from app.models import (
    ArtifactInfo,
    CampaignDetail,
    CampaignInfo,
    CampaignSessionInfo,
    CampaignUpdateRequest,
    CampaignsResponse,
    ConfigResponse,
    CreateCampaignRequest,
    CreateCampaignSessionRequest,
    EstimateTokensRequest,
    EstimateTokensResponse,
    ExportRequest,
    ExportResponse,
    LabelSpeakersRequest,
    LabelSpeakersResponse,
    NextSessionNumberResponse,
    ProviderInfo,
    ProviderKeyUpdate,
    ProviderTestRequest,
    ProviderTestResult,
    SessionInfo,
    SessionMetadataRequest,
    SummarizeRequest,
    SummarizeResponse,
    TranscribeRequest,
    TranscribeResponse,
    UpdateConfigRequest,
    UploadResponse,
)
from app.services.llm_providers import (
    PROVIDER_REGISTRY,
    PROVIDER_REGISTRY_BY_ID,
    get_provider_key,
    list_provider_keys,
    upsert_provider_key,
)
from app.services.campaigns import (
    create_campaign,
    get_campaign_detail,
    list_campaigns,
    next_session_number,
    update_campaign,
)
from app.services.context_window import ContextWindowAnalyzer
from app.services.export import export_session
from app.services.sessions import (
    OutputRootError,
    create_campaign_session,
    delete_session,
    delete_summary,
    delete_transcript,
    get_campaign_metadata,
    load_session,
    list_campaign_sessions,
    list_sessions,
    read_artifact_content,
    save_session,
    set_campaign_metadata,
    validate_output_root,
)
from app.storage.artifacts import list_artifacts
from app.services.summarization import SummarizationError, summarize_session
from app.services.transcribe import transcribe_session
from app.services.transcription import get_available_providers, resolve_transcription_provider
from app.services.upload import extract_craig_zip
from app.storage.campaigns import increment_session_number
from app.storage.config import get_config, update_config


app = fastapi.FastAPI(
    title="Chronicle Keeper API",
    description="Simplified backend for D&D session processing",
    version="0.1.0",
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.exception_handler(Exception)
async def _unhandled_error(_request: fastapi.Request, exc: Exception):
    """Log full traceback for any unhandled exception, then return 500."""
    if isinstance(exc, OutputRootError):
        log.warning("Invalid output_root: %s", exc)
        return JSONResponse(status_code=400, content={"detail": str(exc)})
    log.exception("Unhandled error: %s", exc)
    return JSONResponse(status_code=500, content={"detail": str(exc)})


@app.post("/upload", response_model=UploadResponse)
async def upload_craig_zip(
    file: fastapi.UploadFile = fastapi.File(...),
    session_id: str | None = fastapi.Form(default=None),
):
    """Upload and extract a Craig Bot ZIP file."""
    temp_path = None
    try:
        suffix = Path(file.filename or "upload.zip").suffix or ".zip"
        with tempfile.NamedTemporaryFile(delete=False, suffix=suffix) as tmp:
            temp_path = tmp.name
            tmp.write(await file.read())

        result = extract_craig_zip(Path(temp_path), session_id=session_id)
        return UploadResponse(**result)
    except ValueError as exc:
        raise fastapi.HTTPException(status_code=400, detail=str(exc))
    finally:
        if temp_path:
            Path(temp_path).unlink(missing_ok=True)


@app.post("/label-speakers", response_model=LabelSpeakersResponse)
def label_speakers(request: LabelSpeakersRequest):
    """Save speaker labels for a session."""
    try:
        session = load_session(request.session_id)
        session["speakers"] = [speaker.model_dump() for speaker in request.speakers]
        save_session(request.session_id, session)
        return LabelSpeakersResponse(
            session_id=request.session_id,
            speakers=request.speakers,
        )
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.post("/session-metadata")
def update_session_metadata(request: SessionMetadataRequest):
    """Update session metadata (campaign, title, date)."""
    try:
        session_number = request.session_number
        should_increment = False
        if request.campaign_id and session_number is None:
            session_number = next_session_number(request.campaign_id)
            should_increment = True

        campaign = set_campaign_metadata(
            session_id=request.session_id,
            campaign_id=request.campaign_id,
            session_number=session_number,
            title=request.title,
            date=request.date,
            metadata=request.metadata,
            notes=request.notes,
        )
        if should_increment and request.campaign_id:
            increment_session_number(request.campaign_id)
        return {"status": "success", "campaign": campaign}
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.post("/transcribe", response_model=TranscribeResponse)
def transcribe(request: TranscribeRequest):
    """Transcribe session tracks."""
    try:
        result = transcribe_session(
            session_id=request.session_id,
            language=request.language,
            model=request.model,
            provider=request.provider,
        )
        return TranscribeResponse(**result)
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=400, detail=str(exc))


@app.post("/estimate-tokens", response_model=EstimateTokensResponse)
def estimate_tokens(request: EstimateTokensRequest):
    """Analyze context window usage for a transcript."""
    analyzer = ContextWindowAnalyzer()
    prompt = request.system_prompt or ""
    result = analyzer.analyze(
        transcript=request.transcript,
        prompt=prompt,
        model=request.model,
        is_cloud=request.is_cloud,
    )
    return EstimateTokensResponse(**result)


@app.get("/prompts")
def get_prompts():
    """Return available system prompt presets."""
    from app.prompts import get_available_prompts

    return get_available_prompts()


@app.post("/summarize", response_model=SummarizeResponse)
def summarize(request: SummarizeRequest):
    """Summarize a session transcript."""
    try:
        result = summarize_session(
            session_id=request.session_id,
            transcript_path=request.transcript_path,
            output_path=request.output_path,
            title=request.title,
            context=request.context,
            provider=request.provider,
            model=request.model,
            base_url=request.base_url,
            system_prompt=request.system_prompt,
        )
        return SummarizeResponse(
            summary=result.summary,
            provider=result.provider,
            model=result.model,
            summary_path=result.summary_path,
            metadata=result.metadata,
        )
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=400, detail=str(exc))
    except SummarizationError as exc:
        raise fastapi.HTTPException(status_code=500, detail=str(exc))


@app.post("/export", response_model=ExportResponse)
def export_notes(request: ExportRequest):
    """Export session notes to markdown."""
    try:
        result = export_session(
            session_id=request.session_id,
            use_obsidian_format=request.use_obsidian_format,
            custom_filename=request.custom_filename,
        )
        return ExportResponse(
            content=result["content"],
            filename=result["filename"],
            use_obsidian_format=result["use_obsidian_format"],
        )
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))
    except ValueError as exc:
        raise fastapi.HTTPException(status_code=400, detail=str(exc))


@app.get("/config", response_model=ConfigResponse)
def read_config():
    config = get_config()
    tpref = config.get("transcription_provider", "auto")
    return ConfigResponse(
        output_root=config["output_root"],
        summary_provider=config["summary_provider"],
        ollama_base_url=config["ollama_base_url"],
        ollama_model=config["ollama_model"],
        ollama_timeout_seconds=config["ollama_timeout_seconds"],
        litellm_model=config["litellm_model"],
        litellm_api_base=config["litellm_api_base"],
        litellm_timeout_seconds=config["litellm_timeout_seconds"],
        default_language=config["default_language"],
        whisperx_model=config["whisperx_model"],
        transcription_provider=tpref,
        transcription_provider_effective=resolve_transcription_provider(tpref),
        has_litellm_key=bool(config.get("litellm_api_key")),
    )


@app.put("/config", response_model=ConfigResponse)
def write_config(request: UpdateConfigRequest):
    payload = request.model_dump(exclude_none=True)
    if "output_root" in payload:
        validate_output_root(str(payload["output_root"]))
    if "transcription_provider" in payload:
        allowed = {"auto", "mlx-audio", "onnx-asr"}
        tp = str(payload["transcription_provider"]).strip().lower()
        if tp not in allowed:
            raise fastapi.HTTPException(
                status_code=400,
                detail=f"transcription_provider must be one of: {', '.join(sorted(allowed))}",
            )
        payload["transcription_provider"] = tp
    if "summary_provider" in payload:
        from app.services.llm_providers import REGISTRY_IDS
        sp = str(payload["summary_provider"]).strip().lower()
        allowed = {"ollama", "litellm"} | REGISTRY_IDS
        if sp not in allowed:
            raise fastapi.HTTPException(
                status_code=400,
                detail=f"summary_provider must be one of: {', '.join(sorted(allowed))}",
            )
        payload["summary_provider"] = sp
    updated = update_config(payload)
    tpref = updated.get("transcription_provider", "auto")
    return ConfigResponse(
        output_root=updated["output_root"],
        summary_provider=updated["summary_provider"],
        ollama_base_url=updated["ollama_base_url"],
        ollama_model=updated["ollama_model"],
        ollama_timeout_seconds=updated["ollama_timeout_seconds"],
        litellm_model=updated["litellm_model"],
        litellm_api_base=updated["litellm_api_base"],
        litellm_timeout_seconds=updated["litellm_timeout_seconds"],
        default_language=updated["default_language"],
        whisperx_model=updated["whisperx_model"],
        transcription_provider=tpref,
        transcription_provider_effective=resolve_transcription_provider(tpref),
        has_litellm_key=bool(updated.get("litellm_api_key")),
    )


@app.get("/sessions", response_model=list[SessionInfo])
def read_sessions():
    return [SessionInfo(**item) for item in list_sessions()]


@app.get("/session/{session_id}")
def read_session(session_id: str):
    try:
        return load_session(session_id)
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.get("/session/{session_id}/metadata")
def read_session_metadata(session_id: str):
    try:
        return get_campaign_metadata(session_id)
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.get("/sessions/{session_id}/transcripts", response_model=list[ArtifactInfo])
def read_session_transcripts(session_id: str):
    return [ArtifactInfo(**item) for item in list_artifacts(session_id, "transcript")]


@app.get("/sessions/{session_id}/transcripts/{artifact_id}/content")
def read_transcript_text(session_id: str, artifact_id: int):
    """Return the text content of a specific transcript."""
    try:
        content = read_artifact_content(session_id, artifact_id)
        return fastapi.responses.PlainTextResponse(content)
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.delete("/sessions/{session_id}/transcripts/{artifact_id}")
def remove_transcript(session_id: str, artifact_id: int):
    """Delete a specific transcript."""
    try:
        delete_transcript(session_id, artifact_id)
        return {"status": "deleted", "artifact_id": artifact_id}
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.get("/sessions/{session_id}/summaries", response_model=list[ArtifactInfo])
def read_session_summaries(session_id: str):
    return [ArtifactInfo(**item) for item in list_artifacts(session_id, "summary")]


@app.get("/sessions/{session_id}/summaries/{artifact_id}/content")
def read_summary_text(session_id: str, artifact_id: int):
    """Return the text content of a specific summary."""
    try:
        content = read_artifact_content(session_id, artifact_id)
        return fastapi.responses.PlainTextResponse(content)
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.delete("/sessions/{session_id}/summaries/{artifact_id}")
def remove_summary(session_id: str, artifact_id: int):
    """Delete a specific summary."""
    try:
        delete_summary(session_id, artifact_id)
        return {"status": "deleted", "artifact_id": artifact_id}
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.delete("/sessions/{session_id}")
def remove_session(session_id: str):
    try:
        delete_session(session_id)
        return {"status": "deleted", "session_id": session_id}
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.get("/providers")
def read_providers():
    return get_available_providers()


@app.get("/llm-providers", response_model=list[ProviderInfo])
def read_llm_providers():
    """Return registry of cloud LLM providers with saved-key status."""
    saved = list_provider_keys()
    return [
        ProviderInfo(
            id=p["id"],
            name=p["name"],
            models=p["models"],
            default_model=p["default_model"],
            needs_key=p["needs_key"],
            default_api_base=p.get("default_api_base"),
            has_key=bool(saved.get(p["id"], {}).get("api_key")),
            has_custom_base=bool(saved.get(p["id"], {}).get("api_base")),
            saved_model=saved.get(p["id"], {}).get("default_model") or None,
        )
        for p in PROVIDER_REGISTRY
    ]


@app.put("/llm-providers/{provider_id}", response_model=ProviderInfo)
def write_provider_key(provider_id: str, request: ProviderKeyUpdate):
    """Save or update the API key and optional base URL for a provider."""
    provider = PROVIDER_REGISTRY_BY_ID.get(provider_id)
    if provider is None:
        raise fastapi.HTTPException(status_code=404, detail=f"Unknown provider: {provider_id}")
    upsert_provider_key(
        provider_id,
        api_key=request.api_key or "",
        api_base=request.api_base or "",
        default_model=request.default_model or "",
    )
    saved = get_provider_key(provider_id) or {}
    return ProviderInfo(
        id=provider["id"],
        name=provider["name"],
        models=provider["models"],
        default_model=provider["default_model"],
        needs_key=provider["needs_key"],
        default_api_base=provider.get("default_api_base"),
        has_key=bool(saved.get("api_key")),
        has_custom_base=bool(saved.get("api_base")),
        saved_model=saved.get("default_model") or None,
    )


@app.post("/llm-providers/{provider_id}/test", response_model=ProviderTestResult)
def test_llm_provider(provider_id: str, request: ProviderTestRequest):
    """Test connectivity for a configured provider using a 1-token completion."""
    import time
    provider = PROVIDER_REGISTRY_BY_ID.get(provider_id)
    if provider is None:
        raise fastapi.HTTPException(status_code=404, detail=f"Unknown provider: {provider_id}")
    saved = get_provider_key(provider_id) or {}
    api_key = saved.get("api_key", "")
    api_base = saved.get("api_base") or provider.get("default_api_base")
    model_id = request.model or provider["default_model"]
    litellm_model = f"{provider['litellm_prefix']}/{model_id}"
    start = time.monotonic()
    try:
        import litellm as _litellm
        kwargs: dict = {
            "model": litellm_model,
            "messages": [{"role": "user", "content": "Hi"}],
            "max_tokens": 1,
            "timeout": 15.0,
        }
        if api_key:
            kwargs["api_key"] = api_key
        if api_base:
            kwargs["api_base"] = api_base
        _litellm.completion(**kwargs)
        latency_ms = int((time.monotonic() - start) * 1000)
        return ProviderTestResult(ok=True, latency_ms=latency_ms)
    except Exception as exc:
        latency_ms = int((time.monotonic() - start) * 1000)
        return ProviderTestResult(ok=False, latency_ms=latency_ms, error=str(exc))


@app.get("/campaigns", response_model=CampaignsResponse)
def read_campaigns():
    data = list_campaigns()
    campaigns = [CampaignInfo(**item) for item in data["campaigns"]]
    return CampaignsResponse(
        campaigns=campaigns, current_campaign_id=data["current_campaign_id"]
    )


@app.get("/campaigns/{campaign_id}", response_model=CampaignDetail)
def read_campaign_detail(campaign_id: str):
    try:
        campaign = get_campaign_detail(campaign_id)
        return CampaignDetail(**campaign)
    except KeyError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.put("/campaigns/{campaign_id}", response_model=CampaignDetail)
def update_campaign_detail(campaign_id: str, request: CampaignUpdateRequest):
    try:
        campaign = update_campaign(campaign_id, request.model_dump(exclude_none=True))
        return CampaignDetail(**campaign)
    except KeyError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))


@app.get(
    "/campaigns/{campaign_id}/sessions",
    response_model=list[CampaignSessionInfo],
)
def read_campaign_sessions(campaign_id: str):
    return [CampaignSessionInfo(**item) for item in list_campaign_sessions(campaign_id)]


@app.post(
    "/campaigns/{campaign_id}/sessions",
    response_model=CampaignSessionInfo,
)
def create_campaign_session_route(
    campaign_id: str, request: CreateCampaignSessionRequest
):
    try:
        session = create_campaign_session(
            campaign_id=campaign_id,
            session_number=request.session_number,
            title=request.title,
            date=request.date,
        )
        campaign = session.get("campaign") or {}
        sid = session.get("session_id")
        return CampaignSessionInfo(
            session_id=sid,
            session_number=campaign.get("session_number"),
            title=campaign.get("title"),
            date=campaign.get("date"),
            metadata=session.get("metadata") or {},
            has_transcription=bool(list_artifacts(sid, "transcript")),
            has_summary=bool(list_artifacts(sid, "summary")),
        )
    except FileNotFoundError as exc:
        raise fastapi.HTTPException(status_code=404, detail=str(exc))
    except FileExistsError as exc:
        raise fastapi.HTTPException(status_code=409, detail=str(exc))


@app.post("/campaigns")
def create_campaign_route(request: CreateCampaignRequest):
    campaign = create_campaign(
        request.campaign_id, request.name, request.start_session_number
    )
    return {"status": "success", "campaign": campaign}


@app.get("/next-session-number", response_model=NextSessionNumberResponse)
def read_next_session_number(campaign_id: str | None = None):
    return NextSessionNumberResponse(
        next_session_number=next_session_number(campaign_id)
    )
