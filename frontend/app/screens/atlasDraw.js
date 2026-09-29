// Atlas annotations: freehand / line / box / circle / stamp / text / eraser,
// stored on the map doc in normalised coordinates. Strokes and labels keep a
// constant on-screen size (scaled by 1/zoom inside the map layer), like pins.
import { html, useState, useRef, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { Icon } from '../ui.js';
import { thinPoint, eraseAt, finishDrawing, clamp01 } from './atlasGeom.js';

export const STAMPS = ['castle', 'tower', 'house', 'mountain', 'tree', 'tent', 'anchor', 'cave', 'ruin', 'bridge', 'flag', 'flame'];
const SWATCHES = ['#7A2E1F', '#1F1813', '#355370', '#4A5D3A', '#A87328', '#FBF6E9'];
const WIDTHS = [2, 4, 8];
const TOOLS = [['pen', 'Pen'], ['line', 'Line'], ['rect', 'Box'], ['circle', 'Circle'], ['stamp', 'Stamp'], ['text', 'Text'], ['eraser', 'Erase']];
const SHAPES = new Set(['pen', 'line', 'rect', 'circle']);

const newId = () => `d${Date.now().toString(36)}${Math.random().toString(36).slice(2, 5)}`;

export function useDrawTools({ map, commit, view, W, H, screenToNorm }) {
  const [tool, setTool] = useState(null);
  const [color, setColor] = useState('#7A2E1F');
  const [width, setWidth] = useState(4);
  const [stamp, setStamp] = useState('tower');
  const [draft, setDraft] = useState(null);       // { kind, points } while dragging a shape
  const [override, setOverride] = useState(null); // { drawings, texts } live while erasing / dragging text
  const [brush, setBrush] = useState(null);       // { x, y, r } eraser ring (normalised centre, image-px radius)
  const [edit, setEdit] = useState(null);         // text editor state
  const gesture = useRef(null);
  const live = useRef({});
  live.current = { map, commit, view, W, H, screenToNorm, tool, color, width, stamp };

  // stable window listeners that always call the latest handlers
  const handlers = useRef({});
  const mv = useRef((e) => handlers.current.onMove(e)).current;
  const up = useRef((e) => handlers.current.onUp(e)).current;

  const pointOf = (e) => { const p = live.current.screenToNorm(e.clientX, e.clientY); return [p.x, p.y]; };
  const docOf = () => ({ drawings: live.current.map.drawings || [], texts: live.current.map.texts || [] });

  const erase = (g, p) => {
    const L = live.current;
    const radius = Math.max(8, L.width * 3) / L.view.zoom;
    const r = eraseAt(g.doc, p, radius, L.W, L.H, L.view.zoom);
    if (r.changed) { g.changed = true; g.doc = { drawings: r.drawings, texts: r.texts }; setOverride(g.doc); }
    setBrush({ x: p[0], y: p[1], r: radius });
  };

  const onMove = (e) => {
    const g = gesture.current, L = live.current;
    if (!g) return;
    const p = pointOf(e);
    if (g.kind === 'erase') return erase(g, p);
    if (g.kind === 'text-drag') {
      const dx = p[0] - g.start[0], dy = p[1] - g.start[1];
      if (!g.moved && Math.hypot(dx * L.W, dy * L.H) < 3 / L.view.zoom) return;
      g.moved = true;
      g.doc = { ...g.doc, texts: g.doc.texts.map((t) => (t.id === g.id ? { ...t, x: clamp01(g.orig[0] + dx), y: clamp01(g.orig[1] + dy) } : t)) };
      return setOverride(g.doc);
    }
    if (g.kind === 'pen') { if (thinPoint(g.points, p, 2 / L.view.zoom, L.W, L.H)) g.points.push(p); }
    else g.points = [g.points[0], p];
    setDraft({ kind: g.kind, points: [...g.points] });
  };

  const onUp = () => {
    window.removeEventListener('mousemove', mv);
    window.removeEventListener('mouseup', up);
    const g = gesture.current, L = live.current;
    gesture.current = null;
    if (!g) return;
    setDraft(null); setOverride(null); setBrush(null);
    if (g.kind === 'erase') {
      if (g.changed) L.commit({ ...L.map, drawings: g.doc.drawings, texts: g.doc.texts });
    } else if (g.kind === 'text-drag') {
      if (g.moved) L.commit({ ...L.map, texts: g.doc.texts });
      else { const t = g.doc.texts.find((x) => x.id === g.id); if (t) setEdit({ ...t }); }
    } else {
      const pts = finishDrawing(g.kind, g.points, 3 / L.view.zoom, L.W, L.H);
      if (pts) L.commit({ ...L.map, drawings: [...(L.map.drawings || []), { id: newId(), kind: g.kind, points: pts, color: L.color, width: L.width }] });
    }
  };

  handlers.current = { onMove, onUp };

  const begin = (g) => {
    gesture.current = g;
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', up);
  };

  // Returns true when the tool consumed the press (Shift-drag always pans).
  const down = (e) => {
    const L = live.current;
    if (!L.tool || e.shiftKey || !L.view) return false;
    const p = pointOf(e);
    if (SHAPES.has(L.tool)) { begin({ kind: L.tool, points: [p] }); setDraft({ kind: L.tool, points: [p] }); return true; }
    if (L.tool === 'stamp') {
      L.commit({ ...L.map, drawings: [...(L.map.drawings || []), { id: newId(), kind: 'stamp', points: [p], color: L.color, width: L.width * 6, icon: L.stamp }] });
      return true;
    }
    if (L.tool === 'eraser') { const g = { kind: 'erase', doc: docOf(), changed: false }; begin(g); erase(g, p); return true; }
    if (L.tool === 'text') {
      const el = e.target.closest?.('[data-text-id]');
      const t = el && (L.map.texts || []).find((x) => x.id === el.getAttribute('data-text-id'));
      if (t) begin({ kind: 'text-drag', id: t.id, start: p, orig: [t.x, t.y], moved: false, doc: docOf() });
      else setEdit({ id: null, x: p[0], y: p[1], text: '', size: 18, color: L.color, rotation: 0 });
      return true;
    }
    return false;
  };

  const saveText = (t) => {
    const m = live.current.map;
    const cur = m.texts || [];
    const text = t.text.trim();
    const clean = { id: t.id || newId(), x: t.x, y: t.y, text, size: t.size, color: t.color, rotation: t.rotation };
    const next = !text ? cur.filter((x) => x.id !== t.id) : t.id ? cur.map((x) => (x.id === t.id ? clean : x)) : [...cur, clean];
    if (JSON.stringify(next) !== JSON.stringify(cur)) live.current.commit({ ...m, texts: next });
    setEdit(null);
  };

  const deleteText = (id) => {
    const m = live.current.map;
    live.current.commit({ ...m, texts: (m.texts || []).filter((x) => x.id !== id) });
    setEdit(null);
  };

  // Esc: abandon the gesture / editor first, then put the tool down.
  const cancel = () => {
    if (gesture.current || edit) {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', up);
      gesture.current = null;
      setDraft(null); setOverride(null); setBrush(null); setEdit(null);
    } else setTool(null);
  };

  useEffect(() => () => { window.removeEventListener('mousemove', mv); window.removeEventListener('mouseup', up); }, []);

  return { tool, setTool, color, setColor, width, setWidth, stamp, setStamp, draft, override, brush, edit, setEdit, saveText, deleteText, down, cancel };
}

function Shape({ d, zoom, W, H }) {
  const sw = d.width / zoom;
  const p = d.points.map((q) => [q[0] * W, q[1] * H]);
  const common = { stroke: d.color, 'stroke-width': sw, fill: 'none', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' };
  if (d.kind === 'pen') {
    return p.length === 1
      ? html`<circle cx=${p[0][0]} cy=${p[0][1]} r=${sw / 2} fill=${d.color} />`
      : html`<polyline points=${p.map((q) => q.join(',')).join(' ')} ...${common} />`;
  }
  if (p.length < 2) return null;
  if (d.kind === 'line') return html`<line x1=${p[0][0]} y1=${p[0][1]} x2=${p[1][0]} y2=${p[1][1]} ...${common} />`;
  if (d.kind === 'rect') {
    return html`<rect x=${Math.min(p[0][0], p[1][0])} y=${Math.min(p[0][1], p[1][1])} width=${Math.abs(p[1][0] - p[0][0])} height=${Math.abs(p[1][1] - p[0][1])} ...${common} />`;
  }
  if (d.kind === 'circle') return html`<circle cx=${p[0][0]} cy=${p[0][1]} r=${Math.hypot(p[1][0] - p[0][0], p[1][1] - p[0][1])} ...${common} />`;
  return null;
}

// Drawn inside the image-space layer, under the pins.
export function DrawLayer({ drawings, texts, draft, brush, zoom, W, H, tool, color, width, onTextDblClick }) {
  const items = [...drawings, ...(draft ? [{ id: 'draft', ...draft, color, width }] : [])];
  const shapes = items.filter((d) => d.kind !== 'stamp');
  const stamps = drawings.filter((d) => d.kind === 'stamp');
  const textsLive = tool === null || tool === 'text';
  return html`<div style=${{ position: 'absolute', inset: 0, pointerEvents: 'none', zIndex: 12 }}>
    <svg width=${W} height=${H} style=${{ position: 'absolute', inset: 0, overflow: 'visible' }}>
      ${shapes.map((d) => html`<${Shape} key=${d.id} d=${d} zoom=${zoom} W=${W} H=${H} />`)}
      ${brush && html`<circle cx=${brush.x * W} cy=${brush.y * H} r=${brush.r} fill="rgba(251,246,233,.25)" stroke="#7A2E1F" stroke-width=${1 / zoom} stroke-dasharray=${`${3 / zoom} ${3 / zoom}`} />`}
    </svg>
    ${stamps.map((d) => html`<div key=${d.id} style=${{ position: 'absolute', left: `${d.points[0][0] * 100}%`, top: `${d.points[0][1] * 100}%`,
      transform: `translate(-50%,-50%) scale(${1 / zoom})`, color: d.color, display: 'flex', filter: 'drop-shadow(0 1px 0 rgba(251,246,233,.8))' }}>
      <${Icon} name=${d.icon || 'pin'} size=${d.width} />
    </div>`)}
    ${texts.map((t) => html`<div key=${t.id} data-text-id=${t.id} onDblClick=${() => onTextDblClick(t)} style=${{
      position: 'absolute', left: `${t.x * 100}%`, top: `${t.y * 100}%`, transform: `translate(-50%,-50%) rotate(${t.rotation || 0}deg) scale(${1 / zoom})`,
      transformOrigin: 'center', whiteSpace: 'pre', fontFamily: 'var(--font-display)', fontWeight: 500, fontSize: t.size, lineHeight: 1.15, color: t.color,
      textShadow: '0 0 3px rgba(251,246,233,.95), 0 0 6px rgba(251,246,233,.7)', userSelect: 'none',
      pointerEvents: textsLive ? 'auto' : 'none', cursor: tool === 'text' ? 'move' : 'default' }}>${t.text}</div>`)}
  </div>`;
}

const chip = (active) => ({ fontSize: 12, padding: '4px 10px', borderRadius: 999, cursor: 'pointer', whiteSpace: 'nowrap',
  background: active ? 'var(--burgundy)' : 'var(--surface)', color: active ? '#FBF6E9' : 'var(--ink-soft)',
  border: `1px solid ${active ? 'var(--burgundy-700)' : 'var(--rule)'}` });

export function DrawPalette({ draw, onPick, history }) {
  return html`<div onMouseDown=${(e) => e.stopPropagation()} style=${{ position: 'absolute', bottom: 16, left: 56, zIndex: 89, maxWidth: 'calc(100% - 320px)',
    display: 'flex', flexDirection: 'column', gap: 8, padding: '9px 11px', background: 'var(--surface-raised)', border: '1px solid var(--rule)',
    borderRadius: 10, boxShadow: 'var(--shadow-card)' }}>
    <div style=${{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
      ${TOOLS.map(([id, label]) => html`<button key=${id} onClick=${() => onPick(draw.tool === id ? null : id)} style=${chip(draw.tool === id)}>${label}</button>`)}
      <span style=${{ flex: 1 }} />
      <button onClick=${() => history.step(false)} disabled=${!history.canUndo} title="Undo (⌘Z)" style=${{ ...chip(false), opacity: history.canUndo ? 1 : 0.4 }}><${Icon} name="undo" size=${11} /></button>
      <button onClick=${() => history.step(true)} disabled=${!history.canRedo} title="Redo (⇧⌘Z)" style=${{ ...chip(false), opacity: history.canRedo ? 1 : 0.4, transform: 'scaleX(-1)' }}><${Icon} name="undo" size=${11} /></button>
    </div>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 7, flexWrap: 'wrap' }}>
      ${SWATCHES.map((c) => html`<button key=${c} onClick=${() => draw.setColor(c)} title=${c} style=${{ width: 18, height: 18, borderRadius: '50%', cursor: 'pointer', background: c,
        border: draw.color.toLowerCase() === c.toLowerCase() ? '2px solid var(--ink)' : '1px solid var(--rule-strong)' }} />`)}
      <input type="color" value=${draw.color.length === 7 ? draw.color : '#7a2e1f'} onInput=${(e) => draw.setColor(e.target.value)}
        style=${{ width: 22, height: 22, padding: 0, border: 'none', background: 'none', cursor: 'pointer' }} />
      <span style=${{ width: 1, height: 16, background: 'var(--rule)' }} />
      ${WIDTHS.map((w) => html`<button key=${w} onClick=${() => draw.setWidth(w)} title=${`${w}px`} style=${{ ...chip(draw.width === w), padding: '4px 9px' }}>${w === 2 ? 'Thin' : w === 4 ? 'Mid' : 'Thick'}</button>`)}
    </div>
    ${draw.tool === 'stamp' && html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
      ${STAMPS.map((s) => html`<button key=${s} onClick=${() => draw.setStamp(s)} title=${s} style=${{ width: 28, height: 28, display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer', borderRadius: 6,
        background: draw.stamp === s ? 'var(--burgundy-50)' : 'transparent', border: `1px solid ${draw.stamp === s ? 'var(--burgundy-300)' : 'var(--rule-soft)'}`, color: 'var(--ink-soft)' }}><${Icon} name=${s} size=${15} /></button>`)}
    </div>`}
    <div style=${{ fontSize: 11, color: 'var(--ink-faint)', fontStyle: 'italic', fontFamily: 'var(--font-display)' }}>
      ${draw.tool ? 'Shift-drag pans · Esc puts the tool down' : 'Pick a tool, then draw on the map.'}</div>
  </div>`;
}

const field = { fontSize: 12.5, padding: '3px 6px', border: '1px solid var(--rule)', borderRadius: 4, background: 'var(--surface)', color: 'var(--ink)' };

// Popover at the label's spot: text, size, rotation, colour.
export function TextEditor({ edit, setEdit, view, W, H, onSave, onDelete, onClose }) {
  if (!edit || !view) return null;
  const set = (patch) => setEdit({ ...edit, ...patch });
  return html`<div onMouseDown=${(e) => e.stopPropagation()} style=${{ position: 'absolute', zIndex: 96, left: view.tx + edit.x * W * view.zoom, top: view.ty + edit.y * H * view.zoom + 16,
    transform: 'translateX(-50%)', width: 250, padding: 10, display: 'flex', flexDirection: 'column', gap: 8, background: 'var(--surface-raised)',
    border: '1px solid var(--rule-strong)', borderRadius: 10, boxShadow: 'var(--shadow-raised)' }}>
    <input autoFocus value=${edit.text} placeholder="Label text" onInput=${(e) => set({ text: e.target.value })}
      onKeyDown=${(e) => { if (e.key === 'Enter') onSave(edit); }} style=${{ ...field, fontSize: 14, fontFamily: 'var(--font-display)' }} />
    <div style=${{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 11.5, color: 'var(--ink-muted)' }}>
      Size <input type="number" min="6" max="120" value=${edit.size} onInput=${(e) => set({ size: Math.min(120, Math.max(6, +e.target.value || 18)) })} style=${{ ...field, width: 54 }} />
      <input type="color" value=${edit.color.length === 7 ? edit.color : '#1f1813'} onInput=${(e) => set({ color: e.target.value })} style=${{ width: 24, height: 22, padding: 0, border: 'none', background: 'none' }} />
    </div>
    <label style=${{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 11.5, color: 'var(--ink-muted)' }}>Rotate
      <input type="range" min="-180" max="180" value=${edit.rotation} onInput=${(e) => set({ rotation: +e.target.value })} style=${{ flex: 1 }} />
      <span style=${{ width: 34, textAlign: 'right' }}>${Math.round(edit.rotation)}°</span></label>
    <div style=${{ display: 'flex', gap: 6 }}>
      ${edit.id && html`<button onClick=${() => onDelete(edit.id)} style=${{ ...chip(false), color: 'var(--burgundy-700)' }}><${Icon} name="trash" size=${11} /> Delete</button>`}
      <span style=${{ flex: 1 }} />
      <button onClick=${onClose} style=${chip(false)}>Cancel</button>
      <button onClick=${() => onSave(edit)} style=${chip(true)}>Done</button>
    </div>
  </div>`;
}
