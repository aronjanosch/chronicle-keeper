// Keeper → Skills: the app-global skill library. List (filter, search, enable
// toggle), editor (name, description, SKILL.md body, suggest-on-kinds), and the
// create / duplicate / restore / delete flows. Backed by /skills.
import { html, useState, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { setOp, store } from './core.js';
import { Icon, Btn, Card, Menu, SearchField, Spinner } from './ui.js';
import { loadSkills, getSkill, saveSkill, duplicateSkill, restoreSkill, deleteSkill, setSkillEnabled, revealPath } from './actions.js';

const FILTERS = [['all', 'All'], ['custom', 'Custom'], ['builtin', 'Built-in'], ['disabled', 'Disabled']];
const COMMON_KINDS = ['npc', 'pc', 'place', 'faction', 'item', 'lore', 'event', 'thread', 'prep'];

export function skillBadge(s) {
  if (s.source !== 'user') return { label: 'Built-in', chip: '' };
  if (s.overrides_default) return { label: 'Modified', chip: 'chip-ochre' };
  return { label: 'Custom', chip: 'chip-burgundy' };
}

const fail = (e) => setOp(String(e.message || e), 'err');

function Toggle({ on, onChange, title }) {
  return html`<button type="button" role="switch" aria-checked=${on} title=${title || (on ? 'Enabled' : 'Disabled')}
    onClick=${(e) => { e.stopPropagation(); onChange(!on); }}
    style=${{ width: 34, height: 20, borderRadius: 999, border: 'none', padding: 0, position: 'relative', flex: '0 0 auto', cursor: 'pointer', background: on ? 'var(--moss)' : 'var(--ink-ghost)', transition: 'background .14s' }}>
    <span style=${{ position: 'absolute', top: 2, left: on ? 16 : 2, width: 16, height: 16, borderRadius: '50%', background: '#FFFCF3', transition: 'left .14s' }} />
  </button>`;
}

function PaneHeader({ title, sub, children }) {
  return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 16px', borderBottom: '1px solid var(--rule)', minHeight: 32 }}>
    <div style=${{ flex: 1, minWidth: 0 }}>
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${title}</div>
      ${sub && html`<div style=${{ fontSize: 12, color: 'var(--ink-muted)', marginTop: 1 }}>${sub}</div>`}
    </div>
    ${children}
  </div>`;
}

function KindChips({ kinds }) {
  return kinds.slice(0, 3).map((k) => html`<span key=${k} class="chip chip-mono">${k}</span>`);
}

function SkillList({ skills, onOpen, onNew, onWrite, onDuplicate }) {
  const [q, setQ] = useState('');
  const [filter, setFilter] = useState('all');
  const enabled = skills.filter((s) => s.enabled !== false).length;
  const ql = q.trim().toLowerCase();
  const shown = skills.filter((s) => {
    if (filter === 'custom' && !(s.source === 'user' && !s.overrides_default)) return false;
    if (filter === 'builtin' && !(s.source !== 'user' || s.overrides_default)) return false;
    if (filter === 'disabled' && s.enabled !== false) return false;
    return !ql || `${s.name} ${s.slug} ${s.description} ${(s.kinds || []).join(' ')}`.toLowerCase().includes(ql);
  });

  const toggle = (s, on) => setSkillEnabled(s.slug, on).catch(fail);

  return html`<div style=${{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, minHeight: 0 }}>
    <${PaneHeader} title="Skills" sub=${`${enabled} of ${skills.length} enabled`}>
      <${Btn} icon="feather" onClick=${onWrite}>Ask Keeper to write one</${Btn}>
      <${Btn} kind="primary" icon="plus" onClick=${onNew}>New skill</${Btn}>
    </${PaneHeader}>
    <div style=${{ flex: 1, overflow: 'auto', padding: '18px 28px', display: 'flex', flexDirection: 'column', gap: 14 }}>
      <div style=${{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
        <${SearchField} value=${q} onInput=${setQ} placeholder="Filter skills…" count=${ql ? shown.length : undefined} style=${{ width: 280 }} />
        <div role="tablist" style=${{ display: 'flex', border: '1px solid var(--rule)', borderRadius: 6, overflow: 'hidden', fontSize: 13 }}>
          ${FILTERS.map(([id, label]) => html`<button key=${id} type="button" role="tab" aria-selected=${filter === id} onClick=${() => setFilter(id)}
            style=${{ padding: '6px 12px', border: 'none', cursor: 'pointer', fontFamily: 'inherit', fontSize: 13,
              background: filter === id ? 'var(--paper-deep)' : 'var(--surface)', fontWeight: filter === id ? 600 : 400, color: filter === id ? 'var(--ink)' : 'var(--ink-muted)' }}>${label}</button>`)}
        </div>
      </div>
      <${Card} bodyPad=${false} style=${{ overflow: 'visible' }}>
        ${shown.map((s, i) => {
          const b = skillBadge(s);
          const off = s.enabled === false;
          const custom = b.label === 'Custom';
          const restorable = s.source === 'user' && s.overrides_default;
          return html`<div key=${s.slug} class="ck-skill-row" onClick=${() => onOpen(s.slug)} style=${{
            display: 'flex', alignItems: 'center', gap: 14, padding: '13px 16px', cursor: 'pointer',
            borderTop: i ? '1px solid var(--rule-soft)' : 'none', opacity: off ? 0.62 : 1,
          }}>
            <span style=${{ width: 32, height: 32, borderRadius: 6, flex: '0 0 auto', display: 'flex', alignItems: 'center', justifyContent: 'center',
              background: custom ? 'var(--burgundy-50)' : 'var(--surface-inset)', color: custom ? 'var(--burgundy-700)' : 'var(--ink-muted)' }}>
              <${Icon} name="feather" size=${14} />
            </span>
            <div style=${{ flex: 1, minWidth: 0 }}>
              <div style=${{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
                <span style=${{ fontFamily: 'var(--font-display)', fontSize: 15.5, fontWeight: 500 }}>${s.name}</span>
                <span class=${`chip ${b.chip}`} style=${{ fontSize: 11 }}>${b.label}</span>
              </div>
              <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', marginTop: 2, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${s.description || 'No description'}</div>
            </div>
            <span style=${{ display: 'flex', gap: 4, justifyContent: 'flex-end', flex: '0 0 auto' }}><${KindChips} kinds=${s.kinds || []} /></span>
            <${Toggle} on=${!off} onChange=${(on) => toggle(s, on)} />
            <span onClick=${(e) => e.stopPropagation()} style=${{ display: 'flex' }}>
              <${Menu} title="Skill actions" items=${[
                { label: 'Edit', icon: 'edit', onClick: () => onOpen(s.slug) },
                { label: 'Duplicate', icon: 'copy', onClick: () => onDuplicate(s) },
                s.source === 'user' ? { label: restorable ? 'Restore built-in…' : 'Delete…', icon: restorable ? 'undo' : 'trash', danger: true, onClick: () => onOpen(s.slug, 'remove') } : null,
              ]} />
            </span>
          </div>`;
        })}
        ${!shown.length && html`<div style=${{ padding: '28px 16px', textAlign: 'center', fontSize: 13, color: 'var(--ink-muted)' }}>
          ${skills.length ? 'No skills match.' : 'No skills yet — write one, or ask the Keeper to.'}
        </div>`}
      </${Card}>
      <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', lineHeight: 1.55 }}>
        Skills are shared by every world. Invoke one with <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 12 }}>/</span> in chat, from a kind chip on a page, or let the Keeper pull it when the description matches. Chips show the page kinds a skill is suggested on.
      </div>
    </div>
  </div>`;
}

const label = { fontSize: 12.5, fontWeight: 500 };
const hint = { fontSize: 12, color: 'var(--ink-muted)', lineHeight: 1.45 };
const box = {
  width: '100%', boxSizing: 'border-box', padding: '7px 10px', border: '1px solid var(--rule)', borderRadius: 6,
  background: 'var(--surface-raised)', fontSize: 13.5, color: 'var(--ink)', fontFamily: 'inherit',
};

function Labeled({ text, note, children }) {
  return html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 4 }}>
    <label style=${label}>${text}</label>${children}${note && html`<span style=${hint}>${note}</span>`}
  </div>`;
}

function KindEditor({ kinds, onChange }) {
  const [adding, setAdding] = useState(false);
  const [text, setText] = useState('');
  const known = [...new Set([...COMMON_KINDS, ...(store.kindSchemas || []).map((k) => String(k.kind || k.id || k.name || '').toLowerCase()).filter(Boolean)])];
  const add = (v) => {
    const k = String(v ?? text).trim().toLowerCase().replace(/[,[\]\s]+/g, '-');
    if (k && !kinds.includes(k)) onChange([...kinds, k]);
    setText(''); setAdding(false);
  };
  return html`<div style=${{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
    ${kinds.map((k) => html`<span key=${k} class="chip chip-burgundy">${k}
      <button type="button" title=${`Remove ${k}`} onClick=${() => onChange(kinds.filter((x) => x !== k))}
        style=${{ border: 'none', background: 'transparent', color: 'inherit', cursor: 'pointer', padding: 0, display: 'flex' }}><${Icon} name="x" size=${10} /></button></span>`)}
    ${adding
      ? html`<input autofocus list="ck-skill-kinds" value=${text} placeholder="kind" onInput=${(e) => setText(e.target.value)}
          onKeyDown=${(e) => { if (e.key === 'Enter') { e.preventDefault(); add(); } else if (e.key === 'Escape') { setText(''); setAdding(false); } }}
          onBlur=${() => (text.trim() ? add() : setAdding(false))}
          style=${{ ...box, width: 110, padding: '3px 8px', fontSize: 12 }} />
        <datalist id="ck-skill-kinds">${known.filter((k) => !kinds.includes(k)).map((k) => html`<option key=${k} value=${k} />`)}</datalist>`
      : html`<button type="button" class="chip chip-ghost" onClick=${() => setAdding(true)} style=${{ cursor: 'pointer' }}>+ add kind</button>`}
  </div>`;
}

function SkillEditor({ slug, intent, onBack, onOpen, onTry, onImprove, onRemoved }) {
  const isNew = slug === '__new';
  const [loaded, setLoaded] = useState(isNew ? { name: '', description: '', body: '', kinds: [], source: 'user', overrides_default: false, slug: '' } : null);
  const [f, setF] = useState(loaded);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [confirm, setConfirm] = useState(intent === 'remove' ? 'remove' : null); // 'remove' | 'leave'

  useEffect(() => {
    if (isNew) return undefined;
    let alive = true;
    setLoaded(null);
    getSkill(slug).then((r) => { if (alive) { setLoaded(r); setF(r); setError(''); } }).catch((e) => { if (alive) setError(String(e.message || e)); });
    return () => { alive = false; };
  }, [slug]);

  if (!loaded || !f) {
    return html`<div style=${{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
      ${error ? html`<span style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${error}</span>` : html`<${Spinner} size=${16} />`}
    </div>`;
  }

  const live = (store.keeperSkills || []).find((s) => s.slug === slug);
  const enabled = isNew ? true : live ? live.enabled !== false : true;
  const dirty = isNew
    ? !!(f.name.trim() || f.description.trim() || f.body.trim())
    : f.name !== loaded.name || f.description !== loaded.description || f.body !== loaded.body || (f.kinds || []).join() !== (loaded.kinds || []).join();
  const builtin = !isNew && loaded.source !== 'user';
  const modified = !isNew && loaded.source === 'user' && loaded.overrides_default;
  const b = skillBadge(loaded);
  const set = (patch) => setF((cur) => ({ ...cur, ...patch }));

  async function save() {
    if (!f.name.trim()) { setError('Give the skill a name.'); return; }
    setBusy(true); setError('');
    try {
      const r = await saveSkill(isNew ? null : slug, { name: f.name, description: f.description, kinds: f.kinds || [], body: f.body });
      setLoaded(r); setF(r);
      setOp('Skill saved', 'done');
      if (isNew) onOpen(r.slug);
    } catch (e) { setError(String(e.message || e)); }
    setBusy(false);
  }

  async function remove() {
    setBusy(true);
    try {
      if (modified) await restoreSkill(slug); else await deleteSkill(slug);
      setOp(modified ? 'Built-in restored' : 'Skill deleted', 'done');
      if (modified) { const r = await getSkill(slug); setLoaded(r); setF(r); setConfirm(null); } else onRemoved();
    } catch (e) { setError(String(e.message || e)); setConfirm(null); }
    setBusy(false);
  }

  async function duplicate() {
    try { const r = await duplicateSkill(slug); setOp('Skill duplicated', 'done'); onOpen(r.slug); } catch (e) { setError(String(e.message || e)); }
  }

  const back = () => { if (dirty && confirm !== 'leave') setConfirm('leave'); else onBack(); };
  const path = isNew ? '' : `Skills/${loaded.slug}/SKILL.md`;
  const sourceText = builtin
    ? 'Built-in · bundled with the app'
    : modified ? 'Modified built-in · your copy shadows the bundled one' : 'Custom · on disk in your Skills folder';

  return html`<div style=${{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, minHeight: 0 }}>
    <${PaneHeader} title=${isNew ? 'New skill' : loaded.name || slug}
      sub=${html`<span><a href="#" onClick=${(e) => { e.preventDefault(); back(); }} style=${{ color: 'var(--burgundy)' }}>Skills</a> / ${isNew ? 'New' : b.label} · shared by every world${dirty ? ' · unsaved' : ''}</span>`}>
      <${Btn} disabled=${!dirty || busy} onClick=${() => { setF(loaded); setError(''); }}>Discard</${Btn}>
      <${Btn} kind="primary" disabled=${!dirty || busy || !f.name.trim()} onClick=${save}>${busy ? 'Saving…' : 'Save'}</${Btn}>
    </${PaneHeader}>
    ${confirm === 'leave' && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 16px', background: 'var(--burgundy-50)', borderBottom: '1px solid var(--rule-soft)', fontSize: 12.5, color: 'var(--burgundy-700)' }}>
      <span style=${{ flex: 1 }}><b>Discard unsaved changes?</b></span>
      <${Btn} size="sm" kind="danger" onClick=${onBack}>Discard</${Btn}>
      <${Btn} size="sm" kind="ghost" onClick=${() => setConfirm(null)}>Keep editing</${Btn}>
    </div>`}
    <div style=${{ flex: 1, display: 'grid', gridTemplateColumns: 'minmax(0,1fr) 270px', overflow: 'hidden', minHeight: 0 }}>
      <div style=${{ padding: '20px 28px', display: 'flex', flexDirection: 'column', gap: 14, overflow: 'auto', minHeight: 0 }}>
        <${Labeled} text="Name">
          <input value=${f.name} placeholder="e.g. House rules" onInput=${(e) => set({ name: e.target.value })} style=${box} autofocus=${isNew} />
        </${Labeled}>
        <${Labeled} text="Description" note="The only part in every prompt. Make it specific: it decides when the Keeper pulls the skill.">
          <input value=${f.description} placeholder="What it does and when to use it" onInput=${(e) => set({ description: e.target.value })} style=${box} />
        </${Labeled}>
        <${Labeled} text="Body (SKILL.md)" note="Loaded on demand and treated as reference, not instructions. Leave empty for a starter outline.">
          <textarea value=${f.body} onInput=${(e) => set({ body: e.target.value })} spellcheck=${false}
            style=${{ ...box, fontFamily: 'var(--font-mono)', fontSize: 12.5, lineHeight: 1.65, minHeight: 260, resize: 'vertical' }} />
        </${Labeled}>
        ${error && html`<div style=${{ fontSize: 12.5, color: 'var(--burgundy-700)' }}>${error}</div>`}
        <div style=${{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}>
          ${!isNew && html`<${Btn} icon="feather" onClick=${() => onImprove(loaded)}>Ask Keeper to improve</${Btn}>
            <${Btn} icon="feather" disabled=${!enabled || dirty} title=${!enabled ? 'Enable the skill first' : dirty ? 'Save your changes first' : ''} onClick=${() => onTry(loaded)}>Try in chat</${Btn}>`}
          ${path && html`<span style=${hint}>Saved as ${path}</span>`}
        </div>
      </div>
      <div style=${{ borderLeft: '1px solid var(--rule)', padding: '20px 18px', display: 'flex', flexDirection: 'column', gap: 16, background: 'var(--surface-inset)', overflow: 'auto', minHeight: 0 }}>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <${Toggle} on=${enabled} onChange=${(on) => (isNew ? null : setSkillEnabled(slug, on).catch(fail))} />
          <span style=${{ fontSize: 13, fontWeight: 500 }}>Enabled</span>
        </div>
        <${Labeled} text="Suggest on page kinds" note="Shows a chip in the Keeper panel on matching pages.">
          <${KindEditor} kinds=${f.kinds || []} onChange=${(kinds) => set({ kinds })} />
        </${Labeled}>
        <${Labeled} text="Source">
          <div style=${hint}>${sourceText}</div>
          ${!isNew && (window.__TAURI__
            ? html`<a href="#" onClick=${(e) => { e.preventDefault(); revealPath(store.skillsPath); }} style=${{ fontSize: 12.5, color: 'var(--burgundy)' }}>Open folder</a>`
            : store.skillsPath && html`<span style=${{ ...hint, fontFamily: 'var(--font-mono)', wordBreak: 'break-all' }}>${store.skillsPath}</span>`)}
        </${Labeled}>
        <div style=${{ flex: 1 }} />
        ${!isNew && html`<div style=${{ borderTop: '1px solid var(--rule)', paddingTop: 12, display: 'flex', flexDirection: 'column', gap: 8 }}>
          <${Btn} icon="copy" onClick=${duplicate}>Duplicate</${Btn}>
          ${builtin
            ? html`<span style=${hint}>Built-in skills can be disabled or edited (saving keeps a restorable copy), not deleted.</span>`
            : confirm === 'remove'
              ? html`<div style=${{ border: '1px solid rgba(122,46,31,.3)', background: 'var(--burgundy-50)', borderRadius: 6, padding: '8px 10px', fontSize: 12.5, color: 'var(--burgundy-700)' }}>
                  <b>${modified ? 'Restore the built-in?' : 'Delete this skill?'}</b> ${modified ? 'Your edits are discarded and the bundled version returns.' : 'This removes it for good.'}
                  <div style=${{ display: 'flex', gap: 10, marginTop: 6 }}>
                    <button type="button" onClick=${remove} disabled=${busy} style=${{ border: 'none', background: 'transparent', cursor: 'pointer', fontWeight: 600, color: 'inherit', padding: 0, fontFamily: 'inherit', fontSize: 12.5 }}>${modified ? 'Restore' : 'Delete'}</button>
                    <button type="button" onClick=${() => setConfirm(null)} style=${{ border: 'none', background: 'transparent', cursor: 'pointer', color: 'var(--ink-muted)', padding: 0, fontFamily: 'inherit', fontSize: 12.5 }}>Cancel</button>
                  </div>
                </div>`
              : html`<${Btn} kind="danger" icon=${modified ? 'undo' : 'trash'} onClick=${() => setConfirm('remove')}>${modified ? 'Restore built-in' : 'Delete'}</${Btn}>`}
        </div>`}
      </div>
    </div>
  </div>`;
}

// sel: null = list, '__new' = blank editor, else a slug.
export function SkillsView({ sel, setSel, onWrite, onImprove, onTry }) {
  const [intent, setIntent] = useState(null);
  useEffect(() => { loadSkills(null, true); }, []);
  const skills = store.keeperSkills;
  const open = (slug, why) => { setIntent(why || null); setSel(slug); };
  if (sel) {
    return html`<${SkillEditor} key=${sel} slug=${sel} intent=${intent} onBack=${() => setSel(null)} onOpen=${open}
      onTry=${onTry} onImprove=${onImprove} onRemoved=${() => setSel(null)} />`;
  }
  if (!skills) return html`<div style=${{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}><${Spinner} size=${16} /></div>`;

  async function duplicate(s) {
    try { const r = await duplicateSkill(s.slug); setOp('Skill duplicated', 'done'); open(r.slug); } catch (e) { fail(e); }
  }
  return html`<${SkillList} skills=${skills} onOpen=${open} onNew=${() => open('__new')} onWrite=${onWrite}
    onDuplicate=${duplicate} />`;
}
