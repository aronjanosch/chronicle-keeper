// Shared atoms ported from the design's atoms.jsx + a few form/primitive helpers.
import { html, useState, useEffect, useLayoutEffect, useRef, useCallback } from '../vendor/htm-preact-standalone.mjs';
import { marked } from '../vendor/marked.esm.js';
import { navigate, store, apiBlob, apiFetch, openInNewTab } from './core.js';

// ── Icon — monoline 16-grid SVGs ──────────────────────────────────
const PATHS = {
  book:     'M3 2.5h7a2 2 0 0 1 2 2V14M3 2.5v11.5h7M3 2.5v11.5a1 1 0 0 0 1 1h8',
  scroll:   'M4 2h7a2 2 0 0 1 2 2v8M11 14H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2M5 5h6M5 8h6M5 11h4',
  compass:  null, // custom below
  feather:  'M13 3 6 10l-2 3 3-2 7-7Z M3 13l3-2 M9 4v3h3',
  sparkle:  'M8 2v4M8 10v4M2 8h4M10 8h4M4 4l2 2M10 10l2 2M4 12l2-2M10 6l2-2',
  mic:      null,
  tag:      null,
  users:    null,
  shield:   'M8 1.5 3 3v5c0 3 2.4 5.5 5 6.5 2.6-1 5-3.5 5-6.5V3l-5-1.5Z',
  flame:    'M8 14c2.8 0 4.5-1.8 4.5-4 0-2-1.5-3-2-5-1 1-1.5 2-1.5 3 0-1.5-1-3-2-4-.5 2.5-3 4-3 6.5C4 12.5 5.2 14 8 14Z',
  map:      'm2 4 4-1 4 1 4-1v9l-4 1-4-1-4 1V4ZM6 3v10M10 4v10',
  gem:      'm8 2 4 3-4 9-4-9 4-3ZM4 5h8M8 2v3M6 5l2 9M10 5l-2 9',
  cog:      null,
  plus:     'M8 3v10M3 8h10',
  check:    'm3 8 3 3 7-7',
  'chev-r': 'm6 3 5 5-5 5',
  'chev-l': 'm10 3-5 5 5 5',
  'chev-d': 'm3 6 5 5 5-5',
  'chev-u': 'm3 10 5-5 5 5',
  'arrow-r':'M3 8h10M9 4l4 4-4 4',
  upload:   'M8 11V3M5 6l3-3 3 3M3 13h10',
  download: 'M8 3v8M5 8l3 3 3-3M3 13h10',
  export:   'M8 2v8M5 5l3-3 3 3M3 9v4h10V9',
  search:   null,
  filter:   'M2 3h12L9 9v4l-2 1V9L2 3Z',
  dots:     null,
  edit:     'm3 13 1-3 7-7 2 2-7 7-3 1ZM10 4l2 2',
  copy:     'M5.5 5h7a.5.5 0 0 1 .5.5v7a.5.5 0 0 1-.5.5h-7a.5.5 0 0 1-.5-.5v-7A.5.5 0 0 1 5.5 5ZM3 10.5v-7A.5.5 0 0 1 3.5 3h7',
  link:     'M6.5 9.5 9.5 6.5M6 4l1-1a2.5 2.5 0 0 1 3.5 3.5l-1 1M10 12l-1 1A2.5 2.5 0 0 1 5.5 9.5l1-1',
  time:     null,
  cal:      null,
  play:     null,
  doc:      'M4 1.5h5l3 3V14a.5.5 0 0 1-.5.5h-7A.5.5 0 0 1 4 14V2a.5.5 0 0 1 .5-.5ZM9 1.5v3h3M6 7h4M6 9.5h4M6 12h2.5',
  folder:   'M2 4a.5.5 0 0 1 .5-.5h3l1.5 1.5h6.5a.5.5 0 0 1 .5.5v7a.5.5 0 0 1-.5.5h-11A.5.5 0 0 1 2 12.5V4Z',
  cloud:    'M4.5 12a3 3 0 0 1 0-6 3.5 3.5 0 0 1 6.7-1A2.8 2.8 0 0 1 12 12H4.5Z',
  waveform: 'M2 8h1M4 6v4M6 4v8M8 5v6M10 3v10M12 6v4M14 8h.5',
  cpu:      null,
  eye:      null,
  archive:  null,
  sun:      null,
  globe:    null,
  trash:    'M3 4h10M6 4V2.5h4V4M5 4l.5 9.5a.5.5 0 0 0 .5.5h4a.5.5 0 0 0 .5-.5L11 4M6.5 6.5v5M9.5 6.5v5',
  x:        'm4 4 8 8M12 4l-8 8',
  pin:      'M8 14V9M5 9h6M5 9V3h6v6M6.5 5h3',
  castle:   'M2 14h12M3 14V6l1.5-1V3M6.5 14V8L8 7l1.5 1v6M13 14V6l-1.5-1V3M3 6h2M11 6h2',
  sword:    'm13 3-7 7M13 3l-1 4M13 3l-4 1M6 10l-3 3 1 1 3-3M6 10l1 1M9 13l1 1',
  flag:     'M3 14V2M3 3h8l-1.5 2.5L11 8H3',
  backlink: 'M9.5 6.5 6.5 9.5M10 4l1-1a2.5 2.5 0 0 1 3.5 3.5l-1 1M6 12l-1 1A2.5 2.5 0 0 1 1.5 9.5l1-1M2 2v3h3',
  undo:     'M3.5 7.5a5 5 0 1 1-1 3M3.5 7.5V4M3.5 7.5H7',
};
// icons needing extra geometry (circles/rects)
function customIcon(name, p) {
  switch (name) {
    case 'compass': return html`<svg ...${p}><circle cx="8" cy="8" r="6"/><path d="m10.5 5.5-1.6 3.4-3.4 1.6 1.6-3.4 3.4-1.6Z"/></svg>`;
    case 'mic':     return html`<svg ...${p}><rect x="6" y="2" width="4" height="8" rx="2"/><path d="M3.5 8a4.5 4.5 0 0 0 9 0M8 12.5V14M5.5 14h5"/></svg>`;
    case 'tag':     return html`<svg ...${p}><path d="M2 8V3a1 1 0 0 1 1-1h5l6 6-6 6-6-6Z"/><circle cx="5" cy="5" r=".7" fill="currentColor"/></svg>`;
    case 'users':   return html`<svg ...${p}><circle cx="6" cy="6" r="2.4"/><path d="M2 13c0-2.4 1.8-4 4-4s4 1.6 4 4M10 6.5a2 2 0 1 0 0-4M14 13c0-2-1.4-3.4-3-3.8"/></svg>`;
    case 'cog':     return html`<svg ...${p}><circle cx="8" cy="8" r="2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.3 3.3l1.4 1.4M11.3 11.3l1.4 1.4M3.3 12.7l1.4-1.4M11.3 4.7l1.4-1.4"/></svg>`;
    case 'search':  return html`<svg ...${p}><circle cx="7" cy="7" r="4.2"/><path d="m10.2 10.2 3 3"/></svg>`;
    case 'dots':    return html`<svg ...${p}><circle cx="3.5" cy="8" r=".9" fill="currentColor"/><circle cx="8" cy="8" r=".9" fill="currentColor"/><circle cx="12.5" cy="8" r=".9" fill="currentColor"/></svg>`;
    case 'time':    return html`<svg ...${p}><circle cx="8" cy="8" r="6"/><path d="M8 5v3l2 1.5"/></svg>`;
    case 'cal':     return html`<svg ...${p}><rect x="2.5" y="3.5" width="11" height="10" rx="1"/><path d="M2.5 6.5h11M5 2v3M11 2v3"/></svg>`;
    case 'play':    return html`<svg ...${p}><path d="m5 3 7 5-7 5V3Z" fill="currentColor"/></svg>`;
    case 'cpu':     return html`<svg ...${p}><rect x="3.5" y="3.5" width="9" height="9" rx="1"/><rect x="6" y="6" width="4" height="4"/><path d="M6 1.5v2M10 1.5v2M6 12.5v2M10 12.5v2M1.5 6h2M1.5 10h2M12.5 6h2M12.5 10h2"/></svg>`;
    case 'eye':     return html`<svg ...${p}><path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8s-2.5 4.5-6.5 4.5S1.5 8 1.5 8Z"/><circle cx="8" cy="8" r="1.8"/></svg>`;
    case 'archive': return html`<svg ...${p}><rect x="2" y="3" width="12" height="3"/><path d="M3 6v7.5a.5.5 0 0 0 .5.5h9a.5.5 0 0 0 .5-.5V6M6.5 9h3"/></svg>`;
    case 'sun':     return html`<svg ...${p}><circle cx="8" cy="8" r="3"/><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.5 1.5M11.5 11.5 13 13M3 13l1.5-1.5M11.5 4.5 13 3"/></svg>`;
    case 'globe':   return html`<svg ...${p}><circle cx="8" cy="8" r="6"/><path d="M2 8h12M8 2c2 2 3 4 3 6s-1 4-3 6c-2-2-3-4-3-6s1-4 3-6Z"/></svg>`;
    default:        return html`<svg ...${p}><circle cx="8" cy="8" r="4"/></svg>`;
  }
}
export function Icon({ name, size = 14, className = '', style = {} }) {
  const p = {
    width: size, height: size, viewBox: '0 0 16 16',
    stroke: 'currentColor', strokeWidth: 1.4, fill: 'none',
    strokeLinecap: 'round', strokeLinejoin: 'round',
    class: 'ck-ic ' + className,
    style: { flex: '0 0 auto', display: 'block', ...style },
  };
  const d = PATHS[name];
  if (d) return html`<svg ...${p}><path d=${d} /></svg>`;
  return customIcon(name, p);
}

