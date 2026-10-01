// Pure helpers for the Keeper transcript: fold a turn's tool loop into one
// summary row, derive source chips, estimate context use. No DOM, no store.

const PAGE_READS = new Set(['read_page']);
const SUMMARY_READS = new Set(['read_summary', 'read_prep', 'read_recap']);
const TRANSCRIPT_READS = new Set(['read_transcript']);
const SEARCHES = new Set([
  'search_pages', 'search_summaries', 'search_transcripts', 'query_world', 'web_search',
  'list_pages', 'get_backlinks', 'list_sessions', 'read_relations', 'read_timeline',
]);
const SESSION_READS = new Set([...SUMMARY_READS, ...TRANSCRIPT_READS]);

const ARG_KEYS = ['path', 'query', 'session', 'name', 'url', 'folder', 'q'];

// Short human label for a tool call's arguments (object or JSON string).
export function argLabel(args) {
  let a = args;
  if (typeof a === 'string') {
    try { a = JSON.parse(a); } catch (_) { return a; }
  }
  if (!a || typeof a !== 'object') return a == null ? '' : String(a);
  for (const k of ARG_KEYS) {
    if (a[k] != null && a[k] !== '') return k === 'session' ? `session ${a[k]}` : String(a[k]);
  }
  const s = JSON.stringify(a);
  return s === '{}' ? '' : s;
}

export function isEmptyResult(summary) {
  const s = (summary || '').trim();
  if (!s) return true;
  return /^(no (pages?|results?|matches|hits|sessions?|backlinks?|summar|transcript|memor)|nothing\b|none\b|0 (results?|hits|matches|pages)|\[\]$)/i.test(s);
}

// A tool result that changes something (or runs a command) stays visible as its
// own row; everything else folds into the step summary.
export function isWriteStep(step) {
  const d = step.diff;
  return !!d && (d.new != null || d.command != null || d.summary != null);
}

const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;

export function summarizeSteps(steps) {
  let pages = 0; let sums = 0; let trs = 0; let searched = 0; let other = 0; let failed = 0;
  for (const s of steps) {
    if (s.isError) failed++;
    if (PAGE_READS.has(s.name)) pages++;
    else if (SUMMARY_READS.has(s.name)) sums++;
    else if (TRANSCRIPT_READS.has(s.name)) trs++;
    else if (SEARCHES.has(s.name)) searched++;
    else other++;
  }
  const reads = [];
  if (pages) reads.push(plural(pages, 'page', 'pages'));
  if (sums) reads.push(plural(sums, 'summary', 'summaries'));
  if (trs) reads.push(plural(trs, 'transcript', 'transcripts'));
  const parts = [];
  if (reads.length) parts.push(`Read ${reads.join(', ')}`);
  if (searched) parts.push(`${parts.length ? 's' : 'S'}earched ${plural(searched, 'source', 'sources')}`);
  if (other) parts.push(`${parts.length ? 'u' : 'U'}sed ${plural(other, 'other tool', 'other tools')}`);
  let out = parts.join(', ') || 'No steps';
  if (failed) out += ` · ${failed} failed`;
  return out;
}

function firstLine(content) {
  return ((content || '').split('\n').find(
    (l) => l.trim() && !l.startsWith('Tool output') && /[\p{L}\p{N}]/u.test(l),
  ) || '').trim();
}

// Split a normalized step list ({name, args, summary, isError, running, diff})
// into render items: consecutive read-only steps become one `steps` group,
// writes stay `row`s, ask_user is hidden (the question card owns it).
export function foldSteps(steps) {
  const items = [];
  let group = null;
  for (const s of steps) {
    if (s.name === 'ask_user') continue;
    if (isWriteStep(s)) { group = null; items.push({ kind: 'row', step: s }); continue; }
    if (!group) { group = { kind: 'steps', steps: [] }; items.push(group); }
    group.steps.push(s);
  }
  return items;
}

