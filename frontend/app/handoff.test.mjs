// Regression tests for the handoff rules: what may carry forward, where it may
// land, and what is only ever suggested.
// Run: node --test frontend/app/handoff.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  carryCandidates, draftDestinations, possibilityCandidates, suggestedDestination,
} from './handoff.js';

test('happened cards cannot carry; unused and changed ones can', () => {
  const cards = [
    { id: 'c1', section: 'scene', outcome: 'happened', text: 'The docks confrontation' },
    { id: 'c2', section: 'scene', outcome: 'unused', text: 'Bargain with the magistrate' },
    { id: 'c3', section: 'reminder', outcome: 'changed', text: 'The courier is still missing' },
    { id: 'c4', section: 'scene', outcome: 'unmarked', text: 'Warehouse rumour' },
  ];
  assert.deepEqual(carryCandidates(cards).map((c) => c.id), ['c2', 'c3', 'c4']);
});

test('an unsaved card has no id to carry', () => {
  assert.deepEqual(carryCandidates([{ id: null, outcome: 'unused', text: 'draft' }]), []);
});

test('only a pending possibility is offered', () => {
  const run = {
    possibilities: [
      { id: 'p1', title: 'The watch questions the dockworkers', decision: 'pending' },
      { id: 'p2', title: 'Already carried', decision: 'saved_to_prep' },
      { id: 'p3', title: 'Dismissed', decision: 'dismissed' },
    ],
  };
  assert.deepEqual(possibilityCandidates(run).map((p) => p.id), ['p1']);
});

test('destinations exclude this session and anything already summarized', () => {
  const sessions = [
    { session_id: 's12', session_number: 12, has_summary: true },
    { session_id: 's13', session_number: 13, title: 'Next', has_summary: false },
    { session_id: 's14', session_number: 14, has_summary: false },
  ];
  const dests = draftDestinations(sessions, 's12', 12);
  assert.deepEqual(dests.map((d) => d.session_id), ['s13', 's14']);
  assert.equal(dests[0].label, 'Session 13 · Next');
});

test('a single later draft is suggested; several are not guessed', () => {
  const one = draftDestinations([{ session_id: 's13', session_number: 13, has_summary: false }], 's12', 12);
  assert.equal(suggestedDestination(one), 's13');

  const many = draftDestinations([
    { session_id: 's13', session_number: 13, has_summary: false },
    { session_id: 's14', session_number: 14, has_summary: false },
  ], 's12', 12);
  assert.equal(suggestedDestination(many), null);
});
