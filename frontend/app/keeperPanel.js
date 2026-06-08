// The Keeper docked panel: floating pill on every in-world screen,
// right-docked chat over the current screen. 6.3: permission mode selector,
// diff-approval cards, undo. Keeper screen + attachments land in 6.4.
import { html, useState, useEffect, useRef } from '../vendor/htm-preact-standalone.mjs';
import { apiFetch, apiJson, apiStream, setOp, setState, store } from './core.js';
import { Icon, Spinner, renderBlockHtml, wikilinkClick } from './ui.js';

// store.keeper = { open, chatId, events[], live: {text, tools[], ask}|null, error, mode }

const MODES = [
  { id: 'read_only', label: 'Read-only' },
  { id: 'ask', label: 'Ask' },
  { id: 'accept_edits', label: 'Accept edits' },
];

function keeperState() {
  const k = store.keeper || { open: false, chatId: null, events: [], live: null, error: null };
  return k.mode ? k : { ...k, mode: localStorage.getItem('ck_keeper_mode') || 'ask' };
}

function patchKeeper(patch) {
  setState({ keeper: { ...keeperState(), ...patch } });
}

async function openPanel() {
  const cid = store.campaign?.campaign_id;
  if (!cid) return;
  // Chats are per-world — a stale chat id from another world must not leak.
  if (keeperState().campaignId !== cid) {
    patchKeeper({ campaignId: cid, chatId: null, events: [], live: null });
  }
  patchKeeper({ open: true, error: null });
  const k = keeperState();
  if (k.chatId) return;
  try {
    const { chats } = await apiFetch(`/campaigns/${cid}/agent/chats`);
    const chat = chats[0] || (await apiJson(`/campaigns/${cid}/agent/chats`, 'POST', {}));
    const { events } = await apiFetch(`/campaigns/${cid}/agent/chats/${chat.id}`);
    patchKeeper({ chatId: chat.id, events });
  } catch (e) {
    patchKeeper({ error: String(e.message || e) });
  }
}

async function newChat() {
  const cid = store.campaign?.campaign_id;
  if (!cid) return;
  try {
    const chat = await apiJson(`/campaigns/${cid}/agent/chats`, 'POST', {});
    patchKeeper({ chatId: chat.id, events: [], live: null, error: null });
  } catch (e) {
    patchKeeper({ error: String(e.message || e) });
  }
}

async function sendMessage(text) {
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (!cid || !k.chatId || k.live) return;
  const events = [...k.events, { type: 'user', text }];
  patchKeeper({ events, live: { text: '', tools: [] }, error: null });
  try {
    await apiStream(`/campaigns/${cid}/agent/chats/${k.chatId}/messages`, { text, mode: k.mode }, (ev) => {
      const cur = keeperState();
      const live = cur.live || { text: '', tools: [] };
      if (ev.type === 'text_delta') {
        patchKeeper({ live: { ...live, text: live.text + ev.text } });
      } else if (ev.type === 'permission_request') {
        patchKeeper({ live: { ...live, ask: { requestId: ev.request_id, name: ev.name, diff: ev.diff } } });
      } else if (ev.type === 'tool_start') {
        patchKeeper({ live: { ...live, ask: null, tools: [...live.tools, { name: ev.name, args: ev.args_summary, diff: ev.diff, running: true }] } });
      } else if (ev.type === 'tool_result') {
        const tools = live.tools.slice();
        const i = tools.findLastIndex((t) => t.running && t.name === ev.name);
        if (i >= 0) tools[i] = { ...tools[i], running: false, summary: ev.summary, isError: ev.is_error };
        // A tool round means the streamed text so far belongs to a finished
        // assistant turn — fold it into the row list and reset the buffer.
        patchKeeper({ live: { ...live, text: '', tools, ask: null } });
        if (live.text.trim()) {
          patchKeeper({ events: [...keeperState().events, { type: 'assistant', text: live.text }] });
        }
      } else if (ev.type === 'error') {
        patchKeeper({ error: ev.message });
      }
    });
  } catch (e) {
    patchKeeper({ error: String(e.message || e) });
  }
  // Authoritative reload: persisted jsonl is the truth for the transcript.
  try {
    const { events: fresh } = await apiFetch(`/campaigns/${cid}/agent/chats/${keeperState().chatId}`);
    patchKeeper({ events: fresh, live: null });
  } catch (_) {
    patchKeeper({ live: null });
  }
}

async function abortRun() {
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (!cid || !k.chatId) return;
  try { await apiJson(`/campaigns/${cid}/agent/chats/${k.chatId}/abort`, 'POST', {}); } catch (_) {}
}

function setMode(mode) {
  localStorage.setItem('ck_keeper_mode', mode);
  patchKeeper({ mode });
}

async function decide(requestId, decision) {
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (!cid || !k.chatId) return;
  if (k.live) patchKeeper({ live: { ...k.live, ask: null } });
  try {
    await apiJson(`/campaigns/${cid}/agent/chats/${k.chatId}/approve`, 'POST', { request_id: requestId, decision });
  } catch (e) {
    patchKeeper({ error: String(e.message || e) });
  }
}

