//! Session preparation as a Codex page (`kind: prep`). The markdown body is
//! the source of truth; cards, outcomes and links are read from its text and
//! every CK-side change is a surgical line edit, so hand-written prose survives.
//!
//! Layout: `## Opening` (the whole section is one card), `## Scenes` (each
//! `###` is a card), `## Reminders` (each top-level list item is a card), and
//! anything else, which is kept verbatim as notes. Cards carry Obsidian block
//! ids (`^c7f2a91b`); a card written by hand has none until CK first edits it,
//! and is addressed by position (`@3`) until then.

use serde_yaml::{Mapping, Value as YamlValue};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::session_prep::{PrepCard, PrepOrigin, PrepOutcome, PrepSection};

pub const KIND: &str = "prep";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sec {
    Opening,
    Scenes,
    Reminders,
    Notes,
    Other,
}

impl Sec {
    fn of(heading: &str) -> Sec {
        let name = heading.trim().trim_end_matches(':').to_lowercase();
        match name.as_str() {
            "opening" | "einstieg" | "auftakt" | "start" => Sec::Opening,
            "scenes" | "szenen" => Sec::Scenes,
            "reminders" | "erinnerungen" => Sec::Reminders,
            "notes" | "notizen" => Sec::Notes,
            _ => Sec::Other,
        }
    }

    fn order(self) -> u8 {
        match self {
            Sec::Opening => 0,
            Sec::Scenes => 1,
            Sec::Reminders => 2,
            Sec::Notes | Sec::Other => 3,
        }
    }

    fn name(self, lang: &str) -> &'static str {
        let de = lang.eq_ignore_ascii_case("de");
        match (self, de) {
            (Sec::Opening, false) => "Opening",
            (Sec::Opening, true) => "Einstieg",
            (Sec::Scenes, false) => "Scenes",
            (Sec::Scenes, true) => "Szenen",
            (Sec::Reminders, false) => "Reminders",
            (Sec::Reminders, true) => "Erinnerungen",
            (Sec::Notes | Sec::Other, false) => "Notes",
            (Sec::Notes | Sec::Other, true) => "Notizen",
        }
    }

    fn of_section(section: PrepSection) -> Sec {
        match section {
            PrepSection::Opening => Sec::Opening,
            PrepSection::Scene => Sec::Scenes,
            PrepSection::Reminder => Sec::Reminders,
        }
    }
}

struct SectionSpan {
    sec: Sec,
    heading: usize,
    end: usize,
}

struct CardSpan {
    section: PrepSection,
    /// Heading (opening, scene) or list line (reminder).
    head: usize,
    end: usize,
    id: Option<String>,
    outcome_line: Option<usize>,
}

/// A prep page split into raw frontmatter and body lines.
pub struct PrepPage {
    frontmatter: Option<String>,
    lines: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Parsed {
    pub cards: Vec<PrepCard>,
    /// Raw `[[wikilink]]` targets from the `threads:` frontmatter list.
    pub threads: Vec<String>,
    pub notes: String,
    pub handoffs: Vec<YamlValue>,
}

impl PrepPage {
    pub fn parse(content: &str) -> PrepPage {
        let content = content.replace("\r\n", "\n");
        let (frontmatter, body) = split(&content);
        PrepPage {
            frontmatter: frontmatter.map(str::to_string),
            lines: body.split('\n').map(str::to_string).collect(),
        }
    }

    /// A fresh page with the section headings in the world's language.
    pub fn new(lang: &str, session_number: Option<i64>, summary: &str) -> PrepPage {
        let mut fm = Mapping::new();
        fm.insert("kind".into(), KIND.into());
        if let Some(n) = session_number {
            fm.insert("session".into(), n.into());
        }
        if !summary.trim().is_empty() {
            fm.insert("summary".into(), summary.trim().into());
        }
        let mut lines = Vec::new();
        for sec in [Sec::Opening, Sec::Scenes, Sec::Reminders, Sec::Notes] {
            lines.push(format!("## {}", sec.name(lang)));
            lines.push(String::new());
        }
        let mut page = PrepPage {
            frontmatter: None,
            lines,
        };
        page.set_frontmatter(fm);
        page
    }

    pub fn render(&self) -> String {
        let body = self.lines.join("\n");
        match &self.frontmatter {
            Some(fm) => format!("---\n{}\n---\n{}", fm.trim_end_matches('\n'), body),
            None => body,
        }
    }

