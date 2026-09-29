// World packs (Phase 39): share a slice of a world as one zip — pages, media,
// templates, kind schemas, Atlas maps — and import one with a reviewed plan
// and a one-click rollback. Modals are hosted by modals.js.
import { html, useState, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { store, closeModal, apiJson, apiFetch } from './core.js';
import { Btn, Field, Input, Textarea, Spinner } from './ui.js';
import { ModalShell } from './modals.js';
import { revealPath, loadVaultTree, loadVaultLinks, loadAtlasMaps, loadVaultTags } from './actions.js';

const cid = () => store.campaign?.campaign_id;

async function pickPackFile() {
  const dialog = window.__TAURI__?.dialog;
  if (!dialog?.open) return null;
  const picked = await dialog.open({
    multiple: false, title: 'Choose a world pack',
    filters: [{ name: 'World pack', extensions: ['zip'] }],
  });
  return typeof picked === 'string' ? picked : null;
}

async function refreshWorld() {
  const id = cid();
  await Promise.all([loadVaultTree(id), loadVaultLinks(id), loadAtlasMaps(id), loadVaultTags(id)]);
}

const errBox = (err) => err && html`<div style=${{ fontSize: 12.5, color: 'var(--burgundy)' }}>${err}</div>`;
const hint = { fontSize: 12.5, color: 'var(--ink-muted)', lineHeight: 1.5 };

function Check({ checked, onChange, children, disabled }) {
  return html`<label style=${{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 13, cursor: disabled ? 'default' : 'pointer' }}>
    <input type="checkbox" checked=${checked} disabled=${disabled} onChange=${onChange} style=${{ width: 14, height: 14, cursor: 'inherit' }} />
    ${children}
  </label>`;
}

// ── Export ────────────────────────────────────────────────────────
export function ExportPackModal() {
  const topFolders = (store.vaultFolders || []).map((f) => f.path).filter((p) => p && !p.includes('/')).sort();
  const [name, setName] = useState(store.campaign?.name || '');
  const [description, setDescription] = useState('');
  const [picked, setPicked] = useState(() => new Set(topFolders));
  const [atlas, setAtlas] = useState(true);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState(null);
  const [err, setErr] = useState(null);

  const toggle = (f) => setPicked((s) => { const n = new Set(s); if (n.has(f)) n.delete(f); else n.add(f); return n; });
  const all = topFolders.every((f) => picked.has(f));

  async function run() {
    setBusy(true); setErr(null);
    try {
      const body = { name: name.trim(), description: description.trim(), include_atlas: atlas };
      if (topFolders.length && !all) body.folders = [...picked];
      setResult(await apiJson(`/campaigns/${cid()}/packs/export`, 'POST', body));
    } catch (e) { setErr(e.message || 'Export failed'); }
    finally { setBusy(false); }
  }

  return html`<${ModalShell} title="Export world pack" footer=${result
    ? html`<${Btn} kind="ghost" icon="folder" onClick=${() => revealPath(result.path)}>Reveal in file manager</${Btn}><${Btn} kind="primary" onClick=${closeModal}>Done</${Btn}>`
    : html`<${Btn} kind="primary" icon="download" disabled=${busy || !name.trim() || (topFolders.length > 0 && picked.size === 0 && !atlas)} onClick=${run}>${busy ? html`<${Spinner} size=${14} /> Packing…` : 'Export pack'}</${Btn}>`}>
    <div style=${hint}>
      A pack holds pages, the media they use, templates, kind schemas and Atlas maps — nothing from sessions,
      transcripts, players or settings. Share the zip; others import it with a review step.
    </div>
    ${!result && html`
      <${Field} label="Pack name"><${Input} value=${name} onInput=${setName} /></${Field}>
      <${Field} label="Description" hint="Shown to whoever imports it."><${Textarea} value=${description} onInput=${setDescription} rows=${2} /></${Field}>
      ${topFolders.length > 0 && html`<${Field} label="Codex folders">
        <${Check} checked=${all} onChange=${() => setPicked(new Set(all ? [] : topFolders))}><b>All folders</b></${Check}>
        <div style=${{ display: 'flex', flexDirection: 'column', gap: 5, paddingLeft: 4, maxHeight: 180, overflow: 'auto' }}>
          ${topFolders.map((f) => html`<${Check} key=${f} checked=${picked.has(f)} onChange=${() => toggle(f)}>${f}</${Check}>`)}
        </div>
      </${Field}>`}
      <${Check} checked=${atlas} onChange=${() => setAtlas(!atlas)}>Include Atlas maps <span style=${{ fontSize: 11.5, color: 'var(--ink-faint)' }}>(pins keep their page links)</span></${Check}>`}
    ${result && html`<div>
      <div style=${{ fontSize: 13, marginBottom: 4 }}>Packed ${result.files} files as “${result.name}”.</div>
      <div style=${{ fontSize: 12, fontFamily: 'var(--font-mono)', color: 'var(--ink-soft)', wordBreak: 'break-all' }}>${result.path}</div>
    </div>`}
    ${errBox(err)}
  </${ModalShell}>`;
}

// ── Import ────────────────────────────────────────────────────────
const GROUPS = [
  ['added', 'New', 'Will be added.'],
  ['updated', 'Updated by the pack', 'You haven’t edited these; they take the pack’s newer version.'],
  ['removed', 'Dropped by the pack', 'You haven’t edited these; the pack no longer ships them.'],
  ['conflict', 'Conflicts', 'You and the pack both changed these (or they existed already). Your copy stays unless you tick it.'],
  ['restored', 'Deleted by you', 'Installed earlier, deleted since. Tick to bring them back.'],
  ['kept', 'Edited by you', 'The pack didn’t change them; nothing to do.'],
  ['unchanged', 'Already identical', ''],
];
const short = (t) => t.replace(/^Codex\//, '');

function Plan({ plan, sel, setSel }) {
  const by = {};
  for (const it of plan.items) (by[it.status] ||= []).push(it);
  const actionable = (it) => it.status !== 'unchanged' && it.status !== 'kept';
  return html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 12 }}>
    ${GROUPS.filter(([s]) => by[s]).map(([s, title, blurb]) => html`<div key=${s}>
      <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: s === 'conflict' ? 'var(--burgundy)' : 'var(--ink-faint)', marginBottom: 2 }}>
        ${title} <span style=${{ fontFamily: 'var(--font-mono)' }}>${by[s].length}</span>
      </div>
      ${blurb && html`<div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginBottom: 5 }}>${blurb}</div>`}
      <div style=${{ display: 'flex', flexDirection: 'column', gap: 3, maxHeight: s === 'unchanged' ? 90 : 170, overflow: 'auto' }}>
        ${by[s].map((it) => actionable(it)
          ? html`<${Check} key=${it.target} checked=${!!sel[it.target]} onChange=${() => setSel({ ...sel, [it.target]: !sel[it.target] })}>
              <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11.5, wordBreak: 'break-all' }}>${short(it.target)}</span>
            </${Check}>`
          : html`<div key=${it.target} style=${{ fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--ink-faint)', paddingLeft: 22, wordBreak: 'break-all' }}>${short(it.target)}</div>`)}
      </div>
    </div>`)}
  </div>`;
}