// ── Sigil — initials in a tinted square ───────────────────────────
const SIGIL_SIZE = {
  md: { width: 32, height: 32, fontSize: 14, borderRadius: 4 },
  lg: { width: 44, height: 44, fontSize: 17, borderRadius: 8 },
  xl: { width: 64, height: 64, fontSize: 24, borderRadius: 8 },
};
function sigilTone(t) {
  const base = {
    display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
    fontFamily: 'var(--font-display)', fontWeight: 600,
    border: '1px solid var(--rule)', background: 'var(--paper-deep)',
    color: 'var(--ink-soft)', flex: '0 0 auto',
  };
  const map = {
    burgundy: { background: 'var(--burgundy-50)', color: 'var(--burgundy-700)', borderColor: 'rgba(122,46,31,.18)' },
    moss:     { background: 'var(--moss-50)', color: 'var(--moss)', borderColor: 'rgba(74,93,58,.22)' },
    blue:     { background: 'var(--ink-blue-50)', color: 'var(--ink-blue)', borderColor: 'rgba(53,83,112,.22)' },
    ochre:    { background: 'var(--ochre-50)', color: 'var(--ochre)', borderColor: 'rgba(168,115,40,.24)' },
    gilt:     { background: '#F0DFB1', color: '#6B5121', borderColor: '#D9C188' },
    ink:      { background: 'var(--ink)', color: '#F4ECD8', borderColor: 'var(--ink)' },
  };
  return { ...base, ...(map[t] || {}) };
}
export function Sigil({ ch, tone = 'default', size = 'md' }) {
  return html`<div style=${{ ...SIGIL_SIZE[size], ...sigilTone(tone) }}>${ch}</div>`;
}

// ── BrandMark ─────────────────────────────────────────────────────
export function BrandMark({ size = 28 }) {
  return html`<div style=${{
    width: size, height: size, background: 'var(--burgundy)', color: '#F2D9D2',
    borderRadius: 4, display: 'flex', alignItems: 'center', justifyContent: 'center',
    fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: size * 0.55,
    letterSpacing: '-0.02em', position: 'relative', flex: '0 0 auto',
    boxShadow: 'inset 0 0 0 1px rgba(255,255,255,.10)',
  }}>
    <svg width=${size * 0.55} height=${size * 0.55} viewBox="0 0 20 20" style=${{ position: 'absolute' }}>
      <path d="M14.5 6.5C13.5 5.5 12 5 10.5 5c-3 0-5 2-5 5s2 5 5 5c1.5 0 3-.5 4-1.5"
        stroke="#F2D9D2" stroke-width="1.8" stroke-linecap="round" fill="none" />
    </svg>
  </div>`;
}

// ── StagePill ─────────────────────────────────────────────────────
const STAGE_LABELS = { upload: 'Uploaded', transcribe: 'Transcribed', summarize: 'Summarized' };
export function StagePill({ stage, complete, current }) {
  const style = current
    ? { background: 'var(--burgundy-50)', color: 'var(--burgundy-700)', borderColor: 'rgba(122,46,31,.2)' }
    : complete
      ? { background: 'var(--moss-50)', color: 'var(--moss)', borderColor: 'rgba(74,93,58,.22)' }
      : { background: 'transparent', color: 'var(--ink-faint)', borderColor: 'var(--rule)' };
  const dot = current ? 'var(--burgundy)' : complete ? 'var(--moss)' : 'var(--ink-ghost)';
  return html`<span style=${{
    display: 'inline-flex', alignItems: 'center', gap: 5, padding: '2px 8px',
    borderRadius: 999, fontSize: 11, fontWeight: 500, border: '1px solid', ...style,
  }}>
    <span style=${{ width: 6, height: 6, borderRadius: '50%', background: dot }} />
    ${STAGE_LABELS[stage] || stage}
  </span>`;
}

