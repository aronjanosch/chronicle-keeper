import os
# Force CPU for pyannote components to avoid CUDA hanging
os.environ["PYANNOTE_DEVICE"] = "cpu"

import whisperx
import gc
import torch
from typing import Dict, List
import logging

logger = logging.getLogger(__name__)

class WhisperTranscriber:
    def __init__(self, model_size: str = "base", device: str = None, language: str = "auto", transcription_settings: Dict = None):
        """
        Initialize WhisperX transcriber

        Args:
            model_size: Whisper model size (tiny, base, small, medium, large-v2)
            device: Device to use (auto-detected if None)
            language: Language code for transcription ("auto" for auto-detection)
            transcription_settings: Anti-hallucination settings
        """
        self.model_size = model_size
        self.device = device or ("cuda" if torch.cuda.is_available() else "cpu")
        self.language = language
        self.model = None
        self.align_model = None
        self.metadata = None
        self.detected_language = None

        # Load transcription settings
        self.transcription_settings = transcription_settings or {
            "no_speech_threshold": 0.6,
            "logprob_threshold": -1.0,
            "compression_ratio_threshold": 2.4,
            "condition_on_previous_text": False,
            "filter_hallucinations": True
        }
        
    def load_model(self):
        """Load Whisper model and alignment model"""
        if self.model is None:
            # Clear CUDA cache before loading model
            if torch.cuda.is_available():
                torch.cuda.empty_cache()

            # Use float32 on CUDA for better stability, int8 on CPU
            compute_type = "float32" if self.device == "cuda" else "int8"

            try:
                # Try to load without VAD first
                import whisperx.asr
                from faster_whisper import WhisperModel

                # Load faster-whisper model directly
                # Model will be downloaded automatically if not available
                logger.info(f"Loading Whisper model '{self.model_size}' (will download if not available)...")
                whisper_model = WhisperModel(
                    self.model_size,
                    device=self.device,
                    compute_type=compute_type,
                    download_root=None  # Use default cache location
                )
                logger.info(f"Whisper model '{self.model_size}' loaded successfully")
                
                # Create a minimal wrapper that skips VAD
                class NoVADWhisperModel:
                    def __init__(self, model, settings):
                        self.model = model
                        self.settings = settings

                    def transcribe(self, audio, batch_size=16, language=None):
                        # Transcribe without VAD preprocessing, but with anti-hallucination parameters
                        transcribe_kwargs = {
                            "beam_size": 5,
                            # Anti-hallucination parameters from settings
                            "no_speech_threshold": self.settings.get("no_speech_threshold", 0.6),
                            "log_prob_threshold": self.settings.get("logprob_threshold", -1.0),  # Note: faster-whisper uses log_prob_threshold
                            "compression_ratio_threshold": self.settings.get("compression_ratio_threshold", 2.4),
                            "condition_on_previous_text": self.settings.get("condition_on_previous_text", False),
                        }
                        if language and language != "auto":
                            transcribe_kwargs["language"] = language

                        segments, info = self.model.transcribe(audio, **transcribe_kwargs)
                        
                        # Convert to WhisperX format and filter hallucinations
                        result_segments = []
                        for segment in segments:
                            # Apply hallucination filtering if enabled
                            if self.settings.get("filter_hallucinations", True):
                                # Skip segments with high no_speech probability (likely hallucinations)
                                if segment.no_speech_prob > 0.9:
                                    logger.debug(f"Filtered segment with high no_speech_prob={segment.no_speech_prob:.2f}: {segment.text[:50]}")
                                    continue

                                # Skip segments with known hallucination patterns
                                text_lower = segment.text.lower().strip()
                                hallucination_patterns = [
                                    "untertitel",  # German subtitle markers
                                    "amara.org",
                                    "zdf",
                                    "das war's für heute",  # YouTube outro patterns
                                    "lasst einen daumen",
                                    "abonniert meinen kanal",
                                    "bis zum nächsten mal",
                                    "sous-titres",  # French
                                    "soustitreur.com",
                                    "copyright wdr",
                                    "im auftrag des",
                                ]

                                if any(pattern in text_lower for pattern in hallucination_patterns):
                                    logger.debug(f"Filtered hallucinated segment: {segment.text[:50]}")
                                    continue

                            result_segments.append({
                                "start": segment.start,
                                "end": segment.end,
                                "text": segment.text,
                                "no_speech_prob": segment.no_speech_prob,  # Include for debugging
                                "avg_logprob": segment.avg_logprob,  # Include for debugging
                            })

                        return {
                            "segments": result_segments,
                            "language": info.language
                        }

                self.model = NoVADWhisperModel(whisper_model, self.transcription_settings)
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
                # Always load alignment model on CPU to avoid hanging
                self.align_model, self.metadata = whisperx.load_align_model(
                    language_code=language_code, 
                    device="cpu"
                )
            except Exception as e:
                logger.error(f"Failed to load alignment model: {e}")
                raise
    
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
            transcribe_language = self.language if self.language != "auto" else None
            result = self.model.transcribe(audio, batch_size=batch_size, language=transcribe_language)
            
            # Store detected language for alignment
            self.detected_language = result.get("language", "en")
            logger.info(f"Detected/used language: {self.detected_language}")
            
            # Align whisper output
            self.load_align_model(self.detected_language)
            
            # Always use CPU for alignment to avoid CUDA memory issues
            result = whisperx.align(
                result["segments"],
                self.align_model,
                self.metadata,
                audio,
                "cpu",
                return_char_alignments=False
            )

            # Post-alignment filtering: Remove segments that failed alignment
            # These often indicate hallucinations
            if self.transcription_settings.get("filter_hallucinations", True):
                filtered_segments = []
                for segment in result.get("segments", []):
                    # Check if alignment succeeded (has word-level alignment)
                    words = segment.get("words", [])

                    # If segment has no words or failed alignment, it might be a hallucination
                    # But only filter if it also matches hallucination patterns
                    if len(words) == 0:
                        text_lower = segment.get("text", "").lower().strip()
                        hallucination_patterns = [
                            "untertitel", "amara.org", "zdf",
                            "das war's für heute", "lasst einen daumen",
                            "abonniert meinen kanal", "bis zum nächsten mal",
                            "sous-titres", "soustitreur.com",
                            "copyright wdr", "im auftrag des",
                        ]

                        if any(pattern in text_lower for pattern in hallucination_patterns):
                            logger.info(f"Filtered failed alignment segment with hallucination pattern: {segment.get('text', '')[:50]}")
                            continue

                    filtered_segments.append(segment)

                result["segments"] = filtered_segments

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

