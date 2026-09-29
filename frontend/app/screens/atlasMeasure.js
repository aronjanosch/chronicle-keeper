// Atlas ruler: a polyline measured against the map's scale, with travel time.
// A scale is `{ width, unit }` — the full image width spans `width` units, so
// distances need only the image's pixel aspect (W × H), not a grid.
import { html, useState } from '../../vendor/htm-preact-standalone.mjs';
import { Icon } from '../ui.js';

const KM_PER = { km: 1, mi: 1.609344, league: 4.828032, m: 0.001 };
export const UNITS = Object.keys(KM_PER);

// km per day — rough overland/sea rules of thumb, not a simulation
export const PACES = [
  { id: 'foot', label: 'On foot', kmPerDay: 30 },
  { id: 'mounted', label: 'Mounted', kmPerDay: 50 },
  { id: 'wagon', label: 'Wagon', kmPerDay: 25 },
  { id: 'ship', label: 'Sailing ship', kmPerDay: 100 },
];

export function pathPixels(pts, W, H) {
  let total = 0;
  for (let i = 1; i < pts.length; i++) {
    total += Math.hypot((pts[i].x - pts[i - 1].x) * W, (pts[i].y - pts[i - 1].y) * H);
  }
  return total;
}

// Real length of the path in the scale's unit; null without a scale.
export function pathLength(pts, W, H, scale) {
  if (!scale || !(scale.width > 0) || pts.length < 2) return null;
  return (pathPixels(pts, W, H) / W) * scale.width;
}

export function travelDays(distance, unit, kmPerDay) {
  const per = KM_PER[unit];
  return per ? (distance * per) / kmPerDay : null;
}

export const fmtDistance = (d, unit) => `${d >= 100 ? Math.round(d) : Math.round(d * 10) / 10} ${unit}`;

export function fmtDays(days) {
  if (days == null) return '—';
  if (days < 0.1) return '< 1 hour';
  if (days < 1) return `${Math.round(days * 24)} hours`;
  return `${Math.round(days * 10) / 10} day${Math.abs(days - 1) < 0.05 ? '' : 's'}`;
}

// Width that makes `pts` span `real` units — used to calibrate on a known distance.
export function scaleFromPath(pts, W, H, real) {
  const px = pathPixels(pts, W, H);
  return px > 0 && real > 0 ? (real * W) / px : null;
}

// Drawn inside the map layer (image space); strokes stay 2px on screen at any zoom.
export function MeasureLayer({ pts, hover, W, H, zoom, active }) {
  const all = active && hover && pts.length ? [...pts, hover] : pts;
  if (!all.length) return null;
  const px = all.map((p) => `${p.x * W},${p.y * H}`);
  const w = 2 / zoom;
  return html`<svg width=${W} height=${H} style=${{ position: 'absolute', inset: 0, pointerEvents: 'none', overflow: 'visible', zIndex: 15 }}>
    <polyline points=${px.join(' ')} fill="none" stroke="rgba(251,246,233,.9)" stroke-width=${w * 3.4} stroke-linecap="round" stroke-linejoin="round" />
    <polyline points=${px.join(' ')} fill="none" stroke="#7A2E1F" stroke-width=${w} stroke-dasharray=${`${6 / zoom} ${4 / zoom}`} stroke-linecap="round" stroke-linejoin="round" />
    ${pts.map((p, i) => html`<circle key=${i} cx=${p.x * W} cy=${p.y * H} r=${4 / zoom} fill="#FBF6E9" stroke="#7A2E1F" stroke-width=${w} />`)}
  </svg>`;
}

const chip = { fontSize: 11.5, padding: '3px 9px', borderRadius: 999, border: '1px solid var(--rule)', background: 'transparent', color: 'var(--ink-soft)', cursor: 'pointer' };

export function MeasureReadout({ pts, W, H, scale, onCalibrate, onUndo, onClear, onClose }) {
  const [form, setForm] = useState(false);
  const [real, setReal] = useState('');
  const [unit, setUnit] = useState(scale?.unit || 'km');
  const dist = pathLength(pts, W, H, scale);
  const ready = pts.length >= 2;
  const submit = () => {
    const w = scaleFromPath(pts, W, H, parseFloat(real));
    if (w) { onCalibrate({ width: w, unit }); setForm(false); setReal(''); }
  };
  return html`<div onMouseDown=${(e) => e.stopPropagation()} style=${{ position: 'absolute', top: 16, left: '50%', transform: 'translateX(-50%)', zIndex: 95,
    minWidth: 250, padding: '10px 14px', borderRadius: 10, background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)',
    boxShadow: 'var(--shadow-raised)', display: 'flex', flexDirection: 'column', gap: 8, fontSize: 13 }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <${Icon} name="ruler" size=${14} style=${{ color: 'var(--burgundy)' }} />
      <b style=${{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 15 }}>${dist != null ? fmtDistance(dist, scale.unit) : ready ? 'No scale yet' : 'Click the map to start'}</b>
      <span style=${{ flex: 1 }} />
      <button onClick=${onClose} style=${{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ink-muted)' }}><${Icon} name="x" size=${13} /></button>
    </div>
    ${dist != null && html`<div style=${{ display: 'grid', gridTemplateColumns: 'auto auto', gap: '2px 16px', color: 'var(--ink-soft)' }}>
      ${PACES.map((p) => html`<${Row} key=${p.id} label=${p.label} value=${fmtDays(travelDays(dist, scale.unit, p.kmPerDay))} />`)}
    </div>`}
    ${dist == null && ready && !form && html`<div style=${{ color: 'var(--ink-muted)', fontSize: 12.5 }}>Set a scale to read distances and travel time.</div>`}
    ${form && html`<div style=${{ display: 'flex', gap: 6, alignItems: 'center' }}>
      <input type="number" min="0" step="any" value=${real} placeholder="real length" onInput=${(e) => setReal(e.target.value)}
        style=${{ width: 96, fontSize: 12.5, padding: '3px 6px', border: '1px solid var(--rule)', borderRadius: 4 }} />
      <select value=${unit} onChange=${(e) => setUnit(e.target.value)} style=${{ fontSize: 12.5, padding: '3px 4px', border: '1px solid var(--rule)', borderRadius: 4 }}>
        ${UNITS.map((u) => html`<option key=${u} value=${u}>${u}</option>`)}
      </select>
      <button onClick=${submit} style=${{ ...chip, background: 'var(--burgundy)', color: '#FBF6E9', border: '1px solid var(--burgundy-700)' }}>Set</button>
    </div>`}
    <div style=${{ display: 'flex', gap: 6 }}>
      ${ready && html`<button onClick=${() => setForm((f) => !f)} style=${chip}>${scale ? 'Recalibrate' : 'Set scale'}</button>`}
      ${pts.length > 0 && html`<button onClick=${onUndo} style=${chip}>Undo point</button>`}
      ${pts.length > 0 && html`<button onClick=${onClear} style=${chip}>Clear</button>`}
    </div>
    ${form && html`<div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', fontStyle: 'italic', fontFamily: 'var(--font-display)' }}>
      Enter the real length of the line you drew — e.g. between two towns whose distance you know.</div>`}
  </div>`;
}

function Row({ label, value }) {
  return html`<span>${label}</span><span style=${{ textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>${value}</span>`;
}
