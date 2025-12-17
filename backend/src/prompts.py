"""
Centralized prompt management for Chronicle Keeper.

This module contains all LLM prompts and metadata definitions.
Uses a two-call approach: (1) generate summary, (2) extract metadata.
"""

from typing import Dict

# ============================================================================
# SUMMARY GENERATION PROMPTS (Two-call approach)
# ============================================================================

# These prompts are used for Call 1: Generate the session summary
# Call 2 uses metadata extraction prompts (see METADATA section below)

BASE_PROMPTS: Dict[str, str] = {
    "en": """You are an RPG assistant for the GM. Create a concise, structured session summary.

CHARACTER NAMES: Always use CHARACTER NAMES from the transcript (not player names). Use correct pronouns.

FOCUS: Only GM-relevant details - what's important for continuity and next session? Be specific and concise.

STRUCTURE: Follow this Markdown structure with ### headings:

## Session Summary

### What Happened
[Opening scene, major plot developments, and how it ended - combined narrative flow]

### Combat & Challenges
[Enemy types, tactics, outcomes - MAX 3-4 SENTENCES, NO combat report!]

### NPCs & Relationships
[Who they met, what was discussed, promises made]

### Character Highlights
[Memorable moments, key decisions, critical fails/successes]

### Loot & Resources
- Items gained: [list items received]
- Resources used: [list resources spent]

## Looking Forward

### Decisions Made
[Major choices and reasoning]

### Open Threads
- Unresolved plots: [story hooks]
- Important NPCs: [who needs follow-up]
- Goals for next session: [concrete next steps]
- Commitments: [what the party owes/promised]""",

    "de": """Du bist ein RPG-Assistent für den Spielleiter. Erstelle eine prägnante, strukturierte Sitzungszusammenfassung.

CHARAKTERNAMEN: Verwende immer den CHARAKTERNAMEN aus dem Transkript (nicht Spielername). Nutze die korrekten Pronomen.

FOKUS: Nur GM-relevante Details - was ist wichtig für Kontinuität und nächste Sitzung? Sei konkret und prägnant.

STRUKTUR: Folge dieser Markdown-Struktur mit ### Überschriften:

## Sitzungszusammenfassung

### Was geschah
[Eröffnungsszene, große Entwicklungen und wie es endete - zusammenhängender Handlungsfluss]

### Kämpfe & Herausforderungen
[Gegnertypen, Taktiken, Ergebnisse - MAX 3-4 SÄTZE, KEIN Kampfbericht!]

### NPCs & Beziehungen
[Wen sie trafen, was besprochen wurde, gemachte Versprechen]

### Charakter-Highlights
[Denkwürdige Momente, wichtige Entscheidungen, kritische Fehlschläge/Erfolge]

### Beute & Ressourcen
- Erhaltene Gegenstände: [Liste erhaltene Items]
- Verwendete Ressourcen: [Liste verbrauchte Ressourcen]

## Vorausschau

### Getroffene Entscheidungen
[Wichtige Entscheidungen und Begründungen]

### Offene Fäden
- Ungelöste Handlungsstränge: [Story-Hooks]
- Wichtige NPCs: [wer braucht Follow-up]
- Ziele für nächste Sitzung: [konkrete nächste Schritte]
- Verpflichtungen: [was die Gruppe schuldet/versprochen hat]"""
}

# ============================================================================
# METADATA STRUCTURE & GUIDELINES
# ============================================================================

METADATA_JSON_STRUCTURE = {
    "suggested_tags": [],
    "mentioned_characters": [],
    "mentioned_locations": [],
    "session_tone": [],
    "key_events": []
}

METADATA_GUIDELINES: Dict[str, str] = {
    "en": """Metadata guidelines:
- suggested_tags: REQUIRED. List 3-5 tags. E.g., "Combat", "Social", "Exploration", "Mystery".
- mentioned_characters: List important PCs and NPCs. Use specific names.
- mentioned_locations: List specific locations visited or mentioned.
- session_tone: REQUIRED. List 1-3 mood descriptors. E.g., "Tense", "Humorous", "Dark".
- key_events: REQUIRED. List 3-5 short bullet points of major events.

Ensure ALL required fields are populated. Do not return empty lists for tags, tone, or events.""",
    "de": """Metadaten-Richtlinien:
- suggested_tags: ERFORDERLICH. Liste 3-5 Tags. Z.B. "Kampf", "Sozial", "Erkundung", "Mysterium".
- mentioned_characters: Liste wichtige SCs und NPCs. Verwende spezifische Namen.
- mentioned_locations: Liste spezifische besuchte oder erwähnte Orte.
- session_tone: ERFORDERLICH. Liste 1-3 Stimmungsbeschreibungen. Z.B. "Angespannt", "Humorvoll", "Düster".
- key_events: ERFORDERLICH. Liste 3-5 kurze Stichpunkte zu Hauptereignissen.

Stelle sicher, dass ALLE erforderlichen Felder ausgefüllt sind. Gib KEINE leeren Listen für Tags, Stimmung oder Ereignisse zurück."""
}

