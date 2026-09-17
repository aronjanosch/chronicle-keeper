// Screen — Update the Codex (Phase 5). After a session summary, the AI proposes
// page edits as a reviewable diff: proposal list left, per-change diff right.
// Nothing writes until "Commit". Decisions persist server-side as you review.
import { html, useState, useEffect } from '../../vendor/htm-preact-standalone.mjs';
import { navigate } from '../core.js';
import { loadCodexUpdate, runCodexUpdate, saveCodexUpdateDecisions, commitCodexUpdate, openCampaign } from '../actions.js';
import { Shell, Sidebar, Topbar } from '../shell.js';
import { Icon, Btn, Spinner, Empty, Textarea } from '../ui.js';
import { KINDS, iconForKind } from './codex.js';

const CHIP = {
  new:     { label: 'New page',     tone: 'moss' },
  summary: { label: 'Summary',      tone: 'burgundy' },
  body:    { label: 'Body',         tone: 'ink-blue' },
  rel:     { label: 'Relationship', tone: 'ochre' },
};
function toneCol(tone) { return tone === 'ink-blue' ? 'var(--ink-blue)' : `var(--${tone})`; }

function ChangeChip({ kind }) {
  const c = CHIP[kind] || CHIP.body;
  return html`<span style=${{ padding: '1px 6px', borderRadius: 3, fontSize: 10, fontWeight: 600, letterSpacing: '0.02em', background: `var(--${c.tone}-50)`, color: toneCol(c.tone), border: '1px solid rgba(0,0,0,.04)' }}>${c.label}</span>`;
}

function glyphFor(kind, size = 16) {
  const tone = (KINDS.find((x) => x.value === kind) || {}).tone || 'burgundy';
  return html`<div style=${{
    width: size, height: size, borderRadius: Math.max(4, size / 6), flex: '0 0 auto',
    background: `var(--${tone}-50)`, color: toneCol(tone),
    display: 'flex', alignItems: 'center', justifyContent: 'center', border: '1px solid rgba(0,0,0,.06)',
  }}><${Icon} name=${iconForKind(kind)} size=${Math.round(size * 0.55)} /></div>`;
}

// Which chip kinds a proposal carries (for the row + header).
function chipsOf(p) {
  const set = [];
  for (const c of p.changes || []) {
    const k = c.type === 'new' ? 'new' : c.type;
    if (!set.includes(k)) set.push(k);
  }
  return set;
}

function ProposalRow({ p, selected, onSelect, onToggle }) {
  const accepted = p.decision === 'accepted' || p.decision === 'edited';
  return html`<div onClick=${onSelect} style=${{
    display: 'flex', alignItems: 'flex-start', gap: 10, padding: '11px 12px', borderRadius: 7, cursor: 'pointer',
    background: selected ? 'var(--surface)' : 'transparent',
    border: selected ? '1px solid var(--rule)' : '1px solid transparent',
    boxShadow: selected ? 'var(--shadow-soft)' : 'none',
    opacity: p.ungrounded && !accepted ? 0.65 : 1,
  }}>
    <span onClick=${(e) => { e.stopPropagation(); onToggle(); }} style=${{
      width: 16, height: 16, flex: '0 0 auto', marginTop: 1, borderRadius: 4,
      border: '1px solid var(--burgundy)', background: accepted ? 'var(--burgundy)' : 'transparent',
      display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#FBF6E9',
    }}>
      ${accepted && html`<${Icon} name="check" size=${10} style=${{ strokeWidth: 2.2 }} />`}
    </span>
    ${glyphFor(p.kind, 26)}
    <div style=${{ flex: 1, minWidth: 0 }}>
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 13.5, fontWeight: 500, color: 'var(--ink)', display: 'flex', alignItems: 'center', gap: 6 }}>
        ${p.title}
        ${p.ungrounded && html`<span title="Could not be verified against the transcript" style=${{ color: 'var(--ochre)', display: 'flex' }}><${Icon} name="flame" size=${11} /></span>`}
      </div>
      <div style=${{ display: 'flex', flexWrap: 'wrap', gap: 4, marginTop: 5 }}>
        ${chipsOf(p).map((c, i) => html`<${ChangeChip} key=${i} kind=${c} />`)}
      </div>
    </div>
  </div>`;
}

