# Repository Guidelines

## Project Structure & Module Organization

Chronicle Keeper is a Rust workspace with a Tauri desktop shell and build-free JavaScript frontend.

- `crates/ck-core/src/`: domain logic, indexing, HTTP handlers, transcription, LLM providers, Keeper agent, and Foundry integration. Tests normally live beside their modules.
- `src-tauri/`: desktop entry point, native menus, window behavior, and Tauri configuration.
- `frontend/`: Preact/htm UI, organized into shared modules under `frontend/app/` and route-level views under `frontend/app/screens/`. Dependencies are vendored in `frontend/vendor/`; there is no Node build step.
- `site/`: public documentation; `docs/screenshots/` contains repository media.
- `docs/internal/`: a separate private Git checkout. Never add its contents to this public repository.

## Build, Test, and Development Commands

- `cargo tauri dev`: build and run the desktop application.
- `cargo run -p ck-core --bin ck-serve`: run the core API at `http://127.0.0.1:8000` without Tauri.
- `cargo test -p ck-core`: run core tests.
- `cargo check -p ck-core`: type-check the lightweight core configuration.
- `cargo fmt --all --check`: verify Rust formatting.
- `cargo clippy -p ck-core -- -D warnings`: lint Rust and reject warnings.
- `find frontend -name '*.mjs' -o -name '*.js' | while read -r f; do node --check "$f" || exit 1; done`: syntax-check all frontend modules.

## Coding Style & Naming Conventions

Use `rustfmt` and idiomatic Rust naming: `snake_case` functions/modules and `PascalCase` types. JavaScript uses two-space indentation, camelCase functions, and PascalCase components. Preserve the plain ESM/htm architecture and existing HTTP contracts. Comment sparingly: explain subtle invariants, workarounds, or non-obvious reasons rather than narrating the code.

## Architecture & Data Safety

Files are truth: world Markdown and session files are canonical, `.ck/index.db` is a rebuildable cache, and the global database holds app settings. Preserve the local-first architecture. Avoid new runtimes, sidecars, CDNs, telemetry, or unnecessary external services; discuss any such architectural change first.

## Testing Guidelines

Add focused tests near changed Rust code using descriptive names. Use `#[tokio::test]` for async paths. Run relevant tests while developing, then the full core test, formatting, clippy, and frontend syntax checks before submitting. Hardware-, network-, or Foundry-dependent smoke tests remain explicit and opt-in.

## Commit & Pull Request Guidelines

Follow the existing Conventional Commit style, such as `feat(graph): add local scope` or `fix(llm): handle rate limits`. Keep commits and pull requests focused. Explain the resulting behavior, link issues, include screenshots for UI changes, and list verification and platform limitations. Do not add AI `Co-Authored-By` trailers to commits or pull requests.
