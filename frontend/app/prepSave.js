// Prep save queue: debounces edits, coalesces bursts into one request, never
// overlaps PUTs, and lets a navigation guard await quiescence. Kept pure (the
// I/O and status are injected) so the delayed/failed-save behavior is testable.
export function createSaveQueue({ save, onStatus, delay = 800, timers = {} }) {
  const setTimer = timers.setTimeout || setTimeout;
  const clearTimer = timers.clearTimeout || clearTimeout;
  let timer = null;
  let dirty = false;
  let conflict = false;
  let running = null;

  function cancelTimer() {
    if (timer != null) { clearTimer(timer); timer = null; }
  }

  function schedule() {
    dirty = true;
    if (conflict) return;
    cancelTimer();
    timer = setTimer(() => { timer = null; run(); }, delay);
  }

  function run() {
    if (running) return running;
    cancelTimer();
    const p = (async () => {
      if (conflict) return false;
      let didSave = false;
      while (dirty && !conflict) {
        didSave = true;
        dirty = false;
        onStatus('saving');
        try {
          await save();
        } catch (e) {
          if (e && e.status === 409) {
            conflict = true;
            onStatus('conflict', e);
            return false;
          }
          dirty = true; // keep the draft dirty so Retry / the next leave re-attempts
          onStatus('error', e);
          return false;
        }
      }
      if (didSave) onStatus('saved');
      return true;
    })();
    running = p.then(
      (result) => { running = null; return result; },
      (error) => { running = null; throw error; },
    );
    return running;
  }

  return {
    schedule,
    flush: run,
    isDirty: () => dirty,
    isBusy: () => !!running,
    isConflict: () => conflict,
    markClean: () => { cancelTimer(); dirty = false; },
    resolveConflict: () => { conflict = false; },
  };
}
