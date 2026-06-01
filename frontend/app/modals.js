// Overlay forms: campaign editor, session editor, transcribe, export, provider, viewer.
import { html, useState, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { store, closeModal, setOp } from './core.js';
import { Icon, Btn, Field, Input, Textarea, Select, Spinner } from './ui.js';
import {
  createCampaign, updateCampaign, saveSessionMetadata, loadSession,
  runExport,
  loadLlmProviders, saveLlmProvider, testLlmProvider,
  importCodex, commitCodexImport,
  createPromptTemplate, updatePromptTemplate,
} from './actions.js';

const PRONOUNS = ['she/her', 'he/him', 'they/them'];

const CODEX_KINDS = [
  { value: 'pc', label: 'PC' }, { value: 'npc', label: 'NPC' }, { value: 'place', label: 'Place' },
  { value: 'faction', label: 'Faction' }, { value: 'item', label: 'Item' },
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
    <div class="ck" style=${{ width: wide ? 720 : 480, maxWidth: '100%', maxHeight: '88vh', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 12, boxShadow: 'var(--shadow-raised)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
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
    players: edit.players?.length ? edit.players : [], extra_info: edit.extra_info || '', start: edit.next_session_number || 1,
  } : { name: '', system: '', setting: '', default_language: '', gm: '', gm_pronouns: '', players: [{ player_name: '', character_name: '', pronouns: '' }], extra_info: '', start: 1 });
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

  return html`<${ModalShell} title=${edit ? 'Edit campaign' : 'New campaign'} footer=${html`
    <${Btn} kind="ghost" onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${save}>${busy ? 'Saving…' : (edit ? 'Save changes' : 'Create campaign')}</${Btn}>`}>
    ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 13 }}>${err}</div>`}
    <${Field} label="Campaign name"><${Input} value=${f.name} onInput=${(v) => set('name', v)} placeholder="The Iron Crown" /></${Field}>
    <div style=${{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
      <${Field} label="System"><${Input} value=${f.system} onInput=${(v) => set('system', v)} placeholder="D&D 5e" /></${Field}>
      <${Field} label="Setting"><${Input} value=${f.setting} onInput=${(v) => set('setting', v)} placeholder="Forgotten Realms" /></${Field}>
      <${Field} label="GM / DM"><div style=${{ display: 'flex', gap: 6 }}>
        <${Input} value=${f.gm} onInput=${(v) => set('gm', v)} />
        <${PronounSelect} value=${f.gm_pronouns} onChange=${(v) => set('gm_pronouns', v)} />
      </div></${Field}>
      <${Field} label="Default language"><${Input} value=${f.default_language} onInput=${(v) => set('default_language', v)} placeholder="en" mono /></${Field}>
    </div>
    ${!edit && html`<${Field} label="Start session #" hint="First session number for this campaign."><${Input} type="number" value=${f.start} onInput=${(v) => set('start', v)} style=${{ width: 120 }} /></${Field}>`}
    <${Field} label="Players"><${PlayerRows} players=${f.players} onChange=${(p) => set('players', p)} /></${Field}>
    <${Field} label="Additional information" hint="World frame or special campaign notes."><${Textarea} value=${f.extra_info} onInput=${(v) => set('extra_info', v)} rows=${3} /></${Field}>
  </${ModalShell}>`;
}

// ── Session metadata edit ─────────────────────────────────────────
function SessionModal({ session }) {
  const cam = session.campaign || {};
  const md = session.metadata || {};
  const [f, setF] = useState({
    title: cam.title || '', date: cam.date || '', number: cam.session_number || '', notes: cam.notes || '',
    characters: (md.characters || []).join(', '), locations: (md.locations || []).join(', '),
    items: (md.items || []).join(', '), tags: (md.tags || []).join(', '),
  });
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const set = (k, v) => setF((s) => ({ ...s, [k]: v }));
  const split = (s) => s.split(',').map((x) => x.trim()).filter(Boolean);

  async function save() {
    setBusy(true); setErr(null);
    try {
      await saveSessionMetadata({
        session_id: session.session_id, campaign_id: cam.campaign_id || null,
        session_number: Number(f.number) || null, title: f.title.trim() || null, date: f.date || null,
        metadata: { characters: split(f.characters), locations: split(f.locations), events: md.events || [], items: split(f.items), tags: split(f.tags) },
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
    <${Field} label="Characters" hint="Comma-separated."><${Input} value=${f.characters} onInput=${(v) => set('characters', v)} /></${Field}>
    <${Field} label="Locations" hint="Comma-separated."><${Input} value=${f.locations} onInput=${(v) => set('locations', v)} /></${Field}>
    <${Field} label="Items" hint="Comma-separated."><${Input} value=${f.items} onInput=${(v) => set('items', v)} /></${Field}>
    <${Field} label="Tags" hint="Comma-separated."><${Input} value=${f.tags} onInput=${(v) => set('tags', v)} /></${Field}>
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
  if (!p) { closeModal(); return null; }

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
    <${Field} label="Default model" hint=${p.id === 'ollama' ? 'Must match a model pulled in Ollama.' : (p.models?.length ? 'Pick a suggestion or type any model name.' : 'Type the exact model id (e.g. from ollama.com).')}>
      <${Input} value=${model} onInput=${setModel} mono list=${p.models?.length ? 'ck-prov-models' : undefined} />
      ${p.models?.length ? html`<datalist id="ck-prov-models">${p.models.map((m, i) => html`<option key=${i} value=${m} />`)}</datalist>` : ''}
    </${Field}>
    <${Field} label=${`API base${p.default_api_base ? '' : ' (optional)'}`}><${Input} value=${apiBase} onInput=${setApiBase} placeholder=${p.default_api_base ? `Default: ${p.default_api_base}` : 'Provider default'} mono /></${Field}>
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
    default: return null;
  }
}
