// Core: global store + HTTP client. The store is a plain object with a tiny
// pub/sub; `useStore()` re-renders any component that reads it. Actions live in
// actions.js and mutate via setState().
import { useState, useEffect } from '../vendor/htm-preact-standalone.mjs';

// ── Global state ──────────────────────────────────────────────────
export const store = {
  apiBase: 'http://127.0.0.1:8000',
  apiToken: null,
  shellMode: false,       // true when the Tauri shell injected the API base (browser-dev → false)

  // routing: { name: 'library'|'campaign'|'sessions'|'session'|'newSession'|'summarize'|'settings'|'codex'|'page', params }
  route: { name: 'library', params: {} },

  // data
  config: null,
  campaigns: [],          // [{campaign_id, name, ...detail}]
  campaign: null,         // current campaign detail
  campaignSessions: [],   // sessions of current campaign
  vaultPages: [],
  vaultFolders: [],
  atlasMaps: [],          // [{id, name, kind, seed, parent, page, pins[]}]
  atlasMapId: null,       // map currently shown on the Atlas stage (sidebar selection)
  currentPage: null,
  session: null,          // current session detail (campaign{}, tracks[], speakers[], metadata{})
  transcripts: [],
  summaries: [],
  summaryPreview: null,   // { id, text } latest summary content for session screen
  summaryStreaming: null, // { stage:'reading'|'writing'|'metadata', text } live summarize run (null = idle)
  codexUpdate: null,      // Phase 5 proposal run for current session ({status:'none'} = never generated)
  codexUpdateStreaming: null, // { stage:'candidates'|'grounding' } generation in flight
  providers: null,        // transcription engines
  llmProviders: null,     // LLM provider registry
  providerStatus: null,   // { ok, reason } for the active summary provider (null = unknown)
  promptTemplates: null,  // user-managed summary prompt templates [{id, label, text, builtin}]

  // migration
  migrationStatus: null,  // { needs_migration, campaigns } — null = not checked yet
  migrationRunning: false,
  migrationResult: null,  // { ok, campaigns_migrated, sessions_migrated, errors } after run

  // tabs (Phase 15): open codex pages, per world. One tab per path; the
  // active tab is whichever path the 'page' route currently shows.
  tabs: [],               // [path]

  // transient UI
  op: null,               // { msg, state: ''|'done'|'err' } global op banner
  modal: null,            // { kind, props } overlay
  loading: false,
  error: null,
  canNavBack: false,
  canNavFwd: false,
};

const listeners = new Set();
export function setState(patch) {
  Object.assign(store, patch);
  listeners.forEach((l) => l());
}
export function useStore() {
  const [, force] = useState(0);
  useEffect(() => {
    const l = () => force((n) => n + 1);
    listeners.add(l);
    return () => listeners.delete(l);
  }, []);
  return store;
}

// Freshness signals: bump('keeper'|'vault') after a mutation another surface
// may be showing; screens refetch via a `store.dirty_*` effect dependency.
export function bump(domain) {
  const key = `dirty_${domain}`;
  setState({ [key]: (store[key] || 0) + 1 });
}

// ── Navigation ────────────────────────────────────────────────────
const navStack = { back: [], fwd: [] };
const NAV_CAP = 50;

export function navigate(name, params = {}) {
  const cur = store.route;
  const same = cur.name === name && JSON.stringify(cur.params) === JSON.stringify(params);
  if (!same) {
    navStack.back.push({ name: cur.name, params: cur.params });
    if (navStack.back.length > NAV_CAP) navStack.back.shift();
    navStack.fwd = [];
  }
  if (name === 'page' && params.path) { recordRecentPage(params.path); syncTabsForPage(params.path); }
  setState({ route: { name, params }, canNavBack: navStack.back.length > 0, canNavFwd: false });
}

export function navigateBack() {
  const entry = navStack.back.pop();
  if (!entry) return;
  navStack.fwd.push({ name: store.route.name, params: store.route.params });
  if (entry.name === 'page' && entry.params.path) { recordRecentPage(entry.params.path); syncTabsForPage(entry.params.path); }
  setState({ route: entry, canNavBack: navStack.back.length > 0, canNavFwd: navStack.fwd.length > 0 });
}

export function navigateForward() {
  const entry = navStack.fwd.pop();
  if (!entry) return;
  navStack.back.push({ name: store.route.name, params: store.route.params });
  if (entry.name === 'page' && entry.params.path) { recordRecentPage(entry.params.path); syncTabsForPage(entry.params.path); }
  setState({ route: entry, canNavBack: navStack.back.length > 0, canNavFwd: navStack.fwd.length > 0 });
}

