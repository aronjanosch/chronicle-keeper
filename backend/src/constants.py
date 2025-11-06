"""
Centralized constants for Chronicle Keeper.

This module contains all magic numbers, strings, and configuration values
used throughout the application. Consolidating these values provides:
- Single source of truth for configuration
- Easier modification of behavior
- Better maintainability
"""

from pathlib import Path
from typing import Dict, List

# ============================================================================
# PATH CONFIGURATION
# ============================================================================

# Base directory for temporary session storage
SESSION_BASE_PATH = "/tmp/chronicle_sessions"

# Supported audio file extensions for Craig Bot uploads
SUPPORTED_AUDIO_EXTENSIONS = {'.flac', '.wav', '.mp3', '.m4a', '.ogg'}

# ============================================================================
# LLM CONFIGURATION
# ============================================================================

# Temperature settings for different LLM tasks
TEMPERATURE_SUMMARIZATION = 0.7  # Higher for creative summarization
TEMPERATURE_METADATA = 0.3  # Lower for consistent structured output
TEMPERATURE_DEFAULT = 0.3  # Default for most operations

# Token limits
DEFAULT_MAX_TOKENS = 2048

# ============================================================================
# TRANSCRIPTION CONFIGURATION
# ============================================================================

# Whisper batch sizes (optimized for stability)
WHISPER_BATCH_SIZE_CUDA = 8  # Smaller batch for GPU to avoid memory issues
WHISPER_BATCH_SIZE_CPU = 16  # Larger batch for CPU processing

# Compute types by device
WHISPER_COMPUTE_TYPE_CUDA = "float32"  # Better stability than float16
WHISPER_COMPUTE_TYPE_CPU = "int8"  # Optimized for CPU

# Default transcription settings (anti-hallucination)
DEFAULT_TRANSCRIPTION_SETTINGS = {
    "no_speech_threshold": 0.6,
    "logprob_threshold": -1.0,
    "compression_ratio_threshold": 2.4,
    "condition_on_previous_text": False,
    "filter_hallucinations": True
}

# Hallucination filter patterns (common false transcriptions)
HALLUCINATION_PATTERNS = [
    # German subtitle artifacts
    "untertitel",
    "amara.org",
    "zdf",
    "das war's für heute",
    "lasst einen daumen",
    "abonniert meinen kanal",
    "bis zum nächsten mal",
    "copyright wdr",
    "im auftrag des",

    # French subtitle artifacts
    "sous-titres",
    "soustitreur.com",

    # English common hallucinations
    "thank you",
    "thanks for watching",
    "like and subscribe",
    "see you next time",
    "don't forget to",
]

# ============================================================================
# AVAILABLE MODELS & LANGUAGES
# ============================================================================

# Available Whisper models with descriptions
AVAILABLE_WHISPER_MODELS = {
    "tiny": "Tiny (~39 MB, fastest, lowest quality)",
    "base": "Base (~74 MB, fast, good quality)",
    "small": "Small (~244 MB, slower, better quality)",
    "medium": "Medium (~769 MB, slow, high quality)",
    "large-v2": "Large-v2 (~1550 MB, slowest, best quality - recommended for non-English)"
}

# Default Whisper model
DEFAULT_WHISPER_MODEL = "large-v2"

# Available transcription languages
AVAILABLE_TRANSCRIPTION_LANGUAGES = {
    "auto": "Auto-detect",
    "en": "English",
    "de": "German (Deutsch)",
    "es": "Spanish",
    "fr": "French",
    "it": "Italian",
    "pt": "Portuguese",
    "ru": "Russian",
    "ja": "Japanese",
    "ko": "Korean",
    "zh": "Chinese"
}

# Available Ollama models with descriptions
AVAILABLE_OLLAMA_MODELS = {
    "llama3.2": "Llama 3.2 (~2 GB, recommended, good balance)",
    "llama3.2:1b": "Llama 3.2 1B (~1.3 GB, fast, lower quality)",
    "llama3.1": "Llama 3.1 (~4.7 GB, higher quality)",
    "llama3.1:70b": "Llama 3.1 70B (~40 GB, best quality, requires powerful GPU)",
    "mistral": "Mistral (~4.1 GB, good alternative)",
    "mistral-small": "Mistral Small (~8.9 GB, balanced performance)",
    "gemma2": "Gemma 2 (~5.4 GB, Google's model)",
    "qwen2.5": "Qwen 2.5 (~4.7 GB, multilingual support)"
}

# Default Ollama model
DEFAULT_OLLAMA_MODEL = "llama3.2"

# Default Ollama base URL
DEFAULT_OLLAMA_BASE_URL = "http://127.0.0.1:11434"

# ============================================================================
# TIMEOUT VALUES
# ============================================================================

# Request timeouts (in seconds)
TIMEOUT_OLLAMA_REQUEST = 300  # 5 minutes for LLM generation
TIMEOUT_API_REQUEST = 120  # 2 minutes for API calls
TIMEOUT_SHORT = 5  # 5 seconds for quick checks

# ============================================================================
# LOGGING & DEBUG
# ============================================================================

# Debug file export location
DEBUG_EXPORT_DIR = Path.home() / "Downloads" / "chronicle_debug"

# ============================================================================
# FILE NAMING
# ============================================================================

# Default filename pattern for Obsidian export
DEFAULT_OBSIDIAN_FILENAME_PATTERN = "Session {session_number:02d} - {session_date}"