// ── Pipeline — 4-step strip ───────────────────────────────────────
export function Pipeline({ stages }) {
  return html`<div style=${{ display: 'flex', alignItems: 'stretch', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden' }}>
    ${stages.map((s, i) => {
      const last = i === stages.length - 1;
      const tone = s.current ? { bg: 'var(--burgundy-50)', col: 'var(--burgundy-700)' }
        : s.done ? { bg: 'transparent', col: 'var(--moss)' }
        : { bg: 'transparent', col: 'var(--ink-faint)' };
      return html`<div key=${s.key} style=${{
        flex: 1, padding: '12px 16px', background: tone.bg,
        borderRight: last ? 'none' : '1px solid var(--rule-soft)',
        display: 'flex', flexDirection: 'column', gap: 4,
      }}>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 7, fontSize: 11, fontWeight: 600, letterSpacing: '0.08em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>
          <span style=${{ fontFamily: 'var(--font-mono)', color: tone.col }}>${String(i + 1).padStart(2, '0')}</span>
          <span class=${s.running ? 'ck-pulse' : ''} style=${{ color: tone.col }}>${s.label}</span>
          ${s.done && !s.current && html`<${Icon} name="check" size=${11} style=${{ marginLeft: 2, color: 'var(--moss)' }} />`}
        </div>
        <div style=${{ fontSize: 13, color: 'var(--ink)' }}>${s.detail}</div>
        ${s.meta && html`<div style=${{ fontSize: 11, color: 'var(--ink-muted)', fontFamily: 'var(--font-mono)' }}>${s.meta}</div>`}
      </div>`;
    })}
  </div>`;
}

// ── Button ────────────────────────────────────────────────────────
export function Btn({ kind = 'secondary', icon, iconRight, size, onClick, disabled, children, style = {}, title, type = 'button' }) {
  const base = {
    display: 'inline-flex', alignItems: 'center', gap: 6,
    padding: size === 'sm' ? '5px 9px' : '7px 12px',
    borderRadius: 4, fontSize: size === 'sm' ? 12 : 13, fontWeight: 500, lineHeight: 1,
    border: '1px solid transparent', whiteSpace: 'nowrap', cursor: disabled ? 'default' : 'pointer',
    opacity: disabled ? 0.5 : 1, transition: 'background .14s, border-color .14s',
  };
  const kinds = {
    primary:   { background: 'var(--burgundy)', color: '#FBF6E9', borderColor: 'var(--burgundy-700)' },
    secondary: { background: 'var(--surface)', color: 'var(--ink)', borderColor: 'var(--rule)' },
    ghost:     { color: 'var(--ink-soft)', background: 'transparent' },
    danger:    { color: 'var(--burgundy-700)', background: 'transparent' },
  };
  return html`<button type=${type} title=${title} disabled=${disabled}
    onClick=${disabled ? undefined : onClick}
    style=${{ ...base, ...kinds[kind], ...style }}>
    ${icon && html`<${Icon} name=${icon} size=${size === 'sm' ? 12 : 13} />`}
    ${children}
    ${iconRight && html`<${Icon} name=${iconRight} size=${size === 'sm' ? 12 : 13} />`}
  </button>`;
}

// Overflow "kebab" menu — groups secondary actions behind one ⋯ button so a
// toolbar shows only its primary action. `items`: {label, icon, onClick,
// danger, disabled, hidden}. `align` anchors the dropdown left|right (default right).
export function Menu({ items, align = 'right', title = 'More actions', label }) {
  const [open, setOpen] = useState(false);
  const ref = useRef(null);
  const visible = items.filter((it) => it && !it.hidden);
  useEffect(() => {
    if (!open) return undefined;
    const onDoc = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    const onKey = (e) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onKey);
    return () => { document.removeEventListener('mousedown', onDoc); document.removeEventListener('keydown', onKey); };
  }, [open]);
  if (!visible.length) return null;
  return html`<div ref=${ref} style=${{ position: 'relative', display: 'inline-flex' }}>
    <${Btn} kind="ghost" icon=${label ? undefined : 'dots'} iconRight=${label ? 'chev-d' : undefined} title=${title}
      onClick=${() => setOpen((v) => !v)}>${label || ''}</${Btn}>
    ${open && html`<div style=${{
      position: 'absolute', top: 'calc(100% + 4px)', [align]: 0, zIndex: 50, minWidth: 184,
      background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 8,
      boxShadow: 'var(--shadow-raised)', overflow: 'hidden', padding: 4,
    }}>
      ${visible.map((it, i) => html`<button key=${i} type="button" disabled=${it.disabled}
        onClick=${() => { if (it.disabled) return; setOpen(false); it.onClick?.(); }}
        style=${{
          display: 'flex', alignItems: 'center', gap: 9, width: '100%', textAlign: 'left',
          padding: '7px 9px', borderRadius: 5, border: 'none', background: 'transparent', cursor: it.disabled ? 'default' : 'pointer',
          fontSize: 13, fontWeight: 500, fontFamily: 'inherit', color: it.danger ? 'var(--burgundy-700)' : 'var(--ink)', opacity: it.disabled ? 0.45 : 1,
        }}
        onMouseEnter=${(e) => { if (!it.disabled) e.currentTarget.style.background = it.danger ? 'var(--burgundy-50)' : 'var(--paper-deep)'; }}
        onMouseLeave=${(e) => { e.currentTarget.style.background = 'transparent'; }}>
        ${it.icon && html`<${Icon} name=${it.icon} size=${13} />`}
        <span style=${{ flex: 1 }}>${it.label}</span>
      </button>`)}
    </div>`}
  </div>`;
}

// ── Context menu (Phase 14A) ──────────────────────────────────────
// One app-wide right-click menu: call openContextMenu(e, items) from any
// onContextMenu handler; ContextMenuHost (mounted once in main.js) renders it
// fixed-position so overflow:auto containers can't clip it. Items use the same
// shape as Menu ({label, icon, onClick, danger, disabled, hidden}); the string
// '-' is a separator. Calling it again replaces the open menu.
let cmShow = null;

export function openContextMenu(e, items) {
  e.preventDefault();
  e.stopPropagation();
  const list = (items || []).filter((it) => it && (it === '-' || !it.hidden));
  while (list[0] === '-') list.shift();
  while (list[list.length - 1] === '-') list.pop();
  if (list.length && cmShow) cmShow({ x: e.clientX, y: e.clientY, items: list });
}

