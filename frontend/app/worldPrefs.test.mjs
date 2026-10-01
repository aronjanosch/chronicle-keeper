import assert from 'node:assert/strict';
import { togglePinned, movePinned, remapPaths, gapCounts, visibleGaps, skillForGap } from './worldPrefs.js';

assert.deepEqual(togglePinned(['a'], 'b'), ['a', 'b']);
assert.deepEqual(togglePinned(['a', 'b'], 'a'), ['b']);
assert.deepEqual(movePinned(['a', 'b', 'c'], 'c', 0), ['c', 'a', 'b']);
assert.deepEqual(movePinned(['a', 'b', 'c'], 'a', 1), ['b', 'a', 'c']);
assert.deepEqual(movePinned(['a', 'b'], 'z', 9), ['a', 'b', 'z']);
assert.deepEqual(remapPaths(['NPCs/A.md', 'NPCs/B.md', 'X.md'], 'NPCs', 'People'), ['People/A.md', 'People/B.md', 'X.md']);
assert.deepEqual(remapPaths(['A.md', 'B.md'], 'A.md', 'B.md'), ['B.md']);

const rows = [
  { key: 'page:a', row: 'page', stub: true, open_questions: 2, kind: 'npc' },
  { key: 'page:b', row: 'page', stub: false, open_questions: 1 },
  { key: 'link:c', row: 'link' },
  { key: 'page:d', row: 'page', stub: true, open_questions: 0 },
];
assert.deepEqual(gapCounts(rows, []), { all: 4, stubs: 2, open: 2, links: 1 });
assert.deepEqual(gapCounts(rows, ['page:a']), { all: 3, stubs: 1, open: 1, links: 1 });
assert.deepEqual(visibleGaps(rows, ['link:c'], 'links'), []);
assert.equal(skillForGap(rows[0], [{ name: 'NPC voice', kinds: ['npc'] }, { name: 'Flesh out a character', kinds: ['npc'] }]).name, 'Flesh out a character');
assert.equal(skillForGap(rows[1], [{ name: 'Flesh out a place', kinds: ['place'] }]), null);
console.log('worldPrefs ok');
