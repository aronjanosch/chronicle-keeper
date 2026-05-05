"""ONNX-ASR transcription provider for cross-platform (CPU/NVIDIA GPU).

Supports multiple STT model architectures through the onnx-asr library:
- Parakeet (NVIDIA's fast & accurate STT)
- Canary (NVIDIA's multilingual)
- Whisper (OpenAI's general-purpose)
"""

from __future__ import annotations

import gc
import shutil
import subprocess
from pathlib import Path

from app.logging_config import get_logger
from app.services.transcription.base import (
    TranscriptionProvider,
    TranscriptionResult,
    TranscriptionSegment,
    speaker_label,
)

log = get_logger("onnx-asr")


class OnnxAsrProvider(TranscriptionProvider):
    """ONNX-ASR transcription provider using ONNX Runtime.

    Supports multiple STT model architectures:
    - Parakeet: NVIDIA's fast & accurate STT
    - Canary: NVIDIA's best accuracy, multilingual
    - Whisper: OpenAI's general-purpose, 99+ languages
    """

    def __init__(
        self,
        model_name: str = "nemo-parakeet-tdt-0.6b-v3",
        **kwargs,
    ):
        self._model_name = model_name
        self._model = None

    @property
    def name(self) -> str:
        return "onnx-asr"

    @property
    def supports_diarization(self) -> bool:
        return False

    def _load_model(self):
        """Lazy-load the onnx-asr model with VAD for long audio segmentation."""
        if self._model is None:
            import onnx_asr

            log.info("Loading onnx-asr model: %s", self._model_name)
            base_model = onnx_asr.load_model(self._model_name)
            vad = onnx_asr.load_vad("silero")
            self._model = base_model.with_vad(vad)
        return self._model

    def _load_waveform(self, audio_path: Path) -> tuple:
        """Decode an audio file to mono 16 kHz float32 PCM via ffmpeg.

        onnx_asr's built-in reader only handles PCM WAV. We use ffmpeg
        instead because (a) Craig Bot produces streamed FLAC files that
        libsndfile/soundfile can't seek in ("psf_fseek() failed"), and
        (b) ffmpeg is already required by WhisperX. Resampling to 16 kHz
        here also matches Parakeet/Canary/Whisper's native rate, so
        onnx_asr's resampler becomes a no-op.
        """
        import numpy as np

        if shutil.which("ffmpeg") is None:
            raise RuntimeError(
                "ffmpeg not found on PATH; required to decode audio for onnx-asr"
            )

        target_sr = 16_000
        cmd = [
            "ffmpeg",
            "-nostdin",
            "-hide_banner",
            "-loglevel", "error",
            "-i", str(audio_path),
            "-f", "f32le",
            "-ac", "1",
            "-ar", str(target_sr),
            "-",
        ]
        proc = subprocess.run(cmd, check=False, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if proc.returncode != 0:
            msg = proc.stderr.decode("utf-8", errors="replace").strip() or "unknown error"
            raise RuntimeError(f"ffmpeg failed to decode {audio_path}: {msg}")

        waveform = np.frombuffer(proc.stdout, dtype=np.float32)
        return np.ascontiguousarray(waveform), target_sr

    def _transcribe_file(self, audio_path: str | Path, language: str) -> list[dict]:
        """Transcribe a single audio file and return segment dicts."""
        model = self._load_model()

        log.info("Transcribing: %s", audio_path)

        waveform, sample_rate = self._load_waveform(Path(audio_path))

        segments = []
        for segment in model.recognize(waveform, sample_rate=sample_rate):
            text = segment.text.strip() if hasattr(segment, "text") else str(segment).strip()
            if not text:
                continue
            segments.append({
                "text": text,
                "start": getattr(segment, "start", 0.0),
                "end": getattr(segment, "end", 0.0),
            })

        return segments

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
                gc.collect()

        all_segments.sort(key=lambda s: s.start)
        return TranscriptionResult(
            segments=all_segments,
            language=language,
            provider=self.name,
            metadata={"model": self._model_name},
        )
