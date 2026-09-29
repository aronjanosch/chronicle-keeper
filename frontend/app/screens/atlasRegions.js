// Atlas regions: drawn polygons that each stand for a place page. Rendered in
// the image-space layer under drawings and pins; labels keep a constant screen size.
import { html } from '../../vendor/htm-preact-standalone.mjs';
import { centroid } from './atlasGeom.js';

export const REGION_COLORS = [['Crimson', '#7A2E1F'], ['Moss', '#4A5D3A'], ['Ink blue', '#355370'], ['Ochre', '#A87328'], ['Plum', '#6B3E6B'], ['Slate', '#4B5563']];
const DEFAULT = '#7A2E1F';

export function RegionLayer({ regions, poly, polyHover, zoom, W, H, interactive, selectedId, onOpen, onMenu }) {
  const path = (pts) => pts.map((p) => `${p[0] * W},${p[1] * H}`).join(' ');
  const open = [...poly, ...(polyHover ? [polyHover] : [])];
  return html`<div style=${{ position: 'absolute', inset: 0, pointerEvents: 'none', zIndex: 11 }}>
    <svg width=${W} height=${H} style=${{ position: 'absolute', inset: 0, overflow: 'visible' }}>
      ${regions.map((r) => {
        const sel = selectedId === r.id;
        const c = r.color || DEFAULT;
        return html`<polygon key=${r.id} points=${path(r.points)} fill=${c} fill-opacity=${sel ? 0.34 : 0.18} stroke=${c} stroke-width=${(sel ? 2.5 : 1.5) / zoom} stroke-linejoin="round"
          style=${{ pointerEvents: interactive ? 'visiblePainted' : 'none', cursor: interactive ? 'pointer' : 'inherit' }}
          onClick=${(e) => onOpen(r, e)} onContextMenu=${(e) => onMenu(e, r)} />`;
      })}
      ${open.length > 0 && html`<polyline points=${path(open)} fill="none" stroke=${DEFAULT} stroke-width=${2 / zoom} stroke-dasharray=${`${6 / zoom} ${4 / zoom}`} stroke-linecap="round" stroke-linejoin="round" />`}
      ${poly.map((p, i) => html`<circle key=${i} cx=${p[0] * W} cy=${p[1] * H} r=${4 / zoom} fill="#FBF6E9" stroke=${DEFAULT} stroke-width=${1.5 / zoom} />`)}
    </svg>
    ${regions.filter((r) => r.name).map((r) => {
      const [cx, cy] = centroid(r.points);
      return html`<div key=${r.id} style=${{ position: 'absolute', left: `${cx * 100}%`, top: `${cy * 100}%`, transform: `translate(-50%,-50%) scale(${1 / zoom})`,
        whiteSpace: 'nowrap', fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 14, fontWeight: 500, color: '#1F1813', letterSpacing: '0.04em',
        textShadow: '0 0 3px rgba(251,246,233,.95), 0 0 7px rgba(251,246,233,.8)', pointerEvents: 'none' }}>${r.name}</div>`;
    })}
  </div>`;
}
