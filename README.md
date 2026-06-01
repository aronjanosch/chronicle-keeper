# Chronicle Keeper

Turn your D&D session recordings into clean, structured notes — on your own machine,
offline, with whatever LLM you like.

Drop in a [Craig Bot](https://craig.chat) recording, label who's who, and Chronicle Keeper
transcribes every track on-device and writes up the session. The notes land as Markdown with
Obsidian frontmatter, ready to paste into your vault. Nothing leaves your computer unless you
choose to.

- **Local-first and private.** Transcription runs on your device. Your recordings and notes
  stay with you. No per-minute cloud bills, no account required.
- **Bring your own LLM.** Local [Ollama](https://ollama.com), Anthropic, or any
  OpenAI-compatible provider. Your API keys never leave the machine.
- **A pipeline, not a workspace.** Record → transcribe → summarize → export. You live in
  Obsidian or Notion; this just generates the content and gets out of the way.

## Screenshots

_Coming soon._

<!-- Add PNGs to docs/screenshots/ (see that folder's README for names), then uncomment:
| Library | Session | Summary |
| --- | --- | --- |
| ![Library](docs/screenshots/library.png) | ![Session](docs/screenshots/session.png) | ![Summary](docs/screenshots/summary.png) |
-->


## Download & install

No build, no command line. Grab the installer for your OS from the
[**Releases**](https://github.com/aronjanosch/chronicle-keeper/releases) page.

> **Heads up:** the app isn't code-signed yet (signing is planned), so your OS will show a
> one-time "unknown developer" warning on first launch. Everything runs locally on your
> machine — here's how to get past it.

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

On first transcription the speech model (Parakeet TDT v3) downloads once and is reused after.

## First launch

The app opens with a sample campaign, **The Ashfall Compact**, already filled in — a codex of
characters and places, plus one session with a finished transcript and AI summary — so you can
see the whole pipeline before recording anything. It's marked **Example**; delete it whenever
you're ready and it won't come back.

## Set up your LLM

Open **Settings** and pick one:

- **Local (free):** run [Ollama](https://ollama.com) — `ollama serve && ollama pull llama3.2`.
- **Cloud:** paste an Anthropic or any OpenAI-compatible API key. Keys stay on your machine.

## How it works

```
Upload Craig ZIP → label speakers → transcribe on-device → summarize (your LLM) → export
```

It's a single Rust core embedded in a [Tauri](https://tauri.app) window — no Python, no
sidecar. Transcription uses [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) with NVIDIA's
Parakeet TDT v3 model (25 European languages, several× realtime on CPU). Everything persists in
one SQLite database in your app-data folder.

Optional **multi-device sync** keeps your notes in step across machines via a small self-hostable
server ([chronicle-keeper-sync-server](https://github.com/aronjanosch/chronicle-keeper-sync-server)).
Transcription always stays on your device; only the text syncs. Sync is off until you add a
server URL in Settings.

## Build from source

You'll need [Rust](https://rustup.rs) and the
[Tauri system prerequisites](https://tauri.app/start/prerequisites/) for your OS. No Python,
Node, or GPU toolchain.

```bash
cargo tauri dev                          # build + run the desktop app
cargo tauri build                        # build installers for your OS
cargo run -p ck-core --bin ck-serve      # core API only, no window (http://127.0.0.1:8000)
RUST_LOG=debug cargo run -p ck-core --bin ck-serve   # verbose logs
```

The frontend (`frontend/`) is plain HTML/CSS/JS with no build step. The sync protocol is
documented in [`docs/SYNC_PROTOCOL.md`](docs/SYNC_PROTOCOL.md). Contributions welcome — see
[`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

[MIT](LICENSE). Free to use, including commercially.
