// Tiny persisted boolean layout flags shared across components (rail compact,
// vault panel open). One source per key so a hotkey and its button stay in sync.
import { useState, useEffect } from '../vendor/htm-preact-standalone.mjs';

const subs = new Map();

export function getFlag(key, dflt) {
  try {
    const v = localStorage.getItem(key);
    return v == null ? dflt : v === '1';
  } catch (_) { return dflt; }
}
export function setFlag(key, v) {
  try { localStorage.setItem(key, v ? '1' : '0'); } catch (_) { /* private mode */ }
  (subs.get(key) || []).forEach((fn) => fn(v));
}
export function toggleFlag(key, dflt) { setFlag(key, !getFlag(key, dflt)); }

export function useFlag(key, dflt) {
  const [v, setV] = useState(() => getFlag(key, dflt));
  useEffect(() => {
    if (!subs.has(key)) subs.set(key, new Set());
    subs.get(key).add(setV);
    setV(getFlag(key, dflt));
    return () => subs.get(key).delete(setV);
  }, [key]);
  return v;
}

export const RAIL_COMPACT = 'ck_rail_compact';
export const PANEL_OPEN = 'ck_panel_open';
