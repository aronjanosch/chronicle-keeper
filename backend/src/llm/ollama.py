"""
Ollama LLM client for local model inference.

Handles Ollama server management, model availability checks, and API calls.
Supports both legacy requests-based API and new ollama library with structured outputs.
"""

import requests
import subprocess
import time
import logging
from typing import Dict, Any

import ollama

from llm.base import BaseLLMClient
from models import SummaryResponse, get_empty_metadata
from prompts import build_enhanced_prompt, build_structured_prompt

logger = logging.getLogger(__name__)


class OllamaClient(BaseLLMClient):
    def __init__(self, base_url: str = "http://127.0.0.1:11434", model: str = "llama3.2", keep_alive: int | str = 0):
        """
        Initialize Ollama client

        Args:
            base_url: Ollama server URL
            model: Model name to use (default: llama3.2)
        """
        self.base_url = base_url
        self.model = model
        self.api_url = f"{base_url}/api"
        # keep_alive: 0 (immediate unload), or duration string like "5m"
        self.keep_alive = keep_alive

    def is_server_running(self) -> bool:
        """Check if Ollama server is running"""
        try:
            response = requests.get(f"{self.base_url}/api/tags", timeout=5)
            return response.status_code == 200
        except requests.RequestException:
            return False

    def ensure_server_running(self) -> bool:
        """Ensure Ollama server is running, try to start if not"""
        if self.is_server_running():
            return True

        logger.info("Ollama server not running, attempting to start...")
        try:
            # Try to start Ollama (assuming it's in PATH)
            subprocess.Popen(
                ["ollama", "serve"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL
            )

            # Wait for server to start
            for _ in range(10):  # Wait up to 10 seconds
                time.sleep(1)
                if self.is_server_running():
                    logger.info("Ollama server started successfully")
                    return True

            logger.error("Failed to start Ollama server within timeout")
            return False

        except FileNotFoundError:
            logger.error("Ollama not found in PATH. Please install Ollama.")
            return False
        except Exception as e:
            logger.error(f"Error starting Ollama: {e}")
            return False

    def is_model_available(self) -> bool:
        """Check if the specified model is available"""
        try:
            response = requests.get(f"{self.api_url}/tags")
            if response.status_code == 200:
                models = response.json().get("models", [])
                return any(model["name"].startswith(self.model) for model in models)
            return False
        except requests.RequestException:
            return False

    def pull_model(self) -> bool:
        """Pull the model if not available"""
        logger.info(f"Pulling model {self.model}...")
        try:
            response = requests.post(
                f"{self.api_url}/pull",
                json={"name": self.model},
                stream=True,
                timeout=300  # 5 minute timeout for model download
            )

            if response.status_code == 200:
                # Stream the download progress
                for line in response.iter_lines():
                    if line:
                        import json
                        data = json.loads(line)
                        if "status" in data:
                            logger.info(f"Model pull: {data['status']}")
                        if data.get("status") == "success":
                            return True

            return False

        except requests.RequestException as e:
            logger.error(f"Error pulling model: {e}")
            return False

    def ensure_model_ready(self) -> bool:
        """Ensure model is available, pull if necessary"""
        if not self.ensure_server_running():
            return False

        if self.is_model_available():
            return True

        return self.pull_model()

    def _call_llm(self, prompt: str, **kwargs) -> str:
        """
        Call Ollama API and return the response text.

        Args:
            prompt: The full prompt to send
            **kwargs: Additional parameters (temperature, etc.)

        Returns:
            Raw response text from Ollama
        """
        if not self.ensure_model_ready():
            raise Exception("Ollama model not available")

        # Extract optional parameters
        temperature = kwargs.get('temperature', 0.7)

        try:
            response = requests.post(
                f"{self.api_url}/generate",
                json={
                    "model": self.model,
                    "prompt": prompt,
                    "stream": False,
                    # ensure model is unloaded immediately after request to free VRAM
                    "keep_alive": self.keep_alive,
                    "options": {
                        "temperature": temperature,
                        "top_p": 0.9,
                        # No num_predict limit - let the model decide appropriate length
                    }
                },
                timeout=300  # 5 minute timeout (increased for longer outputs)
            )

            if response.status_code == 200:
                result = response.json()
                return result.get("response", "").strip()
            else:
                raise Exception(f"Ollama API error: {response.status_code}")

        except requests.RequestException as e:
            logger.error(f"Error calling Ollama API: {e}")
            raise Exception(f"Failed to generate response: {str(e)}")

    def generate_summary(self, transcript: str, system_prompt: str) -> str:
        """
        Generate session summary using Ollama (without metadata extraction).

        Args:
            transcript: The session transcript
            system_prompt: System prompt for summarization

        Returns:
            Generated summary
        """
        # Build simple prompt without metadata instructions
        from prompts import build_simple_prompt
        full_prompt = build_simple_prompt(system_prompt, transcript)

        return self._call_llm(full_prompt, temperature=0.7)

    def generate_summary_with_metadata(self, transcript: str, system_prompt: str, language: str = "en") -> Dict[str, Any]:
        """
        Generate session summary and metadata using structured output.

        Uses Ollama's native structured output feature with JSON schema
        to guarantee valid response format without manual parsing.

        Args:
            transcript: The session transcript
            system_prompt: System prompt for summarization
            language: Language code (en, de)

        Returns:
            Dictionary containing summary and metadata suggestions
        """
        if not self.ensure_model_ready():
            raise Exception("Ollama model not available")

        # Build the structured prompt
        full_prompt = build_structured_prompt(system_prompt, transcript, language)

        try:
            # Use ollama library with structured output
            response = ollama.chat(
                model=self.model,
                messages=[{
                    'role': 'user',
                    'content': full_prompt
                }],
                format=SummaryResponse.model_json_schema(),
                options={
                    'temperature': 0.3,  # Lower temperature for consistent structured output
                    # No num_predict limit - let the model decide appropriate length
                }
            )

            # Parse the structured response
            result = SummaryResponse.model_validate_json(response.message.content)

            return {
                "summary": result.summary,
                "metadata": result.metadata.model_dump(),
                "raw_response": response.message.content
            }

        except Exception as e:
            logger.error(f"Error generating structured output with Ollama: {e}")
            # Fallback to empty metadata
            logger.warning("Falling back to legacy parsing method")
            return self._fallback_generation(transcript, system_prompt, language)

    def _fallback_generation(self, transcript: str, system_prompt: str, language: str) -> Dict[str, Any]:
        """
        Fallback to legacy generation without structured output.

        Args:
            transcript: The session transcript
            system_prompt: System prompt for summarization
            language: Language code

        Returns:
            Dictionary with summary and empty metadata
        """
        try:
            from prompts import build_simple_prompt
            full_prompt = build_simple_prompt(system_prompt, transcript, language)
            summary = self._call_llm(full_prompt, temperature=0.7)

            return {
                "summary": summary,
                "metadata": get_empty_metadata(),
                "raw_response": summary
            }
        except Exception as e:
            logger.error(f"Fallback generation also failed: {e}")
            raise

    def estimate_tokens(self, text: str) -> int:
        """
        Estimate token count for input text.

        Uses character-based estimation since Ollama doesn't provide
        a built-in token counting API.

        Args:
            text: Input text to estimate

        Returns:
            Estimated token count
        """
        if not text:
            return 0

        # Character-based estimation: ~4 characters per token
        # Add 10% buffer for markdown and special characters
        estimated = int((len(text) // 4) * 1.1)

        return estimated

    def estimate_prompt_tokens(self, transcript: str, system_prompt: str) -> Dict[str, int]:
        """
        Estimate token count for complete prompt (system prompt + transcript).

        Args:
            transcript: The session transcript
            system_prompt: System prompt for summarization

        Returns:
            Dictionary with token breakdown
        """
        # Build the full prompt to get accurate count
        from prompts import build_structured_prompt
        full_prompt = build_structured_prompt(system_prompt, transcript, "en")

        total_tokens = self.estimate_tokens(full_prompt)

        return {
            "total_tokens": total_tokens,
            "method": "character_estimation",
            "accurate": False  # Character-based estimation is approximate
        }

    def test_connection(self) -> dict:
        """Test connection and return status info"""
        status = {
            "server_running": self.is_server_running(),
            "model_available": False,
            "error": None
        }

        if status["server_running"]:
            status["model_available"] = self.is_model_available()
        else:
            status["error"] = "Ollama server not running"

        return status
