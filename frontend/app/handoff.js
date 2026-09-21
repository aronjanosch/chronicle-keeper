// Next-session handoff: which prep cards and saved possibilities may carry
// forward, and which sessions can receive them. Pure, because the rules decide
// what gets copied into another session's preparation.

// What happened stays in the session it happened in. Everything else is the
// GM's call, so a changed card is eligible but never preselected.
export function carryCandidates(cards) {
  return (cards || [])
    .filter((c) => c.id && c.outcome !== 'happened')
    .map((c) => ({
      id: c.id,
      kind: 'card',
      section: c.section,
      outcome: c.outcome || 'unmarked',
      label: c.title || c.text || '(empty card)',
    }));
}

// A possibility can only carry once: after that the review records where it
// went, and the card lives in that session's prep.
export function possibilityCandidates(run) {
  return ((run && run.possibilities) || [])
    .filter((p) => p.decision === 'pending')
    .map((p) => ({ id: p.id, kind: 'possibility', label: p.title || p.text }));
}

// A draft is a session that has not been summarized yet. The current session is
// never a destination, and a later session number sorts first because that is
// the one a handoff usually means.
export function draftDestinations(sessions, currentSessionId, currentNumber) {
  return (sessions || [])
    .filter((s) => s.session_id !== currentSessionId && !s.has_summary)
    .sort((a, b) => (a.session_number || 0) - (b.session_number || 0))
    .map((s) => ({
      session_id: s.session_id,
      label: `Session ${s.session_number ?? '?'}${s.title ? ` · ${s.title}` : ''}`,
      next: currentNumber != null && (s.session_number || 0) > currentNumber,
    }));
}

// The default suggestion is the nearest later draft — a suggestion only: with
// several drafts the GM still chooses, and nothing is created implicitly.
export function suggestedDestination(destinations) {
  const later = (destinations || []).filter((d) => d.next);
  return later.length === 1 ? later[0].session_id : null;
}