// ── Tabs (Phase 15): per-world page tabs ──────────────────────────
// Plain navigation re-points the active tab (browser model); ⌘-click and the
// context menus append background tabs via openInNewTab. Persisted per world.
const CLOSED_CAP = 20;
let closedTabs = [];  // reopen stack (paths), reset on world switch

function tabsKey(id) { return `ck_tabs_${id}`; }

export function loadWorldTabs(campaignId) {
  closedTabs = [];
  let tabs = [];
  try { tabs = JSON.parse(localStorage.getItem(tabsKey(campaignId)) || '{}').paths || []; }
  catch (_) { /* corrupt blob — start fresh */ }
  setState({ tabs });
}

function setTabs(tabs) {
  const id = store.campaign?.campaign_id;
  if (id) { try { localStorage.setItem(tabsKey(id), JSON.stringify({ paths: tabs })); } catch (_) { /* private mode */ } }
  setState({ tabs });
}

export function activePagePath() {
  return store.route.name === 'page' ? store.route.params.path : null;
}

// Called before the route flips: re-point the tab we're leaving, or append.
function syncTabsForPage(path) {
  const tabs = store.tabs || [];
  if (tabs.includes(path)) return;
  const cur = activePagePath();
  const i = cur ? tabs.indexOf(cur) : -1;
  setTabs(i >= 0 ? tabs.map((p, j) => (j === i ? path : p)) : [...tabs, path]);
}

export function openInNewTab(path, { background = false } = {}) {
  const tabs = store.tabs || [];
  if (!tabs.includes(path)) setTabs([...tabs, path]);
  if (!background) navigate('page', { path });
}

export function closeTab(path) {
  const tabs = store.tabs || [];
  const i = tabs.indexOf(path);
  if (i < 0) return;
  closedTabs = [...closedTabs.filter((p) => p !== path), path].slice(-CLOSED_CAP);
  const next = tabs.filter((p) => p !== path);
  setTabs(next);
  if (activePagePath() === path) {
    if (next.length) navigate('page', { path: next[Math.min(i, next.length - 1)] });
    else navigate('codex', { id: store.campaign?.campaign_id });
  }
}

export function closeOtherTabs(path) {
  const tabs = store.tabs || [];
  if (!tabs.includes(path)) return;
  closedTabs = [...closedTabs, ...tabs.filter((p) => p !== path)].slice(-CLOSED_CAP);
  setTabs([path]);
  if (activePagePath() && activePagePath() !== path) navigate('page', { path });
}

export function reopenClosedTab() {
  const path = closedTabs.pop();
  if (path) openInNewTab(path);
}

export function cycleTab(dir) {
  const tabs = store.tabs || [];
  if (!tabs.length) return;
  const i = tabs.indexOf(activePagePath());
  const next = i < 0 ? (dir > 0 ? 0 : tabs.length - 1) : (i + dir + tabs.length) % tabs.length;
  navigate('page', { path: tabs[next] });
}

// ⌘1–⌘8 jump by position; ⌘9 is always the last tab (browser convention).
export function jumpToTab(n) {
  const tabs = store.tabs || [];
  const path = n === 9 ? tabs[tabs.length - 1] : tabs[n - 1];
  if (path) navigate('page', { path });
}

export function moveTab(from, to) {
  const tabs = [...(store.tabs || [])];
  if (from === to || from < 0 || to < 0 || from >= tabs.length || to >= tabs.length) return;
  tabs.splice(to, 0, tabs.splice(from, 1)[0]);
  setTabs(tabs);
}

// Rename/move cascade: re-point tabs (and the open route) at moved paths.
// `from`/`to` are either a page path or a folder prefix.
export function remapTabs(from, to) {
  const remap = (p) => (p === from ? to : p.startsWith(`${from}/`) ? to + p.slice(from.length) : p);
  const tabs = (store.tabs || []).map(remap);
  if (JSON.stringify(tabs) !== JSON.stringify(store.tabs)) setTabs([...new Set(tabs)]);
  const cur = activePagePath();
  if (cur && remap(cur) !== cur) {
    setState({ route: { name: 'page', params: { ...store.route.params, path: remap(cur) } } });
  }
}

// Drop tabs whose page no longer exists (delete, trash, bulk, external edits).
// Called after every vault tree load; never navigates — the page screen shows
// its own "not found" state if the open page itself vanished.
export function pruneTabs(pages) {
  const alive = new Set((pages || []).map((p) => p.path));
  const tabs = store.tabs || [];
  const next = tabs.filter((p) => alive.has(p));
  if (next.length !== tabs.length) setTabs(next);
}

