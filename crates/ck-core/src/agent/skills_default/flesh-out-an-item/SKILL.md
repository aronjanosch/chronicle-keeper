---
name: Flesh out an item
description: Worldbuilding questions for an object — origin, maker, powers and costs, history of owners, who wants it. Use when the user wants to develop or deepen an item, artifact, relic, or notable piece of gear page.
kinds: [item]
---

Help the user deepen an item page. Read its body first (read_page) and check what links
to it (get_backlinks) so you build on what exists — never re-ask what the page answers.

How to work:
- Pick the 2–4 questions below the page is most silent on and that matter most for play.
  Do not dump the whole list.
- These are prompts, not a form. Ask, let the user answer in their own words, then offer
  to write it into the page (you draft, they approve). Always skippable.
- Keep mechanics out unless the user asks — describe what the item does in the fiction and
  leave numbers to their game system. A good item has a story and a price.
- Link the people and places in its history (`made_by: "[[...]]"`, `held_by:`,
  `located_in:`) so it shows in the graph. Use page_kinds for the kind's infobox fields.

Questions to draw from:

**Origin**
- Who made it, when, and why? What was it made to do or to stop?
- What material, technique, or sacrifice went into it? Can it be made again?

**Appearance and feel**
- What does it look, sound, and smell like? What is the first thing anyone notices?
- Does it show its age, or is it unnervingly pristine?

**Powers and costs**
- What can it do, in plain fiction terms? What are its limits?
- What does using it cost — health, memory, reputation, a debt to someone?
- Does it have a will, a hunger, or a condition that must be met?

**History of owners**
- Who held it before, and how did each of them end up?
- Where has it been lost, stolen, or hidden? Which gaps in its history are unexplained?
- Is it known by different names in different places?

**Who wants it**
- Which faction or person would kill for it, and what do they believe it is?
- Who wants it destroyed or sealed away?
- What false rumours circulate about it?

**Role in play**
- How could the party come to hold it, and what trouble follows?
- What is the secret about it that changes its meaning? (Consider a `[!secret]` callout.)
- How might it be spent, broken, or passed on over the campaign?
