//! Prompt templates ported from the Python `prompts.py` (EN/DE presets,
//! session-context builder, summary + metadata prompt builders).

use serde_json::{json, Value};

const SUMMARY_EN: &str = "You are an RPG assistant for the GM. Create a clean, chronological session summary.\n\nCHARACTER NAMES: Always use CHARACTER NAMES from the transcript (not player names). Use correct pronouns.\n\nFOCUS: Story continuity, not mechanics. NO damage numbers, NO stats, NO ability names, NO Hope/resource tracking.\n\nSTRUCTURE: Follow this Markdown structure:\n\n## What Happened\n\n[Write 3-5 paragraphs telling the story of the session chronologically from start to finish. Focus on the narrative flow - what happened, in what order, and how it ended. Make it readable and cohesive, not fragmented.]\n\n## Remember for Next Time\n\n**Key Events:**\n- [3-5 bullet points of major story moments that matter for continuity]\n\n**Important NPCs:**\n- [Name]: [One sentence about their current status and why they matter]\n- [Name]: [One sentence about their current status and why they matter]\n\n**Decisions & Consequences:**\n- [Major choices the party made and what they mean going forward]\n\n**Major Items Gained:**\n- [Only list significant items - no common loot, no materials, no trivial resources]\n\n**Unresolved:**\n- [Story threads and mysteries that need follow-up]";

const SUMMARY_DE: &str = "Du bist ein RPG-Assistent für den Spielleiter. Erstelle eine klare, chronologische Sitzungszusammenfassung.\n\nCHARAKTERNAMEN: Verwende immer den CHARAKTERNAMEN aus dem Transkript (nicht Spielername). Nutze die korrekten Pronomen.\n\nFOKUS: Story-Kontinuität, keine Mechaniken. KEIN Schaden, KEINE Stats, KEINE Fähigkeitsnamen, KEIN Hope/Ressourcen-Tracking.\n\nSTRUKTUR: Folge dieser Markdown-Struktur:\n\n## Was geschah\n\n[Schreibe 3-5 Absätze, die die Geschichte der Sitzung chronologisch von Anfang bis Ende erzählen. Fokus auf den narrativen Fluss - was geschah, in welcher Reihenfolge, und wie es endete. Mach es lesbar und zusammenhängend, nicht fragmentiert.]\n\n## Wichtig für nächstes Mal\n\n**Schlüsselereignisse:**\n- [3-5 Stichpunkte zu wichtigen Story-Momenten, die für Kontinuität wichtig sind]\n\n**Wichtige NPCs:**\n- [Name]: [Ein Satz über ihren aktuellen Status und warum sie wichtig sind]\n- [Name]: [Ein Satz über ihren aktuellen Status und warum sie wichtig sind]\n\n**Entscheidungen & Konsequenzen:**\n- [Wichtige Entscheidungen der Gruppe und was sie für die Zukunft bedeuten]\n\n**Wichtige erhaltene Gegenstände:**\n- [Nur bedeutende Items auflisten - keine gewöhnliche Beute, keine Materialien, keine trivialen Ressourcen]\n\n**Ungeklärt:**\n- [Story-Fäden und Mysterien, die Follow-up brauchen]";

const METADATA_GUIDE_EN: &str = "Metadata guidelines:\n- characters: List important PCs and NPCs mentioned. Use specific names.\n- locations: List specific locations visited or mentioned.\n- events: List 3-5 short bullet points of major events.\n- items: List significant items gained or mentioned.\n- tags: List 3-5 tags. E.g., \"Combat\", \"Social\", \"Exploration\", \"Mystery\".\n\nEnsure ALL fields are populated. Do not return empty lists.";

const METADATA_GUIDE_DE: &str = "Metadaten-Richtlinien:\n- characters: Liste wichtige SCs und NPCs. Verwende spezifische Namen.\n- locations: Liste spezifische besuchte oder erwähnte Orte.\n- events: Liste 3-5 kurze Stichpunkte zu Hauptereignissen.\n- items: Liste bedeutende erhaltene oder erwähnte Gegenstände.\n- tags: Liste 3-5 Tags. Z.B. \"Kampf\", \"Sozial\", \"Erkundung\", \"Mysterium\".\n\nStelle sicher, dass ALLE Felder ausgefüllt sind. Gib KEINE leeren Listen zurück.";

fn is_de(language: &str) -> bool {
    language == "de"
}

pub fn get_prompt_text(language: &str) -> &'static str {
    if is_de(language) {
        SUMMARY_DE
    } else {
        SUMMARY_EN
    }
}

