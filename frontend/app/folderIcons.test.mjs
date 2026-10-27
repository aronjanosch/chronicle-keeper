import assert from 'node:assert/strict';
import { iconForFolderName, folderGlyph, remapFolderIcons, FOLDER_ICONS } from './folderIcons.js';

assert.equal(iconForFolderName('Taverns').icon, 'mug');
assert.equal(iconForFolderName('03 - Falu').icon, 'house');
assert.equal(iconForFolderName('01 - Főváros').icon, 'castle');
assert.equal(iconForFolderName('Assets').icon, 'image');
assert.equal(iconForFolderName('Rules').icon, 'd20');
assert.equal(iconForFolderName('a_Bestiary').icon, 'claws');
assert.equal(iconForFolderName('Zaubersprüche').icon, 'wand');
assert.equal(iconForFolderName('Riddles of Old'), iconForFolderName('Riddles'));
assert.equal(iconForFolderName('Misc stuff 2').icon, 'archive');
assert.equal(iconForFolderName('Zorblax'), null);

// Picker never shows the same glyph twice.
assert.equal(new Set(FOLDER_ICONS.map((i) => i.icon)).size, FOLDER_ICONS.length);

const kind = { kind: 'place', kindIcon: 'map', kindTone: 'moss' };
assert.deepEqual(folderGlyph({ path: 'Places', name: 'Places', ...kind }), { icon: 'map', tone: 'moss', custom: false });
assert.deepEqual(folderGlyph({ path: 'Rules', name: 'Rules' }), { icon: 'd20', tone: 'burgundy', custom: false });
assert.deepEqual(folderGlyph({ path: 'Places', name: 'Places', custom: { Places: { icon: 'ship', tone: 'ink-blue' } }, ...kind }),
  { icon: 'ship', tone: 'ink-blue', custom: true });
assert.equal(folderGlyph({ path: 'Zorblax', name: 'Zorblax' }), null);

const icons = { Places: { icon: 'map' }, 'Places/Inns': { icon: 'mug' }, PlacesOld: { icon: 'ruin' } };
assert.deepEqual(remapFolderIcons(icons, 'Places', 'World/Places'),
  { 'World/Places': { icon: 'map' }, 'World/Places/Inns': { icon: 'mug' }, PlacesOld: { icon: 'ruin' } });
assert.deepEqual(remapFolderIcons(icons, 'Places', null), { PlacesOld: { icon: 'ruin' } });
