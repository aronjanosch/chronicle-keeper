// Core: global store + HTTP client. The store is a plain object with a tiny
// pub/sub; `useStore()` re-renders any component that reads it. Actions live in
// actions.js and mutate via setState().
import { useState, useEffect } from '../vendor/htm-preact-standalone.mjs';

// ── Global state ──────────────────────────────────────────────────
export const store = {
  apiBase: 'http://127.0.0.1:8000',
  apiToken: null,
  shellMode: false,       // true when the Tauri shell injected the API base (browser-dev → false)

  // routing: { name: 'library'|'campaign'|'sessions'|'session'|'newSession'|'summarize'|'settings'|'codex'|'codexEntry'|'page', params }
  route: { name: 'library', params: {} },

  // data
  config: null,
  campaigns: [],          // [{campaign_id, name, ...detail}]
  campaign: null,         // current campaign detail
  campaignSessions: [],   // sessions of current campaign
  codexEntries: [],       // structured codex entries for current campaign
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
  if (name === 'page' && params.path) recordRecentPage(params.path);
  setState({ route: { name, params }, canNavBack: navStack.back.length > 0, canNavFwd: false });
}

export function navigateBack() {
  const entry = navStack.back.pop();
  if (!entry) return;
  navStack.fwd.push({ name: store.route.name, params: store.route.params });
  if (entry.name === 'page' && entry.params.path) recordRecentPage(entry.params.path);
  setState({ route: entry, canNavBack: navStack.back.length > 0, canNavFwd: navStack.fwd.length > 0 });
}

export function navigateForward() {
  const entry = navStack.fwd.pop();
  if (!entry) return;
  navStack.back.push({ name: store.route.name, params: store.route.params });
  if (entry.name === 'page' && entry.params.path) recordRecentPage(entry.params.path);
  setState({ route: entry, canNavBack: navStack.back.length > 0, canNavFwd: navStack.fwd.length > 0 });
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
