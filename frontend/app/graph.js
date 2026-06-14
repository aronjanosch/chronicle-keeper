// The graph (Phase 9D): hand-rolled force layout + canvas renderer — a vendored
// d3-force would buy little over these ~90 lines, and there's no build step.
// Edges come from page_links (gray) and typed page_relations (burgundy).
import { html, useEffect, useRef } from '../vendor/htm-preact-standalone.mjs';

export const KIND_COLOR = {
  pc: '#B8924A', npc: '#7A2E1F', place: '#4A5D3A', faction: '#355370',
  item: '#A87328', event: '#B47465', lore: '#8B7B5E',
};
const DEFAULT_COLOR = '#A89880';

export function colorForKind(k) {
  return KIND_COLOR[k] || DEFAULT_COLOR;
}

// Nodes for every page; one edge per linked pair (typed relations win the
// styling when both exist). Returns { nodes, edges } ready for layout.
export function buildGraph(pages, links, relations) {
  const ring = Math.max(260, (pages || []).length * 4);
  const nodes = (pages || []).map((p, i) => ({
    path: p.path, title: p.title, kind: p.kind,
    // deterministic ring start so layouts are stable run-to-run
    x: 400 + ring * Math.cos((i / Math.max(1, pages.length)) * 2 * Math.PI),
    y: 300 + ring * Math.sin((i / Math.max(1, pages.length)) * 2 * Math.PI),
    vx: 0, vy: 0, degree: 0,
  }));
  const byPath = new Map(nodes.map((n) => [n.path, n]));
  const seen = new Map(); // "a→b" → edge
  const add = (src, tgt, predicate) => {
    const a = byPath.get(src), b = byPath.get(tgt);
    if (!a || !b || a === b) return;
    const key = `${src}\n${tgt}`;
    const prev = seen.get(key);
    if (prev) { if (predicate && !prev.predicate) prev.predicate = predicate; return; }
    const e = { a, b, predicate: predicate || null };
    seen.set(key, e);
    a.degree++; b.degree++;
  };
  for (const l of links || []) { if (l.target_path) add(l.source_path, l.target_path); }
  for (const r of relations || []) { if (r.target_path) add(r.source_path, r.target_path, r.predicate); }
  return { nodes, edges: [...seen.values()] };
}

// One simulation tick: pairwise repulsion, spring along edges, center gravity.
export function tick(nodes, edges, cx, cy, alpha) {
  // constants scale with graph size so big worlds spread instead of clumping
  const N = nodes.length;
  const REPULSE = 2600 * (1 + N / 50), SPRING = 0.04;
  const LEN = 130 + 60 * Math.min(1, N / 80), GRAVITY = 0.012 / (1 + N / 100);
  for (let i = 0; i < nodes.length; i++) {
    const n = nodes[i];
    for (let j = i + 1; j < nodes.length; j++) {
      const m = nodes[j];
      let dx = n.x - m.x, dy = n.y - m.y;
      let d2 = dx * dx + dy * dy;
      if (d2 < 1) { dx = (i % 2 ? 1 : -1); dy = (j % 2 ? 1 : -1); d2 = 2; }
      const f = (REPULSE * alpha) / d2;
      const d = Math.sqrt(d2);
      n.vx += (dx / d) * f; n.vy += (dy / d) * f;
      m.vx -= (dx / d) * f; m.vy -= (dy / d) * f;
    }
    n.vx += (cx - n.x) * GRAVITY * alpha;
    n.vy += (cy - n.y) * GRAVITY * alpha;
  }
  for (const e of edges) {
    const dx = e.b.x - e.a.x, dy = e.b.y - e.a.y;
    const d = Math.max(1, Math.sqrt(dx * dx + dy * dy));
    const f = SPRING * alpha * (d - LEN);
    e.a.vx += (dx / d) * f; e.a.vy += (dy / d) * f;
    e.b.vx -= (dx / d) * f; e.b.vy -= (dy / d) * f;
  }
  for (const n of nodes) {
    n.x += n.vx; n.y += n.vy;
    n.vx *= 0.6; n.vy *= 0.6;
  }
}

const nodeRadius = (n) => 2.5 + Math.min(6, Math.sqrt(n.degree) * 1.3);

