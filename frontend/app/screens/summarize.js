// Screen 06 — Summarize workspace. Configure on the left, preview on the right.
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, fmtDateTime } from '../core.js';
import { loadLlmProviders, loadPromptPresets, runSummarize, openCampaign } from '../actions.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Btn, Markdown, Empty } from '../ui.js';

function PromptPreset({ active, name, desc, onClick }) {
  return html`<div onClick=${onClick} style=${{
    padding: '10px 12px', background: active ? 'var(--burgundy-50)' : 'var(--paper)',
    border: active ? '1px solid rgba(122,46,31,.22)' : '1px solid var(--rule-soft)',
    borderRadius: 6, display: 'flex', alignItems: 'flex-start', gap: 10, cursor: 'pointer',
  }}>
    <div style=${{ width: 14, height: 14, borderRadius: '50%', flex: '0 0 auto', marginTop: 2, border: `1.5px solid ${active ? 'var(--burgundy)' : 'var(--rule-strong)'}`, background: active ? 'var(--burgundy)' : 'transparent', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
      ${active && html`<div style=${{ width: 5, height: 5, borderRadius: '50%', background: '#FBF6E9' }} />`}
    </div>
    <div style=${{ flex: 1, minWidth: 0 }}>
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 13.5, fontWeight: 500, color: active ? 'var(--burgundy-700)' : 'var(--ink)' }}>${name}</div>
      ${desc && html`<div style=${{ fontSize: 12, color: 'var(--ink-muted)', marginTop: 2, lineHeight: 1.4 }}>${desc}</div>`}
    </div>
  </div>`;
}

const Label = ({ children }) => html`<div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 8 }}>${children}</div>`;

