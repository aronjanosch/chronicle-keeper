// Atlas preview cards: hold ⌘/Ctrl over a pin to read its page in a floating
// card; pin the card to keep it open, drag and resize it. Pinned cards are
// saved with the map (`pinned_previews`, screen px inside the stage).
import { html, useState, useEffect, useRef } from '../../vendor/htm-preact-standalone.mjs';
import { apiFetch, store, navigate, splitPageRef } from '../core.js';
import { Icon, PageBody, Spinner } from '../ui.js';
import { flashHeading } from './atlasPage.js';
import {
  MIN_W, MIN_H, initialKeys, keysReduce, previewTarget, clampRect, placeNear, normalizePreviews, withPreview,
} from './atlasPreviewLogic.js';

// True while ⌘ (or Ctrl) is down; drops on blur so a stuck key can't pin a card open.
export function useModifier() {
  const [keys, setKeys] = useState(initialKeys);
  const dispatch = (ev) => setKeys((s) => keysReduce(s, ev));
  useEffect(() => {
    const on = (e) => dispatch({ type: 'mod', on: e.metaKey || e.ctrlKey });
    const off = () => dispatch({ type: 'blur' });
    window.addEventListener('keydown', on);
    window.addEventListener('keyup', on);
    window.addEventListener('blur', off);
    return () => { window.removeEventListener('keydown', on); window.removeEventListener('keyup', on); window.removeEventListener('blur', off); };
  }, []);
  return [keys, dispatch];
}

function PreviewCard({ pin, rect, pinned, Seal, onPinToggle, onLayout, onEnter, onLeave, onOpen }) {
  const bodyRef = useRef(null);
  const [page, setPage] = useState(null);
  const [err, setErr] = useState(null);
  const [live, setLive] = useState(null); // rect while dragging / resizing
  const { path, heading } = splitPageRef(pin.page);
  const id = store.campaign?.campaign_id;
  const r = live || rect;

  useEffect(() => {
    setPage(null); setErr(null);
    let dead = false;
    apiFetch(`/campaigns/${id}/vault/pages/${encodeURI(path)}`)
      .then((p) => { if (!dead) setPage(p); })
      .catch((e) => { if (!dead) setErr(e.message); });
    return () => { dead = true; };
  }, [path, store.dirty_vault]);

  useEffect(() => {
    if (!page || !heading) return undefined;
    const t = setTimeout(() => flashHeading(bodyRef.current, heading), 60);
    return () => clearTimeout(t);
  }, [page, heading]);

  // one gesture: move (header) or resize (corner); saved once on release
  const gesture = (mode) => (e) => {
    if (e.button !== 0) return;
    e.preventDefault(); e.stopPropagation();
    const start = { x: e.clientX, y: e.clientY, r: { ...rect } };
    let last = start.r;
    const move = (ev) => {
      const dx = ev.clientX - start.x, dy = ev.clientY - start.y;
      last = mode === 'move'
        ? { ...start.r, x: start.r.x + dx, y: start.r.y + dy }
        : { ...start.r, w: Math.max(MIN_W, start.r.w + dx), h: Math.max(MIN_H, start.r.h + dy) };
      setLive(last);
    };
    const up = () => {
      window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up);
      setLive(null);
      if (last !== start.r) onLayout(last);
    };
    window.addEventListener('mousemove', move); window.addEventListener('mouseup', up);
  };

  const iconBtn = { width: 24, height: 24, display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'transparent', border: 'none', borderRadius: 4, cursor: 'pointer', color: 'var(--ink-muted)' };
  return html`<div onMouseDown=${(e) => e.stopPropagation()} onClick=${(e) => e.stopPropagation()} onMouseEnter=${onEnter} onMouseLeave=${onLeave}
    style=${{ position: 'absolute', left: r.x, top: r.y, width: r.w, height: r.h, zIndex: pinned ? 84 : 86, display: 'flex', flexDirection: 'column', overflow: 'hidden',
      background: 'var(--surface-raised)', border: `1px solid ${pinned ? 'var(--burgundy)' : 'var(--rule-strong)'}`, borderRadius: 8, boxShadow: '0 12px 30px rgba(60,40,10,.28)' }}>
    <div onMouseDown=${pinned ? gesture('move') : undefined} style=${{ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 8px 8px 10px', borderBottom: '1px solid var(--rule-soft)', cursor: pinned ? 'move' : 'default', flex: '0 0 auto' }}>
      <${Seal} pin=${pin} size=${24} />
      <div style=${{ flex: 1, minWidth: 0, fontFamily: 'var(--font-display)', fontSize: 14.5, fontWeight: 600, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${page?.title || pin.name}</div>
      <button onClick=${onPinToggle} onMouseDown=${(e) => e.stopPropagation()} title=${pinned ? 'Unpin' : 'Pin this card open'} style=${{ ...iconBtn, color: pinned ? 'var(--burgundy)' : 'var(--ink-muted)', background: pinned ? 'var(--burgundy-50)' : 'transparent' }}><${Icon} name="pin" size=${13} /></button>
      ${pinned && html`<button onClick=${onPinToggle} onMouseDown=${(e) => e.stopPropagation()} title="Close" style=${iconBtn}><${Icon} name="x" size=${13} /></button>`}
    </div>
    <div ref=${bodyRef} style=${{ flex: 1, minHeight: 0, overflow: 'auto', padding: '10px 14px 12px' }}>
      ${err && html`<div style=${{ fontSize: 12.5, color: 'var(--burgundy-700)' }}>${err}</div>`}
      ${!page && !err && html`<div style=${{ display: 'flex', justifyContent: 'center', padding: 14 }}><${Spinner} /></div>`}
      ${page && html`
        ${page.summary && html`<div style=${{ marginBottom: 8, fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 13, lineHeight: 1.5, color: 'var(--ink-soft)' }}>${page.summary}</div>`}
        <${PageBody} text=${page.content} pages=${store.vaultPages} />`}
    </div>
    <div style=${{ display: 'flex', alignItems: 'center', borderTop: '1px solid var(--rule-soft)', background: 'var(--paper-deep)', flex: '0 0 auto' }}>
      <div onClick=${onOpen} style=${{ flex: 1, padding: '7px 12px', fontSize: 11.5, fontWeight: 600, color: 'var(--ink-muted)', display: 'flex', alignItems: 'center', gap: 5, cursor: 'pointer' }}>
        <${Icon} name="doc" size=${11} /> Open page
      </div>
      ${pinned && html`<div onMouseDown=${gesture('resize')} title="Resize" style=${{ width: 22, height: 22, cursor: 'nwse-resize', display: 'flex', alignItems: 'flex-end', justifyContent: 'flex-end', padding: 3, color: 'var(--ink-faint)' }}>
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><path d="M9 3 3 9M9 6.5 6.5 9"/></svg>
      </div>`}
    </div>
  </div>`;
}

