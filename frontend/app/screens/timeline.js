// The Timeline (Phase 11 + 11.5): in-world events (pages with `date:` or
// `order:` frontmatter, parsed + sorted server-side on the world's
// `[calendar]`) and a real-world session lane. Two tabs — the axes are
// different calendars, so they don't mix. Sessions with a `world_date` in
// session.toml also appear on the World lane as `session:<id>` entries.
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, useStore, apiFetch, openModal } from '../core.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Empty, Btn, useAsset } from '../ui.js';
import { refreshCampaignSessions, loadSession, loadVaultTree } from '../actions.js';
import { iconForKind, toneForKind } from './codex.js';

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
  return html`<div style=${{ position: 'relative', paddingLeft: 22, borderLeft: '2px solid var(--rule)', marginLeft: 8, display: 'flex', flexDirection: 'column', gap: 16 }}>${children}</div>`;
}

// Diamond spine marker; span events (with an end date) are filled solid.
function Marker({ tone, span }) {
  return html`<span style=${{
    position: 'absolute', left: -28, top: 13, width: 11, height: 11,
    transform: 'rotate(45deg)', borderRadius: 2,
    background: span ? `var(--${tone})` : 'var(--paper)',
    border: `2px solid var(--${tone})`,
  }} />`;
}

const plural = (n, unit) => `${n} ${unit}${n === 1 ? '' : 's'}`;

// "70 years later" between consecutive events. Cross-era gaps stay blank;
// day-level math only within one month ([calendar] has no month lengths).
function gapLabel(prev, ev) {
  if (!prev || prev.year == null || ev.year == null || (prev.era || '') !== (ev.era || '')) return null;
  const dy = ev.year - prev.year;
  if (dy > 0) return `${plural(dy, 'year')} later`;
  if (dy < 0 || !prev.month || !ev.month) return null;
  const dm = ev.month - prev.month;
  if (dm > 0) return `${plural(dm, 'month')} later`;
  if (dm === 0 && prev.day && ev.day && ev.day > prev.day) return `${plural(ev.day - prev.day, 'day')} later`;
  return null;
}