// Sources a finished turn drew on: pages read + sessions consulted.
export function sourcesOf(steps) {
  const out = [];
  const seen = new Set();
  for (const s of steps) {
    if (s.isError || !s.argsObj) continue;
    let src = null;
    if (PAGE_READS.has(s.name) && s.argsObj.path) src = { type: 'page', path: String(s.argsObj.path) };
    else if (SESSION_READS.has(s.name) && s.argsObj.session != null) src = { type: 'session', number: Number(s.argsObj.session) };
    if (!src) continue;
    const key = `${src.type}:${src.path ?? src.number}`;
    if (!seen.has(key)) { seen.add(key); out.push(src); }
  }
  return out;
}

// Persisted events → render items. `assistant` events carry the tool calls,
// `tool_result` events the outputs; pair them by call_id.
export function buildItems(events, { live = false } = {}) {
  const calls = new Map();
  for (const ev of events) {
    if (ev.type === 'assistant') for (const c of ev.tool_calls || []) calls.set(c.id, c);
  }
  const items = [];
  let steps = [];
  let turn = [];
  let turnHasAnswer = false;
  const flushSteps = () => {
    if (steps.length) { items.push(...foldSteps(steps)); steps = []; }
  };
  const endTurn = (final) => {
    flushSteps();
    const src = sourcesOf(turn);
    if (turnHasAnswer && src.length && !(final && live)) items.push({ kind: 'sources', sources: src });
    turn = []; turnHasAnswer = false;
  };
  events.forEach((ev, i) => {
    if (ev.type === 'user') { endTurn(false); items.push({ kind: 'ev', ev, i }); return; }
    if (ev.type === 'tool_result') {
      const call = calls.get(ev.call_id);
      const argsObj = call?.arguments && typeof call.arguments === 'object' ? call.arguments : null;
      if (ev.name === 'ask_user') {
        flushSteps();
        const m = /^The user answered: ([\s\S]*)$/.exec(ev.content || '');
        items.push({ kind: 'qa', question: argsObj?.question || '', answer: m ? m[1].trim() : '', skipped: !m });
        return;
      }
      const step = {
        name: ev.name, args: argsObj ? argLabel(argsObj) : '', argsObj, callId: ev.call_id,
        summary: firstLine(ev.content), isError: !!ev.is_error, diff: ev.diff,
      };
      turn.push(step);
      steps.push(step);
      return;
    }
    if (ev.type === 'assistant') {
      if ((ev.text || '').trim()) { flushSteps(); turnHasAnswer = true; items.push({ kind: 'ev', ev, i }); }
      return;
    }
    if (ev.type === 'model') return;
    flushSteps();
    items.push({ kind: 'ev', ev, i });
  });
  endTurn(true);
  return items;
}

// Live tool rows ({name, args, summary, isError, running, diff}) → render items.
export function liveItems(tools) {
  return foldSteps((tools || []).map((t) => ({
    ...t, args: argLabel(t.args || ''),
  })));
}

// Rough token count (~4 chars each) of what the model will see next turn:
// everything after the last /compact boundary.
export function estimateTokens(events) {
  let start = 0;
  for (let i = events.length - 1; i >= 0; i--) if (events[i].type === 'compact') { start = i; break; }
  let chars = 0;
  for (const ev of events.slice(start)) {
    chars += (ev.text || '').length + (ev.summary || '').length + (ev.content || '').length;
    if (ev.tool_calls) chars += JSON.stringify(ev.tool_calls).length;
  }
  return Math.round(chars / 4);
}

export function contextLimit(provider, model, ollamaMax) {
  const m = (model || '').toLowerCase();
  if ((provider || '').toLowerCase() === 'ollama') {
    const n = parseInt(ollamaMax, 10);
    return n > 0 ? n : 65536;
  }
  if (/claude|sonnet|opus|haiku/.test(m)) return 200000;
  if (/gemini/.test(m)) return 1000000;
  return 128000;
}

export function fmtTokens(n) {
  return n >= 10000 ? `${Math.round(n / 1000)}k` : n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}
