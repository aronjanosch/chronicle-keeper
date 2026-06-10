// Screen 02 — Campaign Overview. Hero + party + codex teaser + sessions list.
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, openModal, fmtDate, fmtDateTime, toneFor } from '../core.js';
import { deleteCampaign, generateRecap, revealPath, refreshCampaignSessions, backupWorld } from '../actions.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Sigil, Btn, StagePill, Empty, Markdown, Menu } from '../ui.js';
import { KINDS, iconForKind } from './codex.js';

// Collapsed height cap for the recap body. Tall recaps would otherwise push
// the whole page down; we clamp + fade and offer an Expand toggle.
const RECAP_COLLAPSED_MAX = 180;

function StorySoFar({ campaign, sessions, codexEntries }) {
  const [expanded, setExpanded] = useState(false);
  const recap = (campaign.recap || '').trim();
  const canBuild = sessions.some((s) => s.has_summary);
  // Vault worlds: wikilink linkify lands in Phase 2 — pass empty for now.
  const codexLinks = campaign.vault_path ? [] : (codexEntries || []).map((e) => ({ name: e.name, entry_id: e.entry_id }));
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

function Stat({ value, label, italic }) {
  return html`<div>
    <div style=${{ fontFamily: italic ? 'var(--font-display)' : 'var(--font-mono)', fontStyle: italic ? 'italic' : 'normal', fontSize: 22, fontWeight: 500, color: 'var(--ink)' }}>${value}</div>
    <div style=${{ fontSize: 11, color: 'var(--ink-faint)', letterSpacing: '0.08em', textTransform: 'uppercase', fontWeight: 600, marginTop: -2 }}>${label}</div>
  </div>`;
}

// Codex teaser on the overview. For vault worlds: reads vault pages (kind from
// `page.kind`, click → page editor). For legacy campaigns: reads codex entries.
function CodexTeaser({ campaign, entries, vaultPages }) {
  const hasVault = !!campaign.vault_path;
  const open = () => navigate('codex', { id: campaign.campaign_id });

  // Normalize to a common shape for group + recent rendering.
  const items = hasVault
    ? (vaultPages || []).map((p) => ({
        id: p.path, kind: p.kind || 'lore', name: p.title,
        body: p.summary, sortKey: p.modified || 0,
        onClick: () => navigate('page', { page: p.path }),
      }))
    : (entries || []).map((e) => ({
        id: e.entry_id, kind: e.kind, name: e.name,
        body: e.body, sortKey: e.updated_at || '',
        onClick: () => navigate('codexEntry', { entryId: e.entry_id }),
      }));

  const total = items.length;
  const groups = KINDS
    .map((k) => ({ ...k, n: items.filter((i) => i.kind === k.value).length }))
    .filter((g) => g.n);
  const recent = [...items]
    .sort((a, b) => hasVault
      ? (b.sortKey || 0) - (a.sortKey || 0)
      : String(b.sortKey || '').localeCompare(String(a.sortKey || '')))
    .slice(0, 5);

  const subtitle = hasVault
    ? (total ? `${total} ${total === 1 ? 'page' : 'pages'}` : 'wiki pages')
    : (total ? `${total} ${total === 1 ? 'entry' : 'entries'} the LLM remembers` : "the LLM's memory");
  const emptyMsg = hasVault
    ? 'Your Codex starts empty. Build pages for NPCs, places, factions — the AI will help keep them current in Phase 4.'
    : 'What the summarizer remembers — NPCs, places, items, lore. Add entries, or run a summary and names get pulled in automatically.';

  return html`<div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
    <div style=${{ padding: '14px 18px 10px', display: 'flex', alignItems: 'baseline', gap: 10, borderBottom: '1px solid var(--rule-soft)' }}>
      <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, color: 'var(--ink)' }}>Codex</h3>
      <span style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>· ${subtitle}</span>
      <span style=${{ flex: 1 }} />
      <${Btn} kind="ghost" size="sm" icon="chev-r" onClick=${open}>${total ? 'Open' : (hasVault ? 'Add pages' : 'Build')}</${Btn}>
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
            ${recent.map((item) => html`<div key=${item.id} onClick=${item.onClick}
              style=${{ display: 'flex', alignItems: 'center', gap: 9, padding: '6px 8px', borderRadius: 4, cursor: 'pointer' }}
              onMouseEnter=${(ev) => { ev.currentTarget.style.background = 'var(--paper)'; }}
              onMouseLeave=${(ev) => { ev.currentTarget.style.background = 'transparent'; }}>
              <${Icon} name=${iconForKind(item.kind)} size=${13} className="ck-ink-muted" />
              <span style=${{ fontFamily: 'var(--font-display)', fontSize: 14, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${item.name}</span>
              ${item.body && html`<span style=${{ flex: 1, minWidth: 0, fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${item.body}</span>`}
            </div>`)}
          </div>
        </div>`
      : html`<div style=${{ flex: 1, display: 'flex' }}>
          <${Empty} icon="book" title="Codex is empty">${emptyMsg}</${Empty}>
        </div>`}
  </div>`;
}

export function CampaignScreen({ store }) {
  const c = store.campaign;
  // Stale-while-revalidate: the list in the store predates sessions created
  // elsewhere (e.g. a draft saved on the New-Session screen).
  useEffect(() => { refreshCampaignSessions(); }, [c?.campaign_id]);
  if (!c) return html`<div />`;
  const sessions = store.campaignSessions;
  const players = c.players || [];
  const latest = sessions[0];
  const codexEntries = store.codexEntries || [];
  const vaultPages = store.vaultPages || [];

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="campaign" active="overview" campaign=${c} />`}
    topbar=${html`<${Topbar} crumbs=${[{ label: 'Worlds', onClick: () => navigate('library') }, c.name]} right=${html`
      <div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <${Btn} kind="primary" icon="mic" onClick=${() => navigate('newSession', { id: c.campaign_id })}>New session</${Btn}>
        <${Menu} items=${[
          { label: 'Edit world', icon: 'edit', onClick: () => openModal('campaign', { edit: c }) },
          { label: 'Reveal folder', icon: 'folder', hidden: !(window.__TAURI__ && c.vault_path), onClick: () => revealPath(c.vault_path) },
          { label: 'Export world…', icon: 'download', hidden: !c.vault_path, onClick: () => openModal('exportWorld') },
          { label: 'Back up now', icon: 'shield', hidden: !c.vault_path, onClick: backupWorld },
          { label: 'Delete world', icon: 'trash', danger: true, onClick: () => {
            const n = sessions.length;
            const tail = n ? ` and its ${n} session${n === 1 ? '' : 's'} (transcripts and summaries included)` : '';
            openModal('confirm', {
              title: 'Delete world',
              message: `Delete "${c.name}"${tail}? This cannot be undone.`,
              confirmLabel: 'Delete world',
              onConfirm: () => deleteCampaign(c.campaign_id),
            });
          } },
        ]} />
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

      <${CodexTeaser} campaign=${c} entries=${codexEntries} vaultPages=${vaultPages} />
    </div>

  </${Shell}>`;
}