    pub fn read(&self) -> Parsed {
        let fm = self.mapping();
        let origins = fm
            .get("ck_origins")
            .and_then(YamlValue::as_mapping)
            .cloned()
            .unwrap_or_default();
        let cards = self
            .cards()
            .iter()
            .enumerate()
            .map(|(pos, span)| self.card(pos, span, &origins))
            .collect();
        let threads = match fm.get("threads") {
            Some(YamlValue::Sequence(items)) => items
                .iter()
                .filter_map(YamlValue::as_str)
                .filter_map(link_target)
                .collect(),
            Some(YamlValue::String(one)) => link_target(one).into_iter().collect(),
            _ => Vec::new(),
        };
        let handoffs = fm
            .get("ck_handoffs")
            .and_then(YamlValue::as_sequence)
            .cloned()
            .unwrap_or_default();
        Parsed {
            cards,
            threads,
            notes: self.notes(),
            handoffs,
        }
    }

    // ── ops ─────────────────────────────────────────────────────────

    /// Append a card to its section (created in canonical order if missing).
    /// Returns the new card's id.
    pub fn add_card(
        &mut self,
        section: PrepSection,
        title: Option<&str>,
        text: &str,
        origin: Option<&PrepOrigin>,
        lang: &str,
    ) -> AppResult<String> {
        let text = text.trim();
        let title = title.map(str::trim).filter(|t| !t.is_empty());
        if text.is_empty() && title.is_none() {
            return invalid("A card needs some text");
        }
        let id = new_id();
        let at = self.ensure_section(Sec::of_section(section), lang);
        let block: Vec<String> = match section {
            PrepSection::Opening => {
                if self
                    .cards()
                    .iter()
                    .any(|c| c.section == PrepSection::Opening)
                {
                    return Err(AppError::Conflict(
                        "This session already has an opening — replace it instead".into(),
                    ));
                }
                let heading = self.sections()[at].heading;
                self.lines[heading] = with_id(&self.lines[heading], &id);
                let mut block = Vec::new();
                if let Some(t) = title {
                    block.push(format!("**{t}**"));
                }
                block.extend(text.lines().map(str::to_string));
                block
            }
            PrepSection::Scene => {
                let heading = title.unwrap_or_else(|| first_words(text));
                let mut block = vec![format!("### {heading} ^{id}")];
                if !text.is_empty() {
                    block.extend(text.lines().map(str::to_string));
                }
                block
            }
            PrepSection::Reminder => {
                let mut lines = text.lines();
                let first = lines.next().unwrap_or_default();
                let lead = match title {
                    Some(t) if !first.is_empty() => format!("**{t}**: {first}"),
                    Some(t) => format!("**{t}**"),
                    None => first.to_string(),
                };
                let mut block = vec![format!("- [ ] {lead} ^{id}")];
                block.extend(lines.map(|l| format!("  {l}")));
                block
            }
        };
        let span = &self.sections()[at];
        let insert_at = last_content(&self.lines, span.heading, span.end);
        let separated = section != PrepSection::Reminder
            || !self
                .cards()
                .iter()
                .any(|c| c.section == PrepSection::Reminder && c.end >= insert_at);
        let mut chunk = Vec::new();
        if separated && insert_at > span.heading + 1 {
            chunk.push(String::new());
        }
        chunk.extend(block);
        self.insert(insert_at, chunk);
        if let Some(origin) = origin {
            let mut fm = self.mapping();
            let mut origins = fm
                .get("ck_origins")
                .and_then(YamlValue::as_mapping)
                .cloned()
                .unwrap_or_default();
            let mut entry = Mapping::new();
            entry.insert("session_id".into(), origin.session_id.clone().into());
            entry.insert("item_id".into(), origin.item_id.clone().into());
            origins.insert(id.clone().into(), entry.into());
            fm.insert("ck_origins".into(), origins.into());
            self.set_frontmatter(fm);
        }
        Ok(id)
    }

    /// Replace the opening's text (creating the opening if there is none).
    pub fn replace_opening(&mut self, text: &str, lang: &str) -> AppResult<String> {
        let Some(pos) = self
            .cards()
            .iter()
            .position(|c| c.section == PrepSection::Opening)
        else {
            return self.add_card(PrepSection::Opening, None, text, None, lang);
        };
        let id = self.ensure_id(pos);
        let span = &self.cards()[pos];
        let (head, end) = (span.head, last_content(&self.lines, span.head, span.end));
        let mut block: Vec<String> = text.trim().lines().map(str::to_string).collect();
        if end < span.end || span.end == self.lines.len() {
            block.push(String::new());
        }
        self.lines.splice(head + 1..end.max(head + 1), block);
        self.trim_blank_runs();
        Ok(id)
    }

