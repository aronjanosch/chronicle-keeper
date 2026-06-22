// Screen 08 — Settings. Calm single page, grouped into cards. Real config.
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { store, setOp, openModal } from '../core.js';
import { loadConfig, saveConfig, loadLlmProviders, loadPromptTemplates, deletePromptTemplate, restorePromptDefaults, pingLlmProvider, loadSkillsPath, revealPath, loadFoundrySettings, saveFoundrySettings, testFoundry, syncFoundry } from '../actions.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Btn } from '../ui.js';

function Row({ label, hint, children }) {
  return html`<div style=${{ display: 'grid', gridTemplateColumns: '220px 1fr', gap: 24, padding: '14px 0', borderBottom: '1px solid var(--rule-soft)' }}>
    <div>
      <div style=${{ fontSize: 13, fontWeight: 500, color: 'var(--ink)' }}>${label}</div>
      ${hint && html`<div style=${{ fontSize: 11.5, color: 'var(--ink-muted)', marginTop: 3, lineHeight: 1.4 }}>${hint}</div>`}
    </div>
    <div>${children}</div>
  </div>`;
}
function SettingsCard({ icon, title, desc, children }) {
  return html`<div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden' }}>
    <div style=${{ padding: '16px 20px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'flex-start', gap: 12 }}>
      <div style=${{ width: 32, height: 32, borderRadius: 6, flex: '0 0 auto', background: 'var(--paper-deep)', color: 'var(--ink-soft)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}><${Icon} name=${icon} size=${14} /></div>
      <div>
        <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, color: 'var(--ink)' }}>${title}</h3>
        <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', marginTop: 2 }}>${desc}</div>
      </div>
    </div>
    <div style=${{ padding: '4px 20px 18px' }}>${children}</div>
  </div>`;
}
const inp = (extra = {}) => ({ width: '100%', padding: '7px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, color: 'var(--ink)', ...extra });

function ProviderCard({ p, live }) {
  const status = live === false ? 'down' : !p.needs_key ? 'ok' : (p.has_key ? 'ok' : 'missing');
  const model = p.saved_model || p.default_model;
  const badge = { ollama: { ch: '◉', bg: '#1F1813' }, 'ollama-cloud': { ch: '☁', bg: '#0B6E99' }, anthropic: { ch: 'A', bg: '#C96442' }, openai: { ch: 'O', bg: '#0F8C66' }, groq: { ch: 'G', bg: '#F55036' }, mistral: { ch: 'M', bg: '#FF7000' } }[p.id] || { ch: p.name[0], bg: 'var(--ink-muted)' };
  const isDefault = (store.config?.summary_provider || 'ollama').toLowerCase() === p.id;
  return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 12, padding: '11px 14px', background: isDefault ? 'var(--paper)' : 'var(--surface)', border: isDefault ? '1px solid var(--burgundy-300)' : '1px solid var(--rule-soft)', borderRadius: 6 }}>
    <div style=${{ width: 28, height: 28, borderRadius: 5, background: badge.bg, color: '#FBF6E9', display: 'flex', alignItems: 'center', justifyContent: 'center', fontFamily: 'var(--font-mono)', fontWeight: 700, fontSize: 13 }}>${badge.ch}</div>
    <div style=${{ flex: 1, minWidth: 0 }}>
      <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style=${{ fontSize: 13.5, fontWeight: 500, color: 'var(--ink)' }}>${p.name}</span>
        ${isDefault && html`<span style=${{ padding: '2px 6px', borderRadius: 999, background: 'var(--burgundy-50)', color: 'var(--burgundy-700)', fontSize: 10, fontWeight: 600, letterSpacing: '0.05em', textTransform: 'uppercase', border: '1px solid rgba(122,46,31,.2)' }}>Default</span>`}
      </div>
      <div style=${{ fontSize: 11.5, color: 'var(--ink-muted)', fontFamily: 'var(--font-mono)', marginTop: 1 }}>${model}</div>
    </div>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 5, fontSize: 11.5, color: status === 'ok' ? 'var(--moss)' : status === 'down' ? 'var(--burgundy-700)' : 'var(--ink-faint)', fontWeight: 500 }}>
      <span style=${{ width: 6, height: 6, borderRadius: '50%', background: status === 'ok' ? 'var(--moss)' : status === 'down' ? 'var(--burgundy-700)' : 'var(--ink-ghost)' }} />
      ${status === 'down' ? 'Unreachable' : status === 'ok' ? (live ? 'Online' : p.needs_key ? 'Key saved' : 'Local') : 'No key'}
    </div>
    <${Btn} kind="ghost" size="sm" onClick=${() => openModal('provider', { id: p.id })}>Manage ›</${Btn}>
  </div>`;
}

function TemplateRow({ t, onDelete }) {
  return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 12, padding: '11px 14px', background: 'var(--surface)', border: '1px solid var(--rule-soft)', borderRadius: 6 }}>
    <div style=${{ width: 28, height: 28, borderRadius: 5, flex: '0 0 auto', background: 'var(--paper-deep)', color: 'var(--ink-soft)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}><${Icon} name="feather" size=${13} /></div>
    <div style=${{ flex: 1, minWidth: 0 }}>
      <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style=${{ fontSize: 13.5, fontWeight: 500, color: 'var(--ink)' }}>${t.label}</span>
        ${t.builtin && html`<span style=${{ padding: '2px 6px', borderRadius: 999, background: 'var(--paper-deep)', color: 'var(--ink-muted)', fontSize: 10, fontWeight: 600, letterSpacing: '0.05em', textTransform: 'uppercase', border: '1px solid var(--rule-soft)' }}>Built-in</span>`}
      </div>
      <div style=${{ fontSize: 11.5, color: 'var(--ink-muted)', marginTop: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>${(t.text || '').slice(0, 80)}</div>
    </div>
    <${Btn} kind="ghost" size="sm" onClick=${() => openModal('promptTemplate', { edit: t })}>Edit ›</${Btn}>
    <${Btn} kind="ghost" size="sm" icon="trash" onClick=${onDelete} />
  </div>`;
}