export function SummarizeScreen({ store }) {
  const sess = store.session;
  const c = store.campaign;
  const cam = sess?.campaign || {};
  const providers = store.llmProviders || [];
  const presets = store.promptPresets || {};

  const [provider, setProvider] = useState((store.config?.summary_provider || 'ollama').toLowerCase());
  const [model, setModel] = useState('');
  const [transcriptId, setTranscriptId] = useState(store.transcripts[0]?.id || null);
  const [title, setTitle] = useState(cam.title || '');
  const [context, setContext] = useState('');
  const [presetKey, setPresetKey] = useState(null);
  const [systemPrompt, setSystemPrompt] = useState('');
  const [custom, setCustom] = useState(false);

  useEffect(() => { loadLlmProviders(); loadPromptPresets(); }, []);

  // default model when provider changes
  useEffect(() => {
    const p = providers.find((x) => x.id === provider);
    if (p) setModel(p.saved_model || p.default_model || '');
  }, [provider, store.llmProviders]);

  // default preset once presets load
  useEffect(() => {
    const keys = Object.keys(presets);
    if (keys.length && presetKey == null) { setPresetKey(keys[0]); setSystemPrompt(presets[keys[0]].text || ''); }
  }, [store.promptPresets]);

  if (!sess) return html`<div />`;
  const provOptions = providers.map((p) => ({ id: p.id, label: p.needs_key ? (p.has_key ? `${p.name} ✓` : `${p.name} (no key)`) : p.name }));

  function pickPreset(k) { setCustom(false); setPresetKey(k); setSystemPrompt(presets[k]?.text || ''); }
  function generate() {
    runSummarize({ transcriptId, provider, model: model.trim() || null, title: title.trim() || null, context: context.trim() || null, systemPrompt: systemPrompt.trim() || null });
  }

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="campaign" active="sessions" campaign=${c} />`}
    topbar=${html`<${Topbar} crumbs=${[
      { label: 'Campaigns', onClick: () => navigate('library') },
      c && { label: c.name, onClick: () => openCampaign(c.campaign_id) },
      { label: `Session ${cam.session_number || '?'}`, onClick: () => navigate('session', { id: sess.session_id }) },
      'Summarize',
    ]} right=${html`
      <div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <${Btn} kind="ghost" onClick=${() => navigate('session', { id: sess.session_id })}>Cancel</${Btn}>
        <${Btn} kind="primary" icon="sparkle" disabled=${!store.transcripts.length} onClick=${generate}>Generate summary</${Btn}>
      </div>`} />`}
    bodyStyle=${{ padding: 0 }}
  >
    <div style=${{ display: 'grid', gridTemplateColumns: '420px 1fr', height: '100%' }}>
      <div style=${{ borderRight: '1px solid var(--rule-soft)', overflow: 'auto', padding: '22px 24px' }}>
        <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 4 }}>Session ${cam.session_number || ''} · summarize</div>
        <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 24, fontWeight: 500, letterSpacing: '-0.015em', color: 'var(--ink)', lineHeight: 1.15 }}>${cam.title || 'Untitled session'}</h1>
        <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', marginTop: 4, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>Configure the model and voice, then generate.</div>

        ${!store.transcripts.length && html`<div style=${{ marginTop: 16, padding: '10px 12px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.22)', borderRadius: 6, fontSize: 12.5, color: 'var(--ochre)' }}>No transcript yet — transcribe the recording first.</div>`}

        ${store.transcripts.length > 1 && html`<div style=${{ marginTop: 22 }}>
          <${Label}>Transcript</${Label}>
          <select value=${transcriptId} onChange=${(e) => setTranscriptId(Number(e.target.value))} style=${{ width: '100%', padding: '8px 10px', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 6, fontSize: 13, cursor: 'pointer' }}>
            ${store.transcripts.map((t) => html`<option key=${t.id} value=${t.id}>${t.provider} / ${t.model} · ${fmtDateTime(t.created_at)}</option>`)}
          </select>
        </div>`}

        <div style=${{ marginTop: 22 }}>
          <${Label}>Model</${Label}>
          <div style=${{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            <select value=${provider} onChange=${(e) => setProvider(e.target.value)} style=${{ width: '100%', padding: '9px 12px', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 6, fontSize: 13, cursor: 'pointer' }}>
              ${provOptions.map((p) => html`<option key=${p.id} value=${p.id}>${p.label}</option>`)}
            </select>
            <input value=${model} onInput=${(e) => setModel(e.target.value)} placeholder="model name" list="ck-models"
              style=${{ width: '100%', padding: '9px 12px', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 6, fontSize: 13, fontFamily: 'var(--font-mono)', color: 'var(--ink)' }} />
            <datalist id="ck-models">${(providers.find((p) => p.id === provider)?.models || []).map((m, i) => html`<option key=${i} value=${m} />`)}</datalist>
          </div>
        </div>

        <div style=${{ marginTop: 22 }}>
          <${Label}>Voice & style</${Label}>
          <div style=${{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            ${Object.keys(presets).map((k) => html`<${PromptPreset} key=${k} active=${!custom && presetKey === k} name=${presets[k].label || k} desc=${(presets[k].text || '').slice(0, 90)} onClick=${() => pickPreset(k)} />`)}
            <${PromptPreset} active=${custom} name="Custom prompt" desc="Write your own system instructions below." onClick=${() => setCustom(true)} />
          </div>
          ${custom && html`<textarea value=${systemPrompt} onInput=${(e) => setSystemPrompt(e.target.value)} rows=${6} placeholder="System prompt…"
            style=${{ marginTop: 8, width: '100%', padding: '8px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, lineHeight: 1.4, resize: 'vertical', fontFamily: 'inherit', color: 'var(--ink)' }}></textarea>`}
        </div>

        <div style=${{ marginTop: 22 }}>
          <${Label}>Title & extra context</${Label}>
          <div style=${{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            <input value=${title} onInput=${(e) => setTitle(e.target.value)} placeholder="Title hint (optional)" style=${{ width: '100%', padding: '8px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, color: 'var(--ink)' }} />
            <input value=${context} onInput=${(e) => setContext(e.target.value)} placeholder="Extra context for the LLM (optional)" style=${{ width: '100%', padding: '8px 10px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 4, fontSize: 13, color: 'var(--ink)' }} />
          </div>
        </div>

        <!-- Codex context — entries are fed into the prompt automatically (see prompts.rs) -->
        <div style=${{ marginTop: 22 }}>
          <${Label}>Codex context</${Label}>
          <div style=${{ padding: '12px 14px', background: 'var(--paper-deep)', border: '1px solid var(--rule-soft)', borderRadius: 6, fontSize: 12.5, color: 'var(--ink-muted)', display: 'flex', alignItems: 'center', gap: 10 }}>
            <${Icon} name="book" size=${14} />
            <span>${(store.codexEntries?.length || 0) > 0
              ? `${store.codexEntries.length} codex ${store.codexEntries.length === 1 ? 'entry is' : 'entries are'} fed to the summarizer automatically.`
              : 'Codex entries for this campaign are fed to the summarizer automatically. None yet.'}</span>
          </div>
        </div>
      </div>

      <div style=${{ overflow: 'auto', padding: '24px 36px', background: 'var(--paper)' }}>
        <div style=${{ maxWidth: 720, margin: '0 auto' }}>
          <div style=${{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 16 }}>
            <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>Preview · last run</div>
            <span style=${{ flex: 1 }} />
            ${store.summaries[0] && html`<span style=${{ fontSize: 11, color: 'var(--ink-muted)', fontFamily: 'var(--font-mono)' }}>${store.summaries[0].provider} · ${fmtDateTime(store.summaries[0].created_at)}</span>`}
          </div>
          <div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, padding: '32px 44px', boxShadow: '0 1px 0 rgba(120,90,40,.06), 0 2px 8px rgba(60,40,10,.05)' }}>
            ${store.summaryPreview
              ? html`<${Markdown} text=${store.summaryPreview.text} />`
              : html`<${Empty} icon="feather" title="No summary yet">Pick a model and voice, then generate. Your latest summary will preview here.</${Empty}>`}
          </div>
        </div>
      </div>
    </div>
  </${Shell}>`;
}