    pub fn set_outcome(&mut self, card: &str, outcome: PrepOutcome, note: &str) -> AppResult<()> {
        let pos = self.position(card)?;
        self.ensure_id(pos);
        let span = &self.cards()[pos];
        let (head, outcome_line, section) = (span.head, span.outcome_line, span.section);
        let note = note.trim().replace('\n', " ");
        if section == PrepSection::Reminder {
            self.lines[head] = with_checkbox(&self.lines[head], outcome);
            let wanted = (outcome == PrepOutcome::Changed && !note.is_empty())
                .then(|| format!("  > {note}"));
            match (outcome_line, wanted) {
                (Some(i), Some(line)) => self.lines[i] = line,
                (Some(i), None) => {
                    self.lines.remove(i);
                }
                (None, Some(line)) => self.insert(head + 1, vec![line]),
                (None, None) => {}
            }
            return Ok(());
        }
        let wanted = (outcome != PrepOutcome::Unmarked).then(|| {
            let word = outcome_word(outcome);
            if note.is_empty() {
                format!("> outcome: {word}")
            } else {
                format!("> outcome: {word} — {note}")
            }
        });
        match (outcome_line, wanted) {
            (Some(i), Some(line)) => self.lines[i] = line,
            (Some(i), None) => {
                self.lines.remove(i);
            }
            (None, Some(line)) => self.insert(head + 1, vec![line]),
            (None, None) => {}
        }
        Ok(())
    }

    /// Add `[[target]]` to the page's `threads:` list. No-op when present.
    pub fn add_thread(&mut self, target: &str) {
        let target = target.trim().trim_end_matches(".md");
        if target.is_empty() || self.read().threads.iter().any(|t| same(t, target)) {
            return;
        }
        let mut fm = self.mapping();
        let mut threads = match fm.get("threads") {
            Some(YamlValue::Sequence(items)) => items.clone(),
            Some(YamlValue::String(one)) => vec![one.clone().into()],
            _ => Vec::new(),
        };
        threads.push(format!("[[{target}]]").into());
        fm.insert("threads".into(), threads.into());
        self.set_frontmatter(fm);
    }

    /// Mention `[[target]]` in a card. No-op when the card already links it.
    pub fn link(&mut self, card: &str, target: &str) -> AppResult<()> {
        let target = target.trim().trim_end_matches(".md");
        if target.is_empty() {
            return invalid("Choose a page to link");
        }
        let pos = self.position(card)?;
        let parsed = self.read();
        if parsed.cards[pos].links.iter().any(|l| same(l, target)) {
            return Ok(());
        }
        self.ensure_id(pos);
        let span = &self.cards()[pos];
        let link = format!("[[{target}]]");
        if span.section == PrepSection::Reminder {
            let (text, id) = split_block_id(&self.lines[span.head]);
            let joined = format!("{} {link}", text.trim_end());
            self.lines[span.head] = match id {
                Some(id) => format!("{joined} ^{id}"),
                None => joined,
            };
        } else {
            let at = last_content(&self.lines, span.head, span.end);
            self.insert(at, vec![link]);
        }
        Ok(())
    }

    /// Mark the page `kind: prep` (adopting an existing page). True if changed.
    pub fn ensure_kind(&mut self) -> bool {
        let mut fm = self.mapping();
        if fm.get("kind").and_then(YamlValue::as_str) == Some(KIND) {
            return false;
        }
        fm.insert("kind".into(), KIND.into());
        self.set_frontmatter(fm);
        true
    }

    /// Record a carry receipt in frontmatter.
    pub fn push_handoff(&mut self, receipt: YamlValue) {
        let mut fm = self.mapping();
        let mut list = fm
            .get("ck_handoffs")
            .and_then(YamlValue::as_sequence)
            .cloned()
            .unwrap_or_default();
        list.push(receipt);
        fm.insert("ck_handoffs".into(), list.into());
        self.set_frontmatter(fm);
    }

    /// Append free text under the notes section (migration).
    pub fn append_notes(&mut self, text: &str, lang: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let at = self.ensure_section(Sec::Notes, lang);
        let span = &self.sections()[at];
        let insert_at = last_content(&self.lines, span.heading, span.end);
        let mut chunk = Vec::new();
        if insert_at > span.heading + 1 {
            chunk.push(String::new());
        }
        chunk.extend(text.lines().map(str::to_string));
        self.insert(insert_at, chunk);
    }

    // ── scanning ────────────────────────────────────────────────────

    fn sections(&self) -> Vec<SectionSpan> {
        let mut out: Vec<SectionSpan> = Vec::new();
        for (i, line) in self.structural_lines() {
            if let Some(h) = line.strip_prefix("## ") {
                if let Some(prev) = out.last_mut() {
                    prev.end = i;
                }
                out.push(SectionSpan {
                    sec: Sec::of(split_block_id(h).0),
                    heading: i,
                    end: self.lines.len(),
                });
            }
        }
        out
    }

    /// Lines outside fenced code blocks, where headings and list items count.
    fn structural_lines(&self) -> Vec<(usize, &str)> {
        let mut fenced = false;
        let mut out = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            let t = line.trim_start();
            if t.starts_with("```") || t.starts_with("~~~") {
                fenced = !fenced;
                continue;
            }
            if !fenced {
                out.push((i, line.as_str()));
            }
        }
        out
    }

