import zipfile
import os
import shutil
from pathlib import Path
from typing import List, Dict
import tempfile

from constants import SESSION_BASE_PATH, SUPPORTED_AUDIO_EXTENSIONS

def extract_craig_zip(zip_path: str, session_id: str) -> List[Dict[str, str]]:
    """
    Extract Craig Bot ZIP file and return track information
    
    Args:
        zip_path: Path to the uploaded ZIP file
        session_id: Unique session identifier
        
    Returns:
        List of track dictionaries with id, filename, and file_path
    """
    # Create session directory
    session_dir = Path(SESSION_BASE_PATH) / session_id
    session_dir.mkdir(parents=True, exist_ok=True)

    tracks = []

    try:
        with zipfile.ZipFile(zip_path, 'r') as zip_ref:
            # Extract all files
            zip_ref.extractall(session_dir)

            # Find audio files
            audio_extensions = SUPPORTED_AUDIO_EXTENSIONS
            
            for file_path in session_dir.rglob('*'):
                if file_path.is_file() and file_path.suffix.lower() in audio_extensions:
                    # Generate track ID from filename
                    track_id = file_path.stem
                    
                    # Get file duration (placeholder - would use librosa or similar)
                    duration = get_audio_duration(str(file_path))
                    
                    tracks.append({
                        "id": track_id,
                        "filename": file_path.name,
                        "file_path": str(file_path),
                        "duration": duration
                    })
            
            # Sort tracks by name for consistent ordering
            tracks.sort(key=lambda x: x["filename"])
            
    except zipfile.BadZipFile:
        raise ValueError("Invalid ZIP file")
    except Exception as e:
        # Clean up on error
        if session_dir.exists():
            shutil.rmtree(session_dir)
        raise e
    
    if not tracks:
        raise ValueError("No audio files found in ZIP archive")
    
    return tracks

def get_audio_duration(file_path: str) -> float:
    """
    Get duration of audio file in seconds
    
    Args:
        file_path: Path to audio file
        
    Returns:
        Duration in seconds
    """
    try:
        # Using soundfile for basic duration detection
        import soundfile as sf
        with sf.SoundFile(file_path) as f:
            return len(f) / f.samplerate
    except Exception:
        # Fallback: estimate based on file size (very rough)
        file_size = os.path.getsize(file_path)
        # Rough estimate: 1MB ≈ 60 seconds for typical voice recording
        return file_size / (1024 * 1024) * 60

def cleanup_session(session_id: str):
    """
    Clean up temporary session files

    Args:
        session_id: Session identifier to clean up
    """
    session_dir = Path(SESSION_BASE_PATH) / session_id
    if session_dir.exists():
        shutil.rmtree(session_dir)