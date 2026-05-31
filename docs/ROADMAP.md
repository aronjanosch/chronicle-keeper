# Chronicle Keeper — Roadmap

> Last updated: 2026-06-01  
> Branch: `main` (the native-Rust rewrite is merged; `native-rust-core` retired)
>
> **Status in one line:** standalone app works offline end-to-end with a redesigned
> "scriptorium" UI (Preact + htm, no build step); the codex glossary is complete (all three
> phases — manual, auto-extract, folder import) plus a story-so-far recap; multi-device sync is
> functionally complete and verified client↔server. Repo licensed (app MIT, server AGPL-3.0).
>
> **Next milestone: the first public OSS release — `v0.5`, a Reddit launch of the free
> standalone app.** Everything reorders around one question: *can a Windows / macOS / Linux
> D&D player download it, run it, and get a session summary in ~10 minutes without it feeling
> broken or sketchy?* Sync (paid tier), Stripe/VPS, and the `invoke()` refactor are all
> **explicitly post-launch** — see Sprint R.

---

## What this app is

A **pipeline, not a workspace.**

```
Craig Bot ZIP
  → extract audio tracks (kept on device for re-transcription)
  → transcribe on-device (Parakeet via sherpa-onnx, no cloud)
  → summarize on-device (Ollama) or via BYO cloud key
  → store everything in SQLite
  → export on demand → Obsidian / Notion / markdown / any format
```

Users live in Obsidian or Notion. Chronicle Keeper generates the content and gets out of the way. No in-app editing, no file management, no cloud lock-in.

---

## Core principles

1. **Everything in SQLite.** Transcripts, summaries, metadata — all stored as TEXT in the DB. No loose files, no file-path references, no session folders. Single file to backup. Sync becomes a DB-record diff.
2. **Pipeline, not workspace.** Create → process → store → export. The app is a factory; the user's note-taking tool is the destination.
3. **Local and private by default.** Transcription runs on the user's device. LLM keys never leave the device. No telemetry.
4. **Maximum simplicity.** Solo developer. Every feature must earn its maintenance cost.
5. **Audio is kept on device.** Craig tracks are extracted and retained locally so the user can re-transcribe with a different engine as more models land (e.g. Whisper Turbo). Audio stays device-local — never synced (too large), never uploaded. Only transcript/summary text goes to SQLite and sync.

---

## Licensing & business model

| Component | License | Notes |
|---|---|---|
| App (`chronicle-keeper`) | **MIT** | Open source, free forever. Commercial use by professional DMs allowed. |
| Sync server (`chronicle-keeper-sync-server`) | **AGPL v3** | Open source. Self-hosting OK. Commercial resale of the hosted service blocked. |
| Official hosted sync | **Proprietary service** | $2–3/mo Stripe subscription. Finances ops + development. |

**Monetization:** sync only. The app is free with no ads, no DRM, no tracking. The subscription covers a €5–10/mo VPS. Profitable at ~5 paying users.

**Self-hosters:** explicitly supported. The sync protocol is documented at `docs/SYNC_PROTOCOL.md`. Anyone can build a compatible server. Self-hosters were never paying customers — they help adoption.

---

## Architecture

### Current (Sprint 1, shipped)

```
Tauri shell
  └── axum HTTP server (127.0.0.1:<ephemeral>, in-process tokio task)
        ├── campaigns / sessions / artifacts (SQLite via rusqlite)
        ├── transcription (sherpa-onnx, Parakeet TDT v3 int8)
        ├── LLM (Ollama + OpenAI-compat + Anthropic native)
        └── export (Obsidian/markdown)
  └── webview → vanilla JS frontend → fetch() to axum
```

The internal HTTP server is **technical debt.** It was justified by "shared contract with the sync server" — that reason is now gone (sync is `POST /sync`, a completely separate contract). The local HTTP server exposes a port on the machine, requires token injection into the webview, and adds unnecessary complexity.

### Target (Sprint 3)

