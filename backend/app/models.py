"""Pydantic models for the simplified CK backend."""

from __future__ import annotations

from pydantic import BaseModel, Field


class TrackInfo(BaseModel):
    """Information about an extracted audio track."""

    id: str
    filename: str
    file_path: str
    duration: float | None = None


class UploadResponse(BaseModel):
    """Response from uploading and extracting a Craig ZIP."""

    session_id: str
    session_path: str
    tracks: list[TrackInfo]


class SpeakerLabel(BaseModel):
    """Speaker mapping for a specific track."""

    track_id: str
    player_name: str | None = None
    character_name: str | None = None
    pronouns: str | None = None


class LabelSpeakersRequest(BaseModel):
    """Request to label speakers for a session."""

    session_id: str
    speakers: list[SpeakerLabel]


class LabelSpeakersResponse(BaseModel):
    """Response after saving speaker labels."""

    session_id: str
    speakers: list[SpeakerLabel]


class SessionMetadataRequest(BaseModel):
    """Request to update session metadata."""

    session_id: str
    campaign_id: str | None = None
    session_number: int | None = None
    title: str | None = None
    date: str | None = None
    metadata: dict | None = None
    notes: str | None = None


class TranscribeRequest(BaseModel):
    """Request parameters for transcription."""

    session_id: str
    language: str | None = Field(default=None, description="Language code override")
    model: str | None = Field(
        default=None,
        description="Speech-to-text model id for the selected provider (overrides saved default)",
    )
    provider: str | None = Field(
        default=None,
        description="Transcription provider (mlx-audio or onnx-asr)",
    )


class TranscribeResponse(BaseModel):
    """Response from transcription endpoint."""

    language: str
    json_path: str | None
    text_path: str | None


class EstimateTokensRequest(BaseModel):
    """Request for context window estimation."""

    transcript: str
    model: str
    is_cloud: bool = False
    system_prompt: str | None = None


class EstimateTokensResponse(BaseModel):
    """Response for context window estimation."""

    fits: bool
    estimated_tokens: int
    max_tokens: int
    available_tokens: int
    output_buffer: int
    usage_percent: float
    recommended_action: str
    message: str
    model: str
    is_cloud: bool
    breakdown: dict


class SummarizeRequest(BaseModel):
    """Request parameters for summarization."""

    session_id: str
    transcript_path: str | None = Field(
        default=None, description="Optional transcript path override"
    )
    output_path: str | None = Field(default=None, description="Optional output path")
    title: str | None = Field(default=None, description="Optional title")
    context: str | None = Field(default=None, description="Optional context")
    provider: str | None = Field(default=None, description="Provider override")
    model: str | None = Field(default=None, description="Model override")
    base_url: str | None = Field(default=None, description="Provider base URL override")
    system_prompt: str | None = Field(
        default=None,
        description="Custom system prompt; overrides the built-in template when set",
    )


class SummarizeResponse(BaseModel):
    """Response from summarize endpoint."""

    summary: str
    provider: str
    model: str
    summary_path: str | None
    metadata: dict | None = None


class ExportRequest(BaseModel):
    """Request parameters for exporting notes."""

    session_id: str
    use_obsidian_format: bool = True
    custom_filename: str | None = None


class ExportResponse(BaseModel):
    """Response from export endpoint."""

    content: str
    filename: str
    use_obsidian_format: bool


class SessionInfo(BaseModel):
    """Session status overview."""

    session_id: str
    session_path: str
    has_transcription: bool
    has_summary: bool
    transcript_path: str | None = None
    summary_path: str | None = None


class ArtifactInfo(BaseModel):
    """Artifact (transcript or summary) information."""

    id: int
    session_id: str
    kind: str
    provider: str
    model: str
    file_path: str
    created_at: str


class UpdateConfigRequest(BaseModel):
    """Request parameters for updating configuration."""

    output_root: str | None = None
    summary_provider: str | None = None
    ollama_base_url: str | None = None
    ollama_model: str | None = None
    ollama_timeout_seconds: int | None = None
    litellm_model: str | None = None
    litellm_api_key: str | None = None
    litellm_api_base: str | None = None
    litellm_timeout_seconds: int | None = None
    default_language: str | None = None
    whisperx_model: str | None = None
    transcription_provider: str | None = None


class ConfigResponse(BaseModel):
    """Response for configuration endpoints."""

    output_root: str
    summary_provider: str
    ollama_base_url: str
    ollama_model: str
    ollama_timeout_seconds: int
    litellm_model: str
    litellm_api_base: str
    litellm_timeout_seconds: int
    default_language: str
    whisperx_model: str
    transcription_provider: str
    transcription_provider_effective: str
    has_litellm_key: bool


class CampaignInfo(BaseModel):
    """Campaign summary info."""

    campaign_id: str
    name: str
    next_session_number: int


class CampaignDetail(BaseModel):
    """Campaign detail with metadata."""

    campaign_id: str
    name: str
    next_session_number: int
    system: str | None = None
    gm: str | None = None
    setting: str | None = None
    default_language: str | None = None
    players: list[dict] = []
    extra_info: str | None = None


class CampaignsResponse(BaseModel):
    """Response for campaigns listing."""

    campaigns: list[CampaignInfo]
    current_campaign_id: str | None = None


class CreateCampaignRequest(BaseModel):
    """Request to create a campaign."""

    campaign_id: str
    name: str
    start_session_number: int = 1


class CampaignUpdateRequest(BaseModel):
    """Request to update campaign metadata."""

    name: str | None = None
    system: str | None = None
    gm: str | None = None
    setting: str | None = None
    default_language: str | None = None
    players: list[dict] | list[str] | str | None = None
    extra_info: str | None = None
    next_session_number: int | None = None


class CampaignSessionInfo(BaseModel):
    """Campaign session summary info."""

    session_id: str
    session_number: int | None = None
    title: str | None = None
    date: str | None = None
    metadata: dict | None = None
    has_transcription: bool
    has_summary: bool


class CreateCampaignSessionRequest(BaseModel):
    """Request to create a session under a campaign."""

    session_number: int | None = None
    title: str | None = None
    date: str | None = None


class NextSessionNumberResponse(BaseModel):
    """Response for next session number lookup."""

    next_session_number: int


class ProviderInfo(BaseModel):
    """Registry provider with saved-key status."""

    id: str
    name: str
    models: list[str]
    default_model: str
    needs_key: bool
    default_api_base: str | None = None
    has_key: bool
    has_custom_base: bool
    saved_model: str | None = None


class ProviderKeyUpdate(BaseModel):
    """Request to save an API key for a provider."""

    api_key: str | None = None
    api_base: str | None = None
    default_model: str | None = None


class ProviderTestRequest(BaseModel):
    """Request to test a provider connection."""

    model: str | None = None


class ProviderTestResult(BaseModel):
    """Result of a provider connection test."""

    ok: bool
    latency_ms: int
    error: str | None = None
