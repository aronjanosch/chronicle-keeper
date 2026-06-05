// The Keeper docked panel (Phase 6.2 minimal cut): floating pill on every
// in-world screen, right-docked chat over the current screen. Keeper screen,
// permission cards, attachments land in later sprints.
import { html, useState, useEffect, useRef } from '../vendor/htm-preact-standalone.mjs';
import { apiFetch, apiJson, apiStream, setState, store } from './core.js';
import { Icon, Spinner, renderBlockHtml, wikilinkClick } from './ui.js';

// store.keeper = { open, chatId, events[], live: {text, tools[]}|null, error }

function keeperState() {
  return store.keeper || { open: false, chatId: null, events: [], live: null, error: null };
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
    await apiStream(`/campaigns/${cid}/agent/chats/${k.chatId}/messages`, { text }, (ev) => {
      const cur = keeperState();
      const live = cur.live || { text: '', tools: [] };
      if (ev.type === 'text_delta') {
        patchKeeper({ live: { ...live, text: live.text + ev.text } });
      } else if (ev.type === 'tool_start') {
        patchKeeper({ live: { ...live, tools: [...live.tools, { name: ev.name, args: ev.args_summary, running: true }] } });
      } else if (ev.type === 'tool_result') {
        const tools = live.tools.slice();
        const i = tools.findLastIndex((t) => t.running && t.name === ev.name);
        if (i >= 0) tools[i] = { ...tools[i], running: false, summary: ev.summary, isError: ev.is_error };
        // A tool round means the streamed text so far belongs to a finished
        // assistant turn — fold it into the row list and reset the buffer.
        patchKeeper({ live: { text: '', tools } });
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

function ToolRow({ name, summary, isError, running, args }) {
  const [openRow, setOpenRow] = useState(false);
  const tint = isError ? 'var(--burgundy-700)' : 'var(--ink-muted)';
  return html`<div style=${{ margin: '6px 0' }}>
    <div onClick=${() => setOpenRow(!openRow)} style=${{
      display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, color: tint,
      padding: '4px 8px', background: 'var(--paper-deep)', borderRadius: 5,
      border: '1px solid var(--rule-soft)', cursor: summary ? 'pointer' : 'default',
    }}>
      ${running ? html`<${Spinner} size=${12} />` : html`<${Icon} name=${isError ? 'x' : 'check'} size=${12} />`}
      <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11.5 }}>${name}</span>
      <span style=${{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', color: 'var(--ink-faint)' }}>
        ${running ? (args || '') : (summary || '')}
      </span>
    </div>
    ${openRow && summary && html`<div style=${{ fontSize: 12, color: 'var(--ink-muted)', padding: '6px 10px', whiteSpace: 'pre-wrap', fontFamily: 'var(--font-mono)' }}>${summary}</div>`}
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
    return html`<${ToolRow} name=${ev.name} summary=${first.trim()} isError=${ev.is_error} />`;
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
  }, [k.events.length, k.live?.text, k.live?.tools?.length]);
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
      ${!k.live.text && !k.live.tools.some((t) => t.running) && html`<div style=${{ padding: '8px 0' }}><${Spinner} size=${14} /></div>`}
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
