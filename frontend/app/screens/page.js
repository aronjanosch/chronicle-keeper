// The Page — rendered reading view + a CodeMirror 6 source editor (Phase 7.5).
// Read mode renders the markdown with a tabbed right rail — Info (infobox, AI
// memory, tags, map), Links (outline, local graph, links to/from, relations),
// Chat. Edit mode mounts CM6 over the literal .md (frontmatter + body), with
// markdown highlight, [[ / #tag autocomplete, ⌘F search, and format shortcuts.
// Auto-saves to the vault (800ms). See cm.js.
import { html, useState, useEffect, useRef, useMemo, useCallback } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, useStore, openModal, setState } from '../core.js';
import { Shell, Topbar, useSidebarWidth, ResizeHandle } from '../shell.js';
import { Empty, Icon, PageBody, WikilinkHoverCard, splitDoc, joinDoc, parseProps, openContextMenu, useAsset, bannerAsset } from '../ui.js';
import { readVaultPage, saveVaultPage, openCampaign, loadVaultTree, loadKindSchemas, loadAtlasMaps, createVaultPage, watchVault, uploadVaultAsset, loadSnippets, loadRelations, loadSkills, copyText } from '../actions.js';
import { mountEditor, gotoHeading } from '../cm.js';
import { setEditorActive } from '../commands.js';
import { TabStrip, openPageEvt } from '../tabs.js';
import { FileTree, buildTree, makeVaultActions, iconForKind, KINDS, dirOf } from './codex.js';
import { colorForKind } from '../graph.js';
import { keeperState, openPanel, newChat, ModeSelect, Conversation } from '../keeperPanel.js';

function kindLabel(k) {
  return (KINDS.find((x) => x.value === k) || {}).label || k || 'Page';
}

// Frontmatter keys that never render as infobox fields (page-data-model-spec).
const RESERVED_KEYS = new Set(['kind', 'aliases', 'tags', 'summary', 'cssclasses', 'publish', 'permalink', 'image', 'cover']);

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
      onClick=${target ? (e) => { e.stopPropagation(); openPageEvt(target.path, e); } : null}>${label}</span>`;
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

// Per-tab UI memory (Phase 15C): read/edit mode and scroll offsets per
// world:path, session-only — the CM6 EditorState cache (cursor, undo) lives
// in cm.js under the same key.
const tabUi = new Map();
function tabUiGet(key) { return tabUi.get(key) || {}; }
function tabUiSet(key, patch) {
  const next = { ...tabUiGet(key), ...patch };
  tabUi.delete(key);
  tabUi.set(key, next);
  if (tabUi.size > 40) tabUi.delete(tabUi.keys().next().value);
}
function useScrollMemory(uiKey, field) {
  const ref = useRef(null);
  useEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = tabUiGet(uiKey)[field] || 0;
    return () => { if (el) tabUiSet(uiKey, { [field]: el.scrollTop }); };
  }, [uiKey]);
  return ref;
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
      ${tags.map((t, i) => {
        const tag = String(t).replace(/^#/, '');
        const menu = (e) => openContextMenu(e, [
          { label: 'Show tagged pages', icon: 'tag', onClick: () => navigate('codex', { tag }) },
          { label: 'Copy #tag', icon: 'copy', onClick: () => copyText(`#${tag}`, 'Tag copied') },
        ]);
        return html`<span class="ck-prop-tag mono" key=${i} style=${{ cursor: 'pointer' }}
          onClick=${() => navigate('codex', { tag })} onContextMenu=${menu}>#${tag}</span>`;
      })}
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

