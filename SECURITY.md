# Security Policy

## Reporting a vulnerability

Please report security issues **privately** through GitHub:
**[Security → Report a vulnerability](https://github.com/aronjanosch/chronicle-keeper/security/advisories/new)**
(GitHub private security advisories). Don't open a public issue for security problems.

I'll acknowledge within a few days and keep you posted on a fix. This is a solo, best-effort
open-source project — thanks for your patience.

## Threat model in brief

Chronicle Keeper is **local-first**. The app runs entirely on your machine:

- The core API binds to `127.0.0.1` on an ephemeral port and is gated by a per-launch token
  injected into the webview — it is never exposed to the network.
- Recordings, transcripts, notes, and your LLM API keys live in your local app-data folder and
  on-device SQLite. They are not sent anywhere unless you explicitly enable sync.
- Optional multi-device **sync** is a separate, self-hostable component
  ([chronicle-keeper-sync-server](https://github.com/aronjanosch/chronicle-keeper-sync-server)).
  Only your note text syncs — audio and API keys never leave your device.

The desktop installers are **not code-signed yet**, so your OS will warn on first launch. See
the README for how to proceed; signing is planned.

## Supported versions

Only the latest release is supported. Please update before reporting.
