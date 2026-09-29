// Pure logic for Atlas preview cards: layout clamping and the hover/modifier
// state machine. No imports — unit-testable in node.

export const MIN_W = 220;
export const MIN_H = 140;
export const DEFAULT_SIZE = { w: 320, h: 340 };
export const MAX_PINNED = 8;

const num = (v, d) => (Number.isFinite(v) ? v : d);

// Keep a card inside the stage (screen px), enforcing minimum and maximum size.
export function clampRect(r, stage) {
  const w = Math.min(Math.max(num(r.w, DEFAULT_SIZE.w), MIN_W), Math.max(MIN_W, stage.w));
  const h = Math.min(Math.max(num(r.h, DEFAULT_SIZE.h), MIN_H), Math.max(MIN_H, stage.h));
  const x = Math.min(Math.max(num(r.x, 0), 0), Math.max(0, stage.w - w));
  const y = Math.min(Math.max(num(r.y, 0), 0), Math.max(0, stage.h - h));
  return { x, y, w, h };
}

// Where an unpinned card opens: beside the pin, on whichever side has room.
export function placeNear(px, py, stage, size = DEFAULT_SIZE) {
  const gap = 30;
  const x = px + gap + size.w > stage.w ? px - gap - size.w : px + gap;
  return clampRect({ x, y: py - size.h / 2, ...size }, stage);
}

// Saved cards → drawable ones: only pins that still exist, one per pin, clamped.
export function normalizePreviews(list, pinIds, stage, cap = MAX_PINNED) {
  const ids = new Set(pinIds);
  const seen = new Set();
  const out = [];
  for (const v of list || []) {
    if (!v || !ids.has(v.pin_id) || seen.has(v.pin_id)) continue;
    seen.add(v.pin_id);
    out.push({ pin_id: v.pin_id, ...clampRect(v, stage) });
    if (out.length === cap) break;
  }
  return out;
}

// Add, replace or drop the saved card of one pin.
export function withPreview(list, pinId, rect) {
  const rest = (list || []).filter((v) => v.pin_id !== pinId);
  if (!rect) return rest;
  const r = { pin_id: pinId, x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.w), h: Math.round(rect.h) };
  return [...rest, r].slice(-MAX_PINNED);
}

// ── hover + modifier ──────────────────────────────────────────────
// A card is wanted while the modifier is down and the pointer is over the pin
// or over the card itself; the pin remembered so pressing the modifier after
// hovering still opens it.
export const initialKeys = () => ({ mod: false, hover: null, card: null });

export function keysReduce(s, ev) {
  switch (ev.type) {
    case 'mod': return s.mod === ev.on ? s : { ...s, mod: ev.on };
    case 'hover': return s.hover === ev.id ? s : { ...s, hover: ev.id };
    case 'card': return s.card === ev.id ? s : { ...s, card: ev.id };
    case 'blur': return initialKeys();
    default: return s;
  }
}

export const previewTarget = (s) => (s.mod ? (s.hover ?? s.card) : null);