// One menu row (top level or submenu child). Plain div, not <button> — a
// top-level row may contain its absolutely-positioned submenu.
function MenuItem({ it, active, onHover, onRun, children }) {
  return html`<div
    onClick=${(e) => { e.stopPropagation(); if (!it.disabled) onRun(); }}
    onMouseEnter=${onHover}
    style=${{
      position: 'relative', display: 'flex', alignItems: 'center', gap: 9,
      padding: '6px 9px', borderRadius: 5, cursor: it.disabled ? 'default' : 'pointer',
      background: active && !it.disabled ? (it.danger ? 'var(--burgundy-50)' : 'var(--paper-deep)') : 'transparent',
      fontSize: 13, fontWeight: 500, whiteSpace: 'nowrap',
      color: it.danger ? 'var(--burgundy-700)' : 'var(--ink)', opacity: it.disabled ? 0.45 : 1,
    }}>
    ${it.icon && html`<${Icon} name=${it.icon} size=${13} />`}
    <span style=${{ flex: 1 }}>${it.label}</span>
    ${it.children && html`<${Icon} name="chev-r" size=${11} className="ck-ink-faint" />`}
    ${children}
  </div>`;
}

export function ContextMenuHost() {
  const [menu, setMenu] = useState(null);
  const [idx, setIdx] = useState(-1);
  const [sub, setSub] = useState(null);      // top-level index whose submenu is open
  const [subIdx, setSubIdx] = useState(-1);  // highlighted child in the open submenu
  const ref = useRef(null);
  useEffect(() => {
    cmShow = (m) => { setMenu(m); setIdx(-1); setSub(null); setSubIdx(-1); };
    return () => { cmShow = null; };
  }, []);
  // Clamp into the viewport once the rendered size is known.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !menu) return;
    const r = el.getBoundingClientRect();
    const x = Math.max(8, Math.min(menu.x, window.innerWidth - r.width - 8));
    const y = Math.max(8, Math.min(menu.y, window.innerHeight - r.height - 8));
    if (x !== menu.x || y !== menu.y) setMenu({ ...menu, x, y });
  }, [menu]);
  useEffect(() => {
    if (!menu) return undefined;
    const close = () => setMenu(null);
    const onDown = (e) => { if (ref.current && !ref.current.contains(e.target)) close(); };
    const enabledOf = (items) => items.map((it, i) => (it !== '-' && !it.disabled ? i : -1)).filter((i) => i >= 0);
    const step = (items, dir) => (prev) => {
      const en = enabledOf(items);
      if (!en.length) return prev;
      const pos = en.indexOf(prev);
      return en[((pos < 0 ? (dir > 0 ? -1 : 0) : pos) + dir + en.length) % en.length];
    };
    const onKey = (e) => {
      const subItems = sub != null ? menu.items[sub].children : null;
      if (e.key === 'Escape') {
        e.preventDefault();
        if (subItems) { setSub(null); setSubIdx(-1); } else close();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const dir = e.key === 'ArrowDown' ? 1 : -1;
        if (subItems) setSubIdx(step(subItems, dir)); else setIdx(step(menu.items, dir));
      } else if (e.key === 'ArrowRight' && !subItems && idx >= 0 && menu.items[idx].children) {
        e.preventDefault();
        setSub(idx); setSubIdx(0);
      } else if (e.key === 'ArrowLeft' && subItems) {
        e.preventDefault();
        setSub(null); setSubIdx(-1);
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (subItems && subIdx >= 0) { close(); subItems[subIdx].onClick?.(); }
        else if (idx >= 0 && menu.items[idx].children) { setSub(idx); setSubIdx(0); }
        else if (idx >= 0) { close(); menu.items[idx].onClick?.(); }
      }
    };
    document.addEventListener('mousedown', onDown, true);
    document.addEventListener('keydown', onKey, true);
    document.addEventListener('scroll', close, true);
    window.addEventListener('blur', close);
    window.addEventListener('resize', close);
    return () => {
      document.removeEventListener('mousedown', onDown, true);
      document.removeEventListener('keydown', onKey, true);
      document.removeEventListener('scroll', close, true);
      window.removeEventListener('blur', close);
      window.removeEventListener('resize', close);
    };
  }, [menu, idx, sub, subIdx]);
  if (!menu) return null;
  // Submenu flips to the left edge when the parent sits near the right border.
  const subSide = menu.x + 380 > window.innerWidth ? { right: 'calc(100% + 2px)' } : { left: 'calc(100% + 2px)' };
  const panelStyle = {
    background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 8,
    boxShadow: 'var(--shadow-raised)', padding: 4,
  };
  return html`<div ref=${ref} onContextMenu=${(e) => e.preventDefault()}
    style=${{ position: 'fixed', left: menu.x, top: menu.y, zIndex: 1000, minWidth: 196, ...panelStyle }}>
    ${menu.items.map((it, i) => it === '-'
      ? html`<div key=${i} style=${{ height: 1, background: 'var(--rule-soft)', margin: '4px 6px' }} />`
      : html`<${MenuItem} key=${i} it=${it} active=${i === idx}
          onHover=${() => { setIdx(it.disabled ? -1 : i); setSub(it.children && !it.disabled ? i : null); setSubIdx(-1); }}
          onRun=${() => {
            if (it.children) { setSub(i); setSubIdx(-1); return; }
            setMenu(null); it.onClick?.();
          }}>
          ${it.children && sub === i && html`<div style=${{ position: 'absolute', top: -5, zIndex: 1, minWidth: 168, ...subSide, ...panelStyle }}>
            ${it.children.map((c, j) => html`<${MenuItem} key=${j} it=${c} active=${j === subIdx}
              onHover=${() => setSubIdx(c.disabled ? -1 : j)}
              onRun=${() => { setMenu(null); c.onClick?.(); }} />`)}
          </div>`}
        </${MenuItem}>`)}
  </div>`;
}