// Sticky in-page nav. Click jumps to the Nth heading — the read scroller in
// read mode, the live editor in edit mode (onGoto resolves the right target).
function OutlineCard({ outline, onGoto }) {
  if (!outline || outline.length < 2) return null;
  return html`<${RailCard} icon="scroll" title="Outline">
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 1 }}>
      ${outline.map((h, i) => html`<div key=${i} class="ck-rail-link"
        onClick=${() => onGoto(i)} style=${{ paddingLeft: (h.level - 1) * 12 }}>
        <span style=${{ fontSize: h.level === 1 ? 12.5 : 12, color: h.level === 1 ? 'var(--ink)' : 'var(--ink-soft)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${h.text}</span>
      </div>`)}
    </div>
  </${RailCard}>`;
}

// Phase 18C: containment hierarchy. `part_of: "[[Parent]]"` in a child's
// frontmatter is the only stored edge; the reverse ("contains") is derived
// from the index, so parents never accumulate a child list.
const CONTAINMENT_PRED = 'part_of';
function containmentChildren(path, relations) {
  return (relations || [])
    .filter((r) => r.target_path === path && r.predicate === CONTAINMENT_PRED)
    .map((r) => r.source_path);
}
// Ancestors root → … → immediate parent, following the first part_of at each
// level; cycle-guarded so a mis-set loop can't hang the breadcrumb.
function containmentAncestry(path, relations) {
  const chain = [];
  const seen = new Set([path]);
  let cur = path;
  while (cur) {
    const parent = (relations || []).find(
      (r) => r.source_path === cur && r.predicate === CONTAINMENT_PRED && r.target_path,
    )?.target_path;
    if (!parent || seen.has(parent)) break;
    seen.add(parent);
    chain.unshift(parent);
    cur = parent;
  }
  return chain;
}

function ContainsCard({ path, pages, relations }) {
  const children = containmentChildren(path, relations);
  if (!children.length) return null;
  const byPath = new Map((pages || []).map((p) => [p.path, p]));
  return html`<${RailCard} icon="folder" title="Contains" right=${String(children.length)}>
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 2 }}>
      ${children.map((src) => {
        const p = byPath.get(src);
        return html`<div class="ck-rail-link" key=${src} onClick=${(e) => openPageEvt(src, e)}>
          <${Icon} name=${iconForKind(p?.kind)} size=${12} className="ck-ink-muted" />
          <span>${p?.title || src}</span>
        </div>`;
      })}
    </div>
  </${RailCard}>`;
}

// Incoming typed relations grouped by predicate (Phase 9B): the other end of
// `member_of: "[[X]]"` — no manual upkeep of both directions. part_of is
// excluded here; it gets first-class breadcrumb + Contains treatment (18C).
function RelationsCard({ path, pages, relations }) {
  const incoming = (relations || []).filter((r) => r.target_path === path && r.predicate !== CONTAINMENT_PRED);
  if (!incoming.length) return null;
  const byPath = new Map((pages || []).map((p) => [p.path, p]));
  const groups = new Map();
  for (const r of incoming) {
    if (!groups.has(r.predicate)) groups.set(r.predicate, []);
    const g = groups.get(r.predicate);
    if (!g.includes(r.source_path)) g.push(r.source_path);
  }
  return html`<${RailCard} icon="backlink" title="Relations" right=${String(incoming.length)}>
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 9 }}>
      ${[...groups.entries()].map(([pred, sources]) => html`<div key=${pred}>
        <div class="ck-infobox-key" style=${{ marginBottom: 3 }}>${pred} ←</div>
        <div style=${{ display: 'flex', flexDirection: 'column', gap: 2 }}>
          ${sources.map((src) => {
            const p = byPath.get(src);
            return html`<div class="ck-rail-link" key=${src} onClick=${(e) => openPageEvt(src, e)}>
              <${Icon} name=${iconForKind(p?.kind)} size=${12} className="ck-ink-muted" />
              <span>${p?.title || src}</span>
            </div>`;
          })}
        </div>
      </div>`)}
    </div>
  </${RailCard}>`;
}

// 1-hop local graph (Phase 9D): radial SVG — center page, linked neighbors on
// a ring. No force sim needed at this size.
function LocalGraphCard({ path, pages, links, relations }) {
  const byPath = new Map((pages || []).map((p) => [p.path, p]));
  const seen = new Set();
  const neighbors = [];
  const add = (p, typed) => {
    if (!p || p === path || seen.has(p)) return;
    seen.add(p);
    neighbors.push({ path: p, typed });
  };
  for (const l of links || []) {
    if (l.source_path === path) add(l.target_path, false);
    else if (l.target_path === path) add(l.source_path, false);
  }
  for (const r of relations || []) {
    if (r.source_path === path) add(r.target_path, true);
    else if (r.target_path === path) add(r.source_path, true);
  }
  if (!neighbors.length) return null;
  const shown = neighbors.slice(0, 12);
  const W = 248, H = 170, cx = W / 2, cy = H / 2, R = 62;
  const me = byPath.get(path);
  return html`<${RailCard} icon="link" title="Local graph" right=${neighbors.length > 12 ? `12 of ${neighbors.length}` : null}>
    <svg viewBox=${`0 0 ${W} ${H}`} style=${{ width: '100%', display: 'block' }}>
      ${shown.map((n, i) => {
        const a = (i / shown.length) * 2 * Math.PI - Math.PI / 2;
        const x = cx + R * Math.cos(a), y = cy + R * Math.sin(a);
        const p = byPath.get(n.path);
        const label = (p?.title || n.path).slice(0, 14);
        return html`<g key=${n.path} onClick=${(e) => openPageEvt(n.path, e)} style=${{ cursor: 'pointer' }}>
          <line x1=${cx} y1=${cy} x2=${x} y2=${y}
            stroke=${n.typed ? 'rgba(122,46,31,.5)' : 'rgba(31,24,19,.16)'} stroke-width=${n.typed ? 1.4 : 1} />
          <circle cx=${x} cy=${y} r="4.5" fill=${colorForKind(p?.kind)} />
          <text x=${x} y=${y + (y >= cy ? 14 : -8)} text-anchor="middle"
            style=${{ fontSize: 8.5, fontFamily: 'var(--font-mono)', fill: 'var(--ink-soft)' }}>${label}</text>
        </g>`;
      })}
      <circle cx=${cx} cy=${cy} r="6.5" fill=${colorForKind(me?.kind)} stroke="var(--burgundy-700)" stroke-width="1.5" />
    </svg>
  </${RailCard}>`;
}

// Shared list body for "Linked from" / "Links to".
function LinkListCard({ title, paths, pages }) {
  if (!paths.length) return null;
  const byPath = new Map((pages || []).map((p) => [p.path, p]));
  const [hover, setHover] = useState(null);
  const onMouseOver = useCallback((e) => {
    const el = e.target?.closest?.('[data-path]');
    if (!el) { setHover(null); return; }
    const path = el.getAttribute('data-path');
    setHover((h) => (h?.path === path ? h : { path, x: e.clientX, y: e.clientY }));
  }, []);
  return html`<${RailCard} icon="link" title=${title} right=${String(paths.length)}>
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 2 }}
      onMouseOver=${onMouseOver} onMouseLeave=${() => setHover(null)}>
      ${paths.map((pp) => {
        const p = byPath.get(pp);
        return html`<div class="ck-rail-link" key=${pp} data-path=${pp} onClick=${(e) => openPageEvt(pp, e)}>
          <${Icon} name=${iconForKind(p?.kind)} size=${12} className="ck-ink-muted" />
          <span>${p?.title || pp}</span>
        </div>`;
      })}
    </div>
    ${hover && html`<${WikilinkHoverCard} path=${hover.path} pages=${pages} x=${hover.x} y=${hover.y} />`}
  </${RailCard}>`;
}

function BacklinksCard({ path, pages, links }) {
  const sources = [...new Set((links || [])
    .filter((l) => l.target_path === path)
    .map((l) => l.source_path))];
  return html`<${LinkListCard} title="Linked from" paths=${sources} pages=${pages} />`;
}

function OutgoingLinksCard({ path, pages, links }) {
  const targets = [...new Set((links || [])
    .filter((l) => l.source_path === path && l.target_path)
    .map((l) => l.target_path))];
  return html`<${LinkListCard} title="Links to" paths=${targets} pages=${pages} />`;
}

function autoGrow(ta) {
  ta.style.height = 'auto';
  ta.style.height = ta.scrollHeight + 'px';
}

// CodeMirror 6 source editor — edits the whole .md (frontmatter + body) as literal
// text. The view itself is an app-wide singleton (cm.js); this component just
// re-parents it and swaps in this tab's cached EditorState (keyed by uiKey).
function CmEditor({ content, uiKey, pages, snippets, skills, onSave, onCreate, onState, onExtract, onQuote, onAskKeeper }) {
  const hostRef = useRef(null);
  const pagesRef = useRef(pages); pagesRef.current = pages;
  const snippetsRef = useRef(snippets); snippetsRef.current = snippets;
  const skillsRef = useRef(skills); skillsRef.current = skills;
  useEffect(() => {
    let ctl = null, dead = false;
    mountEditor(hostRef.current, {
      doc: content,
      cacheKey: uiKey,
      getPages: () => pagesRef.current,
      getSnippets: () => snippetsRef.current,
      getSkills: () => skillsRef.current,
      onCreatePage: (name, kind) => onCreate(name, kind),
      onUploadAsset: uploadVaultAsset,
      onSave,
      onState,
      onExtract,
      onQuote,
      onAskKeeper,
    }).then((c) => { if (dead) c.destroy(); else ctl = c; });
    setEditorActive(true);
    return () => { dead = true; if (ctl) ctl.destroy(); setEditorActive(false); };
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

// ⌘F in read mode (edit mode's find is CM's search panel). Marks every match
// in the rendered page via the CSS Custom Highlight API and steps through
// them; without the API (old WebKitGTK) it still scrolls match to match.
function FindBar({ scrollRef, doc, focusTick, onClose }) {
  const [q, setQ] = useState(() => (window.getSelection()?.toString() || '').split('\n')[0].trim());
  const [hit, setHit] = useState({ idx: 0, total: 0 });
  const inputRef = useRef(null);
  const rangesRef = useRef([]);

  useEffect(() => { inputRef.current?.focus(); inputRef.current?.select(); }, [focusTick]);

  const paint = (ranges, cur) => {
    if (!window.Highlight || !CSS.highlights) return;
    CSS.highlights.set('ck-find', new Highlight(...ranges));
    CSS.highlights.set('ck-find-cur', new Highlight(...(ranges[cur] ? [ranges[cur]] : [])));
  };

  const goTo = (i) => {
    const ranges = rangesRef.current;
    const r = ranges[i];
    paint(ranges, i);
    setHit({ idx: i, total: ranges.length });
    const sc = scrollRef.current;
    if (!r || !sc) return;
    const rect = r.getBoundingClientRect();
    sc.scrollTop += rect.top - sc.getBoundingClientRect().top - sc.clientHeight / 3;
  };

  useEffect(() => {
    const sc = scrollRef.current;
    const needle = q.toLowerCase();
    const ranges = [];
    if (sc && needle) {
      const walker = document.createTreeWalker(sc, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const hay = node.nodeValue.toLowerCase();
        for (let at = hay.indexOf(needle); at !== -1; at = hay.indexOf(needle, at + needle.length)) {
          const r = document.createRange();
          r.setStart(node, at);
          r.setEnd(node, at + needle.length);
          ranges.push(r);
        }
      }
    }
    rangesRef.current = ranges;
    goTo(0);
  }, [q, doc]);

  useEffect(() => () => { CSS.highlights?.delete('ck-find'); CSS.highlights?.delete('ck-find-cur'); }, []);

  const step = (dir) => {
    const n = rangesRef.current.length;
    if (n) goTo((hit.idx + dir + n) % n);
  };
  const onKey = (e) => {
    if (e.key === 'Enter') { e.preventDefault(); step(e.shiftKey ? -1 : 1); }
    else if (e.key === 'Escape') { e.preventDefault(); onClose(); }
  };

  return html`<div class="ck-findbar">
    <${Icon} name="search" size=${13} className="ck-ink-muted" />
    <input ref=${inputRef} value=${q} placeholder="Find in page" spellcheck="false"
      onInput=${(e) => setQ(e.target.value)} onKeyDown=${onKey} />
    <span class="ck-findbar-count">${q ? `${hit.total ? hit.idx + 1 : 0}/${hit.total}` : ''}</span>
    <button onClick=${() => step(-1)} title="Previous (⇧↵)"><${Icon} name="chev-u" size=${13} /></button>
    <button onClick=${() => step(1)} title="Next (↵)"><${Icon} name="chev-d" size=${13} /></button>
    <button onClick=${onClose} title="Close (esc)"><${Icon} name="x" size=${13} /></button>
  </div>`;
}

function ReadView({ page, path, pages, campaignId, onBroken, scrollRef }) {
  const { fm, body } = splitDoc(page.content);
  const props = parseProps(fm);
  const role = (props.find((p) => p.key === 'role') || {}).values?.[0];
  const eyebrow = [kindLabel(page.kind), role].filter(Boolean).join(' · ');
  const prose = body.replace(/^\s*#\s+.*\n+/, ''); // title rendered above; drop the leading H1

  // ⌘F / Edit → Find in Page. The tick refocuses (and reselects) the input
  // when the bar is already open. (Edit mode uses CM's own search panel.)
  const [findTick, setFindTick] = useState(0);
  useEffect(() => {
    const onCmd = (e) => { if (e.detail === 'find') setFindTick((t) => t + 1); };
    window.addEventListener('ck:cmd', onCmd);
    return () => window.removeEventListener('ck:cmd', onCmd);
  }, []);

  const bannerUrl = useAsset(campaignId, bannerAsset(props));

  return html`<div style=${{ flex: 1, position: 'relative', display: 'flex', minWidth: 0, minHeight: 0 }}>
    ${findTick > 0 && html`<${FindBar} scrollRef=${scrollRef} doc=${page.content} focusTick=${findTick} onClose=${() => setFindTick(0)} />`}
    <div ref=${scrollRef} style=${{ flex: 1, overflow: 'auto', background: 'var(--paper)', padding: '0 0 64px', minWidth: 0 }}>
      ${bannerUrl && html`<img class="ck-page-banner" src=${bannerUrl} alt="" />`}
      <div style=${{ maxWidth: 680, margin: '0 auto', padding: `${bannerUrl ? 24 : 34}px 52px 0` }}>
        <${Provenance} path=${path} />
        <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--burgundy)', marginTop: 16 }}>${eyebrow}</div>
        <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 38, fontWeight: 500, letterSpacing: '-0.02em', lineHeight: 1.08, color: 'var(--ink)', marginTop: 6 }}>${page.title}</h1>
        <div style=${{ height: 1, background: 'var(--rule)', margin: '26px 0' }} />
        <${PageBody} text=${prose} pages=${pages} onBroken=${onBroken} />
      </div>
    </div>
  </div>`;
}

// The page rail — Info / Links / Chat. Lifted out of ReadView so it persists
// across read↔edit (people want the Keeper chat + backlinks while writing too).
function PageRail({ page, path, pages, links, relations, schemas, atlasMaps, campaignId, onSave, railTab, setRailTab, railW, onRailResize, onGotoHeading }) {
  const { fm, body } = splitDoc(page.content);
  const prose = body.replace(/^\s*#\s+.*\n+/, '');
  const outline = useMemo(() => parseOutline(prose), [prose]);
  const meta = (pages || []).find((p) => p.path === path);
  const words = prose.trim() ? prose.trim().split(/\s+/).length : 0;
  const edited = meta?.modified ? new Date(meta.modified * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : null;

  return html`<aside style=${{ width: railW, flex: `0 0 ${railW}px`, position: 'relative', borderLeft: '1px solid var(--rule-soft)', background: 'var(--paper)', display: 'flex', flexDirection: 'column', minHeight: 0 }}>
    <${ResizeHandle} side="left" onMouseDown=${onRailResize} />
    <div style=${{ display: 'flex', padding: '0 8px', borderBottom: '1px solid var(--rule-soft)' }}>
      <${RailTab} icon="book" label="Info" active=${railTab === 'info'} onClick=${() => setRailTab('info')} />
      <${RailTab} icon="link" label="Links" active=${railTab === 'links'} onClick=${() => setRailTab('links')} />
      <${RailTab} icon="feather" label="Chat" active=${railTab === 'chat'} onClick=${() => setRailTab('chat')} />
    </div>
    ${railTab === 'info'
      ? html`<div style=${{ flex: 1, overflow: 'auto', padding: 16, display: 'flex', flexDirection: 'column', gap: 14 }}>
          <${InfoboxCard} fm=${fm} kind=${page.kind} schemas=${schemas} pages=${pages} />
          <${SummaryCard} page=${page} onSave=${onSave} />
          <${TagsCard} tags=${meta?.tags} />
          <${ContainsCard} path=${path} pages=${pages} relations=${relations} />
          <${OnMapCard} path=${path} maps=${atlasMaps} campaignId=${campaignId} />
          <div class="ck-rail-foot">${[edited && `Edited ${edited}`, `${words} words`].filter(Boolean).join(' · ')}</div>
        </div>`
      : railTab === 'links'
      ? html`<div style=${{ flex: 1, overflow: 'auto', padding: 16, display: 'flex', flexDirection: 'column', gap: 14 }}>
          <${OutlineCard} outline=${outline} onGoto=${onGotoHeading} />
          <${LocalGraphCard} path=${path} pages=${pages} links=${links} relations=${relations} />
          <${OutgoingLinksCard} path=${path} pages=${pages} links=${links} />
          <${BacklinksCard} path=${path} pages=${pages} links=${links} />
          <${RelationsCard} path=${path} pages=${pages} relations=${relations} />
        </div>`
      : html`<${RailChat} />`}
  </aside>`;
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
  return html`<div style=${{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 4, padding: '5px 8px', borderBottom: '1px solid var(--rule-soft)' }}>
      <span style=${{ flex: 1, minWidth: 0 }}><${ModeSelect} mode=${k.mode} /></span>
      <button title="New chat" class="btn btn-icon" onClick=${() => newChat()}><${Icon} name="plus" size=${15} /></button>
      <button title="Open the Keeper full-screen" class="btn btn-icon" onClick=${() => navigate('keeper', { id: k.campaignId })}><${Icon} name="export" size=${14} /></button>
    </div>
    <${Conversation} k=${k} />
  </div>`;
}

// Edit-mode scroller with per-tab scroll memory (read mode's lives in ReadView).
function EditScroll({ uiKey, children }) {
  const ref = useScrollMemory(uiKey, 'editScroll');
  return html`<div ref=${ref} style=${{ flex: 1, overflow: 'auto', background: 'var(--paper)', padding: '34px 0 64px', minWidth: 0 }}>
    ${children}
  </div>`;
}

export function PageScreen() {
  const store = useStore();
  const c = store.campaign;
  const path = store.route.params.path;
  const uiKey = `${c?.campaign_id}:${path}`;

  const [page, setPage] = useState(null);
  const [missing, setMissing] = useState(false);
  const [mode, setMode] = useState('read');
  const [saveState, setSaveState] = useState('saved');
  const [railHidden, setRailHidden] = useState(() => loadFlag('ck_rail_hidden', false));
  const [zen, setZen] = useState(() => loadFlag('ck_zen', false)); // edit mode: hide the file tree too
  const [rev, setRev] = useState(0); // bumped on external reload to re-key the editor

  // Rail (Info/Links/Chat) lives at screen level so it survives read↔edit.
  // navigate('page', { path, rail }) (e.g. "Ask Keeper") opens the requested tab.
  const routeRail = store.route.params?.rail;
  const [railTab, setRailTab] = useState(routeRail || 'info');
  useEffect(() => { if (routeRail) setRailTab(routeRail); }, [routeRail]);
  const [railW, onRailResize] = useSidebarWidth('ck_page_rail_w', 300, { min: 240, max: 560, fromRight: true });
  const readScrollRef = useScrollMemory(uiKey, 'readScroll');

  const toggleRail = () => setRailHidden((h) => { saveFlag('ck_rail_hidden', !h); return !h; });
  const toggleZen = () => setZen((z) => { saveFlag('ck_zen', !z); return !z; });
  const showRail = (tab) => { setRailHidden(false); saveFlag('ck_rail_hidden', false); setRailTab(tab); };

  // Outline jump: the read scroller in read mode, the live editor in edit mode.
  const onGotoHeading = (i) => {
    if (mode === 'read') {
      const hs = readScrollRef.current?.querySelectorAll('.ck-prose h1, .ck-prose h2, .ck-prose h3');
      if (hs && hs[i]) hs[i].scrollIntoView({ behavior: 'smooth', block: 'start' });
    } else gotoHeading(i);
  };

  // 14E: ⌘⇧K / View-menu commands for the toggles this screen owns.
  useEffect(() => {
    const onCmd = (e) => {
      if (e.detail === 'toggle-rail') toggleRail();
      else if (e.detail === 'zen' && mode === 'edit') toggleZen();
    };
    window.addEventListener('ck:cmd', onCmd);
    return () => window.removeEventListener('ck:cmd', onCmd);
  }, [mode]);

  const pageRef = useRef(null); pageRef.current = page;
  const saveRef = useRef(saveState); saveRef.current = saveState;

  useEffect(() => {
    if (!c) return;
    loadVaultTree(c.campaign_id); loadKindSchemas(c.campaign_id); loadRelations(c.campaign_id);
    loadSkills(c.campaign_id);
    if (!(store.snippets || []).length) loadSnippets(c.campaign_id);
    if (c.vault_path && !(store.atlasMaps || []).length) loadAtlasMaps(c.campaign_id);
  }, [c?.campaign_id]);

  // Restore this tab's last mode (15C); first visit lands in read mode.
  const changeMode = (m) => { tabUiSet(uiKey, { mode: m }); setMode(m); };

  useEffect(() => {
    let cancelled = false;
    setPage(null); setMissing(false); setMode(tabUiGet(uiKey).mode || 'read');
    readVaultPage(path)
      .then((p) => { if (!cancelled) { setPage(p); setSaveState('saved'); } })
      .catch(() => { if (!cancelled) setMissing(true); });
    return () => { cancelled = true; };
  }, [path, c?.campaign_id]);

  // External edits (Obsidian, Finder) and Keeper writes: reload the open page
  // unless the editor holds unsaved changes.
  const refreshFromDisk = async () => {
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
  };
  useEffect(() => {
    if (!c?.campaign_id) return undefined;
    return watchVault(c.campaign_id, () => {
      loadVaultTree(c.campaign_id);
      loadRelations(c.campaign_id);
      refreshFromDisk();
    });
  }, [path, c?.campaign_id]);
  useEffect(() => { if (store.dirty_vault) refreshFromDisk(); }, [store.dirty_vault]);

  if (!c) { navigate('library'); return null; }

  const pages = store.vaultPages || [];
  const folders = store.vaultFolders || [];
  const tree = buildTree(folders, pages);
  // ASK-KEEPER `/` rows: skills whose kinds: match this page (zero inference).
  const pageKind = (page?.kind || '').toLowerCase();
  const editorSkills = pageKind
    ? (store.keeperSkills || []).filter((s) => (s.kinds || []).some((x) => String(x).toLowerCase() === pageKind))
    : [];
  const act = makeVaultActions(c, folders, {
    afterDelete: () => navigate('codex', { id: c.campaign_id }),
    afterDeleteFolder: (folderPath) => {
      if (path && path.startsWith(`${folderPath}/`)) navigate('codex', { id: c.campaign_id });
    },
  });

  const openBroken = (name) => createVaultPage(name, 'lore', '').then((p) => navigate('page', { path: p.path })).catch(() => {});

  const byPath = new Map(pages.map((p) => [p.path, p]));
  const ancestry = containmentAncestry(path, store.vaultRelations);
  const crumbs = [
    { label: 'Worlds', onClick: () => navigate('library') },
    { label: c.name, onClick: () => openCampaign(c.campaign_id) },
    { label: 'Codex', onClick: () => navigate('codex', { id: c.campaign_id }) },
    ...ancestry.map((ap) => ({ label: byPath.get(ap)?.title || ap, onClick: () => navigate('page', { path: ap }) })),
    (page && page.title) || path,
  ];

  if (missing) {
    return html`<${Shell}
      sidebar=${html`<${FileTree} campaign=${c} tree=${tree} active=${null} onOpen=${(p, e) => openPageEvt(p.path, e)} act=${act} />`}
      topbar=${html`<${Topbar} crumbs=${crumbs} />`} tabstrip=${html`<${TabStrip} />`} bodyStyle=${{ padding: 40 }}>
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
      <button onClick=${toggleRail} title=${railHidden ? 'Show side panel' : 'Hide side panel'}
        style=${{ padding: '6px 8px', color: railHidden ? 'var(--ink-faint)' : 'var(--ink-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>
        <${Icon} name=${railHidden ? 'chev-l' : 'chev-r'} size=${14} />
      </button>
      ${mode === 'edit' && html`<button onClick=${toggleZen} title=${zen ? 'Leave zen mode' : 'Zen mode — hide the sidebar'}
        style=${{ padding: '6px 8px', color: zen ? 'var(--burgundy)' : 'var(--ink-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>
        <${Icon} name="sun" size=${14} />
      </button>`}
      <${ModeToggle} mode=${mode} onChange=${changeMode} />
      ${pageLeaf && html`<${KebabMenu} items=${[
        { icon: 'edit', label: 'Rename', onClick: () => act.renamePage(pageLeaf) },
        { icon: 'folder', label: 'Move…', onClick: () => act.movePage(pageLeaf) },
        { icon: 'sparkle', label: 'Promote to kind…', onClick: () => act.promotePage(pageLeaf) },
        { icon: 'time', label: 'History', onClick: () => openModal('pageHistory', { path, onRestored: () => readVaultPage(path).then(setPage).catch(() => {}) }) },
        { icon: 'trash', label: 'Move to trash', danger: true, onClick: () => act.deletePage(pageLeaf) },
      ]} />`}
    </div>`} />`;

  const doSave = async (content) => { const updated = await saveVaultPage(path, content); setPage(updated); return updated; };

  // 14B: selection → its own page next to this one; resolves on create so the
  // editor can drop a [[link]] in place (never resolves on cancel).
  const extractToPage = (text) => new Promise((resolve) => {
    openModal('textPrompt', {
      title: 'Extract to new page', label: 'New page title',
      initial: (text.trim().split('\n')[0] || '').replace(/^[#>*\-\s[\]!]+/, '').slice(0, 60),
      confirmLabel: 'Create page',
      onSubmit: async (title) => {
        const created = await createVaultPage(title, page?.kind || 'lore', dirOf(path));
        const body = `${created.content.replace(/\n+$/, '')}\n\n${text.trim()}\n`;
        resolve(await saveVaultPage(created.path, body));
      },
    });
  });

  // 14B: selection → Keeper composer as a quote, in the Chat rail tab (the rail
  // now rides along in edit mode, so no mode switch needed).
  const quoteToKeeper = (text) => {
    const draft = `${text.trim().split('\n').map((l) => `> ${l}`).join('\n')}\n\n`;
    setState({ keeper: {
      chatId: null, events: [], attachments: [], live: null, error: null,
      ...(store.keeper || {}), open: true, draft,
    } });
    showRail('chat');
  };

  // 20A: editor `/` ASK-KEEPER rows. A skill row pulls that skill; `/keeper …`
  // is an ad-hoc prompt. Surface the Chat rail and prefill the composer —
  // pull-not-push, nothing fires until the user sends.
  const askKeeper = ({ skill, prompt, selection }) => {
    let draft = skill
      ? `Use the "${skill.name}" skill to help me develop this page.`
      : (prompt || '');
    const sel = (selection || '').trim();
    if (sel) {
      const quoted = sel.split('\n').map((l) => `> ${l}`).join('\n');
      draft = draft ? `${quoted}\n\n${draft}` : `${quoted}\n\n`;
    }
    setState({ keeper: {
      chatId: null, events: [], attachments: [], live: null, error: null,
      ...(store.keeper || {}), open: true, draft,
    } });
    showRail('chat');
  };

  return html`<${Shell}
    sidebar=${mode === 'edit' && zen ? null
      : html`<${FileTree} campaign=${c} tree=${tree} active=${(page && page.title) || null} onOpen=${(p, e) => openPageEvt(p.path, e)} act=${act} />`}
    topbar=${topbar} tabstrip=${html`<${TabStrip} />`} bodyStyle=${{ padding: 0 }}>
    <div style=${{ display: 'flex', height: '100%', minHeight: 0 }}>
      <div style=${{ flex: 1, display: 'flex', minWidth: 0, minHeight: 0 }}>
        ${page === null
          ? html`<div style=${{ flex: 1, padding: 40, color: 'var(--ink-faint)', fontStyle: 'italic' }}>Loading…</div>`
          : mode === 'read'
            ? html`<${ReadView} page=${page} path=${path} pages=${pages} campaignId=${c.campaign_id} onBroken=${openBroken} scrollRef=${readScrollRef} />`
            : html`<${EditScroll} uiKey=${uiKey}>
                <div style=${{ maxWidth: 720, margin: '0 auto', padding: '0 52px' }}>
                  <${Provenance} path=${path} />
                  <${CmEditor} key=${'cm:' + rev + ':' + path} content=${page.content} uiKey=${uiKey} pages=${pages} snippets=${store.snippets} skills=${editorSkills}
                    onSave=${doSave} onCreate=${(name, kind) => createVaultPage(name, kind || 'lore', '')} onState=${setSaveState}
                    onExtract=${extractToPage} onQuote=${quoteToKeeper} onAskKeeper=${askKeeper} />
                </div>
              </${EditScroll}>`}
      </div>
      ${page && !railHidden && html`<${PageRail} page=${page} path=${path} pages=${pages}
        links=${(store.vaultLinks || {}).links} relations=${store.vaultRelations} schemas=${store.kindSchemas}
        atlasMaps=${store.atlasMaps} campaignId=${c.campaign_id} onSave=${doSave}
        railTab=${railTab} setRailTab=${setRailTab} railW=${railW} onRailResize=${onRailResize} onGotoHeading=${onGotoHeading} />`}
    </div>
  </${Shell}>`;
}
