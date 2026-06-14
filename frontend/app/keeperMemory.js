// The Keeper's memory & World Brief view (keeper-memory-spec.md /
// keeper-context-spec.md), shown in the Keeper screen's right pane. The brief
// init run streams like a chat turn (tool rows visible); memories are the
// fact files the Keeper maintains across chats.
import { html, useState, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { apiFetch, apiJson, apiStream, bump, setOp, store } from './core.js';
import { Icon, Spinner, renderBlockHtml, wikilinkClick } from './ui.js';

const TYPE_LABEL = { preference: 'preference', task: 'task', style: 'style', correction: 'correction' };

export async function fetchBriefStatus(cid) {
  try { return await apiFetch(`/campaigns/${cid}/agent/brief`); } catch (_) { return null; }
}

export function MemoryView({ cid }) {
  const [brief, setBrief] = useState(null);
  const [memories, setMemories] = useState(null);
  const [run, setRun] = useState(null); // {text, tools[]} while the init run streams
  const [open, setOpen] = useState({}); // memory bodies expanded by name

  async function reload() {
    const [b, m] = await Promise.all([
      fetchBriefStatus(cid),
      apiFetch(`/campaigns/${cid}/agent/memory`).catch(() => ({ memories: [] })),
    ]);
    setBrief(b);
    setMemories(m.memories || []);
  }
  useEffect(() => { reload(); }, [cid, store.dirty_keeper]);

  async function regenerate() {
    if (run) return;
    setRun({ text: '', tools: [] });
    try {
      await apiStream(`/campaigns/${cid}/agent/brief`, {}, (ev) => {
        setRun((cur) => {
          const c = cur || { text: '', tools: [] };
          if (ev.type === 'text_delta') return { ...c, text: c.text + ev.text };
          if (ev.type === 'tool_start') return { ...c, tools: [...c.tools, { name: ev.name, running: true }] };
          if (ev.type === 'tool_result') {
            const tools = c.tools.slice();
            const i = tools.findLastIndex((t) => t.running && t.name === ev.name);
            if (i >= 0) tools[i] = { ...tools[i], running: false, isError: ev.is_error };
            return { ...c, tools };
          }
          if (ev.type === 'error') { setOp(ev.message, 'error'); return c; }
          return c;
        });
      });
    } catch (e) {
      setOp(String(e.message || e), 'error');
    }
    setRun(null);
    bump('keeper'); // reload here + the brief dot in the chat list
  }

  async function forget(name) {
    try {
      await apiJson(`/campaigns/${cid}/agent/memory/${encodeURIComponent(name)}`, 'DELETE', {});
      setMemories((ms) => (ms || []).filter((m) => m.name !== name));
    } catch (e) { setOp(String(e.message || e), 'error'); }
  }

  return html`<div style=${{ flex: 1, overflow: 'auto', padding: '20px 24px', minWidth: 0 }}>
    <div style=${{ maxWidth: 760, margin: '0 auto' }}>
      ${briefCard({ brief, run, regenerate })}
      ${memoryCard({ memories, open, setOpen, forget })}
    </div>
  </div>`;
}

function briefCard({ brief, run, regenerate }) {
  const head = html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
    <${Icon} name="book" size=${15} />
    <div style=${{ fontFamily: 'var(--font-display)', fontSize: 16, fontWeight: 600, flex: 1 }}>World Brief</div>
    <button class="ck-btn" onClick=${regenerate} disabled=${!!run}>
      ${run ? 'Reading…' : (brief?.exists ? 'Regenerate' : 'Read up on this world')}
    </button>
  </div>`;

  let body;
  if (run) {
    body = html`<div>
      ${run.tools.map((t, i) => html`<div key=${i} style=${{ display: 'flex', alignItems: 'center', gap: 7, fontSize: 12, color: 'var(--ink-muted)', padding: '3px 0' }}>
        ${t.running ? html`<${Spinner} size=${11} />` : html`<${Icon} name=${t.isError ? 'x' : 'check'} size=${11} />`}
        <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11.5 }}>${t.name}</span>
      </div>`)}
      ${run.text && html`<div class="ck-prose" style=${{ fontSize: 13, marginTop: 8 }} dangerouslySetInnerHTML=${{ __html: renderBlockHtml(run.text, store.vaultPages) }} />`}
      ${!run.text && !run.tools.length && html`<${Spinner} size=${14} />`}
    </div>`;
  } else if (brief?.exists) {
    body = html`<div>
      ${brief.stale && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12.5, color: 'var(--burgundy-700)', background: 'var(--burgundy-50)', border: '1px solid var(--rule-soft)', borderRadius: 6, padding: '8px 10px', marginBottom: 10 }}>
        <${Icon} name="sparkle" size=${13} /> The world has moved on since this brief — regenerate to refresh it.
      </div>`}
      <div class="ck-prose" style=${{ fontSize: 13 }} onClick=${wikilinkClick()} dangerouslySetInnerHTML=${{ __html: renderBlockHtml(brief.body || '', store.vaultPages) }} />
      <div style=${{ fontSize: 11, color: 'var(--ink-faint)', marginTop: 10 }}>
        Saw ${brief.sessions_seen} session${brief.sessions_seen === 1 ? '' : 's'}, ${brief.pages_seen} page${brief.pages_seen === 1 ? '' : 's'}.
      </div>
    </div>`;
  } else {
    body = html`<div style=${{ fontSize: 13, color: 'var(--ink-faint)', lineHeight: 1.6 }}>
      No brief yet. Let the Keeper read your Codex and sessions and write a short reference it will keep in mind in every chat.
    </div>`;
  }

  return html`<div style=${{ border: '1px solid var(--rule)', borderRadius: 10, background: 'var(--paper-deep)', padding: 16, marginBottom: 18 }}>
    ${head}${body}
  </div>`;
}

