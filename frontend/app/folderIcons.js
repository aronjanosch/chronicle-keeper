// Folder icons for the Codex. A folder shows, in order of preference: the icon
// the user picked (world prefs `folder_icons`), an icon guessed from its name
// ("Taverns" → mug, "Bestiary" → claws), its codex kind's icon, or a plain
// folder. Name matching reuses the kind normalizer's rules, so ordering
// prefixes and accents ("01 - Főváros") don't get in the way.

export const FOLDER_TONES = ['burgundy', 'moss', 'ink-blue', 'ochre', 'gilt'];

// Picker catalog, grouped. `words` teach the name matcher; `tone` is the
// colour used when the icon came from a name match.
export const FOLDER_ICON_GROUPS = [
  { label: 'Places', icons: [
    { icon: 'castle', label: 'Castle', tone: 'moss', words: ['capital', 'capitals', 'city', 'cities', 'town', 'towns', 'kingdom', 'kingdoms', 'stadt', 'staedte', 'hauptstadt', 'burg', 'burgen', 'fovaros', 'varos', 'varosok', 'ville', 'villes', 'ciudad', 'ciudades', 'citta'] },
    { icon: 'house', label: 'Village', tone: 'moss', words: ['village', 'villages', 'hamlet', 'hamlets', 'settlement', 'settlements', 'home', 'homes', 'dorf', 'doerfer', 'siedlung', 'siedlungen', 'falu', 'falvak', 'pueblo', 'pueblos', 'villaggio'] },
    { icon: 'mug', label: 'Tavern', tone: 'ochre', words: ['tavern', 'taverns', 'inn', 'inns', 'pub', 'pubs', 'taverne', 'tavernen', 'gasthaus', 'gasthaeuser', 'kneipe', 'kneipen', 'fogado', 'kocsma', 'auberge', 'taberna', 'food', 'drinks', 'recipes', 'essen', 'getraenke', 'rezepte', 'etelek', 'italok'] },
    { icon: 'tower', label: 'Tower', tone: 'ink-blue', words: ['tower', 'towers', 'keep', 'keeps', 'fortress', 'fortresses', 'turm', 'tuerme', 'festung', 'festungen'] },
    { icon: 'temple', label: 'Temple', tone: 'gilt', words: ['temple', 'temples', 'church', 'churches', 'shrine', 'shrines', 'tempel', 'kirche', 'kirchen', 'schrein', 'templom'] },
    { icon: 'door', label: 'Dungeon', tone: 'burgundy', words: ['dungeon', 'dungeons', 'crypt', 'crypts', 'kerker', 'verlies', 'verliese', 'gruft', 'kazamata'] },
    { icon: 'cave', label: 'Cave', tone: 'moss', words: ['cave', 'caves', 'underdark', 'mine', 'mines', 'hoehle', 'hoehlen', 'minen', 'barlang'] },
    { icon: 'mountain', label: 'Mountain', tone: 'moss', words: ['mountain', 'mountains', 'peak', 'peaks', 'berg', 'berge', 'gebirge', 'hegy', 'hegyek'] },
    { icon: 'tree', label: 'Forest', tone: 'moss', words: ['forest', 'forests', 'wood', 'woods', 'wilderness', 'wald', 'waelder', 'wildnis', 'erdo', 'erdok'] },
    { icon: 'ruin', label: 'Ruin', tone: 'ochre', words: ['ruin', 'ruins', 'ruine', 'ruinen', 'rom', 'romok'] },
    { icon: 'tent', label: 'Camp', tone: 'ochre', words: ['camp', 'camps', 'lager', 'tabor'] },
    { icon: 'ship', label: 'Ship', tone: 'ink-blue', words: ['ship', 'ships', 'fleet', 'fleets', 'navy', 'sea', 'seas', 'ocean', 'oceans', 'schiff', 'schiffe', 'flotte', 'meer', 'meere', 'hajo', 'hajok', 'tenger'] },
    { icon: 'anchor', label: 'Harbour', tone: 'ink-blue', words: ['harbor', 'harbors', 'harbour', 'harbours', 'port', 'ports', 'hafen', 'haefen', 'kikoto'] },
    { icon: 'bridge', label: 'Bridge', tone: 'ochre', words: ['bridge', 'bridges', 'road', 'roads', 'route', 'routes', 'bruecke', 'bruecken', 'strasse', 'strassen', 'utak'] },
    { icon: 'map', label: 'Map', tone: 'moss', words: ['map', 'maps', 'atlas', 'karte', 'karten', 'terkep', 'terkepek', 'helyek', 'helyszinek'] },
    { icon: 'compass', label: 'Compass', tone: 'moss', words: ['travel', 'travels', 'journey', 'journeys', 'exploration', 'reise', 'reisen', 'utazas'] },
    { icon: 'globe', label: 'World', tone: 'ink-blue', words: ['planes', 'plane', 'continent', 'continents', 'ebenen', 'kontinent', 'kontinente', 'vilag'] },
  ] },
  { label: 'People', icons: [
    { icon: 'users', label: 'People', tone: 'burgundy', words: ['races', 'species', 'peoples', 'folk', 'voelker', 'rassen', 'nepek', 'karakterek', 'szereplok'] },
    { icon: 'sparkle', label: 'Heroes', tone: 'gilt', words: [] },
    { icon: 'crown', label: 'Royalty', tone: 'gilt', words: ['royalty', 'nobility', 'nobles', 'noble', 'rulers', 'ruler', 'kings', 'king', 'queens', 'dynasty', 'dynasties', 'adel', 'koenige', 'herrscher', 'dynastie', 'nemesek', 'kiralyok'] },
    { icon: 'shield', label: 'Order', tone: 'ink-blue', words: [] },
    { icon: 'flag', label: 'Banner', tone: 'burgundy', words: ['nation', 'nations', 'army', 'armies', 'war', 'wars', 'nationen', 'armee', 'armeen', 'krieg', 'kriege', 'haboru'] },
    { icon: 'mask', label: 'Mask', tone: 'burgundy', words: ['villain', 'villains', 'rogue', 'rogues', 'thieves', 'thief', 'secret', 'secrets', 'intrigue', 'cult', 'cults', 'schurken', 'diebe', 'geheimnisse', 'kult', 'kulte', 'gonoszok', 'titkok'] },
    { icon: 'heart', label: 'Bonds', tone: 'burgundy', words: ['relationships', 'relationship', 'bonds', 'romance', 'family', 'families', 'beziehungen', 'familie', 'familien', 'csalad'] },
    { icon: 'music', label: 'Bard', tone: 'ochre', words: ['music', 'songs', 'song', 'bards', 'bard', 'poems', 'musik', 'lieder', 'barden', 'zene', 'dalok'] },
    { icon: 'hammer', label: 'Craft', tone: 'ochre', words: ['craft', 'crafting', 'smith', 'smithing', 'guilds', 'professions', 'handwerk', 'schmiede', 'berufe'] },
  ] },
  { label: 'Creatures & foes', icons: [
    { icon: 'claws', label: 'Monster', tone: 'burgundy', words: ['monster', 'monsters', 'bestiary', 'creature', 'creatures', 'beast', 'beasts', 'enemies', 'enemy', 'foes', 'kreaturen', 'bestiarium', 'gegner', 'feinde', 'szornyek', 'lenyek', 'ellensegek'] },
    { icon: 'skull', label: 'Undead', tone: 'burgundy', words: ['undead', 'death', 'dead', 'necromancy', 'graveyard', 'untote', 'tod', 'tote', 'nekromantie', 'halottak', 'elohalottak'] },
    { icon: 'paw', label: 'Animal', tone: 'moss', words: ['animals', 'animal', 'fauna', 'mounts', 'pets', 'companions', 'tiere', 'tier', 'reittiere', 'allatok'] },
    { icon: 'swords', label: 'Battle', tone: 'burgundy', words: ['encounter', 'encounters', 'battle', 'battles', 'combat', 'fights', 'begegnungen', 'kaempfe', 'kampf', 'schlachten', 'harc', 'csatak', 'osszecsapasok'] },
    { icon: 'tomb', label: 'Grave', tone: 'ink-blue', words: ['graves', 'tombs', 'tomb', 'cemetery', 'graeber', 'friedhof', 'sirok', 'temeto'] },
  ] },
  { label: 'Things', icons: [
    { icon: 'gem', label: 'Gem', tone: 'ochre', words: [] },
    { icon: 'chest', label: 'Treasure', tone: 'ochre', words: [] },
    { icon: 'coins', label: 'Coins', tone: 'gilt', words: ['shop', 'shops', 'merchant', 'merchants', 'market', 'markets', 'economy', 'trade', 'gold', 'money', 'laeden', 'haendler', 'markt', 'handel', 'wirtschaft', 'geld', 'bolt', 'boltok', 'kereskedok', 'penz'] },
    { icon: 'sword', label: 'Weapon', tone: 'ink-blue', words: ['weapon', 'weapons', 'arms', 'waffe', 'waffen', 'fegyver', 'fegyverek'] },
    { icon: 'bow', label: 'Bow', tone: 'moss', words: ['hunting', 'hunters', 'jagd', 'jaeger', 'vadaszat'] },
    { icon: 'key', label: 'Key', tone: 'gilt', words: ['keys', 'key', 'clues', 'clue', 'mysteries', 'mystery', 'schluessel', 'hinweise', 'raetsel', 'riddles', 'kulcsok', 'nyomok', 'rejtvenyek'] },
    { icon: 'ring', label: 'Ring', tone: 'gilt', words: ['ring', 'rings', 'jewelry', 'jewellery', 'ringe', 'schmuck', 'gyuruk', 'ekszerek'] },
    { icon: 'potion', label: 'Potion', tone: 'moss', words: ['potion', 'potions', 'alchemy', 'poison', 'poisons', 'traenke', 'trank', 'alchemie', 'gift', 'gifte', 'bajitalok', 'alkimia', 'mergek'] },
    { icon: 'leaf', label: 'Herb', tone: 'moss', words: ['herbs', 'herb', 'plants', 'plant', 'flora', 'nature', 'kraeuter', 'pflanzen', 'natur', 'novenyek', 'gyogynovenyek', 'termeszet'] },
    { icon: 'image', label: 'Images', tone: 'ink-blue', words: ['assets', 'asset', 'images', 'image', 'pictures', 'art', 'artwork', 'portraits', 'tokens', 'media', 'attachments', 'bilder', 'medien', 'anhaenge', 'kepek', 'portrek'] },
  ] },
  { label: 'Magic & lore', icons: [
    { icon: 'scroll', label: 'Scroll', tone: 'gilt', words: ['handouts', 'handout', 'letters', 'documents', 'briefe', 'dokumente', 'levelek'] },
    { icon: 'book', label: 'Book', tone: 'ink-blue', words: ['books', 'book', 'library', 'compendium', 'campaign', 'adventures', 'adventure', 'modules', 'buecher', 'bibliothek', 'abenteuer', 'kampagne', 'konyvek', 'kalandok'] },
    { icon: 'd20', label: 'Dice', tone: 'burgundy', words: ['rules', 'rule', 'mechanics', 'homebrew', 'system', 'rulebook', 'srd', 'regeln', 'regelwerk', 'hausregeln', 'mechaniken', 'szabalyok', 'szabalykonyv'] },
    { icon: 'scales', label: 'Law', tone: 'ink-blue', words: ['laws', 'law', 'politics', 'justice', 'gesetze', 'gesetz', 'politik', 'recht', 'torvenyek', 'politika'] },
    { icon: 'wand', label: 'Magic', tone: 'ink-blue', words: ['magic', 'spells', 'spell', 'arcana', 'rituals', 'ritual', 'magie', 'zauber', 'zaubersprueche', 'rituale', 'magia', 'varazslatok', 'varazslat'] },
    { icon: 'flame', label: 'Flame', tone: 'burgundy', words: ['elements', 'fire', 'feuer', 'elemente', 'tuz'] },
    { icon: 'sun', label: 'Sun', tone: 'gilt', words: ['gods', 'god', 'deities', 'deity', 'pantheon', 'faith', 'religion', 'religions', 'goetter', 'gottheiten', 'glaube', 'istenek', 'vallas'] },
    { icon: 'moon', label: 'Moon', tone: 'ink-blue', words: ['dreams', 'dream', 'night', 'astrology', 'traeume', 'nacht', 'astrologie', 'almok', 'ejszaka'] },
    { icon: 'star', label: 'Star', tone: 'gilt', words: ['cosmology', 'stars', 'heavens', 'prophecy', 'prophecies', 'kosmologie', 'sterne', 'prophezeiung', 'prophezeiungen', 'csillagok', 'joslatok'] },
    { icon: 'eye', label: 'Eye', tone: 'burgundy', words: ['spies', 'spy', 'visions', 'oracle', 'oracles', 'spione', 'visionen', 'orakel', 'kemek', 'latomasok'] },
    { icon: 'hourglass', label: 'Time', tone: 'ochre', words: ['history', 'eras', 'era', 'ages', 'chronicle', 'chronicles', 'epochen', 'zeitalter', 'chronik', 'chroniken', 'tortenelem', 'korok', 'kronika'] },
    { icon: 'cal', label: 'Calendar', tone: 'ochre', words: ['calendar', 'holidays', 'festivals', 'kalender', 'feiertage', 'feste', 'naptar', 'unnepek'] },
    { icon: 'feather', label: 'Quill', tone: 'ink-blue', words: ['notes', 'note', 'journal', 'journals', 'diary', 'notizen', 'tagebuch', 'jegyzetek', 'naplo'] },
    { icon: 'archive', label: 'Archive', tone: 'ink-blue', words: ['archive', 'archives', 'old', 'misc', 'archiv', 'sonstiges', 'regi', 'egyeb'] },
  ] },
];

