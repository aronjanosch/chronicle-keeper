// CodeMirror 6 page editor (Phase 7.5). Edits the literal .md string — frontmatter
// and body together — so files stay truth (no document-model round-trip mangle).
// The vendored bundle is a single pre-built ESM file, lazy-loaded on first use.
import { openContextMenu } from './ui.js';
import { setOp } from './core.js';

let _cm = null;
export function loadCM() {
  if (!_cm) _cm = import('../vendor/codemirror.bundle.mjs');
  return _cm;
}

// Scriptorium-matched markdown highlight + chrome. Mirrors .ck-prose (app.css) so
// edit mode reads like the rendered page: serif body, small-caps h2/h3, inset code.
function buildTheme(cm) {
  const { HighlightStyle, syntaxHighlighting, tags, EditorView } = cm;
  const hl = HighlightStyle.define([
    { tag: tags.heading1, fontSize: '26px', fontWeight: '500', letterSpacing: '-0.015em', color: 'var(--ink)' },
    { tag: tags.heading2, fontSize: '12px', fontWeight: '600', letterSpacing: '0.12em', textTransform: 'uppercase', fontFamily: 'var(--font-ui)', color: 'var(--ink-faint)' },
    { tag: tags.heading3, fontSize: '11px', fontWeight: '600', letterSpacing: '0.12em', textTransform: 'uppercase', fontFamily: 'var(--font-ui)', color: 'var(--burgundy)' },
    { tag: tags.heading, fontWeight: '600', color: 'var(--ink)' },
    { tag: tags.strong, fontWeight: '600', color: 'var(--ink)' },
    { tag: tags.emphasis, fontStyle: 'italic', color: 'var(--ink-soft)' },
    { tag: tags.strikethrough, textDecoration: 'line-through', color: 'var(--ink-faint)' },
    { tag: tags.link, color: 'var(--burgundy)' },
    { tag: tags.url, color: 'var(--ink-blue)' },
    { tag: tags.monospace, color: 'var(--ink-soft)', fontFamily: 'var(--font-mono)', fontSize: '13px', background: 'var(--surface-inset)', borderRadius: '3px' },
    { tag: tags.contentSeparator, color: 'var(--ink-ghost)', fontFamily: 'var(--font-mono)' },
    { tag: tags.quote, color: 'var(--ink-muted)', fontStyle: 'italic' },
    { tag: tags.list, color: 'var(--burgundy)' },
    { tag: tags.processingInstruction, color: 'var(--ink-ghost)' },
  ]);
  const theme = EditorView.theme({
    '&': { color: 'var(--ink)', backgroundColor: 'transparent', fontSize: '15px' },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': { fontFamily: 'var(--font-display)', lineHeight: '1.7', overflow: 'visible' },
    '.cm-content': { padding: '8px 0', caretColor: 'var(--burgundy)' },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--burgundy)' },
    '.cm-selectionBackground, .cm-content ::selection': { backgroundColor: 'rgba(180,116,101,.25)' },
    '&.cm-focused .cm-selectionBackground': { backgroundColor: 'rgba(180,116,101,.3)' },
    '.cm-gutters': { backgroundColor: 'transparent', border: 'none', color: 'var(--ink-ghost)' },
    '.cm-foldGutter .cm-gutterElement': { padding: '0 4px 0 0', cursor: 'pointer' },
    '.cm-activeLine': { backgroundColor: 'rgba(120,90,40,.045)' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--ink-faint)' },
    '.cm-foldPlaceholder': { background: 'var(--paper-deep)', border: '1px solid var(--rule)', color: 'var(--ink-muted)' },

    // Decorations from inkDecorations(): wikilinks, tags, frontmatter block.
    '.cm-wikilink': { color: 'var(--burgundy)', borderBottom: '1px solid var(--burgundy-300)' },
    '.cm-wikilink-bracket': { color: 'var(--ink-ghost)' },
    '.cm-hashtag': { color: 'var(--ink-blue)', background: 'var(--ink-blue-50)', borderRadius: '4px', padding: '1px 4px', fontFamily: 'var(--font-ui)', fontSize: '12.5px' },
    // The closing --- makes markdown read the YAML as a setext heading; flatten
    // every inherited style so the block stays a quiet mono header.
    '.cm-fmLine, .cm-fmLine span': {
      fontFamily: 'var(--font-mono)', fontSize: '12.5px', fontWeight: 'normal', fontStyle: 'normal',
      textTransform: 'none', letterSpacing: 'normal', color: 'var(--ink-muted)',
      background: 'none', padding: '0', border: 'none', textDecoration: 'none',
    },

    '.cm-tooltip': { background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: '10px', boxShadow: 'var(--shadow-raised)', overflow: 'hidden' },
    '.cm-tooltip.cm-tooltip-autocomplete > ul': {
      fontFamily: 'var(--font-display)', fontSize: '13px', maxHeight: '17em', minWidth: '256px',
      padding: '4px', margin: '0', overflowY: 'auto', overflowX: 'hidden',
      scrollbarWidth: 'thin', scrollbarColor: 'var(--rule-strong) transparent',
    },
    '.cm-tooltip-autocomplete > ul > li[role=option]': {
      display: 'flex', alignItems: 'baseline', gap: '10px',
      padding: '5px 9px', borderRadius: '6px', lineHeight: '1.35', cursor: 'pointer', color: 'var(--ink-soft)',
    },
    '.cm-tooltip-autocomplete ul li[aria-selected]': { background: 'var(--burgundy-50)', color: 'var(--ink)' },
    '.cm-completionIcon': { display: 'none' },
    '.cm-completionLabel': { color: 'var(--ink)', flex: 'none' },
    '.cm-completionDetail': { color: 'var(--ink-faint)', fontStyle: 'normal', fontFamily: 'var(--font-mono)', fontSize: '10.5px', marginLeft: 'auto', whiteSpace: 'nowrap' },
    '.cm-completionSection': {
      color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)', fontSize: '9px', fontWeight: '600',
      letterSpacing: '0.11em', textTransform: 'uppercase', padding: '7px 9px 3px',
    },
    '.cm-completionSection:not(:first-child)': { borderTop: '1px solid var(--rule-soft)', marginTop: '3px' },
    // The app's global pill scrollbar leaks into the narrow popover; override it thin.
    '.cm-tooltip-autocomplete > ul::-webkit-scrollbar': { width: '9px' },
    '.cm-tooltip-autocomplete > ul::-webkit-scrollbar-track': { background: 'transparent' },
    '.cm-tooltip-autocomplete > ul::-webkit-scrollbar-thumb': {
      background: 'var(--rule-strong)', borderRadius: '5px', border: '2px solid var(--surface-raised)', backgroundClip: 'padding-box',
    },

    // Search & replace panel, restyled as an app toolbar.
    '.cm-panels': { background: 'var(--surface-raised)', color: 'var(--ink)' },
    '.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--rule)' },
    '.cm-panel.cm-search': { fontFamily: 'var(--font-ui)', fontSize: '12px', padding: '8px 10px', display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: '6px' },
    '.cm-panel.cm-search br': { display: 'none' },
    '.cm-panel.cm-search .cm-textfield': {
      background: 'var(--paper)', border: '1px solid var(--rule)', borderRadius: '6px',
      color: 'var(--ink)', fontFamily: 'var(--font-mono)', fontSize: '12.5px', padding: '4px 8px', margin: '0', width: '220px',
    },
    '.cm-panel.cm-search .cm-textfield:focus': { outline: 'none', borderColor: 'var(--burgundy-300)' },
    '.cm-panel.cm-search .cm-button': {
      background: 'var(--surface)', backgroundImage: 'none', border: '1px solid var(--rule-strong)', borderRadius: '6px',
      color: 'var(--ink-soft)', fontFamily: 'var(--font-ui)', fontSize: '12px', padding: '4px 10px', margin: '0', cursor: 'pointer',
    },
    '.cm-panel.cm-search .cm-button:hover': { background: 'var(--paper-deep)' },
    '.cm-panel.cm-search label': { display: 'inline-flex', alignItems: 'center', gap: '4px', color: 'var(--ink-muted)', fontSize: '11.5px', textTransform: 'lowercase' },
    '.cm-panel.cm-search input[type=checkbox]': { accentColor: 'var(--burgundy)', margin: '0' },
    '.cm-panel.cm-search [name=close]': { color: 'var(--ink-faint)', fontSize: '16px', cursor: 'pointer', top: '6px', right: '8px' },
    '.cm-searchMatch': { backgroundColor: 'rgba(180,116,101,.25)' },
    '.cm-searchMatch-selected': { backgroundColor: 'var(--burgundy-50)', outline: '1px solid var(--burgundy-300)' },
  }, { dark: false });
  return [theme, syntaxHighlighting(hl)];
}

