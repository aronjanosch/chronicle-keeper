import json
import os
from pathlib import Path
from typing import Dict, Any, Optional
import logging

logger = logging.getLogger(__name__)

class ConfigManager:
    def __init__(self, config_dir: str = None):
        """
        Initialize configuration manager
        
        Args:
            config_dir: Directory to store config files (defaults to user data dir)
        """
        if config_dir is None:
            # Use platform-appropriate config directory
            if os.name == 'nt':  # Windows
                config_dir = os.path.expandvars(r'%APPDATA%\ChronicleKeeper')
            else:  # Unix-like
                config_dir = os.path.expanduser('~/.config/chronicle-keeper')
        
        self.config_dir = Path(config_dir)
        self.config_dir.mkdir(parents=True, exist_ok=True)
        self.config_file = self.config_dir / 'settings.json'
        
        # Initialize with defaults if file doesn't exist
        if not self.config_file.exists():
            self._save_default_config()
    
    def get_default_prompt(self) -> str:
        """Get the default system prompt for session summarization"""
        return """You are a professional Dungeon Master's assistant. Your task is to analyze the following D&D session transcript and generate a CONCISE, structured session summary.

Focus ONLY on the most critical elements:
1. Major plot developments and revelations.
2. Key character decisions and actions (especially combat outcomes or failed rolls that change the story).
3. Action items or goals set for the next session.

Format the output using Markdown with two distinct, bolded sections:

**Summary of Events:**
- [Bullet point 1]
- [Bullet point 2]

**Key Decisions & Next Steps:**
- [Bullet point 1 - A choice the party made]
- [Bullet point 2 - A goal or action item for the next session]"""
    
    def _save_default_config(self):
        """Save default configuration"""
        default_config = {
            "gemini_api_key": "",
            "llm_preference": "local",
            "system_prompt": self.get_default_prompt(),
            "created_at": str(Path().cwd()),
            "version": "1.0.0"
        }
        
        with open(self.config_file, 'w', encoding='utf-8') as f:
            json.dump(default_config, f, indent=2, ensure_ascii=False)
    
    def get_settings(self) -> Dict[str, Any]:
        """Load and return current settings"""
        try:
            with open(self.config_file, 'r', encoding='utf-8') as f:
                return json.load(f)
        except (FileNotFoundError, json.JSONDecodeError) as e:
            logger.warning(f"Could not load config: {e}, using defaults")
            self._save_default_config()
            return self.get_settings()
    
    def update_settings(self, updates: Dict[str, Any]):
        """
        Update settings with new values
        
        Args:
            updates: Dictionary of settings to update
        """
        current_settings = self.get_settings()
        
        # Update only provided values
        for key, value in updates.items():
            if value is not None:  # Don't overwrite with None values
                current_settings[key] = value
        
        # Save updated settings
        with open(self.config_file, 'w', encoding='utf-8') as f:
            json.dump(current_settings, f, indent=2, ensure_ascii=False)
        
        logger.info("Settings updated successfully")
    
    def get_setting(self, key: str, default: Any = None) -> Any:
        """
        Get a specific setting value
        
        Args:
            key: Setting key to retrieve
            default: Default value if key not found
            
        Returns:
            Setting value or default
        """
        settings = self.get_settings()
        return settings.get(key, default)
    
    def reset_to_defaults(self):
        """Reset all settings to defaults"""
        self._save_default_config()
        logger.info("Settings reset to defaults")

class SessionManager:
    def __init__(self, session_dir: str = "/tmp/chronicle_sessions"):
        """
        Initialize session manager
        
        Args:
            session_dir: Directory to store session data
        """
        self.session_dir = Path(session_dir)
        self.session_dir.mkdir(parents=True, exist_ok=True)
        self.sessions = {}  # In-memory session cache
    
    def create_session(self, session_id: str, tracks: list):
        """
        Create a new session
        
        Args:
            session_id: Unique session identifier
            tracks: List of track information
        """
        session_data = {
            "id": session_id,
            "tracks": tracks,
            "speaker_mapping": {},
            "transcript": None,
            "summary": None,
            "created_at": str(Path().cwd())
        }
        
        self.sessions[session_id] = session_data
        self._save_session(session_id)
        
        logger.info(f"Created session {session_id} with {len(tracks)} tracks")
    
    def get_session(self, session_id: str) -> Optional[Dict]:
        """
        Get session data
        
        Args:
            session_id: Session identifier
            
        Returns:
            Session data or None if not found
        """
        if session_id in self.sessions:
            return self.sessions[session_id]
        
        # Try to load from disk
        session_file = self.session_dir / f"{session_id}.json"
        if session_file.exists():
            try:
                with open(session_file, 'r', encoding='utf-8') as f:
                    session_data = json.load(f)
                    self.sessions[session_id] = session_data
                    return session_data
            except (json.JSONDecodeError, FileNotFoundError):
                logger.error(f"Could not load session {session_id}")
        
        return None
    
    def set_speaker_mapping(self, session_id: str, mapping: Dict[str, str]):
        """
        Set speaker mapping for a session
        
        Args:
            session_id: Session identifier
            mapping: Track ID to speaker name mapping
        """
        session = self.get_session(session_id)
        if session:
            session["speaker_mapping"] = mapping
            self._save_session(session_id)
            logger.info(f"Updated speaker mapping for session {session_id}")
        else:
            raise ValueError(f"Session {session_id} not found")
    
    def update_session(self, session_id: str, updates: Dict[str, Any]):
        """
        Update session with new data
        
        Args:
            session_id: Session identifier
            updates: Data to update
        """
        session = self.get_session(session_id)
        if session:
            session.update(updates)
            self._save_session(session_id)
        else:
            raise ValueError(f"Session {session_id} not found")
    
    def _save_session(self, session_id: str):
        """Save session data to disk"""
        session_data = self.sessions.get(session_id)
        if session_data:
            session_file = self.session_dir / f"{session_id}.json"
            with open(session_file, 'w', encoding='utf-8') as f:
                json.dump(session_data, f, indent=2, ensure_ascii=False)
    
    def cleanup_session(self, session_id: str):
        """
        Clean up session data and files
        
        Args:
            session_id: Session identifier
        """
        # Remove from memory
        if session_id in self.sessions:
            del self.sessions[session_id]
        
        # Remove session file
        session_file = self.session_dir / f"{session_id}.json"
        if session_file.exists():
            session_file.unlink()
        
        # Clean up audio files
        from ..audio.extraction import cleanup_session
        cleanup_session(session_id)
        
        logger.info(f"Cleaned up session {session_id}")
    
    def list_sessions(self) -> list:
        """List all available sessions"""
        sessions = []
        
        # Load from disk
        for session_file in self.session_dir.glob("*.json"):
            try:
                with open(session_file, 'r', encoding='utf-8') as f:
                    session_data = json.load(f)
                    sessions.append({
                        "id": session_data["id"],
                        "created_at": session_data.get("created_at"),
                        "track_count": len(session_data.get("tracks", []))
                    })
            except (json.JSONDecodeError, KeyError):
                continue
        
        return sorted(sessions, key=lambda x: x["created_at"], reverse=True)