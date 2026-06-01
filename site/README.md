# Chronicle Keeper — docs site

The marketing landing page + documentation site, published to **GitHub Pages**.

- **No build step.** Hand-written HTML/CSS/JS that mirrors the app's "scriptorium"
  parchment theme (`frontend/tokens.css`). Open `index.html` in a browser, or serve
  the folder: `python3 -m http.server -d site 8765`.
- **Layout**
  - `index.html` — landing page
  - `docs/` — documentation (getting-started, install, workflow, llm-setup, codex,
    sync, troubleshooting, faq). `docs/index.html` redirects to `getting-started.html`.
  - `assets/style.css` — design system · `assets/site.js` — copy buttons, mobile nav,
    auto TOC + scrollspy, active sidebar link · `assets/favicon.svg`
- **Shared chrome** (top nav, sidebar, footer) is inlined per page on purpose — keeps it
  build-free and works from `file://`. The active sidebar link is set automatically by
  `site.js` from the current filename, so adding a page just means copying the chrome.

## Deploying

`/.github/workflows/pages.yml` uploads this folder to GitHub Pages on every push to
`main` that touches `site/**`. **One-time setup in the public repo:**
Settings → Pages → Source → **GitHub Actions**.

Lives at `https://<owner>.github.io/chronicle-keeper/`.
