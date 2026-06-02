// Screen 04 — Session Detail. Pipeline strip, summary prose, speakers, metadata.
import { html, useState } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, openModal, fmtDate, fmtDateTime, toneFor } from '../core.js';
import { deleteArtifact, artifactContent, deleteSession, openCampaign, runTranscribe, saveSessionMetadata, loadSession } from '../actions.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Sigil, Btn, Pipeline, Markdown, Empty } from '../ui.js';

function SpeakerChip({ s }) {
  const ch = s.character_name || s.player_name || `track ${s.track_id}`;
  const isGM = /game ?master|gm/i.test(ch);
  return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 12px', background: 'var(--surface)', border: '1px solid var(--rule-soft)', borderRadius: 6 }}>
    <${Sigil} ch=${isGM ? 'GM' : (ch[0] || '?').toUpperCase()} tone=${isGM ? 'ink' : toneFor(s.player_name || ch)} />
    <div style=${{ flex: 1, minWidth: 0 }}>
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 13.5, fontWeight: 500, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${ch}</div>
      <div style=${{ fontSize: 11, color: 'var(--ink-muted)', marginTop: 1 }}>
        ${s.player_name || '—'}${s.pronouns ? html` <span style=${{ color: 'var(--ink-ghost)' }}> · ${s.pronouns}</span>` : ''}
      </div>
    </div>
    <div style=${{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--ink-faint)' }}>track-${s.track_id}</div>
  </div>`;
}

function ChipRow({ ic, tone, label, items }) {
  if (!items.length) return null;
  return html`<div style=${{ display: 'flex', alignItems: 'flex-start', gap: 12, padding: '10px 0', borderBottom: '1px solid var(--rule-soft)' }}>
    <div style=${{ width: 24, height: 24, borderRadius: 4, flex: '0 0 auto', background: `var(--${tone}-50)`, color: tone === 'ink-blue' ? 'var(--ink-blue)' : `var(--${tone})`, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
      <${Icon} name=${ic} size=${12} />
    </div>
    <div style=${{ width: 90, fontSize: 11.5, fontWeight: 600, color: 'var(--ink-muted)', letterSpacing: '0.06em', textTransform: 'uppercase', paddingTop: 4 }}>${label}</div>
    <div style=${{ flex: 1, display: 'flex', flexWrap: 'wrap', gap: 4 }}>
      ${items.map((it, i) => html`<span key=${i} style=${{ padding: '3px 8px', background: 'var(--paper-deep)', color: 'var(--ink-soft)', border: '1px solid var(--rule-soft)', borderRadius: 4, fontSize: 11.5 }}>${it}</span>`)}
    </div>
  </div>`;
}

// The five metadata lists shown (and now edited) in the "What happened" card.
const META_CATS = [
  { key: 'characters', ic: 'users', tone: 'burgundy', label: 'NPCs' },
  { key: 'locations', ic: 'map', tone: 'moss', label: 'Places' },
  { key: 'items', ic: 'gem', tone: 'ochre', label: 'Items' },
  { key: 'events', ic: 'flame', tone: 'ochre', label: 'Events' },
  { key: 'tags', ic: 'tag', tone: 'ink-blue', label: 'Tags', mono: true },
];

// One editable row: existing items as removable chips + an input that commits
// on Enter, comma, or blur. Comma-pasting several at once splits them.
function TagEditor({ ic, tone, label, items, mono, onChange }) {
  const [draft, setDraft] = useState('');
  const commit = (raw) => {
    const adds = raw.split(',').map((x) => x.trim()).filter((x) => x && !items.includes(x));
    if (adds.length) onChange([...items, ...adds]);
    setDraft('');
  };
  const onKey = (e) => {
    if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); commit(draft); }
    else if (e.key === 'Backspace' && !draft && items.length) { onChange(items.slice(0, -1)); }
  };
  const chip = (it, i) => html`<span key=${i} style=${{ display: 'inline-flex', alignItems: 'center', gap: 4, padding: mono ? '2px 4px 2px 7px' : '3px 4px 3px 8px', background: 'var(--paper-deep)', color: 'var(--ink-soft)', border: '1px solid var(--rule-soft)', borderRadius: mono ? 3 : 4, fontSize: 11.5, fontFamily: mono ? 'var(--font-mono)' : 'inherit' }}>
    ${mono ? '#' : ''}${it}
    <button type="button" onClick=${() => onChange(items.filter((_, j) => j !== i))} title="Remove"
      style=${{ display: 'flex', alignItems: 'center', justifyContent: 'center', width: 14, height: 14, padding: 0, border: 'none', background: 'transparent', color: 'var(--ink-faint)', cursor: 'pointer', borderRadius: 3 }}>
      <${Icon} name="x" size=${9} />
    </button>
  </span>`;
  return html`<div style=${{ display: 'flex', alignItems: 'flex-start', gap: 12, padding: '10px 0', borderBottom: '1px solid var(--rule-soft)' }}>
    <div style=${{ width: 24, height: 24, borderRadius: 4, flex: '0 0 auto', background: `var(--${tone}-50)`, color: tone === 'ink-blue' ? 'var(--ink-blue)' : `var(--${tone})`, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
      <${Icon} name=${ic} size=${12} />
    </div>
    <div style=${{ width: 90, fontSize: 11.5, fontWeight: 600, color: 'var(--ink-muted)', letterSpacing: '0.06em', textTransform: 'uppercase', paddingTop: 6 }}>${label}</div>
    <div style=${{ flex: 1, display: 'flex', flexWrap: 'wrap', gap: 4, alignItems: 'center' }}>
      ${items.map(chip)}
      <input value=${draft} onInput=${(e) => setDraft(e.target.value)} onKeyDown=${onKey} onBlur=${() => commit(draft)}
        placeholder=${items.length ? 'Add…' : `Add ${label.toLowerCase()}…`}
        style=${{ flex: '1 0 80px', minWidth: 80, padding: '3px 4px', border: 'none', background: 'transparent', fontSize: 11.5, fontFamily: mono ? 'var(--font-mono)' : 'inherit', color: 'var(--ink)', outline: 'none' }} />
    </div>
  </div>`;
}

// "What happened" — read-only chip view with an inline editor toggled in place.
// Edits write the whole metadata object; the backend replaces it wholesale, so
// this is the single source of truth (the session modal no longer touches it).
function WhatHappened({ sess }) {
  const cam = sess.campaign || {};
  const md = sess.metadata || {};
  const [draft, setDraft] = useState(null); // null = view mode
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);

  const start = () => { setErr(null); setDraft(Object.fromEntries(META_CATS.map((c) => [c.key, [...(md[c.key] || [])]]))); };
  const cancel = () => { setDraft(null); setErr(null); };
  const setCat = (key, list) => setDraft((d) => ({ ...d, [key]: list }));
  async function save() {
    setBusy(true); setErr(null);
    try {
      await saveSessionMetadata({
        session_id: sess.session_id, campaign_id: cam.campaign_id || null,
        session_number: cam.session_number || null, title: cam.title || null, date: cam.date || null,
        metadata: draft, notes: cam.notes || null,
      });
      await loadSession(sess.session_id);
      setDraft(null);
    } catch (e) { setErr(e.message); }
    setBusy(false);
  }

  const editing = draft !== null;
  const hasAny = META_CATS.some((c) => (md[c.key] || []).length);

  return html`<div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden' }}>
    <div style=${{ padding: '12px 16px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 8 }}>
      <${Icon} name="tag" size=${13} className="ck-ink-muted" />
      <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500 }}>What happened</h3>
      <span style=${{ flex: 1 }} />
      ${editing
        ? html`<${Btn} kind="ghost" size="sm" disabled=${busy} onClick=${cancel}>Cancel</${Btn}>
            <${Btn} kind="primary" size="sm" disabled=${busy} onClick=${save}>${busy ? 'Saving…' : 'Save'}</${Btn}>`
        : html`<${Btn} kind="ghost" size="sm" icon="edit" onClick=${start}>Edit</${Btn}>`}
    </div>
    <div style=${{ padding: '4px 16px 12px' }}>
      ${err && html`<div style=${{ color: 'var(--burgundy-700)', fontSize: 12.5, padding: '8px 0' }}>${err}</div>`}
      ${editing
        ? META_CATS.map((c) => html`<${TagEditor} key=${c.key} ic=${c.ic} tone=${c.tone} label=${c.label} mono=${c.mono} items=${draft[c.key]} onChange=${(list) => setCat(c.key, list)} />`)
        : hasAny
          ? html`
            <${ChipRow} ic="users" tone="burgundy" label="NPCs" items=${md.characters || []} />
            <${ChipRow} ic="map" tone="moss" label="Places" items=${md.locations || []} />
            <${ChipRow} ic="gem" tone="ochre" label="Items" items=${md.items || []} />
            <${ChipRow} ic="flame" tone="ochre" label="Events" items=${md.events || []} />
            ${(md.tags?.length) ? html`<div style=${{ display: 'flex', alignItems: 'flex-start', gap: 12, padding: '10px 0' }}>
              <div style=${{ width: 24, flex: '0 0 auto' }} />
              <div style=${{ width: 90, fontSize: 11.5, fontWeight: 600, color: 'var(--ink-muted)', letterSpacing: '0.06em', textTransform: 'uppercase', paddingTop: 4 }}>Tags</div>
              <div style=${{ flex: 1, display: 'flex', flexWrap: 'wrap', gap: 4 }}>
                ${md.tags.map((t, i) => html`<span key=${i} style=${{ padding: '3px 7px', borderRadius: 3, background: 'var(--paper-deep)', fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--ink-muted)' }}>#${t}</span>`)}
              </div>
            </div>` : ''}`
          : html`<div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic', padding: '8px 4px' }}>Nothing recorded yet — summarize, or add NPCs, places, items, events and tags by hand.</div>`}
    </div>
  </div>`;
}

function ArtifactList({ kind, items }) {
  if (!items.length) return null;
  const label = kind === 'transcripts' ? 'Transcripts' : 'Summaries';
  return html`<div style=${{ marginTop: 10 }}>
    <div style=${{ fontSize: 11, fontWeight: 600, letterSpacing: '0.08em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 6 }}>${label} · ${items.length}</div>
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      ${items.map((a) => html`<div key=${a.id} style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 10px', background: 'var(--paper)', border: '1px solid var(--rule-soft)', borderRadius: 6 }}>
        <${Icon} name=${kind === 'transcripts' ? 'doc' : 'feather'} size=${13} className="ck-ink-muted" />
        <div style=${{ flex: 1, minWidth: 0 }}>
          <div style=${{ fontSize: 12.5, color: 'var(--ink)' }}>${a.provider} / ${a.model}</div>
          <div style=${{ fontSize: 11, color: 'var(--ink-muted)', fontFamily: 'var(--font-mono)' }}>${fmtDateTime(a.created_at)}</div>
        </div>
        <${Btn} kind="ghost" size="sm" icon="eye" onClick=${async () => {
          try { openModal('viewer', { title: `${a.provider} / ${a.model}`, text: await artifactContent(kind, a.id) }); } catch (e) { openModal('viewer', { title: 'Error', text: e.message }); }
        }}>View</${Btn}>
        <${Btn} kind="danger" size="sm" icon="trash" onClick=${() => openModal('confirm', {
          title: `Delete ${kind === 'transcripts' ? 'transcript' : 'summary'}`,
          message: `Delete this ${kind === 'transcripts' ? 'transcript' : 'summary'}? This cannot be undone.`,
          onConfirm: () => deleteArtifact(kind, a.id),
        })} />
      </div>`)}
    </div>
  </div>`;
}

export function SessionScreen({ store }) {
  const sess = store.session;
  if (!sess) return html`<div />`;
  const c = store.campaign;
  const cam = sess.campaign || {};
  const tracks = sess.tracks || [];
  const speakers = sess.speakers || [];
  const hasT = store.transcripts.length > 0;
  const hasS = store.summaries.length > 0;

  const stages = [
    { key: 'u', label: 'Recording', done: tracks.length > 0, current: tracks.length === 0, detail: tracks.length ? `${tracks.length} track${tracks.length === 1 ? '' : 's'}` : 'No upload', meta: tracks.length ? '' : 'Upload a Craig ZIP' },
    { key: 't', label: 'Transcribed', done: hasT, current: tracks.length > 0 && !hasT, detail: hasT ? `${store.transcripts[0].provider} / ${store.transcripts[0].model}` : 'Pending', meta: hasT ? 'on-device' : '' },
    { key: 's', label: 'Summarized', done: hasS, current: hasT && !hasS, detail: hasS ? `${store.summaries[0].provider} / ${store.summaries[0].model}` : 'Pending', meta: hasS ? fmtDateTime(store.summaries[0].created_at) : '' },
  ];

  const primary = !tracks.length
    ? html`<${Btn} kind="primary" icon="upload" onClick=${() => navigate('newSession', { id: cam.campaign_id, attach: sess.session_id })}>Upload recording</${Btn}>`
    : !hasT
      ? html`<${Btn} kind="primary" icon="mic" onClick=${() => runTranscribe()}>Transcribe</${Btn}>`
      : html`<${Btn} kind="primary" icon="sparkle" onClick=${() => navigate('summarize', { id: sess.session_id })}>${hasS ? 'Re-summarize' : 'Summarize'}</${Btn}>`;

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="campaign" active="sessions" campaign=${c} />`}
    topbar=${html`<${Topbar} crumbs=${[
      { label: 'Campaigns', onClick: () => navigate('library') },
      c && { label: c.name, onClick: () => openCampaign(c.campaign_id) },
      `Session ${cam.session_number || '?'}`,
    ]} right=${html`
      <div style=${{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <${Btn} kind="ghost" icon="edit" onClick=${() => openModal('session', { session: sess })}>Edit</${Btn}>
        <${Btn} kind="danger" icon="trash" title="Delete session" onClick=${() => openModal('confirm', {
          title: 'Delete session',
          message: 'Delete this session? This removes its transcripts and summaries permanently.',
          onConfirm: () => deleteSession(sess.session_id),
        })} />
        ${tracks.length > 0 && !hasT ? html`<${Btn} kind="secondary" icon="users" onClick=${() => navigate('newSession', { id: cam.campaign_id, attach: sess.session_id })}>Label speakers</${Btn}>` : ''}
        <${Btn} kind="secondary" icon="export" disabled=${!hasS} onClick=${() => openModal('export', {})}>Export</${Btn}>
        ${primary}
      </div>`} />`}
  >
    <div style=${{ display: 'flex', alignItems: 'flex-start', gap: 20, marginBottom: 22 }}>
      <div style=${{ width: 64, height: 64, flex: '0 0 auto', background: 'var(--burgundy-50)', color: 'var(--burgundy-700)', borderRadius: 8, border: '1px solid rgba(122,46,31,.18)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontFamily: 'var(--font-display)' }}>
        <div style=${{ textAlign: 'center', lineHeight: 1 }}>
          <div style=${{ fontSize: 10, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase' }}>Session</div>
          <div style=${{ fontSize: 24, fontWeight: 500, marginTop: 4 }}>${cam.session_number || '?'}</div>
        </div>
      </div>
      <div style=${{ flex: 1 }}>
        <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.12em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>${c?.name || 'Chronicle'}</div>
        <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 28, fontWeight: 500, letterSpacing: '-0.015em', color: 'var(--ink)', lineHeight: 1.15, marginTop: 2 }}>
          ${cam.title || html`<span style=${{ fontStyle: 'italic', color: 'var(--ink-muted)' }}>Untitled session</span>`}
        </h1>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 14, marginTop: 8, fontSize: 12.5, color: 'var(--ink-muted)' }}>
          ${cam.date && html`<span style=${{ display: 'flex', alignItems: 'center', gap: 5 }}><${Icon} name="cal" size=${12} /> ${fmtDate(cam.date)}</span>`}
          <span style=${{ display: 'flex', alignItems: 'center', gap: 5 }}><${Icon} name="users" size=${12} /> ${speakers.length || tracks.length} speaker${(speakers.length || tracks.length) === 1 ? '' : 's'}</span>
          ${hasS && html`<span style=${{ display: 'flex', alignItems: 'center', gap: 5, color: 'var(--moss)' }}><span style=${{ width: 6, height: 6, borderRadius: '50%', background: 'var(--moss)' }} /> Complete · ready to export</span>`}
        </div>
      </div>
    </div>

    <${Pipeline} stages=${stages} />

    <div style=${{ display: 'grid', gridTemplateColumns: '1fr 360px', gap: 16, marginTop: 22 }}>
      <div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden' }}>
        <div style=${{ padding: '12px 18px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 10 }}>
          <${Icon} name="feather" size=${14} className="ck-ink-muted" />
          <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 16, fontWeight: 500, color: 'var(--ink)' }}>Summary</h3>
          ${hasS && html`<span style=${{ fontSize: 11, color: 'var(--ink-muted)', fontFamily: 'var(--font-mono)' }}>· ${store.summaries[0].provider} / ${store.summaries[0].model}</span>`}
          <span style=${{ flex: 1 }} />
        </div>
        <div style=${{ padding: '24px 28px' }}>
          ${store.summaryPreview
            ? html`<${Markdown} text=${store.summaryPreview.text} codex=${store.codexEntries} />`
            : html`<${Empty} icon="feather" title=${hasT ? 'Not summarized yet' : 'No transcript yet'}>
                ${hasT ? 'Generate a summary with your chosen LLM.' : 'Transcribe the recording, then summarize.'}
              </${Empty}>`}
        </div>
      </div>

      <div style=${{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden' }}>
          <div style=${{ padding: '12px 16px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center', gap: 8 }}>
            <${Icon} name="users" size=${13} className="ck-ink-muted" />
            <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500 }}>At the table</h3>
            <span style=${{ flex: 1 }} />
            <span style=${{ fontSize: 11, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)' }}>${speakers.length}/${tracks.length || speakers.length}</span>
          </div>
          <div style=${{ padding: 10, display: 'flex', flexDirection: 'column', gap: 6 }}>
            ${speakers.length
              ? speakers.map((s) => html`<${SpeakerChip} key=${s.track_id} s=${s} />`)
              : tracks.length
                ? html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'flex-start', padding: '8px 4px' }}>
                    <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic' }}>${tracks.length} track${tracks.length === 1 ? '' : 's'} uploaded, not labelled yet.</div>
                    <${Btn} kind="secondary" size="sm" icon="users" onClick=${() => navigate('newSession', { id: cam.campaign_id, attach: sess.session_id })}>Label speakers</${Btn}>
                  </div>`
                : html`<div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic', padding: '8px 4px' }}>No speakers labelled.</div>`}
          </div>
        </div>

        <${WhatHappened} sess=${sess} />

        <div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, padding: '12px 16px' }}>
          <${ArtifactList} kind="transcripts" items=${store.transcripts} />
          <${ArtifactList} kind="summaries" items=${store.summaries} />
          ${(!store.transcripts.length && !store.summaries.length) && html`<div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic' }}>No transcripts or summaries yet.</div>`}
        </div>
      </div>
    </div>
  </${Shell}>`;
}
