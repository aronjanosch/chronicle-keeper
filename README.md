# Chronicle Keeper
<img width="611" height="369" alt="SCR-20260601-paxt" src="https://github.com/user-attachments/assets/231952c8-b26a-455b-bb55-3325a88c372c" />

Chronicle Keeper is an AI-powered, local-first copilot for D&D session memory.
It turns raw session audio into structured notes, campaign continuity, and reusable world knowledge.

Drop in a [Craig Bot](https://craig.chat) recording, label who's who, and Chronicle Keeper
transcribes every track on-device, then uses your chosen LLM to produce clean, campaign-aware summaries.
The results land as Markdown with Obsidian frontmatter, ready to paste into your vault.

- **AI-first workflow.** Recordings become summaries, recap context, and codex memory.
- **Local-first and private.** Core processing runs on your device.
- **Bring your own LLM.** Local [Ollama](https://ollama.com), Anthropic, and OpenAI-compatible providers.
- **Built for continuity.** Keep characters, places, factions, and items consistent across sessions.

📖 **[Read the docs →](https://aronjanosch.github.io/chronicle-keeper/)** — install guides, LLM
setup (Ollama + getting API keys), the full workflow, and FAQ.

## Features

- **On-device transcription (Parakeet native ASR).** Fast per-speaker transcript generation from Craig ZIP tracks.
- **LLM-powered session summaries.** Generate readable notes with your selected prompt template and provider.
- **Codex memory system.** Build and maintain a campaign glossary (NPCs, places, factions, items, PCs) that is injected into summary prompts to improve naming consistency.
- **Codex import AI distillation.** Import an existing notes folder and let the app extract candidate glossary entries.
- **Story-so-far recap generation.** Produce campaign continuity recaps from existing session history.
- **Prompt template library.** Use built-in prompts or create your own summary styles.
- **Markdown + Obsidian export.** Export notes in a format that drops directly into your vault.
- **Artifact management.** Keep transcript and summary artifacts per session with in-app retrieval.
- **Optional multi-device sync.** Sync text data across devices using a small self-hosted sync server.

## Screenshots
<img width="305" height="218" alt="image" src="https://github.com/user-attachments/assets/437e6911-d5da-439c-b5bc-1e99ee6bba49" />
<img width="305" height="218" alt="image" src="https://github.com/user-attachments/assets/0cfd2ced-eb97-4843-bed3-22fd6e2ce49f" />
<img width="305" height="218" alt="image" src="https://github.com/user-attachments/assets/c3bcb845-bff1-4730-96da-b9c57b1b517d" />

## Download & install

Grab the installer for your OS
[**Releases**](https://github.com/aronjanosch/chronicle-keeper/releases) page.

> **Heads up:** the app isn't code-signed yet (signing is planned), so your OS will show a
> one-time "unknown developer" warning on first launch.

**macOS** (`.dmg`)
1. Open the `.dmg` and drag **Chronicle Keeper** into Applications.
2. First launch: **right-click the app → Open → Open** (a plain double-click is blocked).
   Or allow it under **System Settings → Privacy & Security → Open Anyway**.
3. Still stuck? In Terminal: `xattr -dr com.apple.quarantine "/Applications/Chronicle Keeper.app"`

**Windows** (`.msi` or `.exe`)
1. Run the installer.
2. If SmartScreen says *"Windows protected your PC"*, click **More info → Run anyway**.

**Linux** (`.AppImage` or `.deb`)
- AppImage: `chmod +x Chronicle*.AppImage` then run it.
- Debian/Ubuntu: `sudo dpkg -i chronicle-keeper_*.deb`

On first transcription the speech model (Parakeet TDT v3) downloads once.

## Set up your LLM

Open **Settings** and pick one:

- **Local (free):** run [Ollama](https://ollama.com) — `ollama serve && ollama pull gemma4:e2b`.
- **Cloud:** paste an Anthropic or any OpenAI-compatible API key. Keys stay on your machine.

## How it works

```
Upload Craig ZIP → label speakers → transcribe on-device → summarize (your LLM) → export
```

Optional **multi-device sync** keeps your notes in step across machines via a small self-hostable
server ([chronicle-keeper-sync-server](https://github.com/aronjanosch/chronicle-keeper-sync-server)).
Only text sync. Sync is off until you add a server URL in Settings.

## Build from source

You'll need [Rust](https://rustup.rs) and
[Tauri](https://tauri.app)
```bash
cargo tauri dev                          # build + run the desktop app
cargo tauri build                        # build installers for your OS
cargo run -p ck-core --bin ck-serve      # core API only, no window (http://127.0.0.1:8000)
RUST_LOG=debug cargo run -p ck-core --bin ck-serve   # verbose logs
```
The sync wire format is defined by the code on both ends: the client in
[`crates/ck-core/src/sync.rs`](crates/ck-core/src/sync.rs) and the open-source reference server
[`chronicle-keeper-sync-server`](https://github.com/aronjanosch/chronicle-keeper-sync-server).
Contributions welcome — see [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

[MIT](LICENSE).

Build with ❤️ from germany
