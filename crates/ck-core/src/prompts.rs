//! Prompt templates ported from the Python `prompts.py` (EN/DE presets,
//! session-context builder, summary + metadata prompt builders).

use serde_json::{json, Value};

const SUMMARY_EN: &str = "You are an RPG assistant for the GM. Create a clean, chronological session summary.\n\nCHARACTER NAMES: Always use CHARACTER NAMES from the transcript (not player names). Use correct pronouns.\n\nFOCUS: Story continuity, not mechanics. NO damage numbers, NO stats, NO ability names, NO Hope/resource tracking.\n\nSTRUCTURE: Follow this Markdown structure:\n\n## What Happened\n\n[Write 3-5 paragraphs telling the story of the session chronologically from start to finish. Focus on the narrative flow - what happened, in what order, and how it ended. Make it readable and cohesive, not fragmented.]\n\n## Remember for Next Time\n\n**Key Events:**\n- [3-5 bullet points of major story moments that matter for continuity]\n\n**Important NPCs:**\n- [Name]: [One sentence about their current status and why they matter]\n- [Name]: [One sentence about their current status and why they matter]\n\n**Decisions & Consequences:**\n- [Major choices the party made and what they mean going forward]\n\n**Major Items Gained:**\n- [Only list significant items - no common loot, no materials, no trivial resources]\n\n**Unresolved:**\n- [Story threads and mysteries that need follow-up]";

const SUMMARY_DE: &str = "Du bist ein RPG-Assistent für den Spielleiter. Erstelle eine klare, chronologische Sitzungszusammenfassung.\n\nCHARAKTERNAMEN: Verwende immer den CHARAKTERNAMEN aus dem Transkript (nicht Spielername). Nutze die korrekten Pronomen.\n\nFOKUS: Story-Kontinuität, keine Mechaniken. KEIN Schaden, KEINE Stats, KEINE Fähigkeitsnamen, KEIN Hope/Ressourcen-Tracking.\n\nSTRUKTUR: Folge dieser Markdown-Struktur:\n\n## Was geschah\n\n[Schreibe 3-5 Absätze, die die Geschichte der Sitzung chronologisch von Anfang bis Ende erzählen. Fokus auf den narrativen Fluss - was geschah, in welcher Reihenfolge, und wie es endete. Mach es lesbar und zusammenhängend, nicht fragmentiert.]\n\n## Wichtig für nächstes Mal\n\n**Schlüsselereignisse:**\n- [3-5 Stichpunkte zu wichtigen Story-Momenten, die für Kontinuität wichtig sind]\n\n**Wichtige NPCs:**\n- [Name]: [Ein Satz über ihren aktuellen Status und warum sie wichtig sind]\n- [Name]: [Ein Satz über ihren aktuellen Status und warum sie wichtig sind]\n\n**Entscheidungen & Konsequenzen:**\n- [Wichtige Entscheidungen der Gruppe und was sie für die Zukunft bedeuten]\n\n**Wichtige erhaltene Gegenstände:**\n- [Nur bedeutende Items auflisten - keine gewöhnliche Beute, keine Materialien, keine trivialen Ressourcen]\n\n**Ungeklärt:**\n- [Story-Fäden und Mysterien, die Follow-up brauchen]";

const METADATA_GUIDE_EN: &str = "Metadata guidelines:\n- characters: List important PCs and NPCs mentioned. Use specific names.\n- locations: List specific locations visited or mentioned.\n- events: List 3-5 short bullet points of major events.\n- items: List significant items gained or mentioned.\n- tags: List 3-5 tags. E.g., \"Combat\", \"Social\", \"Exploration\", \"Mystery\".\n\nEnsure ALL fields are populated. Do not return empty lists.";

const METADATA_GUIDE_DE: &str = "Metadaten-Richtlinien:\n- characters: Liste wichtige SCs und NPCs. Verwende spezifische Namen.\n- locations: Liste spezifische besuchte oder erwähnte Orte.\n- events: Liste 3-5 kurze Stichpunkte zu Hauptereignissen.\n- items: Liste bedeutende erhaltene oder erwähnte Gegenstände.\n- tags: Liste 3-5 Tags. Z.B. \"Kampf\", \"Sozial\", \"Erkundung\", \"Mysterium\".\n\nStelle sicher, dass ALLE Felder ausgefüllt sind. Gib KEINE leeren Listen zurück.";

