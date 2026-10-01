// App shell: sidebar + topbar + body slot. Ported from the design's shell.jsx,
// wired to the store's router.
import { html, useState, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { navigate, navigateBack, navigateForward, openModal, store, useStore } from './core.js';
import { dismissUpdate, openCampaign } from './actions.js';
import { Icon, Sigil, BrandMark, Menu, SearchField } from './ui.js';
import { useFlag, toggleFlag, RAIL_COMPACT } from './layoutPrefs.js';
import { runCommand } from './commands.js';

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
  const reset = () => {
    setW(fallback);
    try { localStorage.removeItem(key); } catch (_) { /* private mode */ }
  };
  return [w, onMouseDown, reset];
}

export function ResizeHandle({ onMouseDown, onReset, side }) {
  return html`<div class=${side === 'left' ? 'ck-resize-handle left' : 'ck-resize-handle'} onMouseDown=${onMouseDown} onDblClick=${onReset}
    title=${onReset ? 'Drag to resize · double-click to reset' : 'Drag to resize'} />`;
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
  walk('', 0);
  return rows;
}

// Single source of truth for in-world destinations (rail + palette).
export const WORLD_NAV = [
  { key: 'overview', icon: 'compass', label: 'Overview', screen: 'campaign' },
  { key: 'codex', icon: 'scroll', label: 'Codex', screen: 'codex' },
  { key: 'search', icon: 'search', label: 'Search', screen: 'search' },
  { key: 'atlas', icon: 'map', label: 'Atlas', screen: 'atlas' },
  { key: 'timeline', icon: 'time', label: 'Timeline', screen: 'timeline' },
  { key: 'graph', icon: 'link', label: 'Graph', screen: 'graph' },
  { key: 'keeper', icon: 'feather', label: 'Keeper', screen: 'keeper' },
  { key: 'sessions', icon: 'mic', label: 'Sessions', screen: 'sessions' },
  { key: 'settings', icon: 'cog', label: 'Settings', screen: 'settings' },
];

export function navToWorldDest(dest, campaignId) {
  navigate(dest.screen, dest.key === 'settings' ? undefined : { id: campaignId });
}

function RailItem({ icon, label, active, compact, onClick, onContextMenu, dot, title }) {
  return html`<div class="ck-rail-item" role="button" tabIndex=${0} title=${title || label} onClick=${onClick} onContextMenu=${onContextMenu}
    onKeyDown=${(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onClick(); } }}
    data-active=${active ? '1' : undefined}
    style=${{
      width: compact ? 40 : 60, padding: compact ? '9px 0' : '7px 0 6px', borderRadius: 6,
      display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 3, cursor: 'pointer', position: 'relative',
      background: active ? 'var(--surface)' : 'transparent',
      border: active ? '1px solid var(--rule-soft)' : '1px solid transparent',
      color: active ? 'var(--burgundy)' : 'var(--ink-soft)',
    }}>
    ${active && html`<span style=${{ position: 'absolute', left: -7, top: 9, bottom: 9, width: 3, borderRadius: 2, background: 'var(--burgundy)' }} />`}
    <${Icon} name=${icon} size=${18} />
    ${!compact && html`<span style=${{ fontSize: 11, fontWeight: active ? 600 : 500, lineHeight: 1.2 }}>${label}</span>`}
    ${dot && html`<span style=${{ position: 'absolute', top: 5, right: compact ? 6 : 10, width: 7, height: 7, borderRadius: '50%', background: dot }} />`}
  </div>`;
}

