---
name: Writing Codex page syntax
description: Chronicle Keeper page syntax — transclusion, callouts, typed relations, ck-query, calendar dates, frontmatter keys, plus which app surface shows what. Use before writing or editing a page's body or frontmatter.
---

## Page syntax (Obsidian-flavored)

Beyond plain markdown and [[wikilinks]] (incl. `[[Page|label]]` and `[[Page#Heading]]`),
pages support:

- `![[Page]]` / `![[Page#Heading]]` — transclusion (Obsidian-style embed): the page or
  section renders inline. Reuse canon this way instead of copying text.
- `![[image.png]]` — image embed from the vault (pasted images land in `Assets/`);
  optional `|width`.
- Callouts `> [!note]` / `[!tip]` / `[!warning]` / `[!secret]`, optional title on the same
  line; a `-` suffix starts them collapsed. `[!secret]` is the GM's spoiler box, collapsed
  by default — put twists and foreshadowing there.
- Typed relations: a frontmatter value that is a "[[wikilink]]" is a typed edge and the key
  is the predicate — `location: "[[Ashfall]]"`, `allies:` as a list of links. They feed the
  page's Relations panel and the graph view and survive renames — prefer them over prose
  for structured facts (who serves whom, what is where).
- `date:` frontmatter (`year[-month[-day]] [era]`, e.g. `1374-02-12 DR`) puts a page on the
  world timeline; the `event` kind carries the field by default. Month and era names come
  from `[calendar]` in `.ck/config.toml`.
- Fenced ```ck-query code blocks (Dataview-lite): `LIST FROM #tag AND kind:npc WHERE
  field = [[Page]]` (also `!=`, `contains`) render as a live, self-updating page list — use
  one instead of hand-maintaining "all NPCs in Ashfall"-style index lists.
- The `Inbox/` folder holds the user's quick-capture notes (tagged #inbox) — fleeting
  thoughts waiting to be sorted into real pages.

Standard frontmatter keys: `kind` (drives the infobox — check page_kinds for its fields),
`summary` (the one-liner fed to the AI whenever this page is mentioned in a session —
keep it current when you edit a page), `aliases` (alternate names; wikilinks resolve
through them), `tags` (hierarchical, `character/ranger`).

Caveat: renaming a page rewrites `[[links]]` and frontmatter relations everywhere, but
NOT `![[transclusions]]` of it — after a rename, search for `![[Old Name` and fix those.

## The app (what the user can do, where pages show up)

- **Codex** — explorer + reading view + markdown editor (tabs, slash-menu inserts,
  wikilink autocomplete). Pages are plain `.md` files in the world folder; an import
  flow brings in existing notes.
- **Atlas** — uploaded map art with pins that own or link pages; maps can nest.
- **Timeline** — every `date:`-carrying page plus real-world session dates, ordered on
  the world's own calendar.
- **Graph** — force map of all links, typed relations highlighted.
- **Search** — ⌘K palette and a full-text screen with facets (kind/tag/folder/date);
  session search covers summaries and transcripts.
- **Sessions** — Craig (Discord) recordings → label speakers → on-device transcription →
  summary → "Update the Codex" (AI-proposed page edits the user reviews and commits).
- **Safety nets** — every page save snapshots to history (restorable, yours marked
  keeper-origin), deletes go to a 30-day trash, world backups zip on close. Your edits
  are undoable, so the user can always roll back.
- **You** — this chat panel, with attachments, your memory notebook, and a World Brief
  you maintain.

When the user asks where to see or organise something, point at the right surface.
