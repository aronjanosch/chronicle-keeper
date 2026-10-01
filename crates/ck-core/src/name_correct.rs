//! Name-aware transcript post-correction (Phase 33A). ASR has no vocabulary for
//! fantasy names ("Sylvaine" → "sylvan"); the world already knows every name
//! that matters — page titles plus `aliases`. After ASR, before the transcript
//! is written, words that *sound like* a known name are rewritten to it.
//! Deterministic, model-agnostic (also fixes cloud ASR and imported text), and
//! conservative: a wrong swap is worse than a missed one, so short names must
//! match phonetically exactly and every change is reported.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Correction {
    pub from: String,
    pub to: String,
    pub count: usize,
}

struct Entry {
    display: String,
    words: usize,
    norm: String,
    /// vowel-collapsing key (long names)
    key: String,
    /// vowel-preserving key (short names)
    strict: String,
}

pub struct Vocabulary {
    entries: Vec<Entry>,
    known: HashSet<String>,
    buckets: HashMap<(usize, char), Vec<usize>>,
    max_words: usize,
}

/// Names shorter than this are too easily confused with ordinary words.
const MIN_NAME_LEN: usize = 4;
/// Long names (this many letters or more) may be matched in lowercase text
/// within a small edit distance; shorter ones only when the transcript wrote
/// them capitalised mid-sentence and they sound identical — otherwise every
/// homophone ("torn" → Thorne) would be rewritten.
const LONG_NAME: usize = 7;
const MIN_WINDOW_LEN: usize = 5;
const MAX_EDITS: usize = 2;
const CLOSE_PHONETIC: f64 = 0.93;

fn fold_char(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        'ß' => 's',
        other => other,
    }
}

fn norm_word(w: &str) -> String {
    w.chars()
        .flat_map(char::to_lowercase)
        .map(fold_char)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// A rough sound-alike key: hard/soft letter pairs merge, a silent final `e`
/// and doubled letters vanish. With `skeleton`, all vowels collapse into one
/// class (tolerant, for long names); without it vowels stay distinct apart
/// from common digraph spellings (strict, for short names).
fn phonetic(word: &str, skeleton: bool) -> String {
    let mut w = norm_word(word);
    if !skeleton {
        for (from, to) in [
            ("ee", "i"),
            ("ea", "i"),
            ("ie", "i"),
            ("oo", "u"),
            ("ou", "u"),
            ("ai", "a"),
            ("ay", "a"),
        ] {
            w = w.replace(from, to);
        }
    }
    for (from, to) in [
        ("ph", "f"),
        ("ck", "k"),
        ("kn", "n"),
        ("wr", "r"),
        ("gh", ""),
        ("th", "t"),
        ("sh", "s"),
        ("ch", "k"),
        ("qu", "kv"),
    ] {
        w = w.replace(from, to);
    }
    if w.len() > 3 && w.ends_with('e') && !w[..w.len() - 1].ends_with(is_vowel) {
        w.pop();
    }
    let chars: Vec<char> = w.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let next = chars.get(i + 1).copied().unwrap_or(' ');
        let mapped = match c {
            'a' | 'e' | 'i' | 'o' | 'u' | 'y' if skeleton => 'a',
            'y' => 'i',
            'c' if matches!(next, 'e' | 'i' | 'y') => 's',
            'c' | 'q' => 'k',
            'z' => 's',
            'w' => 'v',
            'x' => 'k',
            other => other,
        };
        if !out.ends_with(mapped) {
            out.push(mapped);
        }
    }
    out
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

fn jaro_winkler(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let range = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut b_used = vec![false; b.len()];
    let mut a_match = Vec::new();
    for (i, &ca) in a.iter().enumerate() {
        let lo = i.saturating_sub(range);
        let hi = (i + range + 1).min(b.len());
        if let Some(j) = (lo..hi).find(|&j| !b_used[j] && b[j] == ca) {
            b_used[j] = true;
            a_match.push(ca);
        }
    }
    if a_match.is_empty() {
        return 0.0;
    }
    let b_match: Vec<char> = b
        .iter()
        .zip(&b_used)
        .filter(|(_, u)| **u)
        .map(|(c, _)| *c)
        .collect();
    let transpositions = a_match.iter().zip(&b_match).filter(|(x, y)| x != y).count() / 2;
    let m = a_match.len() as f64;
    let jaro = (m / a.len() as f64 + m / b.len() as f64 + (m - transpositions as f64) / m) / 3.0;
    let prefix = a.iter().zip(&b).take(4).take_while(|(x, y)| x == y).count() as f64;
    jaro + prefix * 0.1 * (1.0 - jaro)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push(
                (prev[j] + usize::from(ca != *cb))
                    .min(prev[j + 1] + 1)
                    .min(cur[j] + 1),
            );
        }
        prev = cur;
    }
    prev[b.len()]
}