async function undoLast() {
  const cid = store.campaign?.campaign_id;
  const k = keeperState();
  if (!cid || !k.chatId || k.live) return;
  try {
    const { restored } = await apiJson(`/campaigns/${cid}/agent/chats/${k.chatId}/undo`, 'POST', { scope: 'last' });
    setOp(restored.length ? `Restored ${restored.join(', ')}` : 'Nothing to undo', restored.length ? 'done' : '');
  } catch (e) {
    patchKeeper({ error: String(e.message || e) });
  }
}

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

const WRITE_VERB = { create_page: 'create', edit_page: 'edit', write_page: 'overwrite' };

function PermissionCard({ ask }) {
  return html`<div style=${{ margin: '10px 0', border: '1px solid var(--rule)', borderRadius: 8, background: 'var(--paper-deep)', overflow: 'hidden' }}>
    <div style=${{ padding: '8px 12px', fontSize: 12.5, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 7 }}>
      <${Icon} name="feather" size=${13} />
      The Keeper wants to ${WRITE_VERB[ask.name] || ask.name} ${' '}
      <span style=${{ fontFamily: 'var(--font-mono)', fontWeight: 500 }}>${ask.diff?.path || ''}</span>
    </div>
    <div style=${{ padding: '0 10px 8px' }}>
      ${ask.diff && html`<${DiffView} diff=${ask.diff} />`}
    </div>
    <div style=${{ display: 'flex', gap: 8, padding: '0 10px 10px' }}>
      <button class="ck-btn ck-btn--primary" onClick=${() => decide(ask.requestId, 'allow_once')}>Allow once</button>
      <button class="ck-btn" onClick=${() => decide(ask.requestId, 'allow_chat')}>Allow for this chat</button>
      <button class="ck-btn" style=${{ marginLeft: 'auto', color: 'var(--burgundy-700)' }} onClick=${() => decide(ask.requestId, 'deny')}>Deny</button>
    </div>
  </div>`;
}

function ToolRow({ name, summary, isError, running, args, diff }) {
  const [openRow, setOpenRow] = useState(false);
  const tint = isError ? 'var(--burgundy-700)' : 'var(--ink-muted)';
  const expandable = !!summary || !!diff;
  return html`<div style=${{ margin: '6px 0' }}>
    <div onClick=${() => setOpenRow(!openRow)} style=${{
      display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, color: tint,
      padding: '4px 8px', background: 'var(--paper-deep)', borderRadius: 5,
      border: '1px solid var(--rule-soft)', cursor: expandable ? 'pointer' : 'default',
    }}>
      ${running ? html`<${Spinner} size=${12} />` : html`<${Icon} name=${isError ? 'x' : 'check'} size=${12} />`}
      <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11.5 }}>${name}</span>
      <span style=${{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', color: 'var(--ink-faint)' }}>
        ${diff?.path || (running ? (args || '') : (summary || ''))}
      </span>
    </div>
    ${openRow && diff && html`<div style=${{ margin: '4px 0 0 20px' }}><${DiffView} diff=${diff} /></div>`}
    ${openRow && !diff && summary && html`<div style=${{ fontSize: 12, color: 'var(--ink-muted)', padding: '6px 10px', whiteSpace: 'pre-wrap', fontFamily: 'var(--font-mono)' }}>${summary}</div>`}
  </div>`;
}

function EventRow({ ev }) {
  if (ev.type === 'user') {
    return html`<div style=${{ margin: '10px 0', display: 'flex', justifyContent: 'flex-end' }}>
      <div style=${{ maxWidth: '85%', background: 'var(--burgundy-50)', border: '1px solid var(--rule-soft)', borderRadius: '10px 10px 2px 10px', padding: '8px 12px', fontSize: 13, whiteSpace: 'pre-wrap' }}>${ev.text}</div>
    </div>`;
  }
  if (ev.type === 'assistant' && (ev.text || '').trim()) {
    return html`<div class="ck-prose" style=${{ fontSize: 13, margin: '10px 0' }}
      onClick=${wikilinkClick()}
      dangerouslySetInnerHTML=${{ __html: renderBlockHtml(ev.text, store.vaultPages) }} />`;
  }
  if (ev.type === 'tool_result') {
    const first = (ev.content || '').split('\n').find((l) => l.trim() && !l.startsWith('Tool output') && l.trim() !== '```') || '';
    return html`<${ToolRow} name=${ev.name} summary=${first.trim()} isError=${ev.is_error} diff=${ev.diff} />`;
  }
  if (ev.type === 'permission' && ev.decision === 'deny') {
    return html`<div style=${{ margin: '8px 0', fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>Edit to ${ev.diff?.path || 'a page'} denied.</div>`;
  }
  if (ev.type === 'error') {
    return html`<div style=${{ margin: '8px 0', fontSize: 12, color: 'var(--burgundy-700)' }}>⚠ ${ev.message}</div>`;
  }
  if (ev.type === 'aborted') {
    return html`<div style=${{ margin: '8px 0', fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>Stopped.</div>`;
  }
  return null;
}