export function ImportPackModal() {
  const [file, setFile] = useState('');
  const [dest, setDest] = useState('');
  const [plan, setPlan] = useState(null);
  const [sel, setSel] = useState({});
  const [installed, setInstalled] = useState([]);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(null);
  const [err, setErr] = useState(null);

  const loadInstalled = () => apiFetch(`/campaigns/${cid()}/packs`).then((r) => setInstalled(r.packs || [])).catch(() => {});
  useEffect(() => { loadInstalled(); }, []);

  const choose = async () => { const p = await pickPackFile(); if (p) setFile(p); };

  async function review(nextDest) {
    setBusy(true); setErr(null);
    try {
      let d = nextDest;
      let p = await apiJson(`/campaigns/${cid()}/packs/plan`, 'POST', { pack_path: file.trim(), dest: d ?? dest });
      if (d == null && !dest.trim()) {
        d = `Imported/${p.pack.name.replace(/[\\/:.]+/g, ' ').trim()}`;
        p = await apiJson(`/campaigns/${cid()}/packs/plan`, 'POST', { pack_path: file.trim(), dest: d });
        setDest(d);
      }
      setPlan(p);
      setSel(Object.fromEntries(p.items.map((i) => [i.target, i.default_apply])));
    } catch (e) { setErr(e.message || 'Could not read the pack'); setPlan(null); }
    finally { setBusy(false); }
  }

  async function apply() {
    setBusy(true); setErr(null);
    try {
      const overrides = Object.fromEntries(plan.items
        .filter((i) => i.status !== 'unchanged' && i.status !== 'kept' && !!sel[i.target] !== i.default_apply)
        .map((i) => [i.target, !!sel[i.target]]));
      const r = await apiJson(`/campaigns/${cid()}/packs/apply`, 'POST', { pack_path: file.trim(), dest: plan.dest, overrides });
      await refreshWorld();
      setDone({ ...r, name: plan.pack.name });
      loadInstalled();
    } catch (e) { setErr(e.message || 'Import failed'); }
    finally { setBusy(false); }
  }

  async function rollback(packId) {
    setBusy(true); setErr(null);
    try {
      const r = await apiJson(`/campaigns/${cid()}/packs/${encodeURIComponent(packId)}/rollback`, 'POST', {});
      await refreshWorld();
      setDone({ rolledBack: r, pack_id: packId });
      setPlan(null);
      loadInstalled();
    } catch (e) { setErr(e.message || 'Rollback failed'); }
    finally { setBusy(false); }
  }

  const n = plan ? plan.items.filter((i) => i.status !== 'unchanged' && i.status !== 'kept' && sel[i.target]).length : 0;
  const lastImport = done && !done.rolledBack ? installed.find((p) => p.pack_id === done.pack_id && p.can_rollback) : null;

  const footer = done
    ? html`${lastImport && html`<${Btn} kind="ghost" icon="undo" disabled=${busy} onClick=${() => rollback(lastImport.pack_id)}>Roll back this import</${Btn}>`}<${Btn} kind="primary" onClick=${closeModal}>Done</${Btn}>`
    : plan
      ? html`<${Btn} kind="ghost" onClick=${() => { setPlan(null); setErr(null); }}>Back</${Btn}>
          <${Btn} kind="primary" icon="download" disabled=${busy || n === 0} onClick=${apply}>${busy ? html`<${Spinner} size=${14} /> Importing…` : `Import ${n} file${n === 1 ? '' : 's'}`}</${Btn}>`
      : html`<${Btn} kind="primary" disabled=${busy || !file.trim()} onClick=${() => review()}>${busy ? html`<${Spinner} size=${14} /> Reading…` : 'Review'}</${Btn}>`;

  return html`<${ModalShell} wide title="Import world pack" footer=${footer}>
    ${done && !done.rolledBack && html`<div style=${{ fontSize: 13.5 }}>
      Imported “${done.name}”: ${done.applied} file${done.applied === 1 ? '' : 's'} written, ${done.skipped} left as they were.
      Every replaced file was saved first, so this can be rolled back.
    </div>`}
    ${done?.rolledBack && html`<div style=${{ fontSize: 13.5 }}>
      Rolled back: ${done.rolledBack.restored} file${done.rolledBack.restored === 1 ? '' : 's'} restored.
      ${done.rolledBack.skipped_modified.length > 0 && html`<div style=${{ marginTop: 6, fontSize: 12.5, color: 'var(--ink-muted)' }}>
        Left alone because you edited them after the import: ${done.rolledBack.skipped_modified.map(short).join(', ')}
      </div>`}
    </div>`}
    ${!done && !plan && html`
      <div style=${hint}>Pick a pack file. Nothing is written until you have reviewed what would change.</div>
      <${Field} label="Pack file">
        <div style=${{ display: 'flex', gap: 8 }}>
          <${Input} value=${file} onInput=${setFile} mono placeholder="/path/to/pack.zip" style=${{ flex: 1 }} />
          ${window.__TAURI__?.dialog && html`<${Btn} icon="upload" onClick=${choose}>Choose…</${Btn}>`}
        </div>
      </${Field}>
      ${installed.length > 0 && html`<div>
        <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 6 }}>Installed packs</div>
        ${installed.map((p) => html`<div key=${p.pack_id} style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '5px 0', fontSize: 13 }}>
          <span style=${{ flex: 1 }}>${p.name} <span style=${{ color: 'var(--ink-faint)', fontSize: 11.5 }}>→ ${p.dest || 'Codex root'}</span></span>
          ${p.can_rollback && html`<${Btn} size="sm" kind="ghost" icon="undo" disabled=${busy} onClick=${() => rollback(p.pack_id)}>Roll back last import</${Btn}>`}
        </div>`)}
      </div>`}`}
    ${plan && !done && html`
      <div>
        <div style=${{ fontFamily: 'var(--font-display)', fontSize: 16 }}>${plan.pack.name}</div>
        ${plan.pack.description && html`<div style=${{ ...hint, marginTop: 2 }}>${plan.pack.description}</div>`}
        <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 4 }}>
          ${plan.pack.files} files${plan.installed_at ? ' · imported here before — this is an update' : ''}
        </div>
      </div>
      <${Field} label="Import pages into folder" hint="Codex pages and pin links are placed under this folder. Leave empty for the Codex root.">
        <div style=${{ display: 'flex', gap: 8 }}>
          <${Input} value=${dest} onInput=${setDest} style=${{ flex: 1 }} />
          <${Btn} disabled=${busy} onClick=${() => review(dest)}>Update plan</${Btn}>
        </div>
      </${Field}>
      <${Plan} plan=${plan} sel=${sel} setSel=${setSel} />`}
    ${errBox(err)}
  </${ModalShell}>`;
}
