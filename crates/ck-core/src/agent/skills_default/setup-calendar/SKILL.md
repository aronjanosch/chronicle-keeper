---
name: Set up the world calendar
description: Guide the user to define a custom calendar (months, eras) and date frontmatter so pages plot on the Timeline, or to use order/seq for calendar-less worlds. Use when the user asks about dates, the timeline, eras, months, or how to order events.
---

The Timeline plots any page with a `date:` in its frontmatter. Dates render through the
world's calendar, which lives in `.ck/config.toml`. **You cannot edit that file** — you
have no tool for it. Your job is to work out the right calendar with the user and tell
them exactly what to paste and where.

Step 1 — find out what they need:
- Does the world have its own months and eras, or should dates stay plain numbers?
- If they have a calendar: month names in order, era names and abbreviations, and which
  era is current? (Month lengths are not modelled; do not ask for them.)
- If they do not want a calendar: use relative order instead (see below).

Step 2 — give them the config to paste. Open the world's `.ck/config.toml` and add:

```toml
[calendar]
months = ["Frostwane", "Thawmoot", "Seedtide"]
eras = ["AR", "BR"]
```

Adapt the names to theirs. Tell them to save the file and reopen the Timeline. Mention
that `.ck/` is a hidden folder inside the world's folder.

Step 3 — date the pages (you can do this with edit_page, with approval):
- Format is `year[-month[-day]] [ERA]`, numeric month: `date: 412-02-12 AR` renders as
  "12 Thawmoot 412 AR".
- Negative years, era-only dates, and circa (`~` or `c.`) are allowed: `date: c. 890 BR`.
- A span uses `end_date:` (or `until:`): `date: 410 AR` and `end_date: 415 AR`.
- Good candidates are `event` pages; keep dated pages thin and wikilink the people and
  places involved (`participants:`, `location:`) so entities keep the lore.
- Sessions join the timeline through `world_date` in their `session.toml`; tell the user
  to set it there, since you cannot.

Calendar-less worlds: give each page an integer `order:` (or `seq:`) in frontmatter.
Pages sort by that number in a "Relative order" group. Suggest gaps (10, 20, 30) so they
can insert events later without renumbering.

GM-only events: add `gm_only: true` (or `publish: false`) to hide them behind a toggle.

Before writing dates across many pages, use query_world or search_pages to list the
candidates, show the plan, and confirm the batch with the user.
