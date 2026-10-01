import test from 'node:test';
import assert from 'node:assert/strict';
import { summarizeSteps, buildItems, liveItems, isEmptyResult, argLabel, estimateTokens } from './keeperTrace.js';

const tr = (id, name, content, extra = {}) => ({ type: 'tool_result', call_id: id, name, content, ...extra });
const asst = (text, calls = []) => ({ type: 'assistant', text, tool_calls: calls });

test('summary counts reads, searches and failures', () => {
  const s = summarizeSteps([
    { name: 'read_page' }, { name: 'read_page' }, { name: 'read_summary' },
    ...Array(6).fill({ name: 'search_pages' }), { name: 'read_page', isError: true },
  ]);
  assert.equal(s, 'Read 3 pages, 1 summary, searched 6 sources · 1 failed');
  assert.equal(summarizeSteps([{ name: 'search_pages' }]), 'Searched 1 source');
  assert.equal(summarizeSteps([{ name: 'tags' }, { name: 'use_skill' }]), 'Used 2 other tools');
});

test('empty results detected', () => {
  assert.ok(isEmptyResult('No pages match'));
  assert.ok(isEmptyResult(''));
  assert.ok(!isEmptyResult('Brannik.md — a hammer'));
});

test('argLabel prefers path/query', () => {
  assert.equal(argLabel({ path: 'A.md', x: 1 }), 'A.md');
  assert.equal(argLabel('{"query":"docks"}'), 'docks');
  assert.equal(argLabel({ session: 3 }), 'session 3');
});

test('buildItems folds reads, hides ask_user, adds sources after the answer', () => {
  const ev = [
    { type: 'user', text: 'hi' },
    asst('', [{ id: 'a', name: 'read_page', arguments: { path: 'NPCs/Oren.md' } }, { id: 'b', name: 'search_pages', arguments: { query: 'x' } }]),
    tr('a', 'read_page', '```\n---\nOren\n```'),
    tr('b', 'search_pages', 'No pages match'),
    asst('Answer text'),
  ];
  const items = buildItems(ev);
  assert.deepEqual(items.map((i) => i.kind), ['ev', 'steps', 'ev', 'sources']);
  assert.equal(items[1].steps.length, 2);
  assert.deepEqual(items[3].sources, [{ type: 'page', path: 'NPCs/Oren.md' }]);
  assert.equal(buildItems(ev, { live: true }).at(-1).kind, 'ev');
});

test('writes stay separate rows and ask_user becomes a qa item', () => {
  const ev = [
    { type: 'user', text: 'x' },
    asst('', [{ id: 'q', name: 'ask_user', arguments: { question: 'Which?', options: ['a'] } }]),
    tr('q', 'ask_user', 'The user answered: a'),
    tr('w', 'edit_page', 'ok', { diff: { path: 'A.md', old: 'a', new: 'b' } }),
  ];
  const kinds = buildItems(ev).map((i) => i.kind);
  assert.deepEqual(kinds, ['ev', 'qa', 'row']);
  assert.equal(buildItems(ev)[1].answer, 'a');
});

test('liveItems hides ask_user and groups reads', () => {
  const items = liveItems([{ name: 'ask_user', args: '{}' }, { name: 'read_page', args: '{"path":"A.md"}', running: true }]);
  assert.equal(items.length, 1);
  assert.equal(items[0].steps[0].args, 'A.md');
});

test('estimateTokens counts after last compact', () => {
  const t = estimateTokens([{ type: 'user', text: 'x'.repeat(4000) }, { type: 'compact', summary: 'y'.repeat(400) }]);
  assert.equal(t, 100);
});
