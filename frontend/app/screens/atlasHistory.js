// Undo/redo for one map: snapshots of the whole map doc. Only edits to the
// doc are tracked (camera, selection, open panels are not). Transactions nest;
// only the outermost one records a step, and no-op steps are dropped.
// No imports — unit-testable in node.

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

export function createHistory(limit = 50) {
  let past = [];
  let future = [];
  let depth = 0;
  let base = null; // doc before the outermost transaction
  let last = null; // doc after the latest edit inside it

  const record = (before, after) => {
    if (!before || !after || same(before, after)) return;
    past.push(before);
    if (past.length > limit) past.shift();
    future = [];
  };

  return {
    // An edit went from `before` to `after`.
    commit(before, after) {
      if (depth > 0) { last = after; return; }
      record(before, after);
    },
    begin(before) {
      if (depth === 0) { base = before; last = null; }
      depth++;
    },
    end() {
      if (depth === 0) return;
      depth--;
      if (depth === 0) { record(base, last); base = null; last = null; }
    },
    // `current` is the doc being replaced; returns the doc to restore, or null.
    undo(current) {
      if (!past.length) return null;
      future.push(current);
      return past.pop();
    },
    redo(current) {
      if (!future.length) return null;
      past.push(current);
      return future.pop();
    },
    canUndo: () => past.length > 0,
    canRedo: () => future.length > 0,
    clear() { past = []; future = []; depth = 0; base = null; last = null; },
  };
}
