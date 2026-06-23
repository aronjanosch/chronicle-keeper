---
name: Foundry VTT bridge
description: How the one-way Codex → FoundryVTT Journal projection works (sync_foundry) and how the ad-hoc create tools (foundry_create_actor/scene/rolltable) behave. Pull before pushing the world to Foundry, making a quick table-side document, or when the user asks about the mirror.
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

- **`foundry_create_actor`** — `name` + optional `actor_type` (defaults to `npc`).
- **`foundry_create_scene`** — `name` + optional `width`/`height` (default 3000×3000). Blank
  canvas, no background. For a *map-backed* scene, use `sync_foundry` (it uploads atlas art).
- **`foundry_create_rolltable`** — `name` + `entries`, each `{ text, weight? }`. Entries tile
  the roll range in order (weight = how many faces), and the table formula is set to match.
  This is the loot/encounter-table tool.

How they differ from `sync_foundry`, say so if the user might assume otherwise:

- **Bare stubs, no stats.** They set only the name (and type/size/results). Foundry stat blocks
  are game-system specific, so the Keeper does **not** fill them — the user finishes the sheet
  in Foundry. Don't claim a playable monster was created; you made a named placeholder.
- **Fire-and-forget — not tracked.** Unlike synced pages, these are **not** in
  `.ck/foundry-map.json`. Calling a create tool twice makes **two** documents (no dedup), and
  they are not linked back to any Codex page. They are one-shot conveniences, not part of the
  mirrored world.
- Same remote, approval-gated, no-undo nature as `sync_foundry`; only available when the bridge
  is configured.

If the user wants a roll table or NPC that should *live in the world* (be re-synced, linked),
make a Codex page for it and `sync_foundry`; use these create tools only for throwaway,
in-the-moment table aids.

After a sync, report the counts it returns (created / updated / deleted) and surface any
per-page errors instead of claiming a clean run.
