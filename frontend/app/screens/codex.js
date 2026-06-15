// Codex — the world wiki. Vault Explorer: folder tree + page cards, files-as-truth.
import { html, useState, useEffect, useRef } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, openModal, useStore, setOp, store, setState, openInNewTab } from '../core.js';
import { TabStrip, openPageEvt } from '../tabs.js';
import { Shell, Sidebar, Topbar, useSidebarWidth, ResizeHandle, WORLD_NAV, navToWorldDest } from '../shell.js';
import { Btn, Empty, Icon, Markdown, Input, Select, BrandMark, openContextMenu } from '../ui.js';
import { openCampaign,
  loadVaultTree, createVaultFolder, moveVaultEntry,
  deleteVaultPage, deleteVaultFolder, duplicateVaultPage, copyText, attachVault, pickVaultFolder,
  searchVault, loadVaultTags, loadVaultDiagnostics, sniffVault, importVaultFolder, enhanceVaultPages, watchVault,
  bulkVault, saveTemplate } from '../actions.js';
import { kindForFolder } from '../folderKinds.js';

export const KINDS = [
  { value: 'pc',      label: 'PC',      plural: 'PCs',      tone: 'gilt' },
  { value: 'npc',     label: 'NPC',     plural: 'NPCs',     tone: 'burgundy' },
  { value: 'place',   label: 'Place',   plural: 'Places',   tone: 'moss' },
  { value: 'faction', label: 'Faction', plural: 'Factions', tone: 'ink-blue' },
  { value: 'item',    label: 'Item',    plural: 'Items',    tone: 'ochre' },
  { value: 'event',   label: 'Event',   plural: 'Events',   tone: 'ochre' },
  { value: 'lore',    label: 'Lore',    plural: 'Lore',     tone: 'gilt' },
];

export function iconForKind(k) {
  return { pc: 'sparkle', npc: 'users', place: 'map', faction: 'shield', item: 'gem', event: 'cal', lore: 'scroll' }[k] || 'doc';
}
export function toneForKind(k) {
  return (KINDS.find((x) => x.value === k) || {}).tone || 'burgundy';
}

async function attachVaultFlow() {
  let path = await pickVaultFolder();
  if (!path) {
    path = window.prompt('Absolute path to the vault folder for this world:');
    if (!path) return;
  }
  try { await attachVault(path.trim()); }
  catch (e) { window.alert(`Could not attach vault: ${e.message}`); }
}

// Copy-in import: pick a folder of .md notes (e.g. an Obsidian vault), preview
// the page count, copy into the Codex. The source folder is never touched.
async function importNotesFlow() {
  let path = await pickVaultFolder();
  if (!path) {
    path = window.prompt('Folder to import (absolute path):');
    if (!path) return;
  }
  path = path.trim();
  const s = await sniffVault(path);
  if (s && s.md_pages === 0) { window.alert('No markdown pages found in that folder.'); return; }
  const pageCount = s ? `${s.md_pages} page${s.md_pages === 1 ? '' : 's'}` : 'all pages';
  const count = s?.assets ? `${pageCount} and ${s.assets} media file${s.assets === 1 ? '' : 's'}` : pageCount;
  openModal('confirm', {
    title: 'Import notes',
    message: html`Copy ${count} from ${html`<strong>${path}</strong>`} into this world's Codex?
      Folder structure is kept; name collisions get a suffix. The source folder is not changed.`,
    confirmLabel: 'Import',
    onConfirm: async () => {
      const r = await importVaultFolder(path);
      const media = r.assets ? ` and ${r.assets} media file${r.assets === 1 ? '' : 's'}` : '';
      window.alert(`Imported ${r.imported} page${r.imported === 1 ? '' : 's'}${media}${r.renamed ? ` (${r.renamed} renamed — name already taken)` : ''}.`);
    },
  });
}

// Standalone AI enhancement run: pick folders to enhance, then batch-enhance.
function enhanceFlow() {
  openModal('enhanceFolder');
}

// New template: name → a broad blank skeleton in _templates/, then open it.
// `{{title}}` becomes the page title on create; the user sets `kind:` + fields.
function newTemplateFlow() {
  openModal('textPrompt', {
    title: 'New template', label: 'Template name', placeholder: 'villain',
    confirmLabel: 'Create template',
    onSubmit: async (raw) => {
      const name = (raw || '').trim().replace(/[\/\\.]/g, '');
      if (!name) return;
      await saveTemplate(name, '---\nkind: lore\nsummary:\n---\n\n# {{title}}\n\n');
      navigate('template', { name });
    },
  });
}

export const dirOf = (p) => { const i = p.lastIndexOf('/'); return i < 0 ? '' : p.slice(0, i); };
export const baseName = (p) => { const i = p.lastIndexOf('/'); return i < 0 ? p : p.slice(i + 1); };

function agoLabel(secs) {
  if (!secs) return '';
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - secs);
  if (diff < 90) return 'just now';
  if (diff < 3600) return `${Math.round(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)}h ago`;
  return `${Math.round(diff / 86400)}d ago`;
}

// Nest the flat folder + page lists into a tree of { name, path, folders:Map, pages:[] }.
export function buildTree(folders, pages) {
  const root = { name: '', path: '', folders: new Map(), pages: [] };
  const ensure = (relDir) => {
    if (!relDir) return root;
    let node = root, acc = '';
    for (const part of relDir.split('/')) {
      acc = acc ? `${acc}/${part}` : part;
      if (!node.folders.has(part)) node.folders.set(part, { name: part, path: acc, folders: new Map(), pages: [] });
      node = node.folders.get(part);
    }
    return node;
  };
  (folders || []).forEach(ensure);
  (pages || []).forEach((p) => ensure(dirOf(p.path)).pages.push(p));
  return root;
}
function nodeAt(root, path) {
  if (!path) return root;
  let node = root;
  for (const part of path.split('/')) {
    node = node?.folders.get(part);
    if (!node) return null;
  }
  return node;
}
function countPages(node) {
  let n = node.pages.length;
  for (const c of node.folders.values()) n += countPages(c);
  return n;
}

function glyphFor(kind, size = 16) {
  const tone = toneForKind(kind);
  const col = tone === 'ink-blue' ? 'var(--ink-blue)' : `var(--${tone})`;
  return html`<div style=${{
    width: size, height: size, borderRadius: 4, flex: '0 0 auto',
    background: kind ? `var(--${tone}-50)` : 'var(--paper-deep)', color: kind ? col : 'var(--ink-muted)',
    display: 'flex', alignItems: 'center', justifyContent: 'center', border: '1px solid rgba(0,0,0,.06)',
  }}><${Icon} name=${iconForKind(kind)} size=${Math.round(size * 0.6)} /></div>`;
}

