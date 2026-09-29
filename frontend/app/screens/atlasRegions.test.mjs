import assert from 'node:assert/strict';
import { centroid, dedupePoints, setPartOf } from './atlasGeom.js';

// centroid of a unit square and a triangle
const sq = centroid([[0, 0], [1, 0], [1, 1], [0, 1]]);
assert.ok(Math.abs(sq[0] - 0.5) < 1e-9 && Math.abs(sq[1] - 0.5) < 1e-9);
const tri = centroid([[0, 0], [0.6, 0], [0, 0.3]]);
assert.ok(Math.abs(tri[0] - 0.2) < 1e-9 && Math.abs(tri[1] - 0.1) < 1e-9);
// clockwise winding gives the same answer
const cw = centroid([[0, 0], [0, 1], [1, 1], [1, 0]]);
assert.ok(Math.abs(cw[0] - 0.5) < 1e-9 && Math.abs(cw[1] - 0.5) < 1e-9);
// degenerate: collinear points fall back to the mean
const line = centroid([[0, 0], [0.5, 0.5], [1, 1]]);
assert.ok(Math.abs(line[0] - 0.5) < 1e-9);

// dedupe: a double-click adds the last vertex twice
const d = dedupePoints([[0.1, 0.1], [0.5, 0.1], [0.5, 0.1], [0.3, 0.6], [0.3, 0.6]], 2, 1000, 1000);
assert.equal(d.length, 3);

// setPartOf: fills an empty key, appends when missing, never overwrites a value
const empty = '---\nkind: place\nsummary:\npart_of:\n---\n\n# Reach\n';
assert.equal(setPartOf(empty, 'Aethric'), '---\nkind: place\nsummary:\npart_of: "[[Aethric]]"\n---\n\n# Reach\n');
const quotedEmpty = '---\nkind: place\npart_of: ""\n---\nbody';
assert.ok(setPartOf(quotedEmpty, 'A').includes('part_of: "[[A]]"'));
const missing = '---\nkind: place\n---\nbody';
assert.equal(setPartOf(missing, 'A'), '---\nkind: place\npart_of: "[[A]]"\n---\nbody');
const set = '---\nkind: place\npart_of: "[[Other]]"\n---\nbody';
assert.equal(setPartOf(set, 'A'), set);
const list = '---\npart_of: []\n---\nbody';
assert.ok(setPartOf(list, 'A').includes('"[[A]]"'));
assert.equal(setPartOf('no frontmatter', 'A'), 'no frontmatter');
const crlf = '---\r\nkind: place\r\npart_of:\r\n---\r\nbody';
assert.ok(setPartOf(crlf, 'A').includes('part_of: "[[A]]"'));
assert.ok(setPartOf(crlf, 'A').endsWith('---\r\nbody'));
const emptyFm = '---\n\n---\nbody';
assert.ok(setPartOf(emptyFm, 'A').includes('part_of: "[[A]]"'));
console.log('region ok');
