// Screen — New World (Phase 1.7-C). Recreates design/project/screens/new-world.jsx.
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, slugify, initials } from '../core.js';
import { addExampleWorld, createCampaign, pickVaultFolder, sniffVault } from '../actions.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Btn, Field, Input, Textarea } from '../ui.js';

const PRONOUNS = ['she/her', 'he/him', 'they/them'];
const SCAFFOLD_FOLDERS = ['NPCs', 'Places', 'Factions', 'Items', 'Lore'];
const TONE_SWATCHES = [
  { id: 'burgundy', bg: 'var(--burgundy-50)', color: 'var(--burgundy-700)' },
  { id: 'moss', bg: 'var(--moss-50)', color: 'var(--moss)' },
  { id: 'blue', bg: 'var(--ink-blue-50)', color: 'var(--ink-blue)' },
  { id: 'ochre', bg: 'var(--ochre-50)', color: 'var(--ochre)' },
];

const sectionLabel = {
  fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase',
  color: 'var(--burgundy)', marginBottom: 14,
};
const fieldLabel = {
  fontSize: 12, fontWeight: 600, color: 'var(--ink-soft)', letterSpacing: '0.02em', marginBottom: 6,
};
const panel = {
  background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, padding: '18px 20px',
};

function SectionLabel({ children }) {
  return html`<div style=${sectionLabel}>${children}</div>`;
}

function ToneSwatch({ tone, ch, active, onClick }) {
  const t = TONE_SWATCHES.find((x) => x.id === tone) || TONE_SWATCHES[0];
  return html`<button type="button" onClick=${onClick} title=${tone}
    style=${{
      width: 34, height: 34, borderRadius: 7, cursor: 'pointer', display: 'flex',
      alignItems: 'center', justifyContent: 'center', padding: 0,
      background: t.bg, color: t.color, fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 15,
      border: active ? `2px solid ${t.color}` : '1px solid rgba(0,0,0,.08)',
      boxShadow: active ? '0 0 0 3px rgba(122,46,31,.08)' : 'none',
    }}>${ch}</button>`;
}

function PronounSelect({ value, onChange }) {
  return html`<select value=${value || ''} onChange=${(e) => onChange(e.target.value)}
    style=${{ flex: '0 0 116px', padding: '7px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, fontFamily: 'inherit', color: 'var(--ink)', cursor: 'pointer' }}>
    <option value="">pronouns</option>
    ${PRONOUNS.map((p) => html`<option key=${p} value=${p}>${p}</option>`)}
  </select>`;
}

