// Pure review logic: what is selectable, what Apply would write, how a card
// reads, and the exact before/after lines a preview shows. Kept out of the
// screen so the rules that decide whether a page gets written are testable
// without a DOM or a provider.

export const DECISIONS = {
  pending: { label: 'Not decided', tone: 'ink-faint' },
  selected: { label: 'Selected', tone: 'burgundy' },
  skipped: { label: 'Skipped', tone: 'ink-faint' },
  deferred: { label: 'Left for later', tone: 'ochre' },
  applied: { label: 'Applied', tone: 'moss' },
};

export function decisionLabel(decision) {
  return (DECISIONS[decision] || DECISIONS.pending).label;
}

export function developments(run) {
  return (run && run.developments) || [];
}

export function questions(run) {
  return (run && run.questions) || [];
}

export function possibilities(run) {
  return (run && run.possibilities) || [];
}

export function selectedIds(run) {
  return developments(run)
    .filter((d) => d.decision === 'selected')
    .map((d) => d.id);
}

// Questions and possibilities are never part of the apply count — they have no
// targets, and counting them would promise writes that cannot happen.
export function openQuestionCount(run) {
  return questions(run).filter((q) => q.status === 'pending').length;
}

// The one application a development belongs to, or null when it was never
// submitted. Applications are append-only, so the last match is the current one.
export function applicationOf(run, dev) {
  if (!dev || !dev.application_id) return null;
  const apps = (run && run.applications) || [];
  for (let i = apps.length - 1; i >= 0; i--) {
    if (apps[i].id === dev.application_id) return apps[i];
  }
  return null;
}

// How a card reads after an apply attempt. `conflicted` and `partial` stay
// visible and recoverable; they are never relabeled Applied.
export function cardStatus(run, dev) {
  const app = applicationOf(run, dev);
  if (app && app.status === 'conflicted') return 'conflicted';
  if (app && app.status === 'partial') return 'partial';
  if (app && app.status === 'prepared') return 'recovering';
  if (dev.decision === 'applied') return 'applied';
  return dev.decision;
}

export function isReadOnly(run, dev) {
  return dev.decision === 'applied';
}

// Targets whose bytes actually changed on disk, and those still to write.
export function targetProgress(app) {
  const targets = (app && app.targets) || [];
  return {
    written: targets.filter((t) => t.state === 'written').map((t) => t.path),
    pending: targets.filter((t) => t.state === 'pending').map((t) => t.path),
    conflicted: targets.filter((t) => t.state === 'conflict').map((t) => t.path),
  };
}

export function needsRecovery(run) {
  return ((run && run.applications) || []).filter((a) =>
    a.status === 'prepared' || a.status === 'conflicted' || a.status === 'partial');
}

// A stale run's previews were built from source bytes that no longer exist, so
// applying them would write a preview nobody can verify.
export function canApply(run, flags) {
  if (!run || run.status === 'finished') return false;
  if (flags && flags.stale) return false;
  return selectedIds(run).length > 0;
}

// Selecting is a toggle: a selected card goes back to undecided rather than
// silently staying selected after a second click.
export function toggleSelection(dev) {
  return { id: dev.id, decision: dev.decision === 'selected' ? 'pending' : 'selected' };
}

export function skipEntry(dev) {
  return { id: dev.id, decision: dev.decision === 'skipped' ? 'pending' : 'skipped' };
}

// An adjustment carries the whole target set with its edited after-text. The
// server clears the selection when it accepts one, so the GM re-reads the
// preview before choosing it again; editing the description alone leaves every
// target untouched.
export function adjustmentPayload(dev, { description, afters }) {
  const edited = afters || {};
  const targets = (dev.targets || [])
    .filter((t) => edited[t.path] !== undefined && edited[t.path] !== t.after)
    .map((t) => ({ path: t.path, after: edited[t.path] }));
  const payload = { id: dev.id, target_afters: targets };
  if (description !== undefined && description !== dev.description) {
    payload.description = description;
  }
  return payload;
}

export function changesNothing(payload) {
  return !payload.target_afters.length && payload.description === undefined;
}

export function sourceLabel(evidence) {
  if (!evidence) return '';
  switch (evidence.kind) {
    case 'transcript':
      return `Transcript · turns ${evidence.start_turn}–${evidence.end_turn}`;
    case 'summary':
      // Never "transcript verified": the summary alone cannot carry a fact.
      return 'Summary only — not verified against the transcript';
    case 'gm_confirmation':
      return 'You confirmed this';
    default:
      return 'Source';
  }
}

export function evidenceText(evidence) {
  if (!evidence) return '';
  return evidence.excerpt || evidence.text || '';
}

// Line-level diff of the exact bytes a target would replace. Longest common
// subsequence, so an append shows as added lines instead of a whole-file
// rewrite. Both sides are the server's bytes; nothing here is re-rendered as
// Markdown.
export function lineDiff(before, after, { context = 3 } = {}) {
  // An absent file has no lines at all, so a new page previews as pure addition.
  const a = before == null ? [] : before.split('\n');
  const b = after == null ? [] : after.split('\n');
  const lcs = [];
  for (let i = 0; i <= a.length; i++) lcs.push(new Array(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  const rows = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) { rows.push({ mode: 'same', text: a[i] }); i++; j++; }
    else if (lcs[i + 1][j] >= lcs[i][j + 1]) { rows.push({ mode: 'remove', text: a[i] }); i++; }
    else { rows.push({ mode: 'add', text: b[j] }); j++; }
  }
  while (i < a.length) { rows.push({ mode: 'remove', text: a[i++] }); }
  while (j < b.length) { rows.push({ mode: 'add', text: b[j++] }); }
  return collapseContext(rows, context);
}

// Unchanged runs longer than 2×context collapse to a marker, so a long page
// does not bury a two-line append.
function collapseContext(rows, context) {
  const keep = new Array(rows.length).fill(false);
  rows.forEach((row, idx) => {
    if (row.mode === 'same') return;
    for (let k = Math.max(0, idx - context); k <= Math.min(rows.length - 1, idx + context); k++) {
      keep[k] = true;
    }
  });
  const out = [];
  let skipped = 0;
  rows.forEach((row, idx) => {
    if (keep[idx]) {
      if (skipped) { out.push({ mode: 'gap', text: `… ${skipped} unchanged line${skipped === 1 ? '' : 's'}` }); skipped = 0; }
      out.push(row);
    } else {
      skipped++;
    }
  });
  if (skipped) out.push({ mode: 'gap', text: `… ${skipped} unchanged line${skipped === 1 ? '' : 's'}` });
  return out;
}

export function summarizeRun(run) {
  const devs = developments(run);
  return {
    applied: devs.filter((d) => d.decision === 'applied').length,
    skipped: devs.filter((d) => d.decision === 'skipped').length,
    deferred: devs.filter((d) => d.decision === 'deferred').length,
    pending: devs.filter((d) => d.decision === 'pending' || d.decision === 'selected').length,
  };
}

// The legacy adapter is read-only: old runs may have written some pages and not
// others, and the old file cannot say which.
export function isLegacy(run) {
  return !!run && run.status === 'legacy';
}
