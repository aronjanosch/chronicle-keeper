import assert from 'node:assert/strict';
import { createHistory } from './atlasHistory.js';
const d = (n, extra = {}) => ({ id: 'm', name: 'Vale', pins: n, ...extra });

// basic undo / redo, redo cleared by a new edit
let h = createHistory();
assert.equal(h.undo(d(0)), null);
h.commit(d(0), d(1)); h.commit(d(1), d(2));
assert.ok(h.canUndo() && !h.canRedo());
assert.deepEqual(h.undo(d(2)), d(1));
assert.deepEqual(h.undo(d(1)), d(0));
assert.equal(h.undo(d(0)), null);
assert.deepEqual(h.redo(d(0)), d(1));
h.commit(d(1), d(9));
assert.ok(!h.canRedo());
assert.equal(h.redo(d(9)), null);

// no-op steps dropped
h = createHistory();
h.commit(d(1), d(1));
assert.ok(!h.canUndo());

// nested transactions: only the outermost records one step spanning everything
h = createHistory();
h.begin(d(0));
h.commit(d(0), d(1));
h.begin(d(1));
h.commit(d(1), d(2));
h.end();
assert.ok(!h.canUndo());
h.commit(d(2), d(3));
h.end();
assert.ok(h.canUndo());
assert.deepEqual(h.undo(d(3)), d(0));
assert.equal(h.undo(d(0)), null);

// a transaction that ends where it began records nothing
h = createHistory();
h.begin(d(0)); h.commit(d(0), d(1)); h.commit(d(1), d(0)); h.end();
assert.ok(!h.canUndo());

// stray end() is harmless
h.end(); h.end();
assert.ok(!h.canUndo());

// cap
h = createHistory(3);
for (let i = 0; i < 6; i++) h.commit(d(i), d(i + 1));
let cur = d(6), n = 0;
for (let r = h.undo(cur); r; r = h.undo(cur)) { cur = r; n++; }
assert.equal(n, 3); assert.deepEqual(cur, d(3));

// clear
h.clear(); assert.ok(!h.canUndo() && !h.canRedo());
console.log('history ok');