function DiffLine({ mode, children }) {
  const tone = mode === 'add' ? { bg: 'var(--moss-50)', col: 'var(--ink)', mark: '+', markCol: 'var(--moss)' }
    : mode === 'remove' ? { bg: 'rgba(122,46,31,.07)', col: 'var(--ink-muted)', mark: '−', markCol: 'var(--burgundy-700)' }
    : { bg: 'transparent', col: 'var(--ink-muted)', mark: ' ', markCol: 'var(--ink-ghost)' };
  return html`<div style=${{ display: 'flex', gap: 10, padding: '4px 12px', background: tone.bg, fontSize: 13, lineHeight: 1.5 }}>
    <span style=${{ fontFamily: 'var(--font-mono)', color: tone.markCol, flex: '0 0 auto', width: 10 }}>${tone.mark}</span>
    <span style=${{ color: tone.col, textDecoration: mode === 'remove' ? 'line-through' : 'none', textDecorationColor: 'rgba(122,46,31,.4)' }}>${children}</span>
  </div>`;
}

function DiffBlock({ label, children }) {
  return html`<div style=${{ marginBottom: 18 }}>
    <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 7 }}>${label}</div>
    <div style=${{ border: '1px solid var(--rule)', borderRadius: 6, overflow: 'hidden', background: 'var(--surface)', padding: '6px 0' }}>${children}</div>
  </div>`;
}

// One change rendered as a diff block, with inline edit-before-commit.
function ChangeBlock({ change, editing, draft, setDraft }) {
  const editable = (label, value, field) => editing
    ? html`<${DiffBlock} label=${label}>
        <div style=${{ padding: '4px 10px' }}>
          <${Textarea} rows=${3} value=${draft[field] ?? value} onInput=${(v) => setDraft({ ...draft, [field]: v })} />
        </div>
      </${DiffBlock}>`
    : null;
  switch (change.type) {
    case 'summary':
      return editable("Summary · the AI's memory", change.new, 'new') || html`<${DiffBlock} label="Summary · the AI's memory">
        ${change.old && html`<${DiffLine} mode="remove">${change.old}</${DiffLine}>`}
        <${DiffLine} mode="add">${change.new}</${DiffLine}>
      </${DiffBlock}>`;
    case 'body':
      return editable(`Body · ${change.anchor} (append)`, change.text, 'text') || html`<${DiffBlock} label=${`Body · ${change.anchor} (append)`}>
        <${DiffLine} mode="add">${change.text}</${DiffLine}>
      </${DiffBlock}>`;
    case 'rel':
      return html`<${DiffBlock} label="Infobox · relationships">
        <${DiffLine} mode="add">${change.field}: ${change.add}${change.note ? ` — ${change.note}` : ''}</${DiffLine}>
      </${DiffBlock}>`;
    case 'new':
      return html`<div>
        ${(editing || change.summary) && (editable('Summary · the AI\'s memory', change.summary, 'summary') || html`<${DiffBlock} label="Summary · the AI's memory"><${DiffLine} mode="add">${change.summary}</${DiffLine}></${DiffBlock}>`)}
        ${(editing || change.body) && (editable('Body', change.body, 'body') || html`<${DiffBlock} label="Body"><${DiffLine} mode="add">${change.body}</${DiffLine}></${DiffBlock}>`)}
      </div>`;
    default:
      return null;
  }
}

const STAGE_LABEL = {
  candidates: 'Reading the summary & drafting proposals…',
  grounding: 'Verifying against the transcript…',
};

// Legacy page entry (`codexUpdate` route) kept working for deep links and the
// standalone screen; SessionScreen's Review view is the primary host now.
export function CodexUpdateScreen({ store }) {
  const sess = store.session;
  const c = store.campaign;
  const cam = sess?.campaign || {};
  if (!sess) return html`<div />`;

  return html`<${Shell}
    sidebar=${html`<${Sidebar} variant="campaign" active="sessions" campaign=${c} />`}
    topbar=${html`<${Topbar} crumbs=${[
      { label: 'Worlds', onClick: () => navigate('library') },
      c && { label: c.name, onClick: () => openCampaign(c.campaign_id) },
      { label: `Session ${cam.session_number || '?'}`, onClick: () => navigate('session', { id: sess.session_id }) },
      'Update the Codex',
    ]} />`}
    bodyStyle=${{ padding: 0 }}
  >
    <${CodexUpdatePane} store=${store} autoGenerate=${true} />
  </${Shell}>`;
}