// Hover card (⌘ held over a pin) plus every pinned card of the map.
export function PreviewLayer({ keys, dispatch, hover, pins, view, W, H, stage, pinned, Seal, onSave, enabled = true }) {
  const [shown, setShown] = useState(null);
  const withPage = pins.filter((p) => p.page);
  const byId = Object.fromEntries(withPage.map((p) => [p.id, p]));

  useEffect(() => { dispatch({ type: 'hover', id: enabled && hover && byId[hover] ? hover : null }); }, [hover, enabled, pins]);

  const target = enabled ? previewTarget(keys) : null;
  useEffect(() => {
    if (target) { setShown(target); return undefined; }
    if (!keys.mod) { setShown(null); return undefined; }
    const t = setTimeout(() => setShown(null), 220); // room to travel from the pin onto its card
    return () => clearTimeout(t);
  }, [target, keys.mod]);

  const cards = normalizePreviews(pinned, withPage.map((p) => p.id), stage);
  const pinnedIds = new Set(cards.map((c) => c.pin_id));
  const floating = shown && byId[shown] && !pinnedIds.has(shown) ? byId[shown] : null;
  const open = (pin) => navigate('page', { path: splitPageRef(pin.page).path });

  const save = (pinId, rect) => onSave(withPreview(pinned, pinId, rect));
  const floatRect = () => {
    const p = floating;
    return placeNear(view.tx + p.x * W * view.zoom, view.ty + p.y * H * view.zoom, stage);
  };

  return html`<div>
    ${cards.map((c) => html`<${PreviewCard} key=${c.pin_id} pin=${byId[c.pin_id]} rect=${c} pinned Seal=${Seal}
      onPinToggle=${() => save(c.pin_id, null)} onLayout=${(rect) => save(c.pin_id, clampRect(rect, stage))}
      onEnter=${() => {}} onLeave=${() => {}} onOpen=${() => open(byId[c.pin_id])} />`)}
    ${floating && view && html`<${PreviewCard} key=${'f-' + floating.id} pin=${floating} rect=${floatRect()} pinned=${false} Seal=${Seal}
      onPinToggle=${() => save(floating.id, floatRect())} onLayout=${() => {}}
      onEnter=${() => dispatch({ type: 'card', id: floating.id })} onLeave=${() => dispatch({ type: 'card', id: null })}
      onOpen=${() => open(floating)} />`}
  </div>`;
}