// ── Field / Input / Textarea / Select ─────────────────────────────
const fieldBox = {
  width: '100%', background: 'var(--surface-raised)', border: '1px solid var(--rule)',
  borderRadius: 4, padding: '7px 10px', fontSize: 13, color: 'var(--ink)', fontFamily: 'inherit',
};
export function Field({ label, hint, children }) {
  return html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 4 }}>
    ${label && html`<label style=${{ fontSize: 12, fontWeight: 500, color: 'var(--ink-soft)' }}>${label}</label>`}
    ${children}
    ${hint && html`<div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', lineHeight: 1.4 }}>${hint}</div>`}
  </div>`;
}
export function Input({ value, onInput, placeholder, type = 'text', mono, style = {}, ...rest }) {
  return html`<input type=${type} value=${value ?? ''} placeholder=${placeholder}
    onInput=${(e) => onInput && onInput(e.target.value)}
    style=${{ ...fieldBox, fontFamily: mono ? 'var(--font-mono)' : 'inherit', ...style }} ...${rest} />`;
}
export function Textarea({ value, onInput, placeholder, rows = 4, style = {} }) {
  return html`<textarea rows=${rows} placeholder=${placeholder}
    onInput=${(e) => onInput && onInput(e.target.value)}
    style=${{ ...fieldBox, resize: 'vertical', lineHeight: 1.45, ...style }}>${value ?? ''}</textarea>`;
}
export function Select({ value, onChange, options, style = {} }) {
  // options: [{value,label}]
  return html`<select value=${value} onChange=${(e) => onChange && onChange(e.target.value)}
    style=${{ ...fieldBox, cursor: 'pointer', ...style }}>
    ${options.map((o) => html`<option key=${o.value} value=${o.value}>${o.label}</option>`)}
  </select>`;
}

// ── Card + chrome header ──────────────────────────────────────────
export function Card({ title, sub, right, children, bodyPad = true, style = {} }) {
  return html`<div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden', ...style }}>
    ${(title || right) && html`<div style=${{ padding: '12px 18px 10px', display: 'flex', alignItems: 'baseline', gap: 10, borderBottom: '1px solid var(--rule-soft)' }}>
      ${title && html`<h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 16, fontWeight: 500, color: 'var(--ink)' }}>${title}</h3>`}
      ${sub && html`<span style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>${sub}</span>`}
      <span style=${{ flex: 1 }} />
      ${right}
    </div>`}
    <div style=${{ padding: bodyPad ? '14px 18px' : 0 }}>${children}</div>
  </div>`;
}

// ── Spinner ───────────────────────────────────────────────────────
export function Spinner({ size = 16, color = 'var(--burgundy)' }) {
  return html`<svg class="ck-spin" width=${size} height=${size} viewBox="0 0 16 16" style=${{ display: 'block' }}>
    <circle cx="8" cy="8" r="6" stroke="var(--rule)" stroke-width="2" fill="none" />
    <path d="M8 2a6 6 0 0 1 6 6" stroke=${color} stroke-width="2" fill="none" stroke-linecap="round" />
  </svg>`;
}

// ── Empty state ───────────────────────────────────────────────────
export function Empty({ icon = 'scroll', title, children }) {
  return html`<div style=${{
    display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
    gap: 10, padding: '48px 24px', textAlign: 'center', color: 'var(--ink-muted)',
  }}>
    <div style=${{ width: 44, height: 44, borderRadius: 8, background: 'var(--paper-deep)', border: '1px solid var(--rule)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--ink-faint)' }}>
      <${Icon} name=${icon} size=${18} />
    </div>
    ${title && html`<div style=${{ fontFamily: 'var(--font-display)', fontSize: 16, fontWeight: 500, color: 'var(--ink-soft)' }}>${title}</div>`}
    <div style=${{ fontSize: 12.5, fontStyle: 'italic', fontFamily: 'var(--font-display)', maxWidth: 360 }}>${children}</div>
  </div>`;
}

// ── Tiny markdown → vnode renderer (summaries) ────────────────────
// `codex` (optional): [{ name, page }] — occurrences of these names in the
// rendered prose are wrapped as clickable links to the vault page.
export function Markdown({ text, codex }) {
  let out = mdToHtml(text || '');
  const hasCodex = Array.isArray(codex) && codex.length;
  if (hasCodex) out = linkifyCodex(out, codex);
  const onClick = hasCodex ? (e) => {
    const a = e.target && e.target.closest && e.target.closest('.ck-codex-link');
    if (a && a.dataset.page) { e.preventDefault(); navigate('page', { page: a.dataset.page }); }
  } : undefined;
  return html`<div class="ck-prose" onClick=${onClick} dangerouslySetInnerHTML=${{ __html: out }} />`;
}

function escapeRegex(s) { return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }

// Wrap codex name occurrences in <a class="ck-codex-link" data-page="PATH">.
// Longest-name-first (avoid partial overlaps), unicode word boundaries, and a
// tag-aware walk that skips text already inside <a>/<code>. Best-effort: if the
// engine lacks lookbehind/unicode props, return the prose unchanged.
function linkifyCodex(htmlStr, codex) {
  const entries = codex.filter((e) => e && e.name && e.name.trim() && e.page);
  if (!entries.length) return htmlStr;
  const sorted = [...entries].sort((a, b) => b.name.trim().length - a.name.trim().length);
  const byLower = new Map();
  for (const e of sorted) {
    const k = e.name.trim().toLowerCase();
    if (!byLower.has(k)) byLower.set(k, e.page);
  }
  let re;
  try {
    const pattern = sorted.map((e) => escapeRegex(e.name.trim())).join('|');
    re = new RegExp(`(?<![\\p{L}\\p{N}_])(${pattern})(?![\\p{L}\\p{N}_])`, 'giu');
  } catch (_) { return htmlStr; }
  const tokens = htmlStr.split(/(<[^>]+>)/);
  let skip = 0;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t.startsWith('<')) {
      if (/^<(a|code)[\s>]/i.test(t)) skip++;
      else if (/^<\/(a|code)>/i.test(t)) skip = Math.max(0, skip - 1);
      continue;
    }
    if (skip > 0 || !t) continue;
    tokens[i] = t.replace(re, (m) => {
      const page = byLower.get(m.toLowerCase());
      return page ? `<a class="ck-codex-link" data-page="${escapeHtml(page)}">${m}</a>` : m;
    });
  }
  return tokens.join('');
}
function escapeHtml(s) {
  // Escape quotes too: inline() drops escaped text into href="…" attributes, so
  // an unescaped " in a markdown link URL would break out of the attribute.
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}
// Undo marked's text escaping: resolveWikilinks runs on rendered HTML, so a
// [[Remnant's Fury]] arrives as [[Remnant&#39;s Fury]] and would never match
// a page title (and would double-escape on output).
function unescapeHtml(s) {
  return s.replace(/&quot;/g, '"').replace(/&#39;/g, "'")
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
}
function inline(s) {
  return escapeHtml(s)
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*])\*([^*]+)\*/g, '$1<em>$2</em>')
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
}
function mdToHtml(md) {
  const lines = md.replace(/\r\n/g, '\n').split('\n');
  const out = [];
  let inUl = false, inOl = false;
  const closeLists = () => { if (inUl) { out.push('</ul>'); inUl = false; } if (inOl) { out.push('</ol>'); inOl = false; } };
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (!line.trim()) { closeLists(); continue; }
    let m;
    if ((m = line.match(/^(#{1,6})\s+(.*)/))) { closeLists(); const lvl = Math.min(m[1].length, 3); out.push(`<h${lvl}>${inline(m[2])}</h${lvl}>`); continue; }
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(line)) { closeLists(); out.push('<hr/>'); continue; }
    if ((m = line.match(/^>\s?(.*)/))) { closeLists(); out.push(`<blockquote>${inline(m[1])}</blockquote>`); continue; }
    if ((m = line.match(/^[-*+]\s+(.*)/))) { if (inOl) { out.push('</ol>'); inOl = false; } if (!inUl) { out.push('<ul>'); inUl = true; } out.push(`<li>${inline(m[1])}</li>`); continue; }
    if ((m = line.match(/^\d+\.\s+(.*)/))) { if (inUl) { out.push('</ul>'); inUl = false; } if (!inOl) { out.push('<ol>'); inOl = true; } out.push(`<li>${inline(m[1])}</li>`); continue; }
    closeLists();
    out.push(`<p>${inline(line)}</p>`);
  }
  closeLists();
  return out.join('\n');
}

// ── Vault page renderer (marked GFM + CK post-process) ────────────
function stripFrontmatter(text) {
  const m = /^---\r?\n[\s\S]*?\r?\n---\r?\n?/.exec(text || '');
  return m ? text.slice(m[0].length) : (text || '');
}

function ckPostprocess(htmlStr) {
  // `[!type]-` starts collapsed, `[!type]+` starts open (Obsidian fold markers).
  return htmlStr.replace(/<blockquote>\s*<p>\s*\[!([a-zA-Z]+)\]([+-]?)([^\n<]*)(<br\s*\/?>)?/g,
    (_, type, fold, title, br) => {
      const t = title.trim();
      const head = t ? `<span class="ck-callout-title">${escapeHtml(t)}</span>${br || ''}` : '';
      return `<blockquote data-callout="${type.toLowerCase()}"${fold ? ` data-fold="${fold}"` : ''}><p>${head}`;
    });
}

// `==text==` → <mark> (Obsidian highlight). Token walk so <code>/<pre>/<a>
// content and tag attributes are never rewritten.
function markHighlights(htmlStr) {
  if (!htmlStr.includes('==')) return htmlStr;
  const tokens = htmlStr.split(/(<[^>]+>)/);
  let skip = 0;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t.startsWith('<')) {
      if (/^<(a|code|pre)[\s>]/i.test(t)) skip++;
      else if (/^<\/(a|code|pre)>/i.test(t)) skip = Math.max(0, skip - 1);
      continue;
    }
    if (skip > 0) continue;
    tokens[i] = t.replace(/==([^=\n]+)==/g, '<mark>$1</mark>');
  }
  return tokens.join('');
}

// Resolve [[Name]] / [[Name|Label]] / [[Name#Heading]] against the vault page
// list — by title or alias, case-insensitive; collisions pick the shortest
// path. Resolved links carry data-path (navigable); unresolved ones get
// data-name (offer to create). Walks tokens so we never rewrite inside
// existing <a>/<code>.
export function resolveWikilinks(htmlStr, pages) {
  if (!htmlStr.includes('[[')) return htmlStr;
  const byName = new Map();
  // NFC: macOS filenames are NFD, typed link text NFC — keys must agree.
  const norm = (s) => s.trim().toLowerCase().normalize('NFC');
  const add = (name, path) => {
    const key = norm(name);
    if (!key) return;
    const cur = byName.get(key);
    if (!cur || path.length < cur.length || (path.length === cur.length && path < cur)) byName.set(key, path);
  };
  (pages || []).forEach((p) => {
    if (!p || !p.title) return;
    add(p.title, p.path);
    (p.aliases || []).forEach((a) => add(a, p.path));
  });
  const tokens = htmlStr.split(/(<[^>]+>)/);
  let skip = 0;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t.startsWith('<')) {
      if (/^<(a|code|pre)[\s>]/i.test(t)) skip++;
      else if (/^<\/(a|code|pre)>/i.test(t)) skip = Math.max(0, skip - 1);
      continue;
    }
    if (skip > 0 || !t.includes('[[')) continue;
    tokens[i] = t.replace(/(!?)\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (m, bang, target, label) => {
      const raw = unescapeHtml(target.trim());
      if (label) label = unescapeHtml(label);
      // `![[file.ext]]` media embeds → <img>; PageBody fills src via blob fetch
      // (an <img src> can't carry the auth header).
      if (bang && /\.(?!md$)[A-Za-z0-9]+$/i.test(raw)) {
        const w = /^\d+$/.test((label || '').trim()) ? ` width="${(label || '').trim()}"` : '';
        return `<img class="ck-embed-img" data-ck-asset="${escapeHtml(raw)}" alt="${escapeHtml(raw)}"${w} loading="lazy" />`;
      }
      // Block refs ([[Page#^id]]) are a locked Drop — plain text.
      if (raw.includes('#^')) return bang + escapeHtml((label || raw).trim());
      const hash = raw.indexOf('#');
      const name = (hash < 0 ? raw : raw.slice(0, hash)).trim().replace(/\.md$/i, '');
      const anchor = hash < 0 ? '' : raw.slice(hash + 1).trim();
      const text = escapeHtml((label || raw).trim());
      const path = byName.get(norm(name));
      // `![[Note]]` / `![[Note#Heading]]` page transclusion (Phase 8A) —
      // placeholder span; PageBody fetches + renders the body (or heading slice).
      // A <span> because marked wraps the line in <p> (a <div> would break out).
      if (bang) {
        return path
          ? `<span class="ck-transclude" data-ck-embed="${escapeHtml(path)}"${anchor ? ` data-ck-anchor="${escapeHtml(anchor)}"` : ''}></span>`
          : `<a class="ck-wikilink ck-wikilink--broken" data-name="${escapeHtml(name)}">${text}</a>`;
      }
      return path
        ? `<a class="ck-wikilink" data-path="${escapeHtml(path)}"${anchor ? ` data-anchor="${escapeHtml(anchor)}"` : ''}>${text}</a>`
        : `<a class="ck-wikilink ck-wikilink--broken" data-name="${escapeHtml(name)}">${text}</a>`;
    });
  }
  return tokens.join('');
}

