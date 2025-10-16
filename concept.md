# Chronicle Keeper: Final Reference Document for Development Agent

## 1\. Executive Summary

**Project Goal:** Develop a simple, cross-platform desktop application ("Chronicle Keeper") to generate concise D\&D session notes from Discord audio files. The core value is providing a structured summary based on a customizable LLM prompt, using a **Local LLM (Ollama)** for performance and a **Cloud LLM (Gemini API)** as a high-quality fallback.

**Core Principle:** **Maximum Simplicity & Fast Time-to-Market (TTM).** Omit complex features like in-app editing.

## 2\. Technical Stack & Mandatory Rules

| Component | Primary Technology | Rationale & Development Rule |
| :--- | :--- | :--- |
| **App Framework** | **Tauri (Rust/Webview)** | Preferred over Electron for smaller binary size and better native performance. The Rust backend will manage the Python environment and communicate with the frontend via the Tauri IPC bridge. |
| **Core Logic** | **Python Backend (Local API)** | Handles all heavy lifting: audio processing, transcription, and LLM communication (both local & cloud). |
| **Dependency Tooling**| **`uv`** | **Mandatory:** Use `uv` for all Python dependency management and environment creation. **Do not pin versions initially** to benefit from the latest features and performance. |
| **Audio Processing** | **Whisper (via `whisper-cpp` or `WhisperX` bindings)** | Fastest available local, high-accuracy transcription, leveraging the user's GPU (NVIDIA/AMD/Apple Silicon). |
| **Local LLM Runner** | **Ollama** | Simplifies running quantized models (e.g., Llama 3 8B) on all target platforms, exposing a standard local API. |
| **Cloud LLM** | **Google Gemini API (via `google-genai` Python SDK)** | Provides an optional, high-quality, and reliable summarization endpoint (e.g., `gemini-2.5-flash`). |
| **Data Storage** | **Tauri Storage / Local JSON File** | Store user settings (API Keys, System Prompt) in a persistent, simple format. |

-----

## 3\. Application Workflow (Simplified)

The application flow consists of four simple, sequential steps for the user.

| Step | User Action | System Action (Backend) |
| :--- | :--- | :--- |
| **1. Ingestion** | User imports the **Craig Bot ZIP file** (containing multi-track audio) via a file dialog. | Backend extracts files and presents a list of track names (e.g., `track_01.flac`, `track_02.flac`). |
| **2. Labeling** | User assigns a **Speaker Name** to each track (e.g., `track_01` $\rightarrow$ "DM-Alex"). | Backend stores the mapping. |
| **3. Processing** | User clicks **"Generate Notes."** (with a toggle for Local/Cloud LLM) | 1. **Transcription:** Uses the speaker map and **Whisper** to generate a single, time-aligned, speaker-labeled transcript. 2. **Summarization:** Calls the chosen LLM endpoint (Local Ollama or Cloud Gemini API), injecting the **user's custom System Prompt** + the transcript. |
| **4. Export** | The final, raw LLM output is displayed in a non-editable text box. User clicks **"Export."** | Backend saves the content as a **Markdown (.md)** file (and optionally a `.txt` file) to the user's desired location. |

-----

## 4\. Key Feature Implementation Details

### 4.1. LLM Toggle and API Management

  * **User Setting:** A simple radio button/toggle in the UI: **"Summarization Engine: [ Local LLM (Ollama) ] / [ Cloud LLM (Gemini) ]"**
  * **API Key Storage:** The UI must include an input field for the **Gemini API Key**. The key and the selected engine preference must be stored in the local persistent configuration (e.g., a simple JSON file saved in the application data directory).
  * **Backend Logic:** The Python backend function responsible for summarization must check the user's preference and route the request accordingly:
      * **IF Cloud (Gemini):** Use the `google-genai` SDK with the stored API key.
      * **IF Local (Ollama):** Use a standard `requests` call to the Ollama local API endpoint (`http://127.0.0.1:11434`) and ensure the Ollama server is running (e.g., spawn/check the process).

### 4.2. Customizable System Prompt (Prompt Engineering Control)

  * **Settings Panel:** The app must have a dedicated **Settings** screen.

  * **Prompt Input:** This screen must contain a large, multi-line text area labeled **"Session Summarization System Prompt."**

  * **Storage:** The entire text content of this field is the **System Prompt** and must be saved to the local persistent configuration file.

  * **Default Prompt:** The application must ship with a high-quality default prompt:

    ```markdown
    You are a professional Dungeon Master's assistant. Your task is to analyze the following D&D session transcript and generate a CONCISE, structured session summary. 

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
    - [Bullet point 2 - A goal or action item for the next session]
    ```

### 4.3. Export

  * **Tooling:** Use Python's native file I/O operations (`pathlib` or `open()`) to save the final text string to disk.
  * **Output:** The content is saved as a **Markdown (.md)** file to the location chosen by the user in a file-save dialog. The `.md` format is universally compatible with note-taking apps (Notion, Obsidian, etc.).