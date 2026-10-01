// GM-only markers as plain frontmatter, so the .md stays valid anywhere:
//   gm_only: true                       -> the whole page
//   gm_fields: [secret_debt, loyalty]   -> those infobox/frontmatter keys
// Secret callouts (`> [!secret]`) are the third, body-level form.
const FM_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;

function split(content) {
  const m = FM_RE.exec(content || '');
  return m ? { fm: m[1], body: content.slice(m[0].length) } : { fm: '', body: content || '' };
}
function join(fm, body) {
  const f = fm.replace(/\s+$/, '');
  const b = (body || '').replace(/^\n+/, '');
  return f ? `---\n${f}\n---\n\n${b}` : b;
}

export function gmFieldList(fm) {
  const lines = (fm || '').replace(/\r\n/g, '\n').split('\n');
  const i = lines.findIndex((l) => /^gm_fields:/.test(l));
  if (i < 0) return [];
  const rest = lines[i].slice('gm_fields:'.length).trim();
  const clean = (s) => s.trim().replace(/^["']|["']$/g, '');
  if (rest.startsWith('[') && rest.endsWith(']')) return rest.slice(1, -1).split(',').map(clean).filter(Boolean);
  if (rest) return [clean(rest)];
  const out = [];
  for (let j = i + 1; j < lines.length && /^\s*-\s+/.test(lines[j]); j++) out.push(clean(lines[j].replace(/^\s*-\s+/, '')));
  return out;
}

export function isGmPage(fm) {
  const m = /^gm_only:\s*(.*)$/m.exec(fm || '');
  return !!m && ['true', 'yes', '1', 'x'].includes(m[1].trim().toLowerCase());
}

// Replace (or drop, or append) a top-level key including a block-list tail.
function setKey(fm, key, line) {
  const lines = fm ? fm.split('\n') : [];
  const i = lines.findIndex((l) => l.startsWith(`${key}:`));
  let end = i + 1;
  while (i >= 0 && end < lines.length && /^\s+\S|^\s*-\s/.test(lines[end])) end++;
  if (i >= 0) lines.splice(i, end - i, ...(line ? [line] : []));
  else if (line) lines.push(line);
  return lines.join('\n');
}

export function setGmField(content, key, on) {
  const { fm, body } = split(content);
  const cur = gmFieldList(fm).filter((k) => k !== key);
  if (on) cur.push(key);
  return join(setKey(fm, 'gm_fields', cur.length ? `gm_fields: [${cur.join(', ')}]` : ''), body);
}

export function setGmPage(content, on) {
  const { fm, body } = split(content);
  return join(setKey(fm, 'gm_only', on ? 'gm_only: true' : ''), body);
}
