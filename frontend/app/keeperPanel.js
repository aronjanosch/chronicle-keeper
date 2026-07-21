// The Keeper docked panel + shared chat surface. The panel is a floating pill
// on every in-world screen; the Keeper screen (screens/keeper.js) reuses the
// exported Transcript/Composer/actions over the same store.keeper state.
// 6.4: structural/shell permission cards, attachments (picker + drag-drop),
// [[ autocomplete in the composer.
import { html, useState, useEffect, useRef } from '../vendor/htm-preact-standalone.mjs';
import { apiFetch, apiJson, apiStream, bump, navigate, setOp, setState, store } from './core.js';
import { Icon, Spinner, renderBlockHtml, wikilinkClick, openContextMenu } from './ui.js';
import { loadLlmProviders, fetchLlmModels, loadVaultTree, loadSkills, copyText } from './actions.js';

// store.keeper = { open, chatId, campaignId, events[], attachments[], error, mode }
// store.keeperRuns = { [chatId]: { chatId, campaignId, live: {text, tools[], ask} } } —
// every in-flight turn, one per chat (the backend allows one concurrent run
// per chat, not per world). Kept separate from store.keeper so switching which
// chat is displayed never interrupts or corrupts a streaming run: keeperState()
// overlays runs[chatId].live onto whichever chat is displayed, so a
// backgrounded run in another chat keeps streaming untouched and reappears
// mid-token when its chat reopens.

export const MODES = [
  { id: 'read_only', label: 'Read-only' },
  { id: 'ask', label: 'Ask' },
  { id: 'plan', label: 'Plan' },
  { id: 'accept_edits', label: 'Accept edits' },
  { id: 'yolo', label: 'Yolo' },
];

const MAX_FILE_BYTES = 256 * 1024;

// Built-in /commands in the composer (distinct from skills, which seed a
// directive). A command's run() fires immediately when picked.
const KEEPER_COMMANDS = [
  { slug: 'compact', name: 'Compact conversation', description: 'Summarize this chat to free up context', command: true, run: () => compactChat() },
];
const slashLabel = { padding: '5px 8px 3px', fontSize: 10.5, letterSpacing: '0.05em', textTransform: 'uppercase', color: 'var(--ink-faint)' };

// Tools that mutate vault pages — a successful result means the open page may
// now be stale, so refresh it mid-stream instead of waiting for run end.
const VAULT_WRITE_TOOLS = new Set([
  'create_page', 'edit_page', 'multi_edit_page', 'insert_into_page',
  'write_page', 'rename_page', 'move_page',
  'delete_page', 'create_folder',
]);
const MAX_IMAGE_BYTES = 8 * 1024 * 1024;
// Longest-edge cap for pasted images. A raw screenshot is multi-MB of base64
// that gets stored in the chat and re-sent to the model every turn; downscaling
// to this on paste cuts it to a few hundred KB with no loss the model can use.
const MAX_IMAGE_EDGE = 1568;

// Decode → downscale (longest edge ≤ MAX_IMAGE_EDGE) → re-encode JPEG. Returns
// the { media_type, data, url } shape the composer + backend expect.
function downscaleImage(file) {
  return new Promise((resolve, reject) => {
    const objUrl = URL.createObjectURL(file);
    const img = document.createElement('img');
    img.onload = () => {
      URL.revokeObjectURL(objUrl);
      const scale = Math.min(1, MAX_IMAGE_EDGE / Math.max(img.width, img.height));
      const w = Math.max(1, Math.round(img.width * scale));
      const h = Math.max(1, Math.round(img.height * scale));
      const canvas = document.createElement('canvas');
      canvas.width = w; canvas.height = h;
      canvas.getContext('2d').drawImage(img, 0, 0, w, h);
      const dataUrl = canvas.toDataURL('image/jpeg', 0.85);
      resolve({ media_type: 'image/jpeg', data: dataUrl.slice(dataUrl.indexOf(',') + 1), url: dataUrl });
    };
    img.onerror = (err) => { URL.revokeObjectURL(objUrl); reject(err); };
    img.src = objUrl;
  });
}

export function keeperState() {
  const k = store.keeper || { open: false, chatId: null, events: [], error: null };
  const base = k.attachments ? k : { ...k, attachments: [] };
  const withMode = base.mode ? base : { ...base, mode: localStorage.getItem('ck_keeper_mode') || 'ask' };
  // Always recomputed — never trust a stray `live` baked into store.keeper by a
  // previous patchKeeper spread (see patchKeeper below).
  const run = store.keeperRuns?.[withMode.chatId];
  return { ...withMode, live: run ? run.live : null };
}

export function patchKeeper(patch) {
  setState({ keeper: { ...keeperState(), ...patch } });
}

// Mutates one chat's run in place. No-op if that run already ended or was
// replaced — a stale onEvent closure firing after its stream's owner is gone.
function patchRun(chatId, patch) {
  const runs = store.keeperRuns || {};
  const cur = runs[chatId];
  if (!cur) return;
  setState({ keeperRuns: { ...runs, [chatId]: { ...cur, ...patch } } });
}

function clearRun(chatId) {
  const runs = store.keeperRuns || {};
  if (!(chatId in runs)) return;
  const next = { ...runs };
  delete next[chatId];
  setState({ keeperRuns: next });
}

// Providers usable right now: keyless (Ollama) or with a saved key.
export function configuredProviders() {
  return (store.llmProviders || []).filter((p) => !p.needs_key || p.has_key);
}

// What a fresh chat starts on: the default provider on its last-used model
// (saved_model), falling back to the provider's built-in suggestion.
export function defaultPick() {
  const provider = (store.config?.summary_provider || 'ollama').toLowerCase();
  const p = (store.llmProviders || []).find((x) => x.id === provider);
  return { provider, model: (p && (p.saved_model || p.default_model)) || '' };
}

// The model a chat resumes on: whatever it last used (persisted `model` event),
// else the last-used-per-provider default. Keeps old chats on their own model.
function pickForChat(events) {
  for (let i = events.length - 1; i >= 0; i--) {
    if (events[i].type === 'model' && events[i].model) {
      return { provider: events[i].provider, model: events[i].model };
    }
  }
  return defaultPick();
}

// Kind of the page currently open in the editor (drives suggestion chips).
function currentPageKind() {
  if (store.route?.name !== 'page') return '';
  const p = (store.vaultPages || []).find((x) => x.path === store.route?.params?.path);
  return p?.kind || '';
}

// Skills whose kinds: match a page kind — pure string match, zero inference.
function skillsForKind(kind) {
  if (!kind) return [];
  const k = kind.toLowerCase();
  return (store.keeperSkills || []).filter((s) => s.enabled !== false && (s.kinds || []).some((x) => String(x).toLowerCase() === k));
}

// The composer text a chip / command seeds: a plain user directive that makes
// the Keeper pull the skill via use_skill. On a page, point it at that page.
function skillDirective(skill, onPage) {
  return onPage
    ? `Use the "${skill.name}" skill to help me develop this page.`
    : `Use the "${skill.name}" skill. `;
}

async function fetchChatInto(chatId) {
  const cid = store.campaign?.campaign_id;
  if (!cid || !chatId) return;
  const [{ events, undoable }, att] = await Promise.all([
    apiFetch(`/campaigns/${cid}/agent/chats/${chatId}`),
    apiFetch(`/campaigns/${cid}/agent/chats/${chatId}/attachments`).catch(() => ({ attachments: [] })),
  ]);
  // Entering a chat resumes the model it last used; a fresh/empty chat falls
  // back to the last-used-per-provider default.
  patchKeeper({ chatId, events, undoable: undoable || 0, attachments: att.attachments || [], live: null, error: null, ...pickForChat(events) });
}