/// `/prompts` payload: `{lang: {label, text}}`.
pub fn available_prompts() -> Value {
    json!({
        "en": { "label": "English – D&D / TTRPG", "text": SUMMARY_EN },
        "de": { "label": "Deutsch – D&D / TTRPG", "text": SUMMARY_DE },
    })
}

/// Format campaign/session metadata + speakers into a context block.
pub fn build_session_context(ctx: Option<&Value>, language: &str) -> String {
    let Some(ctx) = ctx else { return String::new() };
    let de = is_de(language);

    let fields: [(&str, &str); 8] = [
        ("campaign_name", "Campaign"),
        ("system", "System"),
        ("setting", "Setting"),
        ("session_number", "Session Number"),
        ("title", "Session Title"),
        ("date", "Date"),
        ("gm", "GM"),
        ("extra_info", "Additional Info"),
    ];
    let mut lines: Vec<String> = Vec::new();
    for (key, label) in fields {
        if let Some(v) = ctx.get(key) {
            let s = value_to_plain(v);
            if !s.is_empty() {
                lines.push(format!("- {label}: {s}"));
            }
        }
    }

    let mut block = String::new();
    if !lines.is_empty() {
        let header = if de { "Sitzungskontext:" } else { "Session Context:" };
        block.push_str(header);
        block.push('\n');
        block.push_str(&lines.join("\n"));
        block.push('\n');
    }

    if let Some(speakers) = ctx.get("speakers").and_then(Value::as_array) {
        let gm_name = ctx.get("gm").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
        let plays = if de { "spielt" } else { "plays" };
        let gm_label = if de { "ist der Spielleiter" } else { "is the GM" };
        let speakers_label = if de { "Sprecher:" } else { "Speakers:" };
        let mut sl: Vec<String> = vec![speakers_label.to_string()];
        for s in speakers {
            let player = s.get("player_name").and_then(Value::as_str).unwrap_or("").trim();
            let character = s.get("character_name").and_then(Value::as_str).unwrap_or("").trim();
            let pronouns = s.get("pronouns").and_then(Value::as_str).unwrap_or("").trim();
            if player.is_empty() && character.is_empty() {
                continue;
            }
            let mut part = if !gm_name.is_empty() && player.to_lowercase() == gm_name {
                format!("- {player} {gm_label}")
            } else if !player.is_empty() && !character.is_empty() {
                format!("- {player} {plays} {character}")
            } else if !player.is_empty() {
                format!("- {player}")
            } else {
                format!("- {character}")
            };
            if !pronouns.is_empty() {
                part.push_str(&format!(" ({pronouns})"));
            }
            sl.push(part);
        }
        if sl.len() > 1 {
            block.push('\n');
            block.push_str(&sl.join("\n"));
            block.push('\n');
        }
    }

    block
}

fn value_to_plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn build_summary_prompt(
    transcript: &str,
    title: Option<&str>,
    context: Option<&str>,
    language: &str,
    system_prompt: Option<&str>,
    session_context: Option<&Value>,
) -> String {
    let header = system_prompt
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| get_prompt_text(language));
    let title_line = title.map(|t| format!("Title: {t}\n")).unwrap_or_default();
    let context_line = context.map(|c| format!("Context: {c}\n")).unwrap_or_default();
    let session_block = build_session_context(session_context, language);
    let transcript_label = if is_de(language) { "Transkript:" } else { "Transcript:" };

    format!(
        "{header}\n\n{title_line}{context_line}{session_block}\n{transcript_label}\n{transcript}\n\nReturn only the summary in markdown."
    )
}

pub fn build_metadata_prompt(summary: &str, language: &str) -> String {
    let de = is_de(language);
    let analysis = if de {
        "Analysiere diese TTRPG-Sitzungszusammenfassung und extrahiere Metadaten. Gib NUR gültiges JSON mit dieser exakten Struktur zurück:"
    } else {
        "Analyze this TTRPG session summary and extract metadata. Return ONLY valid JSON with this exact structure:"
    };
    let guidelines = if de { METADATA_GUIDE_DE } else { METADATA_GUIDE_EN };
    let structure = serde_json::to_string_pretty(&json!({
        "characters": [], "locations": [], "events": [], "items": [], "tags": []
    }))
    .unwrap();
    format!("{analysis}\n\n{structure}\n\n{guidelines}\n\nSummary:\n{summary}\n\nReturn only valid JSON.")
}