function TemplatesCard() {
  const templates = store.promptTemplates || [];
  function del(t) {
    openModal('confirm', {
      title: 'Delete template',
      message: html`Delete the prompt template ${html`<strong>${t.label}</strong>`}? ${t.builtin ? 'You can bring built-in templates back with “Restore defaults”.' : 'This cannot be undone.'}`,
      confirmLabel: 'Delete',
      onConfirm: () => deletePromptTemplate(t.id),
    });
  }
  async function restore() {
    try { await restorePromptDefaults(); setOp('Default templates restored', 'done'); }
    catch (e) { setOp(e.message, 'err'); }
  }
  return html`<${SettingsCard} icon="feather" title="Summary templates" desc="The prompt presets offered on the Summarize screen. Edit, add your own, or delete the built-ins.">
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 8, paddingTop: 12 }}>
      ${templates.length
        ? templates.map((t) => html`<${TemplateRow} key=${t.id} t=${t} onDelete=${() => del(t)} />`)
        : html`<div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', padding: '8px 0' }}>No templates yet — add one, or restore the built-in defaults.</div>`}
    </div>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 12 }}>
      <${Btn} kind="secondary" size="sm" icon="plus" onClick=${() => openModal('promptTemplate', {})}>New template</${Btn}>
      <${Btn} kind="ghost" size="sm" onClick=${restore}>Restore defaults</${Btn}>
    </div>
  </${SettingsCard}>`;
}

function FoundryCard() {
  const [f, setF] = useState(null);
  const [busy, setBusy] = useState('');
  const set = (k, v) => setF((s) => ({ ...s, [k]: v }));

  useEffect(() => {
    loadFoundrySettings()
      .then((s) => setF({ server_url: s.server_url || '', user_id: s.user_id || '', password: '', password_set: !!s.password_set }))
      .catch((e) => setOp(`Can't load Foundry settings: ${e.message}`, 'err'));
  }, []);

  if (!f) return html`<${SettingsCard} icon="link" title="Foundry VTT bridge" desc="Project codex pages into a live FoundryVTT world as Journal entries."><div style=${{ padding: '12px 0', fontSize: 12.5, color: 'var(--ink-muted)' }}>Loading…</div></${SettingsCard}>`;

  async function save() {
    setBusy('save');
    try {
      const payload = { server_url: f.server_url.trim(), user_id: f.user_id.trim() };
      if (f.password) payload.password = f.password; // omit to keep the stored one
      await saveFoundrySettings(payload);
      setF((s) => ({ ...s, password: '', password_set: s.password_set || !!s.password }));
      setOp('Foundry settings saved', 'done');
    } catch (e) { setOp(e.message, 'err'); } finally { setBusy(''); }
  }
  async function test() {
    setBusy('test');
    try { await testFoundry(); setOp('Foundry bridge connected', 'done'); }
    catch (e) { setOp(`Foundry: ${e.message}`, 'err'); } finally { setBusy(''); }
  }
  async function sync() {
    setBusy('sync');
    try {
      const r = await syncFoundry(store.campaign.campaign_id);
      const msg = `Synced — ${r.created} created, ${r.updated} updated, ${r.deleted} deleted${r.errors?.length ? `, ${r.errors.length} errors` : ''}`;
      setOp(msg, r.errors?.length ? 'err' : 'done');
    } catch (e) { setOp(`Sync failed: ${e.message}`, 'err'); } finally { setBusy(''); }
  }

  return html`<${SettingsCard} icon="link" title="Foundry VTT bridge" desc="Project codex pages into a live FoundryVTT world as Journal entries. One-way: Chronicle Keeper is the source of truth.">
    <${Row} label="Server URL" hint="Your Foundry world's base URL, e.g. https://foundry.example.com (no /game).">
      <input value=${f.server_url} onInput=${(e) => set('server_url', e.target.value)} placeholder="https://foundry.example.com" style=${inp({ fontFamily: 'var(--font-mono)' })} />
    </${Row}>
    <${Row} label="API user id" hint="The 16-char document _id of a dedicated Assistant-GM user (game.users.getName('name').id in Foundry's console).">
      <input value=${f.user_id} onInput=${(e) => set('user_id', e.target.value)} placeholder="K7zWvqylw1bpbI9b" style=${inp({ width: 280, fontFamily: 'var(--font-mono)' })} />
    </${Row}>
    <${Row} label="Password" hint="That user's password. Stored on this machine only; never displayed.">
      <input type="password" value=${f.password} onInput=${(e) => set('password', e.target.value)} placeholder=${f.password_set ? '•••••••• (saved)' : ''} style=${inp({ width: 280, fontFamily: 'var(--font-mono)' })} />
    </${Row}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 12 }}>
      <${Btn} kind="secondary" size="sm" disabled=${!!busy} onClick=${save}>${busy === 'save' ? 'Saving…' : 'Save'}</${Btn}>
      <${Btn} kind="ghost" size="sm" disabled=${!!busy} onClick=${test}>${busy === 'test' ? 'Testing…' : 'Test connection'}</${Btn}>
      <div style=${{ flex: 1 }} />
      ${store.campaign && html`<${Btn} kind="primary" size="sm" icon="upload" disabled=${!!busy} onClick=${sync}>${busy === 'sync' ? 'Syncing…' : `Sync “${store.campaign.name}” now`}</${Btn}>`}
    </div>
    ${!store.campaign && html`<div style=${{ fontSize: 11.5, color: 'var(--ink-muted)', marginTop: 8 }}>Open a world to sync its codex.</div>`}
  </${SettingsCard}>`;
}

