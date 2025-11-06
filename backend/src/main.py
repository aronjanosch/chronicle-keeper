from fastapi import FastAPI, UploadFile, File, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import Dict, List, Optional
import uvicorn
import json
import uuid
import os
import logging
import sys
from pathlib import Path

from audio.extraction import extract_craig_zip
from audio.transcription import transcribe_session
from llm.ollama import OllamaClient
from llm.gemini import GeminiClient
from storage.config_manager import ConfigManager
from storage.session_manager import SessionManager
from storage.debug_manager import DebugManager

# Configure logging
logging.basicConfig(
    level=logging.DEBUG,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

app = FastAPI(title="Chronicle Keeper API", version="1.0.0")

# Add CORS middleware for frontend communication
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Initialize managers
config_manager = ConfigManager()
session_manager = SessionManager()
debug_manager = DebugManager()

# Pydantic models for request/response
class TrackInfo(BaseModel):
    id: str
    filename: str
    file_path: str
    duration: float

class UploadResponse(BaseModel):
    tracks: List[TrackInfo]
    session_id: str

class SpeakerInfo(BaseModel):
    playerName: str
    characterName: Optional[str] = None
    pronouns: Optional[str] = None

class SpeakerMapping(BaseModel):
    session_id: str
    mappings: Dict[str, SpeakerInfo]

class GenerateNotesRequest(BaseModel):
    session_id: str
    llm_engine: str  # "local" or "cloud"
    custom_prompt: Optional[str] = None
    ollama_model: Optional[str] = None

class ExportRequest(BaseModel):
    content: str
    file_path: str

class SettingsModel(BaseModel):
    gemini_api_key: Optional[str] = None
    llm_preference: str = "local"
    language: str = "en"
    transcription_language: Optional[str] = "auto"
    whisper_model: Optional[str] = "large-v2"
    system_prompt: str
    ollama_model: Optional[str] = "llama3.2"
    ollama_base_url: Optional[str] = "http://127.0.0.1:11434"

class SessionMetadata(BaseModel):
    session_id: str
    session_date: Optional[str] = None
    session_number: Optional[int] = None
    campaign_id: Optional[str] = None
    campaign_name: Optional[str] = None
    locations: List[str] = []
    characters_present: List[str] = []
    tags: List[str] = []
    notes: Optional[str] = None

class CampaignCreate(BaseModel):
    campaign_id: str
    name: str

class ExportOptionsRequest(BaseModel):
    session_id: str
    use_obsidian_format: bool = False
    custom_filename: Optional[str] = None

class MetadataSuggestions(BaseModel):
    suggested_tags: List[str] = []
    mentioned_characters: List[str] = []
    mentioned_locations: List[str] = []
    session_tone: List[str] = []
    key_events: List[str] = []

class AnalyzeMetadataRequest(BaseModel):
    session_id: str
    llm_engine: str = "local"

class PullModelRequest(BaseModel):
    model_name: str

class TestOllamaConnectionRequest(BaseModel):
    base_url: str

@app.post("/upload", response_model=UploadResponse)
async def upload_craig_zip(file: UploadFile = File(...)):
    """Process Craig Bot ZIP file containing multi-track audio"""
    logger.debug(f"Upload request received for file: {file.filename}")
    
    if not file.filename.endswith('.zip'):
        logger.error(f"Invalid file type: {file.filename}")
        raise HTTPException(status_code=400, detail="File must be a ZIP archive")
    
    session_id = str(uuid.uuid4())
    logger.debug(f"Generated session ID: {session_id}")
    
    # Save uploaded file temporarily
    temp_path = f"/tmp/{session_id}_{file.filename}"
    logger.debug(f"Saving uploaded file to: {temp_path}")
    
    with open(temp_path, "wb") as buffer:
        content = await file.read()
        buffer.write(content)
        logger.debug(f"Saved {len(content)} bytes to temporary file")
    
    try:
        logger.debug("Starting ZIP extraction...")
        tracks = extract_craig_zip(temp_path, session_id)
        logger.debug(f"Extracted {len(tracks)} tracks: {[track.get('filename', 'unknown') for track in tracks]}")
        
        # Log track details for debugging
        for track in tracks:
            logger.debug(f"Track details: {track}")
        
        session_manager.create_session(session_id, tracks)
        logger.debug(f"Created session {session_id} with {len(tracks)} tracks")
        
        logger.debug("Creating UploadResponse...")
        response = UploadResponse(tracks=tracks, session_id=session_id)
        logger.debug("UploadResponse created successfully")
        
        return response
    except Exception as e:
        logger.error(f"Error processing ZIP file: {str(e)}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"Failed to process ZIP: {str(e)}")
    finally:
        # Clean up temp file
        if os.path.exists(temp_path):
            os.remove(temp_path)
            logger.debug(f"Cleaned up temporary file: {temp_path}")

@app.post("/label-speakers")
async def label_speakers(mapping: SpeakerMapping):
    """Map track IDs to speaker info (player name, character name, pronouns)"""
    try:
        # Convert Pydantic models to dicts for storage
        mappings_dict = {
            track_id: speaker_info.model_dump()
            for track_id, speaker_info in mapping.mappings.items()
        }
        session_manager.set_speaker_mapping(mapping.session_id, mappings_dict)
        return {"status": "success", "message": "Speaker mapping stored"}
    except Exception as e:
        logger.error(f"Error storing speaker mapping: {str(e)}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"Failed to store mapping: {str(e)}")

@app.post("/generate-notes")
async def generate_notes(request: GenerateNotesRequest):
    """Generate transcript and create session summary with metadata suggestions"""
    try:
        # Get session data
        session_data = session_manager.get_session(request.session_id)
        if not session_data:
            raise HTTPException(status_code=404, detail="Session not found")
        
        # Get transcription language setting
        transcription_language = config_manager.get_transcription_language()
        
        # Transcribe audio files
        transcript = transcribe_session(
            session_data["tracks"], 
            session_data["speaker_mapping"],
            language=transcription_language
        )
        
        # DEBUG: Export raw transcript
        debug_manager.export_transcript(request.session_id, transcript, {
            "tracks_count": len(session_data["tracks"]),
            "speaker_mapping": session_data["speaker_mapping"],
            "transcript_length": len(transcript)
        })
        
        # Get system prompt (localized if no custom prompt)
        settings = config_manager.get_settings()
        system_prompt = request.custom_prompt or config_manager.get_current_prompt()
        current_language = config_manager.get_current_language()
        
        # DEBUG: Export prompt used
        debug_manager.export_prompt_used(
            request.session_id, 
            "summary_generation", 
            system_prompt, 
            current_language,
            is_custom=bool(request.custom_prompt)
        )
        
        # Generate summary and metadata using selected LLM
        if request.llm_engine == "cloud":
            gemini_client = GeminiClient(settings.get("gemini_api_key"))
            result = gemini_client.generate_summary_with_metadata(transcript, system_prompt, current_language)
        else:
            # Allow per-request override of Ollama model; fall back to saved settings
            ollama_model = request.ollama_model or settings.get("ollama_model", "llama3.2")
            ollama_base_url = config_manager.get_ollama_base_url()
            ollama_client = OllamaClient(base_url=ollama_base_url, model=ollama_model)
            result = ollama_client.generate_summary_with_metadata(transcript, system_prompt, current_language)
        
        summary = result["summary"]
        metadata_suggestions = result["metadata"]
        
        # DEBUG: Export LLM interaction (include raw response to show how tags were parsed)
        transcript_label = "Transcript" if current_language == "en" else "Transkript"
        debug_manager.export_llm_interaction(
            request.session_id,
            request.llm_engine,
            f"{system_prompt}\n\n{transcript_label}:\n{transcript}",
            result.get("raw_response", summary),
            {
                "language": current_language,
                "llm_engine": request.llm_engine,
                "ollama_model": (request.ollama_model or settings.get("ollama_model")) if request.llm_engine == "local" else None,
                "has_gemini_key": bool(settings.get("gemini_api_key")) if request.llm_engine == "cloud" else None
            },
            metadata_suggestions,
            parsed={
                "summary": summary,
                "metadata": metadata_suggestions
            }
        )
        
        # Store the summary in the session
        session_manager.update_session(request.session_id, {
            "summary": summary,
            "transcript": transcript,
            "metadata_suggestions": metadata_suggestions
        })
        
        return {
            "summary": summary, 
            "transcript": transcript,
            "metadata_suggestions": metadata_suggestions
        }
    
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to generate notes: {str(e)}")

@app.post("/export")
async def export_notes(request: ExportRequest):
    """Save notes to user-specified file location"""
    try:
        file_path = Path(request.file_path)
        
        # Ensure the file has .md extension
        if not file_path.suffix == '.md':
            file_path = file_path.with_suffix('.md')
        
        # Create directory if it doesn't exist
        file_path.parent.mkdir(parents=True, exist_ok=True)
        
        # Write content to file
        with open(file_path, 'w', encoding='utf-8') as f:
            f.write(request.content)
        
        return {"status": "success", "file_path": str(file_path)}
    
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to export: {str(e)}")

@app.get("/settings")
async def get_settings():
    """Retrieve current settings"""
    settings = config_manager.get_settings()
    current_language = config_manager.get_current_language()

    # Don't return the API key for security
    safe_settings = {
        "llm_preference": settings.get("llm_preference", "local"),
        "language": current_language,
        "transcription_language": config_manager.get_transcription_language(),
        "whisper_model": config_manager.get_whisper_model(),
        "system_prompt": config_manager.get_current_prompt(),
        "has_gemini_key": bool(settings.get("gemini_api_key")),
        "ollama_model": settings.get("ollama_model", "llama3.2"),
        "ollama_base_url": config_manager.get_ollama_base_url(),
        "available_languages": config_manager.get_available_languages(),
        "available_transcription_languages": config_manager.get_available_transcription_languages(),
        "available_whisper_models": config_manager.get_available_whisper_models(),
        "available_ollama_models": config_manager.get_available_ollama_models()
    }
    return safe_settings

@app.post("/settings")
async def update_settings(settings: SettingsModel):
    """Update settings and persist to JSON file"""
    try:
        updates = {
            "gemini_api_key": settings.gemini_api_key,
            "llm_preference": settings.llm_preference,
            "language": settings.language,
            "transcription_language": settings.transcription_language,
            "whisper_model": settings.whisper_model,
            "ollama_model": settings.ollama_model,
            "ollama_base_url": settings.ollama_base_url
        }
        
        # Handle system prompt - if it's the same as the new language default, 
        # store the default to maintain localization
        new_default_prompt = config_manager.get_default_prompt(settings.language)
        if settings.system_prompt == new_default_prompt:
            updates["system_prompt"] = new_default_prompt
        else:
            updates["system_prompt"] = settings.system_prompt
        
        config_manager.update_settings(updates)
        return {"status": "success", "message": "Settings updated"}
    
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to update settings: {str(e)}")

@app.post("/debug-upload", response_model=UploadResponse)
async def debug_upload_example():
    """Debug endpoint to use example recording without uploading"""
    example_zip = "/home/aron/Projects/chronicle-keeper/example-recordings/craig-yNq4gbpXrgTL-lpRQVws6tu6ccFzCF1E-XbJB5QTdQe.flac.zip"
    
    if not os.path.exists(example_zip):
        raise HTTPException(status_code=404, detail="Example recording not found")
    
    session_id = str(uuid.uuid4())
    logger.debug(f"Debug upload - Generated session ID: {session_id}")
    
    try:
        logger.debug("Starting debug ZIP extraction...")
        tracks = extract_craig_zip(example_zip, session_id)
        logger.debug(f"Extracted {len(tracks)} tracks from example recording")
        
        session_manager.create_session(session_id, tracks)
        logger.debug(f"Created debug session {session_id} with {len(tracks)} tracks")
        
        return UploadResponse(tracks=tracks, session_id=session_id)
    except Exception as e:
        logger.error(f"Error processing example ZIP: {str(e)}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"Failed to process example ZIP: {str(e)}")

@app.post("/session-metadata")
async def set_session_metadata(metadata: SessionMetadata):
    """Set session metadata (date, number, campaign, etc.)"""
    try:
        session_data = {
            "session_date": metadata.session_date,
            "session_number": metadata.session_number,
            "campaign_id": metadata.campaign_id,
            "campaign_name": metadata.campaign_name,
            "locations": metadata.locations,
            "characters_present": metadata.characters_present,
            "tags": metadata.tags,
            "notes": metadata.notes
        }
        
        session_manager.set_session_metadata(metadata.session_id, session_data)
        
        # If session number is provided, increment the campaign's next session number
        if metadata.session_number and metadata.campaign_id:
            config_manager.increment_session_number(metadata.campaign_id)
        
        return {"status": "success", "message": "Session metadata updated"}
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to update metadata: {str(e)}")

@app.get("/session-metadata/{session_id}")
async def get_session_metadata(session_id: str):
    """Get session metadata"""
    try:
        metadata = session_manager.get_session_metadata(session_id)
        return metadata
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to get metadata: {str(e)}")

@app.get("/campaigns")
async def get_campaigns():
    """Get all campaigns and their info"""
    try:
        campaigns = config_manager.get_campaigns()
        current_campaign = config_manager.get_current_campaign()
        return {
            "campaigns": campaigns,
            "current_campaign": current_campaign
        }
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to get campaigns: {str(e)}")

@app.post("/campaigns")
async def create_campaign(campaign: CampaignCreate):
    """Create a new campaign"""
    try:
        campaign_data = config_manager.create_campaign(campaign.campaign_id, campaign.name)
        return {"status": "success", "campaign": campaign_data}
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to create campaign: {str(e)}")

@app.get("/next-session-number")
async def get_next_session_number(campaign_id: Optional[str] = None):
    """Get next session number for current or specified campaign"""
    try:
        next_number = config_manager.get_next_session_number(campaign_id)
        return {"next_session_number": next_number}
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to get session number: {str(e)}")

@app.post("/export-obsidian")
async def export_obsidian_notes(request: ExportOptionsRequest):
    """Generate and return Obsidian-formatted notes"""
    try:
        # Get session data
        session_data = session_manager.get_session(request.session_id)
        if not session_data:
            raise HTTPException(status_code=404, detail="Session not found")
        
        summary = session_data.get("summary", "")
        if not summary:
            raise HTTPException(status_code=400, detail="No summary available. Generate notes first.")
        
        if request.use_obsidian_format:
            # Generate Obsidian-formatted content
            content = session_manager.generate_obsidian_content(
                request.session_id, summary, config_manager
            )
            filename = request.custom_filename or session_manager.generate_filename(
                request.session_id, config_manager
            )
        else:
            # Use standard format
            content = summary
            filename = request.custom_filename or "session_notes.md"
        
        return {
            "content": content,
            "filename": filename,
            "use_obsidian_format": request.use_obsidian_format
        }
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to export: {str(e)}")

@app.post("/analyze-metadata", response_model=MetadataSuggestions)
async def analyze_metadata(request: AnalyzeMetadataRequest):
    """Analyze transcript and suggest metadata tags, characters, and locations"""
    try:
        # Get session data
        session_data = session_manager.get_session(request.session_id)
        if not session_data:
            raise HTTPException(status_code=404, detail="Session not found")
        
        # Check if transcript exists
        transcript = session_data.get("transcript")
        if not transcript:
            raise HTTPException(status_code=400, detail="No transcript available. Generate notes first.")
        
        # Get settings for LLM configuration
        settings = config_manager.get_settings()
        current_language = config_manager.get_current_language()
        
        # Analyze metadata using selected LLM
        if request.llm_engine == "cloud":
            gemini_client = GeminiClient(settings.get("gemini_api_key"))
            metadata_suggestions = gemini_client.analyze_metadata(transcript, current_language)
        else:
            ollama_model = settings.get("ollama_model", "llama3.2")
            ollama_base_url = config_manager.get_ollama_base_url()
            ollama_client = OllamaClient(base_url=ollama_base_url, model=ollama_model)
            metadata_suggestions = ollama_client.analyze_metadata(transcript, current_language)
        
        # DEBUG: Export metadata analysis
        debug_manager.export_metadata_analysis(
            request.session_id,
            "metadata_suggestions",
            metadata_suggestions,
            transcript
        )
        
        return MetadataSuggestions(**metadata_suggestions)
    
    except Exception as e:
        logger.error(f"Error analyzing metadata: {e}")
        raise HTTPException(status_code=500, detail=f"Failed to analyze metadata: {str(e)}")

@app.get("/ollama-models")
async def get_ollama_models():
    """Get currently installed Ollama models"""
    try:
        ollama_base_url = config_manager.get_ollama_base_url()
        ollama_client = OllamaClient(base_url=ollama_base_url)

        if not ollama_client.is_server_running():
            return {"models": [], "server_running": False}

        # Get available models by calling the Ollama API directly
        import requests
        response = requests.get(f"{ollama_base_url}/api/tags", timeout=5)

        if response.status_code == 200:
            data = response.json()
            models = [model["name"] for model in data.get("models", [])]
            return {"models": models, "server_running": True}
        else:
            return {"models": [], "server_running": True, "error": "Failed to fetch models"}

    except Exception as e:
        logger.error(f"Error fetching Ollama models: {e}")
        return {"models": [], "server_running": False, "error": str(e)}

@app.post("/ollama-models/pull")
async def pull_ollama_model(request: PullModelRequest):
    """Pull (download) a specific Ollama model"""
    try:
        model_name = request.model_name
        ollama_base_url = config_manager.get_ollama_base_url()
        ollama_client = OllamaClient(base_url=ollama_base_url, model=model_name)

        # Ensure server is running
        if not ollama_client.ensure_server_running():
            raise HTTPException(status_code=503, detail="Ollama server could not be started")

        # Check if model already exists
        if ollama_client.is_model_available():
            return {"status": "success", "message": f"Model '{model_name}' is already available", "already_exists": True}

        # Pull the model
        logger.info(f"Starting pull for model '{model_name}'...")
        success = ollama_client.pull_model()

        if success:
            return {"status": "success", "message": f"Model '{model_name}' pulled successfully", "already_exists": False}
        else:
            raise HTTPException(status_code=500, detail=f"Failed to pull model '{model_name}'")

    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error pulling Ollama model: {e}")
        raise HTTPException(status_code=500, detail=f"Failed to pull model: {str(e)}")

@app.get("/prompts")
async def get_prompts():
    """Get all available localized prompts"""
    try:
        prompts = config_manager.get_localized_prompts()
        available_languages = config_manager.get_available_languages()
        current_language = config_manager.get_current_language()
        
        return {
            "prompts": prompts,
            "languages": available_languages,
            "current_language": current_language
        }
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to get prompts: {str(e)}")

@app.post("/reset-prompt")
async def reset_prompt_to_default():
    """Reset system prompt to localized default"""
    try:
        current_language = config_manager.get_current_language()
        default_prompt = config_manager.get_default_prompt(current_language)
        
        config_manager.update_settings({
            "system_prompt": default_prompt
        })
        
        return {
            "status": "success", 
            "message": f"Prompt reset to {current_language} default",
            "prompt": default_prompt
        }
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to reset prompt: {str(e)}")

@app.get("/debug/files")
async def list_debug_files(session_id: Optional[str] = None):
    """List debug files for a session or all sessions"""
    try:
        debug_files = debug_manager.list_debug_files(session_id)
        return {"debug_files": debug_files}
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to list debug files: {str(e)}")

@app.post("/debug/export-session")
async def export_session_debug(session_id: str):
    """Export complete session data for debugging"""
    try:
        session_data = session_manager.get_session(session_id)
        if not session_data:
            raise HTTPException(status_code=404, detail="Session not found")
        
        filepath = debug_manager.export_session_data(session_id, session_data)
        if filepath:
            return {"status": "success", "file_path": filepath}
        else:
            raise HTTPException(status_code=500, detail="Failed to export session data")
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to export session: {str(e)}")

@app.delete("/debug/cleanup")
async def cleanup_debug_files(days_old: int = 7):
    """Clean up debug files older than specified days"""
    try:
        deleted_count = debug_manager.cleanup_old_files(days_old)
        return {"status": "success", "deleted_files": deleted_count}
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to cleanup files: {str(e)}")

@app.post("/test-ollama-connection")
async def test_ollama_connection(request: TestOllamaConnectionRequest):
    """Test connection to an Ollama server"""
    try:
        import requests
        response = requests.get(f"{request.base_url}/api/tags", timeout=5)

        if response.status_code == 200:
            data = response.json()
            models = [model["name"] for model in data.get("models", [])]
            return {
                "status": "success",
                "server_running": True,
                "models_count": len(models),
                "models": models[:5]  # Return first 5 models
            }
        else:
            return {
                "status": "error",
                "server_running": False,
                "message": f"Server responded with status {response.status_code}"
            }
    except requests.Timeout:
        return {
            "status": "error",
            "server_running": False,
            "message": "Connection timeout - server may be unreachable"
        }
    except requests.ConnectionError:
        return {
            "status": "error",
            "server_running": False,
            "message": "Connection error - check if server is running and URL is correct"
        }
    except Exception as e:
        return {
            "status": "error",
            "server_running": False,
            "message": f"Error: {str(e)}"
        }

class StopOllamaModelRequest(BaseModel):
    model: str

@app.post("/ollama/stop")
async def stop_ollama_model(req: StopOllamaModelRequest):
    """Force-stop an Ollama model to free VRAM (fallback cleanup)."""
    try:
        import subprocess
        subprocess.run(["ollama", "stop", req.model], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return {"status": "success", "stopped": req.model}
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to stop model: {str(e)}")

@app.get("/health")
async def health_check():
    """Health check endpoint"""
    return {"status": "healthy", "service": "Chronicle Keeper API"}

def get_bundled_temp_dir():
    """Get appropriate temp directory for bundled application"""
    if getattr(sys, 'frozen', False):
        # Running as bundled executable
        return Path.home() / ".chronicle-keeper" / "temp"
    else:
        # Running in development
        return Path("/tmp")

def setup_bundled_environment():
    """Setup environment for bundled application"""
    if getattr(sys, 'frozen', False):
        # Create necessary directories
        temp_dir = get_bundled_temp_dir()
        temp_dir.mkdir(parents=True, exist_ok=True)
        
        # Ensure session directory exists
        session_dir = temp_dir / "chronicle_sessions"
        session_dir.mkdir(parents=True, exist_ok=True)

def main():
    """Main entry point for the application"""
    setup_bundled_environment()
    
    # Configure logging for production
    log_level = logging.INFO if getattr(sys, 'frozen', False) else logging.DEBUG
    logging.basicConfig(
        level=log_level,
        format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
    )
    
    # Determine host and port
    host = os.getenv("CHRONICLE_HOST", "127.0.0.1")
    port = int(os.getenv("CHRONICLE_PORT", "8000"))
    
    logger.info(f"Starting Chronicle Keeper API on {host}:{port}")
    uvicorn.run(app, host=host, port=port, log_level="info")

if __name__ == "__main__":
    main()