---
name: About Chronicle Keeper
description: How Chronicle Keeper itself works — world folder layout, sessions pipeline, Atlas, Timeline and calendar setup, templates, packs, backups, your own tools and permission modes. Use when the user asks how to do something in the app, where a setting or feature lives, or what you can and cannot do.
---

Answer app questions from this page, not from guesses. If something isn't covered here, say
so and point at the nearest surface instead of inventing a menu path.

## A world on disk

One world = one portable folder:

- `Codex/` — the wiki pages (`.md`, YAML frontmatter + body). Files are the truth.
- `Sessions/<NNN>/` — `audio/`, `transcript.md`, `summary.md`, `session.toml`.
- `Assets/` — pasted/uploaded images. `Inbox/` — quick-capture notes (tag `#inbox`).
- `.ck/` — `config.toml` (identity, players, kind schemas, `[calendar]`), `index.db`
  (rebuildable search/link cache — safe to delete), `templates/`, `keeper/` (your memory and
  World Brief), `history/`, `trash/`, `checkpoints/`.
- `Backups/` — world zips, newest 10 kept, also written when the app closes.

`AGENTS.md` in the world folder (or Codex/) holds standing instructions the user wants you to
follow every turn.

## Sessions pipeline

Craig (Discord) ZIP → label speakers → on-device transcription → summary. Transcripts can also
be imported (SRT, WebVTT, whisper JSON, plain text). Transcription runs locally (Parakeet by
default; other local models or a cloud ASR key selectable in Settings). Names misheard by the
ASR are corrected against page titles and `aliases:` — so good aliases improve transcripts.
Summaries use each page's `summary:` one-liner, so keep those current.
There is no separate "Update the Codex" screen any more: after a session the user launches the
**review-session** skill into a chat and you propose the page edits.
Session prep is a Codex page with `kind: prep` (skill **prepare-session**).

## Surfaces

- **Atlas** — map image with pins (each owns or links a page), regions, scale, nested maps.
  You can read maps, measure distances and edit pins, regions and scale; creating a map or
  its art is the user's.
- **Timeline** — pages with `date:` ordered on the world calendar. Custom months/eras go in
  `[calendar]` of `.ck/config.toml`; `order:`/`seq:` sorts worlds with no calendar.
- **Graph** — links and typed relations; local scope, depth, presets.
- **Search** — ⌘K palette; full-text screen with kind/tag/folder/date/property facets.
- **Codex** — explorer, tabs, editor with `/` menu (inserts plus Ask-Keeper skills).
- **Templates** — visible `_templates/` folder in the Codex; editable, pickable when creating
  a page. "Promote" turns an inbox capture into a kinded page.
- **Packs** — genre packs seed kinds/templates (merge-only); world packs export/import a
  world slice with a plan step and rollback.
- **Foundry bridge** — one-way projection of Codex/Atlas into FoundryVTT plus live-play reads
  (skill **foundry-bridge**). Off unless configured.

## Safety nets

Every save snapshots page history (yours are tagged keeper-origin, restorable by the user);
deletes go to a 30-day trash; your writes are checkpointed per chat and undoable by the user.
Shell, web and Foundry writes are not undoable.

## You

- **Modes** — ReadOnly (no writes), Ask (approve each write), Plan (propose first), AcceptEdits
  (page writes auto-apply; deletes/renames/moves still ask), Yolo (nothing asks). Shell, web and
  Foundry calls always ask except in Yolo.
- **Capabilities** — Settings → Keeper capabilities can remove web, Foundry or shell tools
  entirely. If a tool is missing, that switch or the mode is the likely reason — say so.
- **Memory** — a per-world notebook (`write_memory`) plus the World Brief the user can ask you
  to regenerate. `/compact` summarises a long chat.
- **Tools beyond the basics** — `read_timeline`, `read_relations`, `page_history` and
  `list_trash` read the timeline, typed relations, saved page versions and the trash.
  `restore_page` brings back an earlier version or a single trashed page; `edit_map` moves,
  edits or deletes pins and regions and sets a map's scale (`place_pin` adds pins). `todo_write`
  keeps a visible checklist, `ask_user` asks the user a question with buttons, and `delegate`
  hands a read-only research sweep to a worker with its own context.
- **Cannot do** — create or delete maps or swap their art, edit settings or world config
  (calendar, players, kinds), run transcription or summaries, restore whole folders from the
  trash, touch other worlds. Tell the user which surface does it instead.