// Sigil at the top of the rail = the world switcher.
function WorldSwitcher({ campaign, compact }) {
  const s = useStore();
  const [open, setOpen] = useState(false);
  const inWorld = !!campaign;
  useEffect(() => {
    if (!open) return undefined;
    const close = () => setOpen(false);
    const onKey = (e) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', close);
    document.addEventListener('keydown', onKey);
    return () => { document.removeEventListener('mousedown', close); document.removeEventListener('keydown', onKey); };
  }, [open]);
  const worlds = s.campaigns || [];
  return html`<div style=${{ position: 'relative', marginBottom: 10 }} onMouseDown=${(e) => e.stopPropagation()}>
    <div class="ck-rail-item" role="button" tabIndex=${0} title=${inWorld ? 'Switch world' : 'All worlds'}
      onClick=${() => (inWorld || worlds.length ? setOpen((o) => !o) : navigate('library'))}
      style=${{ cursor: 'pointer', display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 3, padding: '2px 0', borderRadius: 6 }}>
      ${inWorld ? html`<${Sigil} ch=${campaign.sigil || '?'} tone=${campaign.tone || 'burgundy'} />` : html`<${BrandMark} size=${32} />`}
      ${!compact && html`<${Icon} name="chev-d" size=${10} className="ck-ink-muted" />`}
    </div>
    ${open && html`<div style=${{ position: 'fixed', left: 8, top: 56, zIndex: 120, width: 232, background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 8, boxShadow: 'var(--shadow-raised)', padding: 4 }}>
      <div class="ck-label" style=${{ padding: '6px 9px 4px' }}>Worlds</div>
      ${worlds.map((w) => html`<div key=${w.campaign_id} class="ck-menu-row" onClick=${() => { setOpen(false); openCampaign(w.campaign_id); }}
        style=${{ background: campaign?.campaign_id === w.campaign_id ? 'var(--paper-deep)' : undefined }}>
        <${Sigil} ch=${(w.name || '?').slice(0, 1).toUpperCase()} tone=${w.tone || 'burgundy'} />
        <span style=${{ flex: 1, minWidth: 0, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${w.name}</span>
      </div>`)}
      <div style=${{ height: 1, background: 'var(--rule-soft)', margin: '4px 0' }} />
      <div class="ck-menu-row" onClick=${() => { setOpen(false); navigate('library'); }}><${Icon} name="globe" size=${14} /><span>All worlds</span></div>
      <div class="ck-menu-row" onClick=${() => { setOpen(false); navigate('newWorld'); }}><${Icon} name="plus" size=${14} /><span>New world</span></div>
    </div>`}
  </div>`;
}