// Canvas world graph: animated cooldown, pan (drag empty space), drag nodes,
// zoom (wheel), click → select + spotlight neighbors, double-click →
// onOpen(path). Kind/orphan filters and search matches arrive as props;
// apiRef exposes { zoom, fit, relayout, focus } for external controls.
export function GraphCanvas({ nodes, edges, onOpen, onNodeMenu, focusPath, hiddenKinds, hiddenPaths, hideOrphans, matches, apiRef }) {
  const hostRef = useRef(null);
  const stateRef = useRef(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return undefined;
    const canvas = document.createElement('canvas');
    canvas.style.cssText = 'width:100%;height:100%;display:block;cursor:grab';
    host.appendChild(canvas);
    const ctx = canvas.getContext('2d');
    const st = { tx: 0, ty: 0, scale: 1, hover: null, sel: null, alpha: 1, raf: 0,
      cooling: false, hidden: new Set(), hiddenPaths: new Set(), hideOrphans: false, matches: null,
      dimT: 0, warmup: 0 };
    stateRef.current = st;

    const adj = new Map();
    for (const e of edges) {
      (adj.get(e.a) || adj.set(e.a, new Set()).get(e.a)).add(e.b);
      (adj.get(e.b) || adj.set(e.b, new Set()).get(e.b)).add(e.a);
    }
    const vis = (n) => !st.hidden.has(n.kind) && !st.hiddenPaths.has(n.path) && !(st.hideOrphans && n.degree === 0);

    const size = () => {
      const r = host.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      canvas.width = r.width * dpr;
      canvas.height = r.height * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      return r;
    };
    let rect = size();

    const edgeLabel = (e) => {
      ctx.font = 'italic 8.5px ui-monospace, monospace';
      const mx = (e.a.x + e.b.x) / 2, my = (e.a.y + e.b.y) / 2;
      ctx.strokeStyle = 'rgba(243,238,229,.9)';
      ctx.lineWidth = 3;
      ctx.strokeText(e.predicate, mx + 3, my - 3);
      ctx.fillStyle = 'rgba(122,46,31,.85)';
      ctx.fillText(e.predicate, mx + 3, my - 3);
    };

    const lerp = (a, b, t) => a + (b - a) * t;
    const ease = (t) => t * t * (3 - 2 * t); // smoothstep

    const draw = () => {
      ctx.clearRect(0, 0, rect.width, rect.height);
      ctx.save();
      ctx.translate(st.tx, st.ty);
      ctx.scale(st.scale, st.scale);
      const focus = st.sel || st.hover;
      const nbrs = focus ? adj.get(focus) : null;
      for (const e of edges) {
        if (!vis(e.a) || !vis(e.b)) continue;
        const lit = !focus || e.a === focus || e.b === focus;
        ctx.globalAlpha = lit ? 1 : lerp(1, 0.07, ease(st.dimT));
        ctx.strokeStyle = e.predicate ? 'rgba(90,72,65,.28)' : 'rgba(31,24,19,.10)';
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(e.a.x, e.a.y);
        ctx.lineTo(e.b.x, e.b.y);
        ctx.stroke();
        if (e.predicate && lit && (focus || st.scale > 2.2)) edgeLabel(e);
      }
      for (const n of nodes) {
        if (!vis(n)) continue;
        const lit = !focus || n === focus || (nbrs && nbrs.has(n));
        const match = st.matches && st.matches.has(n.path);
        ctx.globalAlpha = lit ? 1 : lerp(1, 0.12, ease(st.dimT));
        const r = nodeRadius(n);
        ctx.fillStyle = colorForKind(n.kind);
        ctx.beginPath();
        ctx.arc(n.x, n.y, r, 0, 2 * Math.PI);
        ctx.fill();
        if (n === st.sel || n.path === focusPath) {
          ctx.strokeStyle = '#5C2317';
          ctx.lineWidth = 2;
          ctx.stroke();
        } else if (match) {
          ctx.strokeStyle = '#A87328';
          ctx.lineWidth = 2;
          ctx.stroke();
        }
        // Obsidian-style labels: centered below the node, fading in with zoom
        // (hubs surface first) so dense worlds don't render as label soup.
        const active = match || n === st.hover || n === st.sel || n.path === focusPath;
        let la = active ? 1
          : focus ? (lit ? 0.9 : 0)
          : Math.min(1, (st.scale - 0.55) * 2 + Math.sqrt(n.degree) * 0.3);
        if (la > 0.02) {
          ctx.globalAlpha = (lit ? 1 : lerp(1, 0.12, ease(st.dimT))) * la;
          ctx.font = `${active ? 600 : 400} 9px ui-monospace, monospace`;
          ctx.textAlign = 'center';
          ctx.strokeStyle = 'rgba(243,238,229,.85)';
          ctx.lineWidth = 3;
          ctx.strokeText(n.title, n.x, n.y + r + 10);
          ctx.fillStyle = active ? '#1F1813' : 'rgba(31,24,19,.72)';
          ctx.fillText(n.title, n.x, n.y + r + 10);
          ctx.textAlign = 'start';
          ctx.globalAlpha = lit ? 1 : 0.12;
        }
      }
      ctx.globalAlpha = 1;
      ctx.restore();
    };

    let focusRaf = 0;
    const animFocus = () => {
      const target = (st.sel || st.hover) ? 1 : 0;
      const diff = target - st.dimT;
      if (Math.abs(diff) > 0.004) {
        st.dimT += diff * 0.14;
        draw();
        focusRaf = requestAnimationFrame(animFocus);
      } else {
        st.dimT = target;
        draw();
      }
    };

    const cool = () => {
      if (st.alpha > 0.02) {
        st.warmup = Math.min(1, st.warmup + 1 / 35);
        const an = nodes.filter(vis);
        const ae = edges.filter((e) => vis(e.a) && vis(e.b));
        for (let k = 0; k < 3; k++) tick(an, ae, rect.width / (2 * st.scale), rect.height / (2 * st.scale), st.alpha * st.warmup);
        st.alpha *= 0.985;
        draw();
        st.raf = requestAnimationFrame(cool);
      } else {
        st.cooling = false;
        draw();
      }
    };
    const heat = (a) => {
      st.alpha = Math.max(st.alpha, a);
      if (!st.cooling) { st.cooling = true; st.raf = requestAnimationFrame(cool); }
    };
    st.draw = draw;
    st.heat = heat;
    st.cooling = true;
    st.raf = requestAnimationFrame(cool);

    const toWorld = (e) => {
      const r = canvas.getBoundingClientRect();
      return { x: (e.clientX - r.left - st.tx) / st.scale, y: (e.clientY - r.top - st.ty) / st.scale };
    };
    const nodeAt = (p) => {
      let best = null, bd = Infinity;
      for (const n of nodes) {
        if (!vis(n)) continue;
        const d = (n.x - p.x) ** 2 + (n.y - p.y) ** 2;
        if (d < bd) { bd = d; best = n; }
      }
      return best && bd <= (nodeRadius(best) + 6) ** 2 ? best : null;
    };

    const clampScale = (s) => Math.min(4, Math.max(0.2, s));
    st.api = {
      zoom(factor) {
        st.interacted = true;
        const px = rect.width / 2, py = rect.height / 2;
        const next = clampScale(st.scale * factor);
        st.tx = px - ((px - st.tx) / st.scale) * next;
        st.ty = py - ((py - st.ty) / st.scale) * next;
        st.scale = next;
        draw();
      },
      fit() {
        const an = nodes.filter(vis);
        if (!an.length) return;
        let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
        for (const n of an) {
          x0 = Math.min(x0, n.x); y0 = Math.min(y0, n.y);
          x1 = Math.max(x1, n.x); y1 = Math.max(y1, n.y);
        }
        const pad = 60;
        st.scale = clampScale(Math.min((rect.width - 2 * pad) / Math.max(1, x1 - x0), (rect.height - 2 * pad) / Math.max(1, y1 - y0)));
        st.tx = rect.width / 2 - st.scale * (x0 + x1) / 2;
        st.ty = rect.height / 2 - st.scale * (y0 + y1) / 2;
        draw();
      },
      relayout() {
        st.warmup = 0;
        const ring = Math.max(260, nodes.length * 4);
        nodes.forEach((n, i) => {
          n.x = rect.width / 2 + ring * Math.cos((i / Math.max(1, nodes.length)) * 2 * Math.PI);
          n.y = rect.height / 2 + ring * Math.sin((i / Math.max(1, nodes.length)) * 2 * Math.PI);
          n.vx = 0; n.vy = 0;
        });
        st.alpha = 0;
        heat(1);
      },
      focus(path) {
        const n = nodes.find((x) => x.path === path);
        if (!n) return;
        st.sel = n;
        st.tx = rect.width / 2 - st.scale * n.x;
        st.ty = rect.height / 2 - st.scale * n.y;
        cancelAnimationFrame(focusRaf);
        focusRaf = requestAnimationFrame(animFocus);
      },
    };
    if (apiRef) apiRef.current = st.api;

    let drag = null;
    const onDown = (e) => {
      const n = nodeAt(toWorld(e));
      drag = { node: n, x: e.clientX, y: e.clientY, moved: false };
      canvas.style.cursor = 'grabbing';
    };
    const onMove = (e) => {
      if (drag) {
        const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
        if (Math.abs(dx) + Math.abs(dy) > 2) drag.moved = true;
        if (drag.node) {
          const p = toWorld(e);
          drag.node.x = p.x; drag.node.y = p.y;
          drag.node.vx = 0; drag.node.vy = 0;
          heat(0.25);
        } else {
          st.interacted = true;
          st.tx += dx; st.ty += dy;
          drag.x = e.clientX; drag.y = e.clientY;
          draw();
        }
        return;
      }
      const h = nodeAt(toWorld(e));
      if (h !== st.hover) {
        st.hover = h; canvas.style.cursor = h ? 'pointer' : 'grab';
        cancelAnimationFrame(focusRaf);
        focusRaf = requestAnimationFrame(animFocus);
      }
    };
    const onUp = () => {
      if (!drag) return;
      if (!drag.moved) {
        st.sel = drag.node && drag.node !== st.sel ? drag.node : null;
        cancelAnimationFrame(focusRaf);
        focusRaf = requestAnimationFrame(animFocus);
      }
      drag = null;
      canvas.style.cursor = 'grab';
    };
    const onDbl = (e) => {
      const n = nodeAt(toWorld(e));
      if (n && onOpen) onOpen(n.path, e);
    };
    const onCtx = (e) => {
      const n = nodeAt(toWorld(e));
      if (n && onNodeMenu) onNodeMenu(n, e);
    };
    const onWheel = (e) => {
      e.preventDefault();
      st.interacted = true;
      const r = canvas.getBoundingClientRect();
      const px = e.clientX - r.left, py = e.clientY - r.top;
      const next = clampScale(st.scale * Math.exp(-e.deltaY * 0.0015));
      // zoom about the pointer
      st.tx = px - ((px - st.tx) / st.scale) * next;
      st.ty = py - ((py - st.ty) / st.scale) * next;
      st.scale = next;
      draw();
    };
    const onResize = () => { rect = size(); draw(); };

    canvas.addEventListener('mousedown', onDown);
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    canvas.addEventListener('dblclick', onDbl);
    canvas.addEventListener('contextmenu', onCtx);
    canvas.addEventListener('wheel', onWheel, { passive: false });
    window.addEventListener('resize', onResize);
    return () => {
      cancelAnimationFrame(st.raf);
      cancelAnimationFrame(focusRaf);
      canvas.removeEventListener('mousedown', onDown);
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      canvas.removeEventListener('dblclick', onDbl);
      canvas.removeEventListener('contextmenu', onCtx);
      canvas.removeEventListener('wheel', onWheel);
      window.removeEventListener('resize', onResize);
      canvas.remove();
    };
  }, [nodes, edges]);

  // Filters reheat the layout so visible nodes re-settle; search only redraws.
  useEffect(() => {
    const st = stateRef.current;
    if (!st) return;
    st.hidden = hiddenKinds || new Set();
    st.hiddenPaths = hiddenPaths || new Set();
    st.hideOrphans = !!hideOrphans;
    if (st.sel && (st.hidden.has(st.sel.kind) || st.hiddenPaths.has(st.sel.path))) st.sel = null;
    if (st.hover && st.hiddenPaths.has(st.hover.path)) st.hover = null;
    st.heat(0.3);
  }, [hiddenKinds, hiddenPaths, hideOrphans, nodes, edges]);
  useEffect(() => {
    const st = stateRef.current;
    if (!st) return;
    st.matches = matches || null;
    st.draw();
  }, [matches, nodes, edges]);

  return html`<div ref=${hostRef} style=${{ position: 'absolute', inset: 0 }} />`;
}
