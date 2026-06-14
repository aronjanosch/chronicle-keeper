// Phase 14 D+E: the command registry. Global hotkeys, the native menu bar
// (Tauri `ck-menu` events), and the palette all funnel through runCommand(id).
// Screen-local commands (editor format, rail/zen toggles, save, find) broadcast
// a `ck:cmd` window event that the mounted screen or editor listens for.
import { store, navigate, navigateBack, navigateForward, openModal, closeModal,
  activePagePath, closeTab, reopenClosedTab, cycleTab, jumpToTab } from './core.js';
import { createVaultFolder } from './actions.js';

function worldId() { return store.campaign?.campaign_id || null; }

function togglePalette() {
  if (store.modal?.kind === 'commandPalette') closeModal();
  else if (!store.modal) openModal('commandPalette');
}

export function promptNewPage() {
  if (!worldId()) return;
  openModal('newPage', { folder: '' });
}
export function promptNewFolder() {
  if (!worldId()) return;
  openModal('textPrompt', {
    title: 'New folder', label: 'Folder name', confirmLabel: 'Create',
    onSubmit: (name) => createVaultFolder(name),
  });
}

const broadcast = (id) => () => window.dispatchEvent(new CustomEvent('ck:cmd', { detail: id }));
const goWorld = (name) => () => { const id = worldId(); if (id) { if (store.modal) closeModal(); navigate(name, { id }); } };

const COMMANDS = {
  'palette': togglePalette,
  'quick-open': togglePalette,
  'new-page': promptNewPage,
  'new-folder': promptNewFolder,
  'quick-capture': () => { if (worldId() && !store.modal) openModal('quickCapture'); },
  'import': () => { if (worldId()) openModal('codexImport'); },
  'export-world': () => { if (worldId()) openModal('exportWorld'); },
  'search-world': () => { const id = worldId(); if (id) { if (store.modal) closeModal(); navigate('search', { id }); } },
  'settings': () => { if (store.modal) closeModal(); navigate('settings'); },
  'shortcuts': () => { if (store.modal?.kind === 'shortcuts') closeModal(); else openModal('shortcuts'); },
  'nav-back': navigateBack,
  'nav-forward': navigateForward,
  'go-overview': goWorld('campaign'),
  'go-codex': goWorld('codex'),
  'go-atlas': goWorld('atlas'),
  'go-timeline': goWorld('timeline'),
  'go-graph': goWorld('graph'),
  'go-sessions': goWorld('sessions'),
  'go-keeper': goWorld('keeper'),
  'go-library': () => navigate('library'),
  'toggle-rail': broadcast('toggle-rail'),
  'zen': broadcast('zen'),
  'save': broadcast('save'),
  'find': broadcast('find'),
  // 15D: tabs. ⌘W only closes when a page tab is active — never the window.
  'tab-close': () => { const p = activePagePath(); if (p) closeTab(p); },
  'tab-reopen': reopenClosedTab,
  'tab-next': () => cycleTab(1),
  'tab-prev': () => cycleTab(-1),
};
for (const k of ['bold', 'italic', 'code', 'highlight', 'wikilink', 'h1', 'h2', 'h3', 'list', 'quote', 'callout']) {
  COMMANDS[`fmt-${k}`] = broadcast(`fmt-${k}`);
}
for (let n = 1; n <= 9; n++) COMMANDS[`tab-${n}`] = () => jumpToTab(n);

// The same key can reach us twice on some platforms (native menu accelerator
// + webview keydown) — drop the echo.
let lastRun = { id: null, t: 0 };
export function runCommand(id) {
  const cmd = COMMANDS[id];
  if (!cmd) return;
  const now = Date.now();
  if (id === lastRun.id && now - lastRun.t < 150) return;
  lastRun = { id, t: now };
  cmd();
}

// Native menu → frontend (Tauri shell only; standalone browser dev no-ops).
export function initMenuBridge() {
  window.__TAURI__?.event?.listen('ck-menu', (e) => runCommand(String(e.payload)));
}

// Tell the shell whether the Format menu items apply (an editor is mounted).
export function setEditorActive(active) {
  const invoke = window.__TAURI__?.core?.invoke;
  if (invoke) invoke('set_format_enabled', { enabled: !!active }).catch(() => {});
}
