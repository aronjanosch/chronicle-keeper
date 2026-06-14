// Standing full-text search with faceted filters (Phase 7b). Own route; reuses
// the FTS `/vault/search` endpoint, now with kind/tag/folder/date facets pushed
// into SQL so ranking + the 50-hit cap stay correct. Pages-only scope; the
// summaries/transcripts tiers land in 7d.
import { html, useState, useEffect, useRef } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, useStore } from '../core.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Input, Empty } from '../ui.js';
import { searchVault, searchSessions, loadVaultTree, loadVaultTags, refreshCampaignSessions } from '../actions.js';
import { KINDS, iconForKind, dirOf } from './codex.js';
import { openPageEvt } from '../tabs.js';

const SCOPES = [
  { value: 'pages', label: 'Pages', icon: 'book' },
  { value: 'summaries', label: 'Summaries', icon: 'scroll' },
  { value: 'transcripts', label: 'Transcripts', icon: 'mic' },
];

const DATE_PRESETS = [
  { value: 'any', label: 'Any time' },
  { value: '1d', label: '24 hours', days: 1 },
  { value: '7d', label: '7 days', days: 7 },
  { value: '30d', label: '30 days', days: 30 },
];

function Chip({ active, onClick, children, icon }) {
  return html`<button onClick=${onClick} style=${{
    display: 'inline-flex', alignItems: 'center', gap: 5, padding: '4px 11px', borderRadius: 999,
    fontSize: 12, cursor: 'pointer', whiteSpace: 'nowrap',
    background: active ? 'var(--burgundy-50)' : 'var(--surface)',
    border: `1px solid ${active ? 'var(--burgundy-300)' : 'var(--rule)'}`,
    color: active ? 'var(--burgundy-700)' : 'var(--ink-soft)',
  }}>
    ${icon && html`<${Icon} name=${icon} size=${11} />`}${children}
  </button>`;
}

function FacetRow({ label, children }) {
  return html`<div style=${{ display: 'flex', alignItems: 'baseline', gap: 10, marginBottom: 8 }}>
    <div style=${{ flex: '0 0 64px', fontSize: 10, fontWeight: 600, letterSpacing: '0.08em', textTransform: 'uppercase', color: 'var(--ink-faint)', paddingTop: 5 }}>${label}</div>
    <div style=${{ flex: 1, display: 'flex', flexWrap: 'wrap', gap: 6 }}>${children}</div>
  </div>`;
}

