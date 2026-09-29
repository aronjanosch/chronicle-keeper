---
name: Prepare session
description: Prepare an upcoming session on its prep page: an opening, possible scenes, and a reminders checklist, drawn from recent summaries, open threads, unused prep and the Ideas page. Use whenever the user wants to prep, plan or get ready for a session, asks "what should happen next time", or wants ideas for the next game night, even if they don't say "prep".
kinds: [prep]
---

Help the GM get ready for one session. The result is the session's **prep page**, an
ordinary Codex page the GM will keep editing by hand. Prep is *intent*, not canon:
improvisation will change most of it, so go for useful situations, not a script.

## Checklist

Work through these in order. Tick each one off in your head, and mention any you skipped
and why in your final report.

- [ ] **1. Find the page.** Call `read_prep` for the session. If it finds nothing, run
  one `search_pages` for "Session N": the GM may already have a prep page of their own.
  Whichever page you use, make sure its frontmatter has `kind: prep` and `session: N`,
  and add whatever is missing: that keeps it out of canon and attached to the session.
  - If a page exists, read it with `read_page` and build on it in its own format. Never
    replace or reorganise what the GM wrote.
  - If there is none, create `Prep/Session NNN.md` (zero-padded, e.g. `Session 012`)
    using the layout below. The frontmatter `kind: prep` and `session: N` is what
    attaches the page to the session, so both are required.
- [ ] **2. Gather, don't guess, and don't sweep.** Read what the draft needs, stop once
  you have enough, and skip whatever doesn't exist. Tool rounds per message are limited,
  so skip transcripts (summaries are enough), don't load other skills, and don't tour
  every page:
  - the last one or two session summaries (`list_sessions`, `read_summary`)
  - the previous session's prep (`read_prep`): `unused` and `changed` scenes are the
    richest source of material that still makes sense
  - open threads (`query_world` `LIST FROM kind:thread`, then read the ones not resolved)
  - `Prep/Ideas` (if it exists): ideas the GM saved for later
  - any page the GM named, plus the NPCs, places and factions these point at
- [ ] **3. Settle the focus.** If the GM already said what they want (a location, a
  thread, a mood), use that. Otherwise offer two or three possible directions in a
  sentence each and ask which one they want. Keep it to one question, and don't block on
  it: if they want a draft right away, pick the direction with the most loose ends.
- [ ] **4. Draft into the page.**
  - **Opening:** one short paragraph. Where the session starts, what's happening, what
    the players notice first.
  - **Scenes:** 3 to 5, each as a `###` heading plus a few lines. Write each one as a
    situation (who wants what, what is in the way), not a fixed outcome. Link every
    entity as a `[[wikilink]]` by its page title, never a folder path. Mark anything
    you invented rather than took from the world with *(new)* so the GM knows it isn't
    established yet.
  - **Reminders:** `- [ ]` items for promises, debts, clocks, secrets not to forget, and
    rules to look up. Only concrete ones.
  - **Threads:** list the threads in play in the `threads:` frontmatter as wikilinks.
  Write in the world's language. Existing German headings (Einstieg, Szenen, Erinnerungen,
  Notizen) are just as valid.
- [ ] **5. Mark ideas you used.** If a scene draws on an item from `Prep/Ideas`, change
  that item's `- [ ]` to `- [x]` and append `→ [[Session NNN]]`. This keeps the
  Ideas list honest about what is still unused.
- [ ] **6. Report.** Give 2–4 lines: what you wrote, where the material came from, and
  what the GM should decide or flesh out. Don't paste the whole page back.

## Page layout

```markdown
---
kind: prep
session: 12
summary: Trouble at the docks
threads: ["[[Smuggler Ring]]"]
---
## Opening
Rain on the docks; the party arrives as the harbour bell rings.

## Scenes
### The fish market ambush
[[Mara Voss]]'s crew waits between the stalls. They want the ledger back.

## Reminders
- [ ] Lira still owes the party 20 gold

## Notes
```

Leave any `^abc12345` block ids and `> outcome:` lines exactly as they are. The app and
the Review session skill rely on them.

## Boundaries

- Don't write to canon pages (NPCs, places, factions, threads) while preparing. Prep
  isn't fact yet. If an idea needs a new entity, mark it *(new)* in the prep rather than
  creating its page.
- Don't fill every section for the sake of it. Two strong scenes beat five thin ones.
