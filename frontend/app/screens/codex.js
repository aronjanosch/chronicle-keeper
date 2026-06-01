// Codex — the inspector into what the summarizer remembers about a campaign.
// Overview (Phase 3): a kind rail + searchable card grid. Each card is one entry
// fed to the LLM as a one-liner; click through to the entry-detail inspector.
// Keeps the Phase 1 freeform paste box (campaign-wide note, injected verbatim).
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, openModal, useStore } from '../core.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Btn, Empty, Icon, Markdown, Input, Textarea, Select } from '../ui.js';
import { loadCodexEntries, createCodexEntry, openCampaign, updateCampaign,
  loadCampaignTags, renameCampaignTag, deleteCampaignTag } from '../actions.js';

export const KINDS = [
  { value: 'pc',      label: 'PC',      plural: 'PCs',      tone: 'gilt' },
  { value: 'npc',     label: 'NPC',     plural: 'NPCs',     tone: 'burgundy' },
  { value: 'place',   label: 'Place',   plural: 'Places',   tone: 'moss' },
  { value: 'faction', label: 'Faction', plural: 'Factions', tone: 'ink-blue' },
  { value: 'item',    label: 'Item',    plural: 'Items',    tone: 'ochre' },
  { value: 'lore',    label: 'Lore',    plural: 'Lore',     tone: 'gilt' },
];

export function iconForKind(k) {
  return { pc: 'sparkle', npc: 'users', place: 'map', faction: 'shield', item: 'gem', lore: 'scroll' }[k] || 'doc';
}
function toneForKind(k) {
  return (KINDS.find((x) => x.value === k) || {}).tone || 'burgundy';
}

export function SourceBadge({ source }) {
  const tone = source === 'manual'
    ? { bg: 'var(--moss-50)', col: 'var(--moss)', border: 'rgba(74,93,58,.22)' }
    : source === 'auto'
      ? { bg: 'var(--ink-blue-50)', col: 'var(--ink-blue)', border: 'rgba(53,83,112,.22)' }
      : { bg: 'var(--ochre-50)', col: 'var(--ochre)', border: 'rgba(168,115,40,.24)' };
  return html`<span style=${{
    display: 'inline-flex', alignItems: 'center', padding: '1px 7px', borderRadius: 999,
    fontSize: 10.5, fontWeight: 500, letterSpacing: '0.04em', textTransform: 'uppercase',
    background: tone.bg, color: tone.col, border: `1px solid ${tone.border}`,
  }}>${source || 'manual'}</span>`;
}

