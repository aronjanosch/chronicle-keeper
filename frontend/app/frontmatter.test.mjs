import assert from 'node:assert/strict';
import { parseProps } from './ui.js';

const props = parseProps([
  'típus: város',
  'vezető: [[Ödön]]',
  'szövetségesek: [[Ilona]], [[Béla|a kovács]]',
  'rokon: [[Smith, John]]',
  'térkép: ![[Főváros.png]]',
  'lista: [[[Ödön]], "[[Ilona]]"]',
  'tags: [a, b]',
  'idő: 12:30',
  '  - nested',
  '# comment',
].join('\n'));
const byKey = Object.fromEntries(props.map((p) => [p.key, p.values]));

// Non-Latin keys are kept (issue #22).
assert.deepEqual(byKey['típus'], ['város']);
// An unquoted [[link]] stays a link instead of turning into a flow list.
assert.deepEqual(byKey['vezető'], ['[[Ödön]]']);
assert.deepEqual(byKey['szövetségesek'], ['[[Ilona]]', '[[Béla|a kovács]]']);
assert.deepEqual(byKey['rokon'], ['[[Smith, John]]']);
assert.deepEqual(byKey['térkép'], ['![[Főváros.png]]']);
assert.deepEqual(byKey.lista, ['[[Ödön]]', '[[Ilona]]']);
assert.deepEqual(byKey.tags, ['a', 'b']);
assert.deepEqual(byKey['idő'], ['12:30']);
assert.equal(props.length, 8);
