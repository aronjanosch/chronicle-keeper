---
name: Flesh out a faction
description: Worldbuilding questions for an organisation — goals, rivals, resources, secrets, internal splits. Use when the user wants to develop or deepen a faction, guild, order, court, or other group page.
kinds: [faction]
---

Help the user deepen a faction page. Read its body first (read_page) and check who
already points at it (get_backlinks) so you build on what exists — never re-ask what the
page answers.

How to work:
- Pick the 2–4 questions below the page is most silent on and that matter most for play.
  Do not dump the whole list.
- These are prompts, not a form. Ask, let the user answer in their own words, then offer
  to write it into the page (you draft, they approve). Always skippable.
- A faction is people pulling in different directions. Favour tension, scarcity, and
  internal disagreement over tidy org charts.
- Record structure as typed relations (`rival: "[[...]]"`, `ally:`, `led_by:`, `based_in:`)
  so the graph and Relations panel pick them up. Use page_kinds for the kind's infobox
  fields, and query_world to find existing members (`FROM kind:npc WHERE member_of = "[[X]]"`).

Questions to draw from:

**Goals**
- What do they publicly claim to want, and what do they actually want?
- What is the next concrete move they are making, and by when?
- What would count as victory in ten years? What would they sacrifice for it?

**Rivals and allies**
- Who is the main rival, and what is the contest really over?
- Who do they depend on but resent? Who depends on them?
- Is there a truce, treaty, or debt that keeps a conflict cold?

**Resources and reach**
- What do they hold: land, coin, secrets, troops, favours, a monopoly?
- What do they lack that makes them vulnerable?
- Where are they strong, and where does their influence simply stop?

**Structure and splits**
- Who leads, how did they get there, and who could replace them?
- Is there a faction inside the faction — reformers, hardliners, a cell with its own agenda?
- What do members get for belonging, and what does leaving cost?

**Secrets**
- What would shame or destroy them if it came out? Who already knows?
- A member or patron who is secretly working for someone else? (Consider a `[!secret]` callout.)

**Role in play**
- Why would the party meet them — as patron, obstacle, or employer?
- What do they offer, and what do they demand in return?
