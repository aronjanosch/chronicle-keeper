// App shell: sidebar + topbar + body slot. Ported from the design's shell.jsx,
// wired to the store's router.
import { html, useState } from '../vendor/htm-preact-standalone.mjs';
import { navigate, navigateBack, navigateForward, store, useStore } from './core.js';
import { Icon, Sigil, BrandMark } from './ui.js';

// Drag-resizable sidebar width, persisted per key. Returns [width, onMouseDown].
// opts.fromRight flips the drag direction for panels anchored on the right edge.
const SIDEBAR_MIN = 180;
const SIDEBAR_MAX = 480;
export function useSidebarWidth(key, fallback = 220, opts = {}) {
  const min = opts.min ?? SIDEBAR_MIN;
  const max = opts.max ?? SIDEBAR_MAX;
  const dir = opts.fromRight ? -1 : 1;
  const [w, setW] = useState(() => {
    try {
      const v = parseInt(localStorage.getItem(key), 10);
      return v >= min && v <= max ? v : fallback;
    } catch (_) { return fallback; }
  });
  function onMouseDown(e) {
    e.preventDefault();
    const x0 = e.clientX;
    const w0 = w;
    const clamp = (x) => Math.min(max, Math.max(min, w0 + dir * (x - x0)));
    const move = (ev) => setW(clamp(ev.clientX));
    const up = (ev) => {
      document.removeEventListener('mousemove', move);
      document.removeEventListener('mouseup', up);
      document.body.style.cursor = '';
      try { localStorage.setItem(key, String(clamp(ev.clientX))); } catch (_) { /* private mode */ }
    };
    document.body.style.cursor = 'col-resize';
    document.addEventListener('mousemove', move);
    document.addEventListener('mouseup', up);
  }
  return [w, onMouseDown];
}

export function ResizeHandle({ onMouseDown, side }) {
  return html`<div class=${side === 'left' ? 'ck-resize-handle left' : 'ck-resize-handle'} onMouseDown=${onMouseDown} title="Drag to resize" />`;
}

function NavItem({ icon, label, count, active, indent, onClick }) {
  return html`<div onClick=${onClick} style=${{
    display: 'flex', alignItems: 'center', gap: 9,
    padding: indent ? `6px 9px 6px ${9 + Number(indent) * 21}px` : '7px 9px',
    borderRadius: 4, color: active ? 'var(--ink)' : 'var(--ink-soft)',
    fontSize: 13, fontWeight: 500,
    background: active ? 'var(--surface)' : 'transparent',
    border: active ? '1px solid var(--rule-soft)' : '1px solid transparent',
    boxShadow: active ? '0 1px 0 rgba(120,90,40,.05)' : 'none', cursor: 'pointer',
  }}
    onMouseEnter=${(e) => { if (!active) e.currentTarget.style.background = 'rgba(120,90,40,.08)'; }}
    onMouseLeave=${(e) => { if (!active) e.currentTarget.style.background = 'transparent'; }}>
    ${icon && html`<${Icon} name=${icon} />`}
    <span style=${{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${label}</span>
    ${count != null && html`<span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11, color: active ? 'var(--burgundy)' : 'var(--ink-faint)' }}>${count}</span>`}
  </div>`;
}
function codexCount() {
  const n = (store.vaultPages || []).length;
  return n > 0 ? n : null;
}
function sessionsCount() {
  const n = (store.campaignSessions || []).length;
  return n > 0 ? n : null;
}

// Flatten the atlas map hierarchy (parent links) into depth-annotated rows.
function mapTreeRows(maps) {
  const kids = {};
  const ids = new Set(maps.map((m) => m.id));
  for (const m of maps) {
    const parent = m.parent && ids.has(m.parent) ? m.parent : '';
    (kids[parent] ||= []).push(m);
  }
  const rows = [];
  const seen = new Set();
  const walk = (parent, depth) => {
    for (const m of kids[parent] || []) {
      if (seen.has(m.id)) continue;
      seen.add(m.id);
      rows.push({ map: m, depth });
      walk(m.id, depth + 1);
    }
  };
  walk('', 1);
  return rows;
}