export function SettingsScreen({ store }) {
  const [f, setF] = useState(null);
  const [apiBase, setApiBase] = useState(store.apiBase);
  const [saving, setSaving] = useState(false);
  const [pings, setPings] = useState({});
  const set = (k, v) => setF((s) => ({ ...s, [k]: v }));

  useEffect(() => {
    (async () => {
      let cfg;
      loadSkillsPath();
      try { cfg = await loadConfig(); await loadLlmProviders(); await loadPromptTemplates(true); }
      catch (e) { setOp(`Can't load settings: ${e.message}`, 'err'); return; }
      setF({
        output_root: cfg.output_root || '',
        summary_provider: (cfg.summary_provider || 'ollama').toLowerCase(),
        transcription_timeout_seconds: cfg.transcription_timeout_seconds || 600,
      });
      setApiBase(store.apiBase);
      // Live reachability, Ollama-family only (other transports have no keyless probe).
      for (const p of (store.llmProviders || []).filter((x) => x.id.startsWith('ollama'))) {
        pingLlmProvider(p.id)
          .then((r) => setPings((m) => ({ ...m, [p.id]: !!r.ok })))
          .catch(() => setPings((m) => ({ ...m, [p.id]: false })));
      }
    })();
  }, []);

  // keep the world context: settings opened from inside a world keeps its sidebar
  const sidebar = html`<${Sidebar} variant=${store.campaign ? 'campaign' : 'library'} campaign=${store.campaign} active="settings" />`;

  if (!f) return html`<${Shell} sidebar=${sidebar} topbar=${html`<${Topbar} crumbs=${['Workshop', 'Settings']} />`}><div /></${Shell}>`;

  async function save() {
    setSaving(true);
    try {
      const payload = {
        output_root: f.output_root.trim(),
        summary_provider: f.summary_provider || 'ollama',
        transcription_timeout_seconds: Math.max(60, parseInt(f.transcription_timeout_seconds, 10) || 600),
      };
      await saveConfig(payload, apiBase.trim());
      setOp('Settings saved', 'done');
    } catch (e) { setOp(e.message, 'err'); }
    finally { setSaving(false); }
  }

  const providers = store.llmProviders || [];

  return html`<${Shell}
    sidebar=${sidebar}
    topbar=${html`<${Topbar} crumbs=${['Workshop', 'Settings']} right=${html`<${Btn} kind="primary" disabled=${saving} onClick=${save}>${saving ? 'Saving…' : 'Save changes'}</${Btn}>`} />`}
  >
    <div style=${{ maxWidth: 920, margin: '0 auto' }}>
      <div style=${{ marginBottom: 22 }}>
        <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 28, fontWeight: 500, letterSpacing: '-0.015em', lineHeight: 1.1 }}>Settings</h1>
        <div style=${{ fontSize: 13, color: 'var(--ink-muted)', marginTop: 4, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>Configure once. Per-session overrides live in the Summarize screen.</div>
      </div>

      <div style=${{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <${SettingsCard} icon="cog" title="General" desc="App-wide defaults you rarely touch.">
          <${Row} label="Theme" hint="Light parchment is the only mode for now.">
            <div style=${{ display: 'flex', gap: 8 }}>
              <div style=${{ padding: '7px 10px', background: 'var(--surface-raised)', border: '1px solid var(--burgundy-300)', borderRadius: 4, fontSize: 12.5, display: 'flex', alignItems: 'center', gap: 6, color: 'var(--ink)' }}><${Icon} name="sun" size=${12} /> Parchment <${Icon} name="check" size=${11} className="ck-burgundy" /></div>
              <div style=${{ padding: '7px 10px', background: 'var(--paper-deep)', border: '1px solid var(--rule-soft)', borderRadius: 4, fontSize: 12.5, color: 'var(--ink-faint)' }}>Dark (soon)</div>
            </div>
          </${Row}>
          <${Row} label="Transcription stall timeout" hint="Cancel a run after this many seconds without progress. Long sessions keep going as long as they advance.">
            <input type="number" min="60" step="60" value=${f.transcription_timeout_seconds} onInput=${(e) => set('transcription_timeout_seconds', e.target.value)} style=${inp({ width: 140, fontFamily: 'var(--font-mono)' })} />
          </${Row}>
          ${!store.shellMode && html`
          <${Row} label="Backend URL" hint="Where the Chronicle Keeper core is running. Stored in this browser only.">
            <input value=${apiBase} onInput=${(e) => setApiBase(e.target.value)} placeholder="http://127.0.0.1:8000" style=${inp({ width: 340, fontFamily: 'var(--font-mono)' })} />
          </${Row}>`}
        </${SettingsCard}>

        <${SettingsCard} icon="sparkle" title="LLM providers" desc="Bring your own. Keys never leave this machine.">
          <${Row} label="Default provider" hint="Pre-fills the Summarize screen.">
            <select value=${f.summary_provider} onChange=${(e) => set('summary_provider', e.target.value)} style=${inp({ width: 240, cursor: 'pointer' })}>
              ${providers.map((p) => html`<option key=${p.id} value=${p.id}>${p.name}</option>`)}
            </select>
          </${Row}>
          <div style=${{ display: 'flex', flexDirection: 'column', gap: 8, paddingTop: 12 }}>
            ${providers.map((p) => html`<${ProviderCard} key=${p.id} p=${p} live=${pings[p.id]} />`)}
          </div>
        </${SettingsCard}>

        <${TemplatesCard} />

        <${FoundryCard} />

        <${SettingsCard} icon="folder" title="Storage" desc="Where Chronicle Keeper keeps its database, audio and model.">
          <${Row} label="Data folder" hint="Sessions, transcripts and the model live here. Absolute path.">
            <input value=${f.output_root} onInput=${(e) => set('output_root', e.target.value)} style=${inp({ fontFamily: 'var(--font-mono)' })} />
          </${Row}>
        </${SettingsCard}>

        <${SettingsCard} icon="feather" title="Keeper skills" desc="Markdown folders the Keeper pulls on demand. Add a folder to extend it; edit or delete to change the bundled ones.">
          <${Row} label="Skills folder" hint="One SKILL.md per folder. Shared across every world. Edits and deletes are kept.">
            <div style=${{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <input readonly value=${store.skillsPath || '…'} style=${inp({ fontFamily: 'var(--font-mono)', flex: 1 })} />
              <${Btn} kind="ghost" icon="folder" disabled=${!(window.__TAURI__ && store.skillsPath)} onClick=${() => revealPath(store.skillsPath)}>Open folder</${Btn}>
            </div>
          </${Row}>
        </${SettingsCard}>
      </div>
    </div>
  </${Shell}>`;
}
