// The Atlas — explorable, pannable, zoomable map images, ported from the
// design handoff (Map.html). The map is your own art (Inkarnate, Azgaar, a
// scan); pins sit on top in normalised coordinates. Every pin owns a codex
// page: hover for a preview, click to read it in the side panel. A pin whose
// entry is also a map wears a badge + pulsing ring — "Enter the map" (or
// double-click) descends, with a breadcrumb trail back up. Drop new pins from
// the palette to mint codex pages. Maps persist as <world>/Atlas/<id>.json
// with the art copied alongside; pages stay files-as-truth.
import { html, useState, useEffect, useRef, useMemo, useCallback } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, apiFetch, apiBlob, store, setState } from '../core.js';
import { loadAtlasMaps, createAtlasMap, saveAtlasMap, replaceAtlasMapArt, deleteAtlasMap, pickMapImage, loadVaultTree, loadVaultLinks, createVaultPage, openCampaign } from '../actions.js';
import { Shell, Sidebar, Topbar, useSidebarWidth, ResizeHandle } from '../shell.js';
import { Icon, Btn, Empty, Spinner, PageBody, Input, Select, splitDoc, parseProps, openContextMenu } from '../ui.js';

const clamp = (v, a, b) => Math.min(b, Math.max(a, v));

// ── pin kinds: wax-seal tone + engraved glyph per codex kind ──────
export const PIN_KINDS = {
  place:   { ic: 'castle', tone: 'moss',     label: 'Place',   folder: 'Places' },
  npc:     { ic: 'users',  tone: 'burgundy', label: 'Figure',  folder: 'NPCs' },
  faction: { ic: 'shield', tone: 'inkBlue',  label: 'Faction', folder: 'Factions' },
  item:    { ic: 'gem',    tone: 'gilt',     label: 'Relic',   folder: 'Items' },
  lore:    { ic: 'flame',  tone: 'ochre',    label: 'Lore',    folder: 'Lore' },
  pc:      { ic: 'sword',  tone: 'burgundy', label: 'Party',   folder: 'Party' },
};
const PALETTE = ['place', 'npc', 'faction', 'item', 'lore', 'pc'];

// view preferences (per-user, not map data)
const PIN_SCALES = { small: 0.72, medium: 1, large: 1.35 };
const readPref = (k, d) => { try { return localStorage.getItem(k) || d; } catch (_) { return d; } };
const writePref = (k, v) => { try { localStorage.setItem(k, v); } catch (_) {} };

// seal colour ramp per tone: dark rim, body, light sheen, engrave
const SEAL = {
  burgundy: { dark: '#4E1C12', body: '#7A2E1F', light: '#A8493A', eng: '#F0D2CA' },
  moss:     { dark: '#2F3D25', body: '#4A5D3A', light: '#6E8356', eng: '#E0E8D2' },
  inkBlue:  { dark: '#23384C', body: '#355370', light: '#577291', eng: '#DCE6F0' },
  ochre:    { dark: '#6E4A14', body: '#A87328', light: '#C89446', eng: '#F4E6C6' },
  gilt:     { dark: '#7A5E22', body: '#B8924A', light: '#D8B468', eng: '#3A2C0E' },
};
const SURFACE = '#FBF6E9', RULE = '#DDD0AE', INK = '#1F1813';

// ── pins: wax-seal medallions pressed onto the art ────────────────
export function SealHead({ kind, size = 38, selected }) {
  const k = PIN_KINDS[kind] || PIN_KINDS.npc;
  const s = SEAL[k.tone] || SEAL.burgundy;
  return html`<div style=${{
    width: size, height: size, borderRadius: '50%', position: 'relative',
    background: `radial-gradient(circle at 34% 28%, ${s.light}, ${s.body} 56%, ${s.dark})`,
    boxShadow: selected
      ? `0 0 0 3px ${SURFACE}, 0 0 0 5px ${s.body}, 0 6px 14px rgba(40,20,8,.5)`
      : 'inset 0 1px 1px rgba(255,255,255,.28), inset 0 -2px 3px rgba(0,0,0,.35), 0 3px 7px rgba(40,20,8,.4)',
    border: `1px solid ${s.dark}`,
    display: 'flex', alignItems: 'center', justifyContent: 'center',
    transition: 'box-shadow .15s, transform .12s', flex: '0 0 auto',
  }}>
    <div style=${{ position: 'absolute', inset: size * 0.14, borderRadius: '50%', border: `1px solid ${s.eng}`, opacity: 0.4 }} />
    <span style=${{ color: s.eng, display: 'flex', filter: 'drop-shadow(0 1px 0 rgba(0,0,0,.3))' }}>
      <${Icon} name=${k.ic} size=${Math.round(size * 0.46)} />
    </span>
  </div>`;
}

