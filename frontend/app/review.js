// Session review: developments, questions, and possibilities stored as
// `review.json` beside the session (GET/PUT /sessions/:id/review). Every
// mutation carries the revision it was read at, so an external edit or a
// second window conflicts instead of overwriting. Nothing here writes to the
// world; only `applyReview` does, and only for the ids it is given.
import { apiFetch, apiJson, apiStream } from './core.js';

export async function loadReview(sessionId) {
  return apiFetch(`/sessions/${sessionId}/review`);
}

// Decisions, adjustments, and question actions in one PUT.
export function putReview(sessionId, body) {
  return apiJson(`/sessions/${sessionId}/review`, 'PUT', body);
}

// `request_id` is the idempotency receipt: the same id retried after a lost
// response returns the recorded result instead of writing twice.
export function applyReview(sessionId, body) {
  return apiJson(`/sessions/${sessionId}/review/apply`, 'POST', body);
}

export function recoverReview(sessionId, body) {
  return apiJson(`/sessions/${sessionId}/review/recover`, 'POST', body);
}

export function finishReview(sessionId, body) {
  return apiJson(`/sessions/${sessionId}/review/finish`, 'POST', body);
}

export function reopenReview(sessionId, body) {
  return apiJson(`/sessions/${sessionId}/review/reopen`, 'POST', body);
}

export function cancelReviewGeneration(sessionId) {
  return apiJson(`/sessions/${sessionId}/review/generate/cancel`, 'POST', {});
}

// Generation and clarification both stream: {stage:'reading'|'grounding'|
// 'building'}, then {stage:'done', run, revision} or {stage:'error', message}.
export function streamGenerate(sessionId, body, onEvent, opts) {
  return apiStream(`/sessions/${sessionId}/review/generate`, body, onEvent, opts);
}

export function streamClarify(sessionId, body, onEvent, opts) {
  return apiStream(`/sessions/${sessionId}/review/clarify`, body, onEvent, opts);
}

export function newRequestId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  // Non-secure fallback: this only needs to be unique per apply attempt.
  return `req-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
