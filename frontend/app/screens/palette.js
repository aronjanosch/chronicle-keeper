// ⌘K command palette + the app's only global hotkey dispatcher (Phase 7a).
// Pure frontend over shipped endpoints: fuzzy page jump (name + alias),
// full-text hits, tag jump, recent pages, and a handful of nav/create actions.
import { html, useState, useEffect, useRef } from '../../vendor/htm-preact-standalone.mjs';
import { store, navigate, openModal, closeModal, recentPages } from '../core.js';
import { Icon } from '../ui.js';
import { searchVault, loadVaultTags, createVaultPage } from '../actions.js';
import { runCommand, promptNewPage, promptNewFolder } from '../commands.js';

// ⌘<key> / ⌘⇧<key> → command id (14E). Symbol keys match on e.key regardless
// of shift so non-US layouts that type them shifted still work.
const MOD_KEYS = { f: 'find', k: 'palette', p: 'quick-open', n: 'new-page', s: 'save', w: 'tab-close' };
const MOD_SHIFT_KEYS = { f: 'search-world', j: 'quick-capture', k: 'toggle-rail', t: 'tab-reopen' };
const MOD_SYMBOLS = { '[': 'nav-back', ']': 'nav-forward', ',': 'settings', '/': 'shortcuts' };
// ⌘⇧[ / ⌘⇧] cycle tabs (15D); both raw and shifted forms so layouts that
// report '{'/'}' still match.
const MOD_SHIFT_SYMBOLS = { '[': 'tab-prev', '{': 'tab-prev', ']': 'tab-next', '}': 'tab-next' };

