// Pure helpers for per-world prefs (pinned pages, dismissed "Unfinished" rows).
export const togglePinned = (pinned, path) =>
  pinned.includes(path) ? pinned.filter((p) => p !== path) : [...pinned, path];

// Move `path` so it sits at `index` of the resulting list (append when omitted).
export function movePinned(pinned, path, index) {
  const rest = pinned.filter((p) => p !== path);
  const at = index == null ? rest.length : Math.max(0, Math.min(index, rest.length));
  rest.splice(at, 0, path);
  return rest;
}

// Rename/move cascade: `from` is a page path or a folder prefix.
export function remapPaths(list, from, to) {
  const map = (p) => (p === from ? to : p.startsWith(`${from}/`) ? to + p.slice(from.length) : p);
  return [...new Set(list.map(map))];
}

export const GAP_TABS = ['all', 'stubs', 'open', 'links'];

export function gapMatches(row, tab) {
  if (tab === 'stubs') return row.row === 'page' && row.stub;
  if (tab === 'open') return row.row === 'page' && row.open_questions > 0;
  if (tab === 'links') return row.row === 'link';
  return true;
}

export function gapCounts(rows, dismissed) {
  const live = rows.filter((r) => !dismissed.includes(r.key));
  return Object.fromEntries(GAP_TABS.map((t) => [t, live.filter((r) => gapMatches(r, t)).length]));
}

export const visibleGaps = (rows, dismissed, tab) =>
  rows.filter((r) => !dismissed.includes(r.key) && gapMatches(r, tab));

// Which skill fits a row: stubs get the kind's "Flesh out" skill when one exists.
export function skillForGap(row, skills) {
  const kind = String(row.kind || '').toLowerCase();
  const fits = (skills || []).filter((s) => s.enabled !== false && (s.kinds || []).some((k) => String(k).toLowerCase() === kind));
  return fits.find((s) => /^flesh out/i.test(s.name)) || null;
}
