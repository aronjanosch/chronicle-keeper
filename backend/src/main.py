from fastapi import FastAPI, UploadFile, File, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import Dict, List, Optional
import uvicorn
import json
import uuid
import os
import logging
from pathlib import Path

from audio.extraction import extract_craig_zip
from audio.transcription import transcribe_session
from llm.ollama import OllamaClient
from llm.gemini import GeminiClient
from storage.manager import ConfigManager, SessionManager

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

# Pydantic models for request/response
class TrackInfo(BaseModel):
    id: str
    filename: str
    file_path: str
    duration: float

class UploadResponse(BaseModel):
    tracks: List[TrackInfo]
    session_id: str

class SpeakerMapping(BaseModel):
    session_id: str
    mappings: Dict[str, str]

class GenerateNotesRequest(BaseModel):
    session_id: str
    llm_engine: str  # "local" or "cloud"
    custom_prompt: Optional[str] = None

class ExportRequest(BaseModel):
    content: str
    file_path: str

class SettingsModel(BaseModel):
    gemini_api_key: Optional[str] = None
    llm_preference: str = "local"
    system_prompt: str

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
    """Map track IDs to speaker names"""
    try:
        session_manager.set_speaker_mapping(mapping.session_id, mapping.mappings)
        return {"status": "success", "message": "Speaker mapping stored"}
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Failed to store mapping: {str(e)}")

@app.post("/generate-notes")
async def generate_notes(request: GenerateNotesRequest):
    """Generate transcript and create session summary"""
    try:
        # Get session data
        session_data = session_manager.get_session(request.session_id)
        if not session_data:
            raise HTTPException(status_code=404, detail="Session not found")
        
        # Transcribe audio files
        transcript = transcribe_session(
            session_data["tracks"], 
            session_data["speaker_mapping"]
        )
        
        # Get system prompt
        settings = config_manager.get_settings()
        system_prompt = request.custom_prompt or settings.get("system_prompt", "")
        
        # Generate summary using selected LLM
        if request.llm_engine == "cloud":
            gemini_client = GeminiClient(settings.get("gemini_api_key"))
            summary = gemini_client.generate_summary(transcript, system_prompt)
        else:
            ollama_client = OllamaClient()
            summary = ollama_client.generate_summary(transcript, system_prompt)
        
        # Store the summary in the session
        session_manager.update_session(request.session_id, {
            "summary": summary,
            "transcript": transcript
        })
        
        return {"summary": summary, "transcript": transcript}
    
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
    # Don't return the API key for security
    safe_settings = {
        "llm_preference": settings.get("llm_preference", "local"),
        "system_prompt": settings.get("system_prompt", config_manager.get_default_prompt()),
        "has_gemini_key": bool(settings.get("gemini_api_key"))
    }
    return safe_settings

@app.post("/settings")
async def update_settings(settings: SettingsModel):
    """Update settings and persist to JSON file"""
    try:
        config_manager.update_settings({
            "gemini_api_key": settings.gemini_api_key,
            "llm_preference": settings.llm_preference,
            "system_prompt": settings.system_prompt
        })
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

@app.get("/health")
async def health_check():
    """Health check endpoint"""
    return {"status": "healthy", "service": "Chronicle Keeper API"}

if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=8000)