```
Tauri shell
  └── Tauri commands (invoke() IPC, no port, no token, sandboxed)
        ├── campaigns / sessions / artifacts (SQLite)
        ├── transcription (sherpa-onnx)
        ├── LLM clients
        └── export
  └── webview → JS frontend → invoke() calls
```

Remove axum entirely from the Tauri app. Keep `ck-serve` as a dev/debug binary (axum stays in `ck-core` as a library, used only by `ck-serve`). The frontend migrates from `fetch()` to `invoke()` — significant but well-defined refactor.

### Sync (Sprint 2)

```
App (local SQLite, per-record `dirty` flag set on every write)
  │
  │  POST /sync  on startup + every 5 min  (unsynced writes persist as dirty,
  ▼               so anything missed flushes on next launch)
chronicle-keeper-sync-server (VPS, AGPL v3)
  └── SQLite (WAL mode), monotonic server_seq, server-authoritative merge
```

One endpoint. Offline-first. Conflict cursor is the server's `server_seq` (clock-skew
immune). See `docs/SYNC_PROTOCOL.md` for the full spec.

---

## Sprint status

### ✅ Sprint 0 — Transcription spike
Proved sherpa-onnx + Parakeet TDT v3 int8 builds and runs on Linux and macOS. ~16× realtime CPU. Chunking required (int8 encoder ~50s max sequence). **Open:** Windows unverified.

### ✅ Sprint 1 — Standalone desktop
Full offline flow: upload Craig ZIP → label speakers → transcribe → summarize → export. SQLite storage. Tauri shell + in-process axum. Silero VAD chunking. Model download once to app-data. macOS DMGs produced (arm64 + x64). **Open:** Windows installer (needs Windows host/CI); code signing/notarization.

### ✅ Sprint 1.5 — Frontend sync-up
Fixed settings screen 400 on save. Removed dead ONNX/MLX/WhisperX UI. README + CLAUDE.md updated for native Rust core.

### ✅ Sprint 2 — Multi-device sync (core complete)

**Goal (met):** a second device sees synced data; the server rejects unauthenticated requests.
Verified end-to-end — the Rust client and the live Python server round-trip campaigns,
sessions, artifacts, and deletions over HTTP.

**Conflict model (decided 2026-05-27):** server-authoritative. The server stamps every
accepted record with a monotonic `server_seq`; **last push received wins**. Client `updated_at`
is informational, never used for conflicts — immune to client clock skew. Auth is a single
shared `CK_SYNC_TOKEN` for v1 (one token = one data scope); per-user Stripe-scoped tokens are a
later upgrade. See `docs/SYNC_PROTOCOL.md`.

**Sync server** (`chronicle-keeper-sync-server`, AGPL v3):
- [x] Rebuilt around `POST /sync` — replaced the CRUD endpoints (+ `GET /health`)
- [x] Schema: monotonic `server_seq` on campaigns/sessions/artifacts; `artifact_id` (client UUID) PK on artifacts; `deleted_artifacts` tombstones; `updated_at`/`deleted` columns
- [x] Merge logic: last push received wins (overwrite + bump `server_seq`); artifacts push-once (`INSERT OR IGNORE`); deletions tombstoned; null JSON coerced. 6 tests

