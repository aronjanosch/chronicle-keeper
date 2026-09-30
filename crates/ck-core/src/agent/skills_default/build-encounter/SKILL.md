---
name: Build an encounter
description: Design a system-agnostic encounter from codex pages — stakes, participants, terrain from maps, escalation, and outcomes beyond win or lose. Use when the user wants to plan a fight, chase, heist, negotiation, or other set-piece scene.
---

Help the user design one encounter. This skill is system-agnostic: it shapes the scene
and leaves numbers, difficulty, and stat blocks to the user's game system.

Gather from the world first, then ask only what is missing:
- Who is involved? read_page on the relevant NPC, faction, or creature pages to learn
  goals, tactics, and what they would do if losing.
- Where is it? If the place has a map, read_map for its pins and regions, and map_distance
  for real spacing (cover, approach routes, how long reinforcements need). Otherwise read
  the place page for features.
- What came before? search_summaries or read_recap for the threads the party is carrying.

Design checklist:

**Stakes**
- What does each side want from this scene, and what does failure cost them?
- Is it winnable by means other than violence — talk, stealth, bargaining, leaving?
- What is at risk that the players care about: a person, a place, time, a secret?

**Terrain and setting**
- Three features that matter: something to hide behind, something dangerous, something to use.
- Distances and approach routes from the map; where each side starts.
- Time pressure — a ritual, a tide, a patrol, a fire.

**Escalation**
- A beat at about one-third and two-thirds: reinforcements, a collapse, a betrayal, a new goal.
- What changes if the party is winning easily? Losing badly? Have a lever for each.

**Participants**
- For each main opponent: goal, tactic, morale threshold, and one thing they do when cornered.
- Who will surrender, flee, or switch sides, and why?

**Outcomes**
- Write at least four results: clean win, costly win, retreat, defeat — and what each changes in the world.
- Name the pages that would need updating afterwards.

Present the encounter as a compact outline. Offer to save it (create_page or insert_into_page
into the session's prep page if the user has one) — you draft, they approve. If the user
names a system, only then translate it into that system's terms.
