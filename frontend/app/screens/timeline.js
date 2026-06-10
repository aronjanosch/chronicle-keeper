// The Timeline (Phase 11 + 11.5): in-world events (pages with `date:` or
// `order:` frontmatter, parsed + sorted server-side on the world's
// `[calendar]`) and a real-world session lane. Two tabs — the axes are
// different calendars, so they don't mix. Sessions with a `world_date` in
// session.toml also appear on the World lane as `session:<id>` entries.
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, useStore, apiFetch, openModal } from '../core.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Empty, Btn } from '../ui.js';
import { refreshCampaignSessions, loadSession, loadVaultTree } from '../actions.js';
import { iconForKind } from './codex.js';

// Consecutive events of the same era+year share one header; order-only
// (relative) beats have no year and form their own leading group.
function groupEvents(events) {
  const groups = [];
  let cur = null;
  for (const ev of events) {
    const key = ev.year == null && !ev.era ? '·seq' : `${ev.era || ''}·${ev.year}`;
    if (!cur || cur.key !== key) {
      cur = { key, era: ev.era, year: ev.year, items: [] };
      groups.push(cur);
    }
    cur.items.push(ev);
  }
  return groups;
}

const topFolder = (path) => (path.includes('/') ? path.split('/')[0] : '');

function applyFilters(events, f) {
  return events.filter((ev) =>
    (!f.kind || ev.kind === f.kind)
    && (!f.tag || (ev.tags || []).includes(f.tag))
    && (!f.folder || topFolder(ev.path) === f.folder)
    && (!f.entity || (ev.links || []).some((l) => l.label === f.entity))
    && (!f.hideGm || !ev.gm_only));
}

function Tab({ active, onClick, icon, children }) {
  return html`<span onClick=${onClick} style=${{
    padding: '5px 12px', borderRadius: 4, cursor: 'pointer', fontSize: 12.5,
    display: 'flex', alignItems: 'center', gap: 6,
    background: active ? 'var(--paper-deep)' : 'transparent',
    color: active ? 'var(--ink)' : 'var(--ink-muted)', fontWeight: active ? 500 : 400,
  }}><${Icon} name=${icon} size=${12} /> ${children}</span>`;
}

function Rail({ children }) {
  return html`<div style=${{ position: 'relative', paddingLeft: 22, borderLeft: '2px solid var(--rule)', marginLeft: 8, display: 'flex', flexDirection: 'column', gap: 14 }}>${children}</div>`;
}

// Span events (with an end date) get a bar instead of a dot.
function Dot({ span }) {
  return html`<span style=${{
    position: 'absolute', left: span ? -25 : -27, top: 5,
    width: span ? 4 : 8, height: span ? 26 : 8, borderRadius: 999,
    background: 'var(--burgundy)', border: span ? 'none' : '2px solid var(--paper)',
  }} />`;
}

function FacetSelect({ value, onChange, any, options }) {
  return html`<select value=${value || ''} onChange=${(e) => onChange(e.target.value || null)}
    style=${{ fontSize: 12, padding: '3px 6px', borderRadius: 5, border: '1px solid var(--rule)', background: 'var(--surface)', color: 'var(--ink-soft)', maxWidth: 160 }}>
    <option value="">${any}</option>
    ${options.map((o) => html`<option key=${o} value=${o}>${o}</option>`)}
  </select>`;
}

// Lane filters (11.5E): kind / tag / folder / linked entity, built from the
// loaded events; plus a hide-GM-only toggle (11.5D).
function FilterBar({ events, f, setF }) {
  const uniq = (xs) => [...new Set(xs.filter(Boolean))].sort();
  const kinds = uniq(events.map((ev) => ev.kind));
  const tags = uniq(events.flatMap((ev) => ev.tags || []));
  const folders = uniq(events.map((ev) => topFolder(ev.path)));
  const entities = uniq(events.flatMap((ev) => (ev.links || []).map((l) => l.label)));
  const hasGm = events.some((ev) => ev.gm_only);
  const active = f.kind || f.tag || f.folder || f.entity || f.hideGm;
  const set = (k) => (v) => setF({ ...f, [k]: v });
  return html`<div style=${{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 8, marginBottom: 20 }}>
    <${Icon} name="search" size=${12} className="ck-ink-faint" />
    ${kinds.length > 1 && html`<${FacetSelect} value=${f.kind} onChange=${set('kind')} any="Any kind" options=${kinds} />`}
    ${tags.length > 0 && html`<${FacetSelect} value=${f.tag} onChange=${set('tag')} any="Any tag" options=${tags} />`}
    ${folders.length > 1 && html`<${FacetSelect} value=${f.folder} onChange=${set('folder')} any="Any folder" options=${folders} />`}
    ${entities.length > 0 && html`<${FacetSelect} value=${f.entity} onChange=${set('entity')} any="Anyone / anywhere" options=${entities} />`}
    ${hasGm && html`<label style=${{ display: 'flex', alignItems: 'center', gap: 5, fontSize: 12, color: 'var(--ink-soft)', cursor: 'pointer' }}>
      <input type="checkbox" checked=${!!f.hideGm} onChange=${(e) => setF({ ...f, hideGm: e.target.checked })} /> Hide GM-only
    </label>`}
    ${active && html`<button onClick=${() => setF({})} style=${{ fontSize: 11.5, color: 'var(--burgundy)', background: 'none', border: 'none', cursor: 'pointer', padding: 0 }}>Clear</button>`}
  </div>`;
}

