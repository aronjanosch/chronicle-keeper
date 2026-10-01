// Pinned + Recent sections at the top of the vault panel. Pins live per world
// in .ck/prefs.json (actions.togglePin/pinAt); pin from a row menu, the page
// toolbar, or by dragging any page row in here. Pinned order is draggable.
import { html, useState, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { useStore, recentPages, openInNewTab, activePagePath } from './core.js';
import { Icon, openContextMenu } from './ui.js';
import { togglePin, pinAt, loadPrefs } from './actions.js';
import { iconForKind, toneForKind } from './screens/codex.js';

const PAGE_DRAG = 'application/x-ck-page';
const RECENT_KEY = 'ck_recent_open';
const RECENT_SHOWN = 5;

export const startPageDrag = (path) => (e) => {
  e.dataTransfer.setData(PAGE_DRAG, path);
  e.dataTransfer.effectAllowed = 'copyMove';
};

function Glyph({ kind }) {
  const tone = toneForKind(kind);
  const col = tone === 'ink-blue' ? 'var(--ink-blue)' : `var(--${tone})`;
  return html`<div style=${{
    width: 16, height: 16, borderRadius: 4, flex: '0 0 auto',
    background: kind ? `var(--${tone}-50)` : 'var(--paper-deep)', color: kind ? col : 'var(--ink-muted)',
    display: 'flex', alignItems: 'center', justifyContent: 'center', border: '1px solid rgba(0,0,0,.06)',
  }}><${Icon} name=${iconForKind(kind)} size=${10} /></div>`;
}

function Label({ children, right, onClick }) {
  return html`<div class="label-xs" onClick=${onClick} style=${{ padding: '10px 12px 3px', display: 'flex', alignItems: 'center', cursor: onClick ? 'pointer' : 'default' }}>
    <span style=${{ flex: 1 }}>${children}</span>${right}
  </div>`;
}

function Row({ page, active, pinned, onOpen, onMenu, drag }) {
  return html`<div class="ck-pin-row ${drag?.cls || ''}" onClick=${onOpen} onContextMenu=${onMenu} title=${page.path}
    draggable=${true} onDragStart=${drag?.start || startPageDrag(page.path)} onDragEnd=${drag?.end}
    onDragOver=${drag?.over} onDragLeave=${drag?.leave} onDrop=${drag?.drop}
    style=${{
      display: 'flex', alignItems: 'center', gap: 7, padding: '4px 12px 4px 14px', cursor: 'pointer', borderRadius: 5,
      background: active ? 'var(--burgundy-50)' : 'transparent',
      color: active ? 'var(--burgundy-700)' : 'var(--ink-soft)',
      opacity: drag?.dragging ? 0.45 : 1,
    }}>
    <${Glyph} kind=${page.kind} />
    <span style=${{ flex: 1, minWidth: 0, fontSize: 12.5, fontWeight: active ? 500 : 400, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${page.title}</span>
    ${pinned && html`<span style=${{ fontSize: 10, color: 'var(--burgundy)' }} title="Pinned">●</span>`}
  </div>`;
}

export function PinnedRecent({ campaign, onOpen, act }) {
  const s = useStore();
  const cid = campaign?.campaign_id;
  const [recentOpen, setRecentOpen] = useState(() => { try { return localStorage.getItem(RECENT_KEY) !== '0'; } catch (_) { return true; } });
  const [dragPath, setDragPath] = useState(null);
  const [drop, setDrop] = useState(null); // { path, after } while hovering a pinned row
  const [over, setOver] = useState(false);
  useEffect(() => { loadPrefs(cid); }, [cid]);

  const byPath = new Map((s.vaultPages || []).map((p) => [p.path, p]));
  const pinnedPaths = s.worldPrefs?.campaignId === cid ? s.worldPrefs.pinned : [];
  const pinned = pinnedPaths.map((p) => byPath.get(p)).filter(Boolean);
  const pinnedSet = new Set(pinnedPaths);
  const recent = recentPages(cid).filter((p) => byPath.has(p) && !pinnedSet.has(p)).slice(0, RECENT_SHOWN).map((p) => byPath.get(p));
  const cur = activePagePath();
  const toggleRecent = () => setRecentOpen((o) => { try { localStorage.setItem(RECENT_KEY, o ? '0' : '1'); } catch (_) { /* private mode */ } return !o; });

  const menu = (page, isPinned) => (e) => openContextMenu(e, [
    { label: 'Open', icon: 'book', onClick: () => onOpen(page, e) },
    { label: 'Open in new tab', icon: 'plus', onClick: () => openInNewTab(page.path) },
    { label: isPinned ? 'Unpin from top' : 'Pin to top', icon: 'pin', onClick: () => togglePin(page.path) },
    { label: 'Rename…', icon: 'edit', onClick: () => act.renamePage(page) },
    { label: 'Move…', icon: 'arrow-r', onClick: () => act.movePage(page) },
  ]);

  const hasPageDrag = (e) => [...(e.dataTransfer?.types || [])].includes(PAGE_DRAG);
  const endDrag = () => { setDragPath(null); setDrop(null); setOver(false); };
  const dropAt = (index) => (e) => {
    e.preventDefault(); e.stopPropagation();
    const path = e.dataTransfer.getData(PAGE_DRAG);
    endDrag();
    if (path) pinAt(path, index);
  };
  const rowDrag = (page, i) => ({
    dragging: dragPath === page.path,
    cls: drop?.path === page.path ? (drop.after ? 'drop-after' : 'drop-before') : '',
    start: (e) => { startPageDrag(page.path)(e); setDragPath(page.path); },
    end: endDrag,
    over: (e) => {
      if (!hasPageDrag(e)) return;
      e.preventDefault(); e.stopPropagation();
      const r = e.currentTarget.getBoundingClientRect();
      const after = e.clientY > r.top + r.height / 2;
      if (drop?.path !== page.path || drop.after !== after) setDrop({ path: page.path, after });
    },
    leave: () => setDrop((d) => (d?.path === page.path ? null : d)),
    drop: (e) => {
      const after = drop?.path === page.path && drop.after;
      const from = e.dataTransfer.getData(PAGE_DRAG);
      const without = pinnedPaths.filter((p) => p !== from);
      const at = without.indexOf(page.path) + (after ? 1 : 0);
      dropAt(at)(e);
    },
  });

  if (!cid) return null;
  const showPinned = pinned.length > 0 || over;
  return html`<div onDragOver=${(e) => { if (hasPageDrag(e)) { e.preventDefault(); setOver(true); } }}
    onDragLeave=${(e) => { if (!e.currentTarget.contains(e.relatedTarget)) setOver(false); }}
    onDrop=${dropAt(null)}>
    <div
      style=${{ borderRadius: 5, boxShadow: over ? 'inset 0 0 0 1px var(--burgundy-300)' : 'none' }}>
      ${showPinned && html`<${Label}>Pinned</${Label}>`}
      ${pinned.map((p, i) => html`<${Row} key=${p.path} page=${p} pinned active=${p.path === cur}
        onOpen=${(e) => onOpen(p, e)} onMenu=${menu(p, true)} drag=${rowDrag(p, i)} />`)}
      ${over && pinned.length === 0 && html`<div style=${{ padding: '4px 14px 6px', fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>Drop to pin</div>`}
    </div>
    ${recent.length > 0 && html`<${Label} onClick=${toggleRecent}
      right=${html`<span style=${{ fontWeight: 400, textTransform: 'none', letterSpacing: 0 }}>${recentOpen ? '▾' : '▸'}</span>`}>Recent</${Label}>`}
    ${recentOpen && recent.map((p) => html`<${Row} key=${p.path} page=${p} active=${p.path === cur}
      onOpen=${(e) => onOpen(p, e)} onMenu=${menu(p, false)} />`)}
    <${Label}>Vault</${Label}>
  </div>`;
}
