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

// Area centroid of a polygon; the vertex mean when the shape is degenerate.
export function centroid(points) {
  let a = 0, cx = 0, cy = 0;
  points.forEach((p, i) => {
    const q = points[(i + 1) % points.length];
    const cross = p[0] * q[1] - q[0] * p[1];
    a += cross; cx += (p[0] + q[0]) * cross; cy += (p[1] + q[1]) * cross;
  });
  if (Math.abs(a) < 1e-12) {
    return [points.reduce((s, p) => s + p[0], 0) / points.length, points.reduce((s, p) => s + p[1], 0) / points.length];
  }
  return [cx / (3 * a), cy / (3 * a)];
}

// Drop repeated points (a double-click closes a polygon after adding its last point twice).
export function dedupePoints(points, minPx, W, H) {
  return points.filter((p, i) => i === 0 || Math.hypot((p[0] - points[i - 1][0]) * W, (p[1] - points[i - 1][1]) * H) >= minPx);
}

// Point a new page at its parent: sets `part_of: "[[Title]]"` in the frontmatter
// unless the page already names a parent. Content without frontmatter is returned as is.
export function setPartOf(content, title) {
  const m = /^---\r?\n([\s\S]*?)\r?\n---/.exec(content || '');
  if (!m) return content;
  const line = `part_of: "[[${title}]]"`;
  const fm = m[1];
  const cur = /^part_of:[ \t]*(.*)$/m.exec(fm);
  if (cur && !/^(""|''|\[\])?\s*$/.test(cur[1])) return content;
  const nextFm = cur ? fm.replace(/^part_of:.*$/m, line) : `${fm}\n${line}`;
  const start = m[0].indexOf('\n') + 1;
  return content.slice(0, start) + nextFm + content.slice(start + fm.length);
}