// Marks for syntax the markdown parser doesn't know: [[wikilinks]], #tags, and the
// YAML frontmatter block (styled as a quiet mono header, not body prose).
function inkDecorations(cm) {
  const { ViewPlugin, Decoration, MatchDecorator, RangeSetBuilder } = cm;
  const plug = (deco) => ViewPlugin.fromClass(class {
    constructor(view) { this.decorations = deco.createDeco(view); }
    update(u) { this.decorations = deco.updateDeco(u, this.decorations); }
  }, { decorations: (v) => v.decorations });

  const bracketMark = Decoration.mark({ class: 'cm-wikilink-bracket' });
  const linkMark = Decoration.mark({ class: 'cm-wikilink' });
  const wiki = new MatchDecorator({
    regexp: /(!?\[\[)([^\]\n]+)(\]\])/g,
    decorate(add, from, to, m) {
      add(from, from + m[1].length, bracketMark);
      add(from + m[1].length, to - 2, linkMark);
      add(to - 2, to, bracketMark);
    },
  });

  const tagMark = Decoration.mark({ class: 'cm-hashtag' });
  const tag = new MatchDecorator({
    regexp: /(^|\s)(#[A-Za-z][\w/-]*)/g,
    decorate(add, from, to, m) { add(to - m[2].length, to, tagMark); },
  });

  const fmLine = Decoration.line({ class: 'cm-fmLine' });
  const frontmatter = ViewPlugin.fromClass(class {
    constructor(view) { this.decorations = this.build(view); }
    update(u) { if (u.docChanged) this.decorations = this.build(u.view); }
    build(view) {
      const b = new RangeSetBuilder();
      const doc = view.state.doc;
      if (doc.lines > 1 && doc.line(1).text.trim() === '---') {
        let end = 0;
        for (let i = 2; i <= Math.min(doc.lines, 80); i++) {
          if (doc.line(i).text.trim() === '---') { end = i; break; }
        }
        for (let i = 1; i <= end; i++) b.add(doc.line(i).from, doc.line(i).from, fmLine);
      }
      return b.finish();
    }
  }, { decorations: (v) => v.decorations });

  return [plug(wiki), plug(tag), frontmatter];
}