// The review body, shared by the standalone Codex screen and the session
// Review view. `autoGenerate` (default) keeps the standalone route's existing
// behavior; the session Review host passes false so opening the view never
// fires an unsolicited generation call.
export function CodexUpdatePane({ store, autoGenerate = false }) {
  const sess = store.session;
  const cam = sess?.campaign || {};
  const run = store.codexUpdate && store.codexUpdate.status !== 'none' ? store.codexUpdate : null;
  const streaming = store.codexUpdateStreaming;
  const proposals = run?.proposals || [];

  const [selectedId, setSelectedId] = useState(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState({});
  const [busy, setBusy] = useState(false);

  // Latest summary newer than the run → the run is stale; regenerate.
  const summaryAt = store.summaries[0]?.created_at;
  const runStale = run && summaryAt && new Date(summaryAt) > new Date(run.generated_at);

  useEffect(() => {
    if (!sess || !autoGenerate) return;
    (async () => {
      const r = await loadCodexUpdate(sess.session_id);
      const fresh = r && r.status !== 'none'
        && !(store.summaries[0]?.created_at && new Date(store.summaries[0].created_at) > new Date(r.generated_at));
      if (!fresh && !store.codexUpdateStreaming) runCodexUpdate();
    })();
  }, []);

  useEffect(() => {
    if (!selectedId && proposals.length) setSelectedId(proposals[0].id);
  }, [run]);

  if (!sess) return html`<div />`;

  const selected = proposals.find((p) => p.id === selectedId) || proposals[0] || null;
  const acceptedIds = proposals.filter((p) => p.decision === 'accepted' || p.decision === 'edited').map((p) => p.id);
  const newOnes = proposals.filter((p) => !p.page);
  const revised = proposals.filter((p) => p.page);

  const setDecision = (p, decision) => saveCodexUpdateDecisions({ proposals: [{ id: p.id, decision }] });
  const toggle = (p) => setDecision(p, (p.decision === 'accepted' || p.decision === 'edited') ? 'rejected' : 'accepted');

  async function saveEdit() {
    if (!selected) return;
    const changes = selected.changes.map((ch) => {
      const out = { ...ch };
      for (const k of ['new', 'text', 'summary', 'body']) {
        if (draft[k] !== undefined && k in ch) out[k] = draft[k];
      }
      return out;
    });
    await saveCodexUpdateDecisions({ proposals: [{ id: selected.id, decision: 'edited', changes }] });
    setEditing(false); setDraft({});
  }

  async function doCommit() {
    if (!acceptedIds.length) return;
    setBusy(true);
    try { await commitCodexUpdate(acceptedIds); navigate('session', { id: sess.session_id }); }
    catch (_) {}
    setBusy(false);
  }

  async function skip() {
    await saveCodexUpdateDecisions({ status: 'skipped' }).catch(() => {});
    navigate('session', { id: sess.session_id });
  }

  const sectionHead = (label) => html`<div style=${{ fontSize: 10, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', padding: '12px 4px 4px' }}>${label}</div>`;

  // One primary per state: generate when there is no run yet, commit once
  // proposals are selected. Opening Review never auto-generates (SC-02).
  const actions = html`<div style=${{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
    ${!run && !streaming && html`<${Btn} kind="primary" size="sm" icon="sparkle" onClick=${() => runCodexUpdate()}>Find world updates</${Btn}>`}
    ${run && !streaming && html`<${Btn} kind="ghost" size="sm" icon="sparkle" onClick=${() => runCodexUpdate()}>Regenerate</${Btn}>`}
    ${run && !streaming && html`<${Btn} kind="ghost" size="sm" onClick=${skip}>Skip for now</${Btn}>`}
    ${run && !streaming && html`<${Btn} kind="primary" size="sm" icon="check" disabled=${busy || !acceptedIds.length} onClick=${doCommit}>
      ${busy ? 'Committing…' : `Commit ${acceptedIds.length} change${acceptedIds.length === 1 ? '' : 's'}`}
    </${Btn}>`}
  </div>`;

  // Hosted inside the session screen's padded body, so a bounded min-height
  // keeps both columns scrollable instead of overflowing the Shell.
  return html`<div style=${{ display: 'grid', gridTemplateColumns: '348px 1fr', minHeight: 560, border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden', background: 'var(--surface)' }}>
      <div style=${{ borderRight: '1px solid var(--rule-soft)', overflow: 'auto', padding: '20px 16px', display: 'flex', flexDirection: 'column' }}>
        <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--burgundy)' }}>Session ${cam.session_number || '?'} · proposed</div>
        <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 22, fontWeight: 500, letterSpacing: '-0.015em', lineHeight: 1.15, marginTop: 3 }}>Update the Codex</h1>
        <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', marginTop: 5, lineHeight: 1.5, fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>
          The Chronicle read the transcript and the summary. Here's what it would change. Nothing is written until you commit.
        </div>
        <div style=${{ marginTop: 12 }}>${actions}</div>

        ${streaming && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 20, padding: '12px 14px', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 7, fontSize: 12.5, color: 'var(--ink-soft)' }}>
          <${Spinner} size=${14} /> ${STAGE_LABEL[streaming.stage] || 'Working…'}
        </div>`}

        ${runStale && !streaming && html`<div style=${{ marginTop: 16, padding: '10px 12px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.22)', borderRadius: 6, fontSize: 12, color: 'var(--ochre)' }}>
          A newer summary exists — these proposals come from an older run. Regenerate to refresh.
        </div>`}

        ${!streaming && run && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, margin: '16px 0 2px', padding: '0 4px' }}>
          <span style=${{ fontSize: 11, fontWeight: 600, color: 'var(--ink-soft)', display: 'flex', alignItems: 'center', gap: 6 }}>
            <${Icon} name="check" size=${12} className="ck-burgundy" /> ${acceptedIds.length} selected
          </span>
          <span style=${{ flex: 1 }} />
          <span style=${{ fontSize: 11, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)' }}>${proposals.length}${run.token_estimate ? ` · ~${(run.token_estimate / 1000).toFixed(1)}k tokens` : ''}</span>
        </div>`}

        ${!streaming && run && !proposals.length && html`<${Empty} icon="feather" title="Nothing to change">The summary held no codex-worthy updates. Skip ahead, or regenerate after a re-summary.</${Empty}>`}

        ${newOnes.length > 0 && sectionHead('New pages')}
        ${newOnes.map((p) => html`<${ProposalRow} key=${p.id} p=${p} selected=${selected?.id === p.id}
          onSelect=${() => { setSelectedId(p.id); setEditing(false); setDraft({}); }} onToggle=${() => toggle(p)} />`)}
        ${revised.length > 0 && sectionHead('Revised')}
        ${revised.map((p) => html`<${ProposalRow} key=${p.id} p=${p} selected=${selected?.id === p.id}
          onSelect=${() => { setSelectedId(p.id); setEditing(false); setDraft({}); }} onToggle=${() => toggle(p)} />`)}
      </div>

      <div style=${{ overflow: 'auto', padding: '24px 32px', background: 'var(--paper)' }}>
        ${selected ? html`<div style=${{ maxWidth: 720, margin: '0 auto' }}>
          <div style=${{ display: 'flex', alignItems: 'center', gap: 12 }}>
            ${glyphFor(selected.kind, 40)}
            <div style=${{ flex: 1, minWidth: 0 }}>
              <div style=${{ fontFamily: 'var(--font-display)', fontSize: 20, fontWeight: 500, letterSpacing: '-0.01em' }}>${selected.title}</div>
              <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)' }}>${selected.page || `${selected.folder ? selected.folder + '/' : ''}${selected.title}.md · new`}</div>
            </div>
            <div style=${{ display: 'flex', gap: 6 }}>${chipsOf(selected).map((k, i) => html`<${ChangeChip} key=${i} kind=${k} />`)}</div>
          </div>

          ${selected.grounding
            ? html`<${Provenance} g=${selected.grounding} />`
            : selected.ungrounded && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 16, padding: '9px 13px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.22)', borderRadius: 7, fontSize: 12, color: 'var(--ochre)' }}>
                <${Icon} name="flame" size=${13} />
                <span style=${{ lineHeight: 1.4 }}>Not verified against the transcript — review with care before accepting.</span>
              </div>`}

          ${selected.rationale && html`<div style=${{ display: 'flex', alignItems: 'flex-start', gap: 10, marginTop: 14, marginBottom: 24, fontFamily: 'var(--font-display)', fontStyle: 'italic', fontSize: 14, color: 'var(--ink-soft)', lineHeight: 1.55 }}>
            <${Icon} name="feather" size=${14} className="ck-burgundy" style=${{ marginTop: 3 }} />
            <span>“${selected.rationale}”</span>
          </div>`}

          <div style=${{ marginTop: selected.rationale ? 0 : 20 }}>
            ${selected.changes.map((ch, i) => html`<${ChangeBlock} key=${i} change=${ch} editing=${editing} draft=${draft} setDraft=${setDraft} />`)}
          </div>

          <div style=${{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 22, paddingTop: 18, borderTop: '1px solid var(--rule)' }}>
            ${editing
              ? html`<${Btn} kind="ghost" onClick=${() => { setEditing(false); setDraft({}); }}>Cancel</${Btn}>
                  <${Btn} kind="primary" icon="check" onClick=${saveEdit}>Save edit</${Btn}>`
              : html`<${Btn} kind="secondary" icon="x" onClick=${() => setDecision(selected, 'rejected')}>Reject</${Btn}>
                  <${Btn} kind="secondary" icon="edit" onClick=${() => { setEditing(true); setDraft({}); }}>Edit before commit</${Btn}>`}
            <span style=${{ flex: 1 }} />
            ${(selected.decision === 'accepted' || selected.decision === 'edited')
              ? html`<span style=${{ display: 'flex', alignItems: 'center', gap: 6, padding: '7px 14px', background: 'var(--moss-50)', color: 'var(--moss)', border: '1px solid rgba(74,93,58,.3)', borderRadius: 5, fontSize: 12.5, fontWeight: 600 }}>
                  <${Icon} name="check" size=${13} /> ${selected.decision === 'edited' ? 'Accepted · edited' : 'Accepted'}
                </span>`
              : selected.decision === 'stale'
                ? html`<span style=${{ fontSize: 12.5, color: 'var(--ochre)', fontWeight: 600 }}>Stale — page changed since</span>`
                : html`<${Btn} kind="primary" icon="check" onClick=${() => setDecision(selected, 'accepted')}>Accept</${Btn}>`}
          </div>
        </div>` : !streaming && html`<${Empty} icon="book" title="No proposal selected">Pick a proposal on the left to review its changes.</${Empty}>`}
      </div>
    </div>`;
}

// Transcript-grounded provenance bar + collapsible cited excerpt ("Show it").
function Provenance({ g }) {
  const [open, setOpen] = useState(false);
  const [a, b] = g.turns || [0, 0];
  return html`<div style=${{ marginTop: 16 }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 10, padding: '9px 13px', background: 'var(--ink-blue-50)', border: '1px solid rgba(53,83,112,.18)', borderRadius: 7, fontSize: 12, color: 'var(--ink-blue)' }}>
      <${Icon} name="waveform" size=${13} />
      <span style=${{ flex: 1, lineHeight: 1.4 }}>Grounded in the transcript — <b style=${{ fontWeight: 600 }}>turns ${a.toLocaleString()}–${b.toLocaleString()}</b>, not just the summary.</span>
      <button onClick=${() => setOpen(!open)} style=${{ fontSize: 11.5, color: 'var(--ink-blue)', fontWeight: 500, display: 'flex', alignItems: 'center', gap: 4, background: 'none', border: 'none', cursor: 'pointer' }}>
        ${open ? 'Hide it' : 'Show it'} <${Icon} name=${open ? 'chev-d' : 'arrow-r'} size=${11} />
      </button>
    </div>
    ${open && html`<div style=${{ marginTop: 6, padding: '10px 14px', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 7, fontSize: 12.5, lineHeight: 1.6, color: 'var(--ink-soft)', whiteSpace: 'pre-wrap', fontFamily: 'var(--font-mono)', maxHeight: 260, overflow: 'auto' }}>${g.excerpt}</div>`}
  </div>`;
}