// Blank, never-used chats are ephemeral: discard the one we're leaving so the
// list never fills with empty "New chat" rows. Fire-and-forget.
function discardIfEmpty(cid, chat) {
  if (!cid || !chat?.chatId || chat.live || chat.events.length) return;
  apiJson(`/campaigns/${cid}/agent/chats/${chat.chatId}`, 'DELETE', {})
    .then(() => bump('keeper')).catch(() => {});
}

export async function openChat(chatId) {
  const prev = keeperState();
  try {
    await fetchChatInto(chatId);
    if (prev.chatId && prev.chatId !== chatId) discardIfEmpty(prev.campaignId, prev);
  } catch (e) { patchKeeper({ error: String(e.message || e) }); }
}

export async function openPanel() {
  const cid = store.campaign?.campaign_id;
  if (!cid) return;
  // Chats are per-world — a stale chat id from another world must not leak.
  if (keeperState().campaignId !== cid) {
    patchKeeper({ campaignId: cid, chatId: null, events: [], attachments: [], live: null });
  }
  patchKeeper({ open: true, error: null });
  loadLlmProviders();
  loadSkills(cid);
  const k = keeperState();
  if (k.chatId) return;
  try {
    const { chats } = await apiFetch(`/campaigns/${cid}/agent/chats`);
    // Most recent first (server sorts by mtime). Prune abandoned blanks, keeping
    // at most the most-recent chat — that becomes the open one (or a fresh blank).
    const keep = chats[0];
    for (const c of chats) {
      if (c.id !== keep?.id && c.message_count === 0) {
        apiJson(`/campaigns/${cid}/agent/chats/${c.id}`, 'DELETE', {}).catch(() => {});
      }
    }
    const chat = keep || await apiJson(`/campaigns/${cid}/agent/chats`, 'POST', {});
    bump('keeper');
    await fetchChatInto(chat.id);
  } catch (e) {
    patchKeeper({ error: String(e.message || e) });
  }
}

export async function newChat() {
  const cid = store.campaign?.campaign_id;
  if (!cid) return;
  // Already sitting on an untouched blank? Reuse it instead of stacking another.
  const k = keeperState();
  if (k.chatId && !k.events.length && !k.live) return k.chatId;
  try {
    const chat = await apiJson(`/campaigns/${cid}/agent/chats`, 'POST', {});
    patchKeeper({ chatId: chat.id, events: [], undoable: 0, attachments: [], live: null, error: null, ...defaultPick() });
    bump('keeper');
    return chat.id;
  } catch (e) {
    patchKeeper({ error: String(e.message || e) });
  }
}

export async function sendMessage(text, images = []) {
  const cid = store.campaign?.campaign_id;
  let k = keeperState();
  if (!cid || k.live) return;
  // No chat yet (rail opened, openPanel still in flight, or a fresh world):
  // create one so the message lands instead of vanishing.
  if (!k.chatId) {
    if (!(await newChat())) return;
    k = keeperState();
  }
  // Captured once: this run belongs to this chat for its whole lifetime, even
  // if the user switches the displayed chat mid-stream.
  const chatId = k.chatId;
  const isFocused = () => keeperState().chatId === chatId;
  const events = [...k.events, { type: 'user', text, images }];
  patchKeeper({ events, error: null });
  setState({ keeperRuns: { ...store.keeperRuns, [chatId]: { chatId, campaignId: cid, live: { text: '', tools: [] } } } });
  let toolsRan = false;
  try {
    const body = { text, mode: k.mode };
    if (images.length) body.images = images.map((i) => ({ media_type: i.media_type, data: i.data }));
    if (k.provider) body.provider = k.provider;
    if (k.model) body.model = k.model;
    // Silently hand the Keeper the editor state — the open page + other tabs.
    const focusPath = store.route?.name === 'page' ? store.route?.params?.path : null;
    if (focusPath) body.focus = { path: focusPath, tabs: store.tabs || [] };
    await apiStream(`/campaigns/${cid}/agent/chats/${chatId}/messages`, body, (ev) => {
      const run = store.keeperRuns?.[chatId];
      if (!run) return; // aborted/replaced elsewhere
      const live = run.live || { text: '', tools: [] };
      if (ev.type === 'text_delta') {
        patchRun(chatId, { live: { ...live, text: live.text + ev.text } });
      } else if (ev.type === 'permission_request') {
        patchRun(chatId, { live: { ...live, ask: { requestId: ev.request_id, name: ev.name, diff: ev.diff } } });
      } else if (ev.type === 'tool_start') {
        toolsRan = true;
        patchRun(chatId, { live: { ...live, ask: null, tools: [...live.tools, { name: ev.name, args: ev.args_summary, diff: ev.diff, running: true }] } });
      } else if (ev.type === 'tool_result') {
        const tools = live.tools.slice();
        const i = tools.findLastIndex((t) => t.running && t.name === ev.name);
        if (i >= 0) tools[i] = { ...tools[i], running: false, summary: ev.summary, isError: ev.is_error };
        // A tool round means the streamed text so far belongs to a finished
        // assistant turn — fold it into the row list and reset the buffer.
        // Only the currently displayed chat keeps its events array live; a
        // backgrounded chat picks this up from disk when it's reopened.
        patchRun(chatId, { live: { ...live, text: '', tools, ask: null } });
        if (live.text.trim() && isFocused()) {
          patchKeeper({ events: [...keeperState().events, { type: 'assistant', text: live.text }] });
        }
        if (!ev.is_error && VAULT_WRITE_TOOLS.has(ev.name)) { loadVaultTree(cid); bump('vault'); }
      } else if (ev.type === 'notice') {
        // Mode change (e.g. grounded fallback) — show it inline right away;
        // the post-stream reload picks up the persisted event either way.
        if (isFocused()) patchKeeper({ events: [...keeperState().events, { type: 'notice', message: ev.message }] });
      } else if (ev.type === 'error') {
        if (isFocused()) patchKeeper({ error: ev.message });
      }
    });
  } catch (e) {
    if (isFocused()) patchKeeper({ error: String(e.message || e) });
  }
  // Authoritative reload: persisted jsonl is the truth for the transcript.
  // Only apply it to store.keeper if this chat is still the one displayed —
  // otherwise leave it alone, the next openChat() will fetch it fresh.
  try {
    const { events: fresh, undoable } = await apiFetch(`/campaigns/${cid}/agent/chats/${chatId}`);
    if (isFocused()) patchKeeper({ events: fresh, undoable: undoable || 0 });
  } catch (_) {}
  clearRun(chatId);
  bump('keeper'); // chat list title/count, brief staleness, memories
  if (toolsRan) { loadVaultTree(cid); bump('vault'); } // tools may have touched pages
}

// /compact: summarize the chat into a boundary so the context resets. The full
// log stays on disk; the transcript reloads showing a compacted divider. Runs
// through the same per-chat run slot as a turn, surfaced inline (not a toast)
// via a `compacting` live state.
export async function compactChat() {
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (!cid || !k.chatId || k.live) return;
  if (!k.events.some((e) => e.type === 'assistant')) { patchKeeper({ error: 'Nothing to compact yet.' }); return; }
  const chatId = k.chatId;
  const isFocused = () => keeperState().chatId === chatId;
  patchKeeper({ error: null });
  setState({ keeperRuns: { ...store.keeperRuns, [chatId]: { chatId, campaignId: cid, live: { compacting: true, text: '', tools: [] } } } });
  try {
    const body = {};
    if (k.provider) body.provider = k.provider;
    if (k.model) body.model = k.model;
    const { events } = await apiJson(`/campaigns/${cid}/agent/chats/${chatId}/compact`, 'POST', body);
    if (isFocused()) patchKeeper({ events: events || keeperState().events, error: null });
  } catch (e) {
    if (isFocused()) patchKeeper({ error: String(e.message || e) });
  }
  clearRun(chatId);
  bump('keeper');
}

