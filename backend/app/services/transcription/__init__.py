"""Transcription provider registry."""

from __future__ import annotations

import platform

from app.services.transcription.mlx_audio_provider import MLXAudioProvider
from app.services.transcription.onnx_asr_provider import OnnxAsrProvider

PROVIDERS = {
    "mlx-audio": {
        "factory": MLXAudioProvider,
        "display_name": "MLX Audio",
        "description": "Apple Silicon optimized - multiple STT models via mlx-audio",
        "supports_diarization": False,
        "default_model": "mlx-community/parakeet-tdt-0.6b-v3",
        "models": [
            {
                "id": "mlx-community/parakeet-tdt-0.6b-v3",
                "name": "Parakeet TDT 0.6B v3",
                "description": "NVIDIA's accurate STT, 25 EU languages (recommended default)",
            },
            {
                "id": "mlx-community/whisper-large-v3-turbo-asr-fp16",
                "name": "Whisper Large v3 Turbo",
                "description": "Fast and accurate, 99+ languages",
            },
            {
                "id": "mlx-community/whisper-large-v3-asr-fp16",
                "name": "Whisper Large v3",
                "description": "Best Whisper accuracy, 99+ languages",
            },
            {
                "id": "mlx-community/parakeet-tdt-0.6b-v2",
                "name": "Parakeet TDT 0.6B v2",
                "description": "NVIDIA's accurate STT, English only",
            },
            {
                "id": "mlx-community/Qwen3-ASR-1.7B-8bit",
                "name": "Qwen3-ASR 1.7B (8-bit)",
                "description": "Alibaba's multilingual ASR",
            },
            {
                "id": "mlx-community/Qwen3-ASR-0.6B-8bit",
                "name": "Qwen3-ASR 0.6B (8-bit)",
                "description": "Alibaba's smaller multilingual ASR",
            },
            {
                "id": "mlx-community/VibeVoice-ASR-bf16",
                "name": "VibeVoice-ASR 9B",
                "description": "Microsoft's 9B model with built-in diarization",
            },
        ],
    },
    "onnx-asr": {
        "factory": OnnxAsrProvider,
        "display_name": "ONNX ASR",
        "description": "Cross-platform (CPU/NVIDIA GPU) - multiple STT models via ONNX Runtime",
        "supports_diarization": False,
        "default_model": "nemo-parakeet-tdt-0.6b-v3",
        "models": [
            {
                "id": "nemo-parakeet-tdt-0.6b-v3",
                "name": "Parakeet TDT 0.6B v3",
                "description": "NVIDIA's fast & accurate, 25 EU languages (recommended)",
            },
            {
                "id": "nemo-parakeet-tdt-0.6b-v2",
                "name": "Parakeet TDT 0.6B v2",
                "description": "NVIDIA's STT, English only",
            },
            {
                "id": "nemo-canary-1b-v2",
                "name": "Canary 1B v2",
                "description": "NVIDIA's best accuracy, multilingual",
            },
            {
                "id": "whisper-base",
                "name": "Whisper Base",
                "description": "Fast, lower accuracy",
            },
            {
                "id": "onnx-community/whisper-large-v3-turbo",
                "name": "Whisper Large v3 Turbo",
                "description": "Good balance of speed/accuracy, 99+ languages",
            },
        ],
    },
}


def get_default_provider() -> str:
    """Auto-select the best provider for the current platform."""
    if platform.system() == "Darwin" and platform.machine() == "arm64":
        return "mlx-audio"
    return "onnx-asr"


def resolve_transcription_provider(preference: str | None) -> str:
    """Resolve stored preference (e.g. auto) to a concrete provider name."""
    pref = (preference or "auto").strip().lower()
    if pref in PROVIDERS:
        return pref
    return get_default_provider()


def get_provider(name: str, **kwargs):
    provider = PROVIDERS.get(name)
    if not provider:
        raise ValueError(f"Unknown provider: {name}")
    return provider["factory"](**kwargs)


def get_available_providers() -> list[dict]:
    """List providers with platform-recommended engine first in the list."""
    auto_first = get_default_provider()
    names = sorted(
        PROVIDERS.keys(),
        key=lambda n: (0 if n == auto_first else 1, n),
    )
    return [
        {
            "name": name,
            "display_name": PROVIDERS[name]["display_name"],
            "description": PROVIDERS[name]["description"],
            "supports_diarization": PROVIDERS[name]["supports_diarization"],
            "default_model": PROVIDERS[name]["default_model"],
            "models": PROVIDERS[name]["models"],
        }
        for name in names
    ]
