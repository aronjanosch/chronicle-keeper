// Regression tests for Keeper prep suggestions (SC-07): accepting adds a new
// card with section/title/links intact, a second opening is refused unless
// explicitly replaced, the queue caps at five, and dismissal is targeted.
// Run: node --test frontend/app/prepSuggest.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  MAX_SUGGESTIONS,
  acceptSuggestion,
  appendSuggestion,
  canAcceptOpening,
  dismissSuggestion,
  replaceOpening,
  suggestionToCard,
} from './prepSuggest.js';

function suggestion(over = {}) {
  return {
    id: 's1',
    section: 'scene',
    title: 'The docks at dusk',
    text: 'A lantern goes out along the pier.',
    rationale: 'Your last summary ended with the docks.',
    links: ['Places/Docks.md'],
    is_idea: false,
    ...over,
  };
}

test('accept adds a card with the right section, title, and links', () => {
  const cards = [];
  const next = acceptSuggestion(cards, suggestion());
  assert.equal(next.length, 1);
  const card = next[0];
  assert.equal(card.section, 'scene');
  assert.equal(card.title, 'The docks at dusk');
  assert.equal(card.text, 'A lantern goes out along the pier.');
  assert.deepEqual(card.links, ['Places/Docks.md']);
  assert.equal(card.id, null, 'not a persisted prep card yet');
  assert.equal(card.outcome, 'unmarked');
  assert.equal(cards.length, 0, 'input list is not mutated');
});

test('suggestionToCard copies links so later edits do not alias the suggestion', () => {
  const s = suggestion();
  const card = suggestionToCard(s);
  card.links.push('Other.md');
  assert.deepEqual(s.links, ['Places/Docks.md'], 'source suggestion untouched');
});

test('an unknown section falls back to scene rather than corrupting the draft', () => {
  const card = suggestionToCard(suggestion({ section: 'bogus' }));
  assert.equal(card.section, 'scene');
});

test('a second opening is refused while one already exists', () => {
  const opening = suggestion({ id: 'o1', section: 'opening', title: 'Old opening', text: 'The old start.' });
  const cards = acceptSuggestion([], opening);
  assert.equal(cards.length, 1);
  const another = suggestion({ id: 'o2', section: 'opening', title: 'New opening', text: 'The new start.' });
  assert.equal(canAcceptOpening(cards, another), false);
  const after = acceptSuggestion(cards, another);
  assert.equal(after, cards, 'unchanged list identity when refused');
  assert.equal(after[0].text, 'The old start.', 'existing opening preserved');
});

test('canAcceptOpening is true when no opening exists or the section is not opening', () => {
  assert.equal(canAcceptOpening([], suggestion({ section: 'opening' })), true);
  assert.equal(canAcceptOpening([suggestion({ id: 'o1', section: 'opening' })], suggestion({ section: 'scene' })), true);
});

test('replaceOpening swaps the opening in place without touching other cards', () => {
  const opening = suggestion({ id: 'o1', section: 'opening', title: 'Old', text: 'Old start.' });
  const scene = suggestion({ id: 's1', section: 'scene', title: 'Scene', text: 'A scene.' });
  const cards = acceptSuggestion(acceptSuggestion([], opening), scene);
  const replacement = suggestion({ id: 'o2', section: 'opening', title: 'New', text: 'New start.' });
  const next = replaceOpening(cards, replacement);
  assert.equal(next.length, 2);
  assert.equal(next[0].section, 'opening');
  assert.equal(next[0].text, 'New start.');
  assert.equal(next[1].uid, cards[1].uid, 'non-opening card untouched');
  assert.equal(next[1].text, 'A scene.');
});

test('at most five suggestions are queued', () => {
  let list = [];
  for (let i = 0; i < 8; i++) list = appendSuggestion(list, suggestion({ id: `s${i}` }));
  assert.equal(list.length, MAX_SUGGESTIONS);
  assert.equal(list[list.length - 1].id, 's4', 'first five kept');
});

test('appendSuggestion dedupes repeat ids and ignores malformed entries', () => {
  let list = appendSuggestion([], suggestion({ id: 'dup' }));
  list = appendSuggestion(list, suggestion({ id: 'dup' }));
  assert.equal(list.length, 1);
  list = appendSuggestion(list, null);
  list = appendSuggestion(list, suggestion({ id: 'bad', section: 'nope' }));
  assert.equal(list.length, 1);
});

test('dismiss removes only the target suggestion', () => {
  const a = suggestion({ id: 'a' });
  const b = suggestion({ id: 'b' });
  const c = suggestion({ id: 'c' });
  const next = dismissSuggestion([a, b, c], 'b');
  assert.deepEqual(next.map((s) => s.id), ['a', 'c']);
  assert.equal(dismissSuggestion([a, b], 'missing').length, 2);
});