    fn cards(&self) -> Vec<CardSpan> {
        let structural = self.structural_lines();
        let is_structural = |i: usize| structural.binary_search_by_key(&i, |(j, _)| *j).is_ok();
        let mut out = Vec::new();
        for s in self.sections() {
            match s.sec {
                Sec::Opening => {
                    let has_text = (s.heading + 1..s.end).any(|i| !self.lines[i].trim().is_empty());
                    if has_text {
                        out.push(CardSpan {
                            section: PrepSection::Opening,
                            head: s.heading,
                            end: s.end,
                            id: split_block_id(&self.lines[s.heading]).1,
                            outcome_line: self.outcome_line(s.heading + 1, s.end),
                        });
                    }
                }
                Sec::Scenes => {
                    let heads: Vec<usize> = (s.heading + 1..s.end)
                        .filter(|&i| is_structural(i) && self.lines[i].starts_with("### "))
                        .collect();
                    for (k, &h) in heads.iter().enumerate() {
                        let end = heads.get(k + 1).copied().unwrap_or(s.end);
                        out.push(CardSpan {
                            section: PrepSection::Scene,
                            head: h,
                            end,
                            id: split_block_id(&self.lines[h]).1,
                            outcome_line: self.outcome_line(h + 1, end),
                        });
                    }
                }
                Sec::Reminders => {
                    let mut i = s.heading + 1;
                    while i < s.end {
                        if !(is_structural(i) && is_top_item(&self.lines[i])) {
                            i += 1;
                            continue;
                        }
                        let head = i;
                        let mut end = i + 1;
                        let mut j = i + 1;
                        while j < s.end {
                            let l = &self.lines[j];
                            if l.trim().is_empty() {
                                j += 1;
                                continue;
                            }
                            if l.starts_with([' ', '\t']) {
                                j += 1;
                                end = j;
                                continue;
                            }
                            break;
                        }
                        let note = (head + 1..end).find(|&k| {
                            let l = &self.lines[k];
                            l.starts_with([' ', '\t']) && l.trim_start().starts_with("> ")
                        });
                        out.push(CardSpan {
                            section: PrepSection::Reminder,
                            head,
                            end,
                            id: split_block_id(&self.lines[head]).1,
                            outcome_line: note,
                        });
                        i = end.max(head + 1);
                    }
                }
                Sec::Notes | Sec::Other => {}
            }
        }
        out
    }

    fn outcome_line(&self, from: usize, to: usize) -> Option<usize> {
        (from..to).find(|&i| {
            let l = self.lines[i].trim_start();
            l.len() > 10 && l[..10].eq_ignore_ascii_case("> outcome:")
        })
    }

    fn card(&self, pos: usize, span: &CardSpan, origins: &Mapping) -> PrepCard {
        let mut card = PrepCard::new(span.section, String::new());
        card.id = Some(span.id.clone().unwrap_or_else(|| format!("@{pos}")));
        let body_lines = |from: usize| -> String {
            (from..span.end)
                .filter(|&i| Some(i) != span.outcome_line)
                .map(|i| self.lines[i].as_str())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        };
        match span.section {
            PrepSection::Opening => {
                card.text = body_lines(span.head + 1);
                let (outcome, note) = span
                    .outcome_line
                    .map(|i| parse_outcome_line(&self.lines[i]))
                    .unwrap_or_default();
                card.outcome = outcome;
                card.outcome_note = note;
            }
            PrepSection::Scene => {
                let heading = self.lines[span.head].trim_start_matches("### ");
                card.title =
                    Some(split_block_id(heading).0.trim().to_string()).filter(|t| !t.is_empty());
                card.text = body_lines(span.head + 1);
                let (outcome, note) = span
                    .outcome_line
                    .map(|i| parse_outcome_line(&self.lines[i]))
                    .unwrap_or_default();
                card.outcome = outcome;
                card.outcome_note = note;
            }
            PrepSection::Reminder => {
                let (first, _) = split_block_id(&self.lines[span.head]);
                let item = strip_bullet(first);
                let (outcome, item) = split_checkbox(item);
                let (title, item) = split_bold_title(item);
                card.title = title;
                let rest = body_lines(span.head + 1);
                card.text = if rest.is_empty() {
                    item.trim().to_string()
                } else {
                    format!("{}\n{}", item.trim(), rest)
                };
                card.outcome = outcome;
                if outcome == PrepOutcome::Changed {
                    if let Some(i) = span.outcome_line {
                        card.outcome_note = self.lines[i]
                            .trim_start()
                            .trim_start_matches('>')
                            .trim()
                            .to_string();
                    }
                }
            }
        }
        card.links = wikilinks(&card.text);
        if let Some(id) = &span.id {
            if let Some(entry) = origins.get(id.as_str()).and_then(YamlValue::as_mapping) {
                let get = |k: &str| entry.get(k).and_then(YamlValue::as_str).map(str::to_string);
                if let (Some(session_id), Some(item_id)) = (get("session_id"), get("item_id")) {
                    card.origin = Some(PrepOrigin {
                        session_id,
                        item_id,
                    });
                }
            }
        }
        card
    }

