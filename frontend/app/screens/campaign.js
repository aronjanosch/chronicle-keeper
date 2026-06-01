// Screen 02 — Campaign Overview. Hero + party + codex teaser + sessions list.
import { html, useState } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, openModal, fmtDate, fmtDateTime, toneFor } from '../core.js';
import { loadSession, deleteCampaign, generateRecap } from '../actions.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Sigil, Btn, StagePill, Empty, Markdown } from '../ui.js';
import { KINDS, iconForKind } from './codex.js';

// Collapsed height cap for the recap body. Tall recaps would otherwise push
// the whole page down; we clamp + fade and offer an Expand toggle.
const RECAP_COLLAPSED_MAX = 180;

function StorySoFar({ campaign, sessions, codexEntries }) {
  const [expanded, setExpanded] = useState(false);
  const recap = (campaign.recap || '').trim();
  const canBuild = sessions.some((s) => s.has_summary);
  const codexLinks = (codexEntries || []).map((e) => ({ name: e.name, entry_id: e.entry_id }));
  return html`<div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden', marginBottom: 24 }}>
    <div style=${{ padding: '14px 18px 12px', display: 'flex', alignItems: 'baseline', gap: 10, borderBottom: '1px solid var(--rule-soft)' }}>
      <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, color: 'var(--ink)' }}>The Story So Far</h3>
      <span style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>· the whole arc at a glance</span>
      <span style=${{ flex: 1 }} />
      ${recap && campaign.recap_updated_at && html`<span style=${{ fontSize: 11, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)' }}>updated ${fmtDateTime(campaign.recap_updated_at)}</span>`}
      <${Btn} kind=${recap ? 'ghost' : 'primary'} size="sm" icon="sparkle" disabled=${!canBuild}
        title=${canBuild ? '' : 'Summarize at least one session first'}
        onClick=${generateRecap}>${recap ? 'Regenerate' : 'Generate'}</${Btn}>
    </div>
    ${recap
      ? html`<div style=${{ position: 'relative' }}>
          <div style=${{
            padding: '6px 20px 16px',
            maxHeight: expanded ? 'none' : `${RECAP_COLLAPSED_MAX}px`,
            overflowY: expanded ? 'visible' : 'auto',
          }}><${Markdown} text=${recap} codex=${codexLinks} /></div>
          ${!expanded && html`<div style=${{
            position: 'absolute', left: 0, right: 0, bottom: 0, height: 56, pointerEvents: 'none',
            background: 'linear-gradient(to bottom, transparent, var(--surface))',
          }} />`}
          <div style=${{ display: 'flex', justifyContent: 'center', padding: '0 0 12px' }}>
            <${Btn} kind="ghost" size="sm" onClick=${() => setExpanded((v) => !v)}>
              ${expanded ? 'Collapse' : 'Expand'}
              <span style=${{ display: 'inline-flex', transform: expanded ? 'rotate(180deg)' : 'none', transition: 'transform .14s' }}><${Icon} name="chev-d" size=${12} /></span>
            </${Btn}>
          </div>
        </div>`
      : html`<${Empty} icon="book" title=${canBuild ? 'No recap yet' : 'Nothing to recap yet'}>
          ${canBuild
            ? 'Weave every session summary into one running narrative the table can catch up on in a minute.'
            : 'Summarize at least one session, then generate a story-so-far recap here.'}
        </${Empty}>`}
  </div>`;
}

function PartyMember({ player, onClick }) {
  const ch = player.character_name || '—';
  return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 12, padding: '12px 14px', background: 'var(--surface)', border: '1px solid var(--rule-soft)', borderRadius: 6 }}>
    <${Sigil} ch=${(ch[0] || '?').toUpperCase()} tone=${toneFor(player.player_name || ch)} size="lg" />
    <div style=${{ flex: 1, minWidth: 0 }}>
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${ch}</div>
      <div style=${{ fontSize: 12, color: 'var(--ink-muted)', marginTop: 1 }}>${player.player_name || '—'}${player.pronouns ? ` · ${player.pronouns}` : ''}</div>
    </div>
  </div>`;
}

function SessionRow({ s, onClick }) {
  // `complete` marks a finished stage (green); `current` marks the single next
  // incomplete stage (burgundy). They must be mutually exclusive — StagePill
  // renders `current` over `complete`, so setting both turned a done stage red.
  const up = !!s.has_tracks, t = !!s.has_transcription, sm = !!s.has_summary;
  const stages = [
    { stage: 'upload', complete: up, current: !up },
    { stage: 'transcribe', complete: t, current: up && !t },
    { stage: 'summarize', complete: sm, current: t && !sm },
  ];
  return html`<div onClick=${onClick} style=${{ display: 'flex', alignItems: 'center', gap: 16, padding: '14px 18px', borderBottom: '1px solid var(--rule-soft)', cursor: 'pointer' }}
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

function Stat({ value, label, italic }) {
  return html`<div>
    <div style=${{ fontFamily: italic ? 'var(--font-display)' : 'var(--font-mono)', fontStyle: italic ? 'italic' : 'normal', fontSize: 22, fontWeight: 500, color: 'var(--ink)' }}>${value}</div>
    <div style=${{ fontSize: 11, color: 'var(--ink-faint)', letterSpacing: '0.08em', textTransform: 'uppercase', fontWeight: 600, marginTop: -2 }}>${label}</div>
  </div>`;
}

// Codex teaser on the overview — a glance at what the summarizer remembers.
// Kind breakdown + the few most-recent entries; click through to the full codex.
function CodexTeaser({ campaign, entries }) {
  const open = () => navigate('codex', { id: campaign.campaign_id });
  const total = entries.length;
  // Kinds that actually have entries, in the canonical KINDS order.
  const groups = KINDS
    .map((k) => ({ ...k, n: entries.filter((e) => e.kind === k.value).length }))
    .filter((g) => g.n);
  // A handful of entries to preview, freshest first when we have timestamps.
  const recent = [...entries]
    .sort((a, b) => String(b.updated_at || '').localeCompare(String(a.updated_at || '')))
    .slice(0, 5);

  return html`<div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
    <div style=${{ padding: '14px 18px 10px', display: 'flex', alignItems: 'baseline', gap: 10, borderBottom: '1px solid var(--rule-soft)' }}>
      <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, color: 'var(--ink)' }}>Codex</h3>
      <span style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>· ${total ? `${total} ${total === 1 ? 'entry' : 'entries'} the LLM remembers` : "the LLM's memory"}</span>
      <span style=${{ flex: 1 }} />
      <${Btn} kind="ghost" size="sm" icon="chev-r" onClick=${open}>${total ? 'Open' : 'Build'}</${Btn}>
    </div>
    ${total
      ? html`<div style=${{ padding: 14, display: 'flex', flexDirection: 'column', gap: 12, flex: 1 }}>
          <div style=${{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
            ${groups.map((g) => {
              const col = g.tone === 'ink-blue' ? 'var(--ink-blue)' : `var(--${g.tone})`;
              return html`<span key=${g.value} style=${{
                display: 'inline-flex', alignItems: 'center', gap: 5, padding: '3px 9px', borderRadius: 999,
                background: `var(--${g.tone}-50)`, color: col, border: '1px solid rgba(0,0,0,.06)',
                fontSize: 11.5, fontWeight: 500,
              }}>
                <${Icon} name=${iconForKind(g.value)} size=${11} /> ${g.plural}
                <span style=${{ fontFamily: 'var(--font-mono)', opacity: 0.7 }}>${g.n}</span>
              </span>`;
            })}
          </div>
          <div style=${{ display: 'flex', flexDirection: 'column', gap: 1 }}>
            ${recent.map((e) => html`<div key=${e.entry_id} onClick=${() => navigate('codexEntry', { entryId: e.entry_id })}
              style=${{ display: 'flex', alignItems: 'center', gap: 9, padding: '6px 8px', borderRadius: 4, cursor: 'pointer' }}
              onMouseEnter=${(ev) => { ev.currentTarget.style.background = 'var(--paper)'; }}
              onMouseLeave=${(ev) => { ev.currentTarget.style.background = 'transparent'; }}>
              <${Icon} name=${iconForKind(e.kind)} size=${13} className="ck-ink-muted" />
              <span style=${{ fontFamily: 'var(--font-display)', fontSize: 14, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${e.name}</span>
              ${e.body && html`<span style=${{ flex: 1, minWidth: 0, fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${e.body}</span>`}
            </div>`)}
          </div>
        </div>`
      : html`<div style=${{ flex: 1, display: 'flex' }}>
          <${Empty} icon="book" title="Codex is empty">
            What the summarizer remembers — NPCs, places, items, lore. Add entries, or just run a summary and names get pulled in automatically.
          </${Empty}>
        </div>`}
  </div>`;
}

export function CampaignScreen({ store }) {
  const c = store.campaign;
  if (!c) return html`<div />`;
  const sessions = store.campaignSessions;
  const players = c.players || [];
  const latest = sessions[0];
  const codexEntries = store.codexEntries || [];

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="campaign" active="overview" campaign=${c} />`}
    topbar=${html`<${Topbar} crumbs=${[{ label: 'Campaigns', onClick: () => navigate('library') }, c.name]} right=${html`
      <div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <${Btn} kind="ghost" icon="edit" onClick=${() => openModal('campaign', { edit: c })}>Edit campaign</${Btn}>
        <${Btn} kind="danger" icon="trash" title="Delete campaign" onClick=${() => {
          const n = sessions.length;
          const tail = n ? ` and its ${n} session${n === 1 ? '' : 's'} (transcripts and summaries included)` : '';
          openModal('confirm', {
            title: 'Delete campaign',
            message: `Delete "${c.name}"${tail}? This cannot be undone.`,
            confirmLabel: 'Delete campaign',
            onConfirm: () => deleteCampaign(c.campaign_id),
          });
        }} />
        <${Btn} kind="primary" icon="mic" onClick=${() => navigate('newSession', { id: c.campaign_id })}>New session</${Btn}>
      </div>`} />`}
  >
    <div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, padding: '24px 28px', display: 'flex', alignItems: 'flex-start', gap: 24, marginBottom: 24, position: 'relative', overflow: 'hidden' }}>
      <div style=${{ position: 'absolute', top: 0, right: 0, width: 220, height: '100%', background: 'radial-gradient(circle at 100% 0%, rgba(122,46,31,.07), transparent 60%)' }} />
      <${Sigil} ch=${c.sigil} tone=${c.tone} size="xl" />
      <div style=${{ flex: 1 }}>
        <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--burgundy)', marginBottom: 4 }}>
          Chronicle${c.system ? ` · ${c.system}` : ''}
        </div>
        <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 28, fontWeight: 500, letterSpacing: '-0.015em', color: 'var(--ink)', lineHeight: 1.1 }}>${c.name}</h1>
        <div style=${{ fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 14, color: 'var(--ink-muted)', marginTop: 6 }}>
          ${[c.setting, c.gm && `GM ${c.gm}${c.gm_pronouns ? ` (${c.gm_pronouns})` : ''}`].filter(Boolean).join(' · ') || 'No setting recorded yet'}
        </div>
        <div style=${{ display: 'flex', gap: 22, marginTop: 16 }}>
          <${Stat} value=${sessions.length} label="Sessions" />
          <${Stat} value=${players.length} label="Players" />
          <${Stat} value=${`#${c.next_session_number || 1}`} label="Next session" />
        </div>
      </div>
      ${latest && html`<div style=${{ width: 260, padding: 16, borderRadius: 6, background: 'var(--paper)', border: '1px solid var(--rule-soft)' }}>
        <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 6 }}>Latest session</div>
        <div style=${{ fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 15, color: 'var(--ink)', lineHeight: 1.3 }}>
          ${latest.title || 'Untitled'}
        </div>
        <div style=${{ fontSize: 12, color: 'var(--ink-muted)', marginTop: 6, fontFamily: 'var(--font-mono)' }}>#${latest.session_number} · ${fmtDate(latest.date) || '—'}</div>
        <div style=${{ height: 1, background: 'var(--rule-soft)', margin: '10px 0' }} />
        <div style=${{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
          <${StagePill} stage="upload" complete=${!!latest.has_tracks} current=${!latest.has_tracks} />
          <${StagePill} stage="transcribe" complete=${!!latest.has_transcription} current=${!!latest.has_tracks && !latest.has_transcription} />
          <${StagePill} stage="summarize" complete=${!!latest.has_summary} current=${!!latest.has_transcription && !latest.has_summary} />
        </div>
      </div>`}
    </div>

    <${StorySoFar} campaign=${c} sessions=${sessions} codexEntries=${codexEntries} />

    <div style=${{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16, marginBottom: 24 }}>
      <div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden' }}>
        <div style=${{ padding: '14px 18px 10px', display: 'flex', alignItems: 'baseline', gap: 10, borderBottom: '1px solid var(--rule-soft)' }}>
          <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, color: 'var(--ink)' }}>The Party</h3>
          <span style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>· ${players.length} ${players.length === 1 ? 'soul' : 'souls'}</span>
          <span style=${{ flex: 1 }} />
          <${Btn} kind="ghost" size="sm" icon="plus" onClick=${() => openModal('campaign', { edit: c })}>Add</${Btn}>
        </div>
        <div style=${{ padding: 14, display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
          ${players.length
            ? players.map((p, i) => html`<${PartyMember} key=${i} player=${p} />`)
            : html`<div style=${{ gridColumn: 'span 2' }}><${Empty} icon="users" title="No players yet">Add the company that will tell this story.</${Empty}></div>`}
        </div>
      </div>

      <${CodexTeaser} campaign=${c} entries=${codexEntries} />
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
