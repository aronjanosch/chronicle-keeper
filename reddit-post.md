I'm an IT nerd, a GM, and a lazy dungeon master in the Sly Flourish sense. I prep as little as I can get away with. The part I hate isn't the prep, it's the bookkeeping after. My table goes off the rails nearly every session, and then I'm stuck reconciling all that chaos into my notes. I never did it. My world lived in 40 half-finished Obsidian pages and my own memory.

So I built the tool I actually wanted: Chronicle Keeper.

Code: https://github.com/aronjanosch/chronicle-keeper-world
Docs + screenshots: https://aronjanosch.github.io/chronicle-keeper-world/

It's free, open source (MIT), and local-first. Your whole world is just a folder of plain Markdown files on your disk. No account, no cloud, no subscription. Open the same folder in any text editor or your existing Obsidian vault and nothing breaks.

How I use it: when I prep, I dump my unstructured ADHD-fueled ideas into the AI Keeper. It sorts it into proper pages and can for example flag inconsistencies with what I already wrote. At the table, gloves are off and we just play. Afterward I feed it the session recording and it writes and updates the notes.

What's in it:

- A real wiki. [[wikilink]] pages, backlinks, rename anything and every link fixes itself.
- Info boxes with structured fields per page (an NPC's race and age, a city's ruler), and you can invent your own page types and fields.
- Maps you pin places on.
- A timeline of dated events on a calendar you define, your own months and eras.
- A graph view to see how your world connects.
- Live lists. Drop a query in a page like "every NPC in this city who's still alive" and it stays current as the world grows.
- Session notes. Feed it a recording (via Craig bot), it transcribes locally and writes recap, key beats, NPCs, loot, open threads.
- The AI Keeper answers questions and cites the actual pages it used, and suggests edits you accept or reject. Everything's versioned, so it never quietly rewrites your world.
- Bring your own model. Fully local with Ollama, or Claude/ChatGPT if you prefer.

There's some great stuff releasing in this space lately. Where I've put my focus is that your sessions are the source of truth. What actually happened at the table flows in, and the wiki, timeline, and maps build on top of it and stay current. The Keeper is wired into all of it, so it reasons over your real pages and real session history, not a blank prompt.

On the AI thing, I'm not hiding it but I'm not pretending it writes the story. The creativity happens at the table with my friends. The AI just kills the busywork so I show up to play instead of dreading my notes. You pick the model, you approve every change, and it all stays on your machine. For my day job I build AI agent workflows in cybersecurity, where confidently-wrong answers are expensive, so the Keeper is built the same way: grounded in your files, every claim cites its source, every change yours to approve.

It started as a private Python CLI I used at my own table for over a year, then grew into the full app. Early release, solo dev, so expect rough edges.

Would love feedback from people who build worlds seriously: what would make you actually use something like this? MIT means it's yours forever, contributions welcome.