    fn notes(&self) -> String {
        let sections = self.sections();
        let mut parts: Vec<String> = Vec::new();
        let first = sections
            .first()
            .map(|s| s.heading)
            .unwrap_or(self.lines.len());
        let preamble = self.lines[..first].join("\n");
        if !preamble.trim().is_empty() {
            parts.push(preamble.trim().to_string());
        }
        let cards = self.cards();
        for s in &sections {
            match s.sec {
                Sec::Notes | Sec::Other => {
                    let text = self.lines[s.heading..s.end].join("\n");
                    let text = if s.sec == Sec::Notes {
                        self.lines[s.heading + 1..s.end].join("\n")
                    } else {
                        text
                    };
                    if !text.trim().is_empty() {
                        parts.push(text.trim().to_string());
                    }
                }
                // Loose text above the first scene or between list items.
                Sec::Scenes | Sec::Reminders => {
                    let loose: Vec<&str> = (s.heading + 1..s.end)
                        .filter(|&i| !cards.iter().any(|c| (c.head..c.end).contains(&i)))
                        .map(|i| self.lines[i].as_str())
                        .collect();
                    let loose = loose.join("\n");
                    if !loose.trim().is_empty() {
                        parts.push(loose.trim().to_string());
                    }
                }
                Sec::Opening => {}
            }
        }
        parts.join("\n\n")
    }

    // ── helpers ─────────────────────────────────────────────────────

    fn position(&self, card: &str) -> AppResult<usize> {
        let cards = self.cards();
        if let Some(pos) = card.strip_prefix('@').and_then(|p| p.parse::<usize>().ok()) {
            if pos < cards.len() && cards[pos].id.is_none() {
                return Ok(pos);
            }
        }
        cards
            .iter()
            .position(|c| c.id.as_deref() == Some(card))
            .ok_or_else(|| AppError::NotFound(format!("No such card in this prep: {card}")))
    }

    /// Give card `pos` a block id if it has none; returns the id.
    fn ensure_id(&mut self, pos: usize) -> String {
        let span = &self.cards()[pos];
        if let Some(id) = &span.id {
            return id.clone();
        }
        let id = new_id();
        let head = span.head;
        self.lines[head] = with_id(&self.lines[head], &id);
        id
    }

    /// Index into `sections()` of the section for `sec`, inserting a heading in
    /// canonical order when the page has none.
    fn ensure_section(&mut self, sec: Sec, lang: &str) -> usize {
        let sections = self.sections();
        if let Some(i) = sections.iter().position(|s| s.sec == sec) {
            return i;
        }
        let at = sections
            .iter()
            .find(|s| s.sec != Sec::Other && s.sec.order() > sec.order())
            .map(|s| s.heading)
            .unwrap_or(self.lines.len());
        let mut chunk = Vec::new();
        if at > 0 && !self.lines[at - 1].trim().is_empty() {
            chunk.push(String::new());
        }
        chunk.push(format!("## {}", sec.name(lang)));
        chunk.push(String::new());
        let appended = at == self.lines.len();
        if appended && self.lines.last().is_some_and(|l| l.is_empty()) && chunk[0].is_empty() {
            chunk.remove(0);
        }
        self.insert(at, chunk);
        self.sections()
            .iter()
            .position(|s| s.sec == sec)
            .expect("section just inserted")
    }

    fn insert(&mut self, at: usize, chunk: Vec<String>) {
        self.lines.splice(at..at, chunk);
    }

    /// Collapse the runs of blank lines a replacement can leave behind.
    fn trim_blank_runs(&mut self) {
        let mut out: Vec<String> = Vec::with_capacity(self.lines.len());
        for line in self.lines.drain(..) {
            if line.trim().is_empty() && out.last().is_some_and(|l| l.trim().is_empty()) {
                continue;
            }
            out.push(line);
        }
        self.lines = out;
    }

    fn mapping(&self) -> Mapping {
        self.frontmatter
            .as_deref()
            .and_then(|fm| serde_yaml::from_str::<Mapping>(fm).ok())
            .unwrap_or_default()
    }

    fn set_frontmatter(&mut self, fm: Mapping) {
        self.frontmatter = serde_yaml::to_string(&fm).ok();
    }
}