export function renderPageHtml(text, pages) {
  return resolveWikilinks(markHighlights(ckPostprocess(marked.parse(stripFrontmatter(text), { gfm: true }))), pages);
}

// Render a single markdown block (no frontmatter); used by the live-preview editor.
export function renderBlockHtml(text, pages) {
  return resolveWikilinks(markHighlights(ckPostprocess(marked.parse(text || '', { gfm: true }))), pages);
}

// Click handler shared by reading view + live preview: navigate resolved wikilinks
// (⌘-click → background tab, 15B), hand unresolved ones to onBroken (e.g. create
// a page). Also folds/unfolds callouts.
export function wikilinkClick(onBroken) {
  return (e) => {
    const a = e.target && e.target.closest && e.target.closest('.ck-wikilink');
    if (a) {
      e.preventDefault();
      if (a.dataset.path) {
        if (e.metaKey || e.ctrlKey) openInNewTab(a.dataset.path, { background: true });
        else navigate('page', { path: a.dataset.path });
      } else if (a.dataset.name && onBroken) onBroken(a.dataset.name);
      return;
    }
    const head = e.target && e.target.closest && e.target.closest('.ck-callout-head');
    if (head) head.closest('blockquote').classList.toggle('ck-collapsed');
  };
}

// Right-click on a resolved wikilink → open / open-in-new-tab (15B). Falls
// through (returns false) elsewhere so callers can layer their own menus.
export function wikilinkContextMenu(e) {
  const a = e.target && e.target.closest && e.target.closest('.ck-wikilink[data-path]');
  if (!a) return false;
  const path = a.dataset.path;
  openContextMenu(e, [
    { label: 'Open', icon: 'book', onClick: () => navigate('page', { path }) },
    { label: 'Open in new tab', icon: 'plus', onClick: () => openInNewTab(path) },
  ]);
  return true;
}