def get_metadata_guidelines(language: str = "en") -> str:
    """Get metadata guidelines in the specified language."""
    return METADATA_GUIDELINES.get(language, METADATA_GUIDELINES["en"])

# ============================================================================
# STRUCTURED OUTPUT INSTRUCTIONS (for metadata extraction)
# ============================================================================

STRUCTURED_OUTPUT_INSTRUCTIONS: Dict[str, str] = {
    "en": "Analyze the transcript and generate a structured summary. You MUST populate the metadata lists. 'suggested_tags', 'session_tone', and 'key_events' CANNOT be empty. If you are unsure, infer the best options from the context.",
    "de": "Analysiere das Transkript und erstelle eine strukturierte Zusammenfassung. Du MUSST die Metadaten-Listen füllen. 'suggested_tags', 'session_tone' und 'key_events' DÜRFEN NICHT leer sein. Wenn du unsicher bist, leite die besten Optionen aus dem Kontext ab."
}

TRANSCRIPT_LABELS: Dict[str, str] = {
    "en": "Transcript:",
    "de": "Transkript:"
}

# ============================================================================
# PROMPT BUILDER FUNCTIONS
# ============================================================================

def get_base_prompt(language: str = "en") -> str:
    """
    Get the base system prompt for the specified language.

    Args:
        language: Language code (en, de)

    Returns:
        Base system prompt string
    """
    return BASE_PROMPTS.get(language, BASE_PROMPTS["en"])


def get_available_languages() -> Dict[str, str]:
    """
    Get available languages with their display names.

    Returns:
        Dictionary mapping language codes to display names
    """
    return {
        "en": "English",
        "de": "Deutsch"
    }


def build_structured_prompt(base_prompt: str, transcript: str, language: str = "en") -> str:
    """
    Build a prompt specifically for native structured output (JSON schema).
    
    This avoids conflicting formatting instructions (like separators) that confuse
    models when JSON schema enforcement is active.

    Args:
        base_prompt: The base system prompt
        transcript: The session transcript to analyze
        language: Language code (en, de)

    Returns:
        Prompt string optimized for structured output
    """
    transcript_label = TRANSCRIPT_LABELS.get(language, TRANSCRIPT_LABELS["en"])
    instructions = STRUCTURED_OUTPUT_INSTRUCTIONS.get(language, STRUCTURED_OUTPUT_INSTRUCTIONS["en"])
    metadata_guidelines = get_metadata_guidelines(language)
    
    return f"""{base_prompt}

{instructions}

{metadata_guidelines}

{transcript_label}
{transcript}"""


def build_simple_prompt(base_prompt: str, transcript: str, language: str = "en") -> str:
    """
    Build a simple prompt without metadata extraction.

    Args:
        base_prompt: The base system prompt
        transcript: The session transcript to analyze
        language: Language code (en, de)

    Returns:
        Simple prompt string
    """
    transcript_label = TRANSCRIPT_LABELS.get(language, TRANSCRIPT_LABELS["en"])
    return f"""{base_prompt}

{transcript_label}
{transcript}"""


METADATA_ANALYSIS_PROMPTS: Dict[str, str] = {
    "en": """Analyze this TTRPG transcript and extract metadata. Return ONLY valid JSON with this exact structure:""",
    "de": """Analysiere dieses TTRPG-Transkript und extrahiere Metadaten. Gib NUR gültiges JSON mit dieser exakten Struktur zurück:"""
}

JSON_RESPONSE_LABELS: Dict[str, str] = {
    "en": "JSON Response:",
    "de": "JSON-Antwort:"
}

def get_metadata_analysis_prompt(transcript: str, language: str = "en") -> str:
    """
    Build a prompt specifically for metadata extraction.

    Args:
        transcript: The session transcript to analyze
        language: Language code (en, de)

    Returns:
        Metadata analysis prompt
    """
    import json

    prompt_text = METADATA_ANALYSIS_PROMPTS.get(language, METADATA_ANALYSIS_PROMPTS["en"])
    transcript_label = TRANSCRIPT_LABELS.get(language, TRANSCRIPT_LABELS["en"])
    json_label = JSON_RESPONSE_LABELS.get(language, JSON_RESPONSE_LABELS["en"])
    metadata_guidelines = get_metadata_guidelines(language)

    return f"""{prompt_text}

{json.dumps(METADATA_JSON_STRUCTURE, indent=4)}

{metadata_guidelines}

{transcript_label}
{transcript}

{json_label}"""
