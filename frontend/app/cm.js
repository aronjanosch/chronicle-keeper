// CodeMirror 6 page editor (Phase 7.5). Edits the literal .md string — frontmatter
// and body together — so files stay truth (no document-model round-trip mangle).
// The vendored bundle is a single pre-built ESM file, lazy-loaded on first use.

let _cm = null;
export function loadCM() {
  if (!_cm) _cm = import('../vendor/codemirror.bundle.mjs');
  return _cm;
}

// Scriptorium-matched markdown highlight + chrome. Tokens come from tokens.css.
function buildTheme(cm) {
  const { HighlightStyle, syntaxHighlighting, tags, EditorView } = cm;
  const hl = HighlightStyle.define([
    { tag: tags.heading1, fontSize: '1.5em', fontWeight: '600', color: 'var(--ink)' },
    { tag: tags.heading2, fontSize: '1.25em', fontWeight: '600', color: 'var(--ink)' },
    { tag: tags.heading3, fontSize: '1.1em', fontWeight: '600', color: 'var(--ink)' },
    { tag: tags.heading, fontWeight: '600', color: 'var(--ink)' },
    { tag: tags.strong, fontWeight: '700', color: 'var(--ink)' },
    { tag: tags.emphasis, fontStyle: 'italic', color: 'var(--ink-soft)' },
    { tag: tags.strikethrough, textDecoration: 'line-through', color: 'var(--ink-faint)' },
    { tag: tags.link, color: 'var(--burgundy)' },
    { tag: tags.url, color: 'var(--ink-blue)' },
    { tag: [tags.monospace, tags.contentSeparator], color: 'var(--ink-soft)', fontFamily: 'var(--font-mono)' },
    { tag: tags.quote, color: 'var(--ink-muted)', fontStyle: 'italic' },
    { tag: tags.list, color: 'var(--burgundy)' },
    { tag: tags.processingInstruction, color: 'var(--ink-faint)' },
  ]);
  const theme = EditorView.theme({
    '&': { color: 'var(--ink)', backgroundColor: 'transparent', fontSize: '14px' },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: '1.7', overflow: 'visible' },
    '.cm-content': { padding: '8px 0', caretColor: 'var(--burgundy)' },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--burgundy)' },
    '.cm-selectionBackground, .cm-content ::selection': { backgroundColor: 'rgba(180,116,101,.25)' },
    '&.cm-focused .cm-selectionBackground': { backgroundColor: 'rgba(180,116,101,.3)' },
    '.cm-gutters': { backgroundColor: 'transparent', border: 'none', color: 'var(--ink-ghost)' },
    '.cm-activeLine': { backgroundColor: 'rgba(120,90,40,.045)' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--ink-faint)' },
    '.cm-foldPlaceholder': { background: 'var(--paper-deep)', border: '1px solid var(--rule)', color: 'var(--ink-muted)' },
    '.cm-tooltip': { background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: '8px', boxShadow: 'var(--shadow-raised)', overflow: 'hidden' },
    '.cm-tooltip.cm-tooltip-autocomplete > ul': { fontFamily: 'var(--font-display)', fontSize: '13px', maxHeight: '16em' },
    '.cm-tooltip-autocomplete ul li[aria-selected]': { background: 'var(--burgundy-50)', color: 'var(--ink)' },
    '.cm-completionIcon': { display: 'none' },
    '.cm-completionLabel': { color: 'var(--ink)' },
    '.cm-completionDetail': { color: 'var(--ink-faint)', fontStyle: 'normal', fontFamily: 'var(--font-mono)', fontSize: '10.5px' },
    '.cm-panels': { background: 'var(--surface-raised)', color: 'var(--ink)' },
    '.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--rule)' },
    '.cm-panel.cm-search input, .cm-panel.cm-search button': { fontFamily: 'var(--font-mono)', fontSize: '12px' },
    '.cm-panel.cm-search input': { background: 'var(--paper)', border: '1px solid var(--rule)', borderRadius: '4px', color: 'var(--ink)' },
    '.cm-searchMatch': { backgroundColor: 'rgba(180,116,101,.25)' },
    '.cm-searchMatch-selected': { backgroundColor: 'var(--burgundy-50)', outline: '1px solid var(--burgundy-300)' },
  }, { dark: false });
  return [theme, syntaxHighlighting(hl)];
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
        options.push({
          label: `Create “${name}”`, type: 'keyword',
          apply: (view) => { onCreatePage && onCreatePage(name); applyLink(cm, name)(view, null, from, view.state.selection.main.head); },
        });
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

function slashSource() {
  return (ctx) => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = ctx.state.sliceDoc(line.from, ctx.pos);
    const m = /^\s*\/[\w-]*$/.exec(before);
    if (!m) return null;
    const from = line.from + before.indexOf('/');
    return {
      from,
      options: SLASH_ITEMS.map((it) => ({
        label: it.label, detail: it.detail, type: 'keyword',
        apply: (view, _c, f, to) => {
          view.dispatch({
            changes: { from: f, to, insert: it.insert },
            selection: { anchor: f + (it.cursor != null ? it.cursor : it.insert.length) },
            userEvent: 'input.complete',
          });
        },
      })),
      validFor: /^\/[\w-]*$/,
    };
  };
}

// Build the editor. `host` = parent element. Returns { destroy, view }.
export async function mountEditor(host, opts) {
  const cm = await loadCM();
  const {
    EditorState, EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection, dropCursor,
    history, historyKeymap, defaultKeymap, indentWithTab, indentMore, indentLess, Prec,
    foldGutter, foldKeymap, codeFolding, indentOnInput, bracketMatching, syntaxHighlighting: _sh,
    markdown, markdownLanguage, insertNewlineContinueMarkup, deleteMarkupBackward,
    search, searchKeymap, highlightSelectionMatches,
    autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap,
  } = cm;

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

  let saveTimer = null;
  const pending = { dirty: false, doc: opts.doc };
  const flush = () => {
    if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }
    if (!pending.dirty) return;
    if (opts.onState) opts.onState('saving');
    Promise.resolve(opts.onSave(pending.doc))
      .then(() => { pending.dirty = false; if (opts.onState) opts.onState('saved'); })
      .catch(() => { if (opts.onState) opts.onState('dirty'); });
  };
  const onDoc = EditorView.updateListener.of((u) => {
    if (!u.docChanged) return;
    pending.doc = u.state.doc.toString();
    pending.dirty = true;
    if (opts.onState) opts.onState('dirty');
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, 800);
  });

  const view = new EditorView({
    parent: host,
    state: EditorState.create({
      doc: opts.doc,
      extensions: [
        lineNumbers(), foldGutter(), codeFolding(),
        history(), drawSelection(), dropCursor(),
        EditorState.allowMultipleSelections.of(true),
        indentOnInput(), bracketMatching(), closeBrackets(),
        highlightActiveLine(), highlightSelectionMatches(),
        markdown({ base: markdownLanguage }),
        autocompletion({ override: [wikilinkSource(cm, opts.getPages, opts.onCreatePage), tagSource(opts.getPages), slashSource()], icons: false }),
        pasteDrop(cm, opts),
        search({ top: true }),
        buildTheme(cm),
        EditorView.lineWrapping,
        formatKeys, listKeys,
        keymap.of([...closeBracketsKeymap, ...searchKeymap, ...completionKeymap, ...foldKeymap, ...historyKeymap, indentWithTab, ...defaultKeymap]),
        onDoc,
      ],
    }),
  });
  view.focus();
  return {
    view,
    destroy() { flush(); view.destroy(); },
  };
}
