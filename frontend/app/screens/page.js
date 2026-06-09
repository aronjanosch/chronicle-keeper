// The Page — rendered reading view + a CodeMirror 6 source editor (Phase 7.5).
// Read mode renders the markdown with the right rail (infobox, AI memory, outline,
// backlinks). Edit mode mounts CM6 over the literal .md (frontmatter + body), with
// markdown highlight, [[ / #tag autocomplete, ⌘F search, and format shortcuts.
// Auto-saves to the vault (800ms). See cm.js.
import { html, useState, useEffect, useRef, useMemo } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, useStore } from '../core.js';
import { Shell, Topbar, useSidebarWidth, ResizeHandle } from '../shell.js';
import { Empty, Icon, PageBody, splitDoc, joinDoc, parseProps } from '../ui.js';
import { readVaultPage, saveVaultPage, openCampaign, loadVaultTree, loadKindSchemas, loadAtlasMaps, createVaultPage, watchVault, uploadVaultAsset } from '../actions.js';
import { mountEditor } from '../cm.js';
import { FileTree, buildTree, makeVaultActions, iconForKind, KINDS } from './codex.js';
import { keeperState, openPanel, Conversation } from '../keeperPanel.js';

function kindLabel(k) {
  return (KINDS.find((x) => x.value === k) || {}).label || k || 'Page';
}

// Frontmatter keys that never render as infobox fields (page-data-model-spec).
const RESERVED_KEYS = new Set(['kind', 'aliases', 'tags', 'summary', 'cssclasses', 'publish', 'permalink']);

function schemaFor(schemas, kind) {
  return ((schemas || []).find((s) => s.kind === kind) || {}).fields || [];
}

const CHECKED = new Set(['true', 'yes', 'x', '✓', '1']);

function FieldVal({ type, values, pages }) {
  if (type === 'checkbox') {
    const on = CHECKED.has(String(values[0] || '').toLowerCase());
    return html`<span class="ck-prop-tag" style=${{ color: on ? 'var(--moss)' : 'var(--ink-faint)' }}>${on ? '✓ yes' : '— no'}</span>`;
  }
  return values.map((v, i) => {
    const m = /^\[\[([^\]|]+)(?:\|([^\]]+))?\]\]$/.exec(String(v).trim());
    if (!m) return html`<span class="ck-prop-tag" key=${i}>${v}</span>`;
    const name = m[1].split('#')[0].trim();
    const label = (m[2] || m[1]).trim();
    const nl = name.toLowerCase();
    const target = (pages || []).find((p) => p.title.toLowerCase() === nl || (p.aliases || []).includes(nl));
    return html`<span class="ck-prop-tag ${target ? 'ck-prop-link' : ''}" key=${i}
      onClick=${target ? (e) => { e.stopPropagation(); navigate('page', { path: target.path }); } : null}>${label}</span>`;
  });
}

// Schema fields first (typed; blanks kept as placeholders), then custom
// non-reserved keys as plain text rows.
function infoboxRows(props, fields) {
  const byKey = new Map(props.map((p) => [p.key, p]));
  const rows = fields.map((f) => ({
    key: f.name,
    type: f.type,
    values: ((byKey.get(f.name) || {}).values || []).filter((v) => v !== ''),
  }));
  const known = new Set(fields.map((f) => f.name));
  for (const p of props) {
    if (known.has(p.key) || RESERVED_KEYS.has(p.key)) continue;
    rows.push({ key: p.key, type: p.list ? 'list' : 'text', values: p.values.filter((v) => v !== '') });
  }
  return rows;
}

function loadFlag(key, fallback) {
  try { const v = localStorage.getItem(key); return v == null ? fallback : v === '1'; } catch (_) { return fallback; }
}
function saveFlag(key, val) {
  try { localStorage.setItem(key, val ? '1' : '0'); } catch (_) { /* private mode */ }
}