// Contextual panel on the Atlas screen: the map hierarchy (was in the old sidebar).
function AtlasMapsPanel({ campaign }) {
  const rows = mapTreeRows(store.atlasMaps || []);
  if (!rows.length) return null;
  const cur = store.atlasMapId || store.route.params?.map;
  return html`<aside style=${{ width: 200, flex: '0 0 200px', borderRight: '1px solid var(--rule)', background: 'var(--paper-deep)', padding: '14px 8px', overflow: 'auto' }}>
    <div class="ck-label" style=${{ padding: '0 8px 8px' }}>Maps</div>
    ${rows.map(({ map: m, depth }) => html`<div key=${m.id} class="ck-menu-row" onClick=${() => navigate('atlas', { id: campaign?.campaign_id, map: m.id })}
      style=${{ paddingLeft: 9 + depth * 16, background: cur === m.id ? 'var(--surface)' : undefined, border: cur === m.id ? '1px solid var(--rule-soft)' : '1px solid transparent', fontWeight: cur === m.id ? 600 : 500 }}>
      <span style=${{ flex: 1, minWidth: 0, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${m.name}</span>
    </div>`)}
  </aside>`;
}

// The one navigation rail: 72px icon + label, 52px compact (⌘⇧B, persisted).
export function Rail({ variant = 'library', active, campaign }) {
  const compact = useFlag(RAIL_COMPACT, false);
  const warn = store.providerStatus && store.providerStatus.ok === false ? store.providerStatus : null;
  const update = store.updateInfo;
  const inWorld = variant === 'campaign' && campaign;
  const nav = inWorld ? WORLD_NAV.filter((d) => d.key !== 'settings') : [{ key: 'worlds', icon: 'globe', label: 'Worlds', screen: 'library' }];
  const isActive = (k) => active === k || (k === 'worlds' && (active === 'campaigns' || active === 'worlds'));
  return html`<nav aria-label="Main" style=${{
    width: compact ? 52 : 72, flex: `0 0 ${compact ? 52 : 72}px`, transition: 'width .18s', background: 'var(--paper-deep)',
    borderRight: '1px solid var(--rule)', display: 'flex', flexDirection: 'column', alignItems: 'center', padding: '12px 0', gap: 2,
    position: 'relative', zIndex: 30,
  }}>
    <${WorldSwitcher} campaign=${inWorld ? campaign : null} compact=${compact} />
    ${nav.map((d) => html`<${RailItem} key=${d.key} icon=${d.icon} label=${d.label} compact=${compact} active=${isActive(d.key)}
      onClick=${() => (inWorld ? navToWorldDest(d, campaign.campaign_id) : navigate(d.screen))} />`)}
    <div style=${{ flex: 1 }} />
    ${update && html`<${RailItem} icon="download" label="Update" compact=${compact} dot="var(--moss)" title=${`Update v${update.version} available — click to download, right-click to dismiss`}
      onClick=${() => window.__TAURI__?.opener?.openUrl(update.url)}
      onContextMenu=${(e) => { e.preventDefault(); dismissUpdate(update.tag); }} />`}
    <${RailItem} icon="cog" label="Settings" compact=${compact} active=${active === 'settings'} dot=${warn ? 'var(--ochre)' : null}
      title=${warn ? `Settings — needs attention: ${warn.reason}` : 'Settings (⌘,)'} onClick=${() => navigate('settings')} />
    <button class="ck-rail-item" title=${compact ? 'Expand sidebar (⌘⇧B)' : 'Collapse sidebar (⌘⇧B)'} aria-label="Toggle sidebar"
      onClick=${() => toggleFlag(RAIL_COMPACT, false)}
      style=${{ width: compact ? 40 : 60, height: 30, marginTop: 6, borderRadius: 6, color: 'var(--ink-muted)', fontSize: 14 }}>${compact ? '»' : '«'}</button>
  </nav>`;
}

// Rail plus, on the Atlas screen, its contextual map list.
export function Sidebar({ variant = 'library', active, campaign }) {
  return html`<${Rail} variant=${variant} active=${active} campaign=${campaign} />
    ${active === 'atlas' && variant === 'campaign' && html`<${AtlasMapsPanel} campaign=${campaign} />`}`;
}

function NavBtn({ icon, onClick, disabled, title }) {
  return html`<button onClick=${onClick} disabled=${disabled} title=${title} aria-label=${title}
    style=${{ display: 'flex', alignItems: 'center', justifyContent: 'center', width: 26, height: 26,
      background: 'none', border: 'none', borderRadius: 4, cursor: disabled ? 'default' : 'pointer',
      color: disabled ? 'var(--ink-faint)' : 'var(--ink-muted)', padding: 0, flexShrink: 0 }}
    onMouseEnter=${disabled ? null : (e) => { e.currentTarget.style.color = 'var(--ink)'; e.currentTarget.style.background = 'var(--surface)'; }}
    onMouseLeave=${disabled ? null : (e) => { e.currentTarget.style.color = 'var(--ink-muted)'; e.currentTarget.style.background = 'none'; }}>
    <${Icon} name=${icon} size=${13} />
  </button>`;
}

// Uniform topbar: title · centered ⌘K field · secondary actions · primary · Keeper.
// `crumbs` is legacy: only the last entry is used as the title. `right` = legacy
// action cluster; prefer `actions`, `primary`, `overflow` ({label,icon,onClick}[]).
export function Topbar({ title, sub, crumbs, right, actions, primary, overflow, search = true, searchNode }) {
  const s = useStore();
  const last = crumbs && crumbs.filter(Boolean).slice(-1)[0];
  const heading = title ?? (typeof last === 'string' ? last : last?.label);
  const hasWorld = !!s.campaign && s.route.name !== 'library' && s.route.name !== 'newWorld';
  return html`<div style=${{
    padding: '0 20px', borderBottom: '1px solid var(--rule-soft)',
    display: 'flex', alignItems: 'center', gap: 14, background: 'var(--paper)',
    flex: '0 0 auto', height: 48,
  }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 2, flex: '0 0 auto' }}>
      <${NavBtn} icon="chev-l" onClick=${navigateBack} disabled=${!s.canNavBack} title="Go back (⌘[)" />
      <${NavBtn} icon="chev-r" onClick=${navigateForward} disabled=${!s.canNavFwd} title="Go forward (⌘])" />
    </div>
    <div style=${{ display: 'flex', alignItems: 'baseline', gap: 8, minWidth: 0, flex: '0 1 auto' }}>
      <span style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', color: 'var(--ink)' }}>${heading}</span>
      ${sub && html`<span style=${{ fontSize: 12, color: 'var(--ink-muted)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${sub}</span>`}
    </div>
    <div style=${{ flex: 1, display: 'flex', justifyContent: 'center', minWidth: 0 }}>
      ${searchNode || (search && html`<${SearchField} value="" placeholder="Search or jump to…" hint="⌘K" onFocusOpen=${() => runCommand('palette')}
        onKeyDown=${(e) => { if (e.key === 'Enter') runCommand('palette'); }}
        style=${{ width: 'min(100%, 340px)' }} />`)}
    </div>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 8, flex: '0 0 auto' }}>
      ${actions ?? right}
      ${overflow && html`<${Menu} items=${overflow} />`}
      ${primary}
      ${hasWorld && html`<button class="ck-rail-item" title="Ask the Keeper (⌘J)" aria-label="Ask the Keeper" onClick=${() => runCommand('keeper')}
        style=${{ width: 32, height: 32, borderRadius: 6, border: '1px solid var(--rule)', background: 'var(--surface)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--burgundy)' }}>
        <${Icon} name="feather" size=${14} /></button>`}
    </div>
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
