// Folder-name → codex kind recognition. Real vaults name folders in many
// languages and with ordering prefixes ("1 NPCs", "a_npcs", "01-Orte"), so we
// normalize the name and match words against a multilingual keyword table.
// Extend KEYWORDS to teach the Codex new vocabulary — kinds map to the icons
// and tones defined in screens/codex.js.

const KEYWORDS = {
  pc: [
    'pc', 'pcs', 'player', 'players', 'player characters', 'party',
    'spieler', 'spielercharaktere', 'helden', 'heroes', 'gruppe', 'gefaehrten',
    'pj', 'pjs', 'personnages joueurs', 'jugadores', 'giocatori',
  ],
  npc: [
    'npc', 'npcs', 'character', 'characters', 'people',
    'nsc', 'nscs', 'charaktere', 'personen', 'figuren', 'gestalten',
    'pnj', 'pnjs', 'personnages', 'personajes', 'personaggi', 'png',
  ],
  place: [
    'place', 'places', 'location', 'locations', 'city', 'cities', 'region', 'regions',
    'world', 'geography', 'map', 'maps', 'realm', 'realms', 'dungeon', 'dungeons',
    'ort', 'orte', 'staedte', 'stadt', 'regionen', 'welt', 'geographie', 'karten', 'laender', 'reiche',
    'lieux', 'endroits', 'lugares', 'luoghi', 'sitios',
  ],
  faction: [
    'faction', 'factions', 'organization', 'organizations', 'organisation', 'organisations',
    'guild', 'guilds', 'group', 'groups', 'order', 'orders',
    'fraktion', 'fraktionen', 'organisationen', 'gilden', 'gruppen', 'orden', 'haeuser',
    'facciones', 'fazioni', 'guildes',
  ],
  item: [
    'item', 'items', 'object', 'objects', 'artifact', 'artifacts', 'artefact', 'artefacts',
    'loot', 'treasure', 'treasures', 'equipment', 'relic', 'relics',
    'gegenstand', 'gegenstaende', 'objekt', 'objekte', 'artefakt', 'artefakte',
    'schaetze', 'schatz', 'ausruestung', 'relikte',
    'objets', 'objetos', 'oggetti', 'tesoros', 'tesori',
  ],
  event: [
    'event', 'events', 'ereignis', 'ereignisse', 'timeline', 'zeitlinie',
    'evenements', 'eventos', 'eventi',
  ],
  thread: [
    'thread', 'threads', 'plot', 'plots', 'quest', 'quests', 'storyline', 'storylines',
    'faden', 'faeden', 'handlung', 'handlungsstraenge',
  ],
  lore: [
    'lore', 'history', 'myth', 'myths', 'legend', 'legends', 'religion', 'religions',
    'god', 'gods', 'deities', 'cosmology', 'calendar',
    'background', 'knowledge', 'culture', 'cultures',
    'geschichte', 'mythen', 'legenden', 'goetter', 'gottheiten', 'kosmologie',
    'kalender', 'hintergrund', 'wissen', 'kulturen', 'sagen',
    'histoire', 'mythes', 'legendes', 'dieux', 'historia', 'mitos', 'leyendas', 'dioses', 'storia', 'miti',
  ],
};

const WORD_KIND = new Map();
const PHRASE_KIND = new Map();
for (const [kind, words] of Object.entries(KEYWORDS)) {
  for (const w of words) (w.includes(' ') ? PHRASE_KIND : WORD_KIND).set(w, kind);
}

// "1 NPCs" / "a_npcs" / "01-Orte" / "Städte" → "npcs" / "npcs" / "orte" / "staedte"
function normalize(name) {
  return name
    .toLowerCase()
    .replace(/ä/g, 'ae').replace(/ö/g, 'oe').replace(/ü/g, 'ue').replace(/ß/g, 'ss')
    .normalize('NFD').replace(/[̀-ͯ]/g, '')
    .replace(/^[^a-z]+/, '')              // ordering digits, emoji, punctuation
    .replace(/^[a-z][\s._-]+(?=\S)/, '')  // single-letter ordering prefix: "a_npcs"
    .trim();
}

const lookup = (w) => WORD_KIND.get(w) || (w.endsWith('s') ? WORD_KIND.get(w.slice(0, -1)) : WORD_KIND.get(w + 's'));

// Best-effort kind for a folder name; null when nothing matches (custom folder).
export function kindForFolder(name) {
  const n = normalize(name || '');
  if (!n) return null;
  if (PHRASE_KIND.has(n)) return PHRASE_KIND.get(n);
  for (const word of n.split(/[\s._\-/&+,]+/)) {
    const kind = lookup(word);
    if (kind) return kind;
  }
  return null;
}