impl Vocabulary {
    /// Build from display names (titles and aliases). Duplicates and names too
    /// short to trust are dropped.
    pub fn new(names: impl IntoIterator<Item = String>) -> Self {
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        for display in names {
            let display = display.trim().to_string();
            let words: Vec<&str> = display.split_whitespace().collect();
            let norm = words
                .iter()
                .map(|w| norm_word(w))
                .collect::<Vec<_>>()
                .join(" ");
            let flat = norm.replace(' ', "");
            if flat.chars().count() < MIN_NAME_LEN
                || words.len() > 3
                || !display.chars().any(char::is_alphabetic)
                || !seen.insert(norm.clone())
            {
                continue;
            }
            entries.push(Entry {
                key: phonetic(&flat, true),
                strict: phonetic(&flat, false),
                words: words.len(),
                norm,
                display,
            });
        }
        let known = entries.iter().map(|e| e.norm.clone()).collect();
        let mut buckets: HashMap<(usize, char), Vec<usize>> = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            if let Some(c) = e.key.chars().next() {
                buckets.entry((e.words, c)).or_default().push(i);
            }
        }
        let max_words = entries.iter().map(|e| e.words).max().unwrap_or(0);
        Self {
            entries,
            known,
            buckets,
            max_words,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn best_match(&self, window: &[&str], mid_capitalized: bool) -> Option<(&Entry, usize)> {
        let norm = window
            .iter()
            .map(|w| norm_word(w))
            .collect::<Vec<_>>()
            .join(" ");
        let flat = norm.replace(' ', "");
        if flat.chars().count() < MIN_NAME_LEN || self.known.contains(&norm) {
            return None;
        }
        let key = phonetic(&flat, true);
        let strict = phonetic(&flat, false);
        let first = key.chars().next()?;
        let mut best: Option<(&Entry, (usize, u32))> = None;
        let mut tie = false;
        for &i in self.buckets.get(&(window.len(), first))? {
            let e = &self.entries[i];
            let e_flat = e.norm.replace(' ', "");
            // "Gasthauses" is the genitive of the page "Gasthaus", not a mishearing
            if flat.starts_with(&e_flat) && flat.chars().count() - e_flat.chars().count() <= 3 {
                continue;
            }
            let edits = levenshtein(&e_flat, &flat);
            let long = e_flat.chars().count() >= LONG_NAME;
            let ok = if long {
                flat.chars().count() >= MIN_WINDOW_LEN
                    && edits <= MAX_EDITS.min(e_flat.chars().count() / 4)
                    && jaro_winkler(&e.key, &key) >= CLOSE_PHONETIC
            } else {
                mid_capitalized
                    && edits <= MAX_EDITS
                    && (e.strict == strict
                        || (edits == 1
                            && e_flat.chars().count() >= 5
                            && e_flat.chars().count().abs_diff(flat.chars().count()) == 1
                            && e.key == key))
            };
            if !ok {
                continue;
            }
            // closer spelling wins; then closer sound
            let rank = (edits, ((1.0 - jaro_winkler(&e.key, &key)) * 1e6) as u32);
            match best {
                Some((_, r)) if rank == r => tie = true,
                Some((_, r)) if rank > r => {}
                _ => {
                    best = Some((e, rank));
                    tie = false;
                }
            }
        }
        if tie {
            return None;
        }
        best.map(|(e, (edits, _))| (e, edits))
    }

    /// Rewrite sound-alikes of known names. Returns the new text and one
    /// `Correction` per distinct (from → to) with how often it fired.
    pub fn correct(&self, text: &str) -> (String, Vec<Correction>) {
        if self.is_empty() {
            return (text.to_string(), Vec::new());
        }
        // word spans: alphabetic runs, allowing inner apostrophes/hyphens
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut start: Option<usize> = None;
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        for (idx, &(pos, c)) in chars.iter().enumerate() {
            let inner = matches!(c, '\'' | '’' | '-')
                && start.is_some()
                && chars.get(idx + 1).is_some_and(|(_, n)| n.is_alphabetic());
            if c.is_alphabetic() || inner {
                start.get_or_insert(pos);
            } else if let Some(s) = start.take() {
                spans.push((s, pos));
            }
        }
        if let Some(s) = start {
            spans.push((s, text.len()));
        }

        // `[Speaker]` label lines are never rewritten
        let mut label_ranges = Vec::new();
        let mut off = 0;
        for line in text.split_inclusive('\n') {
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                label_ranges.push((off, off + line.len()));
            }
            off += line.len();
        }
        spans.retain(|(a, _)| !label_ranges.iter().any(|(s, e)| a >= s && a < e));
        let lower = text.to_lowercase();
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0;
        let mut counts: HashMap<(String, String), (String, usize)> = HashMap::new();
        let mut i = 0;
        while i < spans.len() {
            let mut replaced = false;
            for k in (1..=self.max_words.min(spans.len() - i)).rev() {
                // a multi-word window may only span plain spaces
                let joined = (i..i + k - 1)
                    .all(|j| text[spans[j].1..spans[j + 1].0].chars().all(|c| c == ' '));
                if !joined {
                    continue;
                }
                let window: Vec<&str> = (i..i + k).map(|j| &text[spans[j].0..spans[j].1]).collect();
                let mid_capitalized = i > 0
                    && window[0].chars().next().is_some_and(char::is_uppercase)
                    && text[spans[i - 1].1..spans[i].0]
                        .chars()
                        .all(|c| matches!(c, ' ' | ',' | ';' | '-' | '—'));
                if let Some((e, edits)) = self.best_match(&window, mid_capitalized) {
                    // two edits away is only trusted when the same mishearing repeats
                    let phrase = text[spans[i].0..spans[i + k - 1].1].to_lowercase();
                    if edits >= 2 && lower.matches(&phrase).count() < 2 {
                        continue;
                    }
                    // a changed short word in a phrase ("Türe" → "Tore") is another word, not a mishearing
                    if k > 1
                        && window
                            .iter()
                            .zip(e.norm.split(' '))
                            .any(|(w, n)| norm_word(w) != n && n.chars().count() < MIN_WINDOW_LEN)
                    {
                        continue;
                    }
                    let (from_start, to_end) = (spans[i].0, spans[i + k - 1].1);
                    out.push_str(&text[cursor..from_start]);
                    out.push_str(&e.display);
                    cursor = to_end;
                    counts
                        .entry((text[from_start..to_end].to_lowercase(), e.display.clone()))
                        .or_insert_with(|| (text[from_start..to_end].to_string(), 0))
                        .1 += 1;
                    i += k;
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                i += 1;
            }
        }
        out.push_str(&text[cursor..]);
        let mut fixes: Vec<Correction> = counts
            .into_iter()
            .map(|((_, to), (from, count))| Correction { from, to, count })
            .collect();
        fixes.sort_by(|a, b| b.count.cmp(&a.count).then(a.from.cmp(&b.from)));
        (out, fixes)
    }
}

/// Titles and aliases of the world's canon pages — the names ASR can't know.
pub fn vocabulary_from_index(conn: &rusqlite::Connection) -> crate::error::AppResult<Vocabulary> {
    let mut stmt = conn.prepare("SELECT title, COALESCE(frontmatter, '{}'), kind FROM pages")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut names = Vec::new();
    for (title, fm, kind) in rows.filter_map(Result::ok) {
        if !crate::vault::is_canon_kind(kind.as_deref()) {
            continue;
        }
        names.push(title);
        if let Ok(serde_json::Value::Object(m)) = serde_json::from_str(&fm) {
            match m.get("aliases") {
                Some(serde_json::Value::Array(a)) => {
                    names.extend(a.iter().filter_map(|v| v.as_str()).map(String::from));
                }
                Some(serde_json::Value::String(a)) => names.push(a.clone()),
                _ => {}
            }
        }
    }
    Ok(Vocabulary::new(names))
}

/// Correct a session transcript against its world's names. Best effort: any
/// failure to find the world or index leaves the text untouched. When anything
/// changed, the untouched text is kept as `transcript.raw.md` and the diff as
/// `name_corrections.json` next to it, so a bad rule is visible and reversible.
pub fn correct_transcript(
    state: &crate::state::AppState,
    session_id: &str,
    text: String,
) -> (String, Vec<Correction>) {
    let Some(session_dir) = state
        .with_db(|conn| crate::store::sessions::session_path_of(conn, session_id))
        .ok()
        .flatten()
        .map(std::path::PathBuf::from)
    else {
        return (text, Vec::new());
    };
    let Some(world_root) = session_dir.parent().and_then(|p| p.parent()) else {
        return (text, Vec::new());
    };
    let Ok(Some(cfg)) = crate::world_config::read(world_root) else {
        return (text, Vec::new());
    };
    let vocab = state
        .with_index(&cfg.codex_dir(world_root), vocabulary_from_index)
        .ok()
        .and_then(|r| r.ok());
    let Some(vocab) = vocab.filter(|v| !v.is_empty()) else {
        return (text, Vec::new());
    };
    let (fixed, fixes) = vocab.correct(&text);
    if fixes.is_empty() {
        return (text, fixes);
    }
    let _ = std::fs::write(session_dir.join("transcript.raw.md"), &text);
    if let Ok(json) = serde_json::to_string_pretty(&fixes) {
        let _ = std::fs::write(session_dir.join("name_corrections.json"), json);
    }
    (fixed, fixes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab(names: &[&str]) -> Vocabulary {
        Vocabulary::new(names.iter().map(|s| s.to_string()))
    }

    #[test]
    fn fixes_misheard_fantasy_names() {
        let v = vocab(&["Sylvaine", "Aethric Reach", "Kaelthorn", "Ashfall"]);
        let (t, fixes) =
            v.correct("Sylvan told us the Ethric Reach fell. Kalthorn nodded, sylvan left.");
        assert_eq!(
            t,
            "Sylvaine told us the Aethric Reach fell. Kaelthorn nodded, Sylvaine left."
        );
        assert_eq!(fixes[0].count, 2);
        assert_eq!(
            (fixes[0].from.as_str(), fixes[0].to.as_str()),
            ("Sylvan", "Sylvaine")
        );
    }

    #[test]
    fn leaves_correct_and_ordinary_words_alone() {
        let v = vocab(&["Bram", "Mira", "Sylvaine", "Ashfall"]);
        let text = "Bram bran the brim; mere Mira said sylvain and Ashfall, Ash fell.";
        let (t, fixes) = v.correct(text);
        // Bram/Mira exact stay; "bran"/"brim"/"mere" must not turn into names
        assert_eq!(
            t,
            "Bram bran the brim; mere Mira said Sylvaine and Ashfall, Ash fell."
        );
        assert_eq!(fixes.len(), 1);
    }

    #[test]
    fn short_names_need_an_exact_sound_match() {
        let v = vocab(&["Mira"]);
        // "Meera" sounds the same; "Mora" and "Mirth" do not
        let (t, _) = v.correct("Then Meera sang, Mora slept, Mirth grew, meera left.");
        assert_eq!(t, "Then Mira sang, Mora slept, Mirth grew, meera left.");
    }

    #[test]
    fn a_word_that_is_another_known_name_is_never_rewritten() {
        let v = vocab(&["Sylvaine", "Sylvain"]);
        let (t, fixes) = v.correct("Sylvain met Sylvaine.");
        assert_eq!(t, "Sylvain met Sylvaine.");
        assert!(fixes.is_empty());
    }

    #[test]
    fn multi_word_names_span_spaces_only() {
        let v = vocab(&["Silver Court"]);
        let (t, _) = v.correct("The Silvur Court wins. Silver.\nCourt is out.");
        assert_eq!(t, "The Silver Court wins. Silver.\nCourt is out.");
    }

    #[test]
    fn ambiguous_matches_are_skipped() {
        let v = vocab(&["Kaelthorn", "Kailthorn"]);
        let (t, fixes) = v.correct("Kaylthorn arrived.");
        assert_eq!(t, "Kaylthorn arrived.");
        assert!(fixes.is_empty());
    }

    #[test]
    fn unicode_and_speaker_lines_survive() {
        let v = vocab(&["Ælfric", "Mörwen"]);
        let (t, _) = v.correct("[Anna]\nDa kam Morven und sagte etwas.");
        assert_eq!(t, "[Anna]\nDa kam Mörwen und sagte etwas.");
    }

    #[test]
    fn inflected_forms_of_a_name_are_left_alone() {
        let v = vocab(&["Gasthaus", "Krankenstation"]);
        let text = "Im Gasthauses-Keller, vor dem Gasthauses, in Krankenstationen.";
        let (t, fixes) = v.correct(text);
        assert_eq!(t, text);
        assert!(fixes.is_empty());
    }

    #[test]
    fn changed_short_words_in_a_phrase_are_not_mishearings() {
        let v = vocab(&["Die Tore"]);
        let text = "Du siehst die Türe. Dann die Türe wieder.";
        let (t, fixes) = v.correct(text);
        assert_eq!(t, text);
        assert!(fixes.is_empty());
    }

    #[test]
    fn a_dropped_vowel_in_a_short_name_is_fixed_but_a_swapped_one_is_not() {
        let v = vocab(&["Dagoon", "Rigel", "Zange"]);
        let (t, _) = v.correct("Dann kam Dagon und die Regel, mit Zunge.");
        assert_eq!(t, "Dann kam Dagoon und die Regel, mit Zunge.");
    }

    #[test]
    fn junk_names_are_dropped() {
        let v = vocab(&["Al", "1234", "A B C D E", "Real Name Here Now"]);
        assert!(v.is_empty());
    }
}