def transcribe_session(tracks: List[Dict], speaker_mapping: Dict[str, any], language: str = "auto") -> str:
    """
    Transcribe all tracks in a session and merge into a single transcript

    Args:
        tracks: List of track dictionaries with file paths
        speaker_mapping: Mapping of track IDs to speaker info (dict with playerName, characterName, pronouns)
                        or legacy string format (just speaker name)
        language: Language code for transcription ("auto" for auto-detection)

    Returns:
        Merged transcript as formatted string
    """
    # Import config manager to get model settings
    from storage.config_manager import ConfigManager
    config_manager = ConfigManager()

    # Get model size and transcription settings from config
    model_size = config_manager.get_whisper_model()
    transcription_settings = config_manager.get_transcription_settings()

    transcriber = WhisperTranscriber(
        model_size=model_size,
        language=language,
        transcription_settings=transcription_settings
    )
    all_segments = []

    try:
        for track in tracks:
            track_id = track["id"]
            file_path = track["file_path"]

            # Handle both new format (dict with playerName, characterName, pronouns)
            # and legacy format (just string speaker name)
            speaker_info = speaker_mapping.get(track_id, f"Speaker_{track_id}")

            if isinstance(speaker_info, dict):
                # New format: use character name if available, otherwise player name
                character_name = speaker_info.get("characterName", "").strip()
                player_name = speaker_info.get("playerName", f"Speaker_{track_id}")
                speaker_name = character_name if character_name else player_name

                # Store full speaker info in segment for later use
                speaker_metadata = {
                    "player_name": player_name,
                    "character_name": character_name,
                    "pronouns": speaker_info.get("pronouns", "")
                }
            else:
                # Legacy format: just a string
                speaker_name = speaker_info
                speaker_metadata = {
                    "player_name": speaker_info,
                    "character_name": "",
                    "pronouns": ""
                }

            logger.info(f"Transcribing {file_path} for {speaker_name}")

            result = transcriber.transcribe_file(file_path, speaker_name)

            # Add segments to combined list with metadata
            for segment in result["segments"]:
                segment["track_id"] = track_id
                segment["speaker_metadata"] = speaker_metadata
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
        segments: List of transcript segments with timestamps and speaker metadata

    Returns:
        Formatted transcript string
    """
    transcript_lines = []
    transcript_lines.append("# D&D Session Transcript\n")

    # Extract and add speaker/character information at the beginning
    speaker_info_map = {}
    for segment in segments:
        if "speaker_metadata" in segment:
            speaker_name = segment.get("speaker", "Unknown")
            metadata = segment["speaker_metadata"]

            if speaker_name not in speaker_info_map:
                player_name = metadata.get("player_name", "")
                character_name = metadata.get("character_name", "")
                pronouns = metadata.get("pronouns", "")

                # Build speaker info display
                info_parts = []
                if character_name:
                    info_parts.append(f"Character: {character_name}")
                if player_name and player_name != character_name:
                    info_parts.append(f"Player: {player_name}")
                if pronouns:
                    info_parts.append(f"Pronouns: {pronouns}")

                if info_parts:
                    speaker_info_map[speaker_name] = " | ".join(info_parts)

    # Add speaker info section if we have any metadata
    if speaker_info_map:
        transcript_lines.append("## Participants\n")
        for speaker_name, info in speaker_info_map.items():
            transcript_lines.append(f"- **{speaker_name}**: {info}")
        transcript_lines.append("\n## Transcript\n")

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