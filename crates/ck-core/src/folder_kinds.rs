//! Folder-name → codex kind, mirroring `frontend/app/folderKinds.js`. Real
//! vaults name their folders in many languages and with ordering prefixes
//! ("1 NPCs", "a_npcs", "01-Orte"), so nothing may assume a folder is called
//! "NPCs" — normalize the name and match its words against a keyword table.

const KEYWORDS: &[(&str, &[&str])] = &[
    (
        "pc",
        &[
            "pc",
            "pcs",
            "player",
            "players",
            "player characters",
            "party",
            "spieler",
            "spielercharaktere",
            "helden",
            "heroes",
            "gruppe",
            "gefaehrten",
            "pj",
            "pjs",
            "personnages joueurs",
            "jugadores",
            "giocatori",
        ],
    ),
    (
        "npc",
        &[
            "npc",
            "npcs",
            "character",
            "characters",
            "people",
            "nsc",
            "nscs",
            "charaktere",
            "personen",
            "figuren",
            "gestalten",
            "pnj",
            "pnjs",
            "personnages",
            "personajes",
            "personaggi",
            "png",
        ],
    ),
    (
        "place",
        &[
            "place",
            "places",
            "location",
            "locations",
            "city",
            "cities",
            "region",
            "regions",
            "world",
            "geography",
            "map",
            "maps",
            "realm",
            "realms",
            "dungeon",
            "dungeons",
            "ort",
            "orte",
            "staedte",
            "stadt",
            "regionen",
            "welt",
            "geographie",
            "karten",
            "laender",
            "reiche",
            "lieux",
            "endroits",
            "lugares",
            "luoghi",
            "sitios",
        ],
    ),
    (
        "faction",
        &[
            "faction",
            "factions",
            "organization",
            "organizations",
            "organisation",
            "organisations",
            "guild",
            "guilds",
            "group",
            "groups",
            "order",
            "orders",
            "fraktion",
            "fraktionen",
            "organisationen",
            "gilden",
            "gruppen",
            "orden",
            "haeuser",
            "facciones",
            "fazioni",
            "guildes",
        ],
    ),
    (
        "item",
        &[
            "item",
            "items",
            "object",
            "objects",
            "artifact",
            "artifacts",
            "artefact",
            "artefacts",
            "loot",
            "treasure",
            "treasures",
            "equipment",
            "relic",
            "relics",
            "gegenstand",
            "gegenstaende",
            "objekt",
            "objekte",
            "artefakt",
            "artefakte",
            "schaetze",
            "schatz",
            "ausruestung",
            "relikte",
            "objets",
            "objetos",
            "oggetti",
            "tesoros",
            "tesori",
        ],
    ),
    (
        "event",
        &[
            "event",
            "events",
            "ereignis",
            "ereignisse",
            "timeline",
            "zeitlinie",
            "evenements",
            "eventos",
            "eventi",
        ],
    ),
    (
        "lore",
        &[
            "lore",
            "history",
            "myth",
            "myths",
            "legend",
            "legends",
            "religion",
            "religions",
            "god",
            "gods",
            "deities",
            "cosmology",
            "calendar",
            "background",
            "knowledge",
            "culture",
            "cultures",
            "geschichte",
            "mythen",
            "legenden",
            "goetter",
            "gottheiten",
            "kosmologie",
            "kalender",
            "hintergrund",
            "wissen",
            "kulturen",
            "sagen",
            "histoire",
            "mythes",
            "legendes",
            "dieux",
            "historia",
            "mitos",
            "leyendas",
            "dioses",
            "storia",
            "miti",
        ],
    ),
];

// "1 NPCs" / "a_npcs" / "01-Orte" / "Städte" → "npcs" / "npcs" / "orte" / "staedte"
fn normalize(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let expanded: String = name
        .to_lowercase()
        .chars()
        .flat_map(|c| match c {
            'ä' => "ae".chars().collect::<Vec<_>>(),
            'ö' => "oe".chars().collect(),
            'ü' => "ue".chars().collect(),
            'ß' => "ss".chars().collect(),
            c => vec![c],
        })
        .collect();
    let stripped: String = expanded
        .nfd()
        .filter(|c| !matches!(*c as u32, 0x0300..=0x036F))
        .collect();
    let head = stripped.trim_start_matches(|c: char| !c.is_ascii_lowercase());
    strip_ordering_prefix(head).trim().to_string()
}

// A single letter followed by separators, then real content: "a_npcs", "b - orte".
fn strip_ordering_prefix(s: &str) -> &str {
    let Some(first) = s.chars().next().filter(char::is_ascii_lowercase) else {
        return s;
    };
    let rest = &s[first.len_utf8()..];
    let trimmed = rest.trim_start_matches([' ', '.', '_', '-']);
    if trimmed.len() < rest.len() && !trimmed.is_empty() {
        trimmed
    } else {
        s
    }
}

fn lookup(word: &str) -> Option<&'static str> {
    for (kind, words) in KEYWORDS {
        for w in *words {
            if w.contains(' ') {
                continue;
            }
            let hit = *w == word
                || word.strip_suffix('s').is_some_and(|s| *w == s)
                || *w == format!("{word}s");
            if hit {
                return Some(kind);
            }
        }
    }
    None
}

/// Best-effort kind for a folder name; `None` when nothing matches.
pub fn kind_for_folder(name: &str) -> Option<&'static str> {
    let n = normalize(name);
    if n.is_empty() {
        return None;
    }
    for (kind, words) in KEYWORDS {
        if words.iter().any(|w| w.contains(' ') && *w == n) {
            return Some(kind);
        }
    }
    n.split([' ', '.', '_', '-', '/', '&', '+', ','])
        .filter(|w| !w.is_empty())
        .find_map(lookup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_prefixed_and_localized_folders() {
        assert_eq!(kind_for_folder("NPCs"), Some("npc"));
        assert_eq!(kind_for_folder("01-Orte"), Some("place"));
        assert_eq!(kind_for_folder("a_npcs"), Some("npc"));
        assert_eq!(kind_for_folder("Städte"), Some("place"));
        assert_eq!(kind_for_folder("Player Characters"), Some("pc"));
        assert_eq!(kind_for_folder("2 Fraktionen"), Some("faction"));
        assert_eq!(kind_for_folder("Zettelkasten"), None);
    }
}
