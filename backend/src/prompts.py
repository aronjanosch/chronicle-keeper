"""
Centralized prompt management for Chronicle Keeper.

This module contains all LLM prompts, format instructions, and metadata definitions
in a single location to eliminate duplication and improve maintainability.
"""

from typing import Dict

# ============================================================================
# BASE SYSTEM PROMPTS (Localized)
# ============================================================================

BASE_PROMPTS: Dict[str, str] = {
    "en": """You are a professional tabletop RPG assistant. Your task is to analyze the following TTRPG session transcript and generate a CONCISE, structured session summary.

IMPORTANT INSTRUCTIONS FOR CHARACTER REFERENCES:
- The transcript includes a "Participants" section with character names, player names, and pronouns
- When referring to characters in your summary, ALWAYS use their CHARACTER NAME (not player name)
- Use the CORRECT PRONOUNS listed for each character consistently throughout your summary
- If only a player name is provided (no character name), use the player name
- Example: If the transcript shows "Gandalf: Character: Gandalf | Player: Alex | Pronouns: he/him", refer to this character as "Gandalf" using "he/him" pronouns

Focus ONLY on the most critical elements:
1. Major plot developments and revelations.
2. Key character decisions and actions (especially combat outcomes or failed rolls that change the story).
3. Action items or goals set for the next session.

Format the output using Markdown with two distinct, bolded sections:

**Summary of Events:**
- [Bullet point 1]
- [Bullet point 2]

**Key Decisions & Next Steps:**
- [Bullet point 1 - A choice the party made]
- [Bullet point 2 - A goal or action item for the next session]""",

    "de": """Du bist ein professioneller Pen-&-Paper-RPG-Assistent. Deine Aufgabe ist es, das folgende TTRPG-Sitzungstranskript zu analysieren und eine PRÄGNANTE, strukturierte Sitzungszusammenfassung zu erstellen.

WICHTIGE ANWEISUNGEN FÜR CHARAKTERREFERENZEN:
- Das Transkript enthält einen Abschnitt "Teilnehmer" mit Charakternamen, Spielernamen und Pronomen
- Verwende bei Verweisen auf Charaktere in deiner Zusammenfassung IMMER deren CHARAKTERNAMEN (nicht Spielernamen)
- Verwende die angegebenen KORREKTEN PRONOMEN für jeden Charakter durchgehend in deiner Zusammenfassung
- Wenn nur ein Spielername angegeben ist (kein Charaktername), verwende den Spielernamen
- Beispiel: Wenn das Transkript zeigt "Gandalf: Charakter: Gandalf | Spieler: Alex | Pronomen: er/ihm", beziehe dich auf diesen Charakter als "Gandalf" mit den Pronomen "er/ihm"

Konzentriere dich NUR auf die wichtigsten Elemente:
1. Große Handlungsentwicklungen und Enthüllungen.
2. Wichtige Charakterentscheidungen und -handlungen (besonders Kampfergebnisse oder gescheiterte Würfe, die die Geschichte verändern).
3. Aufgaben oder Ziele für die nächste Sitzung.

Formatiere die Ausgabe mit Markdown in zwei verschiedenen, fett gedruckten Abschnitten:

**Zusammenfassung der Ereignisse:**
- [Stichpunkt 1]
- [Stichpunkt 2]

**Wichtige Entscheidungen & Nächste Schritte:**
- [Stichpunkt 1 - Eine Entscheidung der Gruppe]
- [Stichpunkt 2 - Ein Ziel oder eine Aufgabe für die nächste Sitzung]"""
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
- suggested_tags: Activity types (combat, social, exploration, investigation, puzzle, travel) and tone (dramatic, comedic, tense, mystery, political)
- mentioned_characters: Names of NPCs, characters, or entities mentioned multiple times
- mentioned_locations: Place names mentioned in the session
- session_tone: Overall mood/tone descriptors
- key_events: Major story beats or important occurrences

Only include items that are clearly mentioned and significant. Limit each array to 5-8 most relevant items.""",
    "de": """Metadaten-Richtlinien:
- suggested_tags: Aktivitätstypen (Kampf, sozial, Erkundung, Ermittlung, Rätsel, Reise) und Stimmung (dramatisch, komisch, angespannt, mysteriös, politisch)
- mentioned_characters: Namen von NPCs, Charakteren oder Entitäten, die mehrfach erwähnt werden
- mentioned_locations: Ortsnamen, die in der Sitzung erwähnt werden
- session_tone: Gesamtstimmungs-/Tonbeschreibungen
- key_events: Wichtige Handlungsmomente oder wichtige Ereignisse

Nur Elemente einbeziehen, die klar erwähnt und bedeutsam sind. Jedes Array auf 5-8 relevanteste Elemente begrenzen."""
}

def get_metadata_guidelines(language: str = "en") -> str:
    """Get metadata guidelines in the specified language."""
    return METADATA_GUIDELINES.get(language, METADATA_GUIDELINES["en"])

# ============================================================================
# FORMAT INSTRUCTIONS
# ============================================================================

RESPONSE_SEPARATOR = "---METADATA---"

SUMMARY_FORMAT_TEMPLATES = {
    "en": """**Summary of Events:**
- [Major plot development or revelation]
- [Key combat outcome or story change]
- [Important discovery or event]

**Key Decisions & Next Steps:**
- [A choice the party made]
- [A goal or action item for the next session]
- [Unresolved situation requiring future action]""",
    "de": """**Zusammenfassung der Ereignisse:**
- [Große Handlungsentwicklung oder Enthüllung]
- [Wichtiges Kampfergebnis oder Wendepunkt]
- [Wichtige Entdeckung oder Ereignis]

**Wichtige Entscheidungen & Nächste Schritte:**
- [Eine Entscheidung der Gruppe]
- [Ein Ziel oder eine Aufgabe für die nächste Sitzung]
- [Offene Situation, die weitere Aktion erfordert]"""
}

ENHANCED_INSTRUCTIONS_TEXT: Dict[str, str] = {
    "en": {
        "critical": "CRITICAL: Follow this EXACT format structure:",
        "instructions": "INSTRUCTIONS:",
        "step1": "1. First write the summary using the EXACT format above",
        "step2": f'2. Then add "{RESPONSE_SEPARATOR}" as a separator',
        "step3": "3. Then add the JSON metadata block",
        "step4": "4. Do NOT deviate from this structure"
    },
    "de": {
        "critical": "KRITISCH: Befolge diese EXAKTE Formatstruktur:",
        "instructions": "ANWEISUNGEN:",
        "step1": "1. Schreibe zuerst die Zusammenfassung im EXAKTEN Format oben",
        "step2": f'2. Füge dann "{RESPONSE_SEPARATOR}" als Trennzeichen hinzu',
        "step3": "3. Füge dann den JSON-Metadatenblock hinzu",
        "step4": "4. Weiche NICHT von dieser Struktur ab"
    }
}

def get_enhanced_instructions(language: str = "en") -> str:
    """
    Get the enhanced formatting instructions with localized section headers.

    Args:
        language: Language code (en, de)

    Returns:
        Instruction string including the localized summary template and separator/JSON block
    """
    template = SUMMARY_FORMAT_TEMPLATES.get(language, SUMMARY_FORMAT_TEMPLATES["en"])
    instructions = ENHANCED_INSTRUCTIONS_TEXT.get(language, ENHANCED_INSTRUCTIONS_TEXT["en"])
    metadata_guidelines = get_metadata_guidelines(language)
    
    return f"""{instructions["critical"]}

{template}

{RESPONSE_SEPARATOR}
{{
    "suggested_tags": [],
    "mentioned_characters": [],
    "mentioned_locations": [],
    "session_tone": [],
    "key_events": []
}}

{instructions["instructions"]}
{instructions["step1"]}
{instructions["step2"]}
{instructions["step3"]}
{instructions["step4"]}

{metadata_guidelines}"""

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


def build_enhanced_prompt(base_prompt: str, transcript: str, language: str = "en") -> str:
    """
    Build the full enhanced prompt with format instructions and metadata guidelines.

    Args:
        base_prompt: The base system prompt
        transcript: The session transcript to analyze
        language: Language code (en, de)

    Returns:
        Complete prompt string ready for LLM
    """
    transcript_label = TRANSCRIPT_LABELS.get(language, TRANSCRIPT_LABELS["en"])
    return f"""{base_prompt}

{get_enhanced_instructions(language)}

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


def get_empty_metadata() -> Dict[str, list]:
    """
    Get an empty metadata structure with all expected keys.

    Returns:
        Dictionary with empty lists for all metadata fields
    """
    return {
        "suggested_tags": [],
        "mentioned_characters": [],
        "mentioned_locations": [],
        "session_tone": [],
        "key_events": []
    }
