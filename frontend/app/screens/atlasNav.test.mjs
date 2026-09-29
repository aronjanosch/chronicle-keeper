import assert from 'node:assert/strict';
import { wheelAction, shortcutAction } from './atlasNav.js';
const w = (o) => ({ deltaMode: 0, deltaX: 0, deltaY: 0, ctrlKey: false, ...o });
// mouse notch
let a = wheelAction(w({ deltaY: 100 })); assert.equal(a.type, 'zoom'); assert.ok(a.factor < 1);
a = wheelAction(w({ deltaY: -120 })); assert.equal(a.type, 'zoom'); assert.ok(a.factor > 1);
// trackpad scroll: small / fractional / horizontal component
a = wheelAction(w({ deltaY: 7 })); assert.deepEqual(a, { type: 'pan', dx: -0, dy: -7 });
a = wheelAction(w({ deltaY: 12.5 })); assert.equal(a.type, 'pan');
a = wheelAction(w({ deltaX: 30, deltaY: 60 })); assert.equal(a.type, 'pan'); assert.equal(a.dx, -30);
// pinch always zooms, in every mode
for (const m of ['auto', 'mouse', 'trackpad']) assert.equal(wheelAction(w({ ctrlKey: true, deltaY: -4 }), m).type, 'zoom');
// forced modes
assert.equal(wheelAction(w({ deltaY: 5 }), 'mouse').type, 'zoom');
assert.equal(wheelAction(w({ deltaY: 100 }), 'trackpad').type, 'pan');
// line-mode wheels (Firefox) are wheels
assert.equal(wheelAction(w({ deltaMode: 1, deltaY: 3 })).type, 'zoom');
// shortcuts
const k = (key, o = {}) => ({ key, metaKey: false, ctrlKey: false, altKey: false, ...o });
assert.equal(shortcutAction(k('m')), 'measure'); assert.equal(shortcutAction(k('M')), 'measure');
assert.equal(shortcutAction(k('r')), 'region'); assert.equal(shortcutAction(k('?')), 'help');
assert.equal(shortcutAction(k('z', { metaKey: true })), null); assert.equal(shortcutAction(k('x')), null);
console.log('nav ok');
