# Chronicle Keeper
<img width="611" height="369" alt="SCR-20260601-paxt" src="https://github.com/user-attachments/assets/231952c8-b26a-455b-bb55-3325a88c372c" />

Turn your D&D session recordings into clean, structured notes — on your own machine,
offline, with whatever LLM you like.

Drop in a [Craig Bot](https://craig.chat) recording, label who's who, and Chronicle Keeper
transcribes every track on-device and writes up the session. The notes land as Markdown with
Obsidian frontmatter, ready to paste into your vault.

- **Local-first and private.** Transcription runs on your device.
- **Bring your own LLM.** Local [Ollama](https://ollama.com), Anthropic, or any
  OpenAI-compatible provider.
- **The pipeline.** Record → transcribe → summarize → export.
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
The sync protocol is documented in [`docs/SYNC_PROTOCOL.md`](docs/SYNC_PROTOCOL.md). Contributions welcome — see
[`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

[MIT](LICENSE).

Build with <3 from germany