// ── Recent pages (MRU, per-world, localStorage) ───────────────────
const RECENT_CAP = 12;
function recentKey(id) { return `ck_recent_pages_${id}`; }
export function recentPages(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return [];
  try { return JSON.parse(localStorage.getItem(recentKey(id)) || '[]'); }
  catch (_) { return []; }
}
function recordRecentPage(path) {
  const id = store.campaign?.campaign_id;
  if (!id) return;
  try {
    const next = [path, ...recentPages(id).filter((p) => p !== path)].slice(0, RECENT_CAP);
    localStorage.setItem(recentKey(id), JSON.stringify(next));
  } catch (_) { /* private mode */ }
}

// ── Op banner (transcribe/summarize/export progress + result) ─────
let opTimer = null;
export function setOp(msg, state = '') {
  if (opTimer) { clearTimeout(opTimer); opTimer = null; }
  if (!msg) { setState({ op: null }); return; }
  setState({ op: { msg, state } });
  if (state === 'done' || state === 'err') {
    opTimer = setTimeout(() => setState({ op: null }), 4500);
  }
}

// ── Modal ─────────────────────────────────────────────────────────
export function openModal(kind, props = {}) { setState({ modal: { kind, props } }); }
export function closeModal() { setState({ modal: null }); }

// ── HTTP client ───────────────────────────────────────────────────
export function apiUrl(path) { return `${store.apiBase}${path}`; }
function authHeaders() { return store.apiToken ? { 'X-CK-Token': store.apiToken } : {}; }

export async function apiFetch(path, options = {}) {
  const opts = { ...options, headers: { ...(options.headers || {}), ...authHeaders() } };
  const res = await fetch(apiUrl(path), opts);
  if (!res.ok) {
    let detail = res.statusText;
    try { const data = await res.json(); detail = data.detail || JSON.stringify(data); } catch (_) {}
    throw new Error(detail);
  }
  return res.json();
}

// POST/PUT JSON convenience
export function apiJson(path, method, body) {
  return apiFetch(path, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

// POST + consume a Server-Sent Events stream. `onEvent` is called with each
// parsed `data:` payload. EventSource can't carry a POST body or the auth
// header, so we read the response body ourselves and split on SSE frame
// boundaries (\n\n). Resolves when the stream ends.
export async function apiStream(path, body, onEvent) {
  const res = await fetch(apiUrl(path), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify(body),
  });
  if (!res.ok || !res.body) {
    let detail = res.statusText;
    try { const d = await res.json(); detail = d.detail || JSON.stringify(d); } catch (_) {}
    throw new Error(detail);
  }
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let buf = '';
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += dec.decode(value, { stream: true });
    let idx;
    while ((idx = buf.indexOf('\n\n')) !== -1) {
      const frame = buf.slice(0, idx);
      buf = buf.slice(idx + 2);
      const dataLine = frame.split('\n').find((l) => l.startsWith('data:'));
      if (!dataLine) continue;
      const payload = dataLine.slice(5).trim();
      if (!payload) continue;
      try { onEvent(JSON.parse(payload)); } catch (_) {}
    }
  }
}

// raw bytes (map art) — returns a Blob
export async function apiBlob(path) {
  const res = await fetch(apiUrl(path), { headers: authHeaders() });
  if (!res.ok) throw new Error('Failed to load image');
  return res.blob();
}

// raw text (transcript / summary content + export)
export async function apiText(path) {
  const res = await fetch(apiUrl(path), { headers: authHeaders() });
  if (!res.ok) throw new Error('Failed to load content');
  return res.text();
}

// ── Boot: resolve API base + token (shell injects these) ──────────
export function loadApiBase() {
  if (window.__CK_API_BASE__) {
    store.apiBase = window.__CK_API_BASE__;
    store.shellMode = true;
  } else {
    const saved = localStorage.getItem('ck_api_base');
    if (saved) store.apiBase = saved;
  }
  if (window.__CK_TOKEN__) store.apiToken = window.__CK_TOKEN__;
}

// ── small shared helpers ──────────────────────────────────────────
export function slugify(v) {
  return String(v).toLowerCase().trim().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)+/g, '');
}
export function fmtDate(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}
export function fmtDateTime(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}
// stable tone from a string — for sigils that have no assigned colour
const TONES = ['burgundy', 'moss', 'blue', 'ochre', 'gilt'];
export function toneFor(str) {
  let h = 0;
  for (const c of String(str || '')) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return TONES[h % TONES.length];
}
export function initials(name) {
  const parts = String(name || '?').trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}