function NavHead({ children }) {
  return html`<div style=${{ padding: '14px 8px 4px', fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${children}</div>`;
}

// Single source of truth for in-world destinations — the full Sidebar and the
// compact icon row on the Codex/page screens both render from this list.
export const WORLD_NAV = [
  { key: 'overview', icon: 'compass', label: 'Overview', screen: 'campaign' },
  { key: 'codex', icon: 'book', label: 'The Codex', screen: 'codex' },
  { key: 'search', icon: 'search', label: 'Search', screen: 'search' },
  { key: 'atlas', icon: 'map', label: 'Atlas', screen: 'atlas' },
  { key: 'timeline', icon: 'time', label: 'Timeline', screen: 'timeline' },
  { key: 'graph', icon: 'link', label: 'Graph', screen: 'graph' },
  { key: 'keeper', icon: 'feather', label: 'The Keeper', screen: 'keeper' },
  { key: 'sessions', icon: 'mic', label: 'Sessions', screen: 'sessions' },
  { key: 'settings', icon: 'cog', label: 'Settings', screen: 'settings' },
];

export function navToWorldDest(dest, campaignId) {
  navigate(dest.screen, dest.key === 'settings' ? undefined : { id: campaignId });
}

export function Sidebar({ variant = 'library', active, campaign }) {
  const warn = store.providerStatus && store.providerStatus.ok === false ? store.providerStatus : null;
  const [width, onResize] = useSidebarWidth('ck_sidebar_w');
  return html`<aside style=${{
    background: 'var(--paper-deep)', borderRight: '1px solid var(--rule)',
    padding: '14px 12px', display: 'flex', flexDirection: 'column', gap: 2,
    width, flex: `0 0 ${width}px`, position: 'relative',
  }}>
    <${ResizeHandle} onMouseDown=${onResize} />
    <div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '4px 6px 14px', borderBottom: '1px solid var(--rule-soft)', marginBottom: 4, cursor: 'pointer' }}
      onClick=${() => navigate('library')}>
      <${BrandMark} size=${30} />
      <div style=${{ lineHeight: 1.15 }}>
        <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500, letterSpacing: '-0.01em' }}>Chronicle Keeper</div>
        <div style=${{ fontSize: 10, fontWeight: 500, color: 'var(--ink-faint)', letterSpacing: '0.08em', textTransform: 'uppercase', marginTop: 2 }}>v0.5 · worldbuilding</div>
      </div>
    </div>

    ${variant === 'library' ? html`
      <${NavHead}>Library</${NavHead}>
      <${NavItem} icon="globe" label="Worlds" active=${active === 'campaigns' || active === 'worlds'} onClick=${() => navigate('library')} />
    ` : html`
      <${NavItem} icon="chev-l" label="All worlds" onClick=${() => navigate('library')} />
      <div style=${{ margin: '10px 4px 6px', padding: '10px', background: 'var(--surface)', border: '1px solid var(--rule-soft)', borderRadius: 6, display: 'flex', alignItems: 'center', gap: 10 }}>
        <${Sigil} ch=${campaign?.sigil || '?'} tone=${campaign?.tone || 'burgundy'} />
        <div style=${{ lineHeight: 1.2, minWidth: 0 }}>
          <div style=${{ fontFamily: 'var(--font-display)', fontSize: 14, fontWeight: 500, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${campaign?.name || 'World'}</div>
          <div style=${{ fontSize: 11, color: 'var(--ink-muted)', marginTop: 2 }}>${campaign?.system || '—'}</div>
        </div>
      </div>
      <${NavHead}>World</${NavHead}>
      ${WORLD_NAV.filter((d) => d.key !== 'settings').map((d) => html`
        <${NavItem} key=${d.key} icon=${d.icon} label=${d.label}
          count=${d.key === 'codex' ? codexCount(campaign) : d.key === 'sessions' ? sessionsCount() : null}
          active=${active === d.key}
          onClick=${() => navToWorldDest(d, campaign?.campaign_id)} />
        ${d.key === 'atlas' && active === 'atlas' && mapTreeRows(store.atlasMaps || []).map(({ map: m, depth }) => html`
          <${NavItem} key=${m.id} indent=${depth} label=${m.name}
            active=${(store.atlasMapId || store.route.params?.map) === m.id}
            onClick=${() => navigate('atlas', { id: campaign?.campaign_id, map: m.id })} />`)}
      `)}
    `}

    <div style=${{ flex: 1 }} />
    <${NavItem} icon="cog" label="Settings" active=${active === 'settings'} onClick=${() => navigate('settings')} />
    ${warn && html`<div onClick=${() => navigate('settings')} title="Open Settings"
      style=${{ margin: '8px 4px 0', padding: '10px 12px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.28)', borderRadius: 6, display: 'flex', alignItems: 'center', gap: 10, fontSize: 12, color: 'var(--ink-soft)', cursor: 'pointer' }}>
      <span style=${{ width: 8, height: 8, borderRadius: '50%', background: 'var(--ochre)', flex: '0 0 auto' }} />
      <div style=${{ lineHeight: 1.3 }}>
        <div style=${{ color: 'var(--ochre)', fontWeight: 600 }}>Needs attention</div>
        <div>${warn.reason}</div>
      </div>
    </div>`}
  </aside>`;
}

function NavBtn({ icon, onClick, disabled, title }) {
  return html`<button onClick=${onClick} disabled=${disabled} title=${title}
    style=${{ display: 'flex', alignItems: 'center', justifyContent: 'center', width: 26, height: 26,
      background: 'none', border: 'none', borderRadius: 4, cursor: disabled ? 'default' : 'pointer',
      color: disabled ? 'var(--ink-faint)' : 'var(--ink-muted)', padding: 0, flexShrink: 0 }}
    onMouseEnter=${disabled ? null : (e) => { e.currentTarget.style.color = 'var(--ink)'; e.currentTarget.style.background = 'var(--surface)'; }}
    onMouseLeave=${disabled ? null : (e) => { e.currentTarget.style.color = 'var(--ink-muted)'; e.currentTarget.style.background = 'none'; }}>
    <${Icon} name=${icon} size=${13} />
  </button>`;
}

export function Topbar({ crumbs = [], right }) {
  const s = useStore();
  return html`<div style=${{
    padding: '0 24px', borderBottom: '1px solid var(--rule-soft)',
    display: 'flex', alignItems: 'center', gap: 12, background: 'var(--paper)',
    flex: '0 0 auto', height: 52,
  }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 2 }}>
      <${NavBtn} icon="chev-l" onClick=${navigateBack} disabled=${!s.canNavBack} title="Go back (⌘[)" />
      <${NavBtn} icon="chev-r" onClick=${navigateForward} disabled=${!s.canNavFwd} title="Go forward (⌘])" />
    </div>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 13, color: 'var(--ink-muted)' }}>
      ${crumbs.filter(Boolean).map((c, i, arr) => {
        const label = typeof c === 'string' ? c : c?.label;
        const onClick = typeof c === 'object' ? c?.onClick : null;
        const last = i === arr.length - 1;
        return html`
          ${i > 0 && html`<span style=${{ color: 'var(--ink-faint)' }}>›</span>`}
          <span onClick=${onClick || undefined}
            style=${{ color: last ? 'var(--ink)' : 'var(--ink-muted)', fontWeight: last ? 500 : 400, cursor: onClick ? 'pointer' : 'default' }}
            onMouseEnter=${onClick ? (e) => { e.currentTarget.style.color = 'var(--burgundy)'; } : undefined}
            onMouseLeave=${onClick ? (e) => { e.currentTarget.style.color = 'var(--ink-muted)'; } : undefined}>${label}</span>
        `;
      })}
    </div>
    <div style=${{ flex: 1 }} />
    ${right}
  </div>`;
}

export function Shell({ sidebar, topbar, tabstrip, children, bodyStyle = {} }) {
  return html`<div class="ck" style=${{ display: 'flex', width: '100%', height: '100%', background: 'var(--paper)' }}>
    ${sidebar}
    <main style=${{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, overflow: 'hidden' }}>
      ${tabstrip}
      ${topbar}
      <div style=${{ flex: 1, overflow: 'auto', padding: '24px 28px', ...bodyStyle }}>${children}</div>
    </main>
  </div>`;
}
