// Pure geometry for Atlas annotations. Points are normalised (0..1) map
// coordinates; distances are measured in image pixels (× W, × H) so a brush
// radius means the same on both axes. No imports — unit-testable in node.

const px = (p, W, H) => [p[0] * W, p[1] * H];

function distToSegment(p, a, b) {
  const dx = b[0] - a[0], dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  const t = len2 ? Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2)) : 0;
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

// Drop points closer than `minDist` px to the previous kept point.
export function thinPoint(points, next, minDist, W, H) {
  if (!points.length) return true;
  const a = px(points[points.length - 1], W, H), b = px(next, W, H);
  return Math.hypot(a[0] - b[0], a[1] - b[1]) >= minDist;
}

// Erase a freehand stroke with a round brush. null = the brush touched nothing;
// [] = fully erased; otherwise the surviving runs of >= 2 points.
export function splitStrokeByBrush(points, center, radius, W, H) {
  const c = px(center, W, H);
  const hit = points.map((p) => Math.hypot(px(p, W, H)[0] - c[0], px(p, W, H)[1] - c[1]) <= radius);
  if (!hit.some(Boolean)) return null;
  const runs = [];
  let cur = [];
  points.forEach((p, i) => {
    if (hit[i]) { if (cur.length) runs.push(cur); cur = []; } else cur.push(p);
  });
  if (cur.length) runs.push(cur);
  return runs.filter((r) => r.length >= 2);
}

// Does a round brush touch a whole-shape drawing (line / rect / circle / stamp)?
export function brushHitsShape(d, center, radius, W, H, zoom = 1) {
  const c = px(center, W, H);
  const p = d.points.map((q) => px(q, W, H));
  const slack = radius + (d.width || 0) / 2 / zoom;
  if (d.kind === 'stamp') return Math.hypot(p[0][0] - c[0], p[0][1] - c[1]) <= radius + (d.width || 0) / 2 / zoom;
  if (d.kind === 'line') return distToSegment(c, p[0], p[1]) <= slack;
  if (d.kind === 'rect') {
    const [a, b] = p;
    const corners = [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]];
    return corners.some((k, i) => distToSegment(c, k, corners[(i + 1) % 4]) <= slack);
  }
  if (d.kind === 'circle') {
    const r = Math.hypot(p[1][0] - p[0][0], p[1][1] - p[0][1]);
    return Math.abs(Math.hypot(c[0] - p[0][0], c[1] - p[0][1]) - r) <= slack;
  }
  return false;
}

// Apply one brush dab to a map's drawings + texts. `changed` is false when
// nothing was touched, so the caller can skip a store write.
export function eraseAt({ drawings, texts }, center, radius, W, H, zoom = 1) {
  let changed = false;
  const nextDrawings = [];
  for (const d of drawings) {
    if (d.kind === 'pen') {
      const runs = splitStrokeByBrush(d.points, center, radius, W, H);
      if (runs === null) { nextDrawings.push(d); continue; }
      changed = true;
      runs.forEach((points, i) => nextDrawings.push({ ...d, id: i === 0 ? d.id : `${d.id}.${i}`, points }));
    } else if (brushHitsShape(d, center, radius, W, H, zoom)) {
      changed = true;
    } else nextDrawings.push(d);
  }
  const c = px(center, W, H);
  const nextTexts = texts.filter((t) => {
    const hit = Math.hypot(t.x * W - c[0], t.y * H - c[1]) <= radius + (t.size || 0) / zoom;
    if (hit) changed = true;
    return !hit;
  });
  return { drawings: nextDrawings, texts: nextTexts, changed };
}

// Build a drawing from a finished gesture; null when it is too small to keep.
export function finishDrawing(kind, points, minPx, W, H) {
  if (kind === 'pen') return points.length >= 2 ? points : null;
  if (points.length < 2) return null;
  const a = px(points[0], W, H), b = px(points[points.length - 1], W, H);
  return Math.hypot(a[0] - b[0], a[1] - b[1]) >= minPx ? [points[0], points[points.length - 1]] : null;
}

export const clamp01 = (v) => Math.min(1, Math.max(0, v));
