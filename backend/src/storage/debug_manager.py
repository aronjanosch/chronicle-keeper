import json
import os
from pathlib import Path
from datetime import datetime
from typing import Dict, Any, Optional
import logging

logger = logging.getLogger(__name__)

class DebugManager:
    def __init__(self, debug_dir: str = None):
        """
        Initialize debug manager
        
        Args:
            debug_dir: Directory to store debug files (defaults to project debug-exports)
        """
        if debug_dir is None:
            # Go up from backend/src/storage to project root, then to debug-exports
            current_dir = Path(__file__).parent
            project_root = current_dir.parent.parent.parent
            debug_dir = project_root / "debug-exports"
        
        self.debug_dir = Path(debug_dir)
        self.debug_dir.mkdir(parents=True, exist_ok=True)

        # Back-compat: keep references to old subdirectories (may exist on disk),
        # but new exports will go into a single timestamped run directory per export.
        self.sessions_dir = self.debug_dir / "sessions"
        self.transcripts_dir = self.debug_dir / "transcripts"
        self.llm_outputs_dir = self.debug_dir / "llm-outputs"
        self.prompts_dir = self.debug_dir / "prompts"
        self.metadata_dir = self.debug_dir / "metadata"

        # Map of session_id -> run directory Path to group one export run together
        self._session_run_dirs: Dict[str, Path] = {}
    
    def _get_or_create_run_dir(self, session_id: str) -> Path:
        """Get or create the current run directory for a session.

        A new folder is created per export run, named by timestamp for
        natural chronological ordering, e.g. 2025-11-06_142311.
        """
        if session_id in self._session_run_dirs:
            return self._session_run_dirs[session_id]

        run_name = datetime.now().strftime("%Y-%m-%d_%H%M%S")
        run_dir = self.debug_dir / run_name
        run_dir.mkdir(parents=True, exist_ok=True)
        self._session_run_dirs[session_id] = run_dir
        return run_dir

    def get_debug_filename(self, session_id: str, prefix: str, extension: str = ".json") -> str:
        """Generate simplified filename (timestamp is on folder name)."""
        return f"{prefix}{extension}"
    
    def export_session_data(self, session_id: str, session_data: Dict[str, Any]):
        """Export complete session data for debugging"""
        try:
            run_dir = self._get_or_create_run_dir(session_id)
            filename = self.get_debug_filename(session_id, "session_data")
            filepath = run_dir / filename
            
            # Add debug metadata
            debug_data = {
                "debug_info": {
                    "exported_at": datetime.now().isoformat(),
                    "session_id": session_id,
                    "export_type": "session_data"
                },
                "session_data": session_data
            }
            
            with open(filepath, 'w', encoding='utf-8') as f:
                json.dump(debug_data, f, indent=2, ensure_ascii=False)
            
            logger.info(f"Exported session data to {filepath}")
            return str(filepath)
        except Exception as e:
            logger.error(f"Failed to export session data: {e}")
            return None
    
    def export_transcript(self, session_id: str, transcript: str, processing_info: Dict[str, Any] = None):
        """Export raw transcript and processing information"""
        try:
            run_dir = self._get_or_create_run_dir(session_id)
            filename = self.get_debug_filename(session_id, "transcript")
            filepath = run_dir / filename
            
            debug_data = {
                "debug_info": {
                    "exported_at": datetime.now().isoformat(),
                    "session_id": session_id,
                    "export_type": "transcript"
                },
                "transcript": transcript,
                "processing_info": processing_info or {}
            }
            
            with open(filepath, 'w', encoding='utf-8') as f:
                json.dump(debug_data, f, indent=2, ensure_ascii=False)
            
            # Also export as plain text for easier reading
            txt_filename = self.get_debug_filename(session_id, "transcript", ".txt")
            txt_filepath = run_dir / txt_filename
            with open(txt_filepath, 'w', encoding='utf-8') as f:
                f.write(f"=== TRANSCRIPT DEBUG EXPORT ===\n")
                f.write(f"Session ID: {session_id}\n")
                f.write(f"Exported at: {datetime.now().isoformat()}\n")
                f.write(f"Processing info: {json.dumps(processing_info or {}, indent=2)}\n")
                f.write(f"\n=== RAW TRANSCRIPT ===\n\n")
                f.write(transcript)
            
            logger.info(f"Exported transcript to {filepath} and {txt_filepath}")
            return str(filepath)
        except Exception as e:
            logger.error(f"Failed to export transcript: {e}")
            return None
    
    def export_llm_interaction(self, session_id: str, engine: str, prompt: str, response: str, 
                              settings: Dict[str, Any] = None, metadata: Dict[str, Any] = None,
                              parsed: Dict[str, Any] = None):
        """Export full LLM prompt and response for debugging"""
        try:
            run_dir = self._get_or_create_run_dir(session_id)
            filename = self.get_debug_filename(session_id, f"llm_{engine}")
            filepath = run_dir / filename
            
            debug_data = {
                "debug_info": {
                    "exported_at": datetime.now().isoformat(),
                    "session_id": session_id,
                    "export_type": "llm_interaction",
                    "engine": engine
                },
                "settings": settings or {},
                "prompt": prompt,
                "response": response,
                "metadata": metadata or {},
                "parsed": parsed or {}
            }
            
            with open(filepath, 'w', encoding='utf-8') as f:
                json.dump(debug_data, f, indent=2, ensure_ascii=False)
            
            # Also export as text for easier reading
            txt_filename = self.get_debug_filename(session_id, f"llm_{engine}", ".txt")
            txt_filepath = run_dir / txt_filename
            with open(txt_filepath, 'w', encoding='utf-8') as f:
                f.write(f"=== LLM INTERACTION DEBUG EXPORT ===\n")
                f.write(f"Session ID: {session_id}\n")
                f.write(f"Engine: {engine}\n")
                f.write(f"Exported at: {datetime.now().isoformat()}\n")
                f.write(f"Settings: {json.dumps(settings or {}, indent=2)}\n")
                f.write(f"Metadata: {json.dumps(metadata or {}, indent=2)}\n")
                f.write(f"\n=== PROMPT SENT TO LLM ===\n\n")
                f.write(prompt)
                f.write(f"\n\n=== RESPONSE FROM LLM ===\n\n")
                f.write(response)
                if parsed:
                    f.write("\n\n=== PARSED RESULT ===\n\n")
                    try:
                        f.write(json.dumps(parsed, indent=2, ensure_ascii=False))
                    except Exception:
                        # Fallback in case of non-serializable content
                        f.write(str(parsed))
            
            logger.info(f"Exported LLM interaction to {filepath} and {txt_filepath}")
            return str(filepath)
        except Exception as e:
            logger.error(f"Failed to export LLM interaction: {e}")
            return None
    
    def export_prompt_used(self, session_id: str, prompt_type: str, prompt: str, 
                          language: str, is_custom: bool = False):
        """Export the exact prompt used for generation"""
        try:
            run_dir = self._get_or_create_run_dir(session_id)
            filename = self.get_debug_filename(session_id, f"prompt_{prompt_type}")
            filepath = run_dir / filename
            
            debug_data = {
                "debug_info": {
                    "exported_at": datetime.now().isoformat(),
                    "session_id": session_id,
                    "export_type": "prompt",
                    "prompt_type": prompt_type,
                    "language": language,
                    "is_custom": is_custom
                },
                "prompt": prompt
            }
            
            with open(filepath, 'w', encoding='utf-8') as f:
                json.dump(debug_data, f, indent=2, ensure_ascii=False)
            
            logger.info(f"Exported prompt to {filepath}")
            return str(filepath)
        except Exception as e:
            logger.error(f"Failed to export prompt: {e}")
            return None
    
    def export_metadata_analysis(self, session_id: str, analysis_type: str, 
                                analysis_result: Dict[str, Any], transcript: str = None):
        """Export metadata analysis results"""
        try:
            run_dir = self._get_or_create_run_dir(session_id)
            filename = self.get_debug_filename(session_id, f"metadata_{analysis_type}")
            filepath = run_dir / filename
            
            debug_data = {
                "debug_info": {
                    "exported_at": datetime.now().isoformat(),
                    "session_id": session_id,
                    "export_type": "metadata_analysis",
                    "analysis_type": analysis_type
                },
                "analysis_result": analysis_result,
                "transcript_length": len(transcript) if transcript else 0,
                "transcript": transcript if transcript else None
            }
            
            with open(filepath, 'w', encoding='utf-8') as f:
                json.dump(debug_data, f, indent=2, ensure_ascii=False)
            
            logger.info(f"Exported metadata analysis to {filepath}")
            return str(filepath)
        except Exception as e:
            logger.error(f"Failed to export metadata analysis: {e}")
            return None
    
    def list_debug_files(self, session_id: str = None) -> Dict[str, list]:
        """List all debug files, optionally filtered by session ID.

        Supports both the new simplified structure (timestamped run folders)
        and the legacy category subfolders for backward compatibility.
        """
        try:
            debug_files = {
                "sessions": [],
                "transcripts": [],
                "llm_outputs": [],
                "prompts": [],
                "metadata": []
            }

            def categorize_and_add(file_path: Path):
                name = file_path.name
                if name.startswith("session_data"):
                    dest = "sessions"
                elif name.startswith("transcript"):
                    dest = "transcripts"
                elif name.startswith("llm_"):
                    dest = "llm_outputs"
                elif name.startswith("prompt_"):
                    dest = "prompts"
                elif name.startswith("metadata_"):
                    dest = "metadata"
                else:
                    return

                # If filtering by session, try to check JSON content debug_info.session_id
                if session_id is not None and file_path.suffix == ".json":
                    try:
                        with open(file_path, "r", encoding="utf-8") as f:
                            data = json.load(f)
                        file_session = data.get("debug_info", {}).get("session_id")
                        if not file_session or not file_session.startswith(session_id):
                            return
                    except Exception:
                        # If unreadable, skip when filtering
                        return

                debug_files[dest].append({
                    "filename": name,
                    "path": str(file_path),
                    "size": file_path.stat().st_size,
                    "modified": datetime.fromtimestamp(file_path.stat().st_mtime).isoformat()
                })

            # New structure: iterate run directories (YYYY-MM-DD_HHMMSS)
            for entry in sorted(self.debug_dir.iterdir() if self.debug_dir.exists() else [], key=lambda p: p.name):
                if entry.is_dir():
                    for file_path in entry.glob("*.json"):
                        categorize_and_add(file_path)

            # Legacy structure fallback
            for directory in [self.sessions_dir, self.transcripts_dir, self.llm_outputs_dir, self.prompts_dir, self.metadata_dir]:
                if directory.exists() and directory.is_dir():
                    for file_path in directory.glob("*.json"):
                        # Legacy names included session_id in file name; quick filter first
                        if session_id is None or session_id[:8] in file_path.name:
                            categorize_and_add(file_path)

            return debug_files
        except Exception as e:
            logger.error(f"Failed to list debug files: {e}")
            return {}
    
    def cleanup_old_files(self, days_old: int = 7):
        """Clean up debug files older than specified days.

        Removes entire run directories older than cutoff, and also cleans
        up legacy flat/category files if present.
        """
        try:
            cutoff_time = datetime.now().timestamp() - (days_old * 24 * 3600)
            deleted_count = 0

            # New structure: remove old run directories
            if self.debug_dir.exists():
                for entry in self.debug_dir.iterdir():
                    if entry.is_dir():
                        try:
                            mtime = entry.stat().st_mtime
                            if mtime < cutoff_time:
                                for child in entry.iterdir():
                                    if child.is_file():
                                        child.unlink()
                                entry.rmdir()
                                deleted_count += 1
                        except Exception:
                            # Best-effort cleanup; continue
                            pass

            # Legacy structure: clean files inside old category folders
            for directory in [self.sessions_dir, self.transcripts_dir, self.llm_outputs_dir, self.prompts_dir, self.metadata_dir]:
                if directory.exists():
                    for file_path in directory.iterdir():
                        if file_path.is_file() and file_path.stat().st_mtime < cutoff_time:
                            file_path.unlink()
                            deleted_count += 1

            logger.info(f"Cleaned up {deleted_count} old debug files/folders")
            return deleted_count
        except Exception as e:
            logger.error(f"Failed to cleanup old files: {e}")
            return 0