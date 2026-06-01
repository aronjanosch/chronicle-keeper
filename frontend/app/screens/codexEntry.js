// Codex entry detail — the inspector. The honest answer to "why did the summary
// call Tannerheim friendly?": click through and see exactly what the LLM was told.
// Source bar + title + one-liner + distilled `detail` prose, inline edit/delete,
// and a "Mentioned in" list built from session metadata we already persist.
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, useStore, openModal, fmtDateTime } from '../core.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Btn, Empty, Icon, Markdown } from '../ui.js';
import { loadCodexEntries, updateCodexEntry, deleteCodexEntry, openCampaign, mentionsOf } from '../actions.js';
import { EntryForm, SourceBadge, iconForKind, KINDS } from './codex.js';

function kindLabel(k) {
  return (KINDS.find((x) => x.value === k) || {}).label || k;
}

function MentionRow({ m }) {
  return html`<div onClick=${() => navigate('session', { id: m.session_id })} style=${{
    padding: '10px 0', borderBottom: '1px solid var(--rule-soft)',
    display: 'flex', alignItems: 'center', gap: 12, cursor: 'pointer',
  }}
    onMouseEnter=${(e) => { e.currentTarget.style.opacity = 0.7; }}
    onMouseLeave=${(e) => { e.currentTarget.style.opacity = 1; }}>
    <div style=${{ width: 44, flex: '0 0 auto', fontFamily: 'var(--font-mono)', fontSize: 13, color: 'var(--burgundy)', fontWeight: 600 }}>
      ${m.session_number != null ? `S${String(m.session_number).padStart(2, '0')}` : '—'}
    </div>
    <div style=${{ flex: 1, fontFamily: 'var(--font-display)', fontSize: 13.5, fontStyle: 'italic', color: 'var(--ink)' }}>
      ${m.title || 'Untitled session'}
    </div>
    <${Icon} name="chev-r" size=${12} className="ck-ink-faint" />
  </div>`;
}

