// Graph view (Phase 9D): the whole world as a force-directed map. Nodes =
// pages (kind-colored), edges = wikilinks (gray) + typed relations (burgundy).
// Overlays: search (focus a page), kind/orphan filters, zoom/fit controls.
import { html, useEffect, useMemo, useRef, useState } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, useStore } from '../core.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Empty } from '../ui.js';
import { loadVaultTree, loadVaultLinks, loadRelations } from '../actions.js';
import { buildGraph, GraphCanvas, colorForKind } from '../graph.js';
import { KINDS } from './codex.js';

const overlayBox = {
  position: 'absolute', display: 'flex', alignItems: 'center', gap: 8,
  padding: '7px 12px', background: 'var(--surface-raised)', border: '1px solid var(--rule)',
  borderRadius: 8, fontSize: 11.5, color: 'var(--ink-soft)', boxShadow: 'var(--shadow-raised)',
};

function Legend({ hiddenKinds, onToggleKind, hideOrphans, onToggleOrphans }) {
  return html`<div style=${{ ...overlayBox, left: 14, bottom: 12, gap: 12, flexWrap: 'wrap' }}>
    ${KINDS.map((k) => {
      const off = hiddenKinds.has(k.value);
      return html`<span key=${k.value} title=${off ? `Show ${k.label}` : `Hide ${k.label}`}
        onClick=${() => onToggleKind(k.value)}
        style=${{ display: 'flex', alignItems: 'center', gap: 5, cursor: 'pointer', opacity: off ? 0.35 : 1, textDecoration: off ? 'line-through' : 'none' }}>
        <span style=${{ width: 8, height: 8, borderRadius: 999, background: colorForKind(k.value) }} />${k.label}
      </span>`;
    })}
    <span style=${{ display: 'flex', alignItems: 'center', gap: 5 }}>
      <span style=${{ width: 14, height: 2, background: 'rgba(90,72,65,.45)' }} />typed relation
    </span>
    <span onClick=${onToggleOrphans} title="Pages without any links"
      style=${{ display: 'flex', alignItems: 'center', gap: 5, cursor: 'pointer', borderLeft: '1px solid var(--rule-soft)', paddingLeft: 12, color: hideOrphans ? 'var(--burgundy)' : 'inherit' }}>
      ${hideOrphans ? 'orphans hidden' : 'hide orphans'}
    </span>
  </div>`;
}

const ctlBtn = {
  width: 26, height: 26, display: 'flex', alignItems: 'center', justifyContent: 'center',
  background: 'none', border: 'none', borderRadius: 5, cursor: 'pointer',
  color: 'var(--ink-soft)', fontSize: 14, lineHeight: 1, padding: 0,
};

function Controls({ api }) {
  const btn = (label, title, run) => html`<button style=${ctlBtn} title=${title} onClick=${run}
    onMouseEnter=${(e) => { e.currentTarget.style.background = 'rgba(120,90,40,.1)'; }}
    onMouseLeave=${(e) => { e.currentTarget.style.background = 'none'; }}>${label}</button>`;
  return html`<div style=${{ ...overlayBox, right: 14, top: 12, padding: 4, gap: 2 }}>
    ${btn('+', 'Zoom in', () => api.current?.zoom(1.25))}
    ${btn('−', 'Zoom out', () => api.current?.zoom(0.8))}
    ${btn('⤢', 'Fit to view', () => api.current?.fit())}
    ${btn('↺', 'Re-run layout', () => api.current?.relayout())}
  </div>`;
}

export function GraphScreen() {
  const store = useStore();
  const c = store.campaign;
  const apiRef = useRef(null);
  const [hiddenKinds, setHiddenKinds] = useState(() => new Set());
  const [hideOrphans, setHideOrphans] = useState(false);
  const [query, setQuery] = useState('');

  useEffect(() => {
    if (!c) return;
    loadVaultTree(c.campaign_id);
    loadVaultLinks(c.campaign_id);
    loadRelations(c.campaign_id);
  }, [c?.campaign_id]);

  if (!c) { navigate('library'); return null; }

  const pages = store.vaultPages || [];
  const links = (store.vaultLinks || {}).links || [];
  const relations = (store.vaultRelations || []);
  const graph = useMemo(
    () => buildGraph(pages, links, relations),
    [pages.length, links.length, relations.length],
  );

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return null;
    return new Set(graph.nodes.filter((n) => (n.title || '').toLowerCase().includes(q)).map((n) => n.path));
  }, [query, graph]);

  const toggleKind = (k) => setHiddenKinds((prev) => {
    const next = new Set(prev);
    if (next.has(k)) next.delete(k); else next.add(k);
    return next;
  });

  const visible = graph.nodes.filter((n) => !hiddenKinds.has(n.kind) && !(hideOrphans && n.degree === 0));
  const filtered = visible.length !== graph.nodes.length;

  const topbar = html`<${Topbar} crumbs=${[
    { label: 'Worlds', onClick: () => navigate('library') },
    { label: c.name, onClick: () => navigate('campaign', { id: c.campaign_id }) },
    'Graph',
  ]} right=${html`<span style=${{ fontSize: 11.5, fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)' }}>
    ${filtered ? `${visible.length} of ${graph.nodes.length}` : graph.nodes.length} pages · ${graph.edges.length} links
  </span>`} />`;

  return html`<${Shell} sidebar=${html`<${Sidebar} variant="campaign" active="graph" campaign=${c} />`}
    topbar=${topbar} bodyStyle=${{ padding: 0 }}>
    <div style=${{ position: 'relative', height: '100%', background: 'var(--paper)' }}>
      ${graph.nodes.length
        ? html`<${GraphCanvas} nodes=${graph.nodes} edges=${graph.edges}
            hiddenKinds=${hiddenKinds} hideOrphans=${hideOrphans} matches=${matches} apiRef=${apiRef}
            onOpen=${(path) => navigate('page', { path })} />
          <div style=${{ ...overlayBox, left: 14, top: 12, padding: '5px 10px' }}>
            <input value=${query} placeholder="Find page…"
              onInput=${(e) => setQuery(e.target.value)}
              onKeyDown=${(e) => {
                if (e.key === 'Enter' && matches && matches.size) apiRef.current?.focus(matches.values().next().value);
                if (e.key === 'Escape') setQuery('');
              }}
              style=${{ background: 'none', border: 'none', outline: 'none', fontSize: 12.5, color: 'var(--ink)', width: 150, fontFamily: 'inherit' }} />
            ${matches && html`<span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11, color: matches.size ? 'var(--ochre)' : 'var(--ink-faint)' }}>
              ${matches.size ? `${matches.size} ⏎` : 'no match'}
            </span>`}
          </div>
          <${Controls} api=${apiRef} />
          <div style=${{ position: 'absolute', right: 14, bottom: 12, fontSize: 10.5, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)' }}>
            click select · double-click open · drag moves
          </div>
          <${Legend} hiddenKinds=${hiddenKinds} onToggleKind=${toggleKind}
            hideOrphans=${hideOrphans} onToggleOrphans=${() => setHideOrphans((v) => !v)} />`
        : html`<div style=${{ padding: 40 }}><${Empty} icon="link" title="Nothing to map yet">Create some pages and link them with <code>[[wikilinks]]</code>.</${Empty}></div>`}
    </div>
  </${Shell}>`;
}
