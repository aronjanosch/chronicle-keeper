// Session preparation: durable prep cards stored in the session folder
// (`prep.md`) via GET/PUT /sessions/:id/prep. Pure client helpers keep the
// draft shape consistent; the server owns ids, ordering, and revision checks.
// Modeled on the focused data modules (keeperPanel.js / keeperMemory.js).
import { apiFetch, apiJson } from './core.js';

export const PREP_SECTIONS = [
  { key: 'opening', label: 'Opening situation', hint: 'How the session begins — a place, a mood, the first thing that happens.', add: 'Add an opening' },
  { key: 'scene', label: 'Possible scenes', hint: 'Moments that might come up. Nothing here is fixed until it happens at the table.', add: 'Add a scene' },
  { key: 'reminder', label: 'Keep in mind', hint: 'Threads, promises, and details not to forget.', add: 'Add a reminder' },
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

// Client-only id for rendering/reorder; stripped before every PUT.
let uidSeq = 0;
function newUid() { return `prep-${++uidSeq}`; }

// A fresh, unsaved card. `section` is opening | scene | reminder. The server
// assigns the persisted `id` on the first successful save.
export function newCard(section, text = '') {
  return {
    uid: newUid(),
    id: null,
    section,
    title: null,
    text,
    links: [],
    outcome: 'unmarked',
    outcome_note: '',
  };
}

export function cardsInSection(cards, section) {
  return (cards || []).filter((c) => c.section === section);
}

export function hasOpening(cards) {
  return (cards || []).some((c) => c.section === 'opening');
}

// Swap a card with the nearest sibling in the same section; other sections'
// relative order is untouched (the file stores display order per section).
export function moveCard(cards, uid, dir) {
  const i = (cards || []).findIndex((c) => c.uid === uid);
  if (i < 0) return cards;
  const section = cards[i].section;
  let j = -1;
  if (dir < 0) {
    for (let k = i - 1; k >= 0; k--) { if (cards[k].section === section) { j = k; break; } }
  } else {
    for (let k = i + 1; k < cards.length; k++) { if (cards[k].section === section) { j = k; break; } }
  }
  if (j < 0) return cards;
  const next = cards.slice();
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}

export function removeCard(cards, uid) {
  return (cards || []).filter((c) => c.uid !== uid);
}

// Re-add a removed card after an undo as a brand-new card (fresh local uid, no
// persisted id). Safe whether or not the removal was already saved: the server
// replaces the whole card list, so a dropped id is simply gone.
export function restoreCard(card) {
  return { ...card, uid: newUid(), id: null };
}

// Insert a copy right after the original. A duplicate is a new card: no id,
// no carry origin, and an unmarked outcome.
export function duplicateCard(cards, uid) {
  const i = (cards || []).findIndex((c) => c.uid === uid);
  if (i < 0) return cards;
  const copy = { ...cards[i], uid: newUid(), id: null, origin: null, outcome: 'unmarked', outcome_note: '' };
  const next = cards.slice();
  next.splice(i + 1, 0, copy);
  return next;
}

export function applyOutcome(cards, uid, outcome, outcomeNote) {
  return (cards || []).map((c) => (c.uid === uid
    ? { ...c, outcome, outcome_note: outcomeNote !== undefined ? outcomeNote : (c.outcome_note || '') }
    : c));
}

function hydrate(card) {
  return {
    ...card,
    uid: newUid(),
    id: card.id ?? null,
    text: card.text || '',
    links: Array.isArray(card.links) ? card.links : [],
    outcome: card.outcome || 'unmarked',
    outcome_note: card.outcome_note || '',
  };
}

// Drop the client-only `uid`; send every server field back unchanged
// (including `links`, `origin`, and any fields carried by new tickets).
function serialize(card) {
  const { uid, ...rest } = card;
  return rest;
}

export async function loadPrep(sessionId) {
  const data = await apiFetch(`/sessions/${sessionId}/prep`);
  return {
    revision: data.revision,
    cards: (data.cards || []).map(hydrate),
    selected_threads: data.selected_threads || [],
    notes: data.notes || '',
  };
}

export function savePrep(sessionId, { base_revision, cards, selected_threads, notes }) {
  return apiJson(`/sessions/${sessionId}/prep`, 'PUT', {
    base_revision,
    cards: (cards || []).map(serialize),
    selected_threads: selected_threads || [],
    notes: notes || '',
  });
}
