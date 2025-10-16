import whisperx
import gc
import torch
from typing import Dict, List
import logging

logger = logging.getLogger(__name__)

class WhisperTranscriber:
    def __init__(self, model_size: str = "base", device: str = None):
        """
        Initialize WhisperX transcriber
        
        Args:
            model_size: Whisper model size (tiny, base, small, medium, large-v2)
            device: Device to use (auto-detected if None)
        """
        self.model_size = model_size
        self.device = device or ("cuda" if torch.cuda.is_available() else "cpu")
        self.model = None
        self.align_model = None
        self.metadata = None
        
    def load_model(self):
        """Load Whisper model and alignment model"""
        if self.model is None:
            # Clear CUDA cache before loading model
            if torch.cuda.is_available():
                torch.cuda.empty_cache()
                
            # Use int8 for better compatibility with newer CUDA versions
            compute_type = "int8" if self.device == "cuda" else "int8"
            
            try:
                self.model = whisperx.load_model(
                    self.model_size, 
                    self.device,
                    compute_type=compute_type,
                    local_files_only=False
                )
            except Exception as e:
                logger.error(f"Failed to load model with CUDA, falling back to CPU: {e}")
                # Fallback to CPU if CUDA fails
                self.device = "cpu"
                self.model = whisperx.load_model(
                    self.model_size, 
                    "cpu",
                    compute_type="int8",
                    local_files_only=False
                )
            
    def load_align_model(self, language_code: str = "en"):
        """Load alignment model for better timestamp accuracy"""
        if self.align_model is None:
            try:
                self.align_model, self.metadata = whisperx.load_align_model(
                    language_code=language_code, 
                    device=self.device
                )
            except Exception as e:
                logger.error(f"Failed to load alignment model with {self.device}, falling back to CPU: {e}")
                # Fallback to CPU if current device fails
                self.align_model, self.metadata = whisperx.load_align_model(
                    language_code=language_code, 
                    device="cpu"
                )
    
    def transcribe_file(self, audio_path: str, speaker_name: str = None) -> Dict:
        """
        Transcribe a single audio file
        
        Args:
            audio_path: Path to audio file
            speaker_name: Name of the speaker (if known)
            
        Returns:
            Transcription result with segments and speaker info
        """
        self.load_model()
        
        try:
            # Load audio
            audio = whisperx.load_audio(audio_path)
            
            # Transcribe with smaller batch size for stability
            batch_size = 8 if self.device == "cuda" else 16
            result = self.model.transcribe(audio, batch_size=batch_size)
            
            # Align whisper output
            self.load_align_model()
            
            # Use CPU device for alignment if main device is CUDA and causing issues
            align_device = "cpu" if self.device == "cuda" else self.device
            result = whisperx.align(
                result["segments"], 
                self.align_model, 
                self.metadata, 
                audio, 
                align_device, 
                return_char_alignments=False
            )
            
        except Exception as e:
            logger.error(f"Error during transcription: {e}")
            # Return a basic structure if transcription fails
            return {
                "segments": [{
                    "start": 0,
                    "end": 10,
                    "text": f"[Transcription failed for {speaker_name or 'Unknown'}]",
                    "speaker": speaker_name or "Unknown"
                }]
            }
        
        # Add speaker information to each segment
        if speaker_name:
            for segment in result["segments"]:
                segment["speaker"] = speaker_name
        
        return result
    
    def cleanup(self):
        """Clean up models to free GPU memory"""
        if self.model is not None:
            del self.model
            self.model = None
        if self.align_model is not None:
            del self.align_model
            self.align_model = None
        if self.metadata is not None:
            del self.metadata
            self.metadata = None
        gc.collect()
        if torch.cuda.is_available():
            torch.cuda.empty_cache()

def transcribe_session(tracks: List[Dict], speaker_mapping: Dict[str, str]) -> str:
    """
    Transcribe all tracks in a session and merge into a single transcript
    
    Args:
        tracks: List of track dictionaries with file paths
        speaker_mapping: Mapping of track IDs to speaker names
        
    Returns:
        Merged transcript as formatted string
    """
    transcriber = WhisperTranscriber()
    all_segments = []
    
    try:
        for track in tracks:
            track_id = track["id"]
            file_path = track["file_path"]
            speaker_name = speaker_mapping.get(track_id, f"Speaker_{track_id}")
            
            logger.info(f"Transcribing {file_path} for {speaker_name}")
            
            result = transcriber.transcribe_file(file_path, speaker_name)
            
            # Add segments to combined list
            for segment in result["segments"]:
                segment["track_id"] = track_id
                all_segments.append(segment)
        
        # Sort all segments by start time
        all_segments.sort(key=lambda x: x.get("start", 0))
        
        # Format as readable transcript
        transcript = format_transcript(all_segments)
        
        return transcript
        
    finally:
        # Always clean up
        transcriber.cleanup()

def format_transcript(segments: List[Dict]) -> str:
    """
    Format transcript segments into readable text
    
    Args:
        segments: List of transcript segments with timestamps and speakers
        
    Returns:
        Formatted transcript string
    """
    transcript_lines = []
    transcript_lines.append("# D&D Session Transcript\n")
    
    current_speaker = None
    speaker_text = []
    
    for segment in segments:
        speaker = segment.get("speaker", "Unknown")
        text = segment.get("text", "").strip()
        start_time = segment.get("start", 0)
        
        if not text:
            continue
            
        # Format timestamp
        minutes = int(start_time // 60)
        seconds = int(start_time % 60)
        timestamp = f"[{minutes:02d}:{seconds:02d}]"
        
        # If speaker changed, write previous speaker's text and start new
        if speaker != current_speaker:
            if current_speaker and speaker_text:
                # Join the accumulated text for the previous speaker
                combined_text = " ".join(speaker_text).strip()
                transcript_lines.append(f"**{current_speaker}:** {combined_text}\n")
                speaker_text = []
            
            current_speaker = speaker
        
        # Add text to current speaker's accumulated text
        speaker_text.append(text)
    
    # Don't forget the last speaker
    if current_speaker and speaker_text:
        combined_text = " ".join(speaker_text).strip()
        transcript_lines.append(f"**{current_speaker}:** {combined_text}\n")
    
    return "\n".join(transcript_lines)