// Typed-token search: `kind:npc -tag:dead born>=300 faction:"Silver Court" wolves`.
// Tokens become structured filters for /vault/search; everything else stays
// free text. A token whose key is unknown degrades to a plain word, so pasted
// text like `http://x` or `time:12` never turns into a filter.

const TOKEN = /(-)?([A-Za-z_][\w-]*)(>=|<=|!=|>|<|=|:|~)(?:"([^"]*)"|([^\s"]*))/y;
const RESERVED = new Set(['kind', 'tag', 'folder', 'edited']);
const UNIT_SECS = { h: 3600, d: 86400, w: 604800 };

function tokenAt(text, i, propKeys) {
  TOKEN.lastIndex = i;
  const m = TOKEN.exec(text);
  if (!m) return null;
  const end = i + m[0].length;
  if (end < text.length && !/\s/.test(text[end])) return null;
  const [, neg, rawKey, op, quoted, bare] = m;
  const value = quoted !== undefined ? quoted : bare;
  if (!value) return null;
  const key = rawKey.toLowerCase();
  const reserved = RESERVED.has(key);
  if (!reserved && !propKeys.has(key)) return null;
  if (reserved && op !== ':' && op !== '=') return null;
  if (neg && (reserved ? key !== 'kind' && key !== 'tag' : !['=', ':', '!='].includes(op))) return null;
  return { raw: m[0], start: i, end, neg: !!neg, key, field: rawKey, op, value };
}

const csvAdd = (cur, v) => (cur ? `${cur},${v}` : v);

export function parseQuery(text, propKeys = new Set()) {
  const keys = propKeys instanceof Set ? propKeys : new Set(propKeys);
  const tokens = [];
  const words = [];
  let i = 0;
  while (i < text.length) {
    if (/\s/.test(text[i])) { i++; continue; }
    const tok = tokenAt(text, i, keys);
    if (tok) { tokens.push(tok); i = tok.end; continue; }
    let j = i;
    while (j < text.length && !/\s/.test(text[j])) j++;
    words.push(text.slice(i, j));
    i = j;
  }

  const facets = { props: [] };
  for (const t of tokens) {
    if (t.key === 'kind') facets[t.neg ? 'not_kind' : 'kind'] = csvAdd(facets[t.neg ? 'not_kind' : 'kind'], t.value);
    else if (t.key === 'tag') facets[t.neg ? 'not_tag' : 'tag'] = csvAdd(facets[t.neg ? 'not_tag' : 'tag'], t.value.replace(/^#/, ''));
    else if (t.key === 'folder') facets.folder = t.value;
    else if (t.key === 'edited') {
      const m = /^(\d+)([hdw])$/i.exec(t.value);
      if (m) facets.edited_after = Math.floor(Date.now() / 1000) - +m[1] * UNIT_SECS[m[2].toLowerCase()];
    } else {
      const op = t.neg ? (t.op === '!=' ? '=' : '!=') : t.op;
      facets.props.push({ field: t.field, op, value: t.value });
    }
  }
  return { words: words.join(' '), tokens, facets };
}

// Short label for a chip.
export function tokenLabel(t) {
  const shown = /\s/.test(t.value) ? `"${t.value}"` : t.value;
  return `${t.neg ? '−' : ''}${t.field}${t.op === '=' ? ':' : t.op}${shown}`;
}

export function removeToken(text, tok) {
  return `${text.slice(0, tok.start)}${text.slice(tok.end)}`.replace(/\s{2,}/g, ' ').trimStart();
}

// Completion for the word under the caret (end of text): keys, then kind/tag values.
export function suggest(text, { propKeys = [], kinds = [], tags = [] } = {}) {
  if (!text || /\s$/.test(text)) return null;
  const start = text.search(/\S+$/);
  const word = text.slice(start);
  const m = /^(-?)([A-Za-z_][\w-]*)([:=])(.*)$/.exec(word);
  if (m) {
    const [, neg, key, op, partial] = m;
    const pool = key.toLowerCase() === 'kind' ? kinds : key.toLowerCase() === 'tag' ? tags : null;
    if (!pool || partial.startsWith('"')) return null;
    const items = pool.filter((v) => v.toLowerCase().startsWith(partial.toLowerCase()) && v !== partial).slice(0, 8);
    return items.length ? { start, items: items.map((v) => ({ label: v, insert: `${neg}${key}${op}${v} ` })) } : null;
  }
  const bare = word.replace(/^-/, '').toLowerCase();
  if (bare.length < 1 || /[^\w-]/.test(bare)) return null;
  const keys = ['kind', 'tag', 'folder', 'edited', ...propKeys];
  const items = keys.filter((k, i) => keys.indexOf(k) === i && k.toLowerCase().startsWith(bare) && k.toLowerCase() !== bare).slice(0, 8);
  const neg = word.startsWith('-') ? '-' : '';
  return items.length ? { start, items: items.map((k) => ({ label: `${k}:`, insert: `${neg}${k}:` })) } : null;
}