/// `(frontmatter, body)`; frontmatter is the text between the `---` fences.
fn split(content: &str) -> (Option<&str>, &str) {
    let Some(rest) = content.strip_prefix("---\n") else {
        return (None, content);
    };
    if let Some(body) = rest.strip_prefix("---\n") {
        return (Some(""), body);
    }
    match rest.find("\n---") {
        Some(end) => {
            let after = &rest[end + 4..];
            let body = after.strip_prefix('\n').unwrap_or(after);
            (Some(&rest[..end]), body)
        }
        None => (None, content),
    }
}

/// `(text, block id)` for a line ending in ` ^id`.
fn split_block_id(line: &str) -> (&str, Option<String>) {
    let t = line.trim_end();
    if let Some(caret) = t.rfind(" ^") {
        let id = &t[caret + 2..];
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return (&t[..caret], Some(id.to_string()));
        }
    }
    (t, None)
}

fn with_id(line: &str, id: &str) -> String {
    let (text, _) = split_block_id(line);
    format!("{} ^{id}", text.trim_end())
}

fn new_id() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_string()
}

fn is_top_item(line: &str) -> bool {
    ["- ", "* ", "+ "].iter().any(|p| line.starts_with(p))
}

fn strip_bullet(line: &str) -> &str {
    line.get(2..).unwrap_or_default()
}

fn split_checkbox(item: &str) -> (PrepOutcome, &str) {
    let b = item.as_bytes();
    if b.len() >= 3 && b[0] == b'[' && b[2] == b']' {
        let outcome = match b[1] {
            b'x' | b'X' => PrepOutcome::Happened,
            b'~' => PrepOutcome::Changed,
            b'-' => PrepOutcome::Unused,
            _ => PrepOutcome::Unmarked,
        };
        return (outcome, item[3..].trim_start());
    }
    (PrepOutcome::Unmarked, item)
}

fn with_checkbox(line: &str, outcome: PrepOutcome) -> String {
    let bullet = &line[..2];
    let (_, rest) = split_checkbox(strip_bullet(line));
    let mark = match outcome {
        PrepOutcome::Unmarked => ' ',
        PrepOutcome::Happened => 'x',
        PrepOutcome::Changed => '~',
        PrepOutcome::Unused => '-',
    };
    format!("{bullet}[{mark}] {rest}")
}

fn split_bold_title(item: &str) -> (Option<String>, &str) {
    let Some(rest) = item.strip_prefix("**") else {
        return (None, item);
    };
    let Some(end) = rest.find("**") else {
        return (None, item);
    };
    let title = rest[..end].trim();
    let after = rest[end + 2..].trim_start_matches([':', ' ', '—', '-']);
    ((!title.is_empty()).then(|| title.to_string()), after)
}

fn outcome_word(outcome: PrepOutcome) -> &'static str {
    match outcome {
        PrepOutcome::Unmarked => "unmarked",
        PrepOutcome::Happened => "happened",
        PrepOutcome::Changed => "changed",
        PrepOutcome::Unused => "unused",
    }
}

fn parse_outcome_line(line: &str) -> (PrepOutcome, String) {
    let value = line.trim_start()[10..].trim();
    let (word, note) = value
        .split_once('—')
        .or_else(|| value.split_once(" - "))
        .unwrap_or((value, ""));
    let outcome = match word.trim().to_lowercase().as_str() {
        "happened" | "passiert" | "done" => PrepOutcome::Happened,
        "changed" | "geändert" | "anders" => PrepOutcome::Changed,
        "unused" | "ungenutzt" | "skipped" => PrepOutcome::Unused,
        _ => PrepOutcome::Unmarked,
    };
    (outcome, note.trim().to_string())
}

/// `[[Target|alias]]` / `[[Target#Heading]]` → `Target`, in order, deduplicated.
pub fn wikilinks(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        if let Some(target) = link_target(&format!("[[{}]]", &after[..end])) {
            if !out.iter().any(|t| same(t, &target)) {
                out.push(target);
            }
        }
        rest = &after[end + 2..];
    }
    out
}

fn link_target(raw: &str) -> Option<String> {
    let inner = raw
        .trim()
        .strip_prefix("[[")
        .and_then(|s| s.strip_suffix("]]"))
        .unwrap_or(raw.trim());
    let target = inner.split(['|', '#']).next().unwrap_or_default().trim();
    (!target.is_empty() && !target.contains("[[")).then(|| target.to_string())
}

fn same(a: &str, b: &str) -> bool {
    a.trim_end_matches(".md")
        .eq_ignore_ascii_case(b.trim_end_matches(".md"))
}

