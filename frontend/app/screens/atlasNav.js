// Wheel/trackpad navigation for the Atlas stage. No imports — unit-testable in node.
//
// A mouse wheel should zoom; a trackpad's two-finger scroll should pan and its
// pinch (delivered as ctrl+wheel) should zoom. Browsers don't say which device
// sent the event, so 'auto' guesses from the delta shape and can be overridden.

export const NAV_MODES = ['auto', 'mouse', 'trackpad'];

const ZOOM_PER_PX = 0.0014;
const PINCH_PER_PX = 0.01;

// A notched mouse wheel reports whole, large steps on the vertical axis only.
function looksLikeMouseWheel(e) {
  if (e.deltaMode !== 0) return true; // line/page units only come from wheels
  if (e.deltaX !== 0) return false;
  return Number.isInteger(e.deltaY) && Math.abs(e.deltaY) >= 50;
}

// -> { type: 'zoom', factor } | { type: 'pan', dx, dy }  (pan is in screen px to add to the view offset)
export function wheelAction(e, mode = 'auto') {
  if (e.ctrlKey) return { type: 'zoom', factor: Math.exp(-e.deltaY * PINCH_PER_PX) };
  const mouse = mode === 'mouse' || (mode === 'auto' && looksLikeMouseWheel(e));
  if (mouse) return { type: 'zoom', factor: Math.exp(-e.deltaY * ZOOM_PER_PX) };
  return { type: 'pan', dx: -e.deltaX, dy: -e.deltaY };
}

// Single-key tool shortcuts. Returns the action name or null.
export const SHORTCUTS = [
  ['P', 'pin', 'Place a pin'],
  ['M', 'measure', 'Measure distance'],
  ['D', 'draw', 'Draw on the map'],
  ['R', 'region', 'Draw a region'],
  ['F', 'fit', 'Fit the map'],
  ['Esc', null, 'Leave the current tool'],
  ['⌘Z / ⇧⌘Z', null, 'Undo / redo'],
  ['Hold ⌘ or Ctrl over a pin', null, 'Preview its page'],
  ['Shift-drag', null, 'Pan while a tool is active'],
  ['?', 'help', 'This list'],
];

export function shortcutAction(e) {
  if (e.metaKey || e.ctrlKey || e.altKey) return null;
  const hit = SHORTCUTS.find(([k, action]) => action && k.toLowerCase() === e.key.toLowerCase());
  if (hit) return hit[1];
  return e.key === '?' ? 'help' : null;
}

export function isTypingTarget(t) {
  return !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable);
}
