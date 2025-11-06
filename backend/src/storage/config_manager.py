"""
Configuration manager for Chronicle Keeper.

Handles application settings, campaigns, and Obsidian integration configuration.
"""

import json
import os
from pathlib import Path
from typing import Dict, Any
from datetime import datetime
import logging

# Import centralized prompts
from src.prompts import get_base_prompt, get_available_languages, get_empty_metadata

# Import centralized constants
from src.constants import (
    DEFAULT_WHISPER_MODEL,
    DEFAULT_TRANSCRIPTION_SETTINGS,
    AVAILABLE_WHISPER_MODELS,
    AVAILABLE_OLLAMA_MODELS,
    AVAILABLE_TRANSCRIPTION_LANGUAGES,
    DEFAULT_OLLAMA_MODEL,
    DEFAULT_OLLAMA_BASE_URL,
    DEFAULT_OBSIDIAN_FILENAME_PATTERN
)

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

    def get_default_prompt(self, language: str = "en") -> str:
        """Get the default system prompt for session summarization in specified language"""
        return get_base_prompt(language)

    def get_localized_prompts(self) -> Dict[str, str]:
        """Get all localized system prompts"""
        from src.prompts import BASE_PROMPTS
        return BASE_PROMPTS

    def _save_default_config(self):
        """Save default configuration"""
        default_config = {
            "gemini_api_key": "",
            "llm_preference": "local",
            "language": "en",
            "transcription_language": "auto",
            "whisper_model": DEFAULT_WHISPER_MODEL,
            "transcription_settings": DEFAULT_TRANSCRIPTION_SETTINGS,
            "system_prompt": self.get_default_prompt("en"),
            "ollama_model": DEFAULT_OLLAMA_MODEL,
            "ollama_base_url": DEFAULT_OLLAMA_BASE_URL,
            "created_at": str(Path().cwd()),
            "version": "1.0.0",
            "campaigns": {
                "default": {
                    "name": "Default Campaign",
                    "next_session_number": 1,
                    "created_date": datetime.now().isoformat(),
                    "session_count": 0
                }
            },
            "current_campaign": "default",
            "obsidian_settings": {
                "enabled": False,
                "vault_path": "",
                "use_frontmatter": True,
                "template": "default",
                "file_naming": DEFAULT_OBSIDIAN_FILENAME_PATTERN
            }
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

        # Check if language is being changed
        language_changed = "language" in updates and updates["language"] != current_settings.get("language")

        # Update only provided values
        for key, value in updates.items():
            if value is not None:  # Don't overwrite with None values
                current_settings[key] = value

        # If language changed and system_prompt is still default, update it to new language default
        if language_changed:
            new_language = updates["language"]
            current_prompt = current_settings.get("system_prompt", "")
            old_language = "de" if new_language == "en" else "en"  # Determine old language
            old_default = self.get_default_prompt(old_language)

            # If current prompt is the old language default, update to new language default
            if current_prompt == old_default:
                current_settings["system_prompt"] = self.get_default_prompt(new_language)
                logger.info(f"Updated system prompt to {new_language} default")

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

    def get_current_language(self) -> str:
        """Get current language setting"""
        return self.get_setting("language", "en")

    def get_available_languages(self) -> Dict[str, str]:
        """Get available languages with their display names"""
        return get_available_languages()

    def get_available_transcription_languages(self) -> Dict[str, str]:
        """Get available transcription languages with their display names"""
        return AVAILABLE_TRANSCRIPTION_LANGUAGES

    def get_transcription_language(self) -> str:
        """Get current transcription language setting"""
        return self.get_setting("transcription_language", "auto")

    def get_whisper_model(self) -> str:
        """Get current Whisper model setting"""
        return self.get_setting("whisper_model", DEFAULT_WHISPER_MODEL)

    def get_available_whisper_models(self) -> Dict[str, str]:
        """Get available Whisper models with descriptions"""
        return AVAILABLE_WHISPER_MODELS

    def get_transcription_settings(self) -> Dict[str, Any]:
        """Get transcription anti-hallucination settings"""
        return self.get_setting("transcription_settings", DEFAULT_TRANSCRIPTION_SETTINGS)

    def get_current_prompt(self) -> str:
        """Get current system prompt based on language setting"""
        current_language = self.get_current_language()
        custom_prompt = self.get_setting("system_prompt")

        # If custom prompt exists and differs from default, use it
        if custom_prompt and custom_prompt != self.get_default_prompt(current_language):
            return custom_prompt

        # Otherwise return localized default
        return self.get_default_prompt(current_language)

    def reset_to_defaults(self):
        """Reset all settings to defaults"""
        self._save_default_config()
        logger.info("Settings reset to defaults")

    def get_campaigns(self) -> Dict[str, Any]:
        """Get all campaigns"""
        settings = self.get_settings()
        return settings.get("campaigns", {})

    def get_current_campaign(self) -> str:
        """Get current campaign ID"""
        settings = self.get_settings()
        return settings.get("current_campaign", "default")

    def create_campaign(self, campaign_id: str, name: str) -> Dict[str, Any]:
        """Create a new campaign"""
        settings = self.get_settings()

        campaign_data = {
            "name": name,
            "next_session_number": 1,
            "created_date": datetime.now().isoformat(),
            "session_count": 0
        }

        if "campaigns" not in settings:
            settings["campaigns"] = {}

        settings["campaigns"][campaign_id] = campaign_data
        settings["current_campaign"] = campaign_id

        with open(self.config_file, 'w', encoding='utf-8') as f:
            json.dump(settings, f, indent=2, ensure_ascii=False)

        logger.info(f"Created campaign {campaign_id}: {name}")
        return campaign_data

    def get_next_session_number(self, campaign_id: str = None) -> int:
        """Get next session number for a campaign"""
        if campaign_id is None:
            campaign_id = self.get_current_campaign()

        campaigns = self.get_campaigns()
        campaign = campaigns.get(campaign_id, {})
        return campaign.get("next_session_number", 1)

    def increment_session_number(self, campaign_id: str = None):
        """Increment session number for a campaign"""
        if campaign_id is None:
            campaign_id = self.get_current_campaign()

        settings = self.get_settings()
        if "campaigns" in settings and campaign_id in settings["campaigns"]:
            settings["campaigns"][campaign_id]["next_session_number"] += 1
            settings["campaigns"][campaign_id]["session_count"] += 1

            with open(self.config_file, 'w', encoding='utf-8') as f:
                json.dump(settings, f, indent=2, ensure_ascii=False)

            logger.info(f"Incremented session number for campaign {campaign_id}")

    def get_obsidian_settings(self) -> Dict[str, Any]:
        """Get Obsidian integration settings"""
        settings = self.get_settings()
        return settings.get("obsidian_settings", {
            "enabled": False,
            "vault_path": "",
            "use_frontmatter": True,
            "template": "default",
            "file_naming": "Session {session_number:02d} - {session_date}"
        })

    def get_ollama_model(self) -> str:
        """Get current Ollama model setting"""
        return self.get_setting("ollama_model", DEFAULT_OLLAMA_MODEL)

    def get_ollama_base_url(self) -> str:
        """Get current Ollama base URL setting"""
        return self.get_setting("ollama_base_url", DEFAULT_OLLAMA_BASE_URL)

    def get_available_ollama_models(self) -> Dict[str, str]:
        """Get recommended Ollama models with descriptions"""
        return AVAILABLE_OLLAMA_MODELS