/// Index just past the last non-blank line in `(head, end)`, at least `head + 1`.
fn last_content(lines: &[String], head: usize, end: usize) -> usize {
    (head + 1..end)
        .rev()
        .find(|&i| !lines[i].trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(head + 1)
}

fn first_words(text: &str) -> &str {
    let line = text.lines().next().unwrap_or_default();
    match line.char_indices().nth(60) {
        Some((i, _)) => &line[..i],
        None => line,
    }
}

fn invalid<T>(message: &str) -> AppResult<T> {
    Err(AppError::Unprocessable(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"---
kind: prep
session: 12
threads: ["[[Threads/Smuggler Ring]]"]
---
Intro line.

## Opening ^op000001
Rain on the docks.

## Scenes
Loose scene note.

### The fish market ^c7f2a91b
> outcome: changed — they bribed her
Mara waits. [[NPCs/Mara Voss]]

### Harbourmaster
Talk about [[Harbour|the harbour]].

```
### not a scene
```

## Reminders
- [x] Give Lira the letter ^r1a0c3d9
- [~] **XP**: ask about XP ^r9b3f002
  > gave milestone instead
- plain item
  continued

## Notes
Free text.
"#;

    fn page() -> PrepPage {
        PrepPage::parse(SAMPLE)
    }

    #[test]
    fn reads_cards_outcomes_links_threads_and_notes() {
        let p = page().read();
        let got: Vec<(PrepSection, Option<&str>, &str)> = p
            .cards
            .iter()
            .map(|c| (c.section, c.title.as_deref(), c.id.as_deref().unwrap()))
            .collect();
        assert_eq!(
            got,
            vec![
                (PrepSection::Opening, None, "op000001"),
                (PrepSection::Scene, Some("The fish market"), "c7f2a91b"),
                (PrepSection::Scene, Some("Harbourmaster"), "@2"),
                (PrepSection::Reminder, None, "r1a0c3d9"),
                (PrepSection::Reminder, Some("XP"), "r9b3f002"),
                (PrepSection::Reminder, None, "@5"),
            ]
        );
        assert_eq!(p.cards[0].text, "Rain on the docks.");
        assert_eq!(p.cards[1].outcome, PrepOutcome::Changed);
        assert_eq!(p.cards[1].outcome_note, "they bribed her");
        assert_eq!(p.cards[1].text, "Mara waits. [[NPCs/Mara Voss]]");
        assert_eq!(p.cards[1].links, vec!["NPCs/Mara Voss"]);
        assert_eq!(p.cards[2].links, vec!["Harbour"]);
        assert!(p.cards[2].text.contains("### not a scene"));
        assert_eq!(p.cards[3].outcome, PrepOutcome::Happened);
        assert_eq!(p.cards[4].outcome, PrepOutcome::Changed);
        assert_eq!(p.cards[4].outcome_note, "gave milestone instead");
        assert_eq!(p.cards[4].text, "ask about XP");
        assert_eq!(p.cards[5].text, "plain item\ncontinued");
        assert_eq!(p.threads, vec!["Threads/Smuggler Ring"]);
        assert!(p.notes.contains("Intro line."));
        assert!(p.notes.contains("Loose scene note."));
        assert!(p.notes.contains("Free text."));
    }

    #[test]
    fn untouched_page_renders_byte_identical() {
        assert_eq!(page().render(), SAMPLE);
    }

    #[test]
    fn set_outcome_edits_only_the_marker() {
        let mut p = page();
        p.set_outcome("c7f2a91b", PrepOutcome::Happened, "")
            .unwrap();
        p.set_outcome("r1a0c3d9", PrepOutcome::Unused, "").unwrap();
        p.set_outcome("r9b3f002", PrepOutcome::Unmarked, "")
            .unwrap();
        let out = p.render();
        assert!(out.contains("### The fish market ^c7f2a91b\n> outcome: happened\nMara waits."));
        assert!(out.contains("- [-] Give Lira the letter ^r1a0c3d9"));
        assert!(out.contains("- [ ] **XP**: ask about XP ^r9b3f002\n- plain item"));
        assert!(!out.contains("gave milestone"));
        assert!(out.contains("Intro line.") && out.contains("Free text."));
    }

    #[test]
    fn outcome_on_a_hand_written_card_assigns_an_id() {
        let mut p = page();
        p.set_outcome("@2", PrepOutcome::Changed, "moved to the tavern")
            .unwrap();
        let read = p.read();
        let id = read.cards[2].id.clone().unwrap();
        assert!(!id.starts_with('@'));
        assert_eq!(read.cards[2].outcome_note, "moved to the tavern");
        assert!(p.render().contains(&format!(
            "### Harbourmaster ^{id}\n> outcome: changed — moved to the tavern"
        )));
        assert!(p.set_outcome("@2", PrepOutcome::Happened, "").is_err());
    }

    #[test]
    fn reminder_changed_note_is_an_indented_quote() {
        let mut p = page();
        p.set_outcome("r1a0c3d9", PrepOutcome::Changed, "gave it to Tomas")
            .unwrap();
        let out = p.render();
        assert!(out.contains("- [~] Give Lira the letter ^r1a0c3d9\n  > gave it to Tomas\n"));
        assert_eq!(p.read().cards[3].outcome_note, "gave it to Tomas");
    }

    #[test]
    fn add_card_appends_in_the_right_section() {
        let mut p = page();
        let scene = p
            .add_card(
                PrepSection::Scene,
                Some("Ambush"),
                "Crossbows. [[NPCs/Kell]]",
                None,
                "en",
            )
            .unwrap();
        let rem = p
            .add_card(PrepSection::Reminder, None, "Roll weather", None, "en")
            .unwrap();
        let read = p.read();
        let titles: Vec<_> = read.cards.iter().map(|c| c.title.clone()).collect();
        assert_eq!(titles[3], Some("Ambush".into()));
        assert_eq!(read.cards[3].id.as_deref(), Some(scene.as_str()));
        assert_eq!(read.cards[3].links, vec!["NPCs/Kell"]);
        assert_eq!(read.cards.last().unwrap().id.as_deref(), Some(rem.as_str()));
        let out = p.render();
        assert!(out.contains("```\n\n### Ambush ^"));
        assert!(out.contains("  continued\n- [ ] Roll weather ^"));
        assert!(out.ends_with("## Notes\nFree text.\n"));
    }

    #[test]
    fn a_second_opening_is_refused_but_can_be_replaced() {
        let mut p = page();
        assert!(p
            .add_card(PrepSection::Opening, None, "Other", None, "en")
            .is_err());
        let id = p.replace_opening("Fog instead.", "en").unwrap();
        assert_eq!(id, "op000001");
        let read = p.read();
        assert_eq!(read.cards[0].text, "Fog instead.");
        assert!(p
            .render()
            .contains("## Opening ^op000001\nFog instead.\n\n## Scenes"));
    }

    #[test]
    fn missing_sections_are_created_in_canonical_order() {
        let mut p = PrepPage::parse("---\nkind: prep\n---\n## Notes\nstuff\n");
        p.add_card(PrepSection::Reminder, None, "One", None, "de")
            .unwrap();
        p.add_card(PrepSection::Opening, None, "Start here", None, "de")
            .unwrap();
        let out = p.render();
        let einstieg = out.find("## Einstieg").unwrap();
        let erinnerungen = out.find("## Erinnerungen").unwrap();
        let notes = out.find("## Notes").unwrap();
        assert!(einstieg < erinnerungen && erinnerungen < notes, "{out}");
        assert_eq!(p.read().cards.len(), 2);
    }

    #[test]
    fn new_page_is_a_prep_page_in_the_world_language() {
        let p = PrepPage::new("de", Some(4), "Nebel");
        let out = p.render();
        assert!(out.starts_with("---\nkind: prep\nsession: 4\nsummary: Nebel\n---\n## Einstieg\n"));
        assert!(p.read().cards.is_empty());
    }

    #[test]
    fn threads_links_and_origins_round_trip() {
        let mut p = page();
        p.add_thread("Threads/Smuggler Ring.md");
        p.add_thread("Threads/Guild War");
        p.link("c7f2a91b", "NPCs/Mara Voss").unwrap();
        p.link("r1a0c3d9", "NPCs/Lira").unwrap();
        let origin = PrepOrigin {
            session_id: "s11".into(),
            item_id: "old1".into(),
        };
        let id = p
            .add_card(
                PrepSection::Reminder,
                Some("Debt"),
                "Kell is owed",
                Some(&origin),
                "en",
            )
            .unwrap();
        let read = p.read();
        assert_eq!(
            read.threads,
            vec!["Threads/Smuggler Ring", "Threads/Guild War"]
        );
        assert_eq!(read.cards[1].links, vec!["NPCs/Mara Voss"]);
        assert_eq!(read.cards[3].links, vec!["NPCs/Lira"]);
        assert!(p
            .render()
            .contains("- [x] Give Lira the letter [[NPCs/Lira]] ^r1a0c3d9"));
        let carried = read
            .cards
            .iter()
            .find(|c| c.id.as_deref() == Some(id.as_str()))
            .unwrap();
        assert_eq!(carried.origin.as_ref().unwrap().item_id, "old1");
        assert_eq!(carried.title.as_deref(), Some("Debt"));
    }

    #[test]
    fn plain_markdown_without_frontmatter_still_parses() {
        let p = PrepPage::parse("## Szenen\n### Nebel\nText\n");
        let read = p.read();
        assert_eq!(read.cards.len(), 1);
        assert_eq!(read.cards[0].title.as_deref(), Some("Nebel"));
        assert_eq!(p.render(), "## Szenen\n### Nebel\nText\n");
    }
}