// ── Prose hydration: callout fold structure, ![[Page]] embeds, image blobs ──

// Restructure marked's callout blockquotes into head (clickable fold) + body.
// Secrets and `[!type]-` start collapsed.
function structureCallouts(scope) {
  for (const bq of scope.querySelectorAll('blockquote[data-callout]')) {
    if (bq.querySelector(':scope > .ck-callout-head')) continue;
    const type = bq.dataset.callout;
    const titleEl = bq.querySelector('.ck-callout-title');
    const title = titleEl ? titleEl.textContent : type.charAt(0).toUpperCase() + type.slice(1);
    if (titleEl) {
      const p = titleEl.closest('p');
      titleEl.remove();
      if (p) {
        while (p.firstChild && p.firstChild.nodeName === 'BR') p.firstChild.remove();
        if (!p.textContent.trim() && !p.children.length) p.remove();
      }
    }
    const body = document.createElement('div');
    body.className = 'ck-callout-body';
    while (bq.firstChild) body.appendChild(bq.firstChild);
    const head = document.createElement('div');
    head.className = 'ck-callout-head';
    const chev = document.createElement('span');
    chev.className = 'ck-callout-chev';
    const t = document.createElement('span');
    t.className = 'ck-callout-title';
    t.textContent = title;
    head.appendChild(chev);
    head.appendChild(t);
    bq.appendChild(head);
    bq.appendChild(body);
    const fold = bq.dataset.fold || '';
    if (fold === '-' || (type === 'secret' && fold !== '+')) bq.classList.add('ck-collapsed');
  }
}

// `![[Page#Heading]]`: the heading's section — from its line to the next
// heading of the same or higher level. Fences skipped. Null when not found.
function sliceHeading(md, anchor) {
  const want = anchor.trim().toLowerCase();
  const lines = md.split('\n');
  let start = -1, level = 0, fence = false;
  for (let i = 0; i < lines.length; i++) {
    if (/^\s*(```|~~~)/.test(lines[i])) { fence = !fence; continue; }
    if (fence) continue;
    const m = lines[i].match(/^(#{1,6})\s+(.+)/);
    if (!m) continue;
    if (start < 0) {
      if (m[2].replace(/[*_`]/g, '').trim().toLowerCase() === want) { start = i; level = m[1].length; }
    } else if (m[1].length <= level) {
      return lines.slice(start, i).join('\n');
    }
  }
  return start < 0 ? null : lines.slice(start).join('\n');
}

const EMBED_DEPTH = 3;

function embedNote(node, msg) {
  node.innerHTML = `<span class="ck-embed-note">${escapeHtml(msg)}</span>`;
}

