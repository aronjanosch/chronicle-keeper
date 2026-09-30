---
name: Fix diagnostics
description: Work through the Codex health report — broken links, orphan pages, broken embeds, sync conflicts — and fix them safely with the user. Use when the user asks to clean up, repair, or tidy the world, or mentions broken links or orphans.
---

Turn the Codex health report into a short, safe cleanup. Start with vault_diagnostics.
Unlike an audit, you may make fixes here — but only with the user's approval, and never
destructively on your own.

Ground rules:
- Never delete a page, and never delete or merge a sync-conflict file, without an
  explicit yes for that specific item. Deletion goes to the world trash, but still ask.
- Confirm in batches: show the full list of proposed changes grouped by type, get one
  approval per group, then apply. Do not apply ten edits one silently after another.
- Prefer the smallest fix. Every change is an edit_page (snapshotted and undoable).

Work through the groups in this order:

**Broken [[wikilinks]]**
For each, read the page that contains it and decide which case it is:
- Typo or renamed page: search_pages for the intended target, then fix the link text.
- A page that ought to exist: offer to create a stub (create_page with a title, the
  right kind, and a one-line `summary:`) — only if the user wants it. Do not auto-stub.
- Stale reference to something cut from the story: offer to unlink it (keep the plain text).
Ask when you cannot tell which case applies.

**Broken ![[embeds]]**
- Look for the asset under a slightly different name (search_pages will not find media;
  check the page's neighbours). Otherwise tell the user which file to re-add and where.

**Orphan pages**
- An orphan has no backlinks. Read it, find the natural hub (its place, faction, or an
  index page) using search_pages, and propose a link line to add there.
- Some orphans are fine (scratch notes, prep, inbox captures). Ask before forcing links.

**Sync-conflict files**
- Two copies of one page exist. Read both, summarise the differences, and ask the user
  which to keep or how to merge. Write the merged result only after approval; leave
  removing the extra copy to them unless they explicitly ask you to.

**Unreadable files**
- Report them with the path. You cannot repair these; suggest opening the file outside the app.

Finish by re-running vault_diagnostics and reporting what is now clean and what remains.