function RailCard({ icon, title, right, children }) {
  const [open, setOpen] = useState(() => loadFlag('ck_rail_card_' + title, true));
  const toggle = () => setOpen((o) => { saveFlag('ck_rail_card_' + title, !o); return !o; });
  return html`<div class="ck-rail-card ${open ? '' : 'collapsed'}">
    <div class="ck-rail-head" onClick=${toggle} title=${open ? 'Collapse' : 'Expand'}>
      <${Icon} name=${icon} size=${12} /> ${title}${right && html`<span class="ck-rail-right">${right}</span>`}
      <${Icon} name=${open ? 'chev-d' : 'chev-r'} size=${11} className="ck-rail-chev" />
    </div>
    ${open && children}
  </div>`;
}

// Infobox: the page kind's frontmatter fields (design page.jsx right rail).
function InfoboxCard({ fm, kind, schemas, pages }) {
  const fields = schemaFor(schemas, kind);
  const rows = infoboxRows(parseProps(fm), fields).filter((r) => r.values.length);
  if (!kind && !rows.length) return null;
  return html`<${RailCard} icon="book" title="Infobox" right="frontmatter">
    ${kind && html`<div class="ck-infobox-row"><span class="ck-infobox-key">kind</span>
      <span class="ck-prop-vals"><span class="ck-prop-tag">${kindLabel(kind)}</span></span></div>`}
    ${rows.map((r) => html`<div class="ck-infobox-row" key=${r.key}>
      <span class="ck-infobox-key">${r.key}</span>
      <span class="ck-prop-vals"><${FieldVal} type=${r.type} values=${r.values} pages=${pages} /></span>
    </div>`)}
    ${!rows.length && html`<div class="ck-prop-blank" style=${{ paddingTop: 4 }}>No fields yet — edit the page properties</div>`}
  </${RailCard}>`;
}

