"""
Google Gemini LLM client for cloud-based model inference.

Handles Gemini API configuration, safety settings, and API calls.
Supports native structured output with JSON schema.
"""

from google import genai
import logging
from typing import Dict, Any
import os

from llm.base import BaseLLMClient
from models import SummaryResponse, get_empty_metadata
from prompts import build_enhanced_prompt, build_structured_prompt

logger = logging.getLogger(__name__)


class GeminiClient(BaseLLMClient):
    def __init__(self, api_key: str, model_name: str = "gemini-2.5-flash"):
        """
        Initialize Gemini client

        Args:
            api_key: Google Gemini API key
            model_name: Model to use (default: gemini-2.5-flash)
        """
        if not api_key:
            raise ValueError("Gemini API key is required")

        self.api_key = api_key
        self.model_name = model_name

        # Set API key as environment variable (required by new SDK)
        os.environ["GOOGLE_API_KEY"] = api_key
        
        # Initialize the client
        self.client = genai.Client(api_key=api_key)

    def _call_llm(self, prompt: str, **kwargs) -> str:
        """
        Call Gemini API and return the response text.

        Args:
            prompt: The full prompt to send
            **kwargs: Additional parameters (temperature, etc.)

        Returns:
            Raw response text from Gemini
        """
        # Extract optional parameters
        temperature = kwargs.get('temperature', 0.7)

        try:
            response = self.client.models.generate_content(
                model=self.model_name,
                contents=prompt,
                config={
                    "temperature": temperature,
                    "top_p": 0.9,
                    "top_k": 40,
                    # No max_output_tokens - let the model decide appropriate length
                }
            )

            return response.text.strip()

        except Exception as e:
            logger.error(f"Error calling Gemini API: {e}")
            raise Exception(f"Failed to generate response: {str(e)}")

    def generate_summary(self, transcript: str, system_prompt: str) -> str:
        """
        Generate session summary using Gemini (without metadata extraction).

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

        Uses Gemini's native structured output feature with JSON schema
        to guarantee valid response format without manual parsing.

        Args:
            transcript: The session transcript
            system_prompt: System prompt for summarization
            language: Language code (en, de)

        Returns:
            Dictionary containing summary and metadata suggestions
        """
        # Build the structured prompt
        full_prompt = build_structured_prompt(system_prompt, transcript, language)

        # Get schema from Pydantic model
        schema = SummaryResponse.model_json_schema()

        try:
            # Use NEW SDK with response_json_schema (not response_schema)
            response = self.client.models.generate_content(
                model=self.model_name,
                contents=full_prompt,
                config={
                    "temperature": 0.7,
                    "response_mime_type": "application/json",
                    "response_json_schema": schema,  # NEW SDK uses response_json_schema
                }
            )

            # Parse the structured response using Pydantic
            result = SummaryResponse.model_validate_json(response.text)

            return {
                "summary": result.summary,
                "metadata": result.metadata.model_dump(),
                "raw_response": response.text
            }

        except Exception as e:
            logger.error(f"Error generating structured output with Gemini: {e}")
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

    def test_connection(self) -> dict:
        """Test API connection and return status"""
        try:
            # Simple test generation
            response = self.client.models.generate_content(
                model=self.model_name,
                contents="Hello"
            )
            return {
                "connected": True,
                "model": self.model_name,
                "error": None
            }
        except Exception as e:
            logger.error(f"Gemini connection test failed: {e}")
            return {
                "connected": False,
                "model": self.model_name,
                "error": str(e)
            }

    def estimate_tokens(self, text: str) -> int:
        """
        Estimate token count for input text using Gemini's count_tokens API.

        Args:
            text: Input text to estimate

        Returns:
            Estimated token count
        """
        try:
            result = self.client.models.count_tokens(
                model=self.model_name,
                contents=text
            )
            return result.total_tokens
        except Exception as e:
            logger.warning(f"Could not count tokens with Gemini API: {e}")
            # Fallback: rough estimation (~4 characters per token)
            return len(text) // 4

    def estimate_prompt_tokens(self, transcript: str, system_prompt: str) -> Dict[str, int]:
        """
        Estimate token count for complete prompt (system prompt + transcript).

        Args:
            transcript: The session transcript
            system_prompt: System prompt for summarization

        Returns:
            Dictionary with token breakdown
        """
        try:
            # Build the full prompt to get accurate count
            from prompts import build_structured_prompt
            full_prompt = build_structured_prompt(system_prompt, transcript, "en")

            total_tokens = self.estimate_tokens(full_prompt)

            return {
                "total_tokens": total_tokens,
                "method": "gemini_api",
                "accurate": True
            }
        except Exception as e:
            logger.warning(f"Could not estimate prompt tokens: {e}")
            # Fallback estimation
            total_estimate = len(transcript) // 4 + len(system_prompt) // 4
            return {
                "total_tokens": total_estimate,
                "method": "character_estimation",
                "accurate": False
            }

    def check_content_length(self, transcript: str, system_prompt: str) -> dict:
        """
        Check if content fits within model limits

        Args:
            transcript: The session transcript
            system_prompt: System prompt

        Returns:
            Dict with length info and recommendations
        """
        full_content = f"{system_prompt}\n\nTranscript:\n{transcript}"
        estimated_tokens = self.estimate_tokens(full_content)

        # Gemini 2.0 Flash has ~1M token context window
        max_tokens = 1000000
        max_recommended = int(max_tokens * 0.8)  # Leave room for output

        return {
            "estimated_tokens": estimated_tokens,
            "max_tokens": max_tokens,
            "within_limit": estimated_tokens < max_recommended,
            "usage_percent": (estimated_tokens / max_tokens) * 100,
            "recommendation": (
                "Content fits within limits" if estimated_tokens < max_recommended
                else "Content may be too long, consider splitting transcript"
            )
        }
