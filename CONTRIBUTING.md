# Contributing

Thanks for your interest! Chronicle Keeper is a small, **local-first, low-maintenance** project
maintained by one person. It aims for maximum simplicity and the lowest possible maintenance
burden — some "obvious" features are deliberately left out (e.g. no in-app editing, no build
step for the frontend). Please keep that spirit in mind, and open an issue to discuss anything
non-trivial before sending a large PR.

## Prerequisites

- [Rust](https://rustup.rs) (stable)
- [`cargo-tauri`](https://tauri.app): `cargo install tauri-cli`
- The [Tauri system prerequisites](https://tauri.app/start/prerequisites/) for your OS

No Python, Node, or GPU toolchain is needed.

## Run it locally

```bash
cargo tauri dev                          # full desktop app (Rust core + window)
cargo run -p ck-core --bin ck-serve      # core API only, no window (http://127.0.0.1:8000)
```

The frontend in `frontend/` is plain HTML/CSS/JS — **no build step**. Edit and reload.

## Before you open a PR

Please make these pass locally (they mirror CI):

```bash
cargo fmt --all                          # format Rust
cargo clippy -p ck-core                  # lint (warnings should be clean)
# Frontend has no build; syntax-check changed modules:
node --check frontend/app/<file>.js
```

## Pull requests

- Branch off `main`; keep PRs focused and small.
- Preserve the HTTP contract in `crates/ck-core/src/http/` — the frontend depends on it
  (including some legacy config key names, kept on purpose).
- Describe what changed and why. Screenshots help for UI changes.

## Reporting bugs & ideas

Open a GitHub issue. For security problems, see [`SECURITY.md`](SECURITY.md) instead.