fn is_de(language: &str) -> bool {
    // Stored config values are user-entered ("de", "De", "DE", " de ") — match
    // case- and whitespace-insensitively so the German presets actually fire.
    language.trim().eq_ignore_ascii_case("de")
}

pub fn get_prompt_text(language: &str) -> &'static str {
    if is_de(language) {
        SUMMARY_DE
    } else {
        SUMMARY_EN
    }
}

/// The prompt templates shipped by default: `(id, label, text)`. Seeded into the
/// `prompt_templates` table on first run and re-creatable via "restore defaults".
/// The user may edit or delete them and add their own — see `store::prompts`.
pub const BUILTIN_TEMPLATES: [(&str, &str, &str); 2] = [
    ("default-en", "English – D&D / TTRPG", SUMMARY_EN),
    ("default-de", "Deutsch – D&D / TTRPG", SUMMARY_DE),
];

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
        let header = if de {
            "Sitzungskontext:"
        } else {
            "Session Context:"
        };
        block.push_str(header);
        block.push('\n');
        block.push_str(&lines.join("\n"));
        block.push('\n');
    }

    if let Some(speakers) = ctx.get("speakers").and_then(Value::as_array) {
        let gm_name = ctx
            .get("gm")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let plays = if de { "spielt" } else { "plays" };
        let gm_label = if de {
            "ist der Spielleiter"
        } else {
            "is the GM"
        };
        let speakers_label = if de { "Sprecher:" } else { "Speakers:" };
        let mut sl: Vec<String> = vec![speakers_label.to_string()];
        for s in speakers {
            let player = s
                .get("player_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let character = s
                .get("character_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let pronouns = s
                .get("pronouns")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
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

    // Codex: per-campaign glossary of known names & lore, passed verbatim so the
    // LLM can recognize and correctly spell NPCs/places/factions/items the ASR mangled.
    // Two sources: the freeform `codex` paste box (Phase 1) and the structured
    // `codex_entries` list (Phase 2). Both are emitted under the same header so
    // the LLM treats them as one glossary.
    let codex_text = ctx
        .get("codex")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let entries = ctx.get("codex_entries").and_then(Value::as_array);
    let has_entries = entries.map(|a| !a.is_empty()).unwrap_or(false);
    if !codex_text.is_empty() || has_entries {
        let header = if de {
            "Bekannte Namen & Lore:"
        } else {
            "Known names & lore:"
        };
        block.push('\n');
        block.push_str(header);
        block.push('\n');
        if !codex_text.is_empty() {
            block.push_str(codex_text);
            block.push('\n');
        }
        if let Some(arr) = entries {
            block.push_str(&render_codex_entries(arr, de));
        }
    }

    block
}

fn kind_label(kind: &str, de: bool) -> &'static str {
    match (kind, de) {
        ("pc", true) => "Spielercharaktere",
        ("pc", false) => "Player characters",
        ("npc", true) => "NPCs",
        ("npc", false) => "NPCs",
        ("place", true) => "Orte",
        ("place", false) => "Places",
        ("faction", true) => "Fraktionen",
        ("faction", false) => "Factions",
        ("item", true) => "Gegenstände",
        ("item", false) => "Items",
        ("lore", true) => "Lore",
        ("lore", false) => "Lore",
        _ => "Other",
    }
}

fn render_codex_entries(entries: &[Value], de: bool) -> String {
    use std::collections::BTreeMap;
    let mut by_kind: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for e in entries {
        let name = e.get("name").and_then(Value::as_str).unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        let kind = e.get("kind").and_then(Value::as_str).unwrap_or("lore");
        let body = e.get("body").and_then(Value::as_str).unwrap_or("").trim();
        by_kind.entry(kind).or_default().push((name, body));
    }
    let mut out = String::new();
    // Stable order: pc, npc, place, faction, item, lore.
    for kind in ["pc", "npc", "place", "faction", "item", "lore"] {
        let Some(list) = by_kind.get(kind) else {
            continue;
        };
        if list.is_empty() {
            continue;
        }
        out.push_str(kind_label(kind, de));
        out.push_str(":\n");
        for (name, body) in list {
            if body.is_empty() {
                out.push_str(&format!("- {name}\n"));
            } else {
                out.push_str(&format!("- {name} — {body}\n"));
            }
        }
    }
    out
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
    let context_line = context
        .map(|c| format!("Context: {c}\n"))
        .unwrap_or_default();
    let session_block = build_session_context(session_context, language);
    let transcript_label = if is_de(language) {
        "Transkript:"
    } else {
        "Transcript:"
    };

    format!(
        "{header}\n\n{title_line}{context_line}{session_block}\n{transcript_label}\n{transcript}\n\nReturn only the summary in markdown."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_is_injected_verbatim_when_present() {
        let ctx = json!({ "campaign_name": "The Iron Crown", "codex": "Neverwinter — frozen trade city." });
        let block = build_session_context(Some(&ctx), "en");
        assert!(
            block.contains("Known names & lore:"),
            "labelled header present"
        );
        assert!(
            block.contains("Neverwinter — frozen trade city."),
            "codex passed verbatim"
        );
    }

    #[test]
    fn codex_block_omitted_when_empty() {
        let ctx = json!({ "campaign_name": "The Iron Crown", "codex": "  " });
        let block = build_session_context(Some(&ctx), "en");
        assert!(
            !block.contains("Known names & lore"),
            "no header for blank codex"
        );
    }

    #[test]
    fn codex_entries_render_grouped_under_same_header() {
        let ctx = json!({
            "codex": "Freeform notes.",
            "codex_entries": [
                { "name": "Aragorn", "kind": "npc", "body": "Heir of Isildur" },
                { "name": "Bree", "kind": "place", "body": "" },
                { "name": "Gandalf", "kind": "npc", "body": "" },
            ],
        });
        let block = build_session_context(Some(&ctx), "en");
        assert!(block.contains("Known names & lore:"));
        assert!(block.contains("Freeform notes."));
        assert!(block.contains("NPCs:"));
        assert!(block.contains("- Aragorn — Heir of Isildur"));
        assert!(block.contains("- Gandalf"));
        assert!(block.contains("Places:"));
        assert!(block.contains("- Bree"));
    }

    #[test]
    fn recap_prompt_includes_name_summaries_and_language() {
        let block = "### Session 1 — Arrival\nThe party reached Bree.";
        let en = build_recap_prompt("The Iron Crown", block, "en");
        assert!(en.contains("Campaign: The Iron Crown"));
        assert!(en.contains("Session summaries:"));
        assert!(en.contains("The party reached Bree."));
        assert!(en.contains("Where Things Stand"));

        let de = build_recap_prompt("Die Eiserne Krone", block, "de");
        assert!(de.contains("Kampagne: Die Eiserne Krone"));
        assert!(de.contains("Sitzungszusammenfassungen:"));
        assert!(de.contains("Aktueller Stand"));
    }

    #[test]
    fn language_match_is_case_and_whitespace_insensitive() {
        // Config stores user-entered values like "De" — German presets must still fire.
        assert!(is_de("De"));
        assert!(is_de("DE"));
        assert!(is_de(" de "));
        assert!(!is_de("en"));
        assert!(get_prompt_text("De").starts_with("Du bist"));
        assert!(build_metadata_prompt("s", "De", &[]).contains("Analysiere"));
    }

    #[test]
    fn metadata_prompt_injects_known_tags() {
        let tags = vec!["Kampf".to_string(), "Mysterium".to_string()];
        let de = build_metadata_prompt("summary", "de", &tags);
        assert!(de.contains("Tag-Bibliothek"));
        assert!(de.contains("Kampf, Mysterium"));

        let en = build_metadata_prompt("summary", "en", &tags);
        assert!(en.contains("tag library"));
        assert!(en.contains("Kampf, Mysterium"));
    }

    #[test]
    fn metadata_prompt_omits_tag_block_when_empty() {
        let p = build_metadata_prompt("summary", "de", &[]);
        assert!(!p.contains("Tag-Bibliothek"));
    }

    #[test]
    fn codex_entries_alone_still_render() {
        let ctx = json!({
            "codex_entries": [{ "name": "Bree", "kind": "place", "body": "" }],
        });
        let block = build_session_context(Some(&ctx), "en");
        assert!(block.contains("Known names & lore:"));
        assert!(block.contains("Places:"));
    }
}

const RECAP_EN: &str = "You are an RPG assistant for the GM. Below are the per-session summaries of a campaign, in chronological order. Weave them into a single flowing \"story so far\" recap the GM can read in one sitting to recall the whole arc.\n\nRULES:\n- Tell it as one continuous narrative, chronological, past tense. Do NOT list it session-by-session.\n- Use CHARACTER names, places, and factions exactly as written in the summaries.\n- Focus on the throughline: how the story built, what changed, where it now stands.\n- End with a short \"## Where Things Stand\" section: open threads, looming threats, and unanswered questions going into the next session.\n- Be concise — a few tight paragraphs, not a retelling of every scene. Markdown only.";

const RECAP_DE: &str = "Du bist ein RPG-Assistent für den Spielleiter. Unten stehen die einzelnen Sitzungszusammenfassungen einer Kampagne in chronologischer Reihenfolge. Verwebe sie zu einer einzigen fließenden \"Was bisher geschah\"-Zusammenfassung, die der SL in einem Zug lesen kann, um den gesamten Handlungsbogen zu erinnern.\n\nREGELN:\n- Erzähle es als eine durchgehende Erzählung, chronologisch, in der Vergangenheitsform. Liste es NICHT sitzungsweise auf.\n- Verwende CHARAKTERNAMEN, Orte und Fraktionen genau so, wie sie in den Zusammenfassungen stehen.\n- Fokus auf den roten Faden: wie sich die Geschichte aufgebaut hat, was sich verändert hat, wo sie jetzt steht.\n- Schließe mit einem kurzen Abschnitt \"## Aktueller Stand\": offene Fäden, drohende Gefahren und ungeklärte Fragen für die nächste Sitzung.\n- Fasse dich kurz — ein paar dichte Absätze, keine Nacherzählung jeder Szene. Nur Markdown.";

/// Build the "story so far" recap prompt from a chronological block of session
/// summaries. `sessions_block` is the pre-joined text (per-session headers + body).
pub fn build_recap_prompt(campaign_name: &str, sessions_block: &str, language: &str) -> String {
    let header = if is_de(language) { RECAP_DE } else { RECAP_EN };
    let name_line = if campaign_name.trim().is_empty() {
        String::new()
    } else if is_de(language) {
        format!("Kampagne: {campaign_name}\n\n")
    } else {
        format!("Campaign: {campaign_name}\n\n")
    };
    let label = if is_de(language) {
        "Sitzungszusammenfassungen:"
    } else {
        "Session summaries:"
    };
    let closing = if is_de(language) {
        "Gib nur die Zusammenfassung in Markdown zurück."
    } else {
        "Return only the recap in markdown."
    };
    format!("{header}\n\n{name_line}{label}\n{sessions_block}\n\n{closing}")
}

pub fn build_metadata_prompt(summary: &str, language: &str, known_tags: &[String]) -> String {
    let de = is_de(language);
    let analysis = if de {
        "Analysiere diese TTRPG-Sitzungszusammenfassung und extrahiere Metadaten. Gib NUR gültiges JSON mit dieser exakten Struktur zurück:"
    } else {
        "Analyze this TTRPG session summary and extract metadata. Return ONLY valid JSON with this exact structure:"
    };
    let guidelines = if de {
        METADATA_GUIDE_DE
    } else {
        METADATA_GUIDE_EN
    };
    let structure = serde_json::to_string_pretty(&json!({
        "characters": [], "locations": [], "events": [], "items": [], "tags": []
    }))
    .unwrap();
    // Tag library: the campaign's existing tag vocabulary. Reusing it keeps tags
    // consistent across sessions (one campaign-wide set, one language) instead of
    // a fresh, differently-cased set every time.
    let tag_block = build_tag_library_block(known_tags, de);
    format!("{analysis}\n\n{structure}\n\n{guidelines}\n{tag_block}\nSummary:\n{summary}\n\nReturn only valid JSON.")
}

/// Render the "reuse these existing tags" instruction, or empty if the campaign
/// has no tags yet.
fn build_tag_library_block(known_tags: &[String], de: bool) -> String {
    let tags: Vec<&str> = known_tags
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if tags.is_empty() {
        return String::new();
    }
    let list = tags.join(", ");
    if de {
        format!(
            "\nVorhandene Tags dieser Kampagne (Tag-Bibliothek): {list}\n\
             Verwende für `tags` MÖGLICHST diese vorhandenen Tags (exakt gleiche Schreibweise), \
             damit Tags über alle Sitzungen konsistent bleiben. Erfinde nur dann einen neuen Tag, \
             wenn wirklich kein passender existiert.\n"
        )
    } else {
        format!(
            "\nExisting tags for this campaign (tag library): {list}\n\
             For `tags`, PREFER reusing these existing tags (exact same spelling) so tags stay \
             consistent across sessions. Only invent a new tag when none of the existing ones fit.\n"
        )
    }
}
