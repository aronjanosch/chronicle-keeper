// Regression tests for the review rules that decide what gets written: what
// Apply counts, what a conflicted card reads as, what an adjustment sends, and
// what a preview shows.
// Run: node --test frontend/app/reviewState.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  adjustmentPayload,
  canApply,
  cardStatus,
  changesNothing,
  lineDiff,
  needsRecovery,
  openQuestionCount,
  selectedIds,
  skipEntry,
  sourceLabel,
  summarizeRun,
  targetProgress,
  toggleSelection,
} from './reviewState.js';

function dev(id, decision = 'pending', extra = {}) {
  return {
    id,
    title: `Development ${id}`,
    description: 'What happened.',
    evidence: [],
    targets: [{ path: `NPCs/${id}.md`, base_hash: 'h', before: 'old', after: 'new' }],
    decision,
    application_id: null,
    compound: false,
    ...extra,
  };
}

test('apply counts developments only, never questions or possibilities', () => {
  const run = {
    status: 'open',
    developments: [dev('a', 'selected'), dev('b', 'pending'), dev('c', 'selected')],
    questions: [{ id: 'q1', status: 'pending' }, { id: 'q2', status: 'deferred' }],
    possibilities: [{ id: 'p1', decision: 'pending' }],
    applications: [],
  };
  assert.deepEqual(selectedIds(run), ['a', 'c']);
  assert.equal(openQuestionCount(run), 1);
  assert.equal(canApply(run, { stale: false }), true);
});

test('a stale or finished run cannot apply its previews', () => {
  const run = { status: 'open', developments: [dev('a', 'selected')], applications: [] };
  assert.equal(canApply(run, { stale: true }), false);
  assert.equal(canApply({ ...run, status: 'finished' }, { stale: false }), false);
  assert.equal(canApply({ ...run, developments: [dev('a', 'pending')] }, {}), false);
});

test('selection and skip are toggles, not one-way switches', () => {
  assert.deepEqual(toggleSelection(dev('a', 'pending')), { id: 'a', decision: 'selected' });
  assert.deepEqual(toggleSelection(dev('a', 'selected')), { id: 'a', decision: 'pending' });
  assert.deepEqual(skipEntry(dev('a', 'skipped')), { id: 'a', decision: 'pending' });
});

test('a conflicted or partial application is never labelled applied', () => {
  const base = dev('a', 'pending', { application_id: 'app1' });
  const conflicted = {
    developments: [base],
    applications: [{ id: 'app1', status: 'conflicted', targets: [
      { path: 'NPCs/a.md', state: 'conflict' },
      { path: 'NPCs/b.md', state: 'written' },
    ] }],
  };
  assert.equal(cardStatus(conflicted, base), 'conflicted');
  assert.deepEqual(targetProgress(conflicted.applications[0]), {
    written: ['NPCs/b.md'],
    pending: [],
    conflicted: ['NPCs/a.md'],
  });

  const partial = {
    developments: [{ ...base, decision: 'applied' }],
    applications: [{ id: 'app1', status: 'partial', targets: [] }],
  };
  assert.equal(cardStatus(partial, partial.developments[0]), 'partial');
  assert.equal(needsRecovery(partial).length, 1);
});

test('an applied development reads as applied and needs no recovery', () => {
  const run = {
    developments: [dev('a', 'applied', { application_id: 'app1' })],
    applications: [{ id: 'app1', status: 'applied', targets: [{ path: 'NPCs/a.md', state: 'written' }] }],
  };
  assert.equal(cardStatus(run, run.developments[0]), 'applied');
  assert.deepEqual(needsRecovery(run), []);
});

test('editing only the description leaves every target untouched', () => {
  const d = dev('a');
  const payload = adjustmentPayload(d, { description: 'Clearer wording.', afters: {} });
  assert.deepEqual(payload, { id: 'a', target_afters: [], description: 'Clearer wording.' });
  assert.equal(changesNothing(payload), false);

  const unchanged = adjustmentPayload(d, { description: d.description, afters: { 'NPCs/a.md': 'new' } });
  assert.deepEqual(unchanged, { id: 'a', target_afters: [] });
  assert.equal(changesNothing(unchanged), true);
});

test('an edited target is sent with its exact replacement text', () => {
  const payload = adjustmentPayload(dev('a'), { afters: { 'NPCs/a.md': 'edited bytes' } });
  assert.deepEqual(payload.target_afters, [{ path: 'NPCs/a.md', after: 'edited bytes' }]);
});

test('summary-only evidence is never labelled transcript verified', () => {
  assert.match(sourceLabel({ kind: 'summary', excerpt: 'x' }), /Summary only/);
  assert.match(sourceLabel({ kind: 'transcript', start_turn: 2, end_turn: 5 }), /turns 2–5/);
  assert.match(sourceLabel({ kind: 'gm_confirmation', text: 'x' }), /You confirmed/);
});

test('a preview shows the appended lines, not a whole-file rewrite', () => {
  const before = ['---', 'kind: npc', '---', '', '# Magistrate', '', '## Notes', ''].join('\n');
  const after = `${before}S12 — Exposed at the docks.\n`;
  const rows = lineDiff(before, after);
  assert.deepEqual(rows.filter((r) => r.mode === 'add').map((r) => r.text), ['S12 — Exposed at the docks.']);
  assert.equal(rows.some((r) => r.mode === 'remove'), false);
});

test('a long unchanged run collapses instead of burying the change', () => {
  const before = Array.from({ length: 40 }, (_, i) => `line ${i}`).join('\n');
  const after = `${before}\nnew tail`;
  const rows = lineDiff(before, after, { context: 2 });
  assert.equal(rows.filter((r) => r.mode === 'gap').length, 1);
  assert.deepEqual(rows.filter((r) => r.mode === 'add').map((r) => r.text), ['new tail']);
});

test('a new page previews as pure addition', () => {
  const rows = lineDiff(null, '---\nkind: npc\n---\n');
  assert.equal(rows.every((r) => r.mode === 'add'), true);
});

test('completion counts report what actually happened', () => {
  const run = {
    developments: [dev('a', 'applied'), dev('b', 'skipped'), dev('c', 'deferred'), dev('d', 'selected')],
  };
  assert.deepEqual(summarizeRun(run), { applied: 1, skipped: 1, deferred: 1, pending: 1 });
});
