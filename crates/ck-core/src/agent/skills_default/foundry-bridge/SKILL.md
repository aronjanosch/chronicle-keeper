---
name: Foundry VTT bridge
description: How the one-way Codex → FoundryVTT Journal projection works and when to run sync_foundry. Pull before pushing the world to Foundry or when the user asks about the table-side mirror.
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
- A page deleted from the Codex is **deleted** from Foundry on the next sync.
- Re-syncing is idempotent: CK owns a `path → journalId` map (`.ck/foundry-map.json`), so
  pages are updated in place — never duplicated.

Only the page **body** is projected. Frontmatter (kind, summary, typed relations) is not
sent; infoboxes and the Relations panel are CK-side surfaces.

## When to run it

- The user asks to push / publish / sync / mirror the world to Foundry or "the table".
- After a batch of edits they want reflected in the live game.

Do **not** run it speculatively. It is a remote, approval-gated, non-undoable action — only
call `sync_foundry` when the user has clearly asked to update Foundry. The tool only appears
when the bridge is configured (Settings → Foundry VTT bridge: server URL, API user id,
password); if it is missing, tell the user to configure it there rather than guessing.

After a sync, report the counts it returns (created / updated / deleted) and surface any
per-page errors instead of claiming a clean run.
