// The Keeper screen — the home for every chat. Two-pane: chat list (left) +
// the full conversation (right), reusing the docked panel's Conversation over
// the same store.keeper state (ask-the-keeper-ux-spec.md).
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, apiFetch, apiJson, bump, fmtDate } from '../core.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Spinner } from '../ui.js';
import {
  keeperState, patchKeeper, openChat, newChat, ModeSelect, Conversation, sendMessage,
} from '../keeperPanel.js';
import { MemoryView, fetchBriefStatus } from '../keeperMemory.js';

const SUGGESTIONS = [
  'Summarize what happened in the last session',
  'Which NPCs have we met but never written up?',
  'Find pages that mention the docks',
  'Draft a page for a place we visited',
];

export function KeeperScreen({ store }) {
  const c = store.campaign;
  const cid = c?.campaign_id;
  const [chats, setChats] = useState(null);
  const [q, setQ] = useState('');
  const [view, setView] = useState('chat'); // 'chat' | 'memory'
  const [brief, setBrief] = useState(null);
  const k = keeperState();

  useEffect(() => { if (cid) fetchBriefStatus(cid).then(setBrief); }, [cid, view, store.dirty_keeper]);

  async function reload(select) {
    if (!cid) return;
    try {
      const { chats: list } = await apiFetch(`/campaigns/${cid}/agent/chats`);
      setChats(list);
      if (select === 'first' && list[0] && !keeperState().chatId) openChat(list[0].id);
    } catch (_) { setChats([]); }
  }

  useEffect(() => { reload('first'); }, [cid, store.dirty_keeper]);

  if (!c) return html`<div />`;

  const onNew = async () => { setView('chat'); await newChat(); };
  const pickChat = (id) => { setView('chat'); openChat(id); };
  const onDelete = async (id, e) => {
    e.stopPropagation();
    try {
      await apiJson(`/campaigns/${cid}/agent/chats/${id}`, 'DELETE', {});
      if (keeperState().chatId === id) {
        const next = (chats || []).find((x) => x.id !== id);
        if (next) { openChat(next.id); }
        else { patchKeeper({ chatId: null, events: [], live: null }); newChat(); }
      }
      bump('keeper');
    } catch (_) {}
  };

  const filtered = (chats || []).filter((ch) => !q.trim() || (ch.title || '').toLowerCase().includes(q.toLowerCase()));

  const list = html`<div style=${{ width: 280, flex: '0 0 280px', borderRight: '1px solid var(--rule)', display: 'flex', flexDirection: 'column', minHeight: 0, background: 'var(--paper-deep)' }}>
    <div style=${{ padding: '12px 12px 8px', borderBottom: '1px solid var(--rule-soft)' }}>
      <button class="btn btn-primary" style=${{ width: '100%', justifyContent: 'center' }} onClick=${onNew}>
        <${Icon} name="plus" size=${13} /> New chat
      </button>
      <input value=${q} placeholder="Search chats…" onInput=${(e) => setQ(e.target.value)}
        style=${{ width: '100%', boxSizing: 'border-box', marginTop: 8, fontSize: 12.5, padding: '6px 8px', borderRadius: 5, border: '1px solid var(--rule)', background: 'var(--surface)', color: 'var(--ink)' }} />
    </div>
    <div style=${{ flex: 1, overflow: 'auto', padding: 6 }}>
      ${chats === null && html`<div style=${{ padding: 16, textAlign: 'center' }}><${Spinner} size=${14} /></div>`}
      ${chats !== null && !filtered.length && html`<div style=${{ padding: 16, fontSize: 12.5, color: 'var(--ink-faint)', textAlign: 'center' }}>No chats yet.</div>`}
      ${filtered.map((ch) => {
        const active = view === 'chat' && ch.id === k.chatId;
        return html`<div key=${ch.id} onClick=${() => pickChat(ch.id)} class="ck-chat-row" style=${{
          padding: '9px 10px', borderRadius: 6, cursor: 'pointer', marginBottom: 2,
          background: active ? 'var(--burgundy-50)' : 'transparent',
          border: `1px solid ${active ? 'var(--rule-soft)' : 'transparent'}`,
          display: 'flex', alignItems: 'center', gap: 8,
        }}>
          <div style=${{ flex: 1, minWidth: 0 }}>
            <div style=${{ fontSize: 13, fontWeight: active ? 600 : 500, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${ch.title}</div>
            <div style=${{ fontSize: 11, color: 'var(--ink-faint)', marginTop: 1 }}>${ch.message_count} msg · ${fmtDate(ch.updated_at) || 'new'}</div>
          </div>
          <span onClick=${(e) => onDelete(ch.id, e)} title="Delete chat" style=${{ color: 'var(--ink-faint)', display: 'flex', padding: 2 }}><${Icon} name="trash" size=${12} /></span>
        </div>`;
      })}
    </div>
    <div style=${{ borderTop: '1px solid var(--rule-soft)', padding: 8 }}>
      <div onClick=${() => setView('memory')} class="ck-chat-row" style=${{
        display: 'flex', alignItems: 'center', gap: 8, padding: '9px 10px', borderRadius: 6, cursor: 'pointer',
        background: view === 'memory' ? 'var(--burgundy-50)' : 'transparent',
        border: `1px solid ${view === 'memory' ? 'var(--rule-soft)' : 'transparent'}`,
      }}>
        <${Icon} name="book" size=${14} />
        <span style=${{ flex: 1, fontSize: 13, fontWeight: view === 'memory' ? 600 : 500, color: 'var(--ink)' }}>Memory & Brief</span>
        ${brief && (!brief.exists || brief.stale) && html`<span title=${brief.exists ? 'Brief is out of date' : 'No brief yet'} style=${{ width: 7, height: 7, borderRadius: 999, background: 'var(--burgundy)' }} />`}
      </div>
    </div>
  </div>`;

  const emptyState = html`<div style=${{ color: 'var(--ink-faint)', fontSize: 13, padding: '36px 16px', textAlign: 'center', lineHeight: 1.7, maxWidth: 460, margin: '0 auto' }}>
    <div style=${{ fontFamily: 'var(--font-display)', fontSize: 18, color: 'var(--ink)', marginBottom: 6 }}>Ask the Keeper</div>
    The resident AI of this world — it knows your Codex and sessions, and can draft or reorganise pages with your approval.
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 6, marginTop: 16 }}>
      ${brief && !brief.exists && html`<button class="btn btn-primary" style=${{ justifyContent: 'flex-start' }} onClick=${() => setView('memory')}>
        <${Icon} name="book" size=${13} /> Let the Keeper read up on this world
      </button>`}
      ${SUGGESTIONS.map((s) => html`<button key=${s} class="btn" style=${{ justifyContent: 'flex-start' }} onClick=${() => sendSuggestion(s)}>${s}</button>`)}
    </div>
  </div>`;

  const memoryPane = html`<div style=${{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, minHeight: 0 }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 16px', borderBottom: '1px solid var(--rule)' }}>
      <${Icon} name="book" size=${15} />
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 600, flex: 1 }}>Memory & World Brief</div>
    </div>
    <${MemoryView} cid=${cid} />
  </div>`;

  const convo = k.chatId
    ? html`<div style=${{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, minHeight: 0 }}>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 16px', borderBottom: '1px solid var(--rule)' }}>
          <${Icon} name="feather" size=${15} />
          <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 600, flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
            ${(chats || []).find((x) => x.id === k.chatId)?.title || 'Chat'}
          </div>
          <${ModeSelect} mode=${k.mode} />
        </div>
        <${Conversation} k=${k} empty=${emptyState} />
      </div>`
    : html`<div style=${{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>${emptyState}</div>`;

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="campaign" active="keeper" campaign=${c} />`}
    topbar=${html`<${Topbar} crumbs=${[{ label: 'Worlds', onClick: () => navigate('library') }, c.name, 'The Keeper']} />`}
    bodyStyle=${{ padding: 0 }}
  >
    <div style=${{ display: 'flex', height: '100%', minHeight: 0 }}>
      ${list}
      ${view === 'memory' ? memoryPane : convo}
    </div>
  </${Shell}>`;
}

async function sendSuggestion(text) {
  if (!keeperState().chatId) await newChat();
  sendMessage(text);
}