export function CodexEntryScreen() {
  const store = useStore();
  const c = store.campaign;
  const entryId = store.route.params.entryId;
  const [editing, setEditing] = useState(false);

  // On a cold load (reload / deep link) entries may not be in the store yet.
  useEffect(() => {
    if (c?.campaign_id && !(store.codexEntries || []).length) loadCodexEntries(c.campaign_id);
  }, [c?.campaign_id]);

  if (!c) { navigate('library'); return null; }

  const entry = (store.codexEntries || []).find((e) => e.entry_id === entryId);
  const sidebar = html`<${Sidebar} variant="campaign" active="codex" campaign=${c} />`;

  if (!entry) {
    const topbar = html`<${Topbar} crumbs=${[
      { label: 'Campaigns', onClick: () => navigate('library') },
      { label: c.name, onClick: () => openCampaign(c.campaign_id) },
      { label: 'Codex', onClick: () => navigate('codex', { id: c.campaign_id }) },
    ]} />`;
    return html`<${Shell} sidebar=${sidebar} topbar=${topbar}>
      <${Empty} icon="book" title="Entry not found">
        It may have been deleted. <a onClick=${() => navigate('codex', { id: c.campaign_id })} style=${{ color: 'var(--burgundy)', cursor: 'pointer' }}>Back to the codex</a>.
      </${Empty}>
    </${Shell}>`;
  }

  const mentions = mentionsOf(entry.name);

  const topbar = html`<${Topbar} crumbs=${[
    { label: 'Campaigns', onClick: () => navigate('library') },
    { label: c.name, onClick: () => openCampaign(c.campaign_id) },
    { label: 'Codex', onClick: () => navigate('codex', { id: c.campaign_id }) },
    entry.name,
  ]}
    right=${html`<div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
      ${!editing && html`<${Btn} kind="ghost" size="sm" icon="edit" onClick=${() => setEditing(true)}>Edit</${Btn}>`}
      ${!editing && html`<${Btn} kind="ghost" size="sm" icon="trash" onClick=${() => openModal('confirm', {
        title: 'Delete codex entry',
        message: `Delete "${entry.name}" from the codex? This cannot be undone.`,
        onConfirm: async () => { await deleteCodexEntry(entry.entry_id); navigate('codex', { id: c.campaign_id }); },
      })}>Delete</${Btn}>`}
    </div>`} />`;

  async function onSave(payload) {
    await updateCodexEntry(entry.entry_id, payload);
    setEditing(false);
  }

  return html`<${Shell} sidebar=${sidebar} topbar=${topbar} bodyStyle=${{ padding: 0 }}>
    <div style=${{ overflow: 'auto', padding: '32px 48px', background: 'var(--paper)', height: '100%' }}>
      <div style=${{ maxWidth: 680, margin: '0 auto' }}>
        ${editing
          ? html`<${EntryForm} initial=${entry} onSubmit=${onSave} onCancel=${() => setEditing(false)} withDetail=${true} />`
          : html`
            <div style=${{
              display: 'flex', alignItems: 'center', gap: 10, padding: '10px 14px',
              background: 'var(--surface)', border: '1px solid var(--rule-soft)',
              borderRadius: 6, marginBottom: 22, fontSize: 12, color: 'var(--ink-muted)',
            }}>
              <${SourceBadge} source=${entry.source} />
              <span style=${{ flex: 1 }} />
              ${entry.updated_at && html`<span style=${{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)' }}>updated ${fmtDateTime(entry.updated_at)}</span>`}
            </div>

            <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--burgundy)', display: 'flex', alignItems: 'center', gap: 6 }}>
              <${Icon} name=${iconForKind(entry.kind)} size=${12} /> ${kindLabel(entry.kind)}
            </div>
            <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 36, fontWeight: 500, letterSpacing: '-0.02em', lineHeight: 1.1, color: 'var(--ink)', marginTop: 6, marginBottom: 0 }}>
              ${entry.name}
            </h1>
            ${entry.body && html`<div style=${{ fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 16, color: 'var(--ink-muted)', marginTop: 8 }}>
              ${entry.body}
            </div>`}

            <div style=${{ height: 1, background: 'var(--rule)', margin: '28px 0' }} />

            ${entry.detail && entry.detail.trim()
              ? html`<${Markdown} text=${entry.detail} />`
              : html`<div style=${{ fontSize: 13, color: 'var(--ink-faint)', fontStyle: 'italic', fontFamily: 'var(--font-display)' }}>
                  No detail yet. The one-liner above is all the summarizer is told — add a fuller write-up with Edit.
                </div>`}

            <div style=${{
              marginTop: 28, padding: '14px 16px', background: 'var(--surface)',
              border: '1px solid var(--rule-soft)', borderRadius: 6, fontSize: 13,
              color: 'var(--ink-muted)', lineHeight: 1.55, display: 'flex', alignItems: 'flex-start', gap: 10,
            }}>
              <${Icon} name="feather" size=${13} className="ck-ink-muted" style=${{ marginTop: 3 }} />
              <div>
                <b style=${{ color: 'var(--ink-soft)', fontWeight: 600 }}>Only the one-liner is fed to the LLM.</b>
                ${' '}This detail is for you — the inspector into what the chronicle remembers.
              </div>
            </div>

            <div style=${{ height: 1, background: 'var(--rule)', margin: '32px 0 20px' }} />
            <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 8 }}>
              ${mentions.length ? `Mentioned in ${mentions.length} ${mentions.length === 1 ? 'session' : 'sessions'}` : 'Mentioned in'}
            </div>
            ${mentions.length
              ? mentions.map((m) => html`<${MentionRow} key=${m.session_id} m=${m} />`)
              : html`<div style=${{ fontSize: 12.5, color: 'var(--ink-faint)', fontStyle: 'italic', fontFamily: 'var(--font-display)' }}>
                  Not yet tagged in any session's "What happened". Add the name to a session's NPCs/Places/Items and it appears here.
                </div>`}
          `}
      </div>
    </div>
  </${Shell}>`;
}
