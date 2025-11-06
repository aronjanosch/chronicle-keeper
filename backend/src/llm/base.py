"""
Base LLM client with shared parsing and prompt building logic.

This abstract base class eliminates duplication between Ollama and Gemini clients
by providing common functionality for response parsing and prompt composition.
"""

from abc import ABC, abstractmethod
from typing import Dict, Any, List, Optional
import json
import re
import logging

from prompts import (
    build_enhanced_prompt,
    get_empty_metadata,
    get_metadata_analysis_prompt,
    RESPONSE_SEPARATOR
)
from constants import TEMPERATURE_METADATA

logger = logging.getLogger(__name__)


class BaseLLMClient(ABC):
    """
    Abstract base class for LLM clients.

    Provides shared functionality for:
    - Prompt building using centralized prompts module
    - Response parsing (separator-based and fallback strategies)
    - JSON extraction from LLM responses
    - Metadata structure handling
    """

    @abstractmethod
    def _call_llm(self, prompt: str, **kwargs) -> str:
        """
        Call the LLM API and return the raw response text.

        This method must be implemented by subclasses to handle
        the specific API call for their LLM service.

        Args:
            prompt: The full prompt to send
            **kwargs: Additional parameters specific to the LLM

        Returns:
            Raw response text from the LLM
        """
        pass

    def generate_summary_with_metadata(self, transcript: str, system_prompt: str) -> Dict[str, Any]:
        """
        Generate session summary and metadata suggestions in single optimized call.

        Args:
            transcript: The session transcript
            system_prompt: System prompt for summarization

        Returns:
            Dictionary containing summary and metadata suggestions
        """
        # Build the enhanced prompt using centralized prompt builder
        enhanced_prompt = build_enhanced_prompt(system_prompt, transcript)

        # Call the LLM (implemented by subclass)
        full_response = self._call_llm(enhanced_prompt, temperature=TEMPERATURE_METADATA)

        # Parse using the explicit separator
        metadata, summary = self._parse_with_separator(full_response)

        return {
            "summary": summary,
            "metadata": metadata
        }

    def analyze_metadata(self, transcript: str) -> Dict[str, List[str]]:
        """
        Analyze transcript and suggest metadata tags, characters, and locations.

        Args:
            transcript: The session transcript

        Returns:
            Dictionary with suggested metadata
        """
        # Build metadata analysis prompt
        analysis_prompt = get_metadata_analysis_prompt(transcript)

        try:
            # Call the LLM with lower temperature for consistent JSON
            result_text = self._call_llm(analysis_prompt, temperature=TEMPERATURE_METADATA)

            # Try to extract JSON from the response
            metadata = self._extract_json_from_text(result_text)
            if metadata:
                return metadata

            # If extraction fails, return empty structure
            logger.warning("Could not parse JSON from metadata analysis response")
            return get_empty_metadata()

        except Exception as e:
            logger.error(f"Error analyzing metadata: {e}")
            return get_empty_metadata()

    def _parse_with_separator(self, full_response: str) -> tuple:
        """
        Parse response using the explicit separator.

        Args:
            full_response: The full LLM response

        Returns:
            Tuple of (metadata_dict, summary_text)
        """
        # First try the explicit separator
        if RESPONSE_SEPARATOR in full_response:
            parts = full_response.split(RESPONSE_SEPARATOR)
            summary = parts[0].strip()
            metadata_text = parts[1].strip()

            metadata = self._extract_json_from_text(metadata_text)
            if metadata:
                return metadata, summary

        # Fall back to the existing parsing strategies
        return self._parse_summary_and_metadata(full_response)

    def _parse_summary_and_metadata(self, full_response: str) -> tuple:
        """
        Parse summary and metadata from LLM response using multiple strategies.

        Args:
            full_response: The full response from the LLM

        Returns:
            Tuple of (metadata_dict, summary_text)
        """
        empty_metadata = get_empty_metadata()

        # Strategy 1: Look for METADATA_JSON: delimiter
        if "METADATA_JSON:" in full_response:
            parts = full_response.split("METADATA_JSON:")
            summary = parts[0].strip()
            metadata_text = parts[1].strip()

            metadata = self._extract_json_from_text(metadata_text)
            if metadata:
                return metadata, summary

        # Strategy 2: Look for ```json code blocks
        json_block_pattern = r'```json\s*\n(.*?)\n```'
        matches = re.findall(json_block_pattern, full_response, re.DOTALL)
        if matches:
            # Take the last JSON block (most likely to be metadata)
            metadata_text = matches[-1].strip()
            metadata = self._extract_json_from_text(metadata_text)
            if metadata:
                # Remove the JSON block from summary
                summary = re.sub(json_block_pattern, '', full_response, flags=re.DOTALL).strip()
                # Clean up any "**Metadata JSON:**" headers
                summary = re.sub(r'\*\*Metadata JSON:\*\*\s*', '', summary).strip()
                return metadata, summary

        # Strategy 3: Look for any JSON object in the response
        json_pattern = r'\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}'
        json_matches = re.findall(json_pattern, full_response, re.DOTALL)
        for json_text in reversed(json_matches):  # Check from end, more likely to be metadata
            metadata = self._extract_json_from_text(json_text)
            if metadata and any(metadata.values()):  # Check if metadata has actual content
                # Remove the JSON from summary
                summary = full_response.replace(json_text, '').strip()
                # Clean up any headers
                summary = re.sub(r'\*\*Metadata JSON:\*\*\s*', '', summary).strip()
                return metadata, summary

        # No metadata found, return empty metadata and full response as summary
        logger.warning("Could not extract metadata from LLM response, using empty metadata")
        return empty_metadata, full_response

    def _extract_json_from_text(self, text: str) -> Optional[Dict[str, List[str]]]:
        """
        Extract and parse JSON from text, handling various formats.

        Args:
            text: Text that should contain JSON

        Returns:
            Parsed metadata dictionary or None if parsing fails
        """
        try:
            # Clean up the text
            text = text.strip()

            # Remove markdown formatting
            text = text.replace('```json', '').replace('```', '').strip()

            # If text doesn't start with {, try to find JSON object
            if not text.startswith('{'):
                json_start = text.find('{')
                if json_start != -1:
                    text = text[json_start:]

            # If text doesn't end with }, try to find end of JSON object
            if not text.endswith('}'):
                json_end = text.rfind('}')
                if json_end != -1:
                    text = text[:json_end + 1]

            # Parse JSON
            metadata = json.loads(text)

            # Validate that it has the expected structure
            expected_keys = {"suggested_tags", "mentioned_characters", "mentioned_locations",
                           "session_tone", "key_events"}
            if isinstance(metadata, dict) and any(key in metadata for key in expected_keys):
                # Ensure all expected keys exist with empty lists as defaults
                for key in expected_keys:
                    if key not in metadata:
                        metadata[key] = []
                    elif not isinstance(metadata[key], list):
                        metadata[key] = []

                return metadata

        except (json.JSONDecodeError, ValueError, TypeError) as e:
            logger.debug(f"Failed to parse JSON from text: {e}")

        return None