function PlayerRows({ players, onChange }) {
  const upd = (i, k, v) => onChange(players.map((p, j) => j === i ? { ...p, [k]: v } : p));
  return html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 6 }}>
    ${players.map((p, i) => html`<div key=${i} style=${{ display: 'flex', gap: 6 }}>
      <${Input} value=${p.player_name} placeholder="Player" onInput=${(v) => upd(i, 'player_name', v)} />
      <${Input} value=${p.character_name} placeholder="Character" onInput=${(v) => upd(i, 'character_name', v)} />
      <${PronounSelect} value=${p.pronouns} onChange=${(v) => upd(i, 'pronouns', v)} />
      <${Btn} kind=${p.is_gm ? 'primary' : 'ghost'} size="sm" title="Also runs the table — their voice is GM narration" onClick=${() => upd(i, 'is_gm', !p.is_gm)}>GM</${Btn}>
      <${Btn} kind="ghost" size="sm" icon="x" onClick=${() => onChange(players.filter((_, j) => j !== i))} />
    </div>`)}
    <${Btn} kind="ghost" size="sm" icon="plus" onClick=${() => onChange([...players, { player_name: '', character_name: '', pronouns: '', is_gm: false }])}>Add player</${Btn}>
  </div>`;
}

function VaultRadio({ checked, onSelect, title, description }) {
  const sel = {
    display: 'flex', alignItems: 'flex-start', gap: 10, padding: '11px 13px', borderRadius: 7,
    cursor: 'pointer', background: checked ? 'var(--burgundy-50)' : 'transparent',
    border: checked ? '1px solid rgba(122,46,31,.22)' : '1px solid var(--rule-soft)',
  };
  const dot = checked
    ? html`<span style=${{ width: 15, height: 15, borderRadius: '50%', border: '1.5px solid var(--burgundy)', background: 'var(--burgundy)', flex: '0 0 auto', marginTop: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <span style=${{ width: 5, height: 5, borderRadius: '50%', background: '#FBF6E9' }} />
      </span>`
    : html`<span style=${{ width: 15, height: 15, borderRadius: '50%', border: '1.5px solid var(--rule-strong)', flex: '0 0 auto', marginTop: 1 }} />`;
  return html`<div role="radio" aria-checked=${checked} onClick=${onSelect} style=${sel}>
    ${dot}
    <div>
      <div style=${{ fontSize: 13, fontWeight: 500, color: checked ? 'var(--burgundy-700)' : 'var(--ink)' }}>${title}</div>
      <div style=${{ fontSize: 11.5, color: 'var(--ink-muted)', marginTop: 1, lineHeight: 1.45 }}>${description}</div>
    </div>
  </div>`;
}

function TreeRow({ depth, icon, name, note, dim }) {
  return html`<div style=${{
    display: 'flex', alignItems: 'center', gap: 7, padding: '3px 0', paddingLeft: depth * 16,
    fontSize: 12, fontFamily: 'var(--font-mono)', color: dim ? 'var(--ink-faint)' : 'var(--ink-soft)',
  }}>
    <${Icon} name=${icon} size=${12} style=${{ color: dim ? 'var(--ink-faint)' : 'var(--ink-muted)' }} />
    <span>${name}</span>
    ${note && html`<span style=${{ marginLeft: 'auto', fontFamily: 'var(--font-ui)', fontSize: 10.5, color: 'var(--ink-faint)' }}>${note}</span>`}
  </div>`;
}

function previewRoot(f) {
  if (f.world_path?.trim()) {
    return f.world_path.replace(/\\/g, '/').split('/').filter(Boolean).pop() || f.world_path;
  }
  return slugify(f.name.trim()) || 'my-world';
}

function VaultPreview({ f, sniff }) {
  const root = previewRoot(f);
  const fresh = f.mode === 'fresh';
  const scaffold = fresh && f.scaffold;
  const adopting = !fresh && sniff;
  let rows;
  let note = 'Plain files you own. Open them in any editor; the app rebuilds its index from the folder.';
  if (adopting && sniff.mode === 'world') {
    rows = [{ depth: 0, icon: 'folder', name: `${root}/`, note: `${sniff.md_pages} pages` }];
    note = `A Chronicle Keeper world (“${sniff.name}”) — opens as-is, nothing is written.`;
  } else if (adopting && sniff.mode === 'foreign') {
    rows = [
      { depth: 0, icon: 'folder', name: `${root}/` },
      { depth: 1, icon: 'doc', name: `existing notes (${sniff.md_pages})`, note: 'untouched' },
      { depth: 1, icon: 'folder', name: 'Codex/', note: 'added' },
      { depth: 1, icon: 'folder', name: 'Sessions/', note: 'added' },
      { depth: 1, icon: 'cog', name: '.ck/config.toml', note: 'added', dim: true },
    ];
    note = 'Not a Chronicle Keeper world yet — a fresh Codex is set up next to the existing files. To bring the notes in, use Codex → Import notes afterwards.';
  } else {
    rows = [
      { depth: 0, icon: 'folder', name: `${root}/` },
      { depth: 1, icon: 'folder', name: 'Codex/', note: adopting && sniff.mode === 'ck-layout' ? `${sniff.md_pages} pages` : null },
      ...(scaffold ? SCAFFOLD_FOLDERS.map((f) => ({ depth: 2, icon: 'folder', name: `${f}/` })) : []),
      { depth: 1, icon: 'folder', name: 'Sessions/', note: fresh ? 'audio + notes' : null },
      { depth: 1, icon: 'cog', name: '.ck/config.toml', note: 'index + schema', dim: true },
    ];
  }
  return html`
    <div style=${panel}>
      ${rows.map((row, i) => html`<${TreeRow} key=${i} ...${row} />`)}
    </div>
    <div style=${{
      display: 'flex', alignItems: 'flex-start', gap: 9, marginTop: 14, padding: '11px 13px',
      background: 'var(--moss-50)', borderRadius: 7, fontSize: 11.5, color: 'var(--moss)', lineHeight: 1.5,
    }}>
      <${Icon} name="check" size=${13} style=${{ marginTop: 1, flex: '0 0 auto' }} />
      <span>${note}</span>
    </div>`;
}

export function NewWorldScreen() {
  const [f, setF] = useState({
    name: '', system: '', setting: '', gm: '', gm_pronouns: '',
    default_language: '', extra_info: '', start: 1, tone: 'burgundy',
    players: [{ player_name: '', character_name: '', pronouns: '' }],
    mode: 'fresh',
    world_path: '',
    scaffold: false,
    detailsOpen: false,
  });
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const [sniff, setSniff] = useState(null);
  const set = (k, v) => setF((s) => ({ ...s, [k]: v }));

  // Layout sniff for the adoption preview (debounced; typed paths too).
  useEffect(() => {
    const path = f.world_path.trim();
    if (f.mode !== 'existing' || !path) { setSniff(null); return; }
    const t = setTimeout(async () => {
      const s = await sniffVault(path);
      setSniff(s);
    }, 300);
    return () => clearTimeout(t);
  }, [f.mode, f.world_path]);

  const sigilCh = (initials(f.name.trim()) || '?').slice(0, 1);

  async function chooseFolder() {
    const picked = await pickVaultFolder();
    if (picked) set('world_path', picked);
    else {
      const typed = window.prompt('Enter world folder path:');
      if (typed?.trim()) set('world_path', typed.trim());
    }
  }

  async function addExample() {
    setBusy(true); setErr(null);
    try { await addExampleWorld(); }
    catch (e) { setErr(e.message); setBusy(false); }
  }

  async function submit() {
    if (!f.name.trim()) { setErr('World name is required'); return; }
    if (f.mode === 'existing' && !f.world_path.trim()) { setErr('Choose an existing folder first'); return; }
    setBusy(true); setErr(null);
    const players = f.players.filter((p) => p.player_name.trim() || p.character_name.trim());
    try {
      await createCampaign({
        ...f,
        name: f.name.trim(),
        players,
        vault_path: f.world_path.trim() || null,
        scaffold: f.mode === 'fresh' && f.scaffold,
      });
    } catch (e) {
      setErr(e.message);
      setBusy(false);
    }
  }

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="library" active="worlds" />`}
    topbar=${html`<${Topbar}
      crumbs=${[{ label: 'Worlds', onClick: () => navigate('library') }, 'New world']}
      right=${html`<div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <${Btn} kind="ghost" size="sm" disabled=${busy} onClick=${() => navigate('library')}>Cancel</${Btn}>
        <${Btn} kind="primary" size="sm" icon="globe" disabled=${busy} onClick=${submit}>
          ${busy ? 'Creating…' : 'Create world'}
        </${Btn}>
      </div>`}
    />`}
  >
    <div style=${{ maxWidth: 900, margin: '0 auto', paddingTop: 8 }}>
      <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>
        A new chronicle begins
      </div>
      <h1 style=${{
        fontFamily: 'var(--font-display)', fontSize: 30, fontWeight: 500, letterSpacing: '-0.02em',
        lineHeight: 1.1, color: 'var(--ink)', marginTop: 4, marginBottom: 0,
      }}>
        Begin a new <em style=${{ color: 'var(--burgundy)', fontStyle: 'italic' }}>world</em>
      </h1>
      <div style=${{ fontSize: 13, color: 'var(--ink-muted)', marginTop: 6, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>
        Name it, set the system, and choose where its pages will live. Everything is yours, on disk.
      </div>

      ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13, padding: '10px 14px', background: '#FBEDE9', borderRadius: 6, marginTop: 16 }}>${err}</div>`}

      <div style=${{ display: 'grid', gridTemplateColumns: '1fr 320px', gap: 22, marginTop: 26 }}>
        <div style=${{ display: 'flex', flexDirection: 'column', gap: 22 }}>
          <div style=${panel}>
            <${SectionLabel}>Identity</${SectionLabel}>
            <div style=${{ display: 'flex', gap: 14, marginBottom: 16 }}>
              <div style=${{ flex: 1 }}>
                <div style=${fieldLabel}>World name</div>
                <${Input} value=${f.name} onInput=${(v) => set('name', v)} placeholder="Ashfall" />
              </div>
              <div>
                <div style=${fieldLabel}>Sigil & colour</div>
                <div style=${{ display: 'flex', gap: 6 }}>
                  ${TONE_SWATCHES.map((t) => html`<${ToneSwatch} key=${t.id} tone=${t.id} ch=${sigilCh}
                    active=${f.tone === t.id} onClick=${() => set('tone', t.id)} />`)}
                </div>
              </div>
            </div>
            <div style=${{ display: 'flex', gap: 14, marginBottom: 16 }}>
              <div style=${{ flex: 1 }}>
                <div style=${fieldLabel}>System</div>
                <${Input} value=${f.system} onInput=${(v) => set('system', v)} placeholder="D&D 5e" />
              </div>
              <div style=${{ flex: 1 }}>
                <div style=${fieldLabel}>
                  Setting <span style=${{ fontWeight: 400, color: 'var(--ink-faint)' }}>optional</span>
                </div>
                <${Input} value=${f.setting} onInput=${(v) => set('setting', v)} placeholder="Homebrew, Forgotten Realms…" />
              </div>
            </div>

            <button type="button" onClick=${() => set('detailsOpen', !f.detailsOpen)}
              style=${{
                display: 'flex', alignItems: 'center', gap: 6, padding: 0, border: 'none', background: 'none',
                fontSize: 12, fontWeight: 500, color: 'var(--ink-muted)', cursor: 'pointer', marginBottom: f.detailsOpen ? 12 : 0,
              }}>
              <${Icon} name=${f.detailsOpen ? 'chev-d' : 'chev-r'} size=${12} />
              ${f.detailsOpen ? 'Hide' : 'Show'} campaign details
            </button>

            ${f.detailsOpen && html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 14, paddingTop: 4, borderTop: '1px solid var(--rule-soft)' }}>
              <div style=${{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
                <${Field} label="GM / DM">
                  <div style=${{ display: 'flex', gap: 6 }}>
                    <${Input} value=${f.gm} onInput=${(v) => set('gm', v)} />
                    <${PronounSelect} value=${f.gm_pronouns} onChange=${(v) => set('gm_pronouns', v)} />
                  </div>
                </${Field}>
                <${Field} label="Default language">
                  <${Input} value=${f.default_language} onInput=${(v) => set('default_language', v)} placeholder="en" mono />
                </${Field}>
              </div>
              <${Field} label="Start session #" hint="First session number for this world.">
                <${Input} type="number" value=${f.start} onInput=${(v) => set('start', v)} style=${{ width: 120 }} />
              </${Field}>
              <${Field} label="Players" hint="Tick GM for a co-GM or guest GM — their voice counts as narration, not player dialogue."><${PlayerRows} players=${f.players} onChange=${(p) => set('players', p)} /></${Field}>
              <${Field} label="Additional information" hint="World frame, house rules, or special notes.">
                <${Textarea} value=${f.extra_info} onInput=${(v) => set('extra_info', v)} rows=${2} />
              </${Field}>
            </div>`}
          </div>

          <div style=${panel}>
            <${SectionLabel}>Where it lives</${SectionLabel}>
            <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', marginBottom: 14, marginTop: -8, lineHeight: 1.5 }}>
              Your world is a folder of markdown files. Keep it anywhere; sync it with your own tool.
            </div>
            <div style=${{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 14 }}>
              <${VaultRadio}
                checked=${f.mode === 'fresh'}
                onSelect=${() => set('mode', 'fresh')}
                title="Create a fresh vault"
                description="An empty world — optionally with starter Codex folders." />
              <${VaultRadio}
                checked=${f.mode === 'existing'}
                onSelect=${() => set('mode', 'existing')}
                title="Open an existing world"
                description="Re-open a Chronicle Keeper world folder (e.g. moved or restored). To bring in Obsidian notes, create the world first, then Codex → Import notes." />
            </div>

            <div style=${fieldLabel}>Folder</div>
            <div style=${{ display: 'flex', gap: 8, marginBottom: f.mode === 'fresh' ? 14 : 0 }}>
              <${Input} value=${f.world_path} onInput=${(v) => set('world_path', v)}
                placeholder=${f.mode === 'fresh' ? 'Default location (leave empty)' : 'Pick a folder…'}
                style=${{ flex: 1 }} mono />
              <${Btn} kind="secondary" size="sm" icon="folder" onClick=${chooseFolder}>Choose…</${Btn}>
            </div>

            ${f.mode === 'fresh' && html`<div>
              <div style=${{ ...fieldLabel, marginBottom: 8 }}>Starter structure</div>
              <div style=${{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                <${VaultRadio}
                  checked=${!f.scaffold}
                  onSelect=${() => set('scaffold', false)}
                  title="Empty Codex"
                  description="Blank Codex — add folders and pages as you go." />
                <${VaultRadio}
                  checked=${f.scaffold}
                  onSelect=${() => set('scaffold', true)}
                  title="Starter folders"
                  description="Creates NPCs, Places, Factions, Items, and Lore under Codex/." />
              </div>
            </div>`}
          </div>

          <div style=${{ paddingBottom: 8 }}>
            <${Btn} kind="ghost" size="sm" icon="book" disabled=${busy} onClick=${addExample}>Add the example world</${Btn}>
          </div>
        </div>

        <div>
          <div style=${{ position: 'sticky', top: 0 }}>
            <div style=${{
              fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase',
              color: 'var(--ink-faint)', marginBottom: 8,
            }}>What gets created</div>
            <${VaultPreview} f=${f} sniff=${sniff} />
          </div>
        </div>
      </div>
    </div>
  </${Shell}>`;
}
