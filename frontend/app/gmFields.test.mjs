import assert from 'node:assert/strict';
import { gmFieldList, isGmPage, setGmField, setGmPage } from './gmFields.js';

const page = '---\nkind: npc\nrole: Mayor\n---\n\nBody\n';
let p = setGmField(page, 'role', true);
assert.equal(p, '---\nkind: npc\nrole: Mayor\ngm_fields: [role]\n---\n\nBody\n');
p = setGmField(p, 'faction', true);
assert.deepEqual(gmFieldList(p.split('---')[1]), ['role', 'faction']);
p = setGmField(p, 'role', false);
assert.match(p, /gm_fields: \[faction\]/);
assert.equal(setGmField(p, 'faction', false), page.replace('\n\nBody', '\n\nBody'));

const block = '---\nkind: npc\ngm_fields:\n  - a\n  - b\ntags: [x]\n---\nB';
assert.deepEqual(gmFieldList('kind: npc\ngm_fields:\n  - a\n  - b\ntags: [x]'), ['a', 'b']);
const cleared = setGmField(setGmField(block, 'a', false), 'b', false);
assert.ok(!cleared.includes('gm_fields') && cleared.includes('tags: [x]'), cleared);

assert.equal(isGmPage('gm_only: true'), true);
assert.equal(isGmPage('gm_only: false'), false);
const g = setGmPage(page, true);
assert.match(g, /gm_only: true/);
assert.equal(setGmPage(g, false), page);
assert.equal(setGmPage('no frontmatter', true), '---\ngm_only: true\n---\n\nno frontmatter');
console.log('gmFields ok');