function durationLabel(ev) {
  if (ev.year == null || ev.end_year == null) return null;
  const dy = ev.end_year - ev.year;
  if (dy > 0) return plural(dy, 'year');
  if (dy < 0 || !ev.month || !ev.end_month) return null;
  const dm = ev.end_month - ev.month;
  if (dm > 0) return plural(dm, 'month');
  if (dm === 0 && ev.day && ev.end_day && ev.end_day > ev.day) return plural(ev.end_day - ev.day, 'day');
  return null;
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

function EventItem({ ev, cid, gap }) {
  const isSession = ev.path.startsWith('session:');
  const tone = isSession ? 'burgundy' : toneForKind(ev.kind);
  const img = useAsset(cid, ev.image);
  const open = () => (isSession ? loadSession(ev.path.slice(8)) : navigate('page', { path: ev.path }));
  const dur = durationLabel(ev);
  const when = ev.display
    ? `${ev.display}${ev.end_display ? ` → ${ev.end_display}` : ''}${dur ? ` (${dur})` : ''}`
    : (ev.order != null ? `seq ${ev.order}` : '');
  return html`<div>
    ${gap && html`<div style=${{ fontSize: 11, fontStyle: 'italic', color: 'var(--ink-faint)', margin: '0 0 6px 2px' }}>
      ${ev.end_display ? `Started ${gap}` : gap}
    </div>`}
    <div style=${{ position: 'relative' }}>
      <${Marker} tone=${tone} span=${!!ev.end_display} />
      <div class="ck-tl-card" style=${{ '--tone': `var(--${tone})` }} onClick=${open}>
        ${img && html`<img class="ck-tl-banner" src=${img} alt="" />`}
        <div style=${{ padding: '10px 14px 11px' }}>
          <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style=${{ color: `var(--${tone})`, display: 'inline-flex' }}>
              <${Icon} name=${isSession ? 'mic' : iconForKind(ev.kind)} size=${14} />
            </span>
            <span style=${{ fontSize: 14.5, fontWeight: 500, color: 'var(--ink)' }}>${ev.title}</span>
            ${ev.gm_only && html`<span style=${{ fontSize: 9.5, fontWeight: 600, letterSpacing: '0.06em', padding: '1px 6px', borderRadius: 3, background: 'var(--paper-deep)', border: '1px solid var(--rule)', color: 'var(--ink-muted)' }}>GM</span>`}
          </div>
          ${when && html`<div style=${{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)', marginTop: 3 }}>${when}</div>`}
          ${ev.summary && html`<div style=${{ fontSize: 13, color: 'var(--ink-soft)', marginTop: 5, maxWidth: 560 }}>${ev.summary}</div>`}
          ${(ev.links || []).length > 0 && html`<${LinkChips} links=${ev.links} />`}
        </div>
      </div>
    </div>
  </div>`;
}

// Preset for the modal when adding from a group header: same year (with era)
// for dated groups, next free order for the relative group.
function groupPreset(g, events) {
  if (g.year != null || g.era) return { date: [g.year, g.era].filter((x) => x != null).join(' ') };
  const next = Math.max(0, ...events.map((ev) => (ev.order != null ? ev.order : -1))) + 1;
  return { order: next };
}

function WorldLane({ events, onAdd, cid }) {
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
  const groups = groupEvents(filtered);
  // Gap labels compare each event to the one before it across group borders;
  // the year-level gap surfaces next to the group header, finer ones inline.
  let prev = null;
  for (const g of groups) {
    for (const ev of g.items) {
      ev._gap = gapLabel(prev, ev);
      prev = ev;
    }
  }
  return html`<div>
    <${FilterBar} events=${events} f=${f} setF=${setF} />
    ${!filtered.length && html`<div style=${{ fontSize: 12.5, color: 'var(--ink-faint)', fontStyle: 'italic' }}>No events match these filters.</div>`}
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 26 }}>
      ${groups.map((g) => html`<div key=${g.key} class="ck-tl-group">
        <div style=${{ display: 'flex', alignItems: 'baseline', gap: 9, marginBottom: 10 }}>
          <span style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, color: 'var(--ink)' }}>
            ${g.year == null && !g.era ? 'Relative order' : [g.year, g.era].filter((x) => x != null).join(' ')}
          </span>
          ${g.items[0]._gap && html`<span style=${{ fontSize: 11, fontStyle: 'italic', color: 'var(--ink-faint)' }}>
            ${g.items[0].end_display ? `started ${g.items[0]._gap}` : g.items[0]._gap}
          </span>`}
          <span class="ck-tl-add" onClick=${() => onAdd(groupPreset(g, events))}
            title="Add an event here"
            style=${{ display: 'inline-flex', alignItems: 'center', cursor: 'pointer', color: 'var(--ink-faint)', padding: 2, alignSelf: 'center' }}>
            <${Icon} name="plus" size=${12} />
          </span>
        </div>
        <${Rail}>
          ${g.items.map((ev, i) => html`<${EventItem} key=${ev.path} ev=${ev} cid=${cid} gap=${i > 0 ? ev._gap : null} />`)}
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
      <${Marker} tone="burgundy" />
      <div class="ck-tl-card" style=${{ '--tone': 'var(--burgundy)' }} onClick=${() => loadSession(s.session_id)}>
        <div style=${{ padding: '10px 14px 11px' }}>
          <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style=${{ color: 'var(--burgundy)', display: 'inline-flex' }}><${Icon} name="mic" size=${14} /></span>
            <span style=${{ fontSize: 14.5, fontWeight: 500, color: 'var(--ink)' }}>${s.title || 'Untitled session'}</span>
          </div>
          <div style=${{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--ink-faint)', marginTop: 3 }}>
            ${s.date || 'no date'} · session ${String(s.session_number || 0).padStart(2, '0')}
          </div>
        </div>
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
  useEffect(() => { if (store.dirty_vault) reload(); }, [store.dirty_vault]);

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
          ? html`<${WorldLane} events=${data.events || []} onAdd=${addEvent} cid=${c.campaign_id} />`
          : html`<${SessionLane} sessions=${sessions} />`}
    </div>
  </${Shell}>`;
}
