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
    "en": """You are a professional tabletop RPG assistant helping the GM maintain campaign continuity. Your task is to analyze the following TTRPG session transcript and generate a detailed, GM-focused session summary.

IMPORTANT INSTRUCTIONS FOR CHARACTER REFERENCES:
- The transcript includes a "Participants" section with character names, player names, and pronouns
- When referring to characters in your summary, ALWAYS use their CHARACTER NAME (not player name)
- Use the CORRECT PRONOUNS listed for each character consistently throughout your summary
- If only a player name is provided (no character name), use the player name
- Example: If the transcript shows "Gandalf: Character: Gandalf | Player: Alex | Pronouns: he/him", refer to this character as "Gandalf" using "he/him" pronouns

GM-FOCUSED SUMMARY GUIDELINES:
Your summary should help the GM remember important details for future sessions. Include:

1. NARRATIVE & STORY PROGRESSION:
   - Major plot developments and revelations
   - New information learned or mysteries uncovered
   - Story hooks and foreshadowing introduced
   - NPC interactions and what NPCs revealed or promised

2. CHARACTER MOMENTS & DECISIONS:
   - Important choices the party made and their reasoning
   - Character development moments and roleplay highlights
   - Party dynamics and conflicts
   - Failed rolls with significant consequences

3. COMBAT & ENCOUNTERS:
   - How combat encounters played out (tactics, key moments)
   - Enemy types faced and their capabilities
   - Combat outcomes and consequences

4. RESOURCES & ITEMS:
   - Items, rewards, or information obtained
   - Resources spent or lost
   - Quest items or clues collected

5. LOCATIONS & WORLD DETAILS:
   - Places visited or discovered
   - Environmental details that matter
   - World-building elements introduced

6. CONTINUITY TRACKING:
   - Unresolved plot threads and mysteries
   - NPCs who need follow-up
   - Promises made or debts incurred
   - Goals and action items for next session

Scale your summary length to match the session - longer sessions need more detail. Be thorough enough that the GM can quickly refresh their memory before the next session.

Format the output using Markdown with two distinct, bolded sections:

**Summary of Events:**
- [Include detailed bullet points covering all important story beats]
- [Capture NPC interactions, discoveries, and character moments]
- [Note combat outcomes and how encounters unfolded]
- [Include any items obtained or resources used]

**Key Decisions & Next Steps:**
- [Document important choices and their context]
- [List unresolved plot threads requiring follow-up]
- [Note promises, debts, or commitments made]
- [Identify clear goals for the next session]""",

    "de": """Du bist ein professioneller Pen-&-Paper-RPG-Assistent, der dem Spielleiter hilft, die Kampagnenkontinuität aufrechtzuerhalten. Deine Aufgabe ist es, das folgende TTRPG-Sitzungstranskript zu analysieren und eine detaillierte, für den Spielleiter optimierte Sitzungszusammenfassung zu erstellen.

WICHTIGE ANWEISUNGEN FÜR CHARAKTERREFERENZEN:
- Das Transkript enthält einen Abschnitt "Teilnehmer" mit Charakternamen, Spielernamen und Pronomen
- Verwende bei Verweisen auf Charaktere in deiner Zusammenfassung IMMER deren CHARAKTERNAMEN (nicht Spielernamen)
- Verwende die angegebenen KORREKTEN PRONOMEN für jeden Charakter durchgehend in deiner Zusammenfassung
- Wenn nur ein Spielername angegeben ist (kein Charaktername), verwende den Spielernamen
- Beispiel: Wenn das Transkript zeigt "Gandalf: Charakter: Gandalf | Spieler: Alex | Pronomen: er/ihm", beziehe dich auf diesen Charakter als "Gandalf" mit den Pronomen "er/ihm"

SPIELLEITER-FOKUSSIERTE RICHTLINIEN:
Deine Zusammenfassung soll dem Spielleiter helfen, sich an wichtige Details für zukünftige Sitzungen zu erinnern. Berücksichtige:

1. ERZÄHLUNG & HANDLUNGSFORTSCHRITT:
   - Große Handlungsentwicklungen und Enthüllungen
   - Neue Informationen oder aufgedeckte Mysterien
   - Eingeführte Story-Hooks und Vorausdeutungen
   - NSC-Interaktionen und was NSCs enthüllt oder versprochen haben

2. CHARAKTERMOMENTE & ENTSCHEIDUNGEN:
   - Wichtige Entscheidungen der Gruppe und ihre Beweggründe
   - Charakterentwicklungsmomente und Rollenspiel-Highlights
   - Gruppendynamik und Konflikte
   - Gescheiterte Würfe mit bedeutsamen Konsequenzen

3. KÄMPFE & BEGEGNUNGEN:
   - Wie Kampfbegegnungen verliefen (Taktiken, Schlüsselmomente)
   - Gegnertypen und ihre Fähigkeiten
   - Kampfergebnisse und Konsequenzen

4. RESSOURCEN & GEGENSTÄNDE:
   - Erhaltene Gegenstände, Belohnungen oder Informationen
   - Verbrauchte oder verlorene Ressourcen
   - Gesammelte Quest-Gegenstände oder Hinweise

5. ORTE & WELTDETAILS:
   - Besuchte oder entdeckte Orte
   - Umgebungsdetails, die wichtig sind
   - Eingeführte Worldbuilding-Elemente

6. KONTINUITÄTSVERFOLGUNG:
   - Ungelöste Handlungsstränge und Mysterien
   - NSCs, die Follow-up benötigen
   - Gegebene Versprechen oder eingegangene Schulden
   - Ziele und Aufgaben für die nächste Sitzung

Passe die Länge deiner Zusammenfassung an die Sitzung an - längere Sitzungen benötigen mehr Details. Sei gründlich genug, dass der Spielleiter vor der nächsten Sitzung schnell sein Gedächtnis auffrischen kann.

Formatiere die Ausgabe mit Markdown in zwei verschiedenen, fett gedruckten Abschnitten:

**Zusammenfassung der Ereignisse:**
- [Füge detaillierte Stichpunkte für alle wichtigen Story-Beats hinzu]
- [Erfasse NSC-Interaktionen, Entdeckungen und Charaktermomente]
- [Notiere Kampfergebnisse und wie Begegnungen verliefen]
- [Füge erhaltene Gegenstände oder verwendete Ressourcen hinzu]

**Wichtige Entscheidungen & Nächste Schritte:**
- [Dokumentiere wichtige Entscheidungen und ihren Kontext]
- [Liste ungelöste Handlungsstränge auf, die Follow-up benötigen]
- [Notiere Versprechen, Schulden oder eingegangene Verpflichtungen]
- [Identifiziere klare Ziele für die nächste Sitzung]"""
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
- mentioned_characters: Liste wichtige SCs und NSCs. Verwende spezifische Namen.
- mentioned_locations: Liste spezifische besuchte oder erwähnte Orte.
- session_tone: ERFORDERLICH. Liste 1-3 Stimmungsbeschreibungen. Z.B. "Angespannt", "Humorvoll", "Düster".
- key_events: ERFORDERLICH. Liste 3-5 kurze Stichpunkte zu Hauptereignissen.