export function SearchScreen() {
  const store = useStore();
  const c = store.campaign;
  const cid = c?.campaign_id;
  const [q, setQ] = useState(store.route.params?.q || '');
  const [scope, setScope] = useState('pages');
  const [kind, setKind] = useState(null);
  const [tag, setTag] = useState(null);
  const [folder, setFolder] = useState(null);
  const [datePreset, setDatePreset] = useState('any');
  const [results, setResults] = useState([]);
  const [loading, setLoading] = useState(false);
  const timer = useRef(null);
  const inputRef = useRef(null);

  useEffect(() => { if (cid) { loadVaultTree(cid); loadVaultTags(cid); refreshCampaignSessions(); } inputRef.current?.focus(); }, [cid]);

  const query = q.trim();
  const facets = (() => {
    const f = {};
    if (kind) f.kind = kind;
    if (tag) f.tag = tag;
    if (folder) f.folder = folder;
    const preset = DATE_PRESETS.find((p) => p.value === datePreset);
    if (preset?.days) f.edited_after = Math.floor(Date.now() / 1000) - preset.days * 86400;
    return f;
  })();
  const facetKey = JSON.stringify(facets);

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current);
    if (!cid || query.length < 2) { setResults([]); setLoading(false); return; }
    setLoading(true);
    timer.current = setTimeout(() => {
      const p = scope === 'pages' ? searchVault(query, facets) : searchSessions(query, scope);
      p.then((hits) => { setResults(hits || []); setLoading(false); })
        .catch(() => { setResults([]); setLoading(false); });
    }, 220);
    return () => { if (timer.current) clearTimeout(timer.current); };
  }, [query, facetKey, scope, cid, store.dirty_vault]);

  if (!c) { navigate('library'); return null; }

  const tags = store.vaultTags || [];
  const folders = (store.vaultFolders || []).map((f) => f.path).filter(Boolean).sort();
  const activeFacets = Object.keys(facets).length;
  const clearFacets = () => { setKind(null); setTag(null); setFolder(null); setDatePreset('any'); };

  // Session hits carry a session *number*; the session screen wants its id.
  const idByNumber = new Map((store.campaignSessions || []).map((s) => [s.session_number, s.session_id]));
  const openSession = (h) => { const sid = idByNumber.get(h.session); if (sid) navigate('session', { id: sid }); };

  const sidebar = html`<${Sidebar} variant="campaign" active="search" campaign=${c} />`;
  const topbar = html`<${Topbar} crumbs=${[
    { label: 'Worlds', onClick: () => navigate('library') },
    { label: c.name, onClick: () => navigate('campaign', { id: cid }) },
    'Search',
  ]} />`;

  return html`<${Shell} sidebar=${sidebar} topbar=${topbar} bodyStyle=${{ padding: 0 }}>
    <div style=${{ height: '100%', overflow: 'auto', padding: '22px 26px', maxWidth: 860, margin: '0 auto', minWidth: 0 }}>
      <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--burgundy)' }}>Search the world</div>
      <h2 style=${{ fontFamily: 'var(--font-display)', fontSize: 24, fontWeight: 500, letterSpacing: '-0.015em', margin: '2px 0 16px' }}>Full-text search</h2>

      <div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 14px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 10, marginBottom: 16 }}>
        <${Icon} name="search" size=${16} className="ck-ink-faint" />
        <input ref=${inputRef} value=${q} onInput=${(e) => setQ(e.target.value)}
          placeholder=${scope === 'pages' ? 'Search page titles, summaries, and body text…' : `Search session ${scope}…`}
          style=${{ flex: 1, border: 'none', outline: 'none', background: 'transparent', fontSize: 15, color: 'var(--ink)', fontFamily: 'inherit' }} />
        ${q && html`<span onClick=${() => setQ('')} title="Clear" style=${{ color: 'var(--ink-faint)', cursor: 'pointer', display: 'flex' }}><${Icon} name="x" size=${14} /></span>`}
      </div>

      <div style=${{ display: 'flex', gap: 4, padding: 3, background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 6, marginBottom: 14, width: 'fit-content' }}>
        ${SCOPES.map((s) => html`<button key=${s.value} onClick=${() => setScope(s.value)}
          style=${{ padding: '5px 12px', borderRadius: 3, whiteSpace: 'nowrap', border: 'none', cursor: 'pointer', fontSize: 12, display: 'flex', alignItems: 'center', gap: 6,
            background: scope === s.value ? 'var(--paper-deep)' : 'transparent', color: scope === s.value ? 'var(--ink)' : 'var(--ink-muted)' }}>
          <${Icon} name=${s.icon} size=${12} /> ${s.label}</button>`)}
      </div>

      ${scope === 'pages' && html`<div style=${{ background: 'var(--surface)', border: '1px solid var(--rule-soft)', borderRadius: 8, padding: '12px 14px', marginBottom: 18 }}>
        <${FacetRow} label="Kind">
          ${KINDS.map((k) => html`<${Chip} key=${k.value} icon=${iconForKind(k.value)}
            active=${kind === k.value} onClick=${() => setKind(kind === k.value ? null : k.value)}>${k.label}</${Chip}>`)}
        </${FacetRow}>
        ${tags.length > 0 && html`<${FacetRow} label="Tag">
          ${tags.slice(0, 24).map((t) => html`<${Chip} key=${t.tag} icon="tag"
            active=${tag === t.tag} onClick=${() => setTag(tag === t.tag ? null : t.tag)}>${t.tag}</${Chip}>`)}
        </${FacetRow}>`}
        ${folders.length > 0 && html`<${FacetRow} label="Folder">
          <select value=${folder || ''} onChange=${(e) => setFolder(e.target.value || null)}
            style=${{ fontSize: 12.5, padding: '4px 8px', borderRadius: 6, border: '1px solid var(--rule)', background: 'var(--surface-raised)', color: 'var(--ink)', maxWidth: 280 }}>
            <option value="">Any folder</option>
            ${folders.map((f) => html`<option key=${f} value=${f}>${f}</option>`)}
          </select>
        </${FacetRow}>`}
        <${FacetRow} label="Edited">
          ${DATE_PRESETS.map((p) => html`<${Chip} key=${p.value}
            active=${datePreset === p.value} onClick=${() => setDatePreset(p.value)}>${p.label}</${Chip}>`)}
        </${FacetRow}>
        ${activeFacets > 0 && html`<div style=${{ marginTop: 4 }}>
          <button onClick=${clearFacets} style=${{ fontSize: 11.5, color: 'var(--burgundy)', background: 'none', border: 'none', cursor: 'pointer', padding: 0 }}>Clear filters</button>
        </div>`}
      </div>`}

      ${query.length < 2
        ? html`<${Empty} icon="search" title="Search your world">Type at least two characters.${scope === 'pages' ? ' Narrow with the kind, tag, folder, and date filters above — they apply in the index so ranking stays exact.' : ` Searching session ${scope}.`}</${Empty}>`
        : loading
          ? html`<div style=${{ fontSize: 12.5, color: 'var(--ink-faint)', fontStyle: 'italic', padding: '8px 0' }}>Searching…</div>`
          : results.length === 0
            ? html`<div style=${{ fontSize: 12.5, color: 'var(--ink-faint)', fontStyle: 'italic', padding: '8px 0' }}>No matches${scope === 'pages' && activeFacets ? ' with these filters' : ''}.</div>`
            : html`<div>
                <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginBottom: 10 }}>${results.length}${results.length >= 50 ? '+' : ''} result${results.length === 1 ? '' : 's'}</div>
                ${scope === 'pages'
                  ? results.map((h) => html`<div key=${h.path} onClick=${(e) => openPageEvt(h.path, e)}
                      style=${{ padding: '11px 14px', border: '1px solid var(--rule)', borderRadius: 8, marginBottom: 8, cursor: 'pointer', background: 'var(--surface)' }}
                      onMouseEnter=${(e) => { e.currentTarget.style.borderColor = 'var(--rule-strong)'; }}
                      onMouseLeave=${(e) => { e.currentTarget.style.borderColor = 'var(--rule)'; }}>
                      <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
                        <${Icon} name=${iconForKind(h.kind)} size=${13} className="ck-ink-muted" />
                        <span style=${{ fontFamily: 'var(--font-display)', fontSize: 14.5, fontWeight: 500, color: 'var(--ink)' }}>${h.title}</span>
                        <span style=${{ flex: 1 }} />
                        <span style=${{ fontSize: 10.5, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)', display: 'flex', alignItems: 'center', gap: 4 }}>
                          <${Icon} name="folder" size=${10} />${dirOf(h.path) || 'root'}
                        </span>
                      </div>
                      ${h.snippet && html`<div style=${{ fontSize: 12.5, color: 'var(--ink-soft)', marginTop: 5, lineHeight: 1.5 }}
                        dangerouslySetInnerHTML=${{ __html: h.snippet }} />`}
                    </div>`)
                  : results.map((h, i) => html`<div key=${i} onClick=${() => openSession(h)}
                      style=${{ padding: '11px 14px', border: '1px solid var(--rule)', borderRadius: 8, marginBottom: 8, cursor: 'pointer', background: 'var(--surface)' }}
                      onMouseEnter=${(e) => { e.currentTarget.style.borderColor = 'var(--rule-strong)'; }}
                      onMouseLeave=${(e) => { e.currentTarget.style.borderColor = 'var(--rule)'; }}>
                      <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
                        <${Icon} name=${scope === 'transcripts' ? 'mic' : 'scroll'} size=${13} className="ck-ink-muted" />
                        <span style=${{ fontFamily: 'var(--font-display)', fontSize: 14.5, fontWeight: 500, color: 'var(--ink)' }}>Session ${h.session}${h.title ? ` — ${h.title}` : ''}</span>
                        <span style=${{ flex: 1 }} />
                        ${h.turn != null && html`<span style=${{ fontSize: 10.5, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)' }}>turn ${h.turn}</span>`}
                      </div>
                      ${h.snippet && html`<div style=${{ fontSize: 12.5, color: 'var(--ink-soft)', marginTop: 5, lineHeight: 1.5 }}
                        dangerouslySetInnerHTML=${{ __html: h.snippet }} />`}
                    </div>`)}
              </div>`}
    </div>
  </${Shell}>`;
}
