---
name: Review session
description: The post-session pass in one workflow. It marks how each prep scene and reminder turned out, proposes grounded world updates (new or changed NPCs, places, factions, thread developments) for the GM to approve, and collects unused ideas on the Ideas page. Use whenever the user wants to review, wrap up or process a played session, update the Codex/world from a session, or asks "what changed after last session".
kinds: [prep]
---

Turn a played session into an up-to-date world. The **summary** is the record, the
**transcript** is the evidence for the few points the summary leaves open, and the
**prep** is only what the GM intended. Work through the checklist in order.

You have a limited number of tool rounds per message, and a real session can have
thousands of transcript turns. Spend your rounds on the pages you'll change, not on
research. Asking the GM costs less than sweeping the transcript and is more reliable.
Aim to have your proposal (step 4) in front of the GM within this first message.

## Checklist

- [ ] **1. Gather, briefly.**
  - `read_summary` for the session. If there is none, stop and tell the GM to summarize
    first.
  - `read_prep` for the same session. If it finds nothing, run one `search_pages` for
    "Session N" in case the GM keeps prep under another name. If you find no prep page,
    skip step 2. If you do, make sure its frontmatter has `kind: prep` and `session: N`
    and add whatever is missing without asking: that keeps the page out of canon and
    attached to the session.
  - Open threads: `query_world` `LIST FROM kind:thread`.
  - Then read only the pages you'll probably change: the ones the summary names. Don't
    tour the whole world, don't load other skills, and don't re-read memories you
    already have.
- [ ] **2. Mark prep outcomes.** For each scene and reminder on the prep page, decide
  from the summary how it turned out:
  - a scene (a heading): add `> outcome: happened` / `changed — <one line on how>` /
    `unused` directly under its heading. Replace an existing outcome line rather than
    adding a second one.
  - a reminder (a list item): `[x]` happened, `[~]` changed (plus an indented `> note`),
    `[-]` unused.

  The GM's prep may not follow the Opening / Scenes / Reminders layout; it might be a
  location card or a loose list. Then mark outcomes inside the structure that's already
  there, and never reorganise the GM's page.

  If the summary doesn't settle an item, leave it unmarked and add it to your questions.
  A wrong mark is worse than an open one. Leave `^id` markers untouched, and make all the
  marks in one edit (`multi_edit_page`).
- [ ] **3. Find world updates.** List what the session changed in the world: new NPCs,
  places, items or factions; status changes (died, left, destroyed, allied); new
  relationships; developments on threads. For each candidate:
  - **Ground it.** Trust the summary for what happened. Go to the transcript
    (`search_transcripts`, then a short `read_transcript` slice around the hit) only for
    a specific claim you're about to write that the summary leaves vague or that
    contradicts a page. Keep it to a handful of targeted lookups for the whole review. If
    one or two lookups don't settle a claim, it becomes a question, not an update.
  - **Attribute it.** A claim made *by a character* stays that character's claim: write
    "[[Mara Voss]] claims the harbourmaster is bribed", not "the harbourmaster is
    bribed". Player speculation and table talk are never world facts.
  - **Know who is who.** Transcript speakers labelled `Character (Player)` are the
    party. Their pages are `kind: pc`, never NPCs. A speaker without a character name is
    usually the GM narrating.
  - **Place it.** Update an existing page wherever one exists (check the digest and
    `search_pages` first, so you don't create duplicates). Thread developments go under
    the thread's `## Developments`, headed with the session number.
- [ ] **4. Propose, then apply.** Show the GM one compact numbered list: *page → change*
  (mark new pages as new), then your open questions. Ask which to apply ("all", "all
  but 3", …). Apply only the approved ones, one page at a time with the edit tools. If
  the GM answers a question, that answer is now grounded and can become an update. For a
  big session, propose the most important changes first and offer the rest afterwards.
- [ ] **5. Collect ideas.** Scenes and reminders marked `unused` or `changed`, and hooks
  that came up in play but went nowhere, can be material for later. Append the ones
  worth keeping to `Prep/Ideas` as `- [ ] <idea in one line> — from [[Session NNN]]`
  (create the page with frontmatter `kind: prep` and a `## Ideas` heading if it doesn't
  exist). Skip filler, and don't duplicate an idea that's already there. This page isn't
  canon and nothing carries over automatically; it's a pool the GM and the Prepare
  session skill can draw from.
- [ ] **6. Report.** A short checklist of what you did: outcomes marked (and how many
  left open), pages updated or created, ideas collected, and the questions still open.

## Boundaries

- Prep never becomes canon by itself. A scene marked `happened` is canon only through
  what the summary says happened, and that goes into the world pages in step 3, not
  through the prep page.
- Keep the GM in charge of the world: nothing from step 3 is written before they approve
  it. Steps 2 and 5 touch only non-canon prep pages, so do them without asking and
  report them.
- Write in the world's language. Link pages by title (`[[Session 012]]`,
  `[[Mara Voss]]`), never with a folder path.
- Don't rewrite pages wholesale. Add or adjust the specific lines the session changed.
