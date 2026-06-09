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
        autocompletion({ override: [wikilinkSource(cm, opts.getPages, opts.onCreatePage), tagSource(opts.getPages)], icons: false }),
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