// Fill ![[Page]] placeholders: fetch the page, render body (or heading slice)
// as prose, recurse for nested embeds. `seen` guards cycles per branch.
async function fillEmbeds(scope, pages, campaignId, depth, seen, done) {
  const nodes = [...scope.querySelectorAll('span[data-ck-embed]:not(.ck-filled)')];
  await Promise.all(nodes.map(async (node) => {
    node.classList.add('ck-filled');
    const path = node.dataset.ckEmbed;
    const anchor = node.dataset.ckAnchor || '';
    if (seen.has(path)) return embedNote(node, `Circular embed: ${path}`);
    if (depth >= EMBED_DEPTH) return embedNote(node, `Embed too deep: ${path}`);
    let page;
    try {
      page = await apiFetch(`/campaigns/${campaignId}/vault/pages/${encodeURI(path)}`);
    } catch (_) {
      return embedNote(node, `Couldn't load embed: ${path}`);
    }
    let body = splitDoc(page.content).body.replace(/^\s*#\s+.*\n+/, '');
    if (anchor) {
      const sliced = sliceHeading(body, anchor);
      if (sliced == null) return embedNote(node, `Heading "${anchor}" not found in ${page.title}`);
      body = sliced;
    }
    node.innerHTML =
      `<span class="ck-embed-src ck-wikilink" data-path="${escapeHtml(path)}">${escapeHtml(page.title)}${anchor ? ` › ${escapeHtml(anchor)}` : ''}</span>`
      + `<span class="ck-embed-body">${renderBlockHtml(body, pages)}</span>`;
    structureCallouts(node);
    await fillEmbeds(node, pages, campaignId, depth + 1, new Set([...seen, path]), done);
    done(node); // let the caller hydrate images inside this embed
  }));
}

// ```ck-query fenced blocks → live page lists (Phase 9C). The backend parses
// and evaluates; errors render inline so a typo'd query explains itself.
function fillQueries(scope, campaignId) {
  for (const code of scope.querySelectorAll('pre > code.language-ck-query, pre > code.language-query, pre > code.language-dataview')) {
    const pre = code.closest('pre');
    if (!pre || pre.classList.contains('ck-filled')) continue;
    pre.classList.add('ck-filled');
    const q = (code.textContent || '').split('\n').map((l) => l.trim()).filter(Boolean).join(' ');
    apiFetch(`/campaigns/${campaignId}/vault/query?q=${encodeURIComponent(q)}`)
      .then((r) => {
        const box = document.createElement('div');
        box.className = 'ck-query';
        if (r.error) {
          box.innerHTML = `<span class="ck-embed-note">${escapeHtml(r.error)}</span>`;
        } else {
          const hits = r.hits || [];
          box.innerHTML =
            `<div class="ck-query-head">${escapeHtml(q)} · ${hits.length} ${hits.length === 1 ? 'page' : 'pages'}</div>`
            + (hits.length
              ? `<ul>${hits.map((h) => `<li><a class="ck-wikilink" data-path="${escapeHtml(h.path)}">${escapeHtml(h.title)}</a>${h.summary ? `<span class="ck-query-sub"> — ${escapeHtml(h.summary)}</span>` : ''}</li>`).join('')}</ul>`
              : '<span class="ck-embed-note">No matches</span>');
        }
        pre.replaceWith(box);
      })
      .catch(() => {});
  }
}

// Swap ![[image]] placeholders' src for authenticated blob URLs.
function fillAssetImgs(scope, campaignId, urls, isDead) {
  for (const img of scope.querySelectorAll('img[data-ck-asset]:not(.ck-filled)')) {
    img.classList.add('ck-filled');
    apiBlob(`/campaigns/${campaignId}/vault/assets/${encodeURI(img.dataset.ckAsset)}`)
      .then((b) => {
        if (isDead()) return;
        const u = URL.createObjectURL(b);
        urls.push(u);
        img.src = u;
      })
      .catch(() => { img.classList.add('ck-embed-img--missing'); });
  }
}

const KIND_ICONS = { pc: 'sparkle', npc: 'users', place: 'map', faction: 'shield', item: 'gem', event: 'cal', lore: 'scroll' };

// Asset name (from `image:` frontmatter) → authenticated blob URL, revoked on unmount.
export function useAsset(cid, name) {
  const [url, setUrl] = useState(null);
  useEffect(() => {
    setUrl(null);
    if (!name) return undefined;
    let dead = false;
    let u = null;
    apiBlob(`/campaigns/${cid}/vault/assets/${encodeURI(name)}`)
      .then((b) => {
        if (dead) return;
        u = URL.createObjectURL(b);
        setUrl(u);
      })
      .catch(() => {});
    return () => { dead = true; if (u) URL.revokeObjectURL(u); };
  }, [cid, name]);
  return url;
}

// `image:`/`cover:` frontmatter value → bare asset name (strips ![[ ]] embeds).
export function bannerAsset(props) {
  const p = (props || []).find((x) => x.key === 'image' || x.key === 'cover');
  const v = p?.values?.[0];
  return v ? v.replace(/^!?\[\[/, '').replace(/\]\]$/, '').trim() || null : null;
}

export function WikilinkHoverCard({ path, pages, x, y }) {
  const p = (pages || []).find((pg) => pg.path === path);
  if (!p) return null;
  const left = Math.min(x + 14, (typeof window !== 'undefined' ? window.innerWidth : 1200) - 280);
  const top = y + 18;
  return html`<div style=${{
    position: 'fixed', left, top, zIndex: 300, width: 260, pointerEvents: 'none',
    background: 'var(--paper)', border: '1px solid var(--rule)', borderRadius: 6,
    padding: '9px 12px', boxShadow: '0 4px 20px rgba(0,0,0,0.13)',
  }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: p.summary ? 5 : 0 }}>
      <${Icon} name=${KIND_ICONS[p.kind] || 'doc'} size=${12} className="ck-ink-muted" />
      <span style=${{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 13, color: 'var(--ink)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>${p.title}</span>
    </div>
    ${p.summary && html`<div style=${{ fontSize: 11.5, color: 'var(--ink-soft)', lineHeight: 1.45, display: '-webkit-box', WebkitLineClamp: 3, WebkitBoxOrient: 'vertical', overflow: 'hidden' }}>${p.summary}</div>`}
  </div>`;
}

export function PageBody({ text, pages, onBroken }) {
  const ref = useRef(null);
  const [hover, setHover] = useState(null);
  const htmlStr = renderPageHtml(text, pages);
  useEffect(() => {
    const root = ref.current;
    const id = store.campaign?.campaign_id;
    if (!root || !id) return undefined;
    let dead = false;
    const urls = [];
    const isDead = () => dead;
    structureCallouts(root);
    fillAssetImgs(root, id, urls, isDead);
    fillQueries(root, id);
    fillEmbeds(root, pages, id, 0, new Set(), (node) => {
      if (dead) return;
      fillAssetImgs(node, id, urls, isDead);
      fillQueries(node, id);
    }).catch(() => {});
    return () => { dead = true; urls.forEach((u) => URL.revokeObjectURL(u)); };
  }, [htmlStr]);
  const onMouseOver = useCallback((e) => {
    const a = e.target?.closest?.('.ck-wikilink[data-path]');
    if (!a) { setHover(null); return; }
    const path = a.getAttribute('data-path');
    setHover((h) => (h?.path === path ? h : { path, x: e.clientX, y: e.clientY }));
  }, []);
  return html`<div style=${{ position: 'relative' }}>
    <div ref=${ref} class="ck-prose" onClick=${wikilinkClick(onBroken)}
      onContextMenu=${(e) => wikilinkContextMenu(e)}
      onMouseOver=${onMouseOver} onMouseLeave=${() => setHover(null)}
      dangerouslySetInnerHTML=${{ __html: htmlStr }} />
    ${hover && html`<${WikilinkHoverCard} path=${hover.path} pages=${pages} x=${hover.x} y=${hover.y} />`}
  </div>`;
}

// ── Frontmatter helpers (client-side; the .md file body is the source of truth) ──
const FM_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;

// Split a full .md file into { fm } (inner YAML text, no fences) and { body }.
export function splitDoc(content) {
  const m = FM_RE.exec(content || '');
  return m ? { fm: m[1], body: content.slice(m[0].length) } : { fm: '', body: content || '' };
}

export function joinDoc(fm, body) {
  const f = (fm || '').trim();
  const b = (body || '').replace(/^\n+/, '');
  return f ? `---\n${f}\n---\n\n${b}` : b;
}

// Parse flat-YAML frontmatter into [{ key, list, values }] for the Properties strip.
// Supports `k: v`, inline `k: [a, b]`, and block lists (`k:` then `- item` lines).
export function parseProps(fm) {
  const lines = (fm || '').replace(/\r\n/g, '\n').split('\n');
  const props = [];
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (!m) continue;
    const key = m[1];
    let rest = m[2].trim();
    if (rest.startsWith('[') && rest.endsWith(']')) {
      const values = rest.slice(1, -1).split(',').map((s) => s.trim().replace(/^["']|["']$/g, '')).filter(Boolean);
      props.push({ key, list: true, values });
    } else if (!rest) {
      const values = [];
      while (i + 1 < lines.length && /^\s*-\s+/.test(lines[i + 1])) {
        values.push(lines[++i].replace(/^\s*-\s+/, '').trim().replace(/^["']|["']$/g, ''));
      }
      props.push(values.length ? { key, list: true, values } : { key, list: false, values: [''] });
    } else {
      props.push({ key, list: false, values: [rest.replace(/^["']|["']$/g, '')] });
    }
  }
  return props;
}
