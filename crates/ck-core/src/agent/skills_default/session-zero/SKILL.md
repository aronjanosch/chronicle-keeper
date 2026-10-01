---
name: Session zero
description: Kick off a new campaign — tone, safety tools, limits, party hooks, and the first seed pages. Use when the user is starting a new campaign or world, preparing a session zero, or asks how to get a group aligned before play.
---

Help the GM prepare session zero: the conversation that aligns the table before play, and
the first pages that capture it. First check what exists (list_pages, read_page on any
Start Here or campaign overview page) so you do not duplicate it.

How to work:
- Ask in rounds of 2–3 questions, not all at once. Let the GM answer for their own table;
  many answers are "ask the players", and that is fine — turn them into questions for the
  session itself.
- You draft pages and the GM approves. Never invent facts the GM has not chosen.
- Keep everything skippable. A table that wants to start playing tonight needs three pages,
  not thirty.

Part 1 — the pitch:
- In one or two sentences, what is this campaign about? What is the central tension?
- What genre and tone: grim, heroic, cosy, comic, horror? Which books, films, or games are touchstones?
- What do the characters do, mostly: explore, scheme, survive, fight, travel?
- What is the scope — a single region, a war, a world? How long should it run?

Part 2 — safety and expectations (prepare these as prompts for the GM to ask the table):
- Lines (never appear) and veils (happen off-screen). How will people raise a concern mid-game?
- What tools will the table use: a pause signal, an X-card, a check-in at the break?
- Schedule, attendance, how absences are handled, and what happens to a character whose player is away.
- Table norms: phones, rules disputes, player knowledge versus character knowledge.

Part 3 — party hooks:
- How do the characters know each other, or what reason brings them together?
- For each player: one connection to the world (a person, place, or faction), one goal, and one complication.
- Which of these should become Codex pages right away?

Part 4 — seed the Codex. Propose a small set and create only what the GM accepts:
- A campaign overview page: pitch, tone, safety agreements, and open questions.
- A home-base place page and two or three NPCs the party will meet first.
- One faction with a clear want, and one unsolved mystery or threat (with a `[!secret]` callout for the truth).
- A page for each player character, linked to the hooks above (use page_kinds for fields).
- Use create_page with the right kind and a one-line `summary:` so the summarizer remembers them.

End with a short checklist of what to ask the players in person, and offer to prepare the
first session with the prepare-session skill.