// Navigable participants/location chips (11.5F).
function LinkChips({ links }) {
  return html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 5, marginTop: 5 }}>
    ${links.map((l, i) => html`<span key=${i}
      onClick=${() => l.path && navigate('page', { path: l.path })}
      style=${{ display: 'inline-flex', alignItems: 'center', gap: 4, padding: '2px 8px', borderRadius: 999, fontSize: 11.5,
        background: 'var(--surface)', border: '1px solid var(--rule)',
        color: l.path ? 'var(--burgundy)' : 'var(--ink-muted)', cursor: l.path ? 'pointer' : 'default' }}>
      <${Icon} name=${l.predicate === 'location' ? 'map' : 'users'} size=${10} /> ${l.label}
    </span>`)}
  </div>`;
}

function EventItem({ ev }) {
  const isSession = ev.path.startsWith('session:');
  const open = () => (isSession ? loadSession(ev.path.slice(8)) : navigate('page', { path: ev.path }));
  const when = ev.display
    ? `${ev.display}${ev.end_display ? ` – ${ev.end_display}` : ''}`
    : (ev.order != null ? `seq ${ev.order}` : '');
  return html`<div style=${{ position: 'relative' }}>
    <${Dot} span=${!!ev.end_display} />
    ${when && html`<div style=${{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)' }}>${when}</div>`}
    <div onClick=${open} style=${{ display: 'flex', alignItems: 'center', gap: 7, cursor: 'pointer', marginTop: 2 }}>
      <${Icon} name=${isSession ? 'mic' : iconForKind(ev.kind)} size=${13} className="ck-ink-muted" />
      <span style=${{ fontSize: 14.5, fontWeight: 500, color: 'var(--burgundy)' }}>${ev.title}</span>
      ${ev.gm_only && html`<span style=${{ fontSize: 9.5, fontWeight: 600, letterSpacing: '0.06em', padding: '1px 6px', borderRadius: 3, background: 'var(--paper-deep)', border: '1px solid var(--rule)', color: 'var(--ink-muted)' }}>GM</span>`}
    </div>
    ${ev.summary && html`<div style=${{ fontSize: 13, color: 'var(--ink-soft)', marginTop: 3, maxWidth: 560 }}>${ev.summary}</div>`}
    ${(ev.links || []).length > 0 && html`<${LinkChips} links=${ev.links} />`}
  </div>`;
}

// Preset for the modal when adding from a group header: same year (with era)
// for dated groups, next free order for the relative group.
function groupPreset(g, events) {
  if (g.year != null || g.era) return { date: [g.year, g.era].filter((x) => x != null).join(' ') };
  const next = Math.max(0, ...events.map((ev) => (ev.order != null ? ev.order : -1))) + 1;
  return { order: next };
}

function WorldLane({ events, onAdd }) {
  const [f, setF] = useState({});
  if (!events.length) {
    return html`<${Empty} icon="time" title="No dated pages yet">
      Give any page a <code>date:</code> frontmatter field (<code>1374-08-12 DR</code> style —
      year, optional month/day, optional era) and it appears here; calendar-less worlds can use
      an <code>order:</code> integer instead. The <b>event</b> kind carries the field by
      default; month and era names come from <code>[calendar]</code> in <code>.ck/config.toml</code>.
      <div style=${{ marginTop: 14 }}><${Btn} kind="primary" onClick=${() => onAdd({})}>Add the first event</${Btn}></div>
    </${Empty}>`;
  }
  const filtered = applyFilters(events, f);
  return html`<div>
    <${FilterBar} events=${events} f=${f} setF=${setF} />
    ${!filtered.length && html`<div style=${{ fontSize: 12.5, color: 'var(--ink-faint)', fontStyle: 'italic' }}>No events match these filters.</div>`}
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 22 }}>
      ${groupEvents(filtered).map((g) => html`<div key=${g.key} class="ck-tl-group">
        <div style=${{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
          <span style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, color: 'var(--ink)' }}>
            ${g.year == null && !g.era ? 'Relative order' : [g.year, g.era].filter((x) => x != null).join(' ')}
          </span>
          <span class="ck-tl-add" onClick=${() => onAdd(groupPreset(g, events))}
            title="Add an event here"
            style=${{ display: 'inline-flex', alignItems: 'center', cursor: 'pointer', color: 'var(--ink-faint)', padding: 2 }}>
            <${Icon} name="plus" size=${12} />
          </span>
        </div>
        <${Rail}>
          ${g.items.map((ev) => html`<${EventItem} key=${ev.path} ev=${ev} />`)}
        </${Rail}>
      </div>`)}
    </div>
  </div>`;
}

function SessionLane({ sessions }) {
  if (!sessions.length) {
    return html`<${Empty} icon="mic" title="No sessions yet">Recorded sessions plot here by their real-world date.</${Empty}>`;
  }
  return html`<${Rail}>
    ${sessions.map((s) => html`<div key=${s.session_id} style=${{ position: 'relative' }}>
      <${Dot} />
      <div style=${{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)' }}>
        ${s.date || 'no date'} · session ${String(s.session_number || 0).padStart(2, '0')}
      </div>
      <div onClick=${() => loadSession(s.session_id)} style=${{ display: 'flex', alignItems: 'center', gap: 7, cursor: 'pointer', marginTop: 2 }}>
        <${Icon} name="mic" size=${13} className="ck-ink-muted" />
        <span style=${{ fontSize: 14.5, fontWeight: 500, color: 'var(--burgundy)' }}>
          ${s.title || 'Untitled session'}
        </span>
      </div>
    </div>`)}
  </${Rail}>`;
}

export function TimelineScreen() {
  const store = useStore();
  const c = store.campaign;
  const [tab, setTab] = useState('world');
  const [data, setData] = useState(null);

  const reload = () => apiFetch(`/campaigns/${c.campaign_id}/timeline`)
    .then(setData)
    .catch(() => setData({ events: [] }));

  useEffect(() => {
    if (!c) return;
    setData(null);
    reload();
    if (!(store.campaignSessions || []).length) refreshCampaignSessions();
    if (!(store.vaultPages || []).length) loadVaultTree(c.campaign_id); // pickers in the new-event modal
  }, [c?.campaign_id]);

  if (!c) { navigate('library'); return null; }

  const addEvent = (preset) => openModal('newEvent', { ...preset, onCreated: reload });

  const sessions = [...(store.campaignSessions || [])]
    .sort((a, b) => String(a.date || '').localeCompare(String(b.date || '')) || (a.session_number || 0) - (b.session_number || 0));

  const topbar = html`<${Topbar} crumbs=${[
    { label: 'Worlds', onClick: () => navigate('library') },
    { label: c.name, onClick: () => navigate('campaign', { id: c.campaign_id }) },
    'Timeline',
  ]} right=${html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10 }}>
    ${tab === 'world' && html`<${Btn} kind="ghost" size="sm" icon="plus" onClick=${() => addEvent({})}>Event</${Btn}>`}
    <div style=${{ display: 'flex', gap: 2, padding: 2, background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 6 }}>
      <${Tab} icon="globe" active=${tab === 'world'} onClick=${() => setTab('world')}>World</${Tab}>
      <${Tab} icon="mic" active=${tab === 'sessions'} onClick=${() => setTab('sessions')}>Sessions</${Tab}>
    </div>
  </div>`} />`;

  return html`<${Shell} sidebar=${html`<${Sidebar} variant="campaign" active="timeline" campaign=${c} />`}
    topbar=${topbar} bodyStyle=${{ padding: '30px 36px' }}>
    <div style=${{ maxWidth: 760, margin: '0 auto' }}>
      ${data === null
        ? html`<div style=${{ color: 'var(--ink-faint)', fontStyle: 'italic' }}>Loading…</div>`
        : tab === 'world'
          ? html`<${WorldLane} events=${data.events || []} onAdd=${addEvent} />`
          : html`<${SessionLane} sessions=${sessions} />`}
    </div>
  </${Shell}>`;
}