// The single global keydown dispatcher. Only ⌘-chords are reserved —
// everything else falls through to the focused element so editors keep their
// own keymaps. In the Tauri shell most of these also exist as native menu
// accelerators; runCommand dedupes the double fire. Mount once at the app root.
export function useGlobalHotkeys() {
  useEffect(() => {
    const onKey = (e) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
      const id = (e.shiftKey && MOD_SHIFT_SYMBOLS[e.key])
        || MOD_SYMBOLS[e.key]
        || (e.shiftKey ? MOD_SHIFT_KEYS[e.key.toLowerCase()] : MOD_KEYS[e.key.toLowerCase()])
        || (!e.shiftKey && /^[1-9]$/.test(e.key) ? `tab-${e.key}` : null);
      if (!id) return;
      e.preventDefault();
      runCommand(id);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}

// Client-side fuzzy: prefix > substring > subsequence, shorter strings win ties.
// `q` is already lowercased by the caller. Returns -1 for no match.
function fuzzyScore(q, text) {
  if (!q) return 0;
  const t = (text || '').toLowerCase();
  const idx = t.indexOf(q);
  if (idx === 0) return 1000 - t.length;
  if (idx > 0) return 600 - idx - t.length * 0.1;
  let ti = 0, gaps = 0, start = -1;
  for (let qi = 0; qi < q.length; qi++) {
    const f = t.indexOf(q[qi], ti);
    if (f === -1) return -1;
    if (start < 0) start = f;
    else if (f !== ti) gaps++;
    ti = f + 1;
  }
  return 200 - gaps * 8 - start;
}

function pageScore(q, p) {
  const names = [p.title, ...(p.aliases || [])];
  return Math.max(...names.map((n) => fuzzyScore(q, n)));
}

const KIND_ICON = { npc: 'users', pc: 'users', place: 'compass', faction: 'flag', item: 'sword', lore: 'doc' };

function Row({ item, active, onHover, onRun }) {
  return html`<div onMouseMove=${onHover} onClick=${onRun}
    style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 14px', cursor: 'pointer',
      background: active ? 'var(--surface)' : 'transparent',
      borderLeft: `2px solid ${active ? 'var(--burgundy)' : 'transparent'}` }}>
    <${Icon} name=${item.icon || 'doc'} size=${13} className=${active ? '' : 'ck-ink-faint'} />
    <span style=${{ flex: 1, minWidth: 0 }}>
      <span style=${{ fontSize: 13, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', display: 'block' }}>${item.label}</span>
      ${item.sub && html`<span class="ck-ink-faint" style=${{ fontSize: 11, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', display: 'block' }}
        dangerouslySetInnerHTML=${item.subHtml ? { __html: item.sub } : undefined}>${item.subHtml ? undefined : item.sub}</span>`}
    </span>
    ${item.hint && html`<span style=${{ fontSize: 10.5, fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)', flex: '0 0 auto' }}>${item.hint}</span>`}
  </div>`;
}

function CommandPalette() {
  const [q, setQ] = useState('');
  const [fts, setFts] = useState([]);
  const [sel, setSel] = useState(0);
  const inputRef = useRef(null);
  const ftsTimer = useRef(null);
  const campaign = store.campaign;
  const cid = campaign?.campaign_id;
  const query = q.trim().toLowerCase();

  useEffect(() => { inputRef.current?.focus(); if (cid) loadVaultTags(cid); }, []);

  // Full-text body hits, debounced; layered under instant name matches.
  useEffect(() => {
    if (ftsTimer.current) clearTimeout(ftsTimer.current);
    if (!cid || query.length < 2) { setFts([]); return; }
    ftsTimer.current = setTimeout(() => {
      searchVault(query).then((h) => setFts(h || [])).catch(() => setFts([]));
    }, 220);
    return () => { if (ftsTimer.current) clearTimeout(ftsTimer.current); };
  }, [query, cid]);

  const pages = store.vaultPages || [];
  const tags = store.vaultTags || [];

  function newEvent() {
    openModal('textPrompt', {
      title: 'New event page', label: 'Event title', confirmLabel: 'Create',
      onSubmit: async (title) => { const p = await createVaultPage(title, 'event', 'Events'); navigate('page', { path: p.path }); },
    });
  }
  const go = (name, params) => () => { closeModal(); navigate(name, params); };

  // Build the flat, grouped item list for the current query.
  const groups = [];
  if (cid) {
    if (query) {
      const matched = pages
        .map((p) => ({ p, s: pageScore(query, p) }))
        .filter((x) => x.s >= 0)
        .sort((a, b) => b.s - a.s).slice(0, 8)
        .map(({ p }) => ({ icon: KIND_ICON[p.kind] || 'doc', label: p.title, sub: p.summary || p.path, run: go('page', { path: p.path }) }));
      if (matched.length) groups.push({ head: 'Pages', items: matched });

      const named = new Set(matched.map((m) => m.label));
      const body = (fts || []).filter((h) => !named.has(h.title)).slice(0, 6)
        .map((h) => ({ icon: 'search', label: h.title, sub: h.snippet, subHtml: true, run: go('page', { path: h.path }) }));
      if (body.length) groups.push({ head: 'In page text', items: body });

      const tagHits = tags.filter((t) => t.tag.toLowerCase().includes(query)).slice(0, 6)
        .map((t) => ({ icon: 'tag', label: `#${t.tag}`, hint: String(t.count), run: go('codex', { id: cid, tag: t.tag }) }));
      if (tagHits.length) groups.push({ head: 'Tags', items: tagHits });
    } else {
      const byPath = new Map(pages.map((p) => [p.path, p]));
      const recent = recentPages(cid).map((path) => byPath.get(path)).filter(Boolean).slice(0, 6)
        .map((p) => ({ icon: KIND_ICON[p.kind] || 'doc', label: p.title, sub: p.summary || p.path, run: go('page', { path: p.path }) }));
      if (recent.length) groups.push({ head: 'Recent pages', items: recent });
    }
  }

  const actionDefs = cid ? [
    { icon: 'plus', label: 'New page', run: () => { closeModal(); promptNewPage(); } },
    { icon: 'cal', label: 'New event page', run: () => { closeModal(); newEvent(); } },
    { icon: 'folder', label: 'New folder', run: () => { closeModal(); promptNewFolder(); } },
    { icon: 'search', label: query ? `Search the world for “${q.trim()}”` : 'Search the world', run: () => { closeModal(); navigate('search', { id: cid, q: q.trim() }); } },
    { icon: 'book', label: 'Go to Codex', run: go('codex', { id: cid }) },
    { icon: 'map', label: 'Go to Atlas', run: go('atlas', { id: cid }) },
    { icon: 'time', label: 'Go to Timeline', run: go('timeline', { id: cid }) },
    { icon: 'link', label: 'Go to Graph', run: go('graph', { id: cid }) },
    { icon: 'feather', label: 'Quick capture', run: () => { closeModal(); openModal('quickCapture'); } },
    { icon: 'feather', label: 'Go to the Keeper', run: go('keeper', { id: cid }) },
    { icon: 'mic', label: 'Go to Sessions', run: go('sessions', { id: cid }) },
    { icon: 'compass', label: 'World overview', run: go('campaign', { id: cid }) },
    { icon: 'globe', label: 'All worlds', run: go('library') },
    { icon: 'cog', label: 'Settings', run: go('settings') },
  ] : [
    { icon: 'globe', label: 'All worlds', run: go('library') },
    { icon: 'cog', label: 'Settings', run: go('settings') },
  ];
  const actions = (query ? actionDefs.filter((a) => a.label.toLowerCase().includes(query)) : actionDefs);
  if (actions.length) groups.push({ head: 'Actions', items: actions });

  const flat = groups.flatMap((g) => g.items);
  // Keep selection in range as the list changes under the query.
  useEffect(() => { setSel(0); }, [query, fts]);
  const cur = Math.min(sel, Math.max(0, flat.length - 1));

  function onKeyDown(e) {
    if (e.key === 'ArrowDown') { e.preventDefault(); setSel((s) => Math.min(flat.length - 1, s + 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setSel((s) => Math.max(0, s - 1)); }
    else if (e.key === 'Enter') { e.preventDefault(); flat[cur]?.run(); }
    else if (e.key === 'Escape') { e.preventDefault(); closeModal(); }
    else if (e.key === 'Tab') {
      // Cycle to the first item of the next group.
      e.preventDefault();
      const starts = [];
      let n = 0;
      for (const g of groups) { starts.push(n); n += g.items.length; }
      const next = starts.find((i) => i > cur);
      setSel(next != null ? next : 0);
    }
  }

  let i = -1;
  return html`<div onClick=${(e) => { if (e.target === e.currentTarget) closeModal(); }}
    style=${{ position: 'fixed', inset: 0, background: 'rgba(28,22,12,.28)', display: 'flex', justifyContent: 'center', alignItems: 'flex-start', paddingTop: '12vh', zIndex: 200 }}>
    <div class="ck" style=${{ width: 600, maxWidth: '92vw', maxHeight: '70vh', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 12, boxShadow: 'var(--shadow-raised)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      <div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 16px', borderBottom: '1px solid var(--rule-soft)' }}>
        <${Icon} name="search" size=${15} className="ck-ink-faint" />
        <input ref=${inputRef} value=${q} onInput=${(e) => setQ(e.target.value)} onKeyDown=${onKeyDown}
          placeholder=${cid ? 'Jump to a page, tag, or action…' : 'Jump to…'}
          style=${{ flex: 1, border: 'none', outline: 'none', background: 'transparent', fontSize: 15, color: 'var(--ink)', fontFamily: 'inherit' }} />
        <span style=${{ fontSize: 10.5, fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)' }}>esc</span>
      </div>
      <div style=${{ overflow: 'auto', padding: '6px 0' }}>
        ${flat.length === 0 && html`<div style=${{ padding: '20px 16px', fontSize: 13, color: 'var(--ink-faint)', fontStyle: 'italic' }}>No matches.</div>`}
        ${groups.map((g) => html`<div key=${g.head}>
          <div style=${{ padding: '8px 16px 3px', fontSize: 10, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>${g.head}</div>
          ${g.items.map((item) => { i++; const idx = i; return html`<${Row} key=${idx} item=${item} active=${idx === cur}
            onHover=${() => setSel(idx)} onRun=${item.run} />`; })}
        </div>`)}
      </div>
    </div>
  </div>`;
}

export { CommandPalette };