// `summary:` is a scalar frontmatter line — replace it in place (or append).
function setFmSummary(content, summary) {
  const { fm, body } = splitDoc(content);
  const s = summary.trim().replace(/\s+/g, ' ');
  const val = /[:#"']/.test(s) || /^[-\s[{*&!|>@`]/.test(s) ? JSON.stringify(s) : s;
  const line = val ? `summary: ${val}` : 'summary:';
  const lines = fm ? fm.split('\n') : [];
  const i = lines.findIndex((l) => /^summary:/.test(l));
  if (i >= 0) lines[i] = line; else lines.push(line);
  return joinDoc(lines.join('\n'), body);
}

// The AI's memory: the one-liner fed to summaries. Click to edit, blur saves.
function SummaryCard({ page, onSave }) {
  const [editing, setEditing] = useState(false);
  const taRef = useRef(null);

  useEffect(() => {
    if (editing && taRef.current) {
      const ta = taRef.current;
      ta.value = page.summary || '';
      autoGrow(ta);
      ta.focus();
      ta.setSelectionRange(ta.value.length, ta.value.length);
    }
  }, [editing]);

  function commit() {
    const v = taRef.current ? taRef.current.value : '';
    setEditing(false);
    if (v.trim() === (page.summary || '').trim()) return;
    onSave(setFmSummary(page.content, v)).catch(() => {});
  }

  return html`<${RailCard} icon="feather" title="In the AI's memory">
    ${editing
      ? html`<textarea ref=${taRef} class="ck-rail-summary-edit" spellcheck="false"
          onInput=${(e) => autoGrow(e.target)} onBlur=${commit}
          onKeyDown=${(e) => { if (e.key === 'Escape' || (e.key === 'Enter' && !e.shiftKey)) { e.preventDefault(); e.target.blur(); } }} />`
      : html`<div class="ck-rail-summary" onClick=${() => setEditing(true)} title="Click to edit">
          ${page.summary && page.summary.trim()
            ? page.summary
            : html`<span class="ck-prop-blank">No summary yet — click to write one</span>`}
        </div>`}
    <div class="ck-rail-hint">Fed to the LLM as background when this page is mentioned in a session.</div>
  </${RailCard}>`;
}

function TagsCard({ tags }) {
  if (!tags || !tags.length) return null;
  return html`<${RailCard} icon="tag" title="Tags">
    <div style=${{ display: 'flex', flexWrap: 'wrap', gap: 5 }}>
      ${tags.map((t, i) => html`<span class="ck-prop-tag mono" key=${i}>#${String(t).replace(/^#/, '')}</span>`)}
    </div>
  </${RailCard}>`;
}

// Reverse lookup: every atlas pin (or map entry) referencing this page.
function OnMapCard({ path, maps, campaignId }) {
  const hits = [];
  for (const m of maps || []) {
    if (m.page === path) hits.push({ map: m, pin: null });
    for (const p of m.pins || []) {
      if (p.page === path) hits.push({ map: m, pin: p });
    }
  }
  if (!hits.length) return null;
  return html`<${RailCard} icon="map" title="On the map" right=${hits.length > 1 ? String(hits.length) : null}>
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 2 }}>
      ${hits.map(({ map: m, pin }, i) => html`<div class="ck-rail-link" key=${i}
        onClick=${() => navigate('atlas', { id: campaignId, map: m.id, ...(pin ? { pin: pin.id } : {}) })}>
        <${Icon} name=${pin ? 'pin' : 'map'} size=${12} className="ck-ink-muted" />
        <span>${pin ? `Pinned on ${m.name}` : `Has its own map: ${m.name}`}</span>
      </div>`)}
    </div>
  </${RailCard}>`;
}

// Headings (H1–H3) parsed from the page body, skipping fenced code blocks.
// Decision (Phase 7c): depth capped at H3.
function parseOutline(md) {
  const out = [];
  let fence = false;
  for (const raw of (md || '').split('\n')) {
    const line = raw.trimEnd();
    if (/^(```|~~~)/.test(line.trim())) { fence = !fence; continue; }
    if (fence) continue;
    const m = line.match(/^(#{1,3})\s+(.+)/);
    if (m) out.push({ level: m[1].length, text: m[2].replace(/[*_`]/g, '').trim() });
  }
  return out;
}

// Sticky in-page nav. Click scrolls to the Nth rendered heading — the prose is
// rendered from the same source in order, so outline index === h1/h2/h3 index.
function OutlineCard({ outline, scrollRef }) {
  if (!outline || outline.length < 2) return null;
  const go = (i) => {
    // Scope to the prose body — the page title <h1> sits outside .ck-prose, so
    // querying the whole scroller would be off by one vs the parsed outline.
    const hs = scrollRef.current?.querySelectorAll('.ck-prose h1, .ck-prose h2, .ck-prose h3');
    if (hs && hs[i]) hs[i].scrollIntoView({ behavior: 'smooth', block: 'start' });
  };
  return html`<${RailCard} icon="scroll" title="Outline">
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 1 }}>
      ${outline.map((h, i) => html`<div key=${i} class="ck-rail-link"
        onClick=${() => go(i)} style=${{ paddingLeft: (h.level - 1) * 12 }}>
        <span style=${{ fontSize: h.level === 1 ? 12.5 : 12, color: h.level === 1 ? 'var(--ink)' : 'var(--ink-soft)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${h.text}</span>
      </div>`)}
    </div>
  </${RailCard}>`;
}

function BacklinksCard({ path, pages, links }) {
  const sources = [...new Set((links || [])
    .filter((l) => l.target_path === path)
    .map((l) => l.source_path))];
  if (!sources.length) return null;
  const byPath = new Map((pages || []).map((p) => [p.path, p]));
  return html`<${RailCard} icon="link" title="Linked from" right=${String(sources.length)}>
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 2 }}>
      ${sources.map((src) => {
        const p = byPath.get(src);
        return html`<div class="ck-rail-link" key=${src} onClick=${() => navigate('page', { path: src })}>
          <${Icon} name=${iconForKind(p?.kind)} size=${12} className="ck-ink-muted" />
          <span>${p?.title || src}</span>
        </div>`;
      })}
    </div>
  </${RailCard}>`;
}

function autoGrow(ta) {
  ta.style.height = 'auto';
  ta.style.height = ta.scrollHeight + 'px';
}

// Pixel position of the textarea caret (mirror-div trick) for the [[ autocomplete.
export function caretCoords(ta) {
  try {
    const s = getComputedStyle(ta);
    const div = document.createElement('div');
    ['fontFamily', 'fontSize', 'fontWeight', 'letterSpacing', 'paddingTop', 'paddingRight',
      'paddingBottom', 'paddingLeft', 'borderWidth', 'boxSizing'].forEach((p) => { div.style[p] = s[p]; });
    div.style.position = 'absolute';
    div.style.visibility = 'hidden';
    div.style.whiteSpace = 'pre-wrap';
    div.style.wordWrap = 'break-word';
    div.style.lineHeight = s.lineHeight;
    div.style.width = ta.clientWidth + 'px';
    document.body.appendChild(div);
    div.textContent = ta.value.slice(0, ta.selectionStart);
    const span = document.createElement('span');
    span.textContent = '​';
    div.appendChild(span);
    const top = span.offsetTop;
    const left = span.offsetLeft;
    const lh = parseFloat(s.lineHeight) || 18;
    document.body.removeChild(div);
    const rect = ta.getBoundingClientRect();
    return {
      top: rect.top + top - ta.scrollTop,
      left: Math.min(rect.left + left - ta.scrollLeft, window.innerWidth - 280),
      lineHeight: lh,
    };
  } catch (_) {
    const r = ta.getBoundingClientRect();
    return { top: r.top, left: r.left, lineHeight: 18 };
  }
}

// CodeMirror 6 source editor — edits the whole .md (frontmatter + body) as literal
// text. Lazy-mounts the vendored bundle into a div; re-keyed on path/reload by parent.
function CmEditor({ content, pages, onSave, onCreate, onState }) {
  const hostRef = useRef(null);
  const pagesRef = useRef(pages); pagesRef.current = pages;
  useEffect(() => {
    let ctl = null, dead = false;
    mountEditor(hostRef.current, {
      doc: content,
      getPages: () => pagesRef.current,
      onCreatePage: (name) => onCreate(name),
      onUploadAsset: uploadVaultAsset,
      onSave,
      onState,
    }).then((c) => { if (dead) c.destroy(); else ctl = c; });
    return () => { dead = true; if (ctl) ctl.destroy(); };
  }, []);
  return html`<div ref=${hostRef} class="ck-cm" />`;
}

function KebabMenu({ items }) {
  const [open, setOpen] = useState(false);
  return html`<div style=${{ position: 'relative' }}>
    <button onClick=${() => setOpen((o) => !o)} style=${{ padding: '6px 8px', color: 'var(--ink-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>
      <${Icon} name="dots" size=${15} />
    </button>
    ${open && html`<div>
      <div onClick=${() => setOpen(false)} style=${{ position: 'fixed', inset: 0, zIndex: 40 }} />
      <div style=${{ position: 'absolute', right: 0, top: '100%', marginTop: 4, zIndex: 41, minWidth: 150,
        background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 8, boxShadow: 'var(--shadow-raised)', overflow: 'hidden', padding: 4 }}>
        ${items.map((it, i) => html`<div key=${i} onClick=${() => { setOpen(false); it.onClick(); }}
          style=${{ display: 'flex', alignItems: 'center', gap: 9, padding: '7px 10px', borderRadius: 5, cursor: 'pointer', fontSize: 13, color: it.danger ? 'var(--burgundy-700)' : 'var(--ink)' }}
          onMouseEnter=${(e) => { e.currentTarget.style.background = 'var(--paper-deep)'; }}
          onMouseLeave=${(e) => { e.currentTarget.style.background = 'transparent'; }}>
          <${Icon} name=${it.icon} size=${13} /> ${it.label}
        </div>`)}
      </div>
    </div>`}
  </div>`;
}

function ModeToggle({ mode, onChange }) {
  const seg = (m, icon, label) => html`<span onClick=${() => onChange(m)}
    style=${{ padding: '4px 10px', borderRadius: 4, cursor: 'pointer', fontSize: 12, display: 'flex', alignItems: 'center', gap: 5,
      background: mode === m ? 'var(--paper-deep)' : 'transparent', color: mode === m ? 'var(--ink)' : 'var(--ink-muted)', fontWeight: mode === m ? 500 : 400 }}>
    <${Icon} name=${icon} size=${12} /> ${label}</span>`;
  return html`<div style=${{ display: 'flex', gap: 2, padding: 2, background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 6 }}>
    ${seg('read', 'eye', 'Read')}${seg('edit', 'edit', 'Edit')}
  </div>`;
}

function Provenance({ path }) {
  const segs = path.split('/');
  return html`<div class="ck-provenance">
    <${Icon} name="doc" size=${11} />
    <span>world${segs.map((s, i) => html`<span key=${i}><span class="ck-sep">/</span><span class=${i === segs.length - 1 ? 'ck-file' : ''}>${s}</span></span>`)}</span>
  </div>`;
}

function ReadView({ page, path, pages, links, schemas, atlasMaps, campaignId, onSave, onBroken, railHidden }) {
  const { fm, body } = splitDoc(page.content);
  const props = parseProps(fm);
  const role = (props.find((p) => p.key === 'role') || {}).values?.[0];
  const eyebrow = [kindLabel(page.kind), role].filter(Boolean).join(' · ');
  const prose = body.replace(/^\s*#\s+.*\n+/, ''); // title rendered above; drop the leading H1

  const meta = (pages || []).find((p) => p.path === path);
  const words = prose.trim() ? prose.trim().split(/\s+/).length : 0;
  const edited = meta?.modified ? new Date(meta.modified * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : null;
  const outline = useMemo(() => parseOutline(prose), [prose]);
  const scrollRef = useRef(null);
  const [railTab, setRailTab] = useState('info');
  const [railW, onRailResize] = useSidebarWidth('ck_page_rail_w', 300, { min: 240, max: 560, fromRight: true });

  return html`<div style=${{ flex: 1, display: 'flex', minWidth: 0, minHeight: 0 }}>
    <div ref=${scrollRef} style=${{ flex: 1, overflow: 'auto', background: 'var(--paper)', padding: '34px 0 64px', minWidth: 0 }}>
      <div style=${{ maxWidth: 680, margin: '0 auto', padding: '0 52px' }}>
        <${Provenance} path=${path} />
        <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--burgundy)', marginTop: 16 }}>${eyebrow}</div>
        <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 38, fontWeight: 500, letterSpacing: '-0.02em', lineHeight: 1.08, color: 'var(--ink)', marginTop: 6 }}>${page.title}</h1>
        <div style=${{ height: 1, background: 'var(--rule)', margin: '26px 0' }} />
        <${PageBody} text=${prose} pages=${pages} onBroken=${onBroken} />
      </div>
    </div>
    ${!railHidden && html`<aside style=${{ width: railW, flex: `0 0 ${railW}px`, position: 'relative', borderLeft: '1px solid var(--rule-soft)', background: 'var(--paper)', display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      <${ResizeHandle} side="left" onMouseDown=${onRailResize} />
      <div style=${{ display: 'flex', padding: '0 8px', borderBottom: '1px solid var(--rule-soft)' }}>
        <${RailTab} icon="book" label="Info" active=${railTab === 'info'} onClick=${() => setRailTab('info')} />
        <${RailTab} icon="feather" label="Chat" active=${railTab === 'chat'} onClick=${() => setRailTab('chat')} />
      </div>
      ${railTab === 'info'
        ? html`<div style=${{ flex: 1, overflow: 'auto', padding: 16, display: 'flex', flexDirection: 'column', gap: 14 }}>
            <${OutlineCard} outline=${outline} scrollRef=${scrollRef} />
            <${InfoboxCard} fm=${fm} kind=${page.kind} schemas=${schemas} pages=${pages} />
            <${SummaryCard} page=${page} onSave=${onSave} />
            <${TagsCard} tags=${meta?.tags} />
            <${OnMapCard} path=${path} maps=${atlasMaps} campaignId=${campaignId} />
            <${BacklinksCard} path=${path} pages=${pages} links=${links} />
            <div class="ck-rail-foot">${[edited && `Edited ${edited}`, `${words} words`].filter(Boolean).join(' · ')}</div>
          </div>`
        : html`<${RailChat} />`}
    </aside>`}
  </div>`;
}

function RailTab({ icon, label, active, onClick }) {
  return html`<button onClick=${onClick} style=${{
    flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 6, padding: '9px 8px', fontSize: 12.5,
    fontWeight: active ? 600 : 500, background: 'none', border: 'none', marginBottom: -1,
    borderBottom: `2px solid ${active ? 'var(--burgundy)' : 'transparent'}`,
    color: active ? 'var(--ink)' : 'var(--ink-muted)', cursor: 'pointer',
  }}>
    <${Icon} name=${icon} size=${13} /> ${label}
  </button>`;
}

// The Keeper, docked in the page rail's Chat tab — opens (or creates) a chat for
// this world on first view, then reuses the shared keeper conversation surface.
function RailChat() {
  const k = keeperState();
  useEffect(() => { if (!k.chatId) openPanel(); }, []);
  return html`<${Conversation} k=${k} />`;
}

export function PageScreen() {
  const store = useStore();
  const c = store.campaign;
  const path = store.route.params.path;

  const [page, setPage] = useState(null);
  const [missing, setMissing] = useState(false);
  const [mode, setMode] = useState('read');
  const [saveState, setSaveState] = useState('saved');
  const [railHidden, setRailHidden] = useState(() => loadFlag('ck_rail_hidden', false));
  const [zen, setZen] = useState(() => loadFlag('ck_zen', false)); // edit mode: hide the file tree too
  const [rev, setRev] = useState(0); // bumped on external reload to re-key the editor

  const toggleRail = () => setRailHidden((h) => { saveFlag('ck_rail_hidden', !h); return !h; });
  const toggleZen = () => setZen((z) => { saveFlag('ck_zen', !z); return !z; });

  const pageRef = useRef(null); pageRef.current = page;
  const saveRef = useRef(saveState); saveRef.current = saveState;

  useEffect(() => {
    if (!c) return;
    loadVaultTree(c.campaign_id); loadKindSchemas(c.campaign_id);
    if (c.vault_path && !(store.atlasMaps || []).length) loadAtlasMaps(c.campaign_id);
  }, [c?.campaign_id]);

  useEffect(() => {
    let cancelled = false;
    setPage(null); setMissing(false); setMode('read');
    readVaultPage(path)
      .then((p) => { if (!cancelled) { setPage(p); setSaveState('saved'); } })
      .catch(() => { if (!cancelled) setMissing(true); });
    return () => { cancelled = true; };
  }, [path, c?.campaign_id]);

  // External edits (Obsidian, Finder): refresh the tree, and reload the open
  // page unless the editor holds unsaved changes.
  useEffect(() => {
    if (!c?.campaign_id) return undefined;
    return watchVault(c.campaign_id, async () => {
      loadVaultTree(c.campaign_id);
      if (saveRef.current !== 'saved') return;
      try {
        const p = await readVaultPage(path);
        const cur = pageRef.current;
        if (cur && cur.content === p.content) return;
        setPage(p);
        setMissing(false);
        setRev((r) => r + 1);
      } catch (_) {
        if (!pageRef.current) return;
        setPage(null);
        setMissing(true);
      }
    });
  }, [path, c?.campaign_id]);

  if (!c) { navigate('library'); return null; }

  const pages = store.vaultPages || [];
  const folders = store.vaultFolders || [];
  const tree = buildTree(folders, pages);
  const act = makeVaultActions(c, folders, {
    afterDelete: () => navigate('codex', { id: c.campaign_id }),
    afterDeleteFolder: (folderPath) => {
      if (path && path.startsWith(`${folderPath}/`)) navigate('codex', { id: c.campaign_id });
    },
  });

  const openBroken = (name) => createVaultPage(name, 'lore', '').then((p) => navigate('page', { path: p.path })).catch(() => {});

  const crumbs = [
    { label: 'Worlds', onClick: () => navigate('library') },
    { label: c.name, onClick: () => openCampaign(c.campaign_id) },
    { label: 'Codex', onClick: () => navigate('codex', { id: c.campaign_id }) },
    (page && page.title) || path,
  ];

  if (missing) {
    return html`<${Shell}
      sidebar=${html`<${FileTree} campaign=${c} tree=${tree} active=${null} onOpen=${(p) => navigate('page', { path: p.path })} act=${act} />`}
      topbar=${html`<${Topbar} crumbs=${crumbs} />`} bodyStyle=${{ padding: 40 }}>
      <${Empty} icon="scroll" title="Page not found">
        <a onClick=${() => navigate('codex', { id: c.campaign_id })} style=${{ color: 'var(--burgundy)', cursor: 'pointer' }}>Back to the codex</a>.
      </${Empty}>
    </${Shell}>`;
  }

  const savedChip = mode === 'edit' && html`<span style=${{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 11.5, fontFamily: 'var(--font-mono)',
    color: saveState === 'saved' ? 'var(--moss)' : 'var(--ink-faint)' }}>
    <${Icon} name=${saveState === 'saved' ? 'check' : 'feather'} size=${12} />
    ${saveState === 'saving' ? 'Saving…' : saveState === 'dirty' ? 'Unsaved' : 'Saved to vault'}
  </span>`;

  const pageLeaf = page ? { path, title: page.title, kind: page.kind } : null;
  const topbar = html`<${Topbar} crumbs=${crumbs}
    right=${html`<div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
      ${savedChip}
      ${mode === 'read' && html`<button onClick=${toggleRail} title=${railHidden ? 'Show side panel' : 'Hide side panel'}
        style=${{ padding: '6px 8px', color: railHidden ? 'var(--ink-faint)' : 'var(--ink-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>
        <${Icon} name=${railHidden ? 'chev-l' : 'chev-r'} size=${14} />
      </button>`}
      ${mode === 'edit' && html`<button onClick=${toggleZen} title=${zen ? 'Leave zen mode' : 'Zen mode — hide the sidebar'}
        style=${{ padding: '6px 8px', color: zen ? 'var(--burgundy)' : 'var(--ink-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>
        <${Icon} name="sun" size=${14} />
      </button>`}
      <${ModeToggle} mode=${mode} onChange=${setMode} />
      ${pageLeaf && html`<${KebabMenu} items=${[
        { icon: 'edit', label: 'Rename', onClick: () => act.renamePage(pageLeaf) },
        { icon: 'folder', label: 'Move…', onClick: () => act.movePage(pageLeaf) },
        { icon: 'trash', label: 'Move to trash', danger: true, onClick: () => act.deletePage(pageLeaf) },
      ]} />`}
    </div>`} />`;

  const doSave = async (content) => { const updated = await saveVaultPage(path, content); setPage(updated); return updated; };

  return html`<${Shell}
    sidebar=${mode === 'edit' && zen ? null
      : html`<${FileTree} campaign=${c} tree=${tree} active=${(page && page.title) || null} onOpen=${(p) => navigate('page', { path: p.path })} act=${act} />`}
    topbar=${topbar} bodyStyle=${{ padding: 0 }}>
    <div style=${{ display: 'flex', height: '100%', minHeight: 0 }}>
      ${page === null
        ? html`<div style=${{ flex: 1, padding: 40, color: 'var(--ink-faint)', fontStyle: 'italic' }}>Loading…</div>`
        : mode === 'read'
          ? html`<${ReadView} page=${page} path=${path} pages=${pages} links=${(store.vaultLinks || {}).links} schemas=${store.kindSchemas} atlasMaps=${store.atlasMaps} campaignId=${c.campaign_id} onSave=${doSave} onBroken=${openBroken} railHidden=${railHidden} />`
          : html`<div style=${{ flex: 1, overflow: 'auto', background: 'var(--paper)', padding: '34px 0 64px', minWidth: 0 }}>
              <div style=${{ maxWidth: 720, margin: '0 auto', padding: '0 52px' }}>
                <${Provenance} path=${path} />
                <${CmEditor} key=${'cm:' + rev + ':' + path} content=${page.content} pages=${pages}
                  onSave=${doSave} onCreate=${(name) => createVaultPage(name, 'lore', '')} onState=${setSaveState} />
              </div>
            </div>`}
    </div>
  </${Shell}>`;
}
