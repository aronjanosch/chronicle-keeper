---
name: Foundry VTT bridge
description: How the one-way Codex → FoundryVTT Journal projection works (sync_foundry), how the ad-hoc create tools (foundry_create_actor/scene/rolltable) behave, and the live-play read tools (foundry_list_actors/get_actor/scene_state/lookup/system_info + foundry_post_chat). Pull before pushing the world, making a table-side document, answering a question about the live table, or when the user asks about the mirror.
---

## What the bridge does

Chronicle Keeper can project the Codex into a running **FoundryVTT** world as Journal
entries, for use at the table. It is **one-way**: CK is the source of truth, Foundry is a
read-only mirror. Nothing in Foundry is ever read back into the Codex — edits made in
Foundry are overwritten on the next sync.

The push is driven by the **`sync_foundry`** tool. It always asks for the user's approval
first, and there is **no remote undo** (unlike Codex page history) — once pushed, the only
way back is another sync. Say so plainly if the user seems to expect a safety net.

## What maps to what

- Each Codex page → one `JournalEntry` with a single text page (its body rendered to HTML).
- Each Codex folder → a Foundry journal **folder**, nested to match the vault tree.
- `[[wikilinks]]` → Foundry `@UUID` links when the target page is also synced; otherwise
  they fall back to plain text (no broken-link noise). `[[Page|Label]]` keeps the label.
- Each Atlas **map** → a Foundry **Scene**: the map art is uploaded as the scene background,
  and every pin whose page is also synced becomes a map **Note** on the scene, linked to that
  page's journal. Pins without a linked page are skipped.
- A page or map deleted from the Codex is **deleted** from Foundry on the next sync.
- Re-syncing is idempotent: CK owns the identity map (`.ck/foundry-map.json`: pages, folders,
  scenes), so journals and scenes are updated in place — never duplicated.

Only the page **body** is projected. Frontmatter (kind, summary, typed relations) is not
sent; infoboxes and the Relations panel are CK-side surfaces.

## When to run it

- The user asks to push / publish / sync / mirror the world to Foundry or "the table".
- After a batch of edits they want reflected in the live game.

Do **not** run it speculatively. It is a remote, approval-gated, non-undoable action — only
call `sync_foundry` when the user has clearly asked to update Foundry. The tool only appears
when the bridge is configured (Settings → Foundry VTT bridge: server URL, API user id,
password); if it is missing, tell the user to configure it there rather than guessing.

## Ad-hoc create tools (quick at-the-table needs)

Separate from the full sync, three tools make a single Foundry document on demand — useful
mid-session ("I need a quick loot table", "drop in an NPC", "give me a blank battle map"):

- **`foundry_create_actor`** — `name` + optional `actor_type` (defaults to `npc`), and optional
  `system` (the stat block) + `items`. Omit `system` for a bare named placeholder; pass it to
  create a **fully statted** NPC (see "Statting an NPC" below).
- **`foundry_create_scene`** — `name` + optional `width`/`height` (default 3000×3000). Blank
  canvas, no background. For a *map-backed* scene, use `sync_foundry` (it uploads atlas art).
- **`foundry_create_rolltable`** — `name` + `entries`, each `{ text, weight? }`. Entries tile
  the roll range in order (weight = how many faces), and the table formula is set to match.
  This is the loot/encounter-table tool.

How they differ from `sync_foundry`, say so if the user might assume otherwise:

- **Scenes and tables are bare.** `foundry_create_scene`/`_rolltable` set only the name (and
  size/results) — no further mechanics. `foundry_create_actor` *can* be statted (see below); left
  unstatted it is a named placeholder the user finishes in Foundry. Don't claim a playable monster
  was created unless you passed a `system` block.
- **Fire-and-forget — not tracked.** Unlike synced pages, these are **not** in
  `.ck/foundry-map.json`. Calling a create tool twice makes **two** documents (no dedup), and
  they are not linked back to any Codex page. They are one-shot conveniences, not part of the
  mirrored world.
- Same remote, approval-gated, no-undo nature as `sync_foundry`; only available when the bridge
  is configured.