Stelle sicher, dass ALLE erforderlichen Felder ausgefüllt sind. Gib KEINE leeren Listen für Tags, Stimmung oder Ereignisse zurück."""
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
- [Opening scene and initial situation]
- [Major plot developments, revelations, or new information learned]
- [NPC interactions: who they met, what was discussed, promises made]
- [Combat encounters: enemies faced, tactics used, outcomes and consequences]
- [Character moments: important decisions, roleplay highlights, failed rolls]
- [Items obtained, rewards received, or resources used]
- [Location details and world-building elements introduced]
- [How the session concluded and immediate situation]

**Key Decisions & Next Steps:**
- [Major choices the party made and their reasoning]
- [Unresolved plot threads and mysteries that need follow-up]
- [NPCs who require attention or promised interactions]
- [Clear goals and action items for the next session]
- [Debts, promises, or commitments the party has made]""",
    "de": """**Zusammenfassung der Ereignisse:**
- [Eröffnungsszene und Ausgangssituation]
- [Große Handlungsentwicklungen, Enthüllungen oder neue erlernte Informationen]
- [NSC-Interaktionen: wen sie trafen, was besprochen wurde, gegebene Versprechen]
- [Kampfbegegnungen: Gegner, eingesetzte Taktiken, Ergebnisse und Konsequenzen]
- [Charaktermomente: wichtige Entscheidungen, Rollenspiel-Highlights, gescheiterte Würfe]
- [Erhaltene Gegenstände, Belohnungen oder verwendete Ressourcen]
- [Ortsdetails und eingeführte Worldbuilding-Elemente]
- [Wie die Sitzung endete und die unmittelbare Situation]

**Wichtige Entscheidungen & Nächste Schritte:**
- [Große Entscheidungen der Gruppe und ihre Beweggründe]
- [Ungelöste Handlungsstränge und Mysterien, die Follow-up benötigen]
- [NSCs, die Aufmerksamkeit benötigen oder versprochene Interaktionen]
- [Klare Ziele und Aufgaben für die nächste Sitzung]
- [Schulden, Versprechen oder Verpflichtungen, die die Gruppe eingegangen ist]"""
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

STRUCTURED_OUTPUT_INSTRUCTIONS: Dict[str, str] = {
    "en": "Analyze the transcript and generate a structured summary. You MUST populate the metadata lists. 'suggested_tags', 'session_tone', and 'key_events' CANNOT be empty. If you are unsure, infer the best options from the context.",
    "de": "Analysiere das Transkript und erstelle eine strukturierte Zusammenfassung. Du MUSST die Metadaten-Listen füllen. 'suggested_tags', 'session_tone' und 'key_events' DÜRFEN NICHT leer sein. Wenn du unsicher bist, leite die besten Optionen aus dem Kontext ab."
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
    
    NOTE: This is for text-based generation where the model outputs text + separator + JSON.
    Do NOT use this for native structured output (JSON schema).

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