// Aborts the run for a specific chat (defaults to the one displayed) — each
// chat now has its own run slot, so this only stops that chat, not the world.
export async function abortRun(chatId) {
  const cid = store.campaign?.campaign_id;
  const id = chatId || keeperState().chatId;
  if (!cid || !id) return;
  try { await apiJson(`/campaigns/${cid}/agent/chats/${id}/abort`, 'POST', {}); } catch (_) {}
}

// If this chat is mid-turn, also flip its live run so the very next gate
// check picks up the switch (e.g. into Yolo) instead of waiting for the
// turn to finish — otherwise this only takes effect on the next message.
export function setMode(mode) {
  localStorage.setItem('ck_keeper_mode', mode);
  patchKeeper({ mode });
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (cid && k.chatId && k.live) {
    apiJson(`/campaigns/${cid}/agent/chats/${k.chatId}/mode`, 'POST', { mode }).catch(() => {});
  }
}

async function decide(requestId, decision) {
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (!cid || !k.chatId) return;
  // Clear the ask on the run itself (the source of truth for `live`), not the
  // derived store.keeper — patching store.keeper.live would be overwritten by
  // keeperState()'s overlay on the next read anyway.
  if (k.live) patchRun(k.chatId, { live: { ...k.live, ask: null } });
  try {
    await apiJson(`/campaigns/${cid}/agent/chats/${k.chatId}/approve`, 'POST', { request_id: requestId, decision });
  } catch (e) {
    patchKeeper({ error: String(e.message || e) });
  }
}

export async function undoLast() {
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (!cid || !k.chatId || k.live) return;
  try {
    const { restored, remaining } = await apiJson(`/campaigns/${cid}/agent/chats/${k.chatId}/undo`, 'POST', { scope: 'last' });
    setOp(restored.length ? `Restored ${restored.join(', ')}` : 'Nothing to undo', restored.length ? 'done' : '');
    if (typeof remaining === 'number') patchKeeper({ undoable: remaining });
    if (restored.length) {
      loadVaultTree(cid);
      bump('vault');
    }
  } catch (e) {
    patchKeeper({ error: String(e.message || e) });
  }
}

// ── attachments ──────────────────────────────────────────────────

async function addRefAttachment(body) {
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (!cid || !k.chatId) return;
  try {
    await apiJson(`/campaigns/${cid}/agent/chats/${k.chatId}/attachments`, 'POST', body);
    const { attachments } = await apiFetch(`/campaigns/${cid}/agent/chats/${k.chatId}/attachments`);
    patchKeeper({ attachments });
  } catch (e) {
    setOp(String(e.message || e), 'error');
  }
}

async function addFileAttachment(file) {
  if (file.size > MAX_FILE_BYTES) { setOp(`${file.name} is too large (max 256 KB).`, 'error'); return; }
  let text;
  try { text = await file.text(); } catch (_) { setOp('Could not read that file.', 'error'); return; }
  if (text.includes('\0')) { setOp('Text files only for now.', 'error'); return; }
  await addRefAttachment({ name: file.name, content: text });
}

async function removeAttachment(id) {
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (!cid || !k.chatId) return;
  try {
    await apiJson(`/campaigns/${cid}/agent/chats/${k.chatId}/attachments/${id}`, 'DELETE', {});
    patchKeeper({ attachments: keeperState().attachments.filter((a) => a.id !== id) });
  } catch (e) {
    setOp(String(e.message || e), 'error');
  }
}

function useDropAttachments() {
  const [dragging, setDragging] = useState(false);
  return {
    dragging,
    bind: {
      onDragOver: (e) => { e.preventDefault(); if (!dragging) setDragging(true); },
      onDragLeave: (e) => { if (e.target === e.currentTarget) setDragging(false); },
      onDrop: (e) => {
        e.preventDefault();
        setDragging(false);
        [...(e.dataTransfer?.files || [])].forEach(addFileAttachment);
      },
    },
  };
}

const ATT_GLYPH = { page: 'book', session: 'mic', transcript: 'mic', file: 'scroll' };

