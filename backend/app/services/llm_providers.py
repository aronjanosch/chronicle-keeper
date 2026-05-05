"""Provider registry and provider_keys DB operations."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from app.storage.db import get_connection

PROVIDER_REGISTRY: list[dict[str, Any]] = [
    {
        "id": "ollama",
        "name": "Ollama (local)",
        "litellm_prefix": "ollama",
        "needs_key": False,
        "default_api_base": "http://localhost:11434",
        "models": [
            "llama3.3", "llama3.2", "llama3.1", "llama3",
            "mistral", "mistral-nemo", "mixtral",
            "gemma3", "gemma2",
            "phi4", "phi4-mini", "phi3",
            "qwen3", "qwen2.5", "qwen2.5-coder",
            "deepseek-r1", "deepseek-r1:14b", "deepseek-r1:32b",
            "command-r",
        ],
        "default_model": "llama3.2",
    },
    {
        "id": "openai",
        "name": "OpenAI",
        "litellm_prefix": "openai",
        "needs_key": True,
        "default_api_base": None,
        "models": [
            "gpt-4.1", "gpt-4.1-mini", "gpt-4.1-nano",
            "gpt-4o", "gpt-4o-mini",
            "o3", "o3-mini", "o4-mini",
        ],
        "default_model": "gpt-4.1-mini",
    },
    {
        "id": "anthropic",
        "name": "Anthropic",
        "litellm_prefix": "anthropic",
        "needs_key": True,
        "default_api_base": None,
        "models": [
            "claude-opus-4-7",
            "claude-sonnet-4-6",
            "claude-haiku-4-5-20251001",
        ],
        "default_model": "claude-sonnet-4-6",
    },
    {
        "id": "gemini",
        "name": "Google Gemini",
        "litellm_prefix": "gemini",
        "needs_key": True,
        "default_api_base": None,
        "models": [
            "gemini-2.5-flash", "gemini-2.5-pro",
            "gemini-2.0-flash", "gemini-2.0-flash-lite",
        ],
        "default_model": "gemini-2.5-flash",
    },
    {
        "id": "minimax",
        "name": "MiniMax",
        "litellm_prefix": "openai",
        "needs_key": True,
        "default_api_base": "https://api.minimax.io/v1",
        "models": ["MiniMax-M1", "MiniMax-Text-01"],
        "default_model": "MiniMax-M1",
    },
    {
        "id": "groq",
        "name": "Groq",
        "litellm_prefix": "groq",
        "needs_key": True,
        "default_api_base": None,
        "models": [
            "llama-3.3-70b-versatile",
            "llama-3.1-8b-instant",
            "gemma2-9b-it",
            "mixtral-8x7b-32768",
            "compound-beta",
        ],
        "default_model": "llama-3.3-70b-versatile",
    },
    {
        "id": "mistral",
        "name": "Mistral",
        "litellm_prefix": "mistral",
        "needs_key": True,
        "default_api_base": None,
        "models": [
            "mistral-large-latest", "mistral-small-latest",
            "mistral-medium-latest", "open-mixtral-8x22b",
            "codestral-latest",
        ],
        "default_model": "mistral-large-latest",
    },
    {
        "id": "deepseek",
        "name": "DeepSeek",
        "litellm_prefix": "deepseek",
        "needs_key": True,
        "default_api_base": None,
        "models": ["deepseek-chat", "deepseek-reasoner"],
        "default_model": "deepseek-chat",
    },
    {
        "id": "together",
        "name": "Together AI",
        "litellm_prefix": "together_ai",
        "needs_key": True,
        "default_api_base": None,
        "models": [
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "meta-llama/Llama-4-Scout-17B-16E-Instruct",
            "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8",
            "Qwen/Qwen2.5-72B-Instruct-Turbo",
            "mistralai/Mixtral-8x7B-Instruct-v0.1",
        ],
        "default_model": "meta-llama/Llama-3.3-70B-Instruct-Turbo",
    },
    {
        "id": "cohere",
        "name": "Cohere",
        "litellm_prefix": "cohere",
        "needs_key": True,
        "default_api_base": None,
        "models": ["command-a-03-2025", "command-r-plus", "command-r", "command-r7b-12-2024"],
        "default_model": "command-a-03-2025",
    },
    {
        "id": "perplexity",
        "name": "Perplexity",
        "litellm_prefix": "perplexity",
        "needs_key": True,
        "default_api_base": None,
        "models": [
            "sonar-pro", "sonar", "sonar-reasoning-pro", "sonar-reasoning",
            "r1-1776",
        ],
        "default_model": "sonar-pro",
    },
]

REGISTRY_BY_ID: dict[str, dict[str, Any]] = {p["id"]: p for p in PROVIDER_REGISTRY}
REGISTRY_IDS: frozenset[str] = frozenset(REGISTRY_BY_ID)

# Alias used by main.py
PROVIDER_REGISTRY_BY_ID = REGISTRY_BY_ID


def get_provider_key(provider_id: str) -> dict[str, str] | None:
    """Return saved api_key, api_base and default_model for a provider, or None if not found."""
    with get_connection() as conn:
        row = conn.execute(
            "SELECT api_key, api_base, default_model FROM provider_keys WHERE provider_id = ?",
            (provider_id,),
        ).fetchone()
    if row is None:
        return None
    return {"api_key": row["api_key"], "api_base": row["api_base"], "default_model": row["default_model"]}


def upsert_provider_key(provider_id: str, *, api_key: str, api_base: str, default_model: str = "") -> None:
    """Insert or update api_key, api_base and default_model for a provider."""
    now = datetime.now(timezone.utc).isoformat()
    with get_connection() as conn:
        conn.execute(
            """
            INSERT INTO provider_keys (provider_id, api_key, api_base, default_model, updated_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(provider_id) DO UPDATE SET
                api_key       = excluded.api_key,
                api_base      = excluded.api_base,
                default_model = excluded.default_model,
                updated_at    = excluded.updated_at
            """,
            (provider_id, api_key, api_base, default_model, now),
        )
        conn.commit()


def list_provider_keys() -> dict[str, dict[str, str]]:
    """Return all saved provider keys as {provider_id: {api_key, api_base, default_model}}."""
    with get_connection() as conn:
        rows = conn.execute("SELECT provider_id, api_key, api_base, default_model FROM provider_keys").fetchall()
    return {
        row["provider_id"]: {
            "api_key": row["api_key"],
            "api_base": row["api_base"],
            "default_model": row["default_model"],
        }
        for row in rows
    }
