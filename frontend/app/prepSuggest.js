// Keeper prep suggestions (SC-07): a thin SSE action plus pure helpers for the
// suggestion queue. Suggestions are ephemeral — nothing is written to `prep.md`
// until the user adds one, which goes through the normal draft mutation path.
// Kept free of DOM/store access so the accept/dismiss/cap rules are testable.
import { apiStream } from './core.js';
import { newCard, hasOpening } from './prep.js';

export const MAX_SUGGESTIONS = 5;

const SECTION_KEYS = new Set(['opening', 'scene', 'reminder']);

export function isPrepSection(section) {
  return SECTION_KEYS.has(section);
}

// A suggestion as a draft prep card: fresh local uid, server id null (it is not
// a prep card yet), title and links carried over.
export function suggestionToCard(suggestion) {
  const s = suggestion || {};
  return {
    ...newCard(isPrepSection(s.section) ? s.section : 'scene', s.text || ''),
    title: s.title || null,
    links: Array.isArray(s.links) ? s.links.slice() : [],
  };
}

// An opening already exists → accepting another one would silently replace it.
// The caller must use replaceOpening after an explicit confirmation instead.
export function canAcceptOpening(cards, suggestion) {
  if (!suggestion || suggestion.section !== 'opening') return true;
  return !hasOpening(cards);
}

// Append the suggestion as a new card. Returns the original list untouched when
// it is malformed or would create a second opening. Other cards are never
// mutated or overwritten; the new card is a copy of the suggestion.
export function acceptSuggestion(cards, suggestion) {
  const list = cards || [];
  if (!suggestion || !isPrepSection(suggestion.section)) return list;
  if (suggestion.section === 'opening' && hasOpening(list)) return list;
  return [...list, suggestionToCard(suggestion)];
}

// Explicit replacement for an existing opening: keeps the opening's position and
// swaps in a card built from the suggestion. Never used implicitly.
export function replaceOpening(cards, suggestion) {
  const list = cards || [];
  if (!suggestion || suggestion.section !== 'opening') return list;
  const i = list.findIndex((c) => c.section === 'opening');
  const card = suggestionToCard(suggestion);
  if (i < 0) return [...list, card];
  const next = list.slice();
  next[i] = card;
  return next;
}

// Queue one suggestion, capped at MAX_SUGGESTIONS and deduped by id (a
// regenerated run can repeat ids across events).
export function appendSuggestion(list, suggestion) {
  const cur = list || [];
  if (!suggestion || !isPrepSection(suggestion.section)) return cur;
  if (cur.length >= MAX_SUGGESTIONS) return cur;
  if (suggestion.id && cur.some((s) => s.id === suggestion.id)) return cur;
  return [...cur, suggestion];
}

export function dismissSuggestion(list, id) {
  return (list || []).filter((s) => s.id !== id);
}

// POST /sessions/:id/prep/suggest. Resolves with the terminal {ok, suggestions}
// or {ok:false, code, message} so callers can surface a recoverable error without
// throwing on a streamed error event. `onEvent` sees every parsed SSE frame.
export async function suggestPrep(sessionId, { instruction = '', linkedPaths = [], onEvent } = {}, { signal } = {}) {
  const body = {};
  const text = (instruction || '').trim();
  if (text) body.instruction = text;
  const paths = (linkedPaths || []).filter(Boolean);
  if (paths.length) body.linked_paths = paths;
  let terminal = null;
  await apiStream(`/sessions/${sessionId}/prep/suggest`, body, (ev) => {
    if (ev && ev.stage === 'done') {
      terminal = { ok: true, suggestions: Array.isArray(ev.suggestions) ? ev.suggestions : [] };
    } else if (ev && ev.stage === 'error') {
      terminal = { ok: false, code: ev.code, message: ev.message };
    }
    if (onEvent) onEvent(ev);
  }, { signal });
  return terminal || { ok: true, suggestions: [] };
}
