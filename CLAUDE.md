# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Chronicle Keeper is a web application that generates structured D&D session notes from Discord audio files (Craig Bot recordings). The architecture follows a strict separation between a Python FastAPI backend for audio processing/LLM integration and a TypeScript/Vite frontend for the web interface.

**Core Principle**: Maximum simplicity and fast time-to-market. Avoid complex features like in-app editing.

## Development Commands

### Backend Development (Python)
```bash
# Navigate to backend directory
cd backend

# Install dependencies (mandatory: use uv, not pip)
uv install

# Verify setup and dependencies
uv run python test_setup.py

# Start development server with auto-reload
uv run python run.py

# Alternative server start
uv run uvicorn src.main:app --host 127.0.0.1 --port 8000 --reload

# API documentation available at: http://127.0.0.1:8000/docs
```

### Frontend Development (Web)
```bash
# Navigate to frontend directory
cd frontend

# Install dependencies
npm install

# Development mode with hot reload
npm run dev

# Build frontend
npm run build

# Preview production build
npm run preview
```

### Dependency Management
- **MANDATORY**: Use `uv` for all Python dependency management, never `pip`
- **MANDATORY**: Do not pin versions initially to benefit from latest features
- Add new dependencies: `uv add <package>`

### External LLM Dependencies
```bash
# Ollama setup (local LLM)
ollama serve
ollama pull llama3.2

# Gemini API key required for cloud LLM (set via /settings endpoint)
```

## Architecture & Code Structure

### Core Backend Components

**FastAPI Application** (`backend/src/main.py`):
- Single entry point with 6 REST endpoints implementing the 4-step user workflow
- CORS enabled for Tauri frontend communication
- Session-based processing with UUID tracking
- Health check endpoint at `/health`

**Audio Processing Pipeline** (`backend/src/audio/`):
- `extraction.py`: Craig Bot ZIP file processing and validation
- `transcription.py`: WhisperX integration with speaker diarization and timestamp alignment
- Temporary session storage in `/tmp/chronicle_sessions/`

**Dual LLM Architecture** (`backend/src/llm/`):
- `ollama.py`: Local LLM client with auto-startup and model management
- `gemini.py`: Cloud LLM client with safety settings and token estimation
- Dynamic routing based on user preference (local/cloud)

**Configuration Management** (`backend/src/storage/`):
- JSON-based persistence in platform-appropriate directories
- Settings include: Gemini API key, LLM preference, custom system prompts
- Default D&D session summarization prompt included

### 4-Step User Workflow (API Design)

1. **POST /upload**: Process Craig ZIP → Extract audio tracks → Return track listing
2. **POST /label-speakers**: Map track IDs to speaker names → Store mapping
3. **POST /generate-notes**: WhisperX transcription → LLM summarization → Return notes
4. **POST /export**: Save session notes as Markdown file

Additional endpoints:
- **GET /settings**: Retrieve current settings (API keys masked for security)
- **POST /settings**: Update settings (Gemini API key, LLM preference, system prompt)
- **GET /health**: Health check endpoint

### Session Management Pattern

Sessions are UUID-identified and stored both in-memory and on-disk:
- Audio files temporarily stored during processing
- Speaker mappings persist per session
- Automatic cleanup after processing
- Session data includes tracks, mappings, transcripts, and summaries

### Technology Stack Constraints

- **Backend**: Python FastAPI with WhisperX (not whisper-cpp due to build issues)
- **Frontend**: TypeScript with Vite (standard web application)
- **LLM Local**: Ollama with llama3.2 recommended
- **LLM Cloud**: Google Gemini API (gemini-2.0-flash-exp)
- **Audio**: WhisperX for GPU-accelerated transcription with speaker diarization

### Configuration Storage

Settings stored in platform-specific locations:
- Linux/Mac: `~/.config/chronicle-keeper/settings.json`
- Windows: `%APPDATA%\ChronicleKeeper\settings.json`

### Import Structure

All backend modules use relative imports from `backend/src/` directory:
```python
from audio.extraction import extract_craig_zip
from audio.transcription import transcribe_session
from llm.ollama import OllamaClient
from llm.gemini import GeminiClient
from storage.manager import ConfigManager, SessionManager
```

### Directory Structure

```
chronicle-keeper/
├── backend/                 # Python FastAPI backend
│   ├── src/
│   │   ├── main.py         # FastAPI application entry point
│   │   ├── audio/          # Audio processing modules
│   │   │   ├── extraction.py
│   │   │   └── transcription.py
│   │   ├── llm/            # LLM client modules
│   │   │   ├── ollama.py
│   │   │   └── gemini.py
│   │   └── storage/        # Configuration and session management
│   │       └── manager.py
│   ├── run.py              # Application entry point
│   ├── test_setup.py       # Dependency verification
│   └── pyproject.toml      # Python dependencies (uv managed)
├── frontend/               # Web application frontend
│   ├── src/                # TypeScript frontend code
│   ├── package.json        # Node.js dependencies
│   └── dist/               # Build output
└── CLAUDE.md               # This file
```

## Key Implementation Details

### LLM Engine Selection
Runtime routing between local and cloud LLMs based on user preference. Ollama client handles server startup and model availability checks. Gemini client includes content length validation and safety settings.

### Audio Processing Chain
Craig Bot ZIP files contain multi-track FLAC/WAV files. WhisperX processes each track separately, then merges into a single speaker-labeled, timestamped transcript. GPU acceleration utilized when available.

### Error Handling Pattern
All endpoints use try/catch with HTTPException responses. Temporary files always cleaned up in finally blocks. Session cleanup includes both memory and disk storage.

### Security Considerations
- API keys stored locally only
- CORS configured for frontend access
- No sensitive data in API responses (keys masked)
- Temporary files auto-cleaned