// ── Selection-wrap commands (⌘B / ⌘I / ⌘L) ──────────────────────────────
function wrapWith(cm, before, after) {
  return (view) => {
    const { state } = view;
    const changes = [];
    let sel = null;
    for (const r of state.selection.ranges) {
      changes.push({ from: r.from, insert: before }, { from: r.to, insert: after });
      if (r.empty) sel = cm.EditorSelection.cursor(r.from + before.length);
    }
    view.dispatch(state.update({
      changes,
      selection: sel || undefined,
      scrollIntoView: true,
    }, { userEvent: 'input.format' }));
    return true;
  };
}

// ⌘L: wrap the selected text as a [[wikilink]] (empty → open completion).
function wrapLink(cm) {
  return (view) => {
    const r = view.state.selection.main;
    if (r.empty) {
      view.dispatch(view.state.update({ changes: { from: r.from, insert: '[[]]' }, selection: cm.EditorSelection.cursor(r.from + 2) }));
      return true;
    }
    return wrapWith(cm, '[[', ']]')(view);
  };
}

// [text](url): wrap the selection as a markdown link, caret inside the parens.
function mdLink() {
  return (view) => {
    const r = view.state.selection.main;
    const text = view.state.sliceDoc(r.from, r.to);
    const insert = `[${text}]()`;
    view.dispatch({
      changes: { from: r.from, to: r.to, insert },
      selection: { anchor: r.from + insert.length - 1 },
      userEvent: 'input.format',
    });
    return true;
  };
}

// ── "Turn into" line transforms (14B) ────────────────────────────
// Existing block prefix (heading / quote / callout head / list / task) that a
// transform replaces; group 1 keeps the indent.
const LINE_PREFIX = /^(\s*)(?:#{1,6}\s+|>\s?(?:\[!\w+\][+-]?\s?)?|(?:[-*+]|\d+\.)\s+(?:\[[ xX]\]\s+)?)?/;

function turnInto(kind) {
  return (view) => {
    const { state } = view;
    const r = state.selection.main;
    const fromLine = state.doc.lineAt(r.from), toLine = state.doc.lineAt(r.to);
    const changes = [];
    let n = 1;
    for (let i = fromLine.number; i <= toLine.number; i++) {
      const line = state.doc.line(i);
      if (!line.text.trim() && kind !== 'callout') continue;
      const prefix = { h1: '# ', h2: '## ', h3: '### ', list: '- ', task: '- [ ] ', quote: '> ', callout: '> ' }[kind]
        || (kind === 'numbered' ? `${n++}. ` : '');
      const m = LINE_PREFIX.exec(line.text);
      changes.push({ from: line.from + m[1].length, to: line.from + m[0].length, insert: prefix });
    }
    if (kind === 'callout') changes.push({ from: fromLine.from, insert: '> [!note]\n' });
    if (changes.length) view.dispatch({ changes, userEvent: 'input.format', scrollIntoView: true });
    return true;
  };
}

// ── Selection context menu + bubble toolbar (14B) ─────────────────
function cutCopy(view, cut) {
  const r = view.state.selection.main;
  const text = view.state.sliceDoc(r.from, r.to);
  navigator.clipboard.writeText(text).then(() => {
    if (cut) view.dispatch({ changes: { from: r.from, to: r.to }, userEvent: 'delete.cut' });
  }, (e) => setOp(`Copy failed: ${e.message}`, 'err'));
}

function pasteClipboard(view) {
  navigator.clipboard.readText().then(
    (t) => { if (t) insertAtSelection(view, t); },
    (e) => setOp(`Paste failed: ${e.message}`, 'err'),
  );
}

// Selection → new page via opts.onExtract(text) (resolves with the created
// page, or never on cancel); a [[link]] replaces the selection.
function extractSelection(view, opts) {
  const r = view.state.selection.main;
  const text = view.state.sliceDoc(r.from, r.to);
  Promise.resolve(opts.onExtract(text)).then((page) => {
    if (!page) return;
    if (view.state.sliceDoc(r.from, r.to) !== text) {
      setOp(`Created “${page.title}” — text changed, link it yourself`, 'err');
      return;
    }
    view.dispatch({
      changes: { from: r.from, to: r.to, insert: `[[${page.title}]]` },
      userEvent: 'input.format', scrollIntoView: true,
    });
  }).catch(() => {});
}

function selectionItems(cm, view, opts) {
  const run = (f) => () => { f(view); view.focus(); };
  return [
    { label: 'Format', icon: 'edit', children: [
      { label: 'Bold', onClick: run(wrapWith(cm, '**', '**')) },
      { label: 'Italic', onClick: run(wrapWith(cm, '*', '*')) },
      { label: 'Code', onClick: run(wrapWith(cm, '`', '`')) },
      { label: 'Highlight', onClick: run(wrapWith(cm, '==', '==')) },
      { label: 'Link', onClick: run(mdLink()) },
    ] },
    { label: 'Turn into', icon: 'doc', children: [
      { label: 'Heading 1', onClick: run(turnInto('h1')) },
      { label: 'Heading 2', onClick: run(turnInto('h2')) },
      { label: 'Heading 3', onClick: run(turnInto('h3')) },
      { label: 'Bullet list', onClick: run(turnInto('list')) },
      { label: 'Numbered list', onClick: run(turnInto('numbered')) },
      { label: 'Task list', onClick: run(turnInto('task')) },
      { label: 'Quote', onClick: run(turnInto('quote')) },
      { label: 'Callout', onClick: run(turnInto('callout')) },
    ] },
    '-',
    { label: 'Wrap as [[wikilink]]', icon: 'link', onClick: run(wrapLink(cm)) },
    opts.onExtract && { label: 'Extract to new page', icon: 'export', onClick: () => extractSelection(view, opts) },
    opts.onQuote && { label: 'Send to Keeper as quote', icon: 'feather', onClick: () => opts.onQuote(view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to)) },
    '-',
    { label: 'Cut', onClick: () => cutCopy(view, true) },
    { label: 'Copy', onClick: () => cutCopy(view, false) },
    { label: 'Paste', onClick: () => pasteClipboard(view) },
  ];
}

function selectionMenu(cm, opts) {
  return cm.EditorView.domEventHandlers({
    contextmenu(e, view) {
      if (view.state.selection.main.empty) return false; // default webview menu
      openContextMenu(e, selectionItems(cm, view, opts));
      return true;
    },
  });
}

// Floating mini-toolbar above the selection. Hand-rolled DOM (no Preact, no CM
// tooltip dependency); shown on mouseup/keyup once a selection exists.
function bubbleToolbar(cm, view, opts) {
  const el = document.createElement('div');
  el.className = 'ck-bubble';
  el.style.display = 'none';
  const add = (label, title, fn, cls) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = label;
    b.title = title;
    if (cls) b.className = cls;
    b.addEventListener('mousedown', (e) => e.preventDefault()); // keep editor selection
    b.addEventListener('click', () => { fn(view); hide(); view.focus(); });
    el.appendChild(b);
  };
  add('B', 'Bold', wrapWith(cm, '**', '**'), 'ck-bb-b');
  add('I', 'Italic', wrapWith(cm, '*', '*'), 'ck-bb-i');
  add('<>', 'Code', wrapWith(cm, '`', '`'));
  add('==', 'Highlight', wrapWith(cm, '==', '=='), 'ck-bb-hl');
  add('[[ ]]', 'Wrap as wikilink', wrapLink(cm));
  if (opts.onExtract) add('→□', 'Extract to new page', (v) => extractSelection(v, opts));
  document.body.appendChild(el);

  const hide = () => { el.style.display = 'none'; };
  const place = () => {
    const r = view.state.selection.main;
    // activeElement check, not view.hasFocus — the latter is false whenever the
    // window itself is unfocused (background window, headless), hiding the bar.
    if (r.empty || !view.dom.contains(document.activeElement)) { hide(); return; }
    const head = view.coordsAtPos(r.from);
    if (!head) { hide(); return; }
    el.style.display = 'flex';
    const w = el.offsetWidth;
    el.style.left = `${Math.max(8, Math.min(head.left, window.innerWidth - w - 8))}px`;
    el.style.top = `${Math.max(8, head.top - el.offsetHeight - 8)}px`;
  };
  let timer = null;
  const schedule = () => { if (timer) clearTimeout(timer); timer = setTimeout(place, 180); };
  const onScroll = () => hide();
  const onBlur = () => setTimeout(() => { if (!view.hasFocus) hide(); }, 120);
  view.dom.addEventListener('mouseup', schedule);
  view.dom.addEventListener('keyup', schedule);
  view.dom.addEventListener('blur', onBlur, true);
  document.addEventListener('scroll', onScroll, true);
  return {
    destroy() {
      if (timer) clearTimeout(timer);
      view.dom.removeEventListener('mouseup', schedule);
      view.dom.removeEventListener('keyup', schedule);
      view.dom.removeEventListener('blur', onBlur, true);
      document.removeEventListener('scroll', onScroll, true);
      el.remove();
    },
  };
}

// ── Completions: [[Page]] and #tag, off the live page index ─────────────
function wikilinkSource(cm, getPages, onCreatePage) {
  return (ctx) => {
    const m = ctx.matchBefore(/\[\[[^\]\n|#]*$/);
    if (!m || (m.from === m.to && !ctx.explicit)) return null;
    const from = m.from + 2;
    const q = ctx.state.sliceDoc(from, ctx.pos).toLowerCase();
    const pages = getPages() || [];
    const options = pages
      .filter((p) => p.title && (p.title.toLowerCase().includes(q) || (p.aliases || []).some((a) => a.includes(q))))
      .slice(0, 8)
      .map((p) => ({ label: p.title, detail: p.kind || 'page', type: 'class', apply: applyLink(cm, p.title) }));
    if (q.trim()) {
      const exact = pages.some((p) => p.title.toLowerCase() === q);
      if (!exact) {
        const name = ctx.state.sliceDoc(from, ctx.pos).trim();
        const create = (kind) => (view) => {
          if (onCreatePage) onCreatePage(name, kind);
          applyLink(cm, name)(view, null, from, view.state.selection.main.head);
        };
        options.push(
          { label: `Create “${name}”`, type: 'keyword', apply: create(undefined) },
          { label: `Create event “${name}”`, detail: 'dated page', type: 'keyword', apply: create('event') },
        );
      }
    }
    return { from, options, validFor: /^[^\]\n|#]*$/ };
  };
}

// Insert `Title]]`, swallowing any auto-closed `]]` already after the caret.
function applyLink(cm, title) {
  return (view, completion, from, to) => {
    const line = view.state.doc.lineAt(to);
    const after = view.state.sliceDoc(to, line.to);
    const eat = after.startsWith(']]') ? 2 : 0;
    const insert = `${title}]]`;
    view.dispatch({ changes: { from, to: to + eat, insert }, selection: { anchor: from + insert.length } });
  };
}

function tagSource(getPages) {
  return (ctx) => {
    const m = ctx.matchBefore(/(^|\s)#[\w/-]*$/);
    if (!m) return null;
    const hash = ctx.state.sliceDoc(m.from, ctx.pos).indexOf('#') + m.from;
    const seen = new Set();
    for (const p of getPages() || []) for (const t of p.tags || []) seen.add(String(t).replace(/^#/, ''));
    if (!seen.size) return null;
    return {
      from: hash,
      options: [...seen].sort().map((t) => ({ label: `#${t}`, type: 'keyword' })),
      validFor: /^#[\w/-]*$/,
    };
  };
}

// ── Paste & drop smarts (Phase 7.5 H) ────────────────────────────
function insertAtSelection(view, text, userEvent = 'input.paste') {
  const r = view.state.selection.main;
  view.dispatch({
    changes: { from: r.from, to: r.to, insert: text },
    selection: { anchor: r.from + text.length },
    userEvent,
    scrollIntoView: true,
  });
}

function tableToMarkdown(rows) {
  const esc = (c) => String(c == null ? '' : c).trim().replace(/\s+/g, ' ').replace(/\|/g, '\\|');
  const width = Math.max(...rows.map((r) => r.length));
  const pad = (r) => { const o = r.map(esc); while (o.length < width) o.push(''); return o; };
  const line = (r) => `| ${pad(r).join(' | ')} |`;
  return [line(rows[0]), `| ${Array(width).fill('---').join(' | ')} |`, ...rows.slice(1).map(line)].join('\n');
}

function htmlTableRows(html) {
  try {
    const table = new DOMParser().parseFromString(html, 'text/html').querySelector('table');
    if (!table) return null;
    const rows = [...table.rows].map((tr) => [...tr.cells].map((td) => td.textContent));
    return rows.length ? rows : null;
  } catch (_) { return null; }
}

function tsvRows(text) {
  const lines = (text || '').replace(/\r/g, '').split('\n').filter((l) => l.trim());
  if (lines.length < 2 || !lines.every((l) => l.includes('\t'))) return null;
  return lines.map((l) => l.split('\t'));
}

async function uploadImages(view, files, onUploadAsset) {
  for (const f of files) {
    const fromType = ((f.type || '').split('/')[1] || 'png').match(/^[a-z0-9]+/i);
    const ext = (fromType ? fromType[0].toLowerCase() : 'png').replace(/^jpeg$/, 'jpg');
    const name = f.name && /\.[A-Za-z0-9]+$/.test(f.name) ? f.name : `Pasted image.${ext}`;
    try {
      const saved = await onUploadAsset(name, f);
      insertAtSelection(view, `![[${saved.name}]]`);
    } catch (_) { /* upload failed — leave the doc untouched */ }
  }
}

function pasteDrop(cm, opts) {
  return cm.EditorView.domEventHandlers({
    paste(e, view) {
      const cd = e.clipboardData;
      if (!cd) return false;
      const imgs = [...cd.items]
        .filter((it) => it.kind === 'file' && it.type.startsWith('image/'))
        .map((it) => it.getAsFile()).filter(Boolean);
      if (imgs.length && opts.onUploadAsset) {
        e.preventDefault();
        uploadImages(view, imgs, opts.onUploadAsset);
        return true;
      }
      const text = cd.getData('text/plain') || '';
      const r = view.state.selection.main;
      if (!r.empty && /^https?:\/\/\S+$/.test(text.trim())) {
        e.preventDefault();
        insertAtSelection(view, `[${view.state.sliceDoc(r.from, r.to)}](${text.trim()})`);
        return true;
      }
      const html = cd.getData('text/html') || '';
      const rows = (html.includes('<table') ? htmlTableRows(html) : null) || tsvRows(text);
      if (rows) {
        e.preventDefault();
        insertAtSelection(view, tableToMarkdown(rows));
        return true;
      }
      return false;
    },
    drop(e, view) {
      const files = [...((e.dataTransfer && e.dataTransfer.files) || [])]
        .filter((f) => f.type.startsWith('image/'));
      if (!files.length || !opts.onUploadAsset) return false;
      e.preventDefault();
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos != null) view.dispatch({ selection: { anchor: pos } });
      uploadImages(view, files, opts.onUploadAsset);
      return true;
    },
  });
}

// ── Slash menu (Phase 7.5 I): block inserts on an empty line ─────
const SLASH_ITEMS = [
  { label: '/h1', detail: 'Heading 1', insert: '# ' },
  { label: '/h2', detail: 'Heading 2', insert: '## ' },
  { label: '/h3', detail: 'Heading 3', insert: '### ' },
  { label: '/list', detail: 'Bullet list', insert: '- ' },
  { label: '/numbered', detail: 'Numbered list', insert: '1. ' },
  { label: '/task', detail: 'Task list', insert: '- [ ] ' },
  { label: '/table', detail: 'Table', insert: '| Column | Column |\n| --- | --- |\n|  |  |', cursor: 2 },
  { label: '/quote', detail: 'Quote', insert: '> ' },
  { label: '/note', detail: 'Callout', insert: '> [!note] ' },
  { label: '/secret', detail: 'GM secret callout', insert: '> [!secret] ' },
  { label: '/warning', detail: 'Warning callout', insert: '> [!warning] ' },
  { label: '/code', detail: 'Code block', insert: '```\n\n```', cursor: 4 },
  { label: '/divider', detail: 'Horizontal rule', insert: '---\n' },
];

// Phase 20A: three visually-separated groups so instant inserts never blur with
// AI actions. Ranks fix order (GENERATE rank 2 reserved for 20B generators).
const SEC_INSERT = { name: 'Insert', rank: 1 };
const SEC_ASK = { name: 'Ask Keeper', rank: 3 };

// Open the Keeper from the cursor with a prefilled composer (pull-not-push:
// nothing fires until the user sends). `skill` = a kind-matched skill row, or
// null for an ad-hoc `/keeper <text>` prompt; the current selection is attached.
function keeperApply(view, f, to, onAskKeeper, arg) {
  view.dispatch({ changes: { from: f, to, insert: '' } });
  const { from, to: selTo } = view.state.selection.main;
  const oa = onAskKeeper && onAskKeeper();
  if (oa) oa({ ...arg, selection: view.state.sliceDoc(from, selTo) });
}

function slashSource(cm, getSnippets, getSkills, onAskKeeper) {
  return (ctx) => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = ctx.state.sliceDoc(line.from, ctx.pos);
    const m = /^\s*\/[\w-]*$/.exec(before);
    if (!m) return null;
    const from = line.from + before.indexOf('/');
    const inserts = [
      ...SLASH_ITEMS,
      // 11.5H: link-or-create an event page via the [[ completion's
      // "Create event" option.
      { label: '/event', detail: 'Link an event page', insert: '[[', complete: true },
      ...((getSnippets && getSnippets()) || []).map((s) => ({
        label: '/' + s.name.toLowerCase().replace(/\s+/g, '-'),
        detail: 'snippet',
        insert: s.content.replace(/\n+$/, '\n'),
      })),
    ].map((it) => ({
      ...it, section: SEC_INSERT, type: 'keyword',
      apply: (view, _c, f, to) => {
        view.dispatch({
          changes: { from: f, to, insert: it.insert },
          selection: { anchor: f + (it.cursor != null ? it.cursor : it.insert.length) },
          userEvent: 'input.complete',
        });
        if (it.complete) cm.startCompletion(view);
      },
    }));
    const ask = [
      ...((getSkills && getSkills()) || []).map((s) => ({
        label: '/' + s.slug, detail: s.description || s.name,
        section: SEC_ASK, type: 'keyword',
        apply: (view, _c, f, to) => keeperApply(view, f, to, onAskKeeper, { skill: s }),
      })),
      {
        label: '/keeper', detail: 'Ask the Keeper…',
        section: SEC_ASK, type: 'keyword',
        apply: (view, _c, f, to) => keeperApply(view, f, to, onAskKeeper, { skill: null }),
      },
    ];
    return { from, options: [...inserts, ...ask], validFor: /^\/[\w-]*$/ };
  };
}

// `/keeper <free text>` — an ad-hoc prompt from the cursor. Separate source
// because the line carries a space (slashSource's token regex has closed).
function keeperPromptSource(onAskKeeper) {
  return (ctx) => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = ctx.state.sliceDoc(line.from, ctx.pos);
    const m = /^\s*\/keeper\s+(.+)$/.exec(before);
    if (!m) return null;
    const text = m[1];
    const from = line.from + before.indexOf('/');
    return {
      from,
      options: [{
        label: `Ask Keeper: ${text}`, type: 'keyword', section: SEC_ASK,
        apply: (view, _c, f, to) => keeperApply(view, f, to, onAskKeeper, { skill: null, prompt: text }),
      }],
      validFor: /^\/keeper\s+.+$/,
    };
  };
}

// ── Singleton view + per-tab state cache (Phase 15C) ─────────────
// One EditorView lives for the whole session; mountEditor() re-parents its DOM
// and swaps EditorStates, so cursor, selection, and undo history survive tab
// switches. Extensions are built once and read the current mount's callbacks
// through `ed.opts`, never through closures over a particular mount.
let ed = null;                 // { cm, view, opts, pending, saveTimer, flush, cmdRuns, extensions }
const stateCache = new Map();  // opts.cacheKey → EditorState
const CACHE_CAP = 16;

function cacheSet(key, state) {
  stateCache.delete(key);
  stateCache.set(key, state);
  if (stateCache.size > CACHE_CAP) stateCache.delete(stateCache.keys().next().value);
}

function createSingleton(cm) {
  const {
    EditorState, EditorView, keymap, highlightActiveLine, drawSelection, dropCursor,
    history, historyKeymap, defaultKeymap, indentWithTab, indentMore, indentLess, Prec,
    foldGutter, foldKeymap, codeFolding, indentOnInput, bracketMatching,
    markdown, markdownLanguage, insertNewlineContinueMarkup, deleteMarkupBackward,
    search, searchKeymap, highlightSelectionMatches,
    autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap,
  } = cm;

  const s = { cm, opts: {}, pending: { dirty: false, doc: '' }, saveTimer: null };
  // Mount-independent views of the current callbacks for the event-time readers.
  const live = {
    get onUploadAsset() { return s.opts.onUploadAsset; },
    get onExtract() { return s.opts.onExtract; },
    get onQuote() { return s.opts.onQuote; },
  };

  const listKeys = Prec.high(keymap.of([
    { key: 'Enter', run: insertNewlineContinueMarkup },
    { key: 'Backspace', run: deleteMarkupBackward },
    { key: 'Tab', run: indentMore, shift: indentLess },
  ]));
  const formatKeys = Prec.high(keymap.of([
    { key: 'Mod-b', run: wrapWith(cm, '**', '**') },
    { key: 'Mod-i', run: wrapWith(cm, '*', '*') },
    { key: 'Mod-l', run: wrapLink(cm) },
  ]));

  s.flush = () => {
    if (s.saveTimer) { clearTimeout(s.saveTimer); s.saveTimer = null; }
    const { pending, opts } = s;
    if (!pending.dirty) return;
    if (opts.onState) opts.onState('saving');
    Promise.resolve(opts.onSave(pending.doc))
      .then(() => { pending.dirty = false; if (opts.onState) opts.onState('saved'); })
      .catch(() => { if (opts.onState) opts.onState('dirty'); });
  };
  const onDoc = EditorView.updateListener.of((u) => {
    if (!u.docChanged) return;
    s.pending.doc = u.state.doc.toString();
    s.pending.dirty = true;
    if (s.opts.onState) s.opts.onState('dirty');
    if (s.saveTimer) clearTimeout(s.saveTimer);
    s.saveTimer = setTimeout(s.flush, 800);
  });

  s.extensions = [
    foldGutter(), codeFolding(),
    history(), drawSelection(), dropCursor(),
    EditorState.allowMultipleSelections.of(true),
    indentOnInput(), bracketMatching(), closeBrackets(),
    highlightActiveLine(), highlightSelectionMatches(),
    markdown({ base: markdownLanguage }),
    autocompletion({
      override: [
        wikilinkSource(cm, () => s.opts.getPages && s.opts.getPages(), (name, kind) => s.opts.onCreatePage && s.opts.onCreatePage(name, kind)),
        tagSource(() => s.opts.getPages && s.opts.getPages()),
        slashSource(cm,
          () => s.opts.getSnippets && s.opts.getSnippets(),
          () => s.opts.getSkills && s.opts.getSkills(),
          () => s.opts.onAskKeeper),
        keeperPromptSource(() => s.opts.onAskKeeper),
      ],
      icons: false,
    }),
    pasteDrop(cm, live),
    selectionMenu(cm, live),
    search({ top: true }),
    buildTheme(cm),
    inkDecorations(cm),
    EditorView.lineWrapping,
    formatKeys, listKeys,
    keymap.of([...closeBracketsKeymap, ...searchKeymap, ...completionKeymap, ...foldKeymap, ...historyKeymap, indentWithTab, ...defaultKeymap]),
    onDoc,
  ];
  s.view = new EditorView({ state: EditorState.create({ doc: '', extensions: s.extensions }) });

  // 14 D+E: commands routed from the native menu / global shortcuts.
  s.cmdRuns = {
    'fmt-bold': wrapWith(cm, '**', '**'), 'fmt-italic': wrapWith(cm, '*', '*'),
    'fmt-code': wrapWith(cm, '`', '`'), 'fmt-highlight': wrapWith(cm, '==', '=='),
    'fmt-wikilink': wrapLink(cm),
    'fmt-h1': turnInto('h1'), 'fmt-h2': turnInto('h2'), 'fmt-h3': turnInto('h3'),
    'fmt-list': turnInto('list'), 'fmt-quote': turnInto('quote'), 'fmt-callout': turnInto('callout'),
  };
  return s;
}

// Scroll the live editor to the i-th body heading — mirrors ReadView's outline
// indexing (frontmatter dropped, leading H1 title dropped, fence-aware) so the
// rail's Outline jumps to the same place in read and edit mode. Returns false
// when there's no mounted view yet or the index is out of range.
export function gotoHeading(index) {
  if (!ed || !ed.view) return false;
  const view = ed.view;
  const full = view.state.doc.toString();
  let body = full, base = 0;
  const fm = full.match(/^---\n[\s\S]*?\n---\n?/);
  if (fm) { base = fm[0].length; body = full.slice(base); }
  const h1 = body.match(/^\s*#\s+.*\n+/);
  if (h1) { base += h1[0].length; body = body.slice(h1[0].length); }
  let pos = base, count = -1, fence = false;
  for (const raw of body.split('\n')) {
    const line = raw.trim();
    if (/^(```|~~~)/.test(line)) fence = !fence;
    else if (!fence && /^#{1,3}\s+/.test(line) && ++count === index) {
      view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
      view.focus();
      return true;
    }
    pos += raw.length + 1;
  }
  return false;
}

// Mount the editor into `host`. `opts.cacheKey` (world:path) keys the per-tab
// EditorState cache — a hit restores cursor/undo, but only while the cached
// doc still matches `opts.doc` (external edits and history restores miss).
// Returns { destroy, view }.
export async function mountEditor(host, opts) {
  const cm = await loadCM();
  if (!ed) ed = createSingleton(cm);
  ed.flush();
  ed.opts = opts;
  ed.pending = { dirty: false, doc: opts.doc };
  host.appendChild(ed.view.dom);
  const cached = opts.cacheKey ? stateCache.get(opts.cacheKey) : null;
  ed.view.setState(cached && cached.doc.toString() === opts.doc
    ? cached
    : cm.EditorState.create({ doc: opts.doc, extensions: ed.extensions }));
  ed.view.focus();
  const bubble = bubbleToolbar(cm, ed.view, opts);

  const onCmd = (e) => {
    const id = e.detail;
    if (id === 'save') ed.flush();
    else if (id === 'find') { cm.openSearchPanel(ed.view); }
    else if (ed.cmdRuns[id]) { ed.cmdRuns[id](ed.view); ed.view.focus(); }
  };
  window.addEventListener('ck:cmd', onCmd);

  let dead = false;
  return {
    view: ed.view,
    destroy() {
      if (dead) return;
      dead = true;
      window.removeEventListener('ck:cmd', onCmd);
      ed.flush();
      if (opts.cacheKey) cacheSet(opts.cacheKey, ed.view.state);
      bubble.destroy();
      if (ed.view.dom.parentNode === host) host.removeChild(ed.view.dom);
    },
  };
}