**Rust core** (`crates/ck-core`):
- [x] Migrate local artifacts from `file_path` to inline `content` in SQLite (core principle #1)
- [x] Schema groundwork: `updated_at` + `deleted` + `dirty` on campaigns/sessions; `artifact_id` (UUID, unique) + `content` + `dirty` on artifacts; idempotent migration + backfill (`db.rs`)
- [x] `sync` module (`sync.rs`): `dirty` flag on every write, `last_sync_at` cursor + `ck_client_id` in config, wire DTOs, `collect_dirty`/`apply_pull`/`clear_dirty`, `SyncClient` + `sync_once`; round-trip unit-tested
- [x] Push-side deletions: `deleted_artifacts` tombstones for artifacts; sessions soft-deleted (`deleted=1`) + UI list filtering
- [x] Background `tokio::time::interval` sync task in the Tauri shell (startup flush + every 5 min)
- [x] Shutdown durability: dirty flags persist in SQLite → unsynced writes flush next launch

**Frontend:**
- [x] Sync settings UI (server URL + write-only token field) + status indicator (off / token-missing / on)

**Repo hygiene & licensing:**
- [x] MIT `LICENSE` (app) / AGPL-3.0 `LICENSE` (server); README rewritten brief + human
- [x] Removed Python/sidecar/Vite-era junk (`spike/`, `dev.sh`, `scripts/`, `tasks/`, `REWRITE_PLAN.md`); untracked `.claude/`

**Still open (verification, not blocking the design):**
- [ ] Click-test sync in the *running app* (only unit- + wire-tested so far — needs the webview + two app instances)

### ✅ Sprint 2.1 — UI redesign ("the scriptorium")

Replaced the generic dark-dashboard frontend with the warm-parchment redesign from the
Claude Design handoff (`docs/` design bundle). **Stack decision:** Preact + htm, vendored as a
single self-contained ESM bundle (`frontend/vendor/htm-preact-standalone.mjs`, ~13 KB) — keeps
the **no-build-step / offline** principle while giving a real component model (vanilla was too
verbose for 8 screens of reused cards; Svelte would have added a Node build pipeline).

- [x] Design system (`tokens.css`) + global layout/prose (`app.css`); atoms, shell, modals
- [x] Six screens wired to the existing HTTP contract: Library, Campaign overview, Session
      detail (pipeline strip), New Session (upload→label→details, replaces the wizard),
      Summarize (full page, replaces the modal), Settings (grouped cards)
- [x] API layer ported 1:1 from the legacy `app.js` (same endpoints/payloads); legacy removed
- [x] Boot-race fix: data loads moved into a mount effect so the store listener is registered
      before the near-instant local-server responses resolve
- **Deferred (placeholder screens):** Codex + Sources — need a backend file indexer and a
  product decision. Per the design handoff, the codex is a *read-only inspector of what the
  summarizer remembers*, not an in-app wiki. Teased, not built.

### 🔲 Sprint R — Public launch (`v0.5`)  ← **NEXT**

**Goal:** the first public OSS release. A Windows / macOS / Linux D&D player downloads it, runs
it, and gets a session summary in ~10 min — without it feeling broken or unsigned. Free
standalone; sync stays behind for the paid follow-up (Sprint P).

**Decisions (2026-05-27):** ship **all three platforms**; **sign + notarize** (signing infra
already in hand from another release); **full onboarding** (sample data + readiness detection).

**Distribution (Win + Mac + Linux):**
- [ ] **Windows build spike** — de-risk sherpa-onnx + WebView2 + MSVC (the open Sprint 0 unknown);
      confirm transcription actually runs on Windows *before* promising it
- [ ] GH Actions matrix (macOS arm64 + x64, Windows, Ubuntu) → build + release on tag `v*`
      (tauri-action); first build fetches the sherpa-onnx prebuilt lib per-OS
- [ ] macOS sign + notarize (Developer ID; secrets in CI); Windows code-sign; Linux AppImage
      (WebKitGTK x11/DMABUF fix already baked in)
- [ ] Make the app repo **public** (MIT)

**Onboarding / first-run (full):**
- [ ] **Sample chronicle** — a bundled read-only demo session (transcript + a pre-made summary)
      so a fresh user sees real output with *zero* setup, no LLM, no recording. Doubles as
      screenshot/GIF material. Surfaced as "View a sample" on the empty Library.
- [ ] **Readiness detection** — real Ollama health check (ping `/api/tags`); make the sidebar
      "Ollama ready" status truthful; the Summarize screen guides when no provider is ready
      ("Ollama not running → install" / "add a key in Settings")
- [ ] **First-run model fetch** — a friendly "downloading transcription engine (~465 MB, once)"
      panel reusing `/model-status` (today it's only a progress toast on first transcribe)
- [ ] "How to record with Craig Bot" — in-app help link + README section

**Robustness / polish:**
- [ ] Session delete in the UI (backend `DELETE /sessions/:id` exists; no UI yet) + confirm
- [ ] Error-state audit — Ollama down, malformed ZIP, missing key, transcription failure all
      surface a clear, non-technical message
- [ ] Cross-platform path handling for the data folder + export (Windows separators, spaces)

**Summary context / Codex (decision 2026-05-27 → ship "A: glossary"):**

The real summary-quality lever for launch. The prompt builder (`prompts.rs::build_session_context`)
*already* feeds the LLM: campaign name/system/**setting**/GM/**extra_info**, the full speaker
roster (player → character + pronouns), and a per-summary free-text `context` field. So PC names
are handled. **The gap is non-PC named entities** — NPCs, places, factions, items, lore — which
the ASR mangles (hears "never winter") and the LLM can't recognize without a glossary.

Key insight: **this is a prompt-context problem, not a transcription one.** Told the names, the
LLM auto-corrects "never winter" → "Neverwinter" in the summary — no ASR hotword biasing needed.

Reframed (2026-05-27) as **one context channel filled three ways** — a per-campaign glossary,
injected into every summary, lightly editable. *Not* a structured entity store the GM must
maintain in our schema (that's the workspace trap). The LLM is the parser; names+lore pass
verbatim. Three phases, cheap-first:

- [x] **Phase 1 — Manual glossary (shipped, launch):** `codex` TEXT column on `campaigns`; a
      textarea in campaign-edit ("Known names & lore — NPCs, places, factions; spell them
      right"); injected as a labelled block in `build_session_context`, applied to every
      summary; read-only **Codex screen** renders it; syncs via the `Campaign` wire DTO.
      Sync server mirrors the column (schema + idempotent ALTER migration + `/sync` merge).
      Verified end-to-end: HTTP round-trip, prompt-injection unit tests, and a sync round-trip
      test (client → server → second device).
- [x] **Phase 2 — structured entries + auto-extract + edit (shipped):** `codex_entries` table
      (name + kind + body + `source` manual/auto), case-insensitive dedup per campaign;
      auto-populate from the `{characters, locations, items}` lists `summarize.rs` already
      extracts (never overwrites a user-edited row); editable Codex screen grouped by kind,
      inline edit/delete, `source` badge; sidebar count = entry count; sync wiring (client +
      server, soft-delete propagates). Closes the self-improving loop.
- [x] **Phase 3 — folder/export import (shipped):** point at a directory (Obsidian / Notion /
      LegendKeeper export); an LLM *distills* it to glossary entries (`codex_import.rs`), with a
      user-chosen provider/model and a preview → commit flow (`POST .../codex/import` returns a
      preview, `.../codex/import/commit` persists it onto the Phase 2 store). Manual snapshot,
      desktop-only fs guard. Supersedes the old "C — Sources indexer" with an LLM-distill take.

**Story-so-far recap (shipped):** `POST /campaigns/:id/recap` synthesizes a running narrative
recap from prior session summaries (prompt in `prompts.rs`); surfaced in the campaign overview UI.
A read-aloud "previously on…" for the GM, regenerated on demand — derived, not hand-maintained.

**Docs / marketing:**
- [ ] README: screenshots of the new scriptorium UI + a short demo GIF of the pipeline
- [ ] Privacy statement (on-device transcription, BYO key, no telemetry, open source)
- [ ] Reddit launch post draft + target subs (r/DMAcademy, r/rpg, r/DnD, r/selfhosted, r/macapps)

### 🔲 Sprint P — Hosted sync (paid tier)  · post-launch

The subscription that finances the VPS. **Deliberately after the free launch** — sync is the
adoption hook, not a launch blocker, and per-user auth must land before any paid signups.
- [ ] Stripe subscription + webhook for subscription validation on the server
- [ ] Per-user auth: replace the shared `CK_SYNC_TOKEN` with per-subscriber tokens scoped to a
      Stripe customer id (server data partitioned per user)
- [ ] VPS provision + Caddy (TLS) + deploy the Docker image; back up `/data`
- [ ] Click-test sync in the running app with two instances (carried over from Sprint 2)

### 🔲 Sprint 3 — Drop internal HTTP server  · post-launch (internal)

**Goal:** remove axum from the Tauri app entirely. Replace with Tauri `invoke()` commands.

Why now and not earlier: the original justification (shared HTTP contract with sync server) is gone. Tauri commands are more secure (no local port), faster (no socket), and the correct architecture for a Tauri app.

**Scope:**
- [ ] Define Tauri command surface (mirrors current HTTP routes)
- [ ] Port `http/*.rs` handlers to `#[tauri::command]` functions
- [ ] Frontend: replace all `fetch()` calls with `invoke()` — systematic, file by file
- [ ] Remove axum from `src-tauri` dependencies; keep in `ck-core` for `ck-serve` only
- [ ] Remove `window.__CK_API_BASE__` + `window.__CK_TOKEN__` injection
- [ ] Remove CORS middleware from Tauri build path

`ck-serve` (dev binary) keeps axum — useful for testing the core without a window.

### ✅ Codex Phase 3 — folder / export import (shipped)

Phases 1 (manual glossary), 2 (structured entries + auto-extract), and 3 (folder import) are all
done. Phase 3 extends the **same** model: one editable glossary store + one injection point
(`prompts.rs::build_session_context`), filled by more sources. **Guiding principle holds:** the
codex is *the summarizer's memory* — derived, lossy, lightly editable — **not** a wiki the GM
lives in. No maps, no AI prep-chat, no full CRUD workspace.

What shipped (LLM distillation of a notes folder):

- [x] **`http/codex.rs` + `codex_import.rs`** — `POST /campaigns/:id/codex/import` (mirrors
      `SummarizeRequest`: provider/model/base_url; resolves key like `summarize.rs`) walks the
      directory with `std::fs`, collects `.md`/`.markdown`/`.txt`, and batches them to the LLM
      with a **distill** prompt → `[{name, kind, body}]`. Returns a **preview**;
      `.../codex/import/commit` persists the chosen entries as `source='import'` (dedup as Phase 2).
- [x] **Desktop-only guard** — reads the local filesystem, so it is gated off in server mode; the
      sync VPS never exposes arbitrary fs.
- [x] **Frontend** — import panel on the Codex screen with directory input + provider/model select
      chosen before running, a privacy line, and a preview the GM edits before committing.
- **Snapshot semantics** — one-time, manual; re-run to refresh. No folder-watching.

### 🔲 Later

- Verify the hardware-accel path actually engages (CoreML on macOS, CUDA/DirectML on Windows) — plumbed with CPU fallback, but never confirmed to use the accelerator
- More transcription engines (e.g. Whisper Turbo) — audio is kept on device specifically so users can re-transcribe with a different model
- Postgres on sync server (when SQLite write contention becomes real; storage is behind an interface)
- Cohere LLM provider (deferred from the LLM port)
- Export targets beyond Obsidian (Notion API, Logseq)
- Optional shutdown-flush hook in the Tauri shell (today's startup + interval is sufficient; dirty flags persist)

---

## Open risks

| Risk | Status |
|---|---|
| **Windows build unverified** | 🔴 **Launch-critical** — the #1 Sprint R item; spike before promising Windows |
| First-run onboarding cliffs (model download, no Ollama/key) | 🟠 Open — addressed by Sprint R (sample data + readiness detection + model-fetch panel) |
| CoreML EP for Parakeet int8 unverified | ⚠️ Open — plumbed with CPU fallback, not tested |
| Sync not click-tested in the running app | ⚠️ Open — unit- + wire-tested only; needs webview + two instances (Sprint P) |
| Hosted sync is single-tenant (shared token) | ⚠️ Expected for v1 — per-user auth is Sprint P, before any paid signups |
| Code signing / notarization | 🟢 Resolved — signing infra in hand; mac notarize + Windows sign in CI (Sprint R) |
| Artifacts stored as file_path locally | ✅ Fixed — inline `content` in SQLite |
| Audio tracks never deleted | ✅ Intentional — kept for re-transcription (principle #5) |
| Internal HTTP port exposure | ⚠️ Technical debt — token-gated, acceptable for launch; removed in Sprint 3 (post-launch) |
| Rust learning curve (solo dev) | 🟡 Manageable — keep surface small, lean on examples |
