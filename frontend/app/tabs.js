// Phase 15A: the tab strip on the Codex/page screens. Tabs are store.tabs
// (paths, per world); the active tab is the path the 'page' route shows.
// Titles and kind icons resolve live from the vault index, so renames and
// kind changes show up without any tab bookkeeping.
import { html, useState } from '../vendor/htm-preact-standalone.mjs';
import { useStore, navigate, activePagePath, closeTab, closeOtherTabs, reopenClosedTab, moveTab, openInNewTab } from './core.js';
import { Icon, openContextMenu } from './ui.js';

const KIND_ICONS = { pc: 'sparkle', npc: 'users', place: 'map', faction: 'shield', item: 'gem', event: 'cal', lore: 'scroll' };

function titleOf(path) {
  const i = path.lastIndexOf('/');
  return path.slice(i + 1).replace(/\.md$/i, '');
}

function Tab({ path, active, page, onDrag }) {
  const title = page?.title || titleOf(path);
  const menu = (e) => openContextMenu(e, [
    { label: 'Close tab', icon: 'x', onClick: () => closeTab(path) },
    { label: 'Close other tabs', icon: 'x', onClick: () => closeOtherTabs(path) },
    { label: 'Reopen closed tab', icon: 'time', onClick: reopenClosedTab },
  ]);
  return html`<div title=${path}
    onClick=${() => { if (!active) navigate('page', { path }); }}
    onAuxClick=${(e) => { if (e.button === 1) { e.preventDefault(); closeTab(path); } }}
    onMouseDown=${(e) => { if (e.button === 1) e.preventDefault(); }}
    onContextMenu=${menu}
    draggable=${true}
    onDragStart=${onDrag.start} onDragOver=${onDrag.over} onDrop=${onDrag.drop} onDragEnd=${onDrag.end}
    style=${{
      display: 'flex', alignItems: 'center', gap: 6, padding: '0 6px 0 10px', height: 30,
      maxWidth: 180, minWidth: 0, flex: '0 1 auto', cursor: 'pointer', userSelect: 'none',
      borderRight: '1px solid var(--rule-soft)', borderTop: `2px solid ${active ? 'var(--burgundy)' : 'transparent'}`,
      background: active ? 'var(--paper)' : 'transparent',
      color: active ? 'var(--ink)' : 'var(--ink-muted)',
      opacity: onDrag.dragging ? 0.45 : 1,
      boxShadow: onDrag.dropOver ? 'inset 2px 0 0 var(--burgundy-300)' : 'none',
    }}
    onMouseEnter=${(e) => { if (!active) e.currentTarget.style.background = 'rgba(120,90,40,.07)'; const x = e.currentTarget.querySelector('.ck-tab-x'); if (x) x.style.opacity = 1; }}
    onMouseLeave=${(e) => { if (!active) e.currentTarget.style.background = 'transparent'; const x = e.currentTarget.querySelector('.ck-tab-x'); if (x && !active) x.style.opacity = 0; }}>
    <${Icon} name=${KIND_ICONS[page?.kind] || 'doc'} size=${12} className=${active ? 'ck-burgundy' : 'ck-ink-faint'} />
    <span style=${{ flex: 1, minWidth: 0, fontSize: 12.5, fontWeight: active ? 500 : 400, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${title}</span>
    <span class="ck-tab-x" title="Close tab"
      onClick=${(e) => { e.stopPropagation(); closeTab(path); }}
      style=${{ display: 'flex', padding: 2, borderRadius: 3, color: 'var(--ink-faint)', opacity: active ? 1 : 0, transition: 'opacity .12s', flex: '0 0 auto' }}
      onMouseEnter=${(e) => { e.currentTarget.style.color = 'var(--burgundy)'; e.currentTarget.style.background = 'var(--burgundy-50)'; }}
      onMouseLeave=${(e) => { e.currentTarget.style.color = 'var(--ink-faint)'; e.currentTarget.style.background = 'transparent'; }}>
      <${Icon} name="x" size=${11} />
    </span>
  </div>`;
}

export function TabStrip() {
  const s = useStore();
  const tabs = s.tabs || [];
  const [drag, setDrag] = useState(null);  // dragged index
  const [over, setOver] = useState(null);  // hovered index
  if (!tabs.length) return null;
  const active = activePagePath();
  const byPath = new Map((s.vaultPages || []).map((p) => [p.path, p]));
  const dragFor = (i) => ({
    dragging: drag === i,
    dropOver: over === i && drag !== null && drag !== i,
    start: (e) => { setDrag(i); e.dataTransfer.effectAllowed = 'move'; },
    over: (e) => { if (drag === null) return; e.preventDefault(); e.dataTransfer.dropEffect = 'move'; if (over !== i) setOver(i); },
    drop: (e) => { e.preventDefault(); if (drag !== null) moveTab(drag, i); setDrag(null); setOver(null); },
    end: () => { setDrag(null); setOver(null); },
  });
  return html`<div style=${{
    display: 'flex', alignItems: 'stretch', flex: '0 0 auto', height: 30, overflowX: 'auto', overflowY: 'hidden',
    background: 'var(--paper-deep)', borderBottom: '1px solid var(--rule-soft)', scrollbarWidth: 'none',
  }}>
    ${tabs.map((path, i) => html`<${Tab} key=${path} path=${path} active=${path === active} page=${byPath.get(path)} onDrag=${dragFor(i)} />`)}
  </div>`;
}

// ⌘-click (or middle-click where the caller forwards aux events) opens a page
// in a background tab instead of navigating — shared by every "open page"
// click site (tree rows, cards, search hits, rail links, wikilinks).
export function openPageEvt(path, e) {
  if (e && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    e.stopPropagation();
    openInNewTab(path, { background: true });
  } else {
    navigate('page', { path });
  }
}
