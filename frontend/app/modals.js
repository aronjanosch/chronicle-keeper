// Overlay forms: campaign editor, session editor, transcribe, export, provider, viewer.
import { html, useState, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { store, closeModal, navigate, setOp, openModal, apiFetch, fmtDateTime } from './core.js';
import { Icon, Btn, Field, Input, Textarea, Select, Spinner } from './ui.js';
import { CommandPalette } from './screens/palette.js';
import { kindForFolder } from './folderKinds.js';
import {
  createCampaign, updateCampaign, saveSessionMetadata, loadSession,
  runExport,
  loadLlmProviders, saveLlmProvider, testLlmProvider, fetchLlmModels,
  importCodex, commitCodexImport,
  createPromptTemplate, updatePromptTemplate,
  enhanceVaultPages, loadVaultDiagnostics, exportWorld, revealPath,
  createVaultPage, saveVaultPage, promoteVaultPage,
  loadPageHistory, readPageVersion, restorePageVersion, loadWorldHistory,
  loadTrash, restoreTrash, emptyTrash,
} from './actions.js';

const PRONOUNS = ['she/her', 'he/him', 'they/them'];

const CODEX_KINDS = [
  { value: 'pc', label: 'PC' }, { value: 'npc', label: 'NPC' }, { value: 'place', label: 'Place' },
  { value: 'faction', label: 'Faction' }, { value: 'item', label: 'Item' },
  { value: 'event', label: 'Event' },
  { value: 'lore', label: 'Lore' },
];

// Split combined notes into <=max-char batches at line boundaries, so a big
// vault (dozens of files) doesn't blow the LLM context in one call.
function chunkNotes(text, max = 12000) {
  const lines = text.split('\n');
  const chunks = [];
  let cur = '';
  for (const ln of lines) {
    if (cur.length + ln.length + 1 > max && cur.trim()) { chunks.push(cur); cur = ''; }
    cur += ln + '\n';
  }
  if (cur.trim()) chunks.push(cur);
  return chunks.length ? chunks : [text];
}

function ModalShell({ title, children, footer, wide }) {
  return html`<div class="ck-backdrop" onClick=${(e) => { if (e.target === e.currentTarget) closeModal(); }}>
    <div class="ck" style=${{ width: wide ? 720 : 480, maxWidth: '100%', height: 'auto', maxHeight: '88vh', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 12, boxShadow: 'var(--shadow-raised)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      <div style=${{ padding: '16px 20px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 10 }}>
        <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 18, fontWeight: 500, color: 'var(--ink)', flex: 1 }}>${title}</h3>
        <${Btn} kind="ghost" size="sm" icon="x" onClick=${closeModal} />
      </div>
      <div style=${{ padding: '18px 20px', overflow: 'auto', display: 'flex', flexDirection: 'column', gap: 14 }}>${children}</div>
      ${footer && html`<div style=${{ padding: '14px 20px', borderTop: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 10, justifyContent: 'flex-end' }}>${footer}</div>`}
    </div>
  </div>`;
}

// ── Campaign create / edit ────────────────────────────────────────
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
      <${Btn} kind="ghost" size="sm" icon="x" onClick=${() => onChange(players.filter((_, j) => j !== i))} />
    </div>`)}
    <${Btn} kind="ghost" size="sm" icon="plus" onClick=${() => onChange([...players, { player_name: '', character_name: '', pronouns: '' }])}>Add player</${Btn}>
  </div>`;
}

