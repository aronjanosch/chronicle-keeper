// Shared atoms ported from the design's atoms.jsx + a few form/primitive helpers.
import { html } from '../vendor/htm-preact-standalone.mjs';
import { navigate } from './core.js';

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
  'arrow-r':'M3 8h10M9 4l4 4-4 4',
  upload:   'M8 11V3M5 6l3-3 3 3M3 13h10',
  download: 'M8 3v8M5 8l3 3 3-3M3 13h10',
  export:   'M8 2v8M5 5l3-3 3 3M3 9v4h10V9',
  search:   null,
  filter:   'M2 3h12L9 9v4l-2 1V9L2 3Z',
  dots:     null,
  edit:     'm3 13 1-3 7-7 2 2-7 7-3 1ZM10 4l2 2',
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
// `codex` (optional): [{ name, entry_id }] — occurrences of these names in the
// rendered prose are wrapped as clickable links to the entry-detail view.
export function Markdown({ text, codex }) {
  let out = mdToHtml(text || '');
  const hasCodex = Array.isArray(codex) && codex.length;
  if (hasCodex) out = linkifyCodex(out, codex);
  const onClick = hasCodex ? (e) => {
    const a = e.target && e.target.closest && e.target.closest('.ck-codex-link');
    if (a && a.dataset.entry) { e.preventDefault(); navigate('codexEntry', { entryId: a.dataset.entry }); }
  } : undefined;
  return html`<div class="ck-prose" onClick=${onClick} dangerouslySetInnerHTML=${{ __html: out }} />`;
}

function escapeRegex(s) { return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }

// Wrap codex name occurrences in <a class="ck-codex-link" data-entry="ID">.
// Longest-name-first (avoid partial overlaps), unicode word boundaries, and a
// tag-aware walk that skips text already inside <a>/<code>. Best-effort: if the
// engine lacks lookbehind/unicode props, return the prose unchanged.
function linkifyCodex(htmlStr, codex) {
  const entries = codex.filter((e) => e && e.name && e.name.trim());
  if (!entries.length) return htmlStr;
  const sorted = [...entries].sort((a, b) => b.name.trim().length - a.name.trim().length);
  const byLower = new Map();
  for (const e of sorted) {
    const k = e.name.trim().toLowerCase();
    if (!byLower.has(k)) byLower.set(k, e.entry_id);
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
      const id = byLower.get(m.toLowerCase());
      return id ? `<a class="ck-codex-link" data-entry="${id}">${m}</a>` : m;
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
