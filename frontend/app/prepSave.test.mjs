// Regression tests for the prep save queue: delayed saves coalesce, a failed
// save keeps the draft dirty for retry, and flush() reports success/failure so
// the navigation guard can decide whether it is safe to leave.
// Run: node --test frontend/app/prepSave.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createSaveQueue } from './prepSave.js';

// Deterministic timers: nothing fires until flush()/tick, so ordering is exact.
function fakeTimers() {
  const pending = new Map();
  let seq = 0;
  return {
    setTimeout: (fn) => { const id = ++seq; pending.set(id, fn); return id; },
    clearTimeout: (id) => { pending.delete(id); },
    fireDue() { const fns = [...pending.values()]; pending.clear(); fns.forEach((fn) => fn()); },
  };
}

function deferred() {
  let resolve, reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

test('coalesces rapid edits into one save', async () => {
  const timers = fakeTimers();
  let calls = 0;
  const q = createSaveQueue({ save: async () => { calls++; }, onStatus: () => {}, timers });
  q.schedule(); q.schedule(); q.schedule();
  assert.equal(calls, 0, 'nothing before the debounce fires');
  timers.fireDue();
  await q.flush();
  assert.equal(calls, 1);
  assert.equal(q.isDirty(), false);
});

test('flush waits for an in-flight save and reports success', async () => {
  const timers = fakeTimers();
  const gate = deferred();
  let calls = 0;
  const q = createSaveQueue({ save: () => { calls++; return gate.promise; }, onStatus: () => {}, timers });
  q.schedule();
  const flushing = q.flush();
  assert.equal(q.isBusy(), true);
  assert.equal(calls, 1, 'started immediately on flush');
  gate.resolve();
  assert.equal(await flushing, true);
  assert.equal(q.isDirty(), false);
});

test('a failed save stays dirty for retry and flush reports false', async () => {
  const timers = fakeTimers();
  let calls = 0;
  const q = createSaveQueue({
    save: async () => { calls++; if (calls === 1) throw new Error('network'); },
    onStatus: () => {},
    timers,
  });
  q.schedule();
  assert.equal(await q.flush(), false);
  assert.equal(q.isDirty(), true, 'draft kept dirty after failure');
  assert.equal(await q.flush(), true, 'retry succeeds');
  assert.equal(calls, 2);
  assert.equal(q.isDirty(), false);
});

test('a 409 conflict stops retries and is distinguishable for the guard', async () => {
  const timers = fakeTimers();
  let calls = 0;
  const conflict = Object.assign(new Error('stale'), { status: 409 });
  const q = createSaveQueue({ save: async () => { calls++; throw conflict; }, onStatus: () => {}, timers });
  q.schedule();
  assert.equal(await q.flush(), false);
  assert.equal(q.isConflict(), true);
  await q.flush();
  assert.equal(calls, 1, 'no further PUTs while conflicted');
});

test('edits made during a save are sent in a follow-up save', async () => {
  const timers = fakeTimers();
  const first = deferred();
  let calls = 0;
  const q = createSaveQueue({
    save: () => { calls++; return calls === 1 ? first.promise : Promise.resolve(); },
    onStatus: () => {},
    timers,
  });
  q.schedule();
  const flushing = q.flush();
  q.schedule();            // user types while the first PUT is in flight
  first.resolve();
  assert.equal(await flushing, true);
  assert.equal(calls, 2, 'the newer edit was saved without a manual retry');
  assert.equal(q.isDirty(), false);
});