function CampaignModal({ edit }) {
  const [f, setF] = useState(() => edit ? {
    name: edit.name || '', system: edit.system || '', setting: edit.setting || '',
    default_language: edit.default_language || '', gm: edit.gm || '', gm_pronouns: edit.gm_pronouns || '',
    players: edit.players?.length ? edit.players : [], extra_info: edit.extra_info || '', start: edit.next_session_number ?? 1,
  } : { name: '', system: '', setting: '', default_language: '', gm: '', gm_pronouns: '', players: [{ player_name: '', character_name: '', pronouns: '' }], extra_info: '', start: 0 });
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const set = (k, v) => setF((s) => ({ ...s, [k]: v }));

  async function save() {
    if (!f.name.trim()) { setErr('Name is required'); return; }
    setBusy(true); setErr(null);
    const players = f.players.filter((p) => p.player_name.trim() || p.character_name.trim());
    try {
      if (edit) await updateCampaign({ name: f.name.trim(), system: f.system.trim(), setting: f.setting.trim(), default_language: f.default_language.trim(), gm: f.gm.trim(), gm_pronouns: f.gm_pronouns, players, extra_info: f.extra_info.trim() });
      else await createCampaign({ ...f, name: f.name.trim(), players });
      closeModal();
    } catch (e) { setErr(e.message); setBusy(false); }
  }

  return html`<${ModalShell} title=${edit ? 'Edit world' : 'New world'} footer=${html`
    <${Btn} kind="ghost" onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${save}>${busy ? 'Saving…' : (edit ? 'Save changes' : 'Create world')}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <${Field} label="World name"><${Input} value=${f.name} onInput=${(v) => set('name', v)} placeholder="The Iron Crown" /></${Field}>
    <div style=${{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
      <${Field} label="System"><${Input} value=${f.system} onInput=${(v) => set('system', v)} placeholder="D&D 5e" /></${Field}>
      <${Field} label="Setting"><${Input} value=${f.setting} onInput=${(v) => set('setting', v)} placeholder="Forgotten Realms" /></${Field}>
      <${Field} label="GM / DM"><div style=${{ display: 'flex', gap: 6 }}>
        <${Input} value=${f.gm} onInput=${(v) => set('gm', v)} />
        <${PronounSelect} value=${f.gm_pronouns} onChange=${(v) => set('gm_pronouns', v)} />
      </div></${Field}>
      <${Field} label="Default language"><${Input} value=${f.default_language} onInput=${(v) => set('default_language', v)} placeholder="en" mono /></${Field}>
    </div>
    ${!edit && html`<${Field} label="Start session #" hint="First session number for this world."><${Input} type="number" value=${f.start} onInput=${(v) => set('start', v)} style=${{ width: 120 }} /></${Field}>`}
    <${Field} label="Players"><${PlayerRows} players=${f.players} onChange=${(p) => set('players', p)} /></${Field}>
    <${Field} label="Additional information" hint="World frame or special notes."><${Textarea} value=${f.extra_info} onInput=${(v) => set('extra_info', v)} rows=${3} /></${Field}>
  </${ModalShell}>`;
}

// ── Session metadata edit ─────────────────────────────────────────
function SessionModal({ session }) {
  const cam = session.campaign || {};
  // Metadata (NPCs, places, items, events, tags) is edited inline in the
  // "What happened" card on the session screen — not here. This modal only
  // touches the session's own fields, so it must preserve the existing
  // metadata untouched (the backend replaces metadata wholesale on save).
  const md = session.metadata || {};
  const [f, setF] = useState({
    title: cam.title || '', date: cam.date || '', number: cam.session_number ?? '', notes: cam.notes || '',
  });
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const set = (k, v) => setF((s) => ({ ...s, [k]: v }));

  async function save() {
    setBusy(true); setErr(null);
    try {
      await saveSessionMetadata({
        session_id: session.session_id, campaign_id: cam.campaign_id || null,
        session_number: (f.number === '' || f.number == null ? null : Number(f.number)), title: f.title.trim() || null, date: f.date || null,
        metadata: md,
        notes: f.notes.trim() || null,
      });
      await loadSession(session.session_id);
      closeModal();
    } catch (e) { setErr(e.message); setBusy(false); }
  }

  return html`<${ModalShell} title="Edit session" footer=${html`
    <${Btn} kind="ghost" onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${save}>${busy ? 'Saving…' : 'Save'}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <${Field} label="Title"><${Input} value=${f.title} onInput=${(v) => set('title', v)} /></${Field}>
    <div style=${{ display: 'grid', gridTemplateColumns: '1fr 120px', gap: 12 }}>
      <${Field} label="Date"><${Input} type="date" value=${f.date} onInput=${(v) => set('date', v)} /></${Field}>
      <${Field} label="Session #"><${Input} type="number" value=${f.number} onInput=${(v) => set('number', v)} mono /></${Field}>
    </div>
    <${Field} label="Notes"><${Textarea} value=${f.notes} onInput=${(v) => set('notes', v)} rows=${3} /></${Field}>
    <div style=${{ fontSize: 12, color: 'var(--ink-faint)', lineHeight: 1.5 }}>NPCs, places, items, events and tags are edited in the “What happened” panel on the session.</div>
  </${ModalShell}>`;
}

// ── Export ────────────────────────────────────────────────────────
function ExportModal() {
  const summaries = store.summaries;
  const [id, setId] = useState(summaries[0]?.id || null);
  function run() { closeModal(); runExport(Number(id)); }
  return html`<${ModalShell} title="Export session" footer=${html`
    <${Btn} kind="ghost" onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" icon="export" disabled=${!summaries.length} onClick=${run}>Export markdown</${Btn}>`}>
    <${Field} label="Summary" hint="Exported as Obsidian-flavoured markdown.">
      <select value=${id} onChange=${(e) => setId(e.target.value)} style=${{ width: '100%', padding: '7px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, cursor: 'pointer' }}>
        ${summaries.map((s) => html`<option key=${s.id} value=${s.id}>${s.provider} / ${s.model}</option>`)}
      </select>
    </${Field}>
  </${ModalShell}>`;
}

// ── LLM provider config ───────────────────────────────────────────
function ProviderModal({ id }) {
  const p = (store.llmProviders || []).find((x) => x.id === id);
  const [model, setModel] = useState(p?.saved_model || p?.default_model || '');
  const [apiBase, setApiBase] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [status, setStatus] = useState(null);
  const [liveModels, setLiveModels] = useState(null);
  useEffect(() => { if (p) fetchLlmModels(id).then((m) => { if (m.length) setLiveModels(m); }); }, [id]);
  if (!p) { closeModal(); return null; }
  const suggestions = liveModels || p.models || [];

  async function save() {
    setStatus({ msg: 'Saving…' });
    try { await saveLlmProvider(id, { api_key: apiKey.trim() || null, api_base: apiBase.trim() || null, default_model: model.trim() || null }); setApiKey(''); setStatus({ msg: 'Saved', ok: true }); }
    catch (e) { setStatus({ msg: e.message, ok: false }); }
  }
  async function test() {
    setStatus({ msg: 'Testing…' });
    try { const r = await testLlmProvider(id, model.trim()); setStatus({ msg: r.ok ? `OK (${r.latency_ms}ms)` : (r.error || 'Failed'), ok: r.ok }); }
    catch (e) { setStatus({ msg: e.message, ok: false }); }
  }

  return html`<${ModalShell} title=${`Configure ${p.name}`} footer=${html`
    <span style=${{ flex: 1, fontSize: 12.5, color: status?.ok === false ? 'var(--burgundy-700)' : 'var(--moss)' }}>${status?.msg || ''}</span>
    <${Btn} kind="ghost" onClick=${test}>Test</${Btn}>
    <${Btn} kind="primary" onClick=${save}>Save</${Btn}>`}>
    <${Field} label="Default model" hint=${liveModels ? 'Installed models, live from the provider.' : (p.id === 'ollama' ? 'Must match a model pulled in Ollama.' : (p.models?.length ? 'Pick a suggestion or type any model name.' : 'Type the exact model id (e.g. from ollama.com).'))}>
      <${Input} value=${model} onInput=${setModel} mono list=${suggestions.length ? 'ck-prov-models' : undefined} />
      ${suggestions.length ? html`<datalist id="ck-prov-models">${suggestions.map((m, i) => html`<option key=${i} value=${m} />`)}</datalist>` : ''}
    </${Field}>
    <${Field} label=${`API base${p.has_custom_base ? ' (saved — enter to replace)' : (p.default_api_base ? '' : ' (optional)')}`}><${Input} value=${apiBase} onInput=${setApiBase} placeholder=${p.has_custom_base ? 'Custom base saved' : (p.default_api_base ? `Default: ${p.default_api_base}` : 'Provider default')} mono /></${Field}>
    ${p.needs_key && html`<${Field} label=${`API key${p.has_key ? ' (saved — enter to replace)' : ''}`}><${Input} type="password" value=${apiKey} onInput=${setApiKey} placeholder=${p.has_key ? '••••••••' : 'Paste API key'} autocomplete="off" /></${Field}>`}
  </${ModalShell}>`;
}

// ── Confirm dialog ────────────────────────────────────────────────
// In-app replacement for window.confirm(), which the Tauri webview does not
// reliably support (it returns false, so the action never fires).
function ConfirmModal({ title = 'Are you sure?', message, confirmLabel = 'Delete', onConfirm }) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  async function go() {
    setBusy(true); setErr(null);
    try { await onConfirm(); closeModal(); }
    catch (e) { setErr(e.message); setBusy(false); }
  }
  return html`<${ModalShell} title=${title} footer=${html`
    <${Btn} kind="ghost" disabled=${busy} onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${go}>${busy ? 'Working…' : confirmLabel}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <div style=${{ fontSize: 13.5, color: 'var(--ink-soft)', lineHeight: 1.5 }}>${message}</div>
  </${ModalShell}>`;
}

// ── Generic single-line prompt (new folder, rename) ───────────────
function TextPromptModal({ title, label, initial = '', placeholder, confirmLabel = 'Save', onSubmit }) {
  const [val, setVal] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  async function go() {
    const v = val.trim();
    if (!v) { setErr('Required'); return; }
    setBusy(true); setErr(null);
    try { await onSubmit(v); closeModal(); }
    catch (e) { setErr(e.message); setBusy(false); }
  }
  return html`<${ModalShell} title=${title} footer=${html`
    <${Btn} kind="ghost" disabled=${busy} onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${go}>${busy ? 'Working…' : confirmLabel}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <${Field} label=${label}>
      <${Input} value=${val} onInput=${setVal} placeholder=${placeholder}
        onKeydown=${(e) => { if (e.key === 'Enter') go(); }} />
    </${Field}>
  </${ModalShell}>`;
}

// ── Quick capture (Phase 8D): fleeting note → Inbox/, sort later ───
function QuickCaptureModal() {
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const taRef = { current: null };
  useEffect(() => { if (taRef.current) taRef.current.focus(); }, []);
  async function go() {
    const t = text.trim();
    if (!t) { setErr('Write something first'); return; }
    setBusy(true); setErr(null);
    const lines = t.split('\n');
    const first = lines[0].replace(/^#+\s*/, '').replace(/[/\\]/g, '-').trim();
    const stamp = new Date().toISOString().slice(0, 16).replace('T', ' ').replace(':', '.');
    const title = first && first.length <= 80 ? first : `Capture ${stamp}`;
    const body = (title === first ? lines.slice(1).join('\n') : t).trim();
    try {
      let page;
      try { page = await createVaultPage(title, 'lore', 'Inbox'); }
      catch (_) { page = await createVaultPage(`${title} ${stamp}`, 'lore', 'Inbox'); }
      await saveVaultPage(page.path, `---\nkind: lore\nsummary:\ntags: [inbox]\n---\n\n# ${page.title}\n\n${body}\n`);
      setOp(`Captured to ${page.path}`, 'done');
      closeModal();
    } catch (e) { setErr(e.message); setBusy(false); }
  }
  return html`<${ModalShell} title="Quick capture" footer=${html`
    <span style=${{ flex: 1, fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>First line becomes the title · lands in Inbox/ · promote it to a kind later</span>
    <${Btn} kind="ghost" disabled=${busy} onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${go}>${busy ? 'Saving…' : 'Capture'}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <textarea ref=${(el) => { taRef.current = el; }} rows=${6} value=${text} placeholder="Jot it down — sort it later (⌘↵ saves)"
      onInput=${(e) => setText(e.target.value)}
      onKeyDown=${(e) => { if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { e.preventDefault(); go(); } }}
      style=${{ width: '100%', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, padding: '7px 10px', fontSize: 13, color: 'var(--ink)', fontFamily: 'inherit', resize: 'vertical', lineHeight: 1.45 }} />
  </${ModalShell}>`;
}

// ── Shortcut cheat sheet (Phase 14E, ⌘/) ──────────────────────────
const SHORTCUT_GROUPS = [
  { head: 'Navigate', keys: [
    ['⌘K', 'Command palette'], ['⌘P', 'Quick-open a page'], ['⌘[ / ⌘]', 'Back / forward'],
    ['⌘⇧F', 'Search the world'], ['⌘,', 'Settings'],
  ] },
  { head: 'Tabs', keys: [
    ['⌘-click', 'Open page in new tab'], ['⌘W', 'Close tab'], ['⌘⇧T', 'Reopen closed tab'],
    ['⌘⇧[ / ⌘⇧]', 'Previous / next tab'], ['⌘1–9', 'Jump to tab (9 = last)'],
  ] },
  { head: 'Create', keys: [
    ['⌘N', 'New page'], ['⌘⇧J', 'Quick capture'],
  ] },
  { head: 'Page', keys: [
    ['⌘F', 'Find in page'], ['⌘⇧K', 'Toggle the side panel'], ['⌘S', 'Save now'], ['⌘/', 'This cheat sheet'],
  ] },
  { head: 'Editor', keys: [
    ['⌘B / ⌘I', 'Bold / italic'], ['⌘L', 'Wrap as [[wikilink]]'],
    ['[[', 'Link a page'], ['/', 'Slash menu (on an empty line)'], ['Tab / ⇧Tab', 'Indent list item'],
  ] },
];

function ShortcutsModal() {
  const mac = /Mac|iP(hone|ad|od)/.test(navigator.platform || '');
  const key = (k) => mac ? k : k.replace(/⌘/g, 'Ctrl+').replace(/⇧/g, 'Shift+');
  return html`<${ModalShell} title="Keyboard shortcuts" wide>
    <div style=${{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '18px 28px' }}>
      ${SHORTCUT_GROUPS.map((g) => html`<div key=${g.head}>
        <div style=${{ fontSize: 10, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 8 }}>${g.head}</div>
        ${g.keys.map(([k, label]) => html`<div key=${k} style=${{ display: 'flex', alignItems: 'baseline', gap: 12, padding: '3px 0' }}>
          <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--ink)', background: 'var(--surface-inset)', border: '1px solid var(--rule-soft)', borderRadius: 4, padding: '1px 6px', whiteSpace: 'nowrap' }}>${key(k)}</span>
          <span style=${{ fontSize: 13, color: 'var(--ink-soft)' }}>${label}</span>
        </div>`)}
      </div>`)}
    </div>
  </${ModalShell}>`;
}

// ── New event (Phase 11.5): mint a thin dated beat from the timeline ──
// Picker over existing pages; Enter accepts free text (broken wikilink
// until the page exists — same contract as typing [[New Name]]).
function PagePick({ pages, exclude = [], onPick, placeholder }) {
  const [q, setQ] = useState('');
  const needle = q.trim().toLowerCase();
  const hits = needle
    ? pages.filter((p) => p.title && p.title.toLowerCase().includes(needle) && !exclude.includes(p.title)).slice(0, 6)
    : [];
  const pick = (title) => { onPick(title); setQ(''); };
  return html`<div style=${{ position: 'relative' }}>
    <${Input} value=${q} onInput=${setQ} placeholder=${placeholder}
      onKeydown=${(e) => { if (e.key === 'Enter' && q.trim()) { e.preventDefault(); pick(hits[0]?.title || q.trim()); } }} />
    ${hits.length > 0 && html`<div style=${{ position: 'absolute', top: '100%', left: 0, right: 0, zIndex: 50, marginTop: 2,
      background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 6, boxShadow: 'var(--shadow-raised)', overflow: 'hidden' }}>
      ${hits.map((p) => html`<div key=${p.path} onClick=${() => pick(p.title)}
        style=${{ display: 'flex', alignItems: 'center', gap: 7, padding: '6px 10px', fontSize: 13, cursor: 'pointer' }}
        onMouseEnter=${(e) => { e.currentTarget.style.background = 'var(--paper-deep)'; }}
        onMouseLeave=${(e) => { e.currentTarget.style.background = 'transparent'; }}>
        <${Icon} name="doc" size=${12} className="ck-ink-muted" /> ${p.title}
        <span style=${{ fontSize: 11, color: 'var(--ink-faint)' }}>${p.kind || ''}</span>
      </div>`)}
    </div>`}
  </div>`;
}

function PickedChip({ label, onRemove }) {
  return html`<span style=${{ display: 'inline-flex', alignItems: 'center', gap: 5, padding: '2px 8px', borderRadius: 999, fontSize: 12,
    background: 'var(--burgundy-50)', border: '1px solid var(--burgundy-300)', color: 'var(--burgundy-700)' }}>
    ${label}<span onClick=${onRemove} style=${{ cursor: 'pointer', fontWeight: 600 }}>×</span>
  </span>`;
}

const yamlQuote = (s) => `"${String(s).replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;

function NewEventModal({ date: presetDate = '', order: presetOrder = '', onCreated }) {
  const [title, setTitle] = useState('');
  const [date, setDate] = useState(presetDate);
  const [order, setOrder] = useState(String(presetOrder));
  const [location, setLocation] = useState('');
  const [participants, setParticipants] = useState([]);
  const [summary, setSummary] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const pages = store.vaultPages || [];

  async function go() {
    const t = title.trim();
    if (!t) { setErr('Title is required'); return; }
    if (!date.trim() && !String(order).trim()) { setErr('Give it a date or an order number'); return; }
    setBusy(true); setErr(null);
    const fm = ['---', 'kind: event'];
    if (date.trim()) fm.push(`date: ${yamlQuote(date.trim())}`);
    else fm.push(`order: ${parseInt(order, 10)}`);
    if (location) fm.push(`location: ${yamlQuote(`[[${location}]]`)}`);
    if (participants.length) {
      fm.push('participants:');
      for (const p of participants) fm.push(`  - ${yamlQuote(`[[${p}]]`)}`);
    }
    if (summary.trim()) fm.push(`summary: ${yamlQuote(summary.trim())}`);
    fm.push('---');
    try {
      const page = await createVaultPage(t, 'event', 'Events');
      await saveVaultPage(page.path, `${fm.join('\n')}\n\n# ${page.title}\n`);
      setOp(`Event saved to ${page.path}`, 'done');
      closeModal();
      if (onCreated) onCreated(page);
    } catch (e) { setErr(e.message); setBusy(false); }
  }

  return html`<${ModalShell} title="New event" footer=${html`
    <span style=${{ flex: 1, fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>A thin dated node — details live on the linked pages</span>
    <${Btn} kind="ghost" disabled=${busy} onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${go}>${busy ? 'Saving…' : 'Create'}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <${Field} label="Title">
      <${Input} value=${title} onInput=${setTitle} placeholder="The Sack of Emberfall" autofocus
        onKeydown=${(e) => { if (e.key === 'Enter') go(); }} />
    </${Field}>
    <div style=${{ display: 'flex', gap: 10 }}>
      <div style=${{ flex: 1 }}><${Field} label="Date">
        <${Input} value=${date} onInput=${setDate} placeholder="1374-08-12 DR · ~890 · -500" />
      </${Field}></div>
      <div style=${{ flex: '0 0 110px' }}><${Field} label="…or order">
        <${Input} value=${order} onInput=${setOrder} placeholder="3" />
      </${Field}></div>
    </div>
    <${Field} label="Location">
      ${location
        ? html`<div><${PickedChip} label=${location} onRemove=${() => setLocation('')} /></div>`
        : html`<${PagePick} pages=${pages} onPick=${setLocation} placeholder="Link a place…" />`}
    </${Field}>
    <${Field} label="Participants">
      ${participants.length > 0 && html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 5, marginBottom: 6 }}>
        ${participants.map((p) => html`<${PickedChip} key=${p} label=${p} onRemove=${() => setParticipants(participants.filter((x) => x !== p))} />`)}
      </div>`}
      <${PagePick} pages=${pages} exclude=${participants} onPick=${(t) => setParticipants([...participants, t])} placeholder="Link people, factions…" />
    </${Field}>
    <${Field} label="Summary">
      <${Input} value=${summary} onInput=${setSummary} placeholder="One line for the timeline rail" />
    </${Field}>
  </${ModalShell}>`;
}

// ── Move a page/folder into another folder ─────────────────────────
function MovePageModal({ name, folders = [], current = '', onSubmit }) {
  const [dest, setDest] = useState(current);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const options = [{ value: '', label: 'Vault root' }, ...folders.map((f) => ({ value: f, label: f }))];
  async function go() {
    setBusy(true); setErr(null);
    try { await onSubmit(dest); closeModal(); }
    catch (e) { setErr(e.message); setBusy(false); }
  }
  return html`<${ModalShell} title=${html`Move <em style=${{ fontStyle: 'italic' }}>${name}</em>`} footer=${html`
    <${Btn} kind="ghost" disabled=${busy} onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${go}>${busy ? 'Moving…' : 'Move'}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <${Field} label="Destination folder">
      <${Select} value=${dest} onChange=${setDest} options=${options} />
    </${Field}>
  </${ModalShell}>`;
}

// World kinds for a select: schema-driven (incl. custom kinds), built-ins as fallback.
function kindOptions() {
  const schemas = store.kindSchemas || [];
  if (!schemas.length) return CODEX_KINDS;
  return schemas.map(({ kind }) => ({
    value: kind,
    label: (CODEX_KINDS.find((k) => k.value === kind) || {}).label || kind.charAt(0).toUpperCase() + kind.slice(1),
  }));
}

// ── New page (Phase 16): title + template — the template's frontmatter
// picks the kind. Falls back to a kind picker when a world has no templates.
function NewPageModal({ folder = '', kind: presetKind = 'npc', onCreated }) {
  const templates = store.templates || [];
  const defaultTpl = (templates.find((t) => t.kind === presetKind) || templates[0])?.name || '';
  const [title, setTitle] = useState('');
  const [tpl, setTpl] = useState(defaultTpl);
  const [kind, setKind] = useState(presetKind);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const hasTemplates = templates.length > 0;
  const pickedKind = templates.find((t) => t.name === tpl)?.kind;
  async function go() {
    const t = title.trim();
    if (!t) { setErr('Title is required'); return; }
    setBusy(true); setErr(null);
    try {
      const page = await createVaultPage(t, hasTemplates ? { template: tpl } : kind, folder);
      closeModal();
      if (onCreated) onCreated(page); else navigate('page', { path: page.path });
    } catch (e) { setErr(e.message); setBusy(false); }
  }
  const tplOptions = templates.map((t) => ({ value: t.name, label: t.kind ? `${t.name} · ${t.kind}` : t.name }));
  return html`<${ModalShell} title="New page" footer=${html`
    <span style=${{ flex: 1, fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>${folder ? `In ${folder}/ · ` : ''}${hasTemplates ? `Starts from the “${tpl}” template${pickedKind ? ` (${pickedKind})` : ''}` : "Starts from the kind's template"}</span>
    <${Btn} kind="ghost" disabled=${busy} onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${go}>${busy ? 'Creating…' : 'Create page'}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <${Field} label="Title">
      <${Input} value=${title} onInput=${setTitle} placeholder="Lord Ulric Tannerheim" autofocus
        onKeydown=${(e) => { if (e.key === 'Enter') go(); }} />
    </${Field}>
    ${hasTemplates
      ? html`<${Field} label="Template">
          <${Select} value=${tpl} onChange=${setTpl} options=${tplOptions} />
        </${Field}>`
      : html`<${Field} label="Kind">
          <${Select} value=${kind} onChange=${setKind} options=${kindOptions()} />
        </${Field}>`}
  </${ModalShell}>`;
}

// ── Promote (Phase 16): capture → kinded page with template applied ─
function PromotePageModal({ page, folders = [] }) {
  const currentDir = page.path.includes('/') ? page.path.slice(0, page.path.lastIndexOf('/')) : '';
  const [kind, setKind] = useState('npc');
  const [dest, setDest] = useState(() => folderForKind('npc', folders) ?? currentDir);
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  function folderForKind(k, all) {
    return all.find((f) => kindForFolder(f.split('/').pop()) === k) ?? null;
  }
  function pickKind(k) {
    setKind(k);
    if (!touched) setDest(folderForKind(k, folders) ?? currentDir);
  }
  const options = [{ value: '', label: 'Vault root' }, ...folders.map((f) => ({ value: f, label: f }))];
  async function go() {
    setBusy(true); setErr(null);
    try {
      const moved = await promoteVaultPage(page.path, kind, dest);
      setOp(`Promoted to ${moved.path}`, 'done');
      closeModal();
      navigate('page', { path: moved.path });
    } catch (e) { setErr(e.message); setBusy(false); }
  }
  return html`<${ModalShell} title=${html`Promote <em style=${{ fontStyle: 'italic' }}>${page.title}</em>`} footer=${html`
    <span style=${{ flex: 1, fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>Keeps your text — adds the kind's fields and headings</span>
    <${Btn} kind="ghost" disabled=${busy} onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${go}>${busy ? 'Promoting…' : 'Promote'}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <${Field} label="Kind">
      <${Select} value=${kind} onChange=${pickKind} options=${kindOptions()} />
    </${Field}>
    <${Field} label="Move to">
      <${Select} value=${dest} onChange=${(v) => { setTouched(true); setDest(v); }} options=${options} />
    </${Field}>
  </${ModalShell}>`;
}

// ── Page history (Phase 13A): versions → diff → restore ───────────

// Line diff (LCS) for the history viewer. Inputs are wiki pages — small
// enough for the quadratic table; degrade to whole-file replace beyond that.
function diffLines(aText, bText) {
  const a = (aText || '').split('\n');
  const b = (bText || '').split('\n');
  if (a.length * b.length > 500000) {
    return [...a.map((t) => ({ op: '-', t })), ...b.map((t) => ({ op: '+', t }))];
  }
  const n = a.length, m = b.length;
  const dp = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const out = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) { out.push({ op: ' ', t: a[i] }); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) out.push({ op: '-', t: a[i++] });
    else out.push({ op: '+', t: b[j++] });
  }
  while (i < n) out.push({ op: '-', t: a[i++] });
  while (j < m) out.push({ op: '+', t: b[j++] });
  return out;
}

function DiffLinesView({ before, after }) {
  const rows = diffLines(before, after);
  // Collapse long unchanged runs so the change stays in view.
  const out = [];
  let run = [];
  const flush = () => {
    if (run.length > 7) {
      out.push(run[0], run[1], { op: 'gap', n: run.length - 4 }, run[run.length - 2], run[run.length - 1]);
    } else out.push(...run);
    run = [];
  };
  for (const r of rows) {
    if (r.op === ' ') run.push(r);
    else { flush(); out.push(r); }
  }
  flush();
  const colors = {
    '+': { bg: 'var(--moss-50)', col: 'var(--moss)', sign: '+' },
    '-': { bg: 'var(--burgundy-50)', col: 'var(--burgundy-700)', sign: '−' },
    ' ': { bg: 'transparent', col: 'var(--ink-soft)', sign: ' ' },
  };
  return html`<pre style=${{ margin: 0, padding: '8px 0', background: 'var(--paper)', border: '1px solid var(--rule-soft)', borderRadius: 6, fontSize: 11.5, fontFamily: 'var(--font-mono)', lineHeight: 1.5, overflow: 'auto', maxHeight: '46vh' }}>
    ${out.map((r, k) => r.op === 'gap'
      ? html`<div key=${k} style=${{ padding: '0 10px', color: 'var(--ink-faint)', fontStyle: 'italic' }}>··· ${r.n} unchanged lines ···</div>`
      : html`<div key=${k} style=${{ padding: '0 10px', whiteSpace: 'pre-wrap', background: colors[r.op].bg, color: colors[r.op].col }}>${colors[r.op].sign} ${r.t}</div>`)}
  </pre>`;
}

function OriginChip({ origin }) {
  const keeper = origin === 'keeper';
  return html`<span style=${{
    display: 'inline-flex', alignItems: 'center', gap: 4, padding: '1px 8px', borderRadius: 999,
    fontSize: 10.5, fontWeight: 600, letterSpacing: '0.04em', textTransform: 'uppercase',
    background: keeper ? 'var(--burgundy-50)' : 'var(--moss-50)',
    color: keeper ? 'var(--burgundy)' : 'var(--moss)',
    border: `1px solid ${keeper ? 'rgba(122,46,31,.22)' : 'rgba(74,93,58,.22)'}`,
  }}>${keeper ? html`<${Icon} name="feather" size=${9} />` : null}${keeper ? 'Keeper' : 'You'}</span>`;
}

// Each version is the page as it was *before* that save — the diff of a row
// is therefore "what that save changed". Restore writes the before-state back.
function PageHistoryModal({ path, onRestored }) {
  const id = store.campaign?.campaign_id;
  const [versions, setVersions] = useState(null); // newest first
  const [current, setCurrent] = useState(null);   // live file content
  const [sel, setSel] = useState(null);           // selected ts
  const [snaps, setSnaps] = useState({});         // ts → content (null = absent)
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const title = path.split('/').pop().replace(/\.md$/, '');

  useEffect(() => {
    loadPageHistory(path).then((v) => {
      const list = [...v].reverse();
      setVersions(list);
      if (list.length) setSel(list[0].ts);
    });
    apiFetch(`/campaigns/${id}/vault/pages/${encodeURI(path)}`)
      .then((p) => setCurrent(p.content))
      .catch(() => setCurrent('')); // deleted/missing page still shows history
  }, [path]);

  // The selected version + the one after it (or the live file) make the diff.
  useEffect(() => {
    if (sel == null || !versions) return;
    const k = versions.findIndex((v) => v.ts === sel);
    const need = [versions[k], versions[k - 1]].filter(Boolean).filter((v) => !(v.ts in snaps));
    need.forEach((v) => {
      readPageVersion(path, v.ts)
        .then((r) => setSnaps((s) => ({ ...s, [v.ts]: r.content })))
        .catch(() => setSnaps((s) => ({ ...s, [v.ts]: '' })));
    });
  }, [sel, versions]);

  async function restore() {
    setBusy(true); setErr(null);
    try {
      const r = await restorePageVersion(path, sel);
      closeModal();
      setOp(r.deleted ? `${title} moved to trash (it didn't exist at that point)` : `${title} restored`, 'done');
      if (onRestored) onRestored(r);
    } catch (e) { setErr(e.message); setBusy(false); }
  }

  const k = versions ? versions.findIndex((v) => v.ts === sel) : -1;
  const before = sel != null ? snaps[sel] : undefined;
  const after = k > 0 ? snaps[versions[k - 1].ts] : current;
  const ready = before !== undefined && after !== undefined && after !== null;

  return html`<${ModalShell} wide title=${html`History — <em style=${{ fontStyle: 'italic' }}>${title}</em>`} footer=${html`
    ${err && html`<span style=${{ color: 'var(--burgundy-700)', fontSize: 12.5, marginRight: 'auto' }}>${err}</span>`}
    <${Btn} kind="ghost" disabled=${busy} onClick=${closeModal}>Close</${Btn}>
    <${Btn} kind="primary" disabled=${busy || sel == null} onClick=${restore}>${busy ? 'Restoring…' : 'Restore this state'}</${Btn}>`}>
    ${versions === null ? html`<${Spinner} />`
      : versions.length === 0
        ? html`<div style=${{ fontSize: 13, color: 'var(--ink-faint)', fontStyle: 'italic' }}>
            No saved versions yet — a snapshot is taken on every save from now on.
          </div>`
        : html`<div style=${{ display: 'grid', gridTemplateColumns: '210px 1fr', gap: 14, minHeight: 0 }}>
            <div style=${{ overflow: 'auto', maxHeight: '52vh', display: 'flex', flexDirection: 'column', gap: 2 }}>
              ${versions.map((v) => html`<div key=${v.ts} onClick=${() => setSel(v.ts)} style=${{
                display: 'flex', alignItems: 'center', gap: 7, padding: '6px 8px', borderRadius: 5, cursor: 'pointer',
                background: sel === v.ts ? 'var(--burgundy-50)' : 'transparent',
                boxShadow: sel === v.ts ? 'inset 2px 0 0 var(--burgundy)' : 'none',
              }}>
                <${OriginChip} origin=${v.origin} />
                <span style=${{ fontSize: 11.5, fontFamily: 'var(--font-mono)', color: 'var(--ink-soft)' }}>${fmtDateTime(v.ts)}</span>
              </div>`)}
            </div>
            <div style=${{ minWidth: 0 }}>
              <div style=${{ fontSize: 11.5, color: 'var(--ink-muted)', marginBottom: 6 }}>
                What this save changed${before === null ? ' (page created by it)' : ''} — restoring returns the page to the state <em>before</em> it.
              </div>
              ${ready ? html`<${DiffLinesView} before=${before || ''} after=${after || ''} />` : html`<${Spinner} />`}
            </div>
          </div>`}
  </${ModalShell}>`;
}

// World-wide feed: every save, filterable to "everything the Keeper changed".
function WorldHistoryModal() {
  const [origin, setOrigin] = useState('keeper');
  const [rows, setRows] = useState(null);
  useEffect(() => {
    setRows(null);
    loadWorldHistory(origin === 'all' ? null : origin, 150).then(setRows);
  }, [origin]);
  const tabs = [['keeper', 'The Keeper'], ['user', 'You'], ['all', 'All']];
  return html`<${ModalShell} title="World history" footer=${html`<${Btn} kind="ghost" onClick=${closeModal}>Close</${Btn}>`}>
    <div style=${{ display: 'flex', gap: 4, padding: 3, background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 5, alignSelf: 'flex-start' }}>
      ${tabs.map(([v, label]) => html`<button key=${v} onClick=${() => setOrigin(v)} style=${{
        padding: '4px 10px', borderRadius: 3, border: 'none', cursor: 'pointer', fontSize: 12,
        background: origin === v ? 'var(--paper-deep)' : 'transparent',
        color: origin === v ? 'var(--ink)' : 'var(--ink-muted)', fontWeight: origin === v ? 500 : 400,
      }}>${label}</button>`)}
    </div>
    ${rows === null ? html`<${Spinner} />`
      : rows.length === 0
        ? html`<div style=${{ fontSize: 13, color: 'var(--ink-faint)', fontStyle: 'italic' }}>
            ${origin === 'keeper' ? 'The Keeper has not changed any pages yet.' : 'No saved versions yet.'}
          </div>`
        : html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 2, overflow: 'auto', maxHeight: '54vh' }}>
            ${rows.map((r, i) => html`<div key=${i} onClick=${() => openModal('pageHistory', { path: r.path })}
              style=${{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 8px', borderRadius: 5, cursor: 'pointer' }}
              onMouseEnter=${(e) => { e.currentTarget.style.background = 'var(--paper-deep)'; }}
              onMouseLeave=${(e) => { e.currentTarget.style.background = 'transparent'; }}>
              <${OriginChip} origin=${r.origin} />
              <span style=${{ flex: 1, fontSize: 12.5, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${r.path.replace(/\.md$/, '')}</span>
              <span style=${{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)' }}>${fmtDateTime(r.ts)}</span>
            </div>`)}
          </div>`}
  </${ModalShell}>`;
}

// ── Trash (Phase 13D): list groups, restore, empty ─────────────────
function TrashModal() {
  const [groups, setGroups] = useState(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const reload = () => loadTrash().then(setGroups);
  useEffect(() => { reload(); }, []);

  async function run(fn, doneMsg) {
    setBusy(true); setErr(null);
    try { await fn(); await reload(); if (doneMsg) setOp(doneMsg, 'done'); }
    catch (e) { setErr(e.message); }
    setBusy(false);
  }

  const itemLabel = (it) => {
    const name = it.rel.split('/').pop().replace(/\.md$/, '');
    if (it.kind === 'folder') return `${name}/ (${it.pages} page${it.pages === 1 ? '' : 's'})`;
    return name;
  };

  return html`<${ModalShell} title="Trash" footer=${html`
    ${err && html`<span style=${{ color: 'var(--burgundy-700)', fontSize: 12.5, marginRight: 'auto' }}>${err}</span>`}
    ${groups && groups.length > 0 && html`<${Btn} kind="ghost" disabled=${busy}
      onClick=${() => run(() => emptyTrash(), 'Trash emptied')}>Empty trash</${Btn}>`}
    <${Btn} kind="primary" onClick=${closeModal}>Close</${Btn}>`}>
    <div style=${{ fontSize: 12, color: 'var(--ink-muted)', lineHeight: 1.5 }}>
      Deleted pages and folders live in the world's own trash (<code>.ck/trash</code>) for 30 days, then vanish.
    </div>
    ${groups === null ? html`<${Spinner} />`
      : groups.length === 0
        ? html`<div style=${{ fontSize: 13, color: 'var(--ink-faint)', fontStyle: 'italic' }}>The trash is empty.</div>`
        : html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 8, overflow: 'auto', maxHeight: '50vh' }}>
            ${groups.map((g) => html`<div key=${g.id} style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 12px', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 6 }}>
              <${Icon} name="trash" size=${13} className="ck-ink-faint" />
              <div style=${{ flex: 1, minWidth: 0 }}>
                <div style=${{ fontSize: 13, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                  ${g.items.map(itemLabel).join(', ')}
                </div>
                <div style=${{ fontSize: 11, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)', marginTop: 2 }}>deleted ${fmtDateTime(g.deleted_at * 1000)}</div>
              </div>
              <${Btn} kind="secondary" size="sm" disabled=${busy}
                onClick=${() => run(() => restoreTrash(g.id), 'Restored from trash')}>Restore</${Btn}>
              <${Btn} kind="ghost" size="sm" icon="x" title="Delete forever" disabled=${busy}
                onClick=${() => run(() => emptyTrash(g.id))} />
            </div>`)}
          </div>`}
  </${ModalShell}>`;
}

// ── Codex import (paste notes → LLM distills → review → save) ──────
function CodexImportModal() {
  const [text, setText] = useState('');
  const [rows, setRows] = useState(null); // null = not extracted; [] = extracted, none found
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState(null);
  const [err, setErr] = useState(null);

  // Read picked files (or a picked folder, via webkitdirectory) into the textarea.
  // Pure client-side — the browser hands us the file contents; nothing reads
  // arbitrary disk paths. Each file is prefixed with its name so the model can use
  // the filename as the entity name in one-NPC-per-file vaults.
  async function onFiles(e) {
    const all = [...(e.target.files || [])];
    e.target.value = ''; // allow re-picking the same files
    const files = all.filter((f) => /\.(md|markdown|txt|text)$/i.test(f.name));
    if (!files.length) {
      if (all.length) setErr('No .md or .txt files in that selection.');
      return;
    }
    setErr(null);
    try {
      const parts = await Promise.all(files.map(async (f) => {
        const base = f.name.replace(/\.(md|markdown|txt|text)$/i, '');
        return `\n\n# ${base}\n${await f.text()}`;
      }));
      setText((t) => (t + parts.join('')).trim());
      setProgress(`Loaded ${files.length} ${files.length === 1 ? 'file' : 'files'} — review or extract.`);
    } catch (e) { setErr(`Could not read files: ${e.message}`); }
  }

  async function extract() {
    const chunks = chunkNotes(text);
    setBusy(true); setErr(null);
    try {
      const merged = [];
      const idx = new Map(); // name|kind → position in merged
      for (let i = 0; i < chunks.length; i++) {
        setProgress(chunks.length > 1 ? `Reading batch ${i + 1}/${chunks.length}…` : null);
        const entries = await importCodex(chunks[i]);
        for (const en of entries) {
          if (!en.name) continue;
          const k = `${en.name.toLowerCase()}|${en.kind}`;
          if (idx.has(k)) {
            // Same entity seen twice (e.g. its own file + a mention elsewhere):
            // last write wins — the later batch overrides body + detail.
            merged[idx.get(k)] = en;
          } else {
            idx.set(k, merged.length); merged.push(en);
          }
        }
      }
      // New entries default checked; ones already in the codex default unchecked
      // so a re-import never silently overwrites — tick to replace.
      setRows(merged.map((e) => ({ ...e, on: !e.exists })));
    } catch (e) { setErr(e.message); }
    setProgress(null); setBusy(false);
  }
  async function save() {
    const picked = (rows || []).filter((r) => r.on && r.name.trim())
      .map(({ name, kind, body, detail }) => ({ name: name.trim(), kind, body: (body || '').trim(), detail: (detail || '').trim() }));
    if (!picked.length) { setErr('Nothing selected to save.'); return; }
    setBusy(true); setErr(null);
    try {
      await commitCodexImport(picked);
      closeModal();
    } catch (e) { setErr(e.message); setBusy(false); }
  }
  const upd = (i, k, v) => setRows((rs) => rs.map((r, j) => j === i ? { ...r, [k]: v } : r));
  const setAll = (on) => setRows((rs) => rs.map((r) => ({ ...r, on })));
  const selectOnlyKind = (kind) => setRows((rs) => rs.map((r) => ({ ...r, on: r.kind === kind })));
  const pickedCount = (rows || []).filter((r) => r.on && r.name.trim()).length;
  const allOn = (rows || []).length > 0 && pickedCount === rows.length;
  const kindCounts = CODEX_KINDS
    .map((k) => ({ ...k, n: (rows || []).filter((r) => r.kind === k.value).length }))
    .filter((k) => k.n > 0);

  const footer = rows === null
    ? html`<${Btn} kind="ghost" onClick=${closeModal}>Cancel</${Btn}>
        <${Btn} kind="primary" icon="sparkle" disabled=${busy || !text.trim()} onClick=${extract}>${busy ? (progress || 'Reading…') : 'Extract entries'}</${Btn}>`
    : html`<${Btn} kind="ghost" onClick=${() => setRows(null)}>Back</${Btn}>
        <${Btn} kind="primary" icon="check" disabled=${busy || !pickedCount} onClick=${save}>${busy ? 'Saving…' : `Save ${pickedCount} ${pickedCount === 1 ? 'entry' : 'entries'}`}</${Btn}>`;

  return html`<${ModalShell} wide title="Import to codex" footer=${footer}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    ${rows === null ? html`
      <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', lineHeight: 1.5 }}>
        Point at your notes — pick the files (one-per-NPC vaults: select them all at once), or paste
        text from Obsidian, Notion, a Google-Doc export, anything. The model sorts them into entries
        (name · kind · one line). You review before anything is saved.
      </div>
      <div style=${{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
        <input id="ck-codex-dir" type="file" webkitdirectory directory
          onChange=${onFiles} style=${{ display: 'none' }} />
        <${Btn} kind="secondary" size="sm" icon="folder" disabled=${busy}
          onClick=${() => document.getElementById('ck-codex-dir')?.click()}>Add a folder…</${Btn}>
        <input id="ck-codex-files" type="file" multiple accept=".md,.markdown,.txt,.text"
          onChange=${onFiles} style=${{ display: 'none' }} />
        <${Btn} kind="secondary" size="sm" icon="upload" disabled=${busy}
          onClick=${() => document.getElementById('ck-codex-files')?.click()}>Add files…</${Btn}>
        <span style=${{ fontSize: 11.5, color: 'var(--ink-faint)' }}>
          .md / .txt — each file is added below, named by its filename. Big vaults are read in batches.
        </span>
      </div>
      ${!busy && progress && html`<div style=${{ fontSize: 12, color: 'var(--moss)' }}>${progress}</div>`}
      ${busy ? html`<div style=${{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 8, padding: 24 }}>
          <${Spinner} />${progress && html`<span style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>${progress}</span>`}
        </div>`
        : html`<${Textarea} value=${text} onInput=${setText} rows=${14} placeholder="# Lord Ulric Tannerheim\nPatrician of Neverwinter who owns the docks…\n\n…or click “Choose files…” above." />`}
    ` : rows.length === 0 ? html`
      <div style=${{ fontSize: 13, color: 'var(--ink-muted)', padding: '10px 0' }}>
        The model found nothing glossary-worthy in that text. Go back and try a different selection.
      </div>
    ` : html`
      <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', lineHeight: 1.5 }}>
        ${rows.length} found. Uncheck anything wrong, fix names or kinds, then save.
        ${rows.some((r) => r.exists) && html`<br />Rows marked <strong>in codex</strong> already exist (often picked up from another note) — they're off by default. Tick one to replace its description with this one.`}
      </div>
      <div style=${{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap', paddingBottom: 2 }}>
        <label style=${{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12.5, color: 'var(--ink-soft)', cursor: 'pointer' }}>
          <input type="checkbox" checked=${allOn} onChange=${(e) => setAll(e.target.checked)} style=${{ cursor: 'pointer' }} />
          ${allOn ? 'Deselect all' : 'Select all'}${rows.some((r) => r.exists) ? ' (overwrites existing)' : ''}
        </label>
        ${kindCounts.length > 1 && html`<span style=${{ fontSize: 11.5, color: 'var(--ink-faint)' }}>· only:</span>
          ${kindCounts.map((k) => html`<button key=${k.value} onClick=${() => selectOnlyKind(k.value)}
            style=${{ fontSize: 11.5, color: 'var(--ink-soft)', border: '1px solid var(--rule)', background: 'var(--surface)', borderRadius: 999, padding: '2px 9px', cursor: 'pointer' }}>
            ${k.label}s <span style=${{ fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)' }}>${k.n}</span>
          </button>`)}`}
        <span style=${{ flex: 1 }} />
        <span style=${{ fontSize: 11.5, color: 'var(--ink-faint)' }}>${pickedCount} of ${rows.length} selected</span>
      </div>
      <div style=${{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        ${rows.map((r, i) => html`<div key=${i} style=${{
          display: 'grid', gridTemplateColumns: 'auto 1.4fr 0.9fr 2fr', gap: 8, alignItems: 'center',
          padding: '6px 8px', borderRadius: 6, background: r.on ? 'var(--surface)' : 'transparent',
          border: '1px solid var(--rule-soft)', opacity: r.on ? 1 : 0.55,
        }}>
          <input type="checkbox" checked=${r.on} onChange=${(e) => upd(i, 'on', e.target.checked)} style=${{ cursor: 'pointer' }} />
          <div style=${{ display: 'flex', alignItems: 'center', gap: 5, minWidth: 0 }}>
            <${Input} value=${r.name} onInput=${(v) => upd(i, 'name', v)} placeholder="Name" />
            ${r.exists && html`<span style=${{ flex: '0 0 auto', fontSize: 9.5, fontWeight: 600, letterSpacing: '0.04em', textTransform: 'uppercase', color: 'var(--ochre)', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.24)', borderRadius: 999, padding: '1px 6px' }} title="Already in the codex — tick to replace">in codex</span>`}
          </div>
          <${Select} value=${r.kind} onChange=${(v) => upd(i, 'kind', v)} options=${CODEX_KINDS} />
          <${Input} value=${r.body} onInput=${(v) => upd(i, 'body', v)} placeholder="One-line description" />
        </div>`)}
      </div>
    `}
  </${ModalShell}>`;
}

// ── Summary prompt template editor ────────────────────────────────
function PromptTemplateModal({ edit }) {
  const [label, setLabel] = useState(edit?.label || '');
  const [text, setText] = useState(edit?.text || '');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);

  async function save() {
    if (!label.trim()) { setErr('Name is required'); return; }
    if (!text.trim()) { setErr('Prompt text is required'); return; }
    setBusy(true); setErr(null);
    try {
      if (edit) await updatePromptTemplate(edit.id, { label: label.trim(), text });
      else await createPromptTemplate({ label: label.trim(), text });
      closeModal();
    } catch (e) { setErr(e.message); setBusy(false); }
  }

  return html`<${ModalShell} wide title=${edit ? 'Edit prompt template' : 'New prompt template'} footer=${html`
    <${Btn} kind="ghost" onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${save}>${busy ? 'Saving…' : (edit ? 'Save changes' : 'Create template')}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    ${edit?.builtin && html`<div style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>This is a built-in template. Your edits stick; "Restore defaults" only brings back ones you've deleted.</div>`}
    <${Field} label="Name"><${Input} value=${label} onInput=${setLabel} placeholder="e.g. English – D&D / TTRPG" /></${Field}>
    <${Field} label="System prompt" hint="The instructions sent to the LLM. The transcript and campaign context are appended automatically.">
      <${Textarea} value=${text} onInput=${setText} rows=${16} placeholder="You are an RPG assistant for the GM…" />
    </${Field}>
  </${ModalShell}>`;
}

// ── Content viewer ────────────────────────────────────────────────
function ViewerModal({ title, text }) {
  return html`<${ModalShell} wide title=${title} footer=${html`<${Btn} kind="secondary" onClick=${closeModal}>Close</${Btn}>`}>
    <pre class="ck-pre">${text}</pre>
  </${ModalShell}>`;
}

// ── Vault folder enhance picker ───────────────────────────────────
function EnhanceFolderModal() {
  const pages = store.vaultPages || [];
  const folders = store.vaultFolders || [];

  const needsEnhance = (p) => !p.kind || !p.summary;
  const topFolders = folders.filter((f) => !f.includes('/'));

  const countFor = (f) => f === ''
    ? pages.filter((p) => !p.path.includes('/') && needsEnhance(p)).length
    : pages.filter((p) => p.path.startsWith(f + '/') && needsEnhance(p)).length;

  const items = [
    { path: '', label: '(root)' },
    ...topFolders.map((f) => ({ path: f, label: f })),
  ].map((x) => ({ ...x, count: countFor(x.path) })).filter((x) => x.count > 0);

  const total = items.reduce((s, x) => s + x.count, 0);
  const [checked, setChecked] = useState(() => new Set(items.map((x) => x.path)));

  const toggle = (path) => setChecked((s) => { const n = new Set(s); n.has(path) ? n.delete(path) : n.add(path); return n; });
  const selectedItems = items.filter((x) => checked.has(x.path));
  const selectedCount = selectedItems.reduce((s, x) => s + x.count, 0);
  const selectedFolders = selectedItems.map((x) => x.path);

  function go() {
    closeModal();
    enhanceVaultPages(selectedFolders).catch(() => {});
  }

  if (total === 0) {
    return html`<${ModalShell} title="Enhance with AI" footer=${html`<${Btn} kind="primary" onClick=${closeModal}>Close</${Btn}>`}>
      <div style=${{ fontSize: 13.5, color: 'var(--ink-soft)', lineHeight: 1.5 }}>All pages already have kind and summary — nothing to enhance.</div>
    </${ModalShell}>`;
  }

  return html`<${ModalShell} title="Enhance pages with AI" footer=${html`
    <${Btn} kind="ghost" onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" icon="sparkle" disabled=${!selectedFolders.length} onClick=${go}>
      Enhance ${selectedCount} page${selectedCount === 1 ? '' : 's'}
    </${Btn}>`}>
    <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', lineHeight: 1.5 }}>
      Select which folders to enhance. Pages in the same folder are sent as one batch — far fewer LLM calls.
      Pages that already have kind and summary are skipped automatically.
    </div>
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      ${items.map((x) => html`<label key=${x.path} style=${{
        display: 'flex', alignItems: 'center', gap: 10, padding: '8px 12px',
        borderRadius: 6, cursor: 'pointer',
        background: checked.has(x.path) ? 'var(--surface)' : 'transparent',
        border: '1px solid ' + (checked.has(x.path) ? 'var(--rule)' : 'var(--rule-soft)'),
      }}>
        <input type="checkbox" checked=${checked.has(x.path)} onChange=${() => toggle(x.path)}
          style=${{ cursor: 'pointer', width: 14, height: 14, flexShrink: 0 }} />
        <span style=${{ flex: 1, fontSize: 13, fontFamily: 'var(--font-mono)', color: 'var(--ink)' }}>${x.label}</span>
        <span style=${{ fontSize: 11.5, color: 'var(--ink-faint)' }}>${x.count} page${x.count === 1 ? '' : 's'}</span>
      </label>`)}
    </div>
  </${ModalShell}>`;
}

// ── Export world (Phase 3) ────────────────────────────────────────
function ExportWorldModal() {
  const [audio, setAudio] = useState(true);
  const [busy, setBusy] = useState(false);
  const [path, setPath] = useState(null);
  const [err, setErr] = useState(null);

  async function run() {
    setBusy(true); setErr(null);
    try { setPath((await exportWorld(audio)).path); }
    catch (e) { setErr(e.message || 'Export failed'); }
    finally { setBusy(false); }
  }

  return html`<${ModalShell} title="Export world as ZIP" footer=${path
    ? html`<${Btn} kind="ghost" icon="folder" onClick=${() => revealPath(path)}>Reveal in file manager</${Btn}><${Btn} kind="primary" onClick=${closeModal}>Done</${Btn}>`
    : html`<${Btn} kind="primary" icon="download" disabled=${busy} onClick=${run}>${busy ? html`<${Spinner} size=${14} /> Zipping…` : 'Export ZIP'}</${Btn}>`}>
    <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', lineHeight: 1.5 }}>
      Your world already is a portable folder — this zips it next to the folder itself.
      The page index cache is left out; it rebuilds automatically.
    </div>
    ${!path && html`<label style=${{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 13, cursor: 'pointer' }}>
      <input type="checkbox" checked=${audio} onChange=${() => setAudio(!audio)} style=${{ width: 14, height: 14, cursor: 'pointer' }} />
      Include session audio <span style=${{ fontSize: 11.5, color: 'var(--ink-faint)' }}>(can be gigabytes)</span>
    </label>`}
    ${path && html`<div style=${{ fontSize: 12, fontFamily: 'var(--font-mono)', color: 'var(--ink-soft)', wordBreak: 'break-all' }}>${path}</div>`}
    ${err && html`<div style=${{ fontSize: 12.5, color: 'var(--burgundy)' }}>${err}</div>`}
  </${ModalShell}>`;
}

// ── Vault diagnostics (Phase 3) ───────────────────────────────────
function DiagSection({ title, items, render }) {
  if (!items || items.length === 0) return null;
  return html`<div>
    <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 6 }}>
      ${title} <span style=${{ fontFamily: 'var(--font-mono)' }}>${items.length}</span>
    </div>
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 2 }}>${items.map(render)}</div>
  </div>`;
}

function DiagRow({ onClick, children, title }) {
  const clickable = !!onClick;
  return html`<div onClick=${onClick} title=${title || ''}
    style=${{ display: 'flex', alignItems: 'center', gap: 8, padding: '5px 8px', borderRadius: 5, fontSize: 12.5, cursor: clickable ? 'pointer' : 'default', color: 'var(--ink-soft)' }}
    onMouseEnter=${clickable ? (e) => { e.currentTarget.style.background = 'rgba(120,90,40,.08)'; } : null}
    onMouseLeave=${clickable ? (e) => { e.currentTarget.style.background = 'transparent'; } : null}>
    ${children}
  </div>`;
}

const mono = { fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--ink-faint)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' };

function VaultDiagnosticsModal() {
  const [diag, setDiag] = useState(store.vaultDiag || null);
  useEffect(() => { loadVaultDiagnostics().then((r) => { if (r) setDiag(r); }); }, []);
  const open = (path) => { closeModal(); navigate('page', { path }); };
  const openMd = (path) => (path.endsWith('.md') ? () => open(path) : null);

  const empty = diag && !diag.broken_links.length && !diag.broken_media.length
    && !diag.orphans.length && !diag.conflicts.length && !diag.scan_errors.length;

  return html`<${ModalShell} title="Vault diagnostics" wide>
    ${!diag && html`<${Spinner} />`}
    ${empty && html`<div style=${{ fontSize: 13, color: 'var(--ink-muted)' }}>All clear — no broken links, orphans, or conflicts.</div>`}
    ${diag && html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <${DiagSection} title="Broken links" items=${diag.broken_links} render=${(b, i) => html`<${DiagRow} key=${i} onClick=${() => open(b.source_path)} title="Open the page containing this link">
        <span style=${{ width: 6, height: 6, borderRadius: '50%', background: 'var(--ochre)', flex: '0 0 auto' }} />
        <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 12 }}>[[${b.link_text}]]</span>
        <span style=${{ flex: 1 }} /><span style=${mono}>${b.source_path}</span>
      </${DiagRow}>`} />
      <${DiagSection} title="Broken image embeds" items=${diag.broken_media} render=${(m, i) => html`<${DiagRow} key=${i} onClick=${() => open(m.source_path)} title="Open the page containing this embed">
        <span style=${{ width: 6, height: 6, borderRadius: '50%', background: 'var(--ochre)', flex: '0 0 auto' }} />
        <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 12 }}>![[${m.target}]]</span>
        <span style=${{ flex: 1 }} /><span style=${mono}>${m.source_path}</span>
      </${DiagRow}>`} />
      <${DiagSection} title="Orphan pages (nothing links here)" items=${diag.orphans} render=${(o) => html`<${DiagRow} key=${o.path} onClick=${() => open(o.path)}>
        <span style=${{ width: 6, height: 6, borderRadius: '50%', background: 'var(--rule-strong)', flex: '0 0 auto' }} />
        <span>${o.title}</span>
        <span style=${{ flex: 1 }} /><span style=${mono}>${o.path}</span>
      </${DiagRow}>`} />
      <${DiagSection} title="Sync-conflict files (resolve by hand)" items=${diag.conflicts} render=${(c) => html`<${DiagRow} key=${c} onClick=${openMd(c)}>
        <span style=${{ width: 6, height: 6, borderRadius: '50%', background: 'var(--burgundy)', flex: '0 0 auto' }} />
        <span style=${{ ...mono, flex: 1, color: 'var(--ink-soft)' }}>${c}</span>
      </${DiagRow}>`} />
      <${DiagSection} title="Unreadable files" items=${diag.scan_errors} render=${(s) => html`<${DiagRow} key=${s.path} title=${s.error}>
        <span style=${{ width: 6, height: 6, borderRadius: '50%', background: 'var(--burgundy)', flex: '0 0 auto' }} />
        <span style=${{ ...mono, flex: 1, color: 'var(--ink-soft)' }}>${s.path}</span>
        <span style=${{ fontSize: 11, color: 'var(--ink-faint)' }}>${s.error}</span>
      </${DiagRow}>`} />
    </div>`}
  </${ModalShell}>`;
}

// ── Host ──────────────────────────────────────────────────────────
export function ModalHost({ modal }) {
  if (!modal) return null;
  switch (modal.kind) {
    case 'campaign': return html`<${CampaignModal} ...${modal.props} />`;
    case 'session': return html`<${SessionModal} ...${modal.props} />`;
    case 'export': return html`<${ExportModal} />`;
    case 'provider': return html`<${ProviderModal} ...${modal.props} />`;
    case 'codexImport': return html`<${CodexImportModal} />`;
    case 'promptTemplate': return html`<${PromptTemplateModal} ...${modal.props} />`;
    case 'viewer': return html`<${ViewerModal} ...${modal.props} />`;
    case 'confirm': return html`<${ConfirmModal} ...${modal.props} />`;
    case 'textPrompt': return html`<${TextPromptModal} ...${modal.props} />`;
    case 'movePage': return html`<${MovePageModal} ...${modal.props} />`;
    case 'newPage': return html`<${NewPageModal} ...${modal.props} />`;
    case 'promotePage': return html`<${PromotePageModal} ...${modal.props} />`;
    case 'enhanceFolder': return html`<${EnhanceFolderModal} />`;
    case 'vaultDiag': return html`<${VaultDiagnosticsModal} />`;
    case 'pageHistory': return html`<${PageHistoryModal} ...${modal.props} />`;
    case 'worldHistory': return html`<${WorldHistoryModal} />`;
    case 'trash': return html`<${TrashModal} />`;
    case 'commandPalette': return html`<${CommandPalette} />`;
    case 'shortcuts': return html`<${ShortcutsModal} />`;
    case 'quickCapture': return html`<${QuickCaptureModal} />`;
    case 'newEvent': return html`<${NewEventModal} ...${modal.props} />`;
    case 'exportWorld': return html`<${ExportWorldModal} />`;
    default: return null;
  }
}