function AttachChips({ attachments }) {
  if (!attachments.length) return null;
  return html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 6, padding: '0 10px 6px' }}>
    ${attachments.map((a) => html`<div key=${a.id} title=${a.label} style=${{
      display: 'flex', alignItems: 'center', gap: 5, maxWidth: 200,
      padding: '3px 6px 3px 8px', fontSize: 11.5, borderRadius: 999,
      background: 'var(--surface)', border: '1px solid var(--rule-soft)', color: 'var(--ink-muted)',
    }}>
      <${Icon} name=${ATT_GLYPH[a.kind] || 'scroll'} size=${11} />
      <span style=${{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${a.label}</span>
      <span onClick=${() => removeAttachment(a.id)} title="Remove" style=${{ cursor: 'pointer', display: 'flex', color: 'var(--ink-faint)' }}><${Icon} name="x" size=${10} /></span>
    </div>`)}
  </div>`;
}

// Popover above the composer. Default: attach pages/sessions as chips (the +
// button). Link mode (onPickPage set, from @): pages only, insert a wikilink.
function AttachPicker({ onClose, onPickPage }) {
  const [q, setQ] = useState('');
  const [sel, setSel] = useState(0);
  const linkMode = !!onPickPage;
  const ql = q.trim().toLowerCase();
  const pages = (store.vaultPages || [])
    .filter((p) => p.title && (!ql || p.title.toLowerCase().includes(ql)))
    .slice(0, 8);
  const sessions = linkMode ? [] : (store.campaignSessions || [])
    .filter((s) => !ql || String(s.session_number || '').includes(ql) || (s.title || '').toLowerCase().includes(ql))
    .slice(0, 6);
  const rows = [
    ...pages.map((p) => ({ key: p.path, icon: 'book', label: p.title, run: () => (linkMode ? onPickPage(p) : addRefAttachment({ kind: 'page', path: p.path })) })),
    ...sessions.map((s) => ({ key: `s${s.session_id}`, icon: 'mic', label: `Session ${String(s.session_number || 0).padStart(2, '0')}${s.title ? ` — ${s.title}` : ''}`, run: () => addRefAttachment({ kind: 'session', session: s.session_number }) })),
  ];
  const at = Math.min(sel, rows.length - 1);
  const pick = (row) => { onClose(); row.run(); };
  const onKey = (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); setSel((i) => (i + 1) % rows.length); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setSel((i) => (i - 1 + rows.length) % rows.length); }
    else if (e.key === 'Enter') { e.preventDefault(); if (rows[at]) pick(rows[at]); }
    else if (e.key === 'Escape') { e.preventDefault(); onClose(); }
  };
  return html`<div style=${{
    position: 'absolute', bottom: 'calc(100% + 6px)', left: 10, right: 10, zIndex: 5,
    background: 'var(--paper)', border: '1px solid var(--rule)', borderRadius: 8,
    boxShadow: 'var(--shadow-raised)', maxHeight: 280, overflow: 'auto', padding: 6,
  }}>
    <input autofocus value=${q} placeholder=${linkMode ? 'Link a page…' : 'Attach a page or session…'}
      onInput=${(e) => { setQ(e.target.value); setSel(0); }} onKeyDown=${onKey}
      style=${{ width: '100%', boxSizing: 'border-box', fontSize: 12.5, padding: '6px 8px', marginBottom: 4, borderRadius: 5, border: '1px solid var(--rule)', background: 'var(--surface)', color: 'var(--ink)' }} />
    ${rows.map((row, i) => html`<div key=${row.key} onClick=${() => pick(row)} onMouseEnter=${() => setSel(i)}
      class=${`ck-ac-item${i === at ? ' on' : ''}`} style=${pickRow}>
      <${Icon} name=${row.icon} size=${12} /> ${row.label}
    </div>`)}
    ${!rows.length && html`<div style=${{ padding: 8, fontSize: 12, color: 'var(--ink-faint)' }}>${linkMode ? 'No page matches.' : 'Nothing matches. Drop a text file to attach it.'}</div>`}
  </div>`;
}
const pickRow = { display: 'flex', alignItems: 'center', gap: 7, padding: '6px 8px', fontSize: 12.5, cursor: 'pointer', borderRadius: 5, color: 'var(--ink)' };

// {path, old, new} → red/green diff lines (Phase 5 DiffLine styling).
function DiffView({ diff }) {
  const lines = (s) => (s == null ? [] : String(s).split('\n'));
  const row = (mode, text, i) => {
    const tone = mode === 'add'
      ? { bg: 'var(--moss-50)', col: 'var(--ink)', mark: '+', markCol: 'var(--moss)' }
      : { bg: 'rgba(122,46,31,.07)', col: 'var(--ink-muted)', mark: '−', markCol: 'var(--burgundy-700)' };
    return html`<div key=${`${mode}${i}`} style=${{ display: 'flex', gap: 8, padding: '2px 10px', background: tone.bg, fontSize: 12, lineHeight: 1.5 }}>
      <span style=${{ fontFamily: 'var(--font-mono)', color: tone.markCol, flex: '0 0 auto', width: 9 }}>${tone.mark}</span>
      <span style=${{ color: tone.col, whiteSpace: 'pre-wrap', wordBreak: 'break-word', textDecoration: mode === 'remove' ? 'line-through' : 'none', textDecorationColor: 'rgba(122,46,31,.4)' }}>${text}</span>
    </div>`;
  };
  return html`<div style=${{ border: '1px solid var(--rule)', borderRadius: 6, overflow: 'auto', background: 'var(--surface)', padding: '4px 0', maxHeight: 260 }}>
    ${lines(diff.old).map((l, i) => row('remove', l, i))}
    ${lines(diff.new).map((l, i) => row('add', l, i))}
  </div>`;
}

const WRITE_VERB = {
  create_page: 'create', edit_page: 'edit', write_page: 'overwrite',
  multi_edit_page: 'edit', insert_into_page: 'add to',
};

function PermissionCard({ ask }) {
  const d = ask.diff || {};
  const isShell = d.command != null;
  const isStructural = !isShell && d.summary != null && d.new == null;
  const isFoundry = d.action === 'sync_foundry' || String(d.action || '').startsWith('foundry_');
  const isWeb = d.action === 'web_search' || d.action === 'web_fetch';
  const title = isShell
    ? html`The Keeper wants to run a command`
    : isStructural
      ? html`The Keeper wants to ${d.summary}`
      : html`The Keeper wants to ${WRITE_VERB[ask.name] || ask.name} ${' '}<span style=${{ fontFamily: 'var(--font-mono)', fontWeight: 500 }}>${d.path || ''}</span>`;
  return html`<div style=${{ margin: '10px 0', border: '1px solid var(--rule)', borderRadius: 8, background: 'var(--paper-deep)', overflow: 'hidden' }}>
    <div style=${{ padding: '8px 12px', fontSize: 12.5, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 7 }}>
      <${Icon} name="feather" size=${13} /> ${title}
    </div>
    <div style=${{ padding: '0 10px 8px' }}>
      ${isShell && html`<div style=${{ fontFamily: 'var(--font-mono)', fontSize: 12, background: 'var(--ink)', color: '#F2ECE0', padding: '8px 10px', borderRadius: 6, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>$ ${d.command}</div>
        <div style=${{ fontSize: 11, color: 'var(--ink-faint)', marginTop: 4 }}>in ${d.cwd || 'the world folder'}</div>`}
      ${!isShell && !isStructural && html`<${DiffView} diff=${d} />`}
    </div>
    <div style=${{ display: 'flex', gap: 8, padding: '0 10px 10px' }}>
      <button class="btn btn-primary" onClick=${() => decide(ask.requestId, 'allow_once')}>Allow once</button>
      ${!isShell && !isFoundry && !isWeb && html`<button class="btn" onClick=${() => decide(ask.requestId, 'allow_chat')}>Allow for this chat</button>`}
      <button class="btn" style=${{ marginLeft: 'auto', color: 'var(--burgundy-700)' }} onClick=${() => decide(ask.requestId, 'deny')}>Deny</button>
    </div>
  </div>`;
}

const ROW_VERB = {
  rename_page: 'rename', move_page: 'move', delete_page: 'delete', create_folder: 'folder', run_command: 'shell',
  vault_diagnostics: 'diagnostics', tags: 'tags', query_world: 'query', page_kinds: 'kinds', read_recap: 'recap',
  multi_edit_page: 'edit', insert_into_page: 'insert', search_summaries: 'summaries',
  web_search: 'web search', web_fetch: 'web fetch',
};

function ToolRow({ name, summary, isError, running, args, diff }) {
  const [openRow, setOpenRow] = useState(false);
  const tint = isError ? 'var(--burgundy-700)' : 'var(--ink-muted)';
  const expandable = !!summary || !!diff;
  const label = diff?.command ? `$ ${diff.command}` : (diff?.summary || diff?.path || (running ? (args || '') : (summary || '')));
  return html`<div style=${{ margin: '6px 0' }}>
    <div onClick=${() => setOpenRow(!openRow)} style=${{
      display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, color: tint,
      padding: '4px 8px', background: 'var(--paper-deep)', borderRadius: 5,
      border: '1px solid var(--rule-soft)', cursor: expandable ? 'pointer' : 'default',
    }}>
      ${running ? html`<${Spinner} size=${12} />` : html`<${Icon} name=${isError ? 'x' : 'check'} size=${12} />`}
      <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11.5 }}>${ROW_VERB[name] || name}</span>
      <span style=${{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', color: 'var(--ink-faint)' }}>${label}</span>
    </div>
    ${openRow && diff && diff.new != null && html`<div style=${{ margin: '4px 0 0 20px' }}><${DiffView} diff=${diff} /></div>`}
    ${openRow && (!diff || diff.new == null) && summary && html`<div style=${{ fontSize: 12, color: 'var(--ink-muted)', padding: '6px 10px', whiteSpace: 'pre-wrap', fontFamily: 'var(--font-mono)' }}>${summary}</div>`}
  </div>`;
}

// In-flight compaction: an assistant-side line with a pulsing feather and
// shimmering label — reads as the Keeper working, not a toast.
function CompactingIndicator() {
  return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, margin: '12px 0' }}>
    <span class="ck-pulse" style=${{ display: 'flex', color: 'var(--burgundy)' }}><${Icon} name="feather" size=${14} /></span>
    <span class="ck-shimmer-text" style=${{ fontSize: 13, fontWeight: 500 }}>Compacting conversation…</span>
  </div>`;
}

// A /compact boundary: a labelled rule; click to read the carried-forward summary.
function CompactRow({ summary }) {
  const [open, setOpen] = useState(false);
  return html`<div style=${{ margin: '14px 0' }}>
    <div onClick=${() => summary && setOpen(!open)} style=${{ display: 'flex', alignItems: 'center', gap: 8, cursor: summary ? 'pointer' : 'default', color: 'var(--ink-faint)' }}>
      <span style=${{ flex: 1, height: 1, background: 'var(--rule-soft)' }} />
      <span style=${{ display: 'flex', alignItems: 'center', gap: 5, fontSize: 11, letterSpacing: '0.04em', textTransform: 'uppercase' }}>
        <${Icon} name="feather" size=${11} /> Conversation compacted${summary ? html` <${Icon} name=${open ? 'chev-d' : 'chev-r'} size=${11} />` : ''}
      </span>
      <span style=${{ flex: 1, height: 1, background: 'var(--rule-soft)' }} />
    </div>
    ${open && summary && html`<div class="ck-prose" style=${{ fontSize: 12.5, marginTop: 8, padding: '8px 12px', background: 'var(--paper-deep)', border: '1px solid var(--rule-soft)', borderRadius: 6, color: 'var(--ink-muted)' }}
      dangerouslySetInnerHTML=${{ __html: renderBlockHtml(summary, store.vaultPages) }} />`}
  </div>`;
}

function EventRow({ ev }) {
  if (ev.type === 'compact') return html`<${CompactRow} summary=${ev.summary} />`;
  if (ev.type === 'user') {
    const imgs = ev.images || [];
    const menu = (e) => openContextMenu(e, [
      ev.text && { label: 'Copy', icon: 'copy', onClick: () => copyText(ev.text, 'Message copied') },
      { label: 'Retry', icon: 'arrow-r', disabled: !!keeperState().live,
        onClick: () => sendMessage(ev.text, ev.images || []) },
    ]);
    return html`<div style=${{ margin: '10px 0', display: 'flex', justifyContent: 'flex-end' }}>
      <div onContextMenu=${menu} style=${{ maxWidth: '85%', background: 'var(--burgundy-50)', border: '1px solid var(--rule-soft)', borderRadius: '10px 10px 2px 10px', padding: '8px 12px', fontSize: 13, whiteSpace: 'pre-wrap' }}>
        ${imgs.length > 0 && html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 6, marginBottom: ev.text ? 6 : 0 }}>
          ${imgs.map((img, i) => html`<img key=${i} src=${img.url || `data:${img.media_type};base64,${img.data}`} alt="pasted"
            style=${{ maxWidth: 180, maxHeight: 180, borderRadius: 6, border: '1px solid var(--rule-soft)', display: 'block' }} />`)}
        </div>`}
        ${ev.text}
      </div>
    </div>`;
  }
  if (ev.type === 'assistant' && (ev.text || '').trim()) {
    return html`<div class="ck-prose" style=${{ fontSize: 13, margin: '10px 0' }}
      onClick=${wikilinkClick()}
      onContextMenu=${(e) => openContextMenu(e, [
        { label: 'Copy', icon: 'copy', onClick: () => copyText(ev.text, 'Message copied') },
      ])}
      dangerouslySetInnerHTML=${{ __html: renderBlockHtml(ev.text, store.vaultPages) }} />`;
  }
  if (ev.type === 'tool_result') {
    const first = (ev.content || '').split('\n').find((l) => l.trim() && !l.startsWith('Tool output') && l.trim() !== '```') || '';
    return html`<${ToolRow} name=${ev.name} summary=${first.trim()} isError=${ev.is_error} diff=${ev.diff} />`;
  }
  if (ev.type === 'permission' && ev.decision === 'deny') {
    return html`<div style=${{ margin: '8px 0', fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>${ev.diff?.summary ? `${ev.diff.summary}` : `Edit to ${ev.diff?.path || 'a page'}`} denied.</div>`;
  }
  if (ev.type === 'notice') {
    return html`<div style=${{ margin: '8px 0', display: 'flex', alignItems: 'flex-start', gap: 7, padding: '7px 10px', fontSize: 12, color: 'var(--ochre)', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.24)', borderRadius: 6, lineHeight: 1.45 }}>
      <${Icon} name="sparkle" size=${12} /> <span>${ev.message}</span>
    </div>`;
  }
  if (ev.type === 'error') {
    return html`<div style=${{ margin: '8px 0', fontSize: 12, color: 'var(--burgundy-700)' }}>⚠ ${ev.message}</div>`;
  }
  if (ev.type === 'aborted') {
    return html`<div style=${{ margin: '8px 0', fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>Stopped.</div>`;
  }
  return null;
}

