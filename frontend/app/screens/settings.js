// Screen 08 — Settings. Calm single page, grouped into cards. Real config.
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { store, setOp, openModal } from '../core.js';
import { loadConfig, saveConfig, loadLlmProviders } from '../actions.js';
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

function ProviderCard({ p }) {
  const status = !p.needs_key ? 'ok' : (p.has_key ? 'ok' : 'missing');
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
    <div style=${{ display: 'flex', alignItems: 'center', gap: 5, fontSize: 11.5, color: status === 'ok' ? 'var(--moss)' : 'var(--ink-faint)', fontWeight: 500 }}>
      <span style=${{ width: 6, height: 6, borderRadius: '50%', background: status === 'ok' ? 'var(--moss)' : 'var(--ink-ghost)' }} />
      ${status === 'ok' ? (p.needs_key ? 'Key saved' : 'Local') : 'No key'}
    </div>
    <${Btn} kind="ghost" size="sm" onClick=${() => openModal('provider', { id: p.id })}>Manage ›</${Btn}>
  </div>`;
}

export function SettingsScreen({ store }) {
  const [f, setF] = useState(null);
  const [apiBase, setApiBase] = useState(store.apiBase);
  const [syncToken, setSyncToken] = useState('');
  const [saving, setSaving] = useState(false);
  const set = (k, v) => setF((s) => ({ ...s, [k]: v }));

  useEffect(() => {
    (async () => {
      let cfg;
      try { cfg = await loadConfig(); await loadLlmProviders(); }
      catch (e) { setOp(`Can't load settings: ${e.message}`, 'err'); return; }
      setF({
        output_root: cfg.output_root || '',
        summary_provider: (cfg.summary_provider || 'ollama').toLowerCase(),
        sync_url: cfg.sync_url || '',
        has_sync_token: !!cfg.has_sync_token,
      });
      setApiBase(store.apiBase);
    })();
  }, []);

  if (!f) return html`<${Shell} sidebar=${html`<${Sidebar} variant="library" active="settings" />`} topbar=${html`<${Topbar} crumbs=${['Workshop', 'Settings']} />`}><div /></${Shell}>`;

  async function save() {
    setSaving(true);
    try {
      const payload = {
        output_root: f.output_root.trim(),
        summary_provider: f.summary_provider || 'ollama',
        sync_url: f.sync_url.trim(),
      };
      if (syncToken) payload.sync_token = syncToken;
      const updated = await saveConfig(payload, apiBase.trim());
      setSyncToken('');
      set('has_sync_token', !!updated.has_sync_token);
      setOp('Settings saved', 'done');
    } catch (e) { setOp(e.message, 'err'); }
    finally { setSaving(false); }
  }

  const providers = store.llmProviders || [];

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="library" active="settings" />`}
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
            ${providers.map((p) => html`<${ProviderCard} key=${p.id} p=${p} />`)}
          </div>
        </${SettingsCard}>

        <${SettingsCard} icon="folder" title="Storage" desc="Where Chronicle Keeper keeps its database, audio and model.">
          <${Row} label="Data folder" hint="Sessions, transcripts and the model live here. Absolute path.">
            <input value=${f.output_root} onInput=${(e) => set('output_root', e.target.value)} style=${inp({ fontFamily: 'var(--font-mono)' })} />
          </${Row}>
        </${SettingsCard}>

        <${SettingsCard} icon="globe" title="Multi-device sync (optional)" desc="Keep notes in step across machines. Off by default. Transcription always stays local.">
          <${Row} label="Sync server" hint="Leave empty to stay local.">
            <input value=${f.sync_url} onInput=${(e) => set('sync_url', e.target.value)} placeholder="https://sync.example.com" style=${inp({ width: 340, fontFamily: 'var(--font-mono)' })} />
          </${Row}>
          <${Row} label="Sync token" hint="Bearer token shared with your sync server. Stored locally.">
            <input type="password" value=${syncToken} onInput=${(e) => setSyncToken(e.target.value)} placeholder=${f.has_sync_token ? '•••••••• (saved)' : '(none set)'} autocomplete="off" style=${inp({ width: 340, fontFamily: 'var(--font-mono)' })} />
          </${Row}>
          <div style=${{ marginTop: 10, fontSize: 12, color: f.sync_url ? (f.has_sync_token ? 'var(--moss)' : 'var(--ochre)') : 'var(--ink-muted)' }}>
            ${f.sync_url ? (f.has_sync_token ? `Sync on — ${f.sync_url}` : 'Sync URL set but no token saved.') : 'Sync off — everything stays on this device.'}
          </div>
        </${SettingsCard}>
      </div>
    </div>
  </${Shell}>`;
}
