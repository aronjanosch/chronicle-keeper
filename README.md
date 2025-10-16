# Chronicle Keeper

A cross-platform desktop application that generates structured D&D session notes from Discord audio files (Craig Bot recordings).

## Features

- **Craig Bot ZIP Processing**: Extract and process multi-track audio files
- **Speaker Mapping**: Assign speaker names to audio tracks  
- **WhisperX Transcription**: High-quality, timestamped transcription with speaker diarization
- **Dual LLM Support**: Local processing with Ollama or cloud processing with Google Gemini
- **Customizable Prompts**: User-defined system prompts for session summarization
- **Export Functionality**: Save session notes as Markdown files

## Architecture

- **Frontend**: Tauri desktop application (TypeScript/HTML/CSS)
- **Backend**: Python FastAPI server with audio processing and LLM integration
- **Local LLM**: Ollama with llama3.2 model
- **Cloud LLM**: Google Gemini API (gemini-2.0-flash-exp)

## Prerequisites

1. **Python 3.11+** and **uv** package manager
2. **Node.js** and **npm** for frontend development
3. **Rust** and **Cargo** for Tauri
4. **Ollama** (for local LLM processing) - Download from [ollama.ai](https://ollama.ai)
5. **Google Gemini API Key** (for cloud LLM processing) - Get from [Google AI Studio](https://makersuite.google.com/app/apikey)

## Installation

### Backend Setup

```bash
# Navigate to backend directory
cd backend

# Install dependencies (uv handles virtual environment automatically)
uv install

# Test the setup
uv run python test_setup.py
```

### Frontend Setup

```bash
# Navigate to frontend directory
cd frontend

# Install dependencies
npm install

# Build frontend
npm run build
```

### LLM Setup

#### Local LLM (Ollama)
```bash
# Install and start Ollama
ollama serve

# Pull a model (recommended: llama3.2)
ollama pull llama3.2
```

#### Cloud LLM (Gemini)
1. Get API key from [Google AI Studio](https://makersuite.google.com/app/apikey)
2. Set via the Settings panel in the application

## Usage

### Running the Application

1. **Start the backend server:**
   ```bash
   cd backend
   uv run python run.py
   ```
   The API will be available at `http://127.0.0.1:8000`

2. **Start the frontend application:**
   ```bash
   cd frontend
   npm run tauri dev
   ```

### 4-Step Workflow

1. **Upload**: Import Craig Bot ZIP file containing multi-track audio
2. **Label Speakers**: Assign speaker names to each audio track
3. **Generate Notes**: Choose LLM engine (Local/Cloud) and process the session
4. **Export**: Save the generated notes as a Markdown file

### Settings Configuration

Access the Settings panel to configure:
- **Gemini API Key**: For cloud LLM processing
- **System Prompt**: Customize the LLM prompt for session summarization
- **LLM Preference**: Choose between Local (Ollama) or Cloud (Gemini)

## Development

### Backend Development

```bash
cd backend

# Start development server with auto-reload
uv run python run.py

# API documentation available at: http://127.0.0.1:8000/docs
```

### Frontend Development

```bash
cd frontend

# Development mode with hot reload
npm run tauri dev

# Build for production
npm run tauri build
```

### Project Structure

```
chronicle-keeper/
├── backend/                 # Python FastAPI backend
│   ├── src/
│   │   ├── main.py         # FastAPI application
│   │   ├── audio/          # Audio processing modules
│   │   ├── llm/            # LLM client modules
│   │   └── storage/        # Configuration management
│   ├── run.py              # Application entry point
│   └── pyproject.toml      # Python dependencies
├── frontend/               # Tauri desktop application
│   ├── src/
│   │   ├── main.ts         # TypeScript application logic
│   │   └── styles.css      # UI styling
│   ├── src-tauri/          # Rust backend for Tauri
│   └── index.html          # Main HTML template
└── README.md
```

## Configuration

Settings are stored in platform-specific locations:
- **Linux/Mac**: `~/.config/chronicle-keeper/settings.json`
- **Windows**: `%APPDATA%\\ChronicleKeeper\\settings.json`

## Troubleshooting

### Common Issues

1. **WhisperX GPU Issues**: Ensure CUDA/PyTorch compatibility
2. **Ollama Connection**: Verify `ollama serve` is running
3. **Gemini API**: Check API key validity and quota
4. **File Permissions**: Ensure write access for export directory

### Backend Logs

Check console output for detailed error messages and processing status.

### Frontend Development

Use browser developer tools when running in development mode.

## License

Built for Chronicle Keeper - D&D Session Note Generator