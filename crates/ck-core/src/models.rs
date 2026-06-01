use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct CampaignInfo {
    pub campaign_id: String,
    pub name: String,
    pub next_session_number: i64,
}

#[derive(Debug, Serialize)]
pub struct CampaignDetail {
    pub campaign_id: String,
    pub name: String,
    pub next_session_number: i64,
    pub system: String,
    pub gm: String,
    pub gm_pronouns: String,
    pub setting: String,
    pub default_language: String,
    pub players: Value,
    pub extra_info: String,
    pub codex: String,
    /// Freeform notes as a JSON array [{title, body}], all fed verbatim into summaries.
    pub codex_notes: Value,
    /// LLM-generated "story so far" narrative rollup (read-only, regenerate on demand).
    pub recap: String,
    /// When `recap` was last generated (local naive timestamp; empty if never).
    pub recap_updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct CampaignsResponse {
    pub campaigns: Vec<CampaignInfo>,
    pub current_campaign_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CampaignSessionInfo {
    pub session_id: String,
    pub session_number: Option<i64>,
    pub title: Option<String>,
    pub date: Option<String>,
    pub metadata: Value,
    pub has_tracks: bool,
    pub has_transcription: bool,
    pub has_summary: bool,
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub session_path: String,
    pub has_tracks: bool,
    pub has_transcription: bool,
    pub has_summary: bool,
    pub transcript_path: Option<String>,
    pub summary_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub session_id: String,
    pub session_path: String,
    pub tracks: Value,
}

#[derive(Debug, Deserialize)]
pub struct LabelSpeakersRequest {
    pub session_id: String,
    pub speakers: Value,
}

#[derive(Debug, Serialize)]
pub struct ArtifactInfo {
    pub id: i64,
    pub artifact_id: String,
    pub session_id: String,
    pub kind: String,
    pub provider: String,
    pub model: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub speaker: Option<String>,
    pub source: Option<String>,
    pub words: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct TranscribeRequest {
    pub session_id: String,
    pub language: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TranscribeResponse {
    pub language: String,
    pub json_path: Option<String>,
    pub text_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NextSessionNumberResponse {
    pub next_session_number: i64,
}

#[derive(Debug, Deserialize)]
pub struct SummarizeRequest {
    pub session_id: String,
    pub transcript_id: Option<i64>,
    pub transcript_path: Option<String>,
    pub output_path: Option<String>,
    pub title: Option<String>,
    pub context: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SummarizeResponse {
    pub summary: String,
    pub provider: String,
    pub model: String,
    pub summary_path: Option<String>,
    pub metadata: Option<Value>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub session_id: String,
    pub summary_id: Option<i64>,
    #[serde(default = "default_true")]
    pub use_obsidian_format: bool,
    pub custom_filename: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub content: String,
    pub filename: String,
    /// Absolute path the note was written to (in the session's data folder).
    pub path: Option<String>,
    pub use_obsidian_format: bool,
}

#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub needs_key: bool,
    pub default_api_base: Option<String>,
    pub has_key: bool,
    pub has_custom_base: bool,
    pub saved_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderKeyUpdate {
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderTestRequest {
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderTestResult {
    pub ok: bool,
    pub latency_ms: i64,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCampaignRequest {
    pub campaign_id: String,
    pub name: String,
    #[serde(default = "one")]
    pub start_session_number: i64,
}

fn one() -> i64 {
    1
}

#[derive(Debug, Default, Deserialize)]
pub struct CampaignUpdateRequest {
    pub name: Option<String>,
    pub system: Option<String>,
    pub gm: Option<String>,
    pub gm_pronouns: Option<String>,
    pub setting: Option<String>,
    pub default_language: Option<String>,
    pub players: Option<Value>,
    pub extra_info: Option<String>,
    pub codex: Option<String>,
    pub codex_notes: Option<Value>,
    pub next_session_number: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CreateCampaignSessionRequest {
    pub session_number: Option<i64>,
    pub title: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SessionMetadataRequest {
    pub session_id: String,
    pub campaign_id: Option<String>,
    pub session_number: Option<i64>,
    pub title: Option<String>,
    pub date: Option<String>,
    pub metadata: Option<Value>,
    pub notes: Option<String>,
}

// --- Codex entries (Phase 2) ---

#[derive(Debug, Serialize, Clone)]
pub struct CodexEntry {
    pub entry_id: String,
    pub campaign_id: String,
    pub name: String,
    pub kind: String,
    pub body: String,
    /// Distilled multi-sentence write-up shown in the entry inspector. NOT fed
    /// into summaries (the one-line `body` still is) — this is for the human.
    pub detail: String,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexEntryCreate {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct CodexEntryUpdate {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub body: Option<String>,
    pub detail: Option<String>,
}

/// Generate the campaign "story so far" recap. Provider/model optional — falls
/// back to the configured summary provider, same as `SummarizeRequest`.
#[derive(Debug, Default, Deserialize)]
pub struct RecapRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RecapResponse {
    pub recap: String,
    pub recap_updated_at: String,
    pub provider: String,
    pub model: String,
    /// Number of session summaries that fed the recap.
    pub sessions_used: usize,
}

// --- Summary prompt templates ---

#[derive(Debug, Serialize, Clone)]
pub struct PromptTemplate {
    pub id: String,
    pub label: String,
    pub text: String,
    /// True for the two shipped defaults. The user may still edit or delete them;
    /// the flag only drives the "restore defaults" affordance and a UI badge.
    pub builtin: bool,
    pub sort_order: i64,
}

#[derive(Debug, Deserialize)]
pub struct PromptTemplateCreate {
    pub label: String,
    pub text: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct PromptTemplateUpdate {
    pub label: Option<String>,
    pub text: Option<String>,
}

/// Paste-and-distill import: raw notes in, proposed entries out (reviewed before save).
#[derive(Debug, Deserialize)]
pub struct CodexImportRequest {
    pub text: String,
}

/// Commit the reviewed entries from an import.
#[derive(Debug, Deserialize)]
pub struct CodexCommitRequest {
    pub entries: Vec<CodexEntryCreate>,
}