// One row in the tree (page leaf or folder), with hover action icons.
function HoverActions({ children }) {
  return html`<span class="ck-row-actions" style=${{ display: 'flex', gap: 2, opacity: 0, transition: 'opacity .12s' }}>${children}</span>`;
}
function ActionIcon({ icon, title, onClick }) {
  return html`<span title=${title} onClick=${(e) => { e.stopPropagation(); onClick(); }}
    style=${{ padding: 2, borderRadius: 3, color: 'var(--ink-faint)', cursor: 'pointer', display: 'flex' }}
    onMouseEnter=${(e) => { e.currentTarget.style.color = 'var(--burgundy)'; }}
    onMouseLeave=${(e) => { e.currentTarget.style.color = 'var(--ink-faint)'; }}><${Icon} name=${icon} size=${12} /></span>`;
}
function rowHover(on) {
  return (e) => { const a = e.currentTarget.querySelector('.ck-row-actions'); if (a) a.style.opacity = on ? 1 : 0; };
}

// In-place rename field (file-browser style): auto-focus + select-all,
// Enter/blur commits, Escape cancels.
function RenameInput({ initial, onCommit, onCancel }) {
  const ref = useRef(null);
  const done = useRef(false);
  useEffect(() => { const el = ref.current; if (el) { el.value = initial; el.focus(); el.select(); } }, []);
  const finish = (commit) => {
    if (done.current) return;
    done.current = true;
    const v = (ref.current?.value || '').replace(/[/\\]/g, '').trim();
    if (commit && v && v !== initial) onCommit(v); else onCancel();
  };
  return html`<input ref=${ref}
    onClick=${(e) => e.stopPropagation()}
    onKeyDown=${(e) => { e.stopPropagation(); if (e.key === 'Enter') finish(true); if (e.key === 'Escape') finish(false); }}
    onBlur=${() => finish(true)}
    style=${{ flex: 1, minWidth: 0, font: 'inherit', fontSize: 12.5, padding: '1px 4px',
      border: '1px solid var(--burgundy)', borderRadius: 3, background: 'var(--surface)',
      color: 'var(--ink)', outline: 'none' }} />`;
}

function pageMenu(page, { onOpen, act, ren }) {
  return (e) => openContextMenu(e, [
    { label: 'Open', icon: 'book', onClick: onOpen },
    { label: 'Open in new tab', icon: 'plus', onClick: () => openInNewTab(page.path) },
    '-',
    { label: 'Rename', icon: 'edit', onClick: () => (ren ? ren.start(page.path) : act.renamePage(page)) },
    { label: 'Move to folder…', icon: 'arrow-r', onClick: () => act.movePage(page) },
    { label: 'Promote to kind…', icon: 'sparkle', onClick: () => act.promotePage(page) },
    { label: 'Duplicate', icon: 'copy', onClick: () => act.duplicatePage(page) },
    '-',
    { label: 'Copy [[wikilink]]', icon: 'link', onClick: () => copyText(`[[${page.title}]]`, 'Wikilink copied') },
    { label: 'Copy path', icon: 'doc', onClick: () => copyText(page.path, 'Path copied') },
    '-',
    { label: 'New page in folder', icon: 'plus', onClick: () => act.newPage(dirOf(page.path)) },
    { label: 'Ask Keeper about this', icon: 'feather', onClick: () => act.askKeeper(page) },
    '-',
    { label: 'Move to trash', icon: 'trash', danger: true, onClick: () => act.deletePage(page) },
  ]);
}

function folderMenu(node, { act, ren }) {
  return (e) => openContextMenu(e, [
    { label: 'New page here', icon: 'plus', onClick: () => act.newPage(node.path) },
    { label: 'New subfolder', icon: 'folder', onClick: () => act.newFolder(node.path) },
    '-',
    { label: 'Rename', icon: 'edit', onClick: () => (ren ? ren.start(node.path) : act.renameFolder(node)) },
    '-',
    { label: 'Move to trash', icon: 'trash', danger: true, onClick: () => act.deleteFolder(node) },
  ]);
}