// Compact overrides on the themed .select class (theme = border/focus/colors).
const pickSelect = { fontSize: 11.5, padding: '4px 6px', width: 'auto', maxWidth: 150, cursor: 'pointer' };

// Provider + model selects for the active chat — embedded in the composer
// footer. Resets to the global default on a new chat; the choice rides along in
// the /messages body as provider/model overrides. Returns the two selects only
// (no wrapper) so the caller controls layout.
export function PickerControls({ k }) {
  const provs = configuredProviders();
  const [models, setModels] = useState([]);

  useEffect(() => {
    if (!k.provider && provs.length) patchKeeper(defaultPick());
  }, [provs.length, k.provider]);

  useEffect(() => {
    if (!k.provider) return;
    let alive = true;
    const p = provs.find((x) => x.id === k.provider);
    fetchLlmModels(k.provider).then((live) => {
      if (alive) setModels(live.length ? live : (p?.models || []));
    });
    return () => { alive = false; };
  }, [k.provider]);

  if (!provs.length) return null;
  const provId = provs.some((p) => p.id === k.provider) ? k.provider : provs[0].id;
  const list = k.model && !models.includes(k.model) ? [k.model, ...models] : models;
  const onProvider = (id) => {
    const p = provs.find((x) => x.id === id);
    patchKeeper({ provider: id, model: (p?.saved_model || p?.default_model) || '' });
  };

  return html`
    ${provs.length > 1 && html`<select class="select" value=${provId} onChange=${(e) => onProvider(e.target.value)} title="Provider" style=${pickSelect}>
      ${provs.map((p) => html`<option key=${p.id} value=${p.id}>${p.name}</option>`)}
    </select>`}
    <select class="select" value=${k.model || ''} onChange=${(e) => patchKeeper({ model: e.target.value })} title="Model"
      style=${{ ...pickSelect, flex: 1, minWidth: 0, maxWidth: 'none' }}>
      ${!list.length && html`<option value=${k.model || ''}>${k.model || 'default'}</option>`}
      ${list.map((m) => html`<option key=${m} value=${m}>${m}</option>`)}
    </select>`;
}

// Float32 PCM chunks → 16-bit mono WAV blob. The mic is captured at the
// AudioContext's native rate (usually 48k); the backend resamples to 16k, so
// we ship the native rate untouched in the header.
function encodeWav(chunks, sampleRate) {
  let len = 0;
  for (const c of chunks) len += c.length;
  const buf = new ArrayBuffer(44 + len * 2);
  const view = new DataView(buf);
  const str = (off, s) => { for (let i = 0; i < s.length; i++) view.setUint8(off + i, s.charCodeAt(i)); };
  str(0, 'RIFF'); view.setUint32(4, 36 + len * 2, true); str(8, 'WAVE');
  str(12, 'fmt '); view.setUint32(16, 16, true); view.setUint16(20, 1, true);
  view.setUint16(22, 1, true); view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true); view.setUint16(32, 2, true); view.setUint16(34, 16, true);
  str(36, 'data'); view.setUint32(40, len * 2, true);
  let off = 44;
  for (const c of chunks) {
    for (let i = 0; i < c.length; i++, off += 2) {
      const s = Math.max(-1, Math.min(1, c[i]));
      view.setInt16(off, s < 0 ? s * 0x8000 : s * 0x7fff, true);
    }
  }
  return new Blob([buf], { type: 'audio/wav' });
}