export const FOLDER_ICONS = FOLDER_ICON_GROUPS.flatMap((g) => g.icons);

const WORD_ICON = new Map();
for (const entry of FOLDER_ICONS) {
  for (const w of entry.words) if (!WORD_ICON.has(w)) WORD_ICON.set(w, entry);
}

// Same shape as folderKinds.normalize: lowercase, umlauts spelled out, accents
// stripped, ordering prefixes ("01 - ", "a_") dropped.
function normalize(name) {
  return String(name || '')
    .toLowerCase()
    .replace(/ä/g, 'ae').replace(/ö/g, 'oe').replace(/ü/g, 'ue').replace(/ß/g, 'ss')
    .normalize('NFD').replace(/[̀-ͯ]/g, '')
    .replace(/^[^a-z]+/, '')
    .replace(/^[a-z][\s._-]+(?=\S)/, '')
    .trim();
}

// Hungarian (and other) names lose their accents rather than gaining an "e":
// try both spellings so "Főváros" matches "fovaros".
const variants = (raw) => {
  const a = normalize(raw);
  const b = normalize(String(raw || '').replace(/[äöü]/gi, (c) => c.normalize('NFD')[0]));
  return a === b ? [a] : [a, b];
};

// Best icon guessed from a folder name; null when nothing fits.
export function iconForFolderName(name) {
  for (const n of variants(name)) {
    if (!n) continue;
    if (WORD_ICON.has(n)) return WORD_ICON.get(n);
    for (const word of n.split(/[\s._\-/&+,]+/)) {
      if (WORD_ICON.has(word)) return WORD_ICON.get(word);
    }
  }
  return null;
}

// Resolve what a folder shows: { icon, tone, custom } or null for a plain folder.
// `kindIcon`/`kindTone` describe the folder's codex kind, when it has one.
export function folderGlyph({ path, name, custom, kind, kindIcon, kindTone }) {
  const picked = custom && custom[path];
  if (picked && picked.icon) return { icon: picked.icon, tone: picked.tone || kindTone || 'burgundy', custom: true };
  const byName = iconForFolderName(name);
  if (byName) return { icon: byName.icon, tone: kind ? kindTone : byName.tone, custom: false };
  if (kind) return { icon: kindIcon, tone: kindTone, custom: false };
  return null;
}

// Keep custom icons attached to their folders across rename/move; drop them
// on delete (`to` null).
export function remapFolderIcons(icons, from, to) {
  const out = {};
  for (const [path, v] of Object.entries(icons || {})) {
    const inside = path === from || path.startsWith(`${from}/`);
    if (!inside) out[path] = v;
    else if (to != null) out[to + path.slice(from.length)] = v;
  }
  return out;
}
