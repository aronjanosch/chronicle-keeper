// Screen 01 — Campaign Library. Real data: each campaign is a "tome" card.
import { html } from '../../vendor/htm-preact-standalone.mjs';
import { useState } from '../../vendor/htm-preact-standalone.mjs';
import { openModal } from '../core.js';
import { openCampaign } from '../actions.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Sigil, Btn, Spinner, Empty } from '../ui.js';

function PartyAvatars({ players = [] }) {
  const cols = ['#E8DAA8', '#E5C4B5', '#C9D4B5', '#B9C7D6', '#E2C0A8'];
  const shown = players.slice(0, 5);
  return html`<div style=${{ display: 'flex' }}>
    ${shown.map((p, i) => html`<div key=${i} style=${{
      width: 22, height: 22, borderRadius: '50%', background: cols[i % cols.length], color: 'var(--ink)',
      fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 9,
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      border: '1.5px solid var(--surface)', marginLeft: i === 0 ? 0 : -7, position: 'relative', zIndex: 10 - i,
    }}>${(p.character_name || p.player_name || '?').slice(0, 1).toUpperCase()}</div>`)}
  </div>`;
}

// Id of the sample campaign seeded on first launch (see ck-core seed.rs). Flagged
// in the UI so users know it's a demo they can safely delete.
const EXAMPLE_CAMPAIGN_ID = 'example-ashfall';

function CampaignCard({ c }) {
  const players = c.players || [];
  const next = c.next_session_number || 1;
  const isExample = c.campaign_id === EXAMPLE_CAMPAIGN_ID;
  return html`<div onClick=${() => openCampaign(c.campaign_id)} style=${{
    background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8,
    padding: 18, display: 'flex', flexDirection: 'column', gap: 14,
    boxShadow: '0 1px 0 rgba(120,90,40,.05), 0 1px 2px rgba(60,40,10,.04)', position: 'relative', cursor: 'pointer',
  }}
    onMouseEnter=${(e) => { e.currentTarget.style.borderColor = 'var(--rule-strong)'; }}
    onMouseLeave=${(e) => { e.currentTarget.style.borderColor = 'var(--rule)'; }}>
    <div style=${{ position: 'absolute', left: 0, top: 14, bottom: 14, width: 3, background: `var(--${c.tone})`, borderRadius: '0 2px 2px 0', opacity: 0.85 }} />
    ${isExample && html`<div title="A sample chronicle — explore it, then delete it whenever you're ready." style=${{
      position: 'absolute', right: 12, top: 12, fontSize: 10, fontWeight: 600, letterSpacing: '0.08em', textTransform: 'uppercase',
      padding: '2px 7px', borderRadius: 999, background: 'var(--paper-deep)', border: '1px solid var(--rule)', color: 'var(--ink-muted)',
    }}>Example</div>`}
    <div style=${{ display: 'flex', alignItems: 'flex-start', gap: 12 }}>
      <${Sigil} ch=${c.sigil} tone=${c.tone} size="lg" />
      <div style=${{ flex: 1, minWidth: 0 }}>
        <div style=${{ fontFamily: 'var(--font-display)', fontSize: 19, fontWeight: 500, color: 'var(--ink)', lineHeight: 1.2, letterSpacing: '-0.01em' }}>${c.name}</div>
        <div style=${{ fontSize: 12, color: 'var(--ink-muted)', marginTop: 3, display: 'flex', alignItems: 'center', gap: 6 }}>
          <span>${c.system || 'System —'}</span>
          ${c.setting && html`<span style=${{ color: 'var(--ink-ghost)' }}>·</span><span style=${{ fontStyle: 'italic', fontFamily: 'var(--font-display)' }}>${c.setting}</span>`}
        </div>
      </div>
    </div>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 10, minHeight: 22 }}>
      <${PartyAvatars} players=${players} />
      <div style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>
        ${players.length} player${players.length === 1 ? '' : 's'}${c.gm ? ` · GM ${c.gm}` : ''}
      </div>
    </div>
    <div style=${{ height: 1, background: 'var(--rule-soft)' }} />
    <div style=${{ display: 'flex', alignItems: 'center', gap: 16, fontSize: 12, color: 'var(--ink-muted)' }}>
      <span>Next session <b style=${{ color: 'var(--ink)', fontWeight: 600, fontFamily: 'var(--font-mono)' }}>#${next}</b></span>
      <span style=${{ flex: 1 }} />
      <${Icon} name="chev-r" size=${12} />
    </div>
  </div>`;
}

export function LibraryScreen({ store }) {
  const [q, setQ] = useState('');
  const campaigns = store.campaigns.filter((c) => !q || (c.name || '').toLowerCase().includes(q.toLowerCase()));
  const newBtn = html`<${Btn} kind="primary" icon="plus" onClick=${() => openModal('campaign', {})}>New campaign</${Btn}>`;

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="library" active="campaigns" />`}
    topbar=${html`<${Topbar} crumbs=${['Library', 'Campaigns']} right=${html`
      <div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 4, color: 'var(--ink-muted)', fontSize: 12.5, minWidth: 220 }}>
          <${Icon} name="search" size=${13} />
          <input value=${q} onInput=${(e) => setQ(e.target.value)} placeholder="Search campaigns…"
            style=${{ flex: 1, border: 'none', background: 'transparent', outline: 'none', fontSize: 12.5, color: 'var(--ink)' }} />
        </div>
        ${newBtn}
      </div>`} />`}
  >
    <div style=${{ marginBottom: 22 }}>
      <div style=${{ fontSize: 11, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 4 }}>Welcome back</div>
      <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 32, fontWeight: 500, letterSpacing: '-0.02em', lineHeight: 1.1, color: 'var(--ink)' }}>
        Your <em style=${{ fontStyle: 'italic', color: 'var(--burgundy)' }}>chronicles</em>
      </h1>
      <div style=${{ fontSize: 13, color: 'var(--ink-muted)', marginTop: 6, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>
        ${store.campaigns.length
          ? `${store.campaigns.length} campaign${store.campaigns.length === 1 ? '' : 's'} in play.`
          : 'No campaigns yet — begin your first chronicle.'}
      </div>
    </div>

    ${store.loading && !store.campaigns.length
      ? html`<div style=${{ display: 'flex', justifyContent: 'center', padding: 60 }}><${Spinner} size=${22} /></div>`
      : html`<div style=${{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 16 }}>
          ${campaigns.map((c) => html`<${CampaignCard} key=${c.campaign_id} c=${c} />`)}
          <div onClick=${() => openModal('campaign', {})} style=${{
            background: 'transparent', border: '1.5px dashed var(--rule-strong)', borderRadius: 8, padding: 24,
            display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: 10,
            minHeight: 160, color: 'var(--ink-muted)', cursor: 'pointer',
            gridColumn: campaigns.length % 2 === 0 ? 'span 2' : 'auto',
          }}>
            <div style=${{ width: 40, height: 40, borderRadius: 6, background: 'var(--paper-deep)', border: '1px solid var(--rule)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--burgundy)' }}>
              <${Icon} name="plus" size=${18} />
            </div>
            <div style=${{ fontFamily: 'var(--font-display)', fontSize: 16, fontWeight: 500, color: 'var(--ink-soft)' }}>Begin a new chronicle</div>
            <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic', fontFamily: 'var(--font-display)', textAlign: 'center' }}>
              Name the campaign, the system, the world, the company that will tell it.
            </div>
          </div>
        </div>`}
  </${Shell}>`;
}