// Push-to-talk dictation: record mic → WAV → POST /transcribe-dictation →
// resolve to text. status is 'idle' | 'recording' | 'transcribing'.
function useDictation(onText) {
  const [status, setStatus] = useState('idle');
  const ref = useRef(null); // { ctx, stream, node, source, chunks }

  async function start() {
    if (status !== 'idle') return;
    let stream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch (_) {
      setOp('Microphone unavailable — check permissions.', 'error');
      return;
    }
    const ctx = new (window.AudioContext || window.webkitAudioContext)();
    const source = ctx.createMediaStreamSource(stream);
    const node = ctx.createScriptProcessor(4096, 1, 1);
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 1024;
    const chunks = [];
    node.onaudioprocess = (e) => chunks.push(new Float32Array(e.inputBuffer.getChannelData(0)));
    source.connect(analyser);
    source.connect(node);
    node.connect(ctx.destination);
    ref.current = { ctx, stream, node, source, analyser, chunks };
    setStatus('recording');
  }

  async function stop() {
    const r = ref.current;
    ref.current = null;
    if (!r) { setStatus('idle'); return; }
    r.node.disconnect(); r.source.disconnect();
    r.stream.getTracks().forEach((t) => t.stop());
    const sampleRate = r.ctx.sampleRate;
    await r.ctx.close();
    if (!r.chunks.length) { setStatus('idle'); return; }
    setStatus('transcribing');
    try {
      const blob = encodeWav(r.chunks, sampleRate);
      const { text } = await apiFetch('/transcribe-dictation', {
        method: 'POST',
        headers: { 'Content-Type': 'audio/wav' },
        body: blob,
      });
      if (text && text.trim()) onText(text.trim());
      else setOp('No speech detected.', 'info');
    } catch (e) {
      setOp(`Dictation failed: ${e.message}`, 'error');
    }
    setStatus('idle');
  }

  function cancel() {
    const r = ref.current;
    ref.current = null;
    if (r) { r.node.disconnect(); r.source.disconnect(); r.stream.getTracks().forEach((t) => t.stop()); r.ctx.close(); }
    setStatus('idle');
  }

  useEffect(() => cancel, []); // tear down on unmount
  return { status, start, stop, cancel, analyser: ref.current?.analyser || null };
}

