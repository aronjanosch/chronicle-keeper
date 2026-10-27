// Full-screen image viewer: darkened backdrop, ←/→ between the images of the
// page or folder it was opened from, wheel/click to zoom, drag to pan.
import { html, useState, useEffect, useRef } from '../vendor/htm-preact-standalone.mjs';
import { store, closeModal, openModal, apiBlob } from './core.js';
import { Icon } from './ui.js';

// items: [{ asset: 'Assets/x.webp' } | { src: 'https://…' }, label?]
export function openLightbox(items, index = 0) {
  if (!items || !items.length) return;
  openModal('lightbox', { items, index: Math.max(0, Math.min(index, items.length - 1)) });
}

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i;
export const isImagePath = (p) => IMAGE_EXT.test(p || '');

const labelOf = (item) => item.label || (item.asset || item.src || '').split('/').pop().split('?')[0];

function useItemSrc(item) {
  const [src, setSrc] = useState(item.src || null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    setFailed(false);
    if (!item.asset) { setSrc(item.src || null); return undefined; }
    setSrc(null);
    const cid = store.campaign?.campaign_id;
    let dead = false;
    let u = null;
    apiBlob(`/campaigns/${cid}/vault/assets/${encodeURI(item.asset)}`)
      .then((b) => { if (dead) return; u = URL.createObjectURL(b); setSrc(u); })
      .catch(() => { if (!dead) setFailed(true); });
    return () => { dead = true; if (u) URL.revokeObjectURL(u); };
  }, [item.asset, item.src]);
  return { src, failed };
}

const btn = {
  width: 40, height: 40, borderRadius: 999, border: 'none', cursor: 'pointer',
  background: 'rgba(255,255,255,.1)', color: '#f4ede2',
  display: 'flex', alignItems: 'center', justifyContent: 'center',
};

export function LightboxModal({ items, index: start = 0 }) {
  const [index, setIndex] = useState(start);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const drag = useRef(null);
  const item = items[index] || items[0];
  const { src, failed } = useItemSrc(item);
  const many = items.length > 1;

  const go = (d) => { setIndex((i) => (i + d + items.length) % items.length); setZoom(1); setPan({ x: 0, y: 0 }); };
  const zoomTo = (z) => { const n = Math.max(1, Math.min(6, z)); setZoom(n); if (n === 1) setPan({ x: 0, y: 0 }); };

  useEffect(() => {
    const onKey = (e) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeModal(); }
      else if (e.key === 'ArrowRight' && many) { e.preventDefault(); go(1); }
      else if (e.key === 'ArrowLeft' && many) { e.preventDefault(); go(-1); }
      else if (e.key === '+' || e.key === '=') zoomTo(zoom * 1.5);
      else if (e.key === '-') zoomTo(zoom / 1.5);
      else if (e.key === '0') zoomTo(1);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });

  const onWheel = (e) => { e.preventDefault(); zoomTo(zoom * (e.deltaY < 0 ? 1.15 : 1 / 1.15)); };
  const onPointerDown = (e) => {
    if (zoom === 1) return;
    e.preventDefault();
    drag.current = { x: e.clientX - pan.x, y: e.clientY - pan.y, moved: false };
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };
  const onPointerMove = (e) => {
    if (!drag.current) return;
    drag.current.moved = true;
    setPan({ x: e.clientX - drag.current.x, y: e.clientY - drag.current.y });
  };
  const onPointerUp = () => { setTimeout(() => { drag.current = null; }, 0); };
  const onImgClick = (e) => {
    e.stopPropagation();
    if (drag.current?.moved) return;
    zoomTo(zoom === 1 ? 2.5 : 1);
  };

  return html`<div role="dialog" aria-label="Image viewer" onClick=${closeModal} onWheel=${onWheel}
    style=${{ position: 'fixed', inset: 0, zIndex: 950, background: 'rgba(14,11,9,.9)', display: 'flex', flexDirection: 'column', animation: 'ck-fade .14s ease-out', userSelect: 'none' }}>
    <div onClick=${(e) => e.stopPropagation()} style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 16px', color: '#f4ede2' }}>
      <span style=${{ fontSize: 13, fontFamily: 'var(--font-mono)', flex: 1, minWidth: 0, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', opacity: 0.85 }}>${labelOf(item)}</span>
      ${many && html`<span class="ck-num" style=${{ fontSize: 12.5, opacity: 0.7 }}>${index + 1} / ${items.length}</span>`}
      <button title="Zoom out (−)" style=${btn} onClick=${() => zoomTo(zoom / 1.5)}><span style=${{ fontSize: 20, lineHeight: 1 }}>−</span></button>
      <button title="Reset zoom (0)" style=${{ ...btn, width: 56, fontSize: 12 }} onClick=${() => zoomTo(1)}>${Math.round(zoom * 100)}%</button>
      <button title="Zoom in (+)" style=${btn} onClick=${() => zoomTo(zoom * 1.5)}><${Icon} name="plus" size=${16} /></button>
      <button title="Close (Esc)" style=${btn} onClick=${closeModal}><${Icon} name="x" size=${16} /></button>
    </div>
    <div style=${{ flex: 1, minHeight: 0, position: 'relative', display: 'flex', alignItems: 'center', justifyContent: 'center', overflow: 'hidden', padding: '0 72px 28px' }}>
      ${failed
        ? html`<div style=${{ color: '#f4ede2', opacity: 0.7, fontSize: 14 }}>Could not load this image.</div>`
        : src && html`<img src=${src} alt=${labelOf(item)} draggable=${false}
            onClick=${onImgClick} onPointerDown=${onPointerDown} onPointerMove=${onPointerMove} onPointerUp=${onPointerUp}
            style=${{ maxWidth: '100%', maxHeight: '100%', objectFit: 'contain', borderRadius: 4, boxShadow: '0 12px 48px rgba(0,0,0,.5)',
              transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`, transition: drag.current ? 'none' : 'transform .12s ease-out',
              cursor: zoom === 1 ? 'zoom-in' : (drag.current ? 'grabbing' : 'grab') }} />`}
      ${many && html`
        <button title="Previous (←)" style=${{ ...btn, position: 'absolute', left: 16, top: '50%', marginTop: -20 }} onClick=${(e) => { e.stopPropagation(); go(-1); }}><${Icon} name="chev-l" size=${18} /></button>
        <button title="Next (→)" style=${{ ...btn, position: 'absolute', right: 16, top: '50%', marginTop: -20 }} onClick=${(e) => { e.stopPropagation(); go(1); }}><${Icon} name="chev-r" size=${18} /></button>`}
    </div>
  </div>`;
}
