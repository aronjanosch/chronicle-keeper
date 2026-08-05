// Screen 01 — Campaign Library. Real data: each campaign is a "tome" card.
import { html } from '../../vendor/htm-preact-standalone.mjs';
import { useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, openModal } from '../core.js';
import { openCampaign, refreshOnboarding, dismissOnboarding, EXAMPLE_CAMPAIGN_ID } from '../actions.js';
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

// Getting-started checklist: state-derived (each step checks real app state,
// nothing is stored except the dismissal), auto-hides once every step is done.
function GettingStarted({ store }) {
  useEffect(() => { refreshOnboarding(); }, []);
  const s = store.onboarding;
  const allDone = s && Object.values(s).every(Boolean);
  useEffect(() => { if (allDone) dismissOnboarding(); }, [allDone]);
  if (!s || allDone) return null;

  const goKeeper = () => {
    const target = store.campaigns.find((c) => c.campaign_id === EXAMPLE_CAMPAIGN_ID) || store.campaigns[0];
    if (target) openCampaign(target.campaign_id).then(() => navigate('keeper', { id: target.campaign_id }));
  };
  const steps = [
    { done: s.provider, label: 'Connect an AI', sub: 'Local Ollama, or any cloud key — it never leaves this machine', go: () => navigate('settings') },
    { done: s.model, label: 'Get the local model', sub: 'gemma4:e2b · ~7.2 GB, runs fully on-device', go: () => openModal('provider', { id: 'ollama' }) },
    { done: s.keeper, label: 'Ask the Keeper', sub: 'Try it on the example world — it reads your pages to answer', go: goKeeper },
    { done: s.world, label: 'Create your own world', sub: 'The example is a sandbox; your chronicle starts here', go: () => navigate('newWorld') },
  ];
  return html`<div style=${{
    background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8,
    padding: '14px 18px', marginBottom: 22, position: 'relative',
  }}>
    <div style=${{ display: 'flex', alignItems: 'center', marginBottom: 10 }}>
      <div style=${{ fontSize: 11, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>Getting started</div>
      <span style=${{ flex: 1 }} />
      <${Btn} kind="ghost" size="sm" icon="x" title="Dismiss" onClick=${dismissOnboarding} />
    </div>
    <div style=${{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: '8px 20px' }}>
      ${steps.map((st) => html`<div key=${st.label} onClick=${st.done ? undefined : st.go} style=${{
        display: 'flex', alignItems: 'flex-start', gap: 10, cursor: st.done ? 'default' : 'pointer',
        opacity: st.done ? 0.55 : 1,
      }}>
        <div style=${{
          width: 18, height: 18, borderRadius: '50%', flex: '0 0 auto', marginTop: 1,
          border: st.done ? 'none' : '1.5px solid var(--rule-strong)',
          background: st.done ? 'var(--moss)' : 'transparent', color: 'var(--paper)',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
        }}>${st.done && html`<${Icon} name="check" size=${11} />`}</div>
        <div style=${{ minWidth: 0 }}>
          <div style=${{ fontSize: 13, fontWeight: 500, color: 'var(--ink)', textDecoration: st.done ? 'line-through' : 'none' }}>${st.label}</div>
          <div style=${{ fontSize: 11.5, color: 'var(--ink-muted)' }}>${st.sub}</div>
        </div>
      </div>`)}
    </div>
  </div>`;
}

function CampaignCard({ c }) {
  // Co-GMs sit in the roster but are not the party.
  const players = (c.players || []).filter((p) => !p.is_gm);
  const gms = c.gms?.length ? c.gms : (c.gm ? [c.gm] : []);
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
        ${players.length} player${players.length === 1 ? '' : 's'}${gms.length ? ` · GM ${gms.join(', ')}` : ''}
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
  const newBtn = html`<${Btn} kind="primary" icon="plus" onClick=${() => navigate('newWorld')}>New world</${Btn}>`;

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="library" active="worlds" />`}
    topbar=${html`<${Topbar} crumbs=${['Library', 'Worlds']} right=${html`
      <div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 4, color: 'var(--ink-muted)', fontSize: 12.5, minWidth: 220 }}>
          <${Icon} name="search" size=${13} />
          <input value=${q} onInput=${(e) => setQ(e.target.value)} placeholder="Search worlds…"
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
          ? `${store.campaigns.length} world${store.campaigns.length === 1 ? '' : 's'} in play.`
          : 'No worlds yet — begin your first chronicle.'}
      </div>
    </div>

    <${GettingStarted} store=${store} />

    ${store.loading && !store.campaigns.length
      ? html`<div style=${{ display: 'flex', justifyContent: 'center', padding: 60 }}><${Spinner} size=${22} /></div>`
      : html`<div style=${{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 16 }}>
          ${campaigns.map((c) => html`<${CampaignCard} key=${c.campaign_id} c=${c} />`)}
          <div onClick=${() => navigate('newWorld')} style=${{
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
              Name the world, the system, the setting, the company that will tell it.
            </div>
          </div>
        </div>`}
  </${Shell}>`;
}