// Live scrolling waveform off the recorder's AnalyserNode. Draws on a canvas
// (rAF, no Preact re-render) — each frame pushes one RMS bar, oldest scroll off
// the left. Bar colour follows the canvas's CSS `color`.
function Waveform({ analyser }) {
  const cvs = useRef(null);
  const bars = useRef([]);
  useEffect(() => {
    if (!analyser) return undefined;
    const buf = new Uint8Array(analyser.fftSize);
    let raf;
    const tick = () => {
      analyser.getByteTimeDomainData(buf);
      let sum = 0;
      for (let i = 0; i < buf.length; i++) { const v = (buf[i] - 128) / 128; sum += v * v; }
      const rms = Math.sqrt(sum / buf.length);
      const c = cvs.current;
      if (c) {
        const dpr = window.devicePixelRatio || 1;
        const w = c.clientWidth, h = c.clientHeight;
        if (c.width !== Math.round(w * dpr)) { c.width = Math.round(w * dpr); c.height = Math.round(h * dpr); }
        const ctx = c.getContext('2d');
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        const gap = 3, bw = 2, n = Math.max(1, Math.floor(w / gap));
        bars.current.push(rms);
        while (bars.current.length > n) bars.current.shift();
        ctx.clearRect(0, 0, w, h);
        ctx.fillStyle = getComputedStyle(c).color || '#888';
        for (let i = 0; i < bars.current.length; i++) {
          const lvl = Math.min(1, bars.current[i] * 3.2);
          const bh = Math.max(2, lvl * h);
          const x = w - (bars.current.length - i) * gap;
          ctx.fillRect(x, (h - bh) / 2, bw, bh);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [analyser]);
  return html`<canvas ref=${cvs} style=${{ flex: 1, height: 24, minWidth: 0, color: 'var(--ink-muted)' }} />`;
}

// Unsent text per chat, kept across chat switches (module-scoped so it survives
// even if the Composer unmounts). Cleared once a chat's draft is sent.
const composerDrafts = new Map();

export function Composer({ busy, compacting }) {
  const [text, setText] = useState(() => composerDrafts.get(keeperState().chatId) || '');
  const [images, setImages] = useState([]);
  const [picker, setPicker] = useState(null); // 'attach' | 'link' | null
  const [linkAt, setLinkAt] = useState(-1); // index of the @ that opened link mode
  const [mentions, setMentions] = useState([]); // titles picked via @, rewritten to [[…]] on send
  const [slash, setSlash] = useState(null); // { q, index } when /command active
  const taRef = useRef(null);
  const chatIdRef = useRef(keeperState().chatId);
  const textRef = useRef(text);
  textRef.current = text;
  const k = keeperState();
  const onPage = store.route?.name === 'page';
  const dictation = useDictation((t) => {
    setText((prev) => (prev ? `${prev} ${t}` : t));
    requestAnimationFrame(() => { const ta = taRef.current; if (ta) { ta.focus(); autoGrow(ta); } });
  });

  // The /command menu lives here, so load skills wherever the composer mounts —
  // the sidebar panel and the dedicated Keeper screen alike (cached after first).
  useEffect(() => { loadSkills(store.campaign?.campaign_id); }, [store.campaign?.campaign_id]);

  // /command menu: built-in commands first, then skills — both filtered by what's
  // typed after the slash. Commands run an action; skills seed a directive.
  const matchQ = (slug, name) => !slash.q || slug.includes(slash.q) || (name || '').toLowerCase().includes(slash.q);
  const slashCmds = slash ? KEEPER_COMMANDS.filter((c) => matchQ(c.slug, c.name)) : [];
  const slashSkills = slash
    ? (store.keeperSkills || [])
      .filter((s) => s.enabled !== false && matchQ(s.slug, s.name))
      .slice(0, 8)
    : [];
  const slashItems = [...slashCmds, ...slashSkills];

  function autoGrow(ta) {
    if (!ta) return;
    ta.style.height = 'auto';
    ta.style.height = `${Math.min(ta.scrollHeight, 168)}px`;
  }
  useEffect(() => { autoGrow(taRef.current); }, [text]);

  // Switching chats: stash whatever's typed under the outgoing chat id, then
  // restore whatever was typed under the incoming one (or blank for a fresh chat).
  useEffect(() => {
    const prevId = chatIdRef.current;
    if (prevId === k.chatId) return;
    if (prevId) {
      if (text.trim()) composerDrafts.set(prevId, text); else composerDrafts.delete(prevId);
    }
    chatIdRef.current = k.chatId;
    setText(composerDrafts.get(k.chatId) || '');
  }, [k.chatId]);

  // The Composer itself unmounts on navigation (Keeper screen ↔ page rail tabs,
  // or the rail's Chat tab closing) — stash on the way out so it survives that too.
  useEffect(() => () => {
    const id = chatIdRef.current;
    if (!id) return;
    if (textRef.current.trim()) composerDrafts.set(id, textRef.current); else composerDrafts.delete(id);
  }, []);

  // One-shot prefill (e.g. "Ask Keeper about this" in the Explorer): consume
  // store.keeper.draft into the local text, never overwriting typed input.
  useEffect(() => {
    if (!k.draft) return;
    setText((t) => t || k.draft);
    patchKeeper({ draft: null });
    taRef.current?.focus();
  }, [k.draft]);

  const send = () => {
    const t = text.trim();
    if ((!t && !images.length) || busy) return;
    // @Title is display sugar — the Keeper speaks [[wikilinks]]. Rewrite the
    // titles the user actually picked (longest first, so overlaps don't clash).
    let out = t;
    [...mentions].sort((a, b) => b.length - a.length)
      .forEach((m) => { out = out.split(`@${m}`).join(`[[${m}]]`); });
    const imgs = images;
    if (k.chatId) composerDrafts.delete(k.chatId);
    setText(''); setImages([]); setPicker(null); setMentions([]);
    sendMessage(out, imgs);
  };

  function onPaste(e) {
    const items = [...(e.clipboardData?.items || [])].filter((it) => it.type.startsWith('image/'));
    if (!items.length) return;
    e.preventDefault();
    items.forEach((it) => {
      const file = it.getAsFile();
      if (!file) return;
      if (file.size > MAX_IMAGE_BYTES) { setOp('Image too large (max 8 MB).', 'error'); return; }
      downscaleImage(file)
        .then((im) => setImages((prev) => [...prev, im]))
        .catch(() => setOp('Could not read that image.', 'error'));
    });
  }

  // @ at a word boundary opens the same picker the + button uses (above the
  // composer, always visible). [[ typed by hand still works — the backend
  // resolves wikilinks, so Obsidian muscle memory needs no UI.
  function onInput(e) {
    const ta = e.target;
    setText(ta.value);
    autoGrow(ta);
    const caret = ta.selectionStart;
    const prev = caret >= 2 ? ta.value[caret - 2] : '';
    if (ta.value[caret - 1] === '@' && (caret === 1 || /\s/.test(prev))) {
      setLinkAt(caret - 1);
      setPicker('link');
    }
    // /command: only while the whole composer is the slash token (no space yet).
    const m = /^\/([\w-]*)$/.exec(ta.value);
    if (m) setSlash({ q: m[1].toLowerCase(), index: 0 });
    else if (slash) setSlash(null);
  }

  // Pick from /command → run a built-in command, or seed a skill directive.
  function pickItem(item) {
    if (item.command) { setSlash(null); setText(''); item.run(); return; }
    pickSkill(item);
  }

  // Tab: autocomplete the token to the full /slug, keep the menu open (Enter
  // then runs it). Never fires the command.
  function completeItem(item) {
    const t = `/${item.slug}`;
    setText(t);
    setSlash({ q: item.slug.toLowerCase(), index: 0 });
    requestAnimationFrame(() => {
      const ta = taRef.current;
      if (ta) { ta.focus(); ta.setSelectionRange(t.length, t.length); autoGrow(ta); }
    });
  }

  // Pick a skill from /command → replace the composer with a directive that
  // makes the Keeper pull it. User can keep typing or send as-is.
  function pickSkill(skill) {
    setSlash(null);
    const dir = skillDirective(skill, onPage);
    setText(dir);
    requestAnimationFrame(() => {
      const ta = taRef.current;
      if (ta) { ta.focus(); ta.setSelectionRange(dir.length, dir.length); autoGrow(ta); }
    });
  }

  // Replace the trigger @ with the visible @Title; rewritten to [[Title]] on send.
  function insertLink(p) {
    const ta = taRef.current;
    const title = (p.title || '').trim();
    setPicker(null);
    if (!ta || !title) return;
    const v = ta.value;
    const at = linkAt >= 0 && v[linkAt] === '@' ? linkAt : v.length;
    const next = v.slice(0, at) + `@${title}` + v.slice(v[at] === '@' ? at + 1 : at);
    setText(next);
    setMentions((m) => (m.includes(title) ? m : [...m, title]));
    const caret = at + title.length + 1;
    requestAnimationFrame(() => { if (taRef.current) { taRef.current.focus(); taRef.current.setSelectionRange(caret, caret); autoGrow(taRef.current); } });
  }

  function onKeyDown(e) {
    if (slash && slashItems.length) {
      const at = Math.min(slash.index, slashItems.length - 1);
      if (e.key === 'ArrowDown') { e.preventDefault(); setSlash({ ...slash, index: (at + 1) % slashItems.length }); return; }
      if (e.key === 'ArrowUp') { e.preventDefault(); setSlash({ ...slash, index: (at - 1 + slashItems.length) % slashItems.length }); return; }
      if (e.key === 'Tab') { e.preventDefault(); completeItem(slashItems[at]); return; }
      if (e.key === 'Enter') { e.preventDefault(); pickItem(slashItems[at]); return; }
      if (e.key === 'Escape') { e.preventDefault(); setSlash(null); return; }
    }
    if (e.key === 'Escape' && picker) { e.preventDefault(); setPicker(null); return; }
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); }
  }

  return html`<div style=${{ borderTop: '1px solid var(--rule)', position: 'relative' }}>
    <${AttachChips} attachments=${k.attachments} />
    ${picker && html`<${AttachPicker} onClose=${() => setPicker(null)} onPickPage=${picker === 'link' ? insertLink : null} />`}
    ${slash && html`<div style=${{
      position: 'absolute', bottom: 'calc(100% + 6px)', left: 10, right: 10, zIndex: 5,
      background: 'var(--paper)', border: '1px solid var(--rule)', borderRadius: 8,
      boxShadow: 'var(--shadow-raised)', maxHeight: 280, overflow: 'auto', padding: 6,
    }}>
      ${slashItems.map((s, i) => html`
        ${i === 0 && slashCmds.length ? html`<div key="lc" style=${slashLabel}>Commands</div>` : ''}
        ${i === slashCmds.length && slashSkills.length ? html`<div key="ls" style=${slashLabel}>Skills</div>` : ''}
        <div key=${s.slug} onClick=${() => pickItem(s)} onMouseEnter=${() => setSlash({ ...slash, index: i })}
          class=${`ck-ac-item${i === Math.min(slash.index, slashItems.length - 1) ? ' on' : ''}`} style=${{ ...pickRow, display: 'block' }}>
          <div style=${{ display: 'flex', alignItems: 'center', gap: 7 }}>
            <${Icon} name=${s.command ? 'sparkle' : 'feather'} size=${12} />
            <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 12.5, color: 'var(--ink)' }}>/${s.slug}</span>
            <span style=${{ fontSize: 11.5, color: 'var(--ink-muted)' }}>${s.name}</span>
          </div>
          ${s.description && html`<div style=${{ fontSize: 11, color: 'var(--ink-faint)', marginLeft: 19, marginTop: 1, whiteSpace: 'normal' }}>${s.description}</div>`}
        </div>`)}
      ${!slashItems.length && html`<div style=${{ padding: 8, fontSize: 12, color: 'var(--ink-faint)' }}>No command or skill matches.</div>`}
    </div>`}
    ${images.length > 0 && html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 6, padding: '0 10px 6px' }}>
      ${images.map((img, i) => html`<div key=${i} style=${{ position: 'relative', width: 52, height: 52, borderRadius: 6, overflow: 'hidden', border: '1px solid var(--rule)' }}>
        <img src=${img.url} alt="pasted" style=${{ width: '100%', height: '100%', objectFit: 'cover', display: 'block' }} />
        <span onClick=${() => setImages(images.filter((_, j) => j !== i))} title="Remove"
          style=${{ position: 'absolute', top: 2, right: 2, width: 16, height: 16, borderRadius: 999, background: 'rgba(0,0,0,.6)', color: '#fff', display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer' }}><${Icon} name="x" size=${9} /></span>
      </div>`)}
    </div>`}
    <div class=${compacting ? 'ck-breathe' : ''} style=${{ margin: 10, border: '1px solid var(--rule)', borderRadius: 10, background: 'var(--surface)', display: 'flex', flexDirection: 'column' }}>
      <textarea ref=${taRef} value=${text} disabled=${compacting}
        placeholder=${compacting ? 'Compacting conversation…' : 'Ask the Keeper… (@ link a page, / a skill, paste images)'} rows=${1}
        onInput=${onInput}
        onKeyDown=${onKeyDown}
        onPaste=${onPaste}
        style=${{ resize: 'none', fontSize: 13, padding: '9px 10px 4px', border: 'none', outline: 'none', background: 'transparent', color: 'var(--ink)', fontFamily: 'inherit', minHeight: 40, maxHeight: 168, overflowY: 'auto' }} />
      ${compacting
        ? html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 10px 8px' }}>
            <${Spinner} size=${14} />
            <span class="ck-shimmer-text" style=${{ fontSize: 12.5, fontWeight: 500 }}>Compacting conversation…</span>
          </div>`
        : dictation.status === 'recording' || dictation.status === 'transcribing'
        ? html`<div style=${{ display: 'flex', alignItems: 'center', gap: 6, padding: '4px 6px 6px' }}>
            <button class="btn btn-ghost on" title="Recording" style=${{ padding: '6px 7px', color: 'var(--burgundy)' }}><${Icon} name="mic" size=${14} /></button>
            ${dictation.status === 'recording'
              ? html`<${Waveform} analyser=${dictation.analyser} />`
              : html`<div style=${{ flex: 1, fontSize: 12, color: 'var(--ink-faint)', paddingLeft: 4 }}>Transcribing…</div>`}
            <button class="btn btn-ghost" title="Discard" onClick=${dictation.cancel} disabled=${dictation.status === 'transcribing'}
              style=${{ padding: '6px 7px' }}><${Icon} name="x" size=${14} /></button>
            ${dictation.status === 'recording'
              ? html`<button class="btn btn-primary" title="Stop & transcribe" onClick=${dictation.stop} style=${{ padding: '6px 8px' }}><${Icon} name="check" size=${14} /></button>`
              : html`<button class="btn btn-primary" disabled style=${{ padding: '6px 8px' }}><${Spinner} size=${14} /></button>`}
          </div>`
        : html`<div style=${{ display: 'flex', alignItems: 'center', gap: 6, padding: '4px 6px 6px' }}>
            <button class="btn btn-ghost" title="Attach a page, session or file" onClick=${() => setPicker(picker === 'attach' ? null : 'attach')}
              style=${{ padding: '6px 7px' }}><${Icon} name="plus" size=${14} /></button>
            <${PickerControls} k=${k} />
            <button class="btn btn-ghost" title="Dictate" disabled=${busy} onClick=${dictation.start}
              style=${{ padding: '6px 7px' }}><${Icon} name="mic" size=${14} /></button>
            ${busy
              ? html`<button class="btn" onClick=${() => abortRun()} title="Stop the Keeper" style=${{ padding: '6px 7px', marginLeft: 'auto' }}><${Icon} name="x" size=${14} /></button>`
              : html`<button class="btn btn-primary" onClick=${send} title="Send (Enter)" disabled=${!text.trim() && !images.length}
                  style=${{ padding: '6px 8px', marginLeft: 'auto' }}><${Icon} name="arrow-r" size=${14} /></button>`}
          </div>`}
    </div>
  </div>`;
}

export function Transcript({ k, empty }) {
  const ref = useRef(null);
  const pinned = useRef(true);
  const onScroll = () => {
    const el = ref.current;
    if (el) pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
  };
  useEffect(() => {
    if (ref.current && pinned.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [k.events.length, k.live?.text, k.live?.tools?.length, k.live?.ask]);
  const isEmpty = !k.events.length && !k.live;
  return html`<div ref=${ref} onScroll=${onScroll} style=${{ flex: 1, overflow: 'auto', padding: '6px 14px' }}>
    ${isEmpty && (empty || html`<div style=${{ color: 'var(--ink-faint)', fontSize: 13, padding: '24px 8px', textAlign: 'center', lineHeight: 1.6 }}>
      The Keeper knows this world's Codex and sessions.<br />Ask about people, places, or what happened.
    </div>`)}
    ${k.events.map((ev, i) => html`<${EventRow} key=${i} ev=${ev} />`)}
    ${k.live && k.live.compacting && html`<${CompactingIndicator} />`}
    ${k.live && !k.live.compacting && html`
      ${k.live.tools.map((t, i) => html`<${ToolRow} key=${`t${i}`} ...${t} />`)}
      ${k.live.text && html`<div class="ck-prose" style=${{ fontSize: 13, margin: '10px 0' }}
        dangerouslySetInnerHTML=${{ __html: renderBlockHtml(k.live.text, store.vaultPages) }} />`}
      ${k.live.ask && html`<${PermissionCard} ask=${k.live.ask} />`}
      ${!k.live.text && !k.live.ask && !k.live.tools.length && html`<div style=${{ padding: '8px 0' }}><${Spinner} size=${14} /></div>`}
    `}
    ${k.error && html`<div style=${{ margin: '8px 0', fontSize: 12, color: 'var(--burgundy-700)' }}>⚠ ${k.error}</div>`}
  </div>`;
}

// Suggestion chips: on a page of kind X, the skills that fit it (kinds: match,
// zero inference). Click seeds the composer with a pull directive. Pure
// discovery — nothing fires until the user sends.
function SkillChips() {
  const skills = skillsForKind(currentPageKind());
  if (!skills.length) return null;
  return html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 6, padding: '6px 12px', borderTop: '1px solid var(--rule-soft)' }}>
    ${skills.map((s) => html`<button key=${s.slug} class="btn btn-ghost" title=${s.description}
      onClick=${() => patchKeeper({ draft: skillDirective(s, true) })}
      style=${{ fontSize: 11.5, padding: '3px 9px', borderRadius: 999, border: '1px solid var(--rule)', color: 'var(--ink-muted)', display: 'flex', alignItems: 'center', gap: 5 }}>
      <${Icon} name="feather" size=${11} /> ${s.name}
    </button>`)}
  </div>`;
}

/// Shared conversation column: transcript + composer + drop-to-attach overlay.
export function Conversation({ k, empty }) {
  const { dragging, bind } = useDropAttachments();
  const noProvider = !configuredProviders().length;
  return html`<div style=${{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0, position: 'relative' }} ...${bind}>
    ${noProvider && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 12px', borderBottom: '1px solid var(--rule-soft)', background: 'var(--paper-deep)', fontSize: 12.5, color: 'var(--ink-muted)' }}>
      <${Icon} name="feather" size=${13} />
      <span style=${{ flex: 1 }}>The Keeper needs a language model. Set one up in Settings — Ollama is free and runs locally.</span>
      <button class="btn" onClick=${() => navigate('settings')}>Open Settings</button>
    </div>`}
    <${Transcript} k=${k} empty=${empty} />
    ${k.undoable > 0 && !k.live && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 7, padding: '4px 12px', borderTop: '1px solid var(--rule-soft)', fontSize: 12, color: 'var(--ink-muted)' }}>
      <span style=${{ flex: 1 }}>The Keeper changed ${k.undoable} ${k.undoable === 1 ? 'file' : 'files'} in this chat.</span>
      <button class="btn" onClick=${undoLast} title="Revert the Keeper's most recent change">
        <${Icon} name="undo" size=${12} /> Undo last change
      </button>
    </div>`}
    ${!k.live && html`<${SkillChips} />`}
    <${Composer} busy=${!!k.live} compacting=${!!k.live?.compacting} />
    ${dragging && html`<div style=${{
      position: 'absolute', inset: 0, zIndex: 8, display: 'flex', alignItems: 'center', justifyContent: 'center',
      background: 'rgba(122,46,31,.08)', border: '2px dashed var(--burgundy)', borderRadius: 8,
      color: 'var(--burgundy-700)', fontSize: 14, fontWeight: 600, pointerEvents: 'none',
    }}>Drop text files to attach</div>`}
  </div>`;
}

export function ModeSelect({ mode }) {
  return html`<select class="select" value=${mode} onChange=${(e) => setMode(e.target.value)}
    title="What the Keeper may do without asking" style=${pickSelect}>
    ${MODES.map((m) => html`<option key=${m.id} value=${m.id}>${m.label}</option>`)}
  </select>`;
}