// One card in the overview grid. Clickable → entry detail.
function CodexCard({ entry, onOpen }) {
  const tone = toneForKind(entry.kind);
  const col = tone === 'ink-blue' ? 'var(--ink-blue)' : `var(--${tone})`;
  return html`<div onClick=${onOpen} style=${{
    background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 6,
    padding: 14, display: 'flex', flexDirection: 'column', gap: 10, cursor: 'pointer',
  }}
    onMouseEnter=${(e) => { e.currentTarget.style.borderColor = 'var(--rule-strong)'; }}
    onMouseLeave=${(e) => { e.currentTarget.style.borderColor = 'var(--rule)'; }}>
    <div style=${{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
      <div style=${{
        width: 32, height: 32, borderRadius: 6, flex: '0 0 auto',
        background: `var(--${tone}-50)`, color: col,
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        border: '1px solid rgba(0,0,0,.06)',
      }}>
        <${Icon} name=${iconForKind(entry.kind)} size=${14} />
      </div>
      <div style=${{ flex: 1, minWidth: 0 }}>
        <div style=${{ fontFamily: 'var(--font-display)', fontSize: 14.5, fontWeight: 500, color: 'var(--ink)', lineHeight: 1.2 }}>${entry.name}</div>
        <div style=${{ fontSize: 11, color: 'var(--ink-muted)', marginTop: 2, textTransform: 'capitalize' }}>${entry.kind}</div>
      </div>
    </div>
    ${entry.body && html`<div style=${{ fontSize: 12.5, color: 'var(--ink-soft)', lineHeight: 1.45, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>${entry.body}</div>`}
    <div style=${{ height: 1, background: 'var(--rule-soft)' }} />
    <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <${SourceBadge} source=${entry.source} />
      <span style=${{ flex: 1 }} />
      ${entry.detail && entry.detail.trim() && html`<span style=${{ fontSize: 11, color: 'var(--ink-faint)', display: 'flex', alignItems: 'center', gap: 4 }}>
        <${Icon} name="feather" size=${10} /> detail
      </span>`}
    </div>
  </div>`;
}

// Inline add form (also reused by the entry-detail editor).
export function EntryForm({ initial, onSubmit, onCancel, withDetail }) {
  const [name, setName] = useState(initial?.name || '');
  const [kind, setKind] = useState(initial?.kind || 'npc');
  const [body, setBody] = useState(initial?.body || '');
  const [detail, setDetail] = useState(initial?.detail || '');
  const [err, setErr] = useState(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    if (!name.trim()) { setErr('Name is required'); return; }
    setBusy(true); setErr(null);
    try {
      await onSubmit({ name: name.trim(), kind, body: body.trim(), detail: detail.trim() });
    } catch (e) { setErr(e.message); setBusy(false); }
  }

  return html`<div style=${{ background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 6, padding: 12, display: 'flex', flexDirection: 'column', gap: 8 }}>
    <div style=${{ display: 'grid', gridTemplateColumns: '2fr 1fr', gap: 8 }}>
      <${Input} value=${name} onInput=${setName} placeholder="Name (e.g. Aragorn)" />
      <${Select} value=${kind} onChange=${setKind} options=${KINDS.map((k) => ({ value: k.value, label: k.label }))} />
    </div>
    <${Textarea} value=${body} onInput=${setBody} placeholder="One-line description — what the summarizer should know" rows=${2} />
    ${withDetail && html`<${Textarea} value=${detail} onInput=${setDetail} rows=${6}
      placeholder="Fuller detail (optional) — relationships, motives, secrets. Shown in the inspector; not fed to summaries." />`}
    ${err && html`<div style=${{ fontSize: 12, color: 'var(--burgundy-700)' }}>${err}</div>`}
    <div style=${{ display: 'flex', justifyContent: 'flex-end', gap: 6 }}>
      <${Btn} kind="ghost" size="sm" onClick=${onCancel}>Cancel</${Btn}>
      <${Btn} kind="primary" size="sm" icon="check" disabled=${busy} onClick=${submit}>Save</${Btn}>
    </div>
  </div>`;
}

// Segmented all/manual/auto filter. Counts so the GM can see at a glance how
// many entries the summarizer auto-pulled vs. ones they curated by hand.
function SourceFilter({ value, onChange, counts }) {
  const opts = [
    { v: 'all', label: 'All' },
    { v: 'manual', label: 'Manual' },
    { v: 'auto', label: 'Auto' },
  ];
  return html`<div style=${{ display: 'inline-flex', border: '1px solid var(--rule)', borderRadius: 5, overflow: 'hidden' }}>
    ${opts.map((o, i) => {
      const active = value === o.v;
      return html`<button key=${o.v} onClick=${() => onChange(o.v)} style=${{
        font: 'inherit', fontSize: 12, cursor: 'pointer', padding: '4px 10px',
        border: 'none', borderLeft: i ? '1px solid var(--rule)' : 'none',
        background: active ? 'var(--burgundy-50)' : 'var(--surface)',
        color: active ? 'var(--burgundy)' : 'var(--ink-soft)',
        fontWeight: active ? 500 : 400,
      }}>
        ${o.label}<span style=${{ marginLeft: 5, fontFamily: 'var(--font-mono)', fontSize: 10.5, color: active ? 'var(--burgundy)' : 'var(--ink-faint)' }}>${counts[o.v]}</span>
      </button>`;
    })}
  </div>`;
}

function KindRail({ entries, selected, onSelect, notesCount, tagsCount }) {
  const counts = { all: entries.length };
  for (const k of KINDS) counts[k.value] = entries.filter((e) => e.kind === k.value).length;
  const Row = ({ value, icon, label, n }) => {
    const active = selected === value;
    return html`<div onClick=${() => onSelect(value)} style=${{
      display: 'flex', alignItems: 'center', gap: 9, padding: '7px 9px', borderRadius: 4,
      color: active ? 'var(--ink)' : 'var(--ink-soft)',
      background: active ? 'var(--surface)' : 'transparent',
      border: active ? '1px solid var(--rule-soft)' : '1px solid transparent',
      fontSize: 13, fontWeight: active ? 500 : 400, cursor: 'pointer',
    }}>
      <${Icon} name=${icon} size=${13} />
      <span style=${{ flex: 1 }}>${label}</span>
      <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11, color: active ? 'var(--burgundy)' : 'var(--ink-faint)' }}>${n}</span>
    </div>`;
  };
  return html`<aside style=${{ padding: '20px 14px', borderRight: '1px solid var(--rule-soft)', display: 'flex', flexDirection: 'column', gap: 1 }}>
    <div style=${{ padding: '0 8px 8px', fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>What the LLM remembers</div>
    <${Row} value="all" icon="book" label="All entries" n=${counts.all} />
    ${KINDS.map((k) => html`<${Row} key=${k.value} value=${k.value} icon=${iconForKind(k.value)} label=${k.plural} n=${counts[k.value]} />`)}
    <div style=${{ height: 1, background: 'var(--rule-soft)', margin: '8px 8px' }} />
    <div style=${{ padding: '0 8px 8px', fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>Campaign-wide</div>
    <${Row} value="notes" icon="feather" label="Freeform notes" n=${notesCount} />
    <${Row} value="tags" icon="tag" label="Tags" n=${tagsCount} />
  </aside>`;
}

// The effective notes list: stored codex_notes, else the legacy single codex
// string surfaced as one note (migrated on first save).
function effectiveNotes(campaign) {
  const raw = Array.isArray(campaign.codex_notes) ? campaign.codex_notes : [];
  if (raw.length) return raw;
  const legacy = (campaign.codex || '').trim();
  return legacy ? [{ title: '', body: legacy }] : [];
}

function NoteEditor({ draft, onChange, onSave, onCancel, saving }) {
  return html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 8, background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 8, padding: 12 }}>
    <${Input} value=${draft.title} onInput=${(v) => onChange({ ...draft, title: v })} placeholder="Title (optional) — e.g. Tone, House rules, Setting" />
    <${Textarea} value=${draft.body} onInput=${(v) => onChange({ ...draft, body: v })} rows=${6}
      placeholder="Anything the model should know for every summary of this campaign." />
    <div style=${{ display: 'flex', justifyContent: 'flex-end', gap: 6 }}>
      <${Btn} kind="ghost" size="sm" disabled=${saving} onClick=${onCancel}>Cancel</${Btn}>
      <${Btn} kind="primary" size="sm" icon="check" disabled=${saving || !draft.body.trim()} onClick=${onSave}>Save</${Btn}>
    </div>
  </div>`;
}

function NotesSection({ campaign, standalone }) {
  const notes = effectiveNotes(campaign);
  const [editIdx, setEditIdx] = useState(null); // index being edited, or 'new'
  const [draft, setDraft] = useState({ title: '', body: '' });
  const [saving, setSaving] = useState(false);

  async function persist(next) {
    setSaving(true);
    try { await updateCampaign({ codex_notes: next }); setEditIdx(null); }
    finally { setSaving(false); }
  }
  const saveEdit = (i) => persist(notes.map((n, j) => (j === i ? draft : n)));
  const saveNew = () => persist([...notes, draft]);
  const remove = (i) => persist(notes.filter((_, j) => j !== i));

  const wrapStyle = standalone ? {} : { marginTop: 28, paddingTop: 20, borderTop: '1px solid var(--rule)' };
  return html`<div style=${wrapStyle}>
    ${standalone && html`<p style=${{ fontSize: 12.5, color: 'var(--ink-muted)', margin: '0 0 18px', lineHeight: 1.5, maxWidth: 640, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>
      Campaign-wide context — tone, setting brief, house rules. Unlike entries, every note is passed verbatim
      into every session summary, not just when a name comes up.
    </p>`}
    <div style=${{ display: 'flex', alignItems: 'baseline', gap: 8, marginBottom: 10 }}>
      <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500, margin: 0 }}>Freeform notes</h3>
      <span style=${{ fontSize: 11.5, color: 'var(--ink-faint)' }}>every note is passed verbatim into every summary</span>
      <span style=${{ flex: 1 }} />
      ${editIdx === null && html`<${Btn} kind="ghost" size="sm" icon="plus" onClick=${() => { setDraft({ title: '', body: '' }); setEditIdx('new'); }}>Add note</${Btn}>`}
    </div>

    <div style=${{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      ${notes.map((n, i) => editIdx === i
        ? html`<${NoteEditor} key=${`e${i}`} draft=${draft} onChange=${setDraft} saving=${saving}
            onSave=${() => saveEdit(i)} onCancel=${() => setEditIdx(null)} />`
        : html`<div key=${i} style=${{ background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 8, padding: '14px 18px' }}>
            <div style=${{ display: 'flex', alignItems: 'baseline', gap: 8, marginBottom: n.title ? 6 : 0 }}>
              ${n.title && html`<h4 style=${{ fontFamily: 'var(--font-display)', fontSize: 14, fontWeight: 500, margin: 0 }}>${n.title}</h4>`}
              <span style=${{ flex: 1 }} />
              ${editIdx === null && html`<div style=${{ display: 'flex', gap: 4 }}>
                <${Btn} kind="ghost" size="sm" icon="edit" onClick=${() => { setDraft({ title: n.title || '', body: n.body || '' }); setEditIdx(i); }}>Edit</${Btn}>
                <${Btn} kind="ghost" size="sm" icon="trash" onClick=${() => remove(i)}>Delete</${Btn}>
              </div>`}
            </div>
            <${Markdown} text=${n.body} />
          </div>`)}

      ${editIdx === 'new' && html`<${NoteEditor} draft=${draft} onChange=${setDraft} saving=${saving}
        onSave=${saveNew} onCancel=${() => setEditIdx(null)} />`}

      ${notes.length === 0 && editIdx === null && html`<div style=${{ fontSize: 12.5, color: 'var(--ink-faint)', fontStyle: 'italic', padding: '4px 0' }}>
        No notes yet — add a setting brief, tone, or house rules the model should know for every summary.
      </div>`}
    </div>
  </div>`;
}

// One tag row: name, usage count, rename (which merges if the target exists),
// and delete-across-all-sessions.
function TagRow({ t, onRename, onDelete }) {
  const [editing, setEditing] = useState(false);
  const [val, setVal] = useState(t.tag);
  const [busy, setBusy] = useState(false);

  async function save() {
    const to = val.trim();
    if (!to || to === t.tag) { setEditing(false); setVal(t.tag); return; }
    setBusy(true);
    try { await onRename(t.tag, to); } catch (e) { console.warn(e); }
    setBusy(false); setEditing(false);
  }

  return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 12px', background: 'var(--surface-raised)', border: '1px solid var(--rule)', borderRadius: 6 }}>
    <${Icon} name="tag" size=${12} className="ck-ink-faint" />
    ${editing
      ? html`<${Input} value=${val} onInput=${setVal} style=${{ flex: 1 }}
          onKeyDown=${(e) => { if (e.key === 'Enter') save(); if (e.key === 'Escape') { setEditing(false); setVal(t.tag); } }} />`
      : html`<span style=${{ flex: 1, fontFamily: 'var(--font-mono)', fontSize: 13, color: 'var(--ink)' }}>${t.tag}</span>`}
    <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--ink-faint)' }}>${t.count}×</span>
    ${editing
      ? html`<div style=${{ display: 'flex', gap: 4 }}>
          <${Btn} kind="primary" size="sm" icon="check" disabled=${busy} onClick=${save}>Save</${Btn}>
          <${Btn} kind="ghost" size="sm" onClick=${() => { setEditing(false); setVal(t.tag); }}>Cancel</${Btn}>
        </div>`
      : html`<div style=${{ display: 'flex', gap: 4 }}>
          <${Btn} kind="ghost" size="sm" icon="edit" onClick=${() => setEditing(true)}>Rename</${Btn}>
          <${Btn} kind="ghost" size="sm" icon="trash" onClick=${() => onDelete(t.tag)}>Delete</${Btn}>
        </div>`}
  </div>`;
}

function TagsSection() {
  const store = useStore();
  const tags = store.campaignTags || [];

  async function onDelete(tag) {
    if (!window.confirm(`Remove "${tag}" from every session in this campaign?`)) return;
    try { await deleteCampaignTag(tag); } catch (e) { console.warn(e); }
  }
  async function onRename(from, to) { await renameCampaignTag(from, to); }

  return html`<div>
    <p style=${{ fontSize: 12.5, color: 'var(--ink-muted)', margin: '0 0 18px', lineHeight: 1.5, maxWidth: 640, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>
      The campaign's tag vocabulary, pulled from every session's metadata. Rename to fix or merge a tag
      across all sessions at once (rename onto an existing tag to merge them); delete to drop one everywhere.
      New summaries reuse this set, so keeping it tidy keeps future tags consistent.
    </p>
    <div style=${{ display: 'flex', alignItems: 'baseline', gap: 8, marginBottom: 10 }}>
      <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500, margin: 0 }}>Tags</h3>
      <span style=${{ fontSize: 11.5, color: 'var(--ink-faint)' }}>${tags.length} in use</span>
    </div>
    ${tags.length === 0
      ? html`<div style=${{ fontSize: 12.5, color: 'var(--ink-faint)', fontStyle: 'italic', padding: '4px 0' }}>
          No tags yet — they appear once you summarize a session.
        </div>`
      : html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 8, maxWidth: 560 }}>
          ${tags.map((t) => html`<${TagRow} key=${t.tag} t=${t} onRename=${onRename} onDelete=${onDelete} />`)}
        </div>`}
  </div>`;
}

export function CodexScreen() {
  const store = useStore();
  const c = store.campaign;
  const [adding, setAdding] = useState(false);
  const [selectedKind, setSelectedKind] = useState('all');
  const [query, setQuery] = useState('');
  const [source, setSource] = useState('all'); // all | manual | auto

  useEffect(() => {
    if (c?.campaign_id) { loadCodexEntries(c.campaign_id); loadCampaignTags(c.campaign_id); }
  }, [c?.campaign_id]);

  if (!c) { navigate('library'); return null; }

  const entries = store.codexEntries || [];
  const notesCount = effectiveNotes(c).length;
  const tagsCount = (store.campaignTags || []).length;
  const showingNotes = selectedKind === 'notes';
  const showingTags = selectedKind === 'tags';
  const q = query.trim().toLowerCase();
  // 'manual' is the historical default for rows with no source set.
  const sourceOf = (e) => e.source || 'manual';
  // Counts for the source toggle reflect the active kind, but not the source
  // filter itself (so each segment shows its own total).
  const inKind = entries.filter((e) => selectedKind === 'all' || e.kind === selectedKind);
  const sourceCounts = {
    all: inKind.length,
    manual: inKind.filter((e) => sourceOf(e) === 'manual').length,
    auto: inKind.filter((e) => sourceOf(e) === 'auto').length,
  };
  const filtered = entries.filter((e) => {
    if (selectedKind !== 'all' && e.kind !== selectedKind) return false;
    if (source !== 'all' && sourceOf(e) !== source) return false;
    if (!q) return true;
    return e.name.toLowerCase().includes(q) || (e.body || '').toLowerCase().includes(q);
  });

  const sidebar = html`<${Sidebar} variant="campaign" active="codex" campaign=${c} />`;
  const topbar = html`<${Topbar} crumbs=${[
    { label: 'Campaigns', onClick: () => navigate('library') },
    { label: c.name, onClick: () => openCampaign(c.campaign_id) },
    'Codex',
  ]}
    right=${(showingNotes || showingTags)
      ? html`<${Btn} kind="ghost" size="sm" icon="compass" onClick=${() => setSelectedKind('all')}>Back to entries</${Btn}>`
      : html`<div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
      <${SourceFilter} value=${source} onChange=${setSource} counts=${sourceCounts} />
      <${Input} value=${query} onInput=${setQuery} placeholder="Search the codex…" style=${{ width: 220 }} />
      <${Btn} kind="ghost" size="sm" icon="sparkle" onClick=${() => openModal('codexImport')}>Import</${Btn}>
      <${Btn} kind="primary" size="sm" icon="plus" onClick=${() => setAdding(true)}>Add entry</${Btn}>
    </div>`} />`;

  async function onCreate(payload) {
    await createCodexEntry(payload);
    setAdding(false);
  }

  return html`<${Shell} sidebar=${sidebar} topbar=${topbar} bodyStyle=${{ padding: 0 }}>
    <div style=${{ display: 'grid', gridTemplateColumns: '220px 1fr', height: '100%' }}>
      <${KindRail} entries=${entries} selected=${selectedKind} onSelect=${setSelectedKind} notesCount=${notesCount} tagsCount=${tagsCount} />

      <div style=${{ padding: '20px 24px', overflow: 'auto' }}>
        ${showingNotes
          ? html`<${NotesSection} campaign=${c} standalone=${true} />`
          : showingTags
          ? html`<${TagsSection} />`
          : html`
        <p style=${{ fontSize: 12.5, color: 'var(--ink-muted)', margin: '0 0 18px', lineHeight: 1.5, maxWidth: 640, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>
          What the summarizer remembers about ${c.name}. Each entry's one-liner is fed to the LLM when it
          summarises a session; open an entry to see (and edit) the fuller detail. Auto entries come from past
          sessions — edit one and it's marked manual so future runs won't overwrite it.
        </p>

        <div style=${{ display: 'flex', alignItems: 'baseline', gap: 8, marginBottom: 10 }}>
          <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500, margin: 0 }}>Entries</h3>
          <span style=${{ fontSize: 11.5, color: 'var(--ink-faint)' }}>
            ${entries.length === 0 ? 'nothing yet' : `${filtered.length} of ${entries.length}`}
          </span>
        </div>

        ${adding && html`<div style=${{ marginBottom: 14 }}>
          <${EntryForm} onSubmit=${onCreate} onCancel=${() => setAdding(false)} withDetail=${true} />
        </div>`}

        ${entries.length === 0 && !adding
          ? html`<${Empty} icon="book" title="No entries yet">
              Add an NPC, place, or item, or run a summary — the summarizer pulls names from your sessions automatically.
            </${Empty}>`
          : filtered.length === 0
            ? html`<div style=${{ fontSize: 12.5, color: 'var(--ink-faint)', fontStyle: 'italic', padding: '12px 0' }}>
                No entries match.
              </div>`
            : KINDS
                .map((k) => ({ kind: k, items: filtered.filter((e) => e.kind === k.value) }))
                .filter((g) => g.items.length)
                .map((g) => html`<div key=${g.kind.value} style=${{ marginBottom: 22 }}>
                  <div style=${{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
                    <${Icon} name=${iconForKind(g.kind.value)} size=${13} className="ck-ink-muted" />
                    <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 14, fontWeight: 500, margin: 0 }}>${g.kind.plural}</h3>
                    <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--ink-faint)' }}>${g.items.length}</span>
                  </div>
                  <div style=${{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 10 }}>
                    ${g.items.map((e) => html`<${CodexCard} key=${e.entry_id} entry=${e}
                      onOpen=${() => navigate('codexEntry', { entryId: e.entry_id })} />`)}
                  </div>
                </div>`)}`}
      </div>
    </div>
  </${Shell}>`;
}