function Composer({ busy }) {
  const [text, setText] = useState('');
  const send = () => {
    const t = text.trim();
    if (!t || busy) return;
    setText('');
    sendMessage(t);
  };
  return html`<div style=${{ borderTop: '1px solid var(--rule)', padding: 10, display: 'flex', gap: 8, alignItems: 'flex-end' }}>
    <textarea
      value=${text}
      placeholder="Ask the Keeper about this world…"
      rows=${2}
      onInput=${(e) => setText(e.target.value)}
      onKeyDown=${(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } }}
      style=${{ flex: 1, resize: 'none', fontSize: 13, padding: '8px 10px', borderRadius: 6, border: '1px solid var(--rule)', background: 'var(--surface)', color: 'var(--ink)', fontFamily: 'inherit' }} />
    ${busy
      ? html`<button class="ck-btn" onClick=${abortRun} title="Stop the Keeper">Stop</button>`
      : html`<button class="ck-btn ck-btn--primary" onClick=${send} disabled=${!text.trim()}>Send</button>`}
  </div>`;
}

function Transcript({ k }) {
  const ref = useRef(null);
  useEffect(() => {
    if (ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [k.events.length, k.live?.text, k.live?.tools?.length, k.live?.ask]);
  const empty = !k.events.length && !k.live;
  return html`<div ref=${ref} style=${{ flex: 1, overflow: 'auto', padding: '6px 14px' }}>
    ${empty && html`<div style=${{ color: 'var(--ink-faint)', fontSize: 13, padding: '24px 8px', textAlign: 'center', lineHeight: 1.6 }}>
      The Keeper knows this world's Codex and sessions.<br />Ask about people, places, or what happened.
    </div>`}
    ${k.events.map((ev, i) => html`<${EventRow} key=${i} ev=${ev} />`)}
    ${k.live && html`
      ${k.live.tools.map((t, i) => html`<${ToolRow} key=${`t${i}`} ...${t} />`)}
      ${k.live.text && html`<div class="ck-prose" style=${{ fontSize: 13, margin: '10px 0' }}
        dangerouslySetInnerHTML=${{ __html: renderBlockHtml(k.live.text, store.vaultPages) }} />`}
      ${k.live.ask && html`<${PermissionCard} ask=${k.live.ask} />`}
      ${!k.live.text && !k.live.ask && !k.live.tools.some((t) => t.running) && html`<div style=${{ padding: '8px 0' }}><${Spinner} size=${14} /></div>`}
    `}
    ${k.error && html`<div style=${{ margin: '8px 0', fontSize: 12, color: 'var(--burgundy-700)' }}>⚠ ${k.error}</div>`}
  </div>`;
}

export function KeeperDock() {
  const inWorld = !!store.campaign?.campaign_id
    && !['library', 'settings', 'newWorld'].includes(store.route.name);
  const k = keeperState();

  useEffect(() => {
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'j') {
        e.preventDefault();
        keeperState().open ? patchKeeper({ open: false }) : openPanel();
      } else if (e.key === 'Escape' && keeperState().open) {
        patchKeeper({ open: false });
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  if (!inWorld) return null;
  if (!k.open) {
    return html`<button onClick=${openPanel} title="Ask the Keeper (⌘J)" style=${{
      position: 'fixed', right: 22, bottom: 22, zIndex: 60,
      display: 'flex', alignItems: 'center', gap: 8, padding: '10px 16px',
      background: 'var(--burgundy)', color: '#FBF7EF', border: 'none', borderRadius: 999,
      fontSize: 13, fontWeight: 600, cursor: 'pointer', boxShadow: 'var(--shadow-raised)',
    }}>
      <${Icon} name="feather" size=${15} /> Ask the Keeper
    </button>`;
  }

  return html`<div style=${{
    position: 'fixed', top: 0, right: 0, bottom: 0, width: 420, zIndex: 60,
    background: 'var(--paper)', borderLeft: '1px solid var(--rule)',
    boxShadow: '-8px 0 24px rgba(60,40,20,.12)', display: 'flex', flexDirection: 'column',
  }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 14px', borderBottom: '1px solid var(--rule)' }}>
      <${Icon} name="feather" size=${15} />
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 14, fontWeight: 600, flex: 1 }}>The Keeper</div>
      <select value=${k.mode} onChange=${(e) => setMode(e.target.value)}
        title="What the Keeper may do without asking"
        style=${{ fontSize: 11.5, padding: '3px 4px', borderRadius: 5, border: '1px solid var(--rule)', background: 'var(--surface)', color: 'var(--ink-muted)' }}>
        ${MODES.map((m) => html`<option key=${m.id} value=${m.id}>${m.label}</option>`)}
      </select>
      <button class="ck-btn" title="Undo the Keeper's last edit in this chat" onClick=${undoLast} disabled=${!!k.live}>Undo</button>
      <button class="ck-btn" title="New chat" onClick=${newChat}>New</button>
      <button onClick=${() => patchKeeper({ open: false })} title="Close (Esc)"
        style=${{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ink-muted)', display: 'flex' }}>
        <${Icon} name="x" size=${15} />
      </button>
    </div>
    <${Transcript} k=${k} />
    <${Composer} busy=${!!k.live} />
  </div>`;
}