// full map marker: ground shadow + seal + tip + label.
// Counter-scales by 1/zoom so pins stay legible at any zoom.
function PinMarker({ pin, invZoom, scale = 1, showLabel = true, selected, hasMap, onHover, onLeave, onClick, onDoubleClick, onMouseDown, onMenu }) {
  const size = pin.kind === 'pc' ? 32 : 38;
  const k = PIN_KINDS[pin.kind] || PIN_KINDS.npc;
  const s = SEAL[k.tone] || SEAL.burgundy;
  return html`<div style=${{ position: 'absolute', left: `${pin.x * 100}%`, top: `${pin.y * 100}%`, zIndex: selected ? 40 : (hasMap ? 30 : 20) }}>
    <div onMouseEnter=${onHover} onMouseLeave=${onLeave} onClick=${onClick} onDblClick=${onDoubleClick} onMouseDown=${onMouseDown} onContextMenu=${onMenu}
      style=${{
        position: 'absolute', left: 0, bottom: 0,
        transform: `translateX(-50%) scale(${invZoom * scale})`, transformOrigin: 'bottom center',
        cursor: 'pointer', display: 'flex', flexDirection: 'column', alignItems: 'center',
        willChange: 'transform',
      }}>
      <div style=${{ position: 'relative', transition: 'transform .12s', transform: selected ? 'translateY(-2px)' : 'none' }}>
        ${hasMap && html`<span style=${{ position: 'absolute', inset: -5, borderRadius: '50%', border: `1.5px solid ${s.body}`, opacity: 0.5, animation: 'ck-ping 2.8s ease-out infinite', pointerEvents: 'none' }} />`}
        <${SealHead} kind=${pin.kind} size=${size} selected=${selected} />
        ${hasMap && html`<span style=${{ position: 'absolute', right: -5, bottom: -4, width: 17, height: 18, display: 'flex', alignItems: 'center', justifyContent: 'center',
          background: SURFACE, borderRadius: 4, boxShadow: '0 1px 3px rgba(40,20,8,.35)' }}>
          <span style=${{ width: 13, height: 14, background: s.body, clipPath: 'polygon(50% 0,100% 27%,100% 73%,50% 100%,0 73%,0 27%)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <svg width="7" height="7" viewBox="0 0 16 16" fill="none" stroke=${s.eng} stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 3l6 5-6 5"/></svg>
          </span>
        </span>`}
      </div>
      <svg width="14" height="12" viewBox="0 0 14 12" style=${{ marginTop: -2 }}>
        <path d="M7 12 L1.5 1 Q7 4 12.5 1 Z" fill=${s.dark} />
      </svg>
      <div style=${{ width: size * 0.7, height: 4, borderRadius: '50%', marginTop: -1,
        background: 'radial-gradient(circle, rgba(40,20,8,.34), transparent 70%)' }} />
    </div>

    ${showLabel && html`<div style=${{ position: 'absolute', left: 0, top: 6,
      transform: `translateX(-50%) scale(${invZoom * scale})`, transformOrigin: 'top center', pointerEvents: 'none' }}>
      <div style=${{
        whiteSpace: 'nowrap', fontFamily: 'var(--font-display)', fontSize: 12.5, fontWeight: 500,
        color: INK, padding: '1px 7px', borderRadius: 4,
        background: 'rgba(251,246,233,.78)', border: `1px solid ${RULE}`,
        boxShadow: '0 1px 2px rgba(60,40,10,.12)', textShadow: '0 1px 0 rgba(255,255,255,.5)',
      }}>${pin.name}${hasMap && html`<span style=${{ marginLeft: 5, color: s.body, fontWeight: 700 }}>›</span>`}</div>
    </div>`}
  </div>`;
}

// floating preview shown on hover, attached to the pin in image space
function HoverCard({ pin, invZoom, scale = 1, summary }) {
  const k = PIN_KINDS[pin.kind] || {};
  const hasMap = !!pin.to;
  return html`<div style=${{ position: 'absolute', left: `${pin.x * 100}%`, top: `${pin.y * 100}%`, zIndex: 70, pointerEvents: 'none' }}>
    <div style=${{ position: 'absolute', left: 0, bottom: Math.round(46 * scale), transform: `translateX(-50%) scale(${invZoom})`, transformOrigin: 'bottom center' }}>
      <div style=${{ width: 236, background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 8, boxShadow: '0 10px 26px rgba(60,40,10,.24)', overflow: 'hidden' }}>
        <div style=${{ padding: '10px 12px', display: 'flex', gap: 9, alignItems: 'flex-start' }}>
          <${SealHead} kind=${pin.kind} size=${30} />
          <div style=${{ minWidth: 0, flex: 1 }}>
            <div style=${{ fontFamily: 'var(--font-display)', fontSize: 14.5, fontWeight: 600, color: 'var(--ink)', lineHeight: 1.18 }}>${pin.name}</div>
            <div style=${{ fontSize: 10.5, color: 'var(--ink-faint)', marginTop: 1, fontFamily: 'var(--font-mono)' }}>${k.label || 'Entry'}</div>
          </div>
        </div>
        ${summary && html`<div style=${{ padding: '0 12px 11px', fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 12.5, lineHeight: 1.5, color: 'var(--ink-soft)' }}>${summary}</div>`}
        <div style=${{ display: 'flex', borderTop: '1px solid var(--rule-soft)', background: 'var(--paper-deep)' }}>
          <div style=${{ flex: 1, padding: '7px 12px', fontSize: 11, fontWeight: 600, color: 'var(--ink-muted)', display: 'flex', alignItems: 'center', gap: 5, whiteSpace: 'nowrap' }}>
            <${Icon} name="doc" size=${11} /> Open page
          </div>
          ${hasMap && html`<div style=${{ flex: 1, padding: '7px 12px', fontSize: 11, fontWeight: 600, color: 'var(--burgundy)', display: 'flex', alignItems: 'center', gap: 5, borderLeft: '1px solid var(--rule-soft)', whiteSpace: 'nowrap' }}>
            <${Icon} name="map" size=${11} /> Enter map ›
          </div>`}
        </div>
      </div>
      <div style=${{ position: 'absolute', left: '50%', bottom: -6, width: 12, height: 12, marginLeft: -6, transform: 'rotate(45deg)', background: 'var(--surface-raised)', borderRight: '1px solid var(--rule-strong)', borderBottom: '1px solid var(--rule-strong)' }} />
    </div>
  </div>`;
}

// frontmatter keys that are app metadata, not infobox rows
const RESERVED_FM = new Set(['kind', 'summary', 'aliases', 'tags', 'cssclasses', 'publish', 'permalink']);

// ── codex panel: the real vault page sliding in beside the map ────
// drag-resizable width shared by the codex + new-entry side panels
const usePanelWidth = () => useSidebarWidth('ck_atlas_panel_w', 380, { min: 320, max: 640, fromRight: true });

function CodexPanel({ pagePath, pinName, kind, to, canChart, onEnterMap, onChartMap, onRemovePin, onClose }) {
  const [page, setPage] = useState(null);
  const [err, setErr] = useState(null);
  const [width, onResize] = usePanelWidth();
  const id = store.campaign?.campaign_id;

  useEffect(() => {
    setPage(null); setErr(null);
    if (!pagePath) return;
    let dead = false;
    apiFetch(`/campaigns/${id}/vault/pages/${encodeURI(pagePath)}`)
      .then((p) => { if (!dead) setPage(p); })
      .catch((e) => { if (!dead) setErr(e.message); });
    return () => { dead = true; };
  }, [pagePath, store.dirty_vault]);

  const k = PIN_KINDS[kind || page?.kind] || { label: 'Entry' };
  const folder = pagePath ? pagePath.split('/').slice(0, -1).join('/') || '—' : '—';
  const props = page ? parseProps(splitDoc(page.content).fm).filter((p) => !RESERVED_FM.has(p.key)) : [];
  const backlinks = ((store.vaultLinks || {}).links || []).filter((l) => l.target_path === pagePath).length;
  const outLinks = ((store.vaultLinks || {}).links || [])
    .filter((l) => l.source_path === pagePath && l.target_path)
    .map((l) => l.target_path)
    .filter((v, i, a) => a.indexOf(v) === i)
    .slice(0, 8);
  const titleOf = (path) => (store.vaultPages || []).find((p) => p.path === path)?.title
    || path.split('/').pop().replace(/\.md$/, '');

  return html`<div style=${{
    width, flex: `0 0 ${width}px`, height: '100%', background: 'var(--surface)',
    borderLeft: '1px solid var(--rule)', boxShadow: '-12px 0 28px rgba(60,40,10,.10)',
    display: 'flex', flexDirection: 'column', minHeight: 0, zIndex: 60, position: 'relative',
  }}>
    <${ResizeHandle} side="left" onMouseDown=${onResize} />
    <div style=${{ padding: '14px 16px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 10 }}>
      <${SealHead} kind=${kind || page?.kind || 'lore'} size=${30} />
      <div style=${{ flex: 1, minWidth: 0 }}>
        <div style=${{ fontSize: 10, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>${k.label}</div>
        <div style=${{ fontFamily: 'var(--font-mono)', fontSize: 10.5, color: 'var(--ink-faint)', marginTop: 1, display: 'flex', alignItems: 'center', gap: 4, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
          <${Icon} name="folder" size=${10} /> ${folder}
        </div>
      </div>
      <button onClick=${onClose} style=${{ width: 28, height: 28, borderRadius: 4, color: 'var(--ink-muted)', display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'transparent', border: 'none', cursor: 'pointer' }}>
        <${Icon} name="x" size=${14} />
      </button>
    </div>

    <div style=${{ flex: 1, overflow: 'auto', padding: '18px 18px 28px' }}>
      <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 24, fontWeight: 500, letterSpacing: '-0.015em', lineHeight: 1.12, color: 'var(--ink)' }}>${page?.title || pinName || '…'}</h1>

      ${err && html`<div style=${{ marginTop: 10, padding: '10px 12px', background: '#FBEDE9', border: '1px solid rgba(122,46,31,.25)', borderRadius: 6, fontSize: 12.5, color: 'var(--burgundy-700)' }}>${err}</div>`}
      ${!page && !err && html`<div style=${{ marginTop: 16, display: 'flex', justifyContent: 'center' }}><${Spinner} /></div>`}

      ${page && html`
        ${page.summary && html`<div style=${{ marginTop: 10, padding: '10px 12px', background: 'var(--paper-deep)', border: '1px solid var(--rule-soft)', borderRadius: 6 }}>
          <div style=${{ fontSize: 9.5, fontWeight: 600, letterSpacing: '0.08em', textTransform: 'uppercase', color: 'var(--burgundy)', display: 'flex', alignItems: 'center', gap: 5, marginBottom: 4 }}>
            <${Icon} name="feather" size=${10} /> The Chronicle remembers
          </div>
          <div style=${{ fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 13.5, color: 'var(--ink-soft)', lineHeight: 1.5 }}>${page.summary}</div>
        </div>`}

        ${to && html`<button onClick=${onEnterMap} style=${{
          marginTop: 12, width: '100%', display: 'flex', alignItems: 'center', gap: 11,
          padding: '11px 13px', borderRadius: 8, cursor: 'pointer', textAlign: 'left',
          background: 'var(--burgundy)', border: '1px solid var(--burgundy-700)', color: '#F7E8E2',
          boxShadow: '0 4px 12px rgba(92,35,23,.22)',
        }}>
          <span style=${{ width: 30, height: 30, flex: '0 0 auto', borderRadius: 6, background: 'rgba(255,255,255,.14)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#F7E8E2' }}>
            <${Icon} name="map" size=${16} />
          </span>
          <span style=${{ flex: 1, minWidth: 0 }}>
            <span style=${{ display: 'block', fontFamily: 'var(--font-display)', fontSize: 14, fontWeight: 600, color: '#FBF6E9' }}>Enter the map of ${page.title || pinName}</span>
            <span style=${{ display: 'block', fontSize: 11, color: 'rgba(247,232,226,.8)', marginTop: 1 }}>This place is charted — explore its pins</span>
          </span>
          <${Icon} name="arrow-r" size=${15} />
        </button>`}

        ${!to && canChart && html`<button onClick=${onChartMap} style=${{
          marginTop: 12, width: '100%', display: 'flex', alignItems: 'center', gap: 10,
          padding: '10px 13px', borderRadius: 8, cursor: 'pointer', textAlign: 'left',
          background: 'var(--surface-raised)', border: '1px dashed var(--rule-strong)', color: 'var(--ink-soft)',
        }}>
          <${Icon} name="map" size=${14} style=${{ color: 'var(--burgundy)' }} />
          <span style=${{ flex: 1, fontSize: 12.5 }}>Add a map of this place — pick its art</span>
          <${Icon} name="chev-r" size=${12} />
        </button>`}

        ${props.length > 0 && html`<div style=${{ marginTop: 14, border: '1px solid var(--rule)', borderRadius: 6, overflow: 'hidden' }}>
          ${props.map((row, i) => html`<div key=${i} style=${{ display: 'flex', gap: 10, padding: '7px 12px', fontSize: 13, borderBottom: i < props.length - 1 ? '1px solid var(--rule-soft)' : 'none', background: i % 2 ? 'transparent' : 'var(--surface-raised)' }}>
            <span style=${{ width: 92, flex: '0 0 auto', color: 'var(--ink-faint)', fontSize: 11, fontWeight: 600, letterSpacing: '0.04em', textTransform: 'uppercase', paddingTop: 1 }}>${row.key}</span>
            <span style=${{ color: 'var(--ink)', fontFamily: 'var(--font-display)', minWidth: 0, overflowWrap: 'anywhere' }}>${row.values.join(', ')}</span>
          </div>`)}
        </div>`}

        <div style=${{ marginTop: 6 }}>
          <${PageBody} text=${page.content} pages=${store.vaultPages} />
        </div>

        ${outLinks.length > 0 && html`<div style=${{ marginTop: 16 }}>
          <div style=${{ fontSize: 10, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 7 }}>Linked pages</div>
          <div style=${{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
            ${outLinks.map((path) => html`<span key=${path} onClick=${() => navigate('page', { path })}
              style=${{ display: 'inline-flex', alignItems: 'center', gap: 5, padding: '3px 9px', borderRadius: 999, background: 'var(--burgundy-50)', color: 'var(--burgundy-700)', border: '1px solid rgba(122,46,31,.18)', fontSize: 12, fontFamily: 'var(--font-display)', cursor: 'pointer' }}>
              <${Icon} name="link" size=${11} /> ${titleOf(path)}
            </span>`)}
          </div>
        </div>`}
      `}
    </div>

    <div style=${{ borderTop: '1px solid var(--rule-soft)', padding: '10px 16px', display: 'flex', alignItems: 'center', gap: 12, fontSize: 11.5, color: 'var(--ink-muted)' }}>
      <span style=${{ display: 'flex', alignItems: 'center', gap: 5 }}><${Icon} name="backlink" size=${12} /> ${backlinks} backlink${backlinks === 1 ? '' : 's'}</span>
      ${onRemovePin && html`<button onClick=${onRemovePin} title="Remove this pin (the page stays)" style=${{ display: 'flex', alignItems: 'center', gap: 4, color: 'var(--ink-faint)', background: 'none', border: 'none', cursor: 'pointer', fontSize: 11.5 }}>
        <${Icon} name="trash" size=${11} /> Remove pin
      </button>`}
      <span style=${{ flex: 1 }} />
      <button onClick=${() => pagePath && navigate('page', { path: pagePath })} style=${{ display: 'inline-flex', alignItems: 'center', gap: 6, padding: '6px 11px', background: 'var(--burgundy)', color: '#FBF6E9', border: '1px solid var(--burgundy-700)', borderRadius: 4, fontSize: 12.5, fontWeight: 500, whiteSpace: 'nowrap', cursor: 'pointer' }}>
        Open page <${Icon} name="arrow-r" size=${12} />
      </button>
    </div>
  </div>`;
}

// Shown when a freshly-dropped pin needs a codex entry: name it to mint a new
// page — or pick an existing page and the pin links to it instead.
function NewEntryPanel({ kind, onCreate, onLink, onCancel, busy }) {
  const k = PIN_KINDS[kind] || {};
  const [width, onResize] = usePanelWidth();
  const [name, setName] = useState('');
  const ex = { place: 'Port Hadwin', npc: 'Reeve Aldwin Lorne', faction: 'The Saltmen', item: 'The Tide-Glass', lore: 'The Drowned Bell', pc: 'New companion' }[kind] || 'A new place';

  // where the page lands — the GM's own folder tree, kind folder preselected
  // when it exists, vault root otherwise
  const folders = store.vaultFolders || [];
  const [folder, setFolder] = useState(folders.includes(k.folder) ? k.folder : '');
  const folderOptions = [
    { value: '', label: '(vault root)' },
    ...folders.map((f) => ({ value: f, label: f })),
    ...(k.folder && !folders.includes(k.folder)
      ? [{ value: k.folder, label: `${k.folder} (new folder)` }] : []),
  ];

  // existing pages matching the typed name — same kind first, then the rest
  const q = name.trim().toLowerCase();
  const matches = useMemo(() => {
    const pages = store.vaultPages || [];
    const hit = (p) => p.title.toLowerCase().includes(q)
      || (p.aliases || []).some((a) => a.toLowerCase().includes(q));
    const pool = q ? pages.filter(hit) : pages.filter((p) => p.kind === kind);
    return [...pool].sort((a, b) =>
      (b.kind === kind) - (a.kind === kind) || a.title.localeCompare(b.title)
    ).slice(0, q ? 12 : 6);
  }, [q, kind, store.vaultPages]);
  return html`<div style=${{ width, flex: `0 0 ${width}px`, height: '100%', background: 'var(--surface)', borderLeft: '1px solid var(--rule)', boxShadow: '-12px 0 28px rgba(60,40,10,.10)', display: 'flex', flexDirection: 'column', minHeight: 0, zIndex: 60, position: 'relative' }}>
    <${ResizeHandle} side="left" onMouseDown=${onResize} />
    <div style=${{ padding: '14px 16px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 10 }}>
      <${SealHead} kind=${kind} size=${30} />
      <div style=${{ flex: 1 }}>
        <div style=${{ fontSize: 10, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--burgundy)' }}>New pin · ${k.label || 'Entry'}</div>
        <div style=${{ fontFamily: 'var(--font-mono)', fontSize: 10.5, color: 'var(--ink-faint)', marginTop: 1, display: 'flex', alignItems: 'center', gap: 4 }}><${Icon} name="folder" size=${10} /> ${folder || '(vault root)'}</div>
      </div>
      <button onClick=${onCancel} style=${{ width: 28, height: 28, borderRadius: 4, color: 'var(--ink-muted)', display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'transparent', border: 'none', cursor: 'pointer' }}><${Icon} name="x" size=${14} /></button>
    </div>

    <div style=${{ flex: 1, overflow: 'auto', padding: '18px 18px 28px' }}>
      <div style=${{ fontSize: 11, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 6 }}>Name this ${(k.label || 'place').toLowerCase()}</div>
      <input autoFocus value=${name} onInput=${(e) => setName(e.target.value)} placeholder=${`e.g. ${ex}`}
        onKeyDown=${(e) => { if (e.key === 'Enter' && name.trim() && !busy) onCreate(name, folder); }}
        style=${{ width: '100%', fontFamily: 'var(--font-display)', fontSize: 20, fontWeight: 500, color: 'var(--ink)', background: 'transparent', border: 'none', outline: 'none', borderBottom: '2px solid var(--rule-strong)', padding: '4px 0 8px' }} />

      <div style=${{ marginTop: 14 }}>
        <div style=${{ fontSize: 11, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 5, display: 'flex', alignItems: 'center', gap: 5 }}>
          <${Icon} name="folder" size=${11} /> Location
        </div>
        <${Select} value=${folder} onChange=${setFolder} options=${folderOptions} />
      </div>

      <div style=${{ marginTop: 14 }}>
        <div style=${{ fontSize: 10, fontWeight: 600, letterSpacing: '0.09em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 6 }}>
          ${q ? 'Or link an existing page' : `Existing ${(k.label || '').toLowerCase()} pages — type to search all pages`}
        </div>
        ${matches.length === 0 && html`<div style=${{ fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 12.5, color: 'var(--ink-faint)' }}>
          ${q ? 'No pages match — Create will mint a new one.' : 'None yet — type a name to search every page, or create one.'}
        </div>`}
        ${matches.length > 0 && html`<div style=${{ border: '1px solid var(--rule)', borderRadius: 8, overflow: 'auto', maxHeight: 280 }}>
          ${matches.map((p, i) => html`<div key=${p.path} onClick=${() => !busy && onLink(p)}
            style=${{ display: 'flex', alignItems: 'center', gap: 9, padding: '7px 10px', cursor: 'pointer',
              borderBottom: i < matches.length - 1 ? '1px solid var(--rule-soft)' : 'none', background: 'var(--surface-raised)' }}
            onMouseEnter=${(e) => { e.currentTarget.style.background = 'rgba(120,90,40,.08)'; }}
            onMouseLeave=${(e) => { e.currentTarget.style.background = 'var(--surface-raised)'; }}>
            <${SealHead} kind=${PIN_KINDS[p.kind] ? p.kind : kind} size=${22} />
            <div style=${{ flex: 1, minWidth: 0 }}>
              <div style=${{ fontFamily: 'var(--font-display)', fontSize: 13.5, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${p.title}</div>
              <div style=${{ fontFamily: 'var(--font-mono)', fontSize: 10, color: 'var(--ink-faint)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${p.path}</div>
            </div>
            <${Icon} name="link" size=${12} style=${{ color: 'var(--burgundy)' }} />
          </div>`)}
        </div>`}
      </div>

      <div style=${{ marginTop: 18, padding: '11px 13px', background: 'var(--paper-deep)', border: '1px dashed var(--rule-strong)', borderRadius: 8 }}>
        <div style=${{ fontSize: 9.5, fontWeight: 600, letterSpacing: '0.08em', textTransform: 'uppercase', color: 'var(--burgundy)', display: 'flex', alignItems: 'center', gap: 5, marginBottom: 6 }}><${Icon} name="feather" size=${10} /> The Chronicle will remember</div>
        <div style=${{ fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 13, color: 'var(--ink-faint)', lineHeight: 1.5 }}>
          A one-line memory is drafted from your sessions as soon as this entry is mentioned in play. You can write it yourself, too.
        </div>
      </div>
    </div>

    <div style=${{ borderTop: '1px solid var(--rule-soft)', padding: '12px 16px', display: 'flex', alignItems: 'center', gap: 10 }}>
      <${Btn} onClick=${onCancel}>Discard pin</${Btn}>
      <span style=${{ flex: 1 }} />
      <${Btn} kind="primary" icon="check" disabled=${!name.trim() || busy} onClick=${() => onCreate(name, folder)}>
        ${busy ? 'Creating…' : 'Create page'}
      </${Btn}>
    </div>
  </div>`;
}

// Create-a-map form: name + the map art. In the Tauri shell "Choose image"
// opens the native picker; in browser-dev the path is typed.
function MapForm({ title, initialName, onCreate, onCancel, busy }) {
  const [name, setName] = useState(initialName || '');
  const [imagePath, setImagePath] = useState('');
  const [err, setErr] = useState(null);
  const pick = async () => {
    const p = await pickMapImage();
    if (p) setImagePath(p);
  };
  const submit = async () => {
    setErr(null);
    try { await onCreate(name.trim(), imagePath.trim()); }
    catch (e) { setErr(e.message); }
  };
  return html`<div style=${{ width: 420, background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 10, boxShadow: 'var(--shadow-raised)', overflow: 'hidden' }}>
    <div style=${{ padding: '13px 16px 11px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 9 }}>
      <${Icon} name="map" size=${15} style=${{ color: 'var(--burgundy)' }} />
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15.5, fontWeight: 500, flex: 1 }}>${title}</div>
      <button onClick=${onCancel} style=${{ width: 26, height: 26, borderRadius: 4, color: 'var(--ink-muted)', display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'transparent', border: 'none', cursor: 'pointer' }}><${Icon} name="x" size=${13} /></button>
    </div>
    <div style=${{ padding: '14px 16px', display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div>
        <div style=${{ fontSize: 11, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 5 }}>Map name</div>
        <${Input} value=${name} onInput=${setName} placeholder="e.g. The Sundered Vale" />
      </div>
      <div>
        <div style=${{ fontSize: 11, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 5 }}>Map art</div>
        <div style=${{ display: 'flex', gap: 8 }}>
          <${Input} value=${imagePath} onInput=${setImagePath} mono placeholder="/path/to/map.png" style=${{ flex: 1 }} />
          <${Btn} icon="upload" onClick=${pick}>Choose…</${Btn}>
        </div>
        <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 5, lineHeight: 1.4, fontStyle: 'italic', fontFamily: 'var(--font-display)' }}>
          Your own art — an Inkarnate or Azgaar export, a scan, any image. It's copied into the world folder.
        </div>
      </div>
      ${err && html`<div style=${{ padding: '8px 11px', background: '#FBEDE9', border: '1px solid rgba(122,46,31,.25)', borderRadius: 6, fontSize: 12, color: 'var(--burgundy-700)' }}>${err}</div>`}
    </div>
    <div style=${{ padding: '11px 16px', borderTop: '1px solid var(--rule-soft)', background: 'var(--paper-deep)', display: 'flex', gap: 10 }}>
      <span style=${{ flex: 1 }} />
      <${Btn} onClick=${onCancel}>Cancel</${Btn}>
      <${Btn} kind="primary" icon="check" disabled=${!name.trim() || !imagePath.trim() || busy} onClick=${submit}>
        ${busy ? 'Creating…' : 'Create map'}
      </${Btn}>
    </div>
  </div>`;
}

// Map settings: rename, swap the art (pins keep their spots), delete.
function MapSettings({ map, busy, onSave, onDelete, onCancel }) {
  const [name, setName] = useState(map.name);
  const [imagePath, setImagePath] = useState('');
  const [confirmDel, setConfirmDel] = useState(false);
  const [err, setErr] = useState(null);
  const dirty = name.trim() !== map.name || imagePath.trim();
  const pick = async () => {
    const p = await pickMapImage();
    if (p) setImagePath(p);
  };
  const submit = async () => {
    setErr(null);
    try { await onSave(name.trim(), imagePath.trim()); }
    catch (e) { setErr(e.message); }
  };
  return html`<div style=${{ width: 420, background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 10, boxShadow: 'var(--shadow-raised)', overflow: 'hidden' }}>
    <div style=${{ padding: '13px 16px 11px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 9 }}>
      <${Icon} name="cog" size=${15} style=${{ color: 'var(--burgundy)' }} />
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15.5, fontWeight: 500, flex: 1 }}>Map settings · ${map.name}</div>
      <button onClick=${onCancel} style=${{ width: 26, height: 26, borderRadius: 4, color: 'var(--ink-muted)', display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'transparent', border: 'none', cursor: 'pointer' }}><${Icon} name="x" size=${13} /></button>
    </div>
    <div style=${{ padding: '14px 16px', display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div>
        <div style=${{ fontSize: 11, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 5 }}>Map name</div>
        <${Input} value=${name} onInput=${setName} />
      </div>
      <div>
        <div style=${{ fontSize: 11, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 5 }}>Replace map art</div>
        <div style=${{ display: 'flex', gap: 8 }}>
          <${Input} value=${imagePath} onInput=${setImagePath} mono placeholder="(keep current art)" style=${{ flex: 1 }} />
          <${Btn} icon="upload" onClick=${pick}>Choose…</${Btn}>
        </div>
        <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 5, lineHeight: 1.4, fontStyle: 'italic', fontFamily: 'var(--font-display)' }}>
          Pins keep their relative spots — best for a redrawn version of the same map.
        </div>
      </div>
      ${err && html`<div style=${{ padding: '8px 11px', background: '#FBEDE9', border: '1px solid rgba(122,46,31,.25)', borderRadius: 6, fontSize: 12, color: 'var(--burgundy-700)' }}>${err}</div>`}
    </div>
    <div style=${{ padding: '11px 16px', borderTop: '1px solid var(--rule-soft)', background: 'var(--paper-deep)', display: 'flex', gap: 10, alignItems: 'center' }}>
      <button onClick=${() => (confirmDel ? onDelete() : setConfirmDel(true))} disabled=${busy} style=${{
        display: 'inline-flex', alignItems: 'center', gap: 6, padding: '6px 11px', borderRadius: 4, fontSize: 12.5, fontWeight: 500, cursor: 'pointer',
        background: confirmDel ? 'var(--burgundy)' : 'transparent', color: confirmDel ? '#FBF6E9' : 'var(--burgundy-700)',
        border: confirmDel ? '1px solid var(--burgundy-700)' : '1px solid rgba(122,46,31,.25)' }}>
        <${Icon} name="trash" size=${12} /> ${confirmDel ? 'Really delete?' : 'Delete map'}
      </button>
      <span style=${{ flex: 1 }} />
      <${Btn} onClick=${onCancel}>Cancel</${Btn}>
      <${Btn} kind="primary" icon="check" disabled=${!name.trim() || !dirty || busy} onClick=${submit}>
        ${busy ? 'Saving…' : 'Save'}
      </${Btn}>
    </div>
  </div>`;
}

// centered overlay host for the map form
function MapFormOverlay(props) {
  return html`<div style=${{ position: 'absolute', inset: 0, zIndex: 120, display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'rgba(31,24,19,.30)' }}
    onMouseDown=${(e) => { if (e.target === e.currentTarget) props.onCancel(); }}>
    <${MapForm} ...${props} />
  </div>`;
}

// ── the Atlas stage ───────────────────────────────────────────────
function AtlasStage({ campaign, maps, initialMapId, initialPinId }) {
  const rootMap = maps.find((m) => m.id === initialMapId) || maps.find((m) => !m.parent) || maps[0];
  const [mapId, setMapId] = useState(rootMap.id);
  const [img, setImg] = useState(null);         // { url, w, h } for the current map art
  const [view, setView] = useState(null);
  const [stage, setStage] = useState({ w: 1000, h: 700 });
  const [hover, setHover] = useState(null);
  const [panel, setPanel] = useState(null);     // { pagePath, pinId?, kind?, name?, to? }
  const [veil, setVeil] = useState(0);
  const [anim, setAnim] = useState(false);
  const [placing, setPlacing] = useState(null); // kind being placed
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [newEntry, setNewEntry] = useState(null); // { pin } draft awaiting a page
  const [ghost, setGhost] = useState(null);
  const [mapForm, setMapForm] = useState(null); // { title, name?, parent?, page?, bindPin? }
  const [settings, setSettings] = useState(false);
  const [viewOpts, setViewOpts] = useState(false);
  const [pinSize, setPinSizeRaw] = useState(() => readPref('ck_atlas_pin_size', 'medium'));
  const [showLabels, setShowLabelsRaw] = useState(() => readPref('ck_atlas_labels', '1') !== '0');
  const [busy, setBusy] = useState(false);
  const [pinPos, setPinPos] = useState(null);   // { id, x, y } live override while dragging a pin

  const stageRef = useRef(null);
  const drag = useRef(null);
  const pinDrag = useRef(null);
  const pinMoved = useRef(false); // suppress the click that follows a pin drag
  const byId = useMemo(() => Object.fromEntries(maps.map((m) => [m.id, m])), [maps]);
  const map = byId[mapId] || rootMap;
  const pins = [...(map.pins || []), ...(newEntry ? [newEntry.pin] : [])]
    .map((p) => (pinPos && p.id === pinPos.id ? { ...p, x: pinPos.x, y: pinPos.y } : p));
  const W = img?.w || 1600, H = img?.h || 1000;

  // current render values for the stable window-level drag handlers
  const live = useRef({});
  live.current = { view, W, H, map };

  // deleted / missing map (e.g. after delete) — fall back to the root
  useEffect(() => { if (!byId[mapId]) setMapId(rootMap.id); }, [byId]);

  const chain = useMemo(() => {
    const out = []; let m = map; const seen = new Set();
    while (m && !seen.has(m.id)) { seen.add(m.id); out.unshift(m); m = m.parent ? byId[m.parent] : null; }
    return out;
  }, [mapId, byId]);

  // mirror the live map into the store so the sidebar tree tracks descents
  useEffect(() => {
    setState({ atlasMapId: mapId });
    return () => setState({ atlasMapId: null });
  }, [mapId]);

  // sidebar map list navigates by route param — follow it
  useEffect(() => {
    if (initialMapId && byId[initialMapId] && initialMapId !== mapId) {
      setMapId(initialMapId); setPanel(null); setNewEntry(null); setHover(null); setView(null);
    }
  }, [initialMapId]);

  // reverse lookup ("show on map"): focus the pin named in the route once
  // the map art is up and the view is fitted
  const focusPin = useRef(initialPinId || null);
  useEffect(() => { if (initialPinId) focusPin.current = initialPinId; }, [initialPinId]);
  useEffect(() => {
    if (!focusPin.current || !view || !img) return;
    const pin = (map.pins || []).find((p) => p.id === focusPin.current);
    focusPin.current = null;
    if (pin) openPin(pin);
  }, [view, img, mapId, initialPinId]);

  // load the map art as a blob (an <img src> can't carry the auth header)
  useEffect(() => {
    let dead = false; let url = null;
    setImg(null);
    apiBlob(`/campaigns/${campaign.campaign_id}/atlas/maps/${encodeURIComponent(map.id)}/image`)
      .then((blob) => {
        if (dead) return;
        url = URL.createObjectURL(blob);
        const el = new Image();
        el.onload = () => { if (!dead) setImg({ url, w: el.naturalWidth, h: el.naturalHeight }); };
        el.src = url;
      })
      .catch((e) => console.warn('map art failed:', e));
    return () => { dead = true; if (url) URL.revokeObjectURL(url); };
  }, [map.id, map.image, map.art_seq]);

  const fitView = useCallback((s, w, h) => {
    const z = Math.min(s.w / w, s.h / h) * 0.95;
    return { zoom: z, tx: (s.w - w * z) / 2, ty: (s.h - h * z) / 2 };
  }, []);

  useEffect(() => {
    const el = stageRef.current; if (!el) return;
    const ro = new ResizeObserver(() => { const r = el.getBoundingClientRect(); setStage({ w: r.width, h: r.height }); });
    ro.observe(el);
    const r = el.getBoundingClientRect(); setStage({ w: r.width, h: r.height });
    return () => ro.disconnect();
  }, []);

  // re-fit whenever a new image arrives
  useEffect(() => { if (img && stage.w > 1) setView(fitView(stage, img.w, img.h)); }, [img]);

  // native non-passive wheel zoom toward the cursor
  useEffect(() => {
    const el = stageRef.current; if (!el) return;
    const onWheel = (e) => {
      e.preventDefault(); setAnim(false);
      setView((v) => {
        if (!v) return v;
        const r = el.getBoundingClientRect();
        const mx = e.clientX - r.left, my = e.clientY - r.top;
        const fit = Math.min(stage.w / W, stage.h / H) * 0.95;
        const z2 = clamp(v.zoom * Math.exp(-e.deltaY * 0.0014), fit * 0.8, 5.5);
        const wx = (mx - v.tx) / v.zoom, wy = (my - v.ty) / v.zoom;
        return { zoom: z2, tx: mx - wx * z2, ty: my - wy * z2 };
      });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [stage, W, H]);

  // Esc cancels placing / creation / form
  useEffect(() => {
    const onKey = (e) => {
      if (e.key !== 'Escape') return;
      setPlacing(null); setPaletteOpen(false); setGhost(null); setNewEntry(null); setMapForm(null); setSettings(false); setViewOpts(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  useEffect(() => {
    const onMove = (e) => {
      if (pinDrag.current) {
        const { view: v, W, H } = live.current;
        if (!v || !stageRef.current) return;
        const r = stageRef.current.getBoundingClientRect();
        const x = clamp((e.clientX - r.left - v.tx) / v.zoom / W, 0, 1);
        const y = clamp((e.clientY - r.top - v.ty) / v.zoom / H, 0, 1);
        Object.assign(pinDrag.current, { moved: true, x, y });
        setPinPos({ id: pinDrag.current.id, x, y });
        return;
      }
      if (!drag.current) return;
      const dx = e.clientX - drag.current.x, dy = e.clientY - drag.current.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) drag.current.moved = true;
      setView((v) => ({ ...v, tx: drag.current.tx + dx, ty: drag.current.ty + dy }));
    };
    const onUp = () => {
      if (pinDrag.current) {
        const d = pinDrag.current;
        pinDrag.current = null;
        if (d.moved && d.x != null) {
          pinMoved.current = true;
          const m = live.current.map;
          saveAtlasMap({ ...m, pins: (m.pins || []).map((p) => (p.id === d.id ? { ...p, x: d.x, y: d.y } : p)) })
            .catch((e) => console.warn('saveAtlasMap failed:', e))
            .finally(() => setPinPos(null)); // keep the override until the store has the new spot
        } else {
          setPinPos(null);
        }
        return;
      }
      drag.current = null;
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => { window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); };
  }, []);

  const screenToNorm = (clientX, clientY) => {
    const r = stageRef.current.getBoundingClientRect();
    const mx = clientX - r.left, my = clientY - r.top;
    return { x: clamp(((mx - view.tx) / view.zoom) / W, 0, 1), y: clamp(((my - view.ty) / view.zoom) / H, 0, 1) };
  };

  const setPinSize = (s) => { setPinSizeRaw(s); writePref('ck_atlas_pin_size', s); };
  const setShowLabels = (v) => { setShowLabelsRaw(v); writePref('ck_atlas_labels', v ? '1' : '0'); };
  const pinScale = PIN_SCALES[pinSize] || 1;

  const onDown = (e) => {
    if (e.button !== 0 || !view) return;
    setViewOpts(false);
    if (placing) {
      const { x, y } = screenToNorm(e.clientX, e.clientY);
      const pin = { id: 'p' + Date.now().toString(36), name: 'Untitled ' + ((PIN_KINDS[placing] || {}).label || 'place'), kind: placing, x, y };
      setNewEntry({ pin }); setPanel(null);
      setPlacing(null); setGhost(null);
      return;
    }
    setAnim(false);
    drag.current = { x: e.clientX, y: e.clientY, tx: view.tx, ty: view.ty, moved: false };
  };

  const onStageMove = (e) => { if (placing) { const r = stageRef.current.getBoundingClientRect(); setGhost({ x: e.clientX - r.left, y: e.clientY - r.top }); } };

  const centerOn = useCallback((px, py, zMin) => {
    setView((v) => {
      const z = Math.max(v.zoom, zMin || 1.0);
      let pw = 380;
      try { pw = parseInt(localStorage.getItem('ck_atlas_panel_w'), 10) || 380; } catch (_) {}
      const sx = (stage.w - pw) / 2, sy = stage.h / 2;
      return { zoom: z, tx: sx - px * W * z, ty: sy - py * H * z };
    });
  }, [stage, W, H]);

  const zoomBy = (factor) => {
    setAnim(true);
    setView((v) => {
      const fit = Math.min(stage.w / W, stage.h / H) * 0.95;
      const z2 = clamp(v.zoom * factor, fit * 0.8, 5.5);
      const cx = stage.w / 2, cy = stage.h / 2;
      const wx = (cx - v.tx) / v.zoom, wy = (cy - v.ty) / v.zoom;
      return { zoom: z2, tx: cx - wx * z2, ty: cy - wy * z2 };
    });
  };

  const goToMap = useCallback((id) => {
    if (!byId[id]) return;
    setAnim(false); setVeil(1);
    setTimeout(() => {
      setMapId(id); setPanel(null); setNewEntry(null); setHover(null); setView(null);
      requestAnimationFrame(() => requestAnimationFrame(() => setVeil(0)));
    }, 280);
  }, [byId]);

  // start dragging an existing pin (the draft pin isn't persisted yet)
  const onPinDown = (pin) => (e) => {
    if (e.button !== 0 || placing || pin.id === newEntry?.pin.id) return;
    e.stopPropagation();
    pinDrag.current = { id: pin.id, moved: false };
  };

  const openPin = (pin) => {
    if (pinMoved.current) { pinMoved.current = false; return; }
    if (drag.current && drag.current.moved) return;
    if (!pin.page) return;
    setNewEntry(null);
    setPanel({ pagePath: pin.page, pinId: pin.id, kind: pin.kind, name: pin.name, to: pin.to });
    setAnim(true); centerOn(pin.x, pin.y, 1.0);
  };

  // the current map's own codex entry (the world / region / city has lore too)
  const openMapEntry = () => {
    if (!map.page) return;
    setNewEntry(null); setAnim(false);
    setPanel((p) => (p && !p.pinId && p.pagePath === map.page) ? null : { pagePath: map.page, kind: 'place', name: map.name });
  };
  const mapEntryOpen = !!(panel && !panel.pinId && map.page && panel.pagePath === map.page);

  const persist = (next) => saveAtlasMap(next).catch((e) => console.warn('saveAtlasMap failed:', e));

  const commitNew = async (n, name, folder) => {
    const nm = (name || '').trim() || n.pin.name;
    setBusy(true);
    try {
      const page = await createVaultPage(nm, n.pin.kind, folder || null);
      const pin = { ...n.pin, name: nm, page: page.path };
      persist({ ...map, pins: [...(map.pins || []), pin] });
      setNewEntry(null);
      setPanel({ pagePath: pin.page, pinId: pin.id, kind: pin.kind, name: pin.name });
    } catch (e) {
      console.warn('create page failed:', e);
    } finally {
      setBusy(false);
    }
  };

  // link the draft pin to an existing page instead of minting a new one
  const commitLink = (n, page) => {
    const kind = PIN_KINDS[page.kind] ? page.kind : n.pin.kind;
    const pin = { ...n.pin, name: page.title, kind, page: page.path };
    persist({ ...map, pins: [...(map.pins || []), pin] });
    setNewEntry(null);
    setPanel({ pagePath: pin.page, pinId: pin.id, kind: pin.kind, name: pin.name });
  };

  const removePin = (pinId) => {
    persist({ ...map, pins: (map.pins || []).filter((p) => p.id !== pinId) });
    setPanel(null);
  };

  // create a map (root or child); a child binds the pin that requested it
  const submitMapForm = async (name, imagePath) => {
    const f = mapForm;
    setBusy(true);
    try {
      const child = await createAtlasMap(name, imagePath, f.parent, f.page);
      if (f.bindPin) {
        persist({ ...map, pins: (map.pins || []).map((p) => (p.id === f.bindPin ? { ...p, to: child.id } : p)) });
        setPanel((p) => (p ? { ...p, to: child.id } : p));
      } else {
        // a new top-level map — jump straight onto it
        navigate('atlas', { id: campaign.campaign_id, map: child.id });
      }
      setMapForm(null);
    } finally {
      setBusy(false);
    }
  };

  const saveSettings = async (name, imagePath) => {
    setBusy(true);
    try {
      if (imagePath) await replaceAtlasMapArt(map.id, imagePath);
      if (name && name !== map.name) await saveAtlasMap({ ...map, name });
      setSettings(false);
    } finally {
      setBusy(false);
    }
  };

  const deleteCurrent = async () => {
    setBusy(true);
    try {
      const parent = map.parent;
      await deleteAtlasMap(map.id);
      setSettings(false); setPanel(null); setNewEntry(null);
      navigate('atlas', { id: campaign.campaign_id, ...(parent ? { map: parent } : {}) });
    } finally {
      setBusy(false);
    }
  };

  const chartFromPin = () => {
    if (!panel?.pinId) return;
    const pin = (map.pins || []).find((p) => p.id === panel.pinId);
    if (!pin) return;
    setMapForm({ title: `Map of ${pin.name}`, name: pin.name, parent: map.id, page: pin.page, bindPin: pin.id });
  };

  const invZoom = view ? clamp(1 / view.zoom, 0.5, 1.5) : 1;
  const hoverPin = hover && !pinPos ? pins.find((p) => p.id === hover) : null;
  const summaryOf = (path) => (store.vaultPages || []).find((p) => p.path === path)?.summary || '';

  return html`<div style=${{ position: 'absolute', inset: 0, display: 'flex', minHeight: 0 }}>
    <div ref=${stageRef} onMouseDown=${onDown} onMouseMove=${onStageMove}
      style=${{ position: 'relative', flex: 1, minWidth: 0, overflow: 'hidden',
        background: 'radial-gradient(circle at 50% 40%, #ECE3CB, #DCCFB0)',
        cursor: placing ? 'crosshair' : 'grab' }}>

      ${!img && html`<div style=${{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center' }}><${Spinner} size=${22} /></div>`}

      ${img && view && html`<div style=${{ position: 'absolute', top: 0, left: 0, width: W, height: H,
        transform: `translate(${view.tx}px, ${view.ty}px) scale(${view.zoom})`, transformOrigin: '0 0',
        transition: anim ? 'transform .55s cubic-bezier(.4,0,.2,1)' : 'none' }}>
        <img src=${img.url} width=${W} height=${H} draggable=${false}
          style=${{ display: 'block', userSelect: 'none', boxShadow: '0 10px 40px rgba(60,40,10,.28)' }} />

        ${pins.map((pin) => html`<${PinMarker} key=${pin.id} pin=${pin} invZoom=${invZoom}
          scale=${pinScale} showLabel=${showLabels}
          selected=${panel?.pinId === pin.id || newEntry?.pin.id === pin.id}
          hasMap=${!!pin.to}
          onHover=${() => !placing && setHover(pin.id)} onLeave=${() => setHover(null)}
          onClick=${() => openPin(pin)} onDoubleClick=${() => pin.to && goToMap(pin.to)}
          onMouseDown=${onPinDown(pin)}
          onMenu=${(e) => openContextMenu(e, [
            pin.page && { label: 'Open page', icon: 'book', onClick: () => navigate('page', { path: pin.page }) },
            pin.page && { label: 'Read here', icon: 'doc', onClick: () => openPin(pin) },
            pin.to && { label: 'Enter the map', icon: 'map', onClick: () => goToMap(pin.to) },
            '-',
            { label: 'Remove pin', icon: 'trash', danger: true, onClick: () => removePin(pin.id) },
          ])} />`)}

        ${hoverPin && hoverPin.page && html`<${HoverCard} pin=${hoverPin} invZoom=${invZoom} scale=${pinScale} summary=${summaryOf(hoverPin.page)} />`}
      </div>`}

      ${placing && ghost && html`<div style=${{ position: 'absolute', left: ghost.x, top: ghost.y, transform: 'translate(-50%,-100%)', pointerEvents: 'none', opacity: 0.7, zIndex: 75 }}>
        <${SealHead} kind=${placing} size=${Math.round(38 * pinScale)} />
      </div>`}

      ${''/* descend / ascend dissolve veil */}
      <div style=${{ position: 'absolute', inset: 0, pointerEvents: 'none', background: 'radial-gradient(circle, rgba(242,235,217,.5), rgba(220,207,176,.96))', opacity: veil, transition: 'opacity .3s ease', zIndex: 80 }} />

      ${''/* breadcrumb (top-left) — the path, nothing more */}
      <div style=${{ position: 'absolute', top: 14, left: 14, zIndex: 90, display: 'flex', alignItems: 'center', gap: 2,
        background: 'rgba(251,246,233,.9)', backdropFilter: 'blur(3px)', border: '1px solid var(--rule)', borderRadius: 8, padding: '5px 10px 5px 8px', boxShadow: 'var(--shadow-card)' }}>
        <${Icon} name="map" size=${13} style=${{ color: 'var(--burgundy)' }} />
        ${chain.map((m, i) => html`
          ${i > 0 && html`<${Icon} name="chev-r" size=${11} style=${{ color: 'var(--ink-faint)' }} />`}
          <button key=${m.id} onClick=${() => m.id !== mapId && goToMap(m.id)} style=${{ fontFamily: 'var(--font-display)', fontSize: 13.5, fontWeight: i === chain.length - 1 ? 600 : 500, color: i === chain.length - 1 ? 'var(--ink)' : 'var(--burgundy)', padding: '2px 6px', borderRadius: 4, cursor: i === chain.length - 1 ? 'default' : 'pointer', background: 'none', border: 'none', whiteSpace: 'nowrap' }}>${m.name}</button>
        `)}
        ${map.page && html`
          <span style=${{ width: 1, height: 15, background: 'var(--rule)', margin: '0 5px' }} />
          <button onClick=${openMapEntry} title="Read this map's own codex entry" style=${{
            display: 'flex', alignItems: 'center', gap: 5, fontSize: 12, fontWeight: 500, fontFamily: 'var(--font-ui)',
            color: mapEntryOpen ? 'var(--burgundy-700)' : 'var(--ink-muted)', background: mapEntryOpen ? 'var(--burgundy-50)' : 'transparent',
            border: mapEntryOpen ? '1px solid rgba(122,46,31,.18)' : '1px solid transparent', padding: '3px 9px', borderRadius: 999, cursor: 'pointer', whiteSpace: 'nowrap' }}>
            <${Icon} name="scroll" size=${12} /> Entry
          </button>
        `}
      </div>

      ${''/* view options + map settings + new root map (top-right) */}
      <div style=${{ position: 'absolute', top: 14, right: 16, zIndex: 90, display: 'flex', gap: 8 }}>
        <button onClick=${() => setViewOpts((o) => !o)} title="View options — pin size, labels" style=${{
          display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: 30, height: 30, borderRadius: 8,
          background: viewOpts ? 'var(--burgundy-50)' : 'rgba(251,246,233,.9)', backdropFilter: 'blur(3px)',
          border: viewOpts ? '1px solid rgba(122,46,31,.25)' : '1px solid var(--rule)',
          boxShadow: 'var(--shadow-card)', color: viewOpts ? 'var(--burgundy-700)' : 'var(--ink-soft)', cursor: 'pointer' }}>
          <${Icon} name="eye" size=${14} />
        </button>
        <button onClick=${() => setSettings(true)} title="Map settings — rename, replace art, delete" style=${{
          display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: 30, height: 30, borderRadius: 8,
          background: 'rgba(251,246,233,.9)', backdropFilter: 'blur(3px)', border: '1px solid var(--rule)',
          boxShadow: 'var(--shadow-card)', color: 'var(--ink-soft)', cursor: 'pointer' }}>
          <${Icon} name="cog" size=${14} />
        </button>
        <button onClick=${() => setMapForm({ title: 'New map' })} style=${{
          display: 'inline-flex', alignItems: 'center', gap: 6, padding: '6px 12px', borderRadius: 8,
          background: 'rgba(251,246,233,.9)', backdropFilter: 'blur(3px)', border: '1px solid var(--rule)',
          boxShadow: 'var(--shadow-card)', color: 'var(--ink-soft)', fontSize: 12.5, fontWeight: 500, cursor: 'pointer', whiteSpace: 'nowrap' }}>
          <${Icon} name="upload" size=${13} /> New map
        </button>
      </div>

      ${viewOpts && html`<div onMouseDown=${(e) => e.stopPropagation()} style=${{
        position: 'absolute', top: 50, right: 16, zIndex: 95, width: 196,
        background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 10,
        boxShadow: 'var(--shadow-raised)', overflow: 'hidden' }}>
        <div style=${{ padding: '9px 12px 7px', borderBottom: '1px solid var(--rule-soft)', fontSize: 10.5, fontWeight: 600, letterSpacing: '0.09em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>View options</div>
        <div style=${{ padding: '10px 12px 12px', display: 'flex', flexDirection: 'column', gap: 11 }}>
          <div>
            <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.05em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 6 }}>Pin size</div>
            <div style=${{ display: 'flex', border: '1px solid var(--rule)', borderRadius: 6, overflow: 'hidden' }}>
              ${['small', 'medium', 'large'].map((s, i) => html`<button key=${s} onClick=${() => setPinSize(s)} style=${{
                flex: 1, padding: '5px 0', fontSize: 11.5, fontWeight: pinSize === s ? 600 : 400, cursor: 'pointer',
                background: pinSize === s ? 'var(--burgundy)' : 'transparent', color: pinSize === s ? '#FBF6E9' : 'var(--ink-soft)',
                border: 'none', borderLeft: i ? '1px solid var(--rule-soft)' : 'none' }}>${s[0].toUpperCase() + s.slice(1)}</button>`)}
            </div>
          </div>
          <button onClick=${() => setShowLabels(!showLabels)} style=${{
            display: 'flex', alignItems: 'center', gap: 8, padding: 0, background: 'none', border: 'none', cursor: 'pointer', textAlign: 'left' }}>
            <span style=${{ width: 15, height: 15, flex: '0 0 auto', borderRadius: 4, display: 'flex', alignItems: 'center', justifyContent: 'center',
              background: showLabels ? 'var(--burgundy)' : 'var(--surface)', border: showLabels ? '1px solid var(--burgundy-700)' : '1px solid var(--rule-strong)', color: '#FBF6E9' }}>
              ${showLabels && html`<${Icon} name="check" size=${10} />`}
            </span>
            <span style=${{ fontSize: 12.5, color: 'var(--ink)' }}>Pin labels</span>
          </button>
        </div>
      </div>`}

      ${placing && html`<div style=${{ position: 'absolute', top: 16, left: '50%', transform: 'translateX(-50%)', zIndex: 95,
        display: 'flex', alignItems: 'center', gap: 9, padding: '7px 14px', borderRadius: 999, whiteSpace: 'nowrap',
        background: 'var(--burgundy)', color: '#FBF6E9', boxShadow: '0 6px 18px rgba(92,35,23,.3)', fontSize: 13 }}>
        <${Icon} name="pin" size=${13} />
        <span>Click the map to place a ${html`<b style=${{ fontWeight: 600 }}>${(PIN_KINDS[placing] || {}).label}</b>`}</span>
        <span style=${{ opacity: 0.6 }}>·</span>
        <button onClick=${() => { setPlacing(null); setGhost(null); }} style=${{ color: '#F7E8E2', fontSize: 12, opacity: 0.85, fontFamily: 'var(--font-mono)', background: 'none', border: 'none', cursor: 'pointer' }}>Esc to cancel</button>
      </div>`}

      ${''/* zoom controls — pinned LEFT so the panel never shoves them */}
      <div style=${{ position: 'absolute', bottom: 16, left: 14, zIndex: 88, display: 'flex', flexDirection: 'column', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 8, boxShadow: 'var(--shadow-card)', overflow: 'hidden' }}>
        ${[['plus', () => zoomBy(1.4), ''], ['compass', () => { setAnim(true); if (img) setView(fitView(stage, img.w, img.h)); }, 'Fit map'], ['x', () => zoomBy(1 / 1.4), '']].map((b, i) => html`
          <button key=${i} onClick=${b[1]} title=${b[2]} style=${{ width: 34, height: 34, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--ink-soft)', borderBottom: i < 2 ? '1px solid var(--rule-soft)' : 'none', background: 'transparent', borderLeft: 'none', borderRight: 'none', borderTop: 'none', cursor: 'pointer' }}>
            <${Icon} name=${b[0]} size=${i === 1 ? 15 : 14} />
          </button>`)}
      </div>

      ${''/* pin palette + Drop-a-pin (bottom-right) — lifted clear of the global Ask-the-Keeper dock button */}
      <div style=${{ position: 'absolute', bottom: 74, right: 16, zIndex: 88, display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 10 }}>
        ${paletteOpen && html`<div style=${{ width: 184, background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 10, boxShadow: 'var(--shadow-raised)', overflow: 'hidden' }}>
          <div style=${{ padding: '9px 12px 7px', borderBottom: '1px solid var(--rule-soft)', fontSize: 10.5, fontWeight: 600, letterSpacing: '0.09em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>New pin · pick a kind</div>
          <div style=${{ padding: 5 }}>
            ${PALETTE.map((k) => html`<div key=${k} onClick=${() => { setPlacing(k); setPaletteOpen(false); }}
              style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '6px 8px', borderRadius: 6, cursor: 'pointer' }}
              onMouseEnter=${(e) => { e.currentTarget.style.background = 'rgba(120,90,40,.08)'; }}
              onMouseLeave=${(e) => { e.currentTarget.style.background = 'transparent'; }}>
              <${SealHead} kind=${k} size=${22} />
              <span style=${{ flex: 1, fontFamily: 'var(--font-display)', fontSize: 13.5, color: 'var(--ink)' }}>${(PIN_KINDS[k] || {}).label}</span>
            </div>`)}
          </div>
          <div style=${{ padding: '7px 12px', borderTop: '1px solid var(--rule-soft)', background: 'var(--paper-deep)', fontSize: 11, color: 'var(--ink-faint)', fontStyle: 'italic', fontFamily: 'var(--font-display)' }}>
            Creates a codex page of that kind.
          </div>
        </div>`}
        <button onClick=${() => { setPaletteOpen((o) => !o); setPlacing(null); setGhost(null); }} style=${{
          display: 'inline-flex', alignItems: 'center', gap: 8, padding: '9px 14px', borderRadius: 999,
          background: paletteOpen ? 'var(--burgundy-700)' : 'var(--burgundy)', color: '#F7E8E2', border: '1px solid var(--burgundy-700)',
          boxShadow: '0 6px 16px rgba(92,35,23,.26)', fontFamily: 'var(--font-display)', fontSize: 13.5, fontWeight: 500, whiteSpace: 'nowrap', cursor: 'pointer' }}>
          <${Icon} name="pin" size=${14} /> Drop a pin <${Icon} name=${paletteOpen ? 'chev-d' : 'chev-r'} size=${11} />
        </button>
      </div>

      ${mapForm && html`<${MapFormOverlay} title=${mapForm.title} initialName=${mapForm.name} busy=${busy}
        onCreate=${submitMapForm} onCancel=${() => setMapForm(null)} />`}

      ${settings && html`<div style=${{ position: 'absolute', inset: 0, zIndex: 120, display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'rgba(31,24,19,.30)' }}
        onMouseDown=${(e) => { if (e.target === e.currentTarget) setSettings(false); }}>
        <${MapSettings} key=${map.id} map=${map} busy=${busy}
          onSave=${saveSettings} onDelete=${deleteCurrent} onCancel=${() => setSettings(false)} />
      </div>`}
    </div>

    ${panel && html`<${CodexPanel} pagePath=${panel.pagePath} pinName=${panel.name} kind=${panel.kind}
      to=${panel.to} canChart=${!!panel.pinId && !busy}
      onEnterMap=${() => goToMap(panel.to)} onChartMap=${chartFromPin}
      onRemovePin=${panel.pinId ? () => removePin(panel.pinId) : null}
      onClose=${() => setPanel(null)} />`}

    ${newEntry && html`<${NewEntryPanel} kind=${newEntry.pin.kind} busy=${busy}
      onCreate=${(name, folder) => commitNew(newEntry, name, folder)}
      onLink=${(page) => commitLink(newEntry, page)}
      onCancel=${() => setNewEntry(null)} />`}
  </div>`;
}

// ── screen ────────────────────────────────────────────────────────
export function AtlasScreen({ store: s }) {
  const c = s.campaign;
  const [loaded, setLoaded] = useState(false);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    if (!c?.campaign_id) return;
    Promise.all([loadAtlasMaps(c.campaign_id), loadVaultTree(c.campaign_id)])
      .then(() => loadVaultLinks(c.campaign_id))
      .finally(() => setLoaded(true));
  }, [c?.campaign_id]);

  if (!c) { navigate('library'); return null; }

  const maps = s.atlasMaps || [];
  const sidebar = html`<${Sidebar} variant="campaign" active="atlas" campaign=${c} />`;
  const topbar = html`<${Topbar} crumbs=${[
    { label: 'Worlds', onClick: () => navigate('library') },
    { label: c.name, onClick: () => openCampaign(c.campaign_id) },
    'Atlas',
  ]} />`;

  let body;
  if (!c.vault_path) {
    body = html`<${Empty} icon="map" title="The Atlas needs a vault">
      Attach a vault folder to this world first — map pins live as codex pages.
    </${Empty}>`;
  } else if (!loaded) {
    body = html`<div style=${{ height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}><${Spinner} size=${22} /></div>`;
  } else if (!maps.length) {
    body = html`<div style=${{ height: '100%', display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: 14 }}>
      ${creating
        ? html`<${MapForm} title="New map" busy=${false}
            onCreate=${async (name, imagePath) => {
              const m = await createAtlasMap(name, imagePath);
              setCreating(false);
              navigate('atlas', { id: c.campaign_id, map: m.id });
            }}
            onCancel=${() => setCreating(false)} />`
        : html`
          <${Empty} icon="map" title="No maps yet">
            Bring your own map art — an Inkarnate or Azgaar export, a scan, any image —
            and pin the world onto it. Every pin is a codex page; places can carry
            maps of their own.
          </${Empty}>
          <${Btn} kind="primary" icon="upload" onClick=${() => setCreating(true)}>Import map art</${Btn}>
        `}
    </div>`;
  } else {
    body = html`<${AtlasStage} campaign=${c} maps=${maps} initialMapId=${s.route.params?.map} initialPinId=${s.route.params?.pin} />`;
  }

  return html`<${Shell} sidebar=${sidebar} topbar=${topbar}
    bodyStyle=${{ padding: 0, position: 'relative', overflow: 'hidden' }}>
    ${body}
  </${Shell}>`;
}
