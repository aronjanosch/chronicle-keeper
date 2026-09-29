// Session preparation. The prep is a Codex page (`kind: prep`) that the GM
// writes in the editor or Obsidian; GET /sessions/:id/prep reads it back as
// cards, and POST /sessions/:id/prep/ops applies small, surgical edits (add a
// card, mark an outcome, link a page) guarded by the page revision.
import { apiFetch, apiJson } from './core.js';

export const PREP_SECTIONS = [
  { key: 'opening', label: 'Opening situation', hint: 'How the session begins — a place, a mood, the first thing that happens.' },
  { key: 'scene', label: 'Possible scenes', hint: 'Moments that might come up. Nothing here is fixed until it happens at the table.' },
  { key: 'reminder', label: 'Keep in mind', hint: 'Threads, promises, and details not to forget.' },
];

export const PREP_OUTCOMES = [
  { key: 'unmarked', label: 'Unmarked' },
  { key: 'happened', label: 'Happened' },
  { key: 'changed', label: 'Changed' },
  { key: 'unused', label: 'Unused' },
];

export function outcomeLabel(key) {
  return (PREP_OUTCOMES.find((o) => o.key === key) || PREP_OUTCOMES[0]).label;
}

// Kept for prepSuggest.js: a suggestion shaped like a card.
let uidSeq = 0;
export function newCard(section, text = '') {
  return { uid: `prep-${++uidSeq}`, id: null, section, title: null, text, links: [], outcome: 'unmarked', outcome_note: '' };
}

export function cardsInSection(cards, section) {
  return (cards || []).filter((c) => c.section === section);
}

export function hasOpening(cards) {
  return (cards || []).some((c) => c.section === 'opening');
}

export async function loadPrep(sessionId) {
  const data = await apiFetch(`/sessions/${sessionId}/prep`);
  return {
    revision: data.revision,
    page: data.page || null,
    cards: data.cards || [],
    selected_threads: data.selected_threads || [],
    notes: data.notes || '',
  };
}

// `ops`: [{op:'create'} | {op:'adopt', page} | {op:'add_card', section, title?, text, links?}
//        | {op:'replace_opening', text} | {op:'set_outcome', id, outcome, note?}
//        | {op:'add_thread', page} | {op:'link', id, page}]
export function prepOps(sessionId, baseRevision, ops) {
  return apiJson(`/sessions/${sessionId}/prep/ops`, 'POST', { base_revision: baseRevision, ops });
}

// Copy unused prep cards and saved possibilities into another session's prep.
// `request_id` is the dedup receipt: the same retry lands one copy, not two.
export function carryPrep(destSessionId, { base_revision, request_id, source_session_id, item_ids }) {
  return apiJson(`/sessions/${destSessionId}/prep/carry`, 'POST', {
    base_revision, request_id, source_session_id, item_ids,
  });
}