function PageLeaf({ page, depth, active, onOpen, act, ren, dnd }) {
  const isActive = page.path === active;
  const renaming = ren && ren.path === page.path;
  const dragging = dnd && dnd.draggingPath === page.path;
  return html`<div onClick=${renaming ? null : onOpen} onMouseEnter=${rowHover(true)} onMouseLeave=${rowHover(false)}
    onContextMenu=${renaming ? null : pageMenu(page, { onOpen, act, ren })}
    draggable=${dnd && !renaming ? true : undefined}
    onDragStart=${dnd ? dnd.startPage(page) : undefined}
    onDragEnd=${dnd ? dnd.end : undefined}
    style=${{
      display: 'flex', alignItems: 'center', gap: 7, padding: '4px 8px', paddingLeft: 27 + depth * 14, borderRadius: 5, cursor: 'pointer',
      background: isActive ? 'var(--burgundy-50)' : 'transparent',
      boxShadow: isActive ? 'inset 2px 0 0 var(--burgundy)' : 'none',
      color: isActive ? 'var(--burgundy-700)' : 'var(--ink-soft)',
      opacity: dragging ? 0.45 : 1,
    }}>
    ${glyphFor(page.kind, 16)}
    ${renaming
      ? html`<${RenameInput} initial=${page.title} onCommit=${(v) => ren.commit(page.path, false, v)} onCancel=${ren.cancel} />`
      : html`<span style=${{ flex: 1, fontSize: 12.5, fontWeight: isActive ? 500 : 400, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${page.title}</span>
    <${HoverActions}>
      <${ActionIcon} icon="edit" title="Rename" onClick=${() => (ren ? ren.start(page.path) : act.renamePage(page))} />
      <${ActionIcon} icon="arrow-r" title="Move to folder" onClick=${() => act.movePage(page)} />
      <${ActionIcon} icon="trash" title="Delete" onClick=${() => act.deletePage(page)} />
    </${HoverActions}>`}
  </div>`;
}

function FolderNode({ node, depth, openSet, toggle, active, onOpen, act, ren, dnd }) {
  const open = openSet.has(node.path);
  const kind = kindForFolder(node.name);
  const renaming = ren && ren.path === node.path;
  const children = [...node.folders.values()].sort((a, b) => a.name.localeCompare(b.name));
  const dragging = dnd && dnd.draggingPath === node.path;
  const dropOver = dnd && dnd.over === node.path;
  return html`<div>
    <div onClick=${renaming ? null : () => toggle(node.path)} onMouseEnter=${rowHover(true)} onMouseLeave=${rowHover(false)}
      onContextMenu=${renaming ? null : folderMenu(node, { act, ren })}
      draggable=${dnd && !renaming ? true : undefined}
      onDragStart=${dnd ? dnd.startFolder(node) : undefined}
      onDragEnd=${dnd ? dnd.end : undefined}
      onDragOver=${dnd ? dnd.overFolder(node.path) : undefined}
      onDragLeave=${dnd ? dnd.leave(node.path) : undefined}
      onDrop=${dnd ? dnd.dropFolder(node.path) : undefined}
      style=${{ display: 'flex', alignItems: 'center', gap: 6, padding: '4px 8px', paddingLeft: 10 + depth * 14, borderRadius: 5, cursor: 'pointer',
        color: 'var(--ink-soft)', opacity: dragging ? 0.45 : 1,
        background: dropOver ? 'var(--burgundy-50)' : 'transparent',
        boxShadow: dropOver ? 'inset 0 0 0 1px var(--burgundy-300)' : 'none' }}>
      <${Icon} name=${open ? 'chev-d' : 'chev-r'} size=${11} className="ck-ink-faint" />
      ${kind ? glyphFor(kind, 16) : html`<${Icon} name="folder" size=${13} className="ck-burgundy" />`}
      ${renaming
        ? html`<${RenameInput} initial=${node.name} onCommit=${(v) => ren.commit(node.path, true, v)} onCancel=${ren.cancel} />`
        : html`<span style=${{ flex: 1, fontSize: 12.5, fontWeight: 500, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${node.name}</span>
      <${HoverActions}>
        <${ActionIcon} icon="plus" title="New page here" onClick=${() => act.newPage(node.path)} />
        <${ActionIcon} icon="edit" title="Rename folder" onClick=${() => (ren ? ren.start(node.path) : act.renameFolder(node))} />
        <${ActionIcon} icon="trash" title="Move folder to trash" onClick=${() => act.deleteFolder(node)} />
      </${HoverActions}>
      <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 10.5, color: 'var(--ink-faint)' }}>${countPages(node) || ''}</span>`}
    </div>
    ${open && html`<div>
      ${children.map((c) => html`<${FolderNode} key=${c.path} node=${c} depth=${depth + 1} openSet=${openSet} toggle=${toggle} active=${active} onOpen=${onOpen} act=${act} ren=${ren} dnd=${dnd} />`)}
      ${node.pages.map((p) => html`<${PageLeaf} key=${p.path} page=${p} depth=${depth + 1} active=${active} onOpen=${(e) => onOpen(p, e)} act=${act} ren=${ren} dnd=${dnd} />`)}
    </div>`}
  </div>`;
}

// Saved searches: a per-world list of { label, query } in localStorage.
function savedSearchKey(campaignId) { return `ck_saved_searches_${campaignId}`; }
function loadSavedSearches(campaignId) {
  try { return JSON.parse(localStorage.getItem(savedSearchKey(campaignId)) || '[]'); } catch (_) { return []; }
}
function storeSavedSearches(campaignId, list) {
  try { localStorage.setItem(savedSearchKey(campaignId), JSON.stringify(list)); } catch (_) { /* private mode */ }
}

// Stub-suggestion bar: dismissable per world. We store the suggestion count at
// dismiss time so the bar re-surfaces only when new links/orphans appear (no
// red-link nagging — akhier's "no red links" rule).
function diagDismissKey(campaignId) { return `ck_diag_dismissed_${campaignId}`; }
function loadDiagDismissed(campaignId) {
  try { return Number(localStorage.getItem(diagDismissKey(campaignId))) || 0; } catch (_) { return 0; }
}
function storeDiagDismissed(campaignId, n) {
  try { localStorage.setItem(diagDismissKey(campaignId), String(n)); } catch (_) { /* private mode */ }
}

// The vault file browser — search, tree, diagnostics, vault-path footer. Filling
// the rest of the FileTree aside below the brand + world nav.
function VaultPanel({ campaign, tree, active, onOpen, act }) {
  const store = useStore();
  const [q, setQ] = useState('');
  const [openSet, setOpenSet] = useState(() => new Set());
  const [ftsHits, setFtsHits] = useState([]);
  const [saved, setSaved] = useState(() => loadSavedSearches(campaign?.campaign_id));
  const [renPath, setRenPath] = useState(null);
  const [tplOpen, setTplOpen] = useState(false);
  const [diagDismissed, setDiagDismissed] = useState(() => loadDiagDismissed(campaign?.campaign_id));
  const ftsTimer = useRef(null);
  // Inline rename (file-browser style) replaces the modal inside the tree.
  const ren = {
    path: renPath,
    start: setRenPath,
    cancel: () => setRenPath(null),
    commit: async (path, isFolder, name) => {
      setRenPath(null);
      const dest = dirOf(path) ? `${dirOf(path)}/${name}` : name;
      try { await moveVaultEntry(path, isFolder ? dest : `${dest}.md`); }
      catch (e) { window.alert(`Rename failed: ${e.message}`); }
    },
  };
  const toggle = (path) => setOpenSet((s) => { const n = new Set(s); n.has(path) ? n.delete(path) : n.add(path); return n; });

  // Drag-to-move: drop a page/folder onto a folder (or the root) → move_entry,
  // with the link-rewrite cascade the backend already runs on rename. The
  // kebab "Move…" modal stays as the a11y fallback.
  const [drag, setDrag] = useState(null);   // { path, isFolder, name }
  const [over, setOver] = useState(null);   // folder path being hovered ('' = root)
  async function doMove(folder) {
    const item = drag; setDrag(null); setOver(null);
    if (!item) return;
    if (item.isFolder && (folder === item.path || folder.startsWith(item.path + '/'))) return; // into self/descendant
    if (dirOf(item.path) === folder) return; // already there
    const dest = folder ? `${folder}/${item.name}` : item.name;
    try { await moveVaultEntry(item.path, dest); }
    catch (e) { window.alert(`Move failed: ${e.message}`); }
  }
  const dnd = {
    draggingPath: drag?.path, over,
    startPage: (page) => (e) => { setDrag({ path: page.path, isFolder: false, name: baseName(page.path) }); e.dataTransfer.effectAllowed = 'move'; },
    startFolder: (node) => (e) => { e.stopPropagation(); setDrag({ path: node.path, isFolder: true, name: baseName(node.path) }); e.dataTransfer.effectAllowed = 'move'; },
    overFolder: (path) => (e) => { if (!drag) return; e.preventDefault(); e.stopPropagation(); e.dataTransfer.dropEffect = 'move'; if (over !== path) setOver(path); },
    leave: (path) => () => { if (over === path) setOver(null); },
    dropFolder: (path) => (e) => { e.preventDefault(); e.stopPropagation(); doMove(path); },
    end: () => { setDrag(null); setOver(null); },
  };

  const query = q.trim().toLowerCase();
  const allPages = [];
  (function walk(node) { node.pages.forEach((p) => allPages.push(p)); node.folders.forEach(walk); })(tree);
  const matches = query
    ? allPages.filter((p) => p.title.toLowerCase().includes(query) || p.path.toLowerCase().includes(query)
        || (p.aliases || []).some((a) => a.includes(query)))
    : null;
  const rootFolders = [...tree.folders.values()].sort((a, b) => a.name.localeCompare(b.name));

  // Full-text hits (body matches) layered under the instant name matches.
  useEffect(() => {
    if (ftsTimer.current) clearTimeout(ftsTimer.current);
    if (query.length < 2) { setFtsHits([]); return; }
    ftsTimer.current = setTimeout(() => {
      searchVault(query).then((hits) => setFtsHits(hits || [])).catch(() => setFtsHits([]));
    }, 250);
    return () => { if (ftsTimer.current) clearTimeout(ftsTimer.current); };
  }, [query]);
  const nameMatched = new Set((matches || []).map((p) => p.path));
  const bodyHits = (ftsHits || []).filter((h) => !nameMatched.has(h.path));
  const links = store.vaultLinks || null;
  const vd = store.vaultDiag || null;
  // Full diagnostics when loaded; the cheap links payload as fallback counts.
  const diag = vd
    ? {
        unresolved: vd.broken_links.length + vd.broken_media.length,
        orphans: vd.orphans.length,
        issues: vd.conflicts.length + vd.scan_errors.length,
      }
    : links && { unresolved: links.unresolved, orphans: links.orphans, issues: 0 };

  function saveCurrentSearch() {
    const trimmed = q.trim();
    if (!trimmed || saved.some((s) => s.query === trimmed)) return;
    const next = [...saved, { label: trimmed, query: trimmed }];
    setSaved(next); storeSavedSearches(campaign?.campaign_id, next);
  }
  function removeSaved(i) {
    const next = saved.filter((_, j) => j !== i);
    setSaved(next); storeSavedSearches(campaign?.campaign_id, next);
  }

  return html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1, minHeight: 0 }}>
    <div style=${{ display: 'flex', gap: 4, alignItems: 'center' }}>
      <${Input} value=${q} onInput=${setQ} placeholder="Search the vault…" style=${{ fontSize: 12.5, flex: 1 }} />
      ${query && html`<span title="Save this search" onClick=${saveCurrentSearch}
        style=${{ color: 'var(--ink-faint)', cursor: 'pointer', padding: 3, display: 'flex' }}><${Icon} name="sparkle" size=${13} /></span>`}
    </div>
    ${!query && saved.length > 0 && html`<div style=${{ padding: '4px 2px 0' }}>
      ${saved.map((s, i) => html`<div key=${s.query} onClick=${() => setQ(s.query)} onMouseEnter=${rowHover(true)} onMouseLeave=${rowHover(false)}
        style=${{ display: 'flex', alignItems: 'center', gap: 6, padding: '3px 6px', borderRadius: 4, fontSize: 11.5, color: 'var(--ink-muted)', cursor: 'pointer' }}>
        <${Icon} name="sparkle" size=${10} className="ck-ink-faint" />
        <span style=${{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', fontFamily: 'var(--font-mono)' }}>${s.label}</span>
        <${HoverActions}><${ActionIcon} icon="trash" title="Remove saved search" onClick=${() => removeSaved(i)} /></${HoverActions}>
      </div>`)}
    </div>`}
    <div style=${{ flex: 1, overflow: 'auto', padding: '4px 0 10px', margin: '0 -12px' }}>
      <div style=${{ display: 'flex', alignItems: 'center', padding: '8px 12px 4px', fontSize: 10, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>
        <span style=${{ flex: 1 }}>Vault</span>
        <span title="New page" onClick=${() => act.newPage('')} style=${{ color: 'var(--ink-faint)', cursor: 'pointer', padding: 2 }}><${Icon} name="plus" size=${12} /></span>
        <span title="New folder" onClick=${() => act.newFolder('')} style=${{ color: 'var(--ink-faint)', cursor: 'pointer', padding: 2 }}><${Icon} name="folder" size=${12} /></span>
      </div>
      ${matches
        ? html`<div>
            ${matches.map((p) => html`<${PageLeaf} key=${p.path} page=${p} depth=${0} active=${active} onOpen=${(e) => onOpen(p, e)} act=${act} ren=${ren} />`)}
            ${bodyHits.length > 0 && html`<div>
              <div style=${{ padding: '10px 12px 4px', fontSize: 10, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>In page text</div>
              ${bodyHits.map((h) => html`<div key=${h.path} onClick=${(e) => onOpen(h, e)}
                style=${{ padding: '4px 12px 4px 27px', cursor: 'pointer', borderRadius: 5 }}>
                <div style=${{ fontSize: 12.5, color: 'var(--ink-soft)', fontWeight: 500 }}>${h.title}</div>
                <div style=${{ fontSize: 11, color: 'var(--ink-faint)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}
                  dangerouslySetInnerHTML=${{ __html: h.snippet }} />
              </div>`)}
            </div>`}
            ${matches.length === 0 && bodyHits.length === 0 && html`<div style=${{ fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic', padding: '6px 12px' }}>No matches.</div>`}
          </div>`
        : html`<div onDragOver=${dnd.overFolder('')} onDragLeave=${dnd.leave('')} onDrop=${dnd.dropFolder('')}
            onContextMenu=${(e) => openContextMenu(e, [
              { label: 'New page', icon: 'plus', onClick: () => act.newPage('') },
              { label: 'New folder', icon: 'folder', onClick: () => act.newFolder('') },
            ])}
            style=${{ minHeight: 40, borderRadius: 5, boxShadow: dnd.over === '' && dnd.draggingPath ? 'inset 0 0 0 1px var(--burgundy-300)' : 'none' }}>
            ${rootFolders.map((c) => html`<${FolderNode} key=${c.path} node=${c} depth=${0} openSet=${openSet} toggle=${toggle} active=${active} onOpen=${onOpen} act=${act} ren=${ren} dnd=${dnd} />`)}
            ${tree.pages.map((p) => html`<${PageLeaf} key=${p.path} page=${p} depth=${0} active=${active} onOpen=${(e) => onOpen(p, e)} act=${act} ren=${ren} dnd=${dnd} />`)}
          </div>`}
    </div>
    ${diag && (() => {
      const suggestions = diag.unresolved + diag.orphans;
      const showSuggest = suggestions > 0 && suggestions > diagDismissed;
      if (!showSuggest && diag.issues === 0) return null;
      const dismiss = (e) => { e.stopPropagation(); storeDiagDismissed(campaign?.campaign_id, suggestions); setDiagDismissed(suggestions); };
      const rowStyle = { margin: '0 -12px', borderTop: '1px solid var(--rule-soft)', padding: '7px 12px', display: 'flex', alignItems: 'center', gap: 10, fontSize: 10.5, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)', cursor: 'pointer' };
      return html`<div>
        ${showSuggest && html`<div title="Linked pages not written yet — click to create them" onClick=${() => openModal('vaultDiag')} style=${rowStyle}>
          ${diag.unresolved > 0 && html`<span style=${{ display: 'flex', alignItems: 'center', gap: 4 }}><${Icon} name="plus" size=${10} className="ck-ink-faint" />${diag.unresolved} to write</span>`}
          ${diag.orphans > 0 && html`<span style=${{ display: 'flex', alignItems: 'center', gap: 4 }}><span style=${{ width: 6, height: 6, borderRadius: '50%', background: 'var(--rule-strong)' }} />${diag.orphans} unlinked</span>`}
          <span style=${{ flex: 1 }} />
          <span title="Dismiss until new links appear" onClick=${dismiss} style=${{ padding: '0 2px', opacity: 0.6 }}><${Icon} name="x" size=${11} /></span>
        </div>`}
        ${diag.issues > 0 && html`<div title="Vault diagnostics — click for the full list" onClick=${() => openModal('vaultDiag')} style=${rowStyle}>
          <span style=${{ display: 'flex', alignItems: 'center', gap: 4 }}><span style=${{ width: 6, height: 6, borderRadius: '50%', background: 'var(--burgundy)' }} />${diag.issues} file issue${diag.issues === 1 ? '' : 's'}</span>
        </div>`}
      </div>`;
    })()}
    <div style=${{ margin: '0 -12px', borderTop: '1px solid var(--rule)' }}>
      <div style=${{ padding: '7px 12px', display: 'flex', alignItems: 'center', gap: 8, fontSize: 11, color: 'var(--ink-faint)', cursor: 'pointer' }}>
        <span onClick=${() => setTplOpen((o) => !o)} style=${{ flex: 1, display: 'flex', alignItems: 'center', gap: 6 }}>
          <${Icon} name=${tplOpen ? 'chev-d' : 'chev-r'} size=${11} />
          <${Icon} name="scroll" size=${11} />
          <span>Templates</span>
          <span style=${{ color: 'var(--ink-faint)', opacity: 0.7 }}>${(store.templates || []).length || ''}</span>
        </span>
        <span title="Deleted pages and folders — restore or empty" onClick=${() => openModal('trash')}
          style=${{ color: 'var(--ink-faint)', cursor: 'pointer', padding: 2 }}><${Icon} name="trash" size=${12} /></span>
        <span title="New template" onClick=${newTemplateFlow} style=${{ color: 'var(--ink-faint)', cursor: 'pointer', padding: 2 }}><${Icon} name="plus" size=${12} /></span>
      </div>
      ${tplOpen && html`<div style=${{ padding: '0 0 6px' }}>
        ${(store.templates || []).length === 0
          ? html`<div style=${{ fontSize: 11, color: 'var(--ink-faint)', fontStyle: 'italic', padding: '2px 12px 6px 30px' }}>No templates yet.</div>`
          : (store.templates || []).map((t) => html`<div key=${t.name}
              onClick=${() => navigate('template', { name: t.name })}
              style=${{ padding: '4px 12px 4px 30px', display: 'flex', alignItems: 'center', gap: 7, fontSize: 12, color: 'var(--ink-soft)', cursor: 'pointer' }}
              onMouseEnter=${(e) => (e.currentTarget.style.background = 'var(--surface)')}
              onMouseLeave=${(e) => (e.currentTarget.style.background = 'transparent')}>
              <span style=${{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${t.name}</span>
              ${t.kind && html`<span style=${{ fontSize: 10, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)' }}>${t.kind}</span>`}
            </div>`)}
      </div>`}
      <div onClick=${attachVaultFlow} title="Change vault folder (advanced)"
        style=${{ borderTop: '1px solid var(--rule-soft)', padding: '6px 12px', display: 'flex', alignItems: 'center', gap: 7, fontSize: 10, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)', cursor: 'pointer' }}>
        <span style=${{ width: 5, height: 5, borderRadius: '50%', background: 'var(--moss)', flex: '0 0 auto' }} />
        <span style=${{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', direction: 'rtl', textAlign: 'left' }}>${campaign.vault_path}</span>
      </div>
    </div>
  </div>`;
}

// Compact icon nav for the Codex/page screens — the same world destinations as
// the main Sidebar, but a single icon row so the file browser keeps its height.
function WorldNavBar({ campaign, active = 'codex' }) {
  const id = campaign?.campaign_id;
  return html`<div style=${{ display: 'flex', gap: 2, padding: '2px 0 6px', marginBottom: 2, borderBottom: '1px solid var(--rule-soft)' }}>
    ${WORLD_NAV.map((d) => {
      const on = d.key === active;
      return html`<span key=${d.key} title=${d.label} onClick=${() => navToWorldDest(d, id)} style=${{
        flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: '6px 0', borderRadius: 4, cursor: 'pointer',
        color: on ? 'var(--burgundy)' : 'var(--ink-soft)',
        background: on ? 'var(--burgundy-50)' : 'transparent',
      }}
        onMouseEnter=${(e) => { if (!on) e.currentTarget.style.background = 'rgba(120,90,40,.08)'; }}
        onMouseLeave=${(e) => { if (!on) e.currentTarget.style.background = 'transparent'; }}>
        <${Icon} name=${d.icon} size=${15} />
      </span>`;
    })}
  </div>`;
}

// Public sidebar for the Codex/page screens: brand + compact world nav, then the
// vault file browser filling the rest. Width shares `ck_sidebar_w` with the main
// Sidebar so resizing is consistent app-wide.
export function FileTree({ campaign, tree, active, onOpen, act }) {
  const [width, onResize] = useSidebarWidth('ck_sidebar_w');
  return html`<aside style=${{ width, flex: `0 0 ${width}px`, borderRight: '1px solid var(--rule)', background: 'var(--paper-deep)', padding: '14px 12px', display: 'flex', flexDirection: 'column', gap: 2, minHeight: 0, position: 'relative' }}>
    <${ResizeHandle} onMouseDown=${onResize} />
    <div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '4px 6px 12px', borderBottom: '1px solid var(--rule-soft)', marginBottom: 4, cursor: 'pointer' }}
      onClick=${() => navigate('library')}>
      <${BrandMark} size=${30} />
      <div style=${{ lineHeight: 1.15, minWidth: 0 }}>
        <div style=${{ fontFamily: 'var(--font-display)', fontSize: 14, fontWeight: 500, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${campaign?.name || 'World'}</div>
        <div style=${{ fontSize: 10, fontWeight: 500, color: 'var(--ink-faint)', letterSpacing: '0.08em', textTransform: 'uppercase', marginTop: 2 }}>${campaign?.system || 'Worldbuilding'}</div>
      </div>
    </div>
    <${WorldNavBar} campaign=${campaign} />
    <${VaultPanel} campaign=${campaign} tree=${tree} active=${active} onOpen=${onOpen} act=${act} />
  </aside>`;
}

function PageCard({ page, onOpen, picked }) {
  const tone = toneForKind(page.kind);
  const folder = dirOf(page.path);
  const border = picked ? 'var(--burgundy)' : 'var(--rule)';
  return html`<div onClick=${onOpen} style=${{
    background: picked ? 'var(--burgundy-50)' : 'var(--surface)', border: `1px solid ${border}`, borderRadius: 8,
    padding: 14, display: 'flex', flexDirection: 'column', gap: 9, cursor: 'pointer', boxShadow: 'var(--shadow-soft)',
    position: 'relative',
  }}
    onMouseEnter=${(e) => { if (!picked) e.currentTarget.style.borderColor = 'var(--rule-strong)'; }}
    onMouseLeave=${(e) => { e.currentTarget.style.borderColor = border; }}>
    ${picked != null && html`<span style=${{
      position: 'absolute', top: 8, right: 8, width: 18, height: 18, borderRadius: '50%',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      background: picked ? 'var(--burgundy)' : 'var(--surface)', color: 'var(--paper)',
      border: `1.5px solid ${picked ? 'var(--burgundy)' : 'var(--rule-strong)'}`,
    }}>${picked && html`<${Icon} name="check" size=${10} />`}</span>`}
    <div style=${{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
      ${glyphFor(page.kind, 34)}
      <div style=${{ flex: 1, minWidth: 0 }}>
        <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500, color: 'var(--ink)', lineHeight: 1.2 }}>${page.title}</div>
        <div style=${{ fontSize: 10.5, color: 'var(--ink-faint)', marginTop: 2, display: 'flex', alignItems: 'center', gap: 5, fontFamily: 'var(--font-mono)' }}>
          <${Icon} name="folder" size=${10} /> ${folder || 'vault root'}
        </div>
      </div>
    </div>
    ${page.summary && html`<div style=${{ fontSize: 12.5, color: 'var(--ink-soft)', lineHeight: 1.5, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>${page.summary}</div>`}
    ${page.modified && html`<div style=${{ display: 'flex', alignItems: 'center', fontSize: 11, color: 'var(--ink-muted)' }}>
      <span style=${{ flex: 1 }} />
      <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 10.5, color: 'var(--ink-faint)' }}>${agoLabel(page.modified)}</span>
    </div>`}
  </div>`;
}

function FolderCard({ node, onOpen }) {
  const kind = kindForFolder(node.name);
  const subN = node.folders.size;
  const sub = subN ? `${subN} folder${subN === 1 ? '' : 's'} · ${countPages(node)} page${countPages(node) === 1 ? '' : 's'}` : `${countPages(node)} page${countPages(node) === 1 ? '' : 's'}`;
  return html`<div onClick=${onOpen} style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, padding: '13px 15px', display: 'flex', alignItems: 'center', gap: 12, cursor: 'pointer', boxShadow: 'var(--shadow-soft)' }}>
    ${kind ? glyphFor(kind, 36) : html`<div style=${{ width: 36, height: 36, borderRadius: 7, flex: '0 0 auto', background: 'var(--burgundy-50)', color: 'var(--burgundy)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
      <${Icon} name="folder" size=${16} />
    </div>`}
    <div style=${{ flex: 1, minWidth: 0 }}>
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 14.5, fontWeight: 500, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${node.name}</div>
      <div style=${{ fontSize: 11, color: 'var(--ink-faint)', marginTop: 1 }}>${sub}</div>
    </div>
    <${Icon} name="chev-r" size=${13} className="ck-ink-faint" />
  </div>`;
}

// The vault-mutation menu shared by the Explorer (codex) and the page editor.
// `opts.afterDelete(path)` lets a caller (e.g. the page screen) react when the
// page it is showing is deleted.
export function makeVaultActions(campaign, folders, opts = {}) {
  return {
    newPage: (folder) => openModal('newPage', { folder, kind: kindForFolder(baseName(folder)) || 'npc' }),
    promotePage: (p) => openModal('promotePage', { page: p, folders }),
    newFolder: (parent) => openModal('textPrompt', {
      title: 'New folder', label: parent ? `New folder inside ${parent}` : 'New folder name', placeholder: 'Riddles', confirmLabel: 'Create folder',
      onSubmit: (name) => createVaultFolder(parent ? `${parent}/${name}` : name),
    }),
    renamePage: (p) => openModal('textPrompt', {
      title: 'Rename page', label: 'New title', initial: p.title, confirmLabel: 'Rename',
      onSubmit: (title) => moveVaultEntry(p.path, dirOf(p.path) ? `${dirOf(p.path)}/${title}.md` : `${title}.md`),
    }),
    movePage: (p) => openModal('movePage', {
      name: p.title, folders, current: dirOf(p.path),
      onSubmit: (dest) => moveVaultEntry(p.path, dest ? `${dest}/${baseName(p.path)}` : baseName(p.path)),
    }),
    duplicatePage: async (p) => {
      try { const copy = await duplicateVaultPage(p.path); setOp(`Duplicated to “${copy.title}”`, 'done'); }
      catch (e) { setOp(`Duplicate failed: ${e.message}`, 'err'); }
    },
    // Open the page with the rail on the Chat tab and a question pre-filled
    // in the composer (the user edits/sends it — nothing fires automatically).
    askKeeper: (p) => {
      // Full default shape: keeperState() only fills defaults when store.keeper
      // is absent entirely, and Transcript renders before openPanel() runs.
      setState({ keeper: {
        chatId: null, events: [], attachments: [], live: null, error: null,
        ...(store.keeper || {}), open: true, draft: `What do we know about [[${p.title}]]?`,
      } });
      navigate('page', { path: p.path, rail: 'chat' });
    },
    deletePage: (p) => openModal('confirm', {
      title: 'Move page to trash', message: html`Move ${html`<strong>${p.title}</strong>`} to the world's trash? Restore it any time from the Trash view (kept 30 days).`,
      confirmLabel: 'Move to trash', onConfirm: async () => { await deleteVaultPage(p.path); if (opts.afterDelete) opts.afterDelete(p.path); },
    }),
    renameFolder: (n) => openModal('textPrompt', {
      title: 'Rename folder', label: 'New folder name', initial: n.name, confirmLabel: 'Rename',
      onSubmit: (name) => moveVaultEntry(n.path, dirOf(n.path) ? `${dirOf(n.path)}/${name}` : name),
    }),
    deleteFolder: (n) => {
      const nPages = countPages(n);
      const detail = nPages
        ? html` This moves the folder, its ${nPages} page${nPages === 1 ? '' : 's'}, and any subfolders to the world's trash.`
        : html` This moves the empty folder to the world's trash.`;
      openModal('confirm', {
        title: 'Move folder to trash',
        message: html`Move ${html`<strong>${n.name}</strong>`} to the world's trash?${detail} Restore any time from the Trash view (kept 30 days).`,
        confirmLabel: 'Move to trash',
        onConfirm: async () => {
          await deleteVaultFolder(n.path);
          if (opts.afterDeleteFolder) opts.afterDeleteFolder(n.path);
        },
      });
    },
  };
}

function VaultView({ campaign }) {
  const store = useStore();
  const [sel, setSel] = useState('');           // current folder path ('' = vault root)
  const [view, setView] = useState('folders');  // 'folders' | 'all' | 'tags'
  const [selTag, setSelTag] = useState(null);
  // Multi-select (Phase 13B): card clicks toggle instead of navigating.
  const [picking, setPicking] = useState(false);
  const [picked, setPicked] = useState(() => new Set());

  useEffect(() => { loadVaultTree(campaign.campaign_id); loadVaultDiagnostics(campaign.campaign_id); }, [campaign.campaign_id, store.dirty_vault]);
  useEffect(() => { if (view === 'tags') loadVaultTags(campaign.campaign_id); }, [view, campaign.campaign_id, store.dirty_vault]);
  // Tag jump target from the command palette (navigate('codex', { tag })).
  const routeTag = store.route.params?.tag;
  useEffect(() => { if (routeTag) { setView('tags'); setSelTag(routeTag); } }, [routeTag]);
  useEffect(() => watchVault(campaign.campaign_id, () => {
    loadVaultTree(campaign.campaign_id);
    loadVaultTags(campaign.campaign_id);
    loadVaultDiagnostics(campaign.campaign_id);
  }), [campaign.campaign_id]);

  const pages = store.vaultPages || [];
  const folders = store.vaultFolders || [];
  const tree = buildTree(folders, pages);
  const cur = nodeAt(tree, sel) || tree;

  function openPage(p, e) { openPageEvt(p.path, e); }
  const act = makeVaultActions(campaign, folders);

  const togglePick = (path) => setPicked((s) => {
    const n = new Set(s);
    if (n.has(path)) n.delete(path); else n.add(path);
    return n;
  });
  const clearPick = () => { setPicked(new Set()); setPicking(false); };
  // Card props in one place — the three grids (folder, all, tag) share them.
  const cardProps = (p) => ({
    page: p,
    onOpen: (e) => (picking ? togglePick(p.path) : openPage(p, e)),
    picked: picking ? picked.has(p.path) : null,
  });
  const reportBulk = (r, verb) =>
    setOp(`${r.done} page${r.done === 1 ? '' : 's'} ${verb}${r.errors.length ? ` · ${r.errors.length} failed` : ''}`,
      r.errors.length ? 'err' : 'done');
  const bulkTag = () => openModal('textPrompt', {
    title: 'Tag pages', label: `Add a tag to ${picked.size} page${picked.size === 1 ? '' : 's'}`,
    placeholder: 'faction/harpers', confirmLabel: 'Add tag',
    onSubmit: async (t) => reportBulk(await bulkVault('tag', [...picked], { tag: t }), 'tagged'),
  });
  const bulkMove = () => openModal('movePage', {
    name: `${picked.size} page${picked.size === 1 ? '' : 's'}`, folders, current: '',
    onSubmit: async (dest) => {
      reportBulk(await bulkVault('move', [...picked], { folder: dest }), 'moved');
      clearPick();
    },
  });
  const bulkDelete = () => openModal('confirm', {
    title: 'Move pages to trash',
    message: `Move ${picked.size} page${picked.size === 1 ? '' : 's'} to the world's trash? They restore together as one group from the Trash view.`,
    confirmLabel: 'Move to trash',
    onConfirm: async () => {
      reportBulk(await bulkVault('delete', [...picked]), 'moved to trash');
      clearPick();
    },
  });

  const total = pages.length;
  const subFolders = [...cur.folders.values()].sort((a, b) => a.name.localeCompare(b.name));
  const recent = [...pages].sort((a, b) => (b.modified || 0) - (a.modified || 0));
  const crumbs = sel ? sel.split('/') : [];

  const topbar = html`<${Topbar} crumbs=${[{ label: campaign.name, onClick: () => openCampaign(campaign.campaign_id) }, 'Codex']}
    right=${html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <${Btn} kind="secondary" size="sm" icon="download" onClick=${importNotesFlow}>Import notes</${Btn}>
      <${Btn} kind="secondary" size="sm" icon="sparkle" onClick=${enhanceFlow}>Enhance with AI</${Btn}>
      <${Btn} kind="secondary" size="sm" icon="time" onClick=${() => openModal('worldHistory')}>History</${Btn}>
      <${Btn} kind=${picking ? 'primary' : 'secondary'} size="sm" icon=${picking ? 'x' : 'check'}
        onClick=${() => (picking ? clearPick() : setPicking(true))}>${picking ? 'Done' : 'Select'}</${Btn}>
    </div>`} />`;
  return html`<${Shell}
    sidebar=${html`<${FileTree} campaign=${campaign} tree=${tree} active=${null} onOpen=${openPage} act=${act} />`}
    topbar=${topbar} tabstrip=${html`<${TabStrip} />`} bodyStyle=${{ padding: 0 }}>
    <div style=${{ height: '100%', overflow: 'auto', padding: '22px 26px', minWidth: 0 }}>
      <div style=${{ display: 'flex', alignItems: 'flex-end', gap: 12, marginBottom: 4 }}>
        <div style=${{ maxWidth: 560 }}>
          <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--burgundy)' }}>The world wiki</div>
          <h2 style=${{ fontFamily: 'var(--font-display)', fontSize: 24, fontWeight: 500, letterSpacing: '-0.015em', marginTop: 2 }}>
            The Codex <span style=${{ color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)', fontSize: 16 }}>${total}</span>
          </h2>
          <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', marginTop: 6, lineHeight: 1.5, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>
            Markdown files in folders you arrange however you like. Folders are real on disk — yours to nest and rename.
          </div>
        </div>
        <span style=${{ flex: 1 }} />
        <div style=${{ display: 'flex', gap: 4, padding: 3, background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 5 }}>
          ${[['folders', 'grid', 'Folders'], ['all', 'scroll', 'All pages'], ['tags', 'tag', 'Tags']].map(([v, ic, label]) => html`
            <button key=${v} onClick=${() => { setView(v); if (v !== 'folders') setSel(''); }} style=${{ padding: '5px 9px', borderRadius: 3, whiteSpace: 'nowrap', background: view === v ? 'var(--paper-deep)' : 'transparent', color: view === v ? 'var(--ink)' : 'var(--ink-muted)', fontSize: 12, display: 'flex', alignItems: 'center', gap: 5, cursor: 'pointer', border: 'none' }}><${Icon} name=${ic} size=${12} /> ${label}</button>`)}
        </div>
      </div>

      ${total === 0
        ? html`<div style=${{ marginTop: 24 }}><${Empty} icon="scroll" title="No pages yet">Create your first page or folder from the Vault panel — each page is a plain markdown file you fully own.</${Empty}></div>`
        : view === 'tags'
          ? html`<div style=${{ marginTop: 18 }}>
              ${(store.vaultTags || []).length === 0
                ? html`<${Empty} icon="tag" title="No page tags yet">Add <code>tags:</code> to a page's frontmatter — hierarchies via <code>/</code> (e.g. <code>Location/City</code>).</${Empty}>`
                : html`<div>
                    <div style=${{ display: 'flex', flexWrap: 'wrap', gap: 6, marginBottom: 18 }}>
                      ${(store.vaultTags || []).map((t) => html`<span key=${t.tag} onClick=${() => setSelTag(selTag === t.tag ? null : t.tag)}
                        onContextMenu=${(e) => openContextMenu(e, [
                          { label: 'Show tagged pages', icon: 'tag', onClick: () => setSelTag(t.tag) },
                          { label: 'Copy #tag', icon: 'copy', onClick: () => copyText(`#${t.tag}`, 'Tag copied') },
                        ])}
                        style=${{ display: 'inline-flex', alignItems: 'center', gap: 5, padding: '3px 10px', borderRadius: 999, fontSize: 12, fontFamily: 'var(--font-mono)', cursor: 'pointer',
                          background: selTag === t.tag ? 'var(--burgundy-50)' : 'var(--surface)', border: `1px solid ${selTag === t.tag ? 'var(--burgundy-300)' : 'var(--rule)'}`,
                          color: selTag === t.tag ? 'var(--burgundy-700)' : 'var(--ink-soft)' }}>
                        #${t.tag}<span style=${{ fontSize: 10.5, color: 'var(--ink-faint)' }}>${t.count}</span>
                      </span>`)}
                    </div>
                    ${selTag && html`<div style=${{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 10 }}>
                      ${pages.filter((p) => (p.tags || []).some((tg) => tg === selTag || tg.startsWith(selTag + '/')))
                        .map((p) => html`<${PageCard} key=${p.path} ...${cardProps(p)} />`)}
                    </div>`}
                  </div>`}
            </div>`
        : view === 'all'
          ? html`<div style=${{ marginTop: 18, display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 10 }}>
              ${recent.map((p) => html`<${PageCard} key=${p.path} ...${cardProps(p)} />`)}
            </div>`
          : html`<div>
              <div style=${{ display: 'flex', alignItems: 'center', gap: 6, margin: '20px 0 10px', fontSize: 12.5, color: 'var(--ink-muted)' }}>
                <span onClick=${() => setSel('')} style=${{ cursor: 'pointer', color: sel ? 'var(--burgundy)' : 'var(--ink-faint)', fontWeight: 600, letterSpacing: '0.08em', textTransform: 'uppercase', fontSize: 10.5 }}>Vault</span>
                ${crumbs.map((part, i) => html`<span key=${i} style=${{ display: 'flex', alignItems: 'center', gap: 6 }}><${Icon} name="chev-r" size=${10} className="ck-ink-faint" /><span onClick=${() => setSel(crumbs.slice(0, i + 1).join('/'))} style=${{ cursor: 'pointer' }}>${part}</span></span>`)}
              </div>

              <div style=${{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(228px, 1fr))', gap: 10 }}>
                ${subFolders.map((n) => html`<${FolderCard} key=${n.path} node=${n} onOpen=${() => setSel(n.path)} />`)}
                <div onClick=${() => newFolder(sel)} style=${{ border: '1.5px dashed var(--rule-strong)', borderRadius: 8, padding: '13px 15px', display: 'flex', alignItems: 'center', gap: 12, color: 'var(--ink-muted)', cursor: 'pointer' }}>
                  <div style=${{ width: 36, height: 36, borderRadius: 7, flex: '0 0 auto', background: 'var(--paper-deep)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--burgundy)' }}><${Icon} name="plus" size=${16} /></div>
                  <div style=${{ fontFamily: 'var(--font-display)', fontSize: 14, color: 'var(--ink-soft)' }}>New folder</div>
                </div>
              </div>

              ${cur.pages.length > 0 && html`<div>
                <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', margin: '24px 0 10px' }}>${sel ? 'Pages here' : 'Pages at the root'}</div>
                <div style=${{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 10 }}>
                  ${cur.pages.map((p) => html`<${PageCard} key=${p.path} ...${cardProps(p)} />`)}
                </div>
              </div>`}
            </div>`}
      ${picking && html`<div style=${{
        position: 'sticky', bottom: 14, marginTop: 20, display: 'flex', alignItems: 'center', gap: 8,
        padding: '10px 14px', background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)',
        borderRadius: 8, boxShadow: 'var(--shadow-raised)', maxWidth: 560,
      }}>
        <span style=${{ fontSize: 12.5, color: 'var(--ink)', fontWeight: 500, flex: 1 }}>
          ${picked.size} selected
        </span>
        <${Btn} kind="secondary" size="sm" icon="tag" disabled=${!picked.size} onClick=${bulkTag}>Tag…</${Btn}>
        <${Btn} kind="secondary" size="sm" icon="folder" disabled=${!picked.size} onClick=${bulkMove}>Move…</${Btn}>
        <${Btn} kind="secondary" size="sm" icon="trash" disabled=${!picked.size} onClick=${bulkDelete}>Delete</${Btn}>
        <${Btn} kind="ghost" size="sm" onClick=${clearPick}>Cancel</${Btn}>
      </div>`}
    </div>
  </${Shell}>`;
}

export function CodexScreen() {
  const store = useStore();
  const c = store.campaign;
  if (!c) { navigate('library'); return null; }
  return html`<${VaultView} campaign=${c} />`;
}
