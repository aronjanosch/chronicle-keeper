"""MLX-Audio transcription provider for Apple Silicon.

Unified provider that supports multiple STT models through the mlx-audio library:
- Whisper (99+ languages)
- Parakeet (25 EU languages)
- Qwen3-ASR (multilingual)
- VibeVoice-ASR (9B model with built-in diarization)
"""

from __future__ import annotations

import json
from pathlib import Path

from app.logging_config import get_logger
from app.services.transcription.base import (
    TranscriptionProvider,
    TranscriptionResult,
    TranscriptionSegment,
    speaker_label,
)

log = get_logger("mlx-audio")


def _parse_result(result) -> list[dict]:
    """Parse mlx-audio STT result into a list of segment dicts.

    Handles different result formats:
    - Parakeet models: result.sentences (objects with .text, .start, .end)
    - Whisper models: result.segments (list of dicts with text, start, end)
    - VibeVoice-ASR: result.segments with speaker_id, start_time, end_time, text
    - Fallback: result.text as a single segment
    """
    segments = []

    # Parakeet models return .sentences
    if hasattr(result, "sentences") and result.sentences:
        for sentence in result.sentences:
            text = sentence.text.strip() if hasattr(sentence, "text") else str(sentence).strip()
            if not text:
                continue
            segments.append({
                "text": text,
                "start": getattr(sentence, "start", 0.0),
                "end": getattr(sentence, "end", 0.0),
                "speaker_id": getattr(sentence, "speaker_id", None),
            })
        return segments

    # Whisper / VibeVoice models return .segments
    if hasattr(result, "segments") and result.segments:
        for seg in result.segments:
            if isinstance(seg, dict):
                text = seg.get("text", "").strip()
                if not text:
                    continue
                # VibeVoice uses start_time/end_time, Whisper uses start/end
                segments.append({
                    "text": text,
                    "start": seg.get("start_time", seg.get("start", 0.0)),
                    "end": seg.get("end_time", seg.get("end", 0.0)),
                    "speaker_id": seg.get("speaker_id", seg.get("speaker", None)),
                })
            else:
                # Object-style segments
                text = getattr(seg, "text", "").strip()
                if not text:
                    continue
                start = getattr(seg, "start_time", getattr(seg, "start", 0.0))
                end = getattr(seg, "end_time", getattr(seg, "end", 0.0))
                speaker_id = getattr(seg, "speaker_id", getattr(seg, "speaker", None))
                segments.append({
                    "text": text,
                    "start": start,
                    "end": end,
                    "speaker_id": speaker_id,
                })
        return segments

    # VibeVoice may return JSON text that needs parsing
    if hasattr(result, "text") and result.text:
        text = result.text.strip()
        try:
            parsed = json.loads(text)
            if isinstance(parsed, list):
                for item in parsed:
                    content = item.get("Content", item.get("text", "")).strip()
                    if not content:
                        continue
                    segments.append({
                        "text": content,
                        "start": item.get("Start", item.get("start", 0.0)),
                        "end": item.get("End", item.get("end", 0.0)),
                        "speaker_id": item.get("Speaker", item.get("speaker_id", None)),
                    })
                return segments
        except (json.JSONDecodeError, TypeError):
            pass

        # Plain text fallback
        if text:
            segments.append({
                "text": text,
                "start": 0.0,
                "end": 0.0,
                "speaker_id": None,
            })

    return segments


class MLXAudioProvider(TranscriptionProvider):
    """Unified MLX-Audio transcription provider for Apple Silicon.

    Supports multiple STT model architectures through the mlx-audio library:
    - Whisper: General-purpose, 99+ languages
    - Parakeet: NVIDIA's accurate STT, 25 EU languages
    - Qwen3-ASR: Alibaba's multilingual ASR
    - VibeVoice-ASR: Microsoft's 9B model with built-in diarization
    """

    def __init__(
        self,
        model_name: str = "mlx-community/whisper-large-v3-turbo-asr-fp16",
        **kwargs,
    ):
        self._model_name = model_name
        self._model = None

    @property
    def name(self) -> str:
        return "mlx-audio"

    @property
    def supports_diarization(self) -> bool:
        # VibeVoice-ASR has built-in diarization
        return "vibevoice" in self._model_name.lower()

    def _load_model(self):
        """Lazy-load the mlx-audio STT model."""
        if self._model is None:
            from mlx_audio.stt.utils import load

            log.info("Loading mlx-audio STT model: %s", self._model_name)
            self._model = load(self._model_name)
        return self._model

    def _transcribe_file(self, audio_path: str | Path, language: str) -> list[dict]:
        """Transcribe a single audio file and return parsed segments."""
        model = self._load_model()

        log.info("Transcribing: %s", audio_path)

        # Use chunked processing for Parakeet to avoid OOM on long files.
        # Parakeet processes the full audio at once by default (chunk_duration=None),
        # which exceeds Metal buffer limits on long recordings.
        if "parakeet" in self._model_name.lower():
            result = model.generate(str(audio_path), chunk_duration=600.0)
        else:
            result = model.generate(str(audio_path))
        return _parse_result(result)

    def transcribe_session(
        self,
        session_path: str | Path,
        tracks: list[dict],
        speakers: list[dict] | None = None,
        language: str = "en",
    ) -> TranscriptionResult:
        """Transcribe each track and label by speaker mapping."""
        session_path = Path(session_path)
        if not session_path.is_dir():
            raise FileNotFoundError(f"Session folder not found: {session_path}")

        speakers = speakers or []
        speaker_map = {item.get("track_id"): item for item in speakers}

        all_segments: list[TranscriptionSegment] = []

        try:
            for track in tracks:
                track_path = Path(track["file_path"])
                if not track_path.exists():
                    log.warning("Track file not found, skipping: %s", track_path)
                    continue

                label = speaker_label(speaker_map.get(track["id"]), track["id"])
                parsed = self._transcribe_file(track_path, language)

                for seg in parsed:
                    all_segments.append(
                        TranscriptionSegment(
                            text=seg["text"],
                            start=seg["start"],
                            end=seg["end"],
                            speaker=label,
                            source=track["id"],
                        )
                    )
        finally:
            if self._model is not None:
                del self._model
                self._model = None
                # Do not call gc.collect() here: it can block for a long time after
                # large transcriptions (Metal/GPU cleanup), which prevents the HTTP
                # response from being sent. The model will be reclaimed when the
                # method returns and references go out of scope.

        all_segments.sort(key=lambda s: s.start)
        log.info("Transcription complete: %d segments", len(all_segments))
        return TranscriptionResult(
            segments=all_segments,
            language=language,
            provider=self.name,
            metadata={"model": self._model_name},
        )