If the user wants a roll table or NPC that should *live in the world* (be re-synced, linked),
make a Codex page for it and `sync_foundry`; use these create tools only for throwaway,
in-the-moment table aids.

### Statting an NPC for the user's system

CK ships **no per-system schema** — yet it can make a fully statted NPC in *any* system, the
same way the read tools work: you read a real example and mirror its shape.

1. **Learn the schema.** Two ways, in order:
   - **`foundry_system_info` with the type** (e.g. `{ doc: "Actor", type: "npc" }`) — returns the
     system's default `system` data model, the exact field paths to fill. Works on an **empty
     world**, no existing actor required. This is the first choice.
   - If it reports a **DataModel** system (no static schema — e.g. Daggerheart), fall back to
     **`foundry_get_actor`** on an existing same-type actor to read its real `system` shape. If the
     world has none, make a bare stub (and say so) or `foundry_lookup` a compendium statblock to
     base it on — don't invent field paths blind.
2. **Fill.** Author a `system` object of the **same shape**, with stats appropriate to the NPC the
   user described. Add `items` (weapons/spells/features) shaped per the matching Item type if the
   system carries them there. Copy the structure faithfully — guessed field names silently no-op.
3. **Create.** Call `foundry_create_actor` with `name`, `actor_type`, the `system` block, and any
   `items`. CK writes them through verbatim.

After creating, suggest the user open the sheet to confirm it imported cleanly (there is no remote
undo).

After a sync, report the counts it returns (created / updated / deleted) and surface any
per-page errors instead of claiming a clean run.

## Live-play reads (asking the table questions)

The bridge is one-way for the **Codex** — but during a live session the Keeper can *read* the
running table to answer the GM's questions. These reads are **ephemeral lookups**: they query
the live world and return an answer, never write anything back to the Codex (the "Foundry is
never truth" rule is about codex sync, not about asking the table a question). They are
**read-only and need no approval**.

- **`foundry_list_actors`** — every Actor (name + type) in the world. "Who exists at the table?"
- **`foundry_get_actor`** — `name` **or actor id** (pass the `actorId` from `foundry_scene_state`
  to resolve a specific token when two share a name). Core fields, the actor's items, and the **raw `system` stat
  block**. Foundry stores mechanical stats (HP/AC/skill mods) under `system.*` in a
  **game-system-specific** shape — so the tool hands you that raw JSON and **you interpret it**
  for the user's system (5e `system.attributes.hp.value`, Daggerheart's own shape, etc.). Don't
  assume 5e; read what's there.
- **`foundry_scene_state`** — the active scene's name, size, and the tokens on it (with linked
  actor ids you can cross-reference via `foundry_get_actor`). "Who's on the battle map?"
- **`foundry_system_info`** — optional `doc` (Actor/Item) + `type`. The system's id/version, the
  Actor and Item **types** it defines, and (with `type`) that type's **default `system` data model**
  — the exact field paths to fill when statting. This is how you stat an actor on an **empty world**
  with no existing actor to sample. If the system defines its schema in code (a DataModel — e.g.
  Daggerheart), it says so and sends you to `foundry_get_actor` instead.
- **`foundry_lookup`** — `query`. Searches the installed **game system's compendium packs**
  (rules / skills / items) by name. This is the right way to look up what a skill or rule does:
  it matches the GM's actual system + version and works offline — prefer it over guessing. If it
  reports no index was available, the system may load packs lazily; say so rather than inventing
  an answer.

## Posting to the table (the one live write)

- **`foundry_post_chat`** — `message` (markdown, rendered to HTML). Posts to the table's chat
  log. This is the only live-play *write*, so it is **approval-gated, no remote undo** like the
  other writes. Use it only when the user clearly asks to say something to the table (read a box
  text, drop a result in chat) — never to "show your work."

Reads vs writes: questions, lookups, and "what's on the scene" are free reads. Posting to chat,
creating an actor/scene/table, and `sync_foundry` are gated writes. When the GM asks *for* an
NPC or a roll table mid-session, the create tools are the move; when they ask *about* the table,
the read tools are.
