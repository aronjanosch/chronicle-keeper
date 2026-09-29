// Session preparation. The prep is a Codex page (`kind: prep`) that the GM
// writes in the editor or Obsidian; GET /sessions/:id/prep reads it back as
// cards, and POST /sessions/:id/prep/ops applies small, surgical edits (add a
// card, mark an outcome, link a page) guarded by the page revision.
import { apiFetch, apiJson } from './core.js';

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