function memoryCard({ memories, open, setOpen, forget }) {
  const head = html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
    <${Icon} name="feather" size=${15} />
    <div style=${{ fontFamily: 'var(--font-display)', fontSize: 16, fontWeight: 600, flex: 1 }}>Memory</div>
  </div>`;
  const sub = html`<div style=${{ fontSize: 12, color: 'var(--ink-faint)', marginBottom: 12 }}>
    What the Keeper has learned about how you like to work — preferences, corrections, ongoing tasks. World lore lives in Codex pages, not here.
  </div>`;

  if (memories === null) return html`<div>${head}<${Spinner} size=${14} /></div>`;
  if (!memories.length) {
    return html`<div>${head}${sub}<div style=${{ fontSize: 13, color: 'var(--ink-faint)', fontStyle: 'italic' }}>Nothing remembered yet.</div></div>`;
  }
  return html`<div>${head}${sub}
    ${memories.map((m) => {
      const isOpen = !!open[m.name];
      return html`<div key=${m.name} style=${{ border: '1px solid var(--rule-soft)', borderRadius: 8, background: 'var(--surface)', padding: '10px 12px', marginBottom: 8 }}>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 12.5, fontWeight: 600 }}>${m.name}</span>
          <span style=${{ fontSize: 10.5, textTransform: 'uppercase', letterSpacing: '.04em', color: 'var(--ink-faint)', border: '1px solid var(--rule-soft)', borderRadius: 999, padding: '1px 7px' }}>${TYPE_LABEL[m.type] || m.type || 'note'}</span>
          <span style=${{ flex: 1 }} />
          <button class="ck-btn" style=${{ padding: '3px 7px', fontSize: 11.5 }} onClick=${() => setOpen((o) => ({ ...o, [m.name]: !o[m.name] }))}>${isOpen ? 'Hide' : 'Read'}</button>
          <span onClick=${() => forget(m.name)} title="Forget" style=${{ cursor: 'pointer', color: 'var(--ink-faint)', display: 'flex', padding: 3 }}><${Icon} name="trash" size=${12} /></span>
        </div>
        <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', marginTop: 3 }}>${m.description || '(no description)'}</div>
        ${isOpen && html`<div class="ck-prose" style=${{ fontSize: 12.5, marginTop: 8, paddingTop: 8, borderTop: '1px solid var(--rule-soft)' }} dangerouslySetInnerHTML=${{ __html: renderBlockHtml(m.body || '', store.vaultPages) }} />`}
      </div>`;
    })}
  </div>`;
}
