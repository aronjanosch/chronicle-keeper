// Sessions — the recording pipeline that feeds the world. Split out of the
// Overview per the worldbuilding IA. A lean list; each row links to its session.
import { html, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, fmtDate, openModal } from '../core.js';
import { loadSession, refreshCampaignSessions, deleteSession } from '../actions.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Btn, StagePill, Empty, openContextMenu } from '../ui.js';

function sessionRowMenu(s, onOpen) {
  return (e) => openContextMenu(e, [
    { label: 'Open', icon: 'book', onClick: onOpen },
    { label: 'Export…', icon: 'export', disabled: !s.has_summary,
      onClick: async () => { await loadSession(s.session_id); openModal('export', {}); } },
    '-',
    { label: 'Delete session', icon: 'trash', danger: true, onClick: () => openModal('confirm', {
      title: 'Delete session',
      message: html`Delete ${html`<strong>session ${String(s.session_number || 0).padStart(2, '0')}</strong>`}${s.title ? ` (“${s.title}”)` : ''} with its audio, transcript, and summary? This cannot be undone.`,
      confirmLabel: 'Delete session',
      onConfirm: () => deleteSession(s.session_id),
    }) },
  ]);
}

function SessionRow({ s, onClick }) {
  // `complete` marks a finished stage (green); `current` marks the single next
  // incomplete stage (burgundy) — mutually exclusive, StagePill renders current over complete.
  const up = !!s.has_tracks, t = !!s.has_transcription, sm = !!s.has_summary;
  const stages = [
    { stage: 'upload', complete: up, current: !up },
    { stage: 'transcribe', complete: t, current: up && !t },
    { stage: 'summarize', complete: sm, current: t && !sm },
  ];
  return html`<div onClick=${onClick} onContextMenu=${sessionRowMenu(s, onClick)}
    style=${{ display: 'flex', alignItems: 'center', gap: 16, padding: '14px 18px', borderBottom: '1px solid var(--rule-soft)', cursor: 'pointer' }}
    onMouseEnter=${(e) => { e.currentTarget.style.background = 'var(--paper)'; }}
    onMouseLeave=${(e) => { e.currentTarget.style.background = 'transparent'; }}>
    <div style=${{ width: 38, textAlign: 'center', fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--ink-faint)' }}>
      <div style=${{ fontSize: 16, color: 'var(--ink)', fontWeight: 500 }}>${String(s.session_number || 0).padStart(2, '0')}</div>
      <div style=${{ marginTop: -2 }}>session</div>
    </div>
    <div style=${{ width: 1, height: 36, background: 'var(--rule-soft)' }} />
    <div style=${{ flex: 1, minWidth: 0 }}>
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
        ${s.title || html`<span style=${{ fontStyle: 'italic', color: 'var(--ink-muted)' }}>Untitled session</span>`}
      </div>
      <div style=${{ display: 'flex', alignItems: 'center', gap: 12, fontSize: 12, color: 'var(--ink-muted)', marginTop: 2 }}>
        <span style=${{ display: 'flex', alignItems: 'center', gap: 4 }}><${Icon} name="cal" size=${11} /> ${fmtDate(s.date) || 'no date'}</span>
      </div>
    </div>
    <div style=${{ display: 'flex', gap: 5 }}>${stages.map((st, i) => html`<${StagePill} key=${i} ...${st} />`)}</div>
    <${Icon} name="chev-r" size=${14} className="ck-ink-muted" />
  </div>`;
}

export function SessionsScreen({ store }) {
  const c = store.campaign;
  // Stale-while-revalidate: sessions created elsewhere (draft saved on the
  // New-Session screen) aren't in the store yet.
  useEffect(() => { refreshCampaignSessions(); }, [c?.campaign_id]);
  if (!c) return html`<div />`;
  const sessions = store.campaignSessions || [];

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="campaign" active="sessions" campaign=${c} />`}
    topbar=${html`<${Topbar} crumbs=${[{ label: 'Worlds', onClick: () => navigate('library') }, c.name, 'Sessions']} right=${html`
      <${Btn} kind="primary" icon="mic" onClick=${() => navigate('newSession', { id: c.campaign_id })}>New session</${Btn}>`} />`}
  >
    <div style=${{ marginBottom: 20 }}>
      <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>The play that feeds the world</div>
      <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 28, fontWeight: 500, letterSpacing: '-0.02em', lineHeight: 1.1, color: 'var(--ink)', marginTop: 3 }}>Sessions</h1>
    </div>

    <div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden' }}>
      <div style=${{ padding: '14px 18px 10px', display: 'flex', alignItems: 'baseline', gap: 10, borderBottom: '1px solid var(--rule-soft)' }}>
        <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, color: 'var(--ink)' }}>Sessions</h3>
        <span style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>· ${sessions.length} recorded</span>
      </div>
      ${sessions.length
        ? sessions.map((s) => html`<${SessionRow} key=${s.session_id} s=${s} onClick=${() => loadSession(s.session_id)} />`)
        : html`<${Empty} icon="scroll" title="No sessions yet">Upload a Craig recording to begin the first chronicle entry.</${Empty}>`}
    </div>
  </${Shell}>`;
}
