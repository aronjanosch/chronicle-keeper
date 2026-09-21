// Session Review (SC-06). Cards are developments — what happened — with their
// sources and the exact page bytes they would write. Questions and future
// possibilities stay visibly separate and never count toward Apply. Nothing
// here writes: selection and application are two deliberate steps.
import { html, useState, useEffect, useRef } from '../../vendor/htm-preact-standalone.mjs';
import { navigate } from '../core.js';
import {
  applySelectedUpdates, cancelReviewGeneration, clarifyQuestion, finishReviewRun,
  generateReview, loadReviewRun, recoverApplication, reopenReviewRun, saveReviewDecisions,
} from '../actions.js';
import { newRequestId } from '../review.js';
import {
  applicationOf, canApply, cardStatus, changesNothing, adjustmentPayload, developments,
  evidenceText, isLegacy, lineDiff, needsRecovery, openQuestionCount, possibilities,
  questions, selectedIds, skipEntry, sourceLabel, summarizeRun, targetProgress, toggleSelection,
} from '../reviewState.js';
import { Btn, Card, Empty, Icon, Spinner, Textarea } from '../ui.js';

const STAGE_LABEL = {
  reading: 'Reading the session…',
  grounding: 'Checking sources…',
  building: 'Preparing the review…',
};

const STATUS_PILL = {
  selected: { label: 'Selected', bg: 'var(--burgundy-50)', col: 'var(--burgundy)' },
  skipped: { label: 'Skipped', bg: 'var(--paper-deep)', col: 'var(--ink-muted)' },
  deferred: { label: 'Left for later', bg: 'var(--ochre-50)', col: 'var(--ochre)' },
  applied: { label: 'Applied', bg: 'var(--moss-50)', col: 'var(--moss)' },
  conflicted: { label: 'Needs attention', bg: 'var(--ochre-50)', col: 'var(--ochre)' },
  partial: { label: 'Partly applied', bg: 'var(--ochre-50)', col: 'var(--ochre)' },
  recovering: { label: 'Interrupted', bg: 'var(--ochre-50)', col: 'var(--ochre)' },
};

function Pill({ status }) {
  const p = STATUS_PILL[status];
  if (!p) return null;
  return html`<span style=${{ padding: '2px 8px', borderRadius: 4, fontSize: 11, fontWeight: 600, background: p.bg, color: p.col, border: '1px solid rgba(0,0,0,.05)' }}>${p.label}</span>`;
}

function SectionHead({ label, count, right }) {
  return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, margin: '26px 0 10px' }}>
    <h2 style=${{ fontFamily: 'var(--font-display)', fontSize: 15, fontWeight: 500, color: 'var(--ink)', margin: 0 }}>${label}</h2>
    ${count != null && html`<span style=${{ fontSize: 11.5, color: 'var(--ink-faint)', fontFamily: 'var(--font-mono)' }}>${count}</span>`}
    <span style=${{ flex: 1 }} />
    ${right}
  </div>`;
}

// Evidence is data, never markup: the excerpt renders as plain preformatted
// text so a transcript line cannot inject anything into the review.
function SourcePanel({ evidence }) {
  if (!evidence || !evidence.length) {
    return html`<div style=${{ fontSize: 12, color: 'var(--ink-faint)', padding: '8px 0' }}>No source recorded.</div>`;
  }
  return html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 10, marginTop: 10 }}>
    ${evidence.map((e, i) => html`<div key=${i} style=${{ border: '1px solid var(--rule)', borderRadius: 6, overflow: 'hidden' }}>
      <div style=${{ padding: '6px 11px', background: e.kind === 'transcript' ? 'var(--ink-blue-50)' : 'var(--paper-deep)', fontSize: 11.5, fontWeight: 600, color: e.kind === 'transcript' ? 'var(--ink-blue)' : 'var(--ink-muted)' }}>
        ${sourceLabel(e)}
      </div>
      <pre style=${{ margin: 0, padding: '10px 12px', background: 'var(--surface)', fontFamily: 'var(--font-mono)', fontSize: 12, lineHeight: 1.55, color: 'var(--ink-soft)', whiteSpace: 'pre-wrap', maxHeight: 260, overflow: 'auto' }}>${evidenceText(e)}</pre>
    </div>`)}
  </div>`;
}

function DiffRows({ before, after }) {
  const rows = lineDiff(before, after);
  return html`<div style=${{ border: '1px solid var(--rule)', borderRadius: 6, overflow: 'auto', background: 'var(--surface)', maxHeight: 340 }}>
    ${rows.map((r, i) => {
      const tone = r.mode === 'add' ? { bg: 'var(--moss-50)', col: 'var(--ink)', mark: '+' }
        : r.mode === 'remove' ? { bg: 'rgba(122,46,31,.07)', col: 'var(--ink-muted)', mark: '−' }
          : r.mode === 'gap' ? { bg: 'var(--paper-deep)', col: 'var(--ink-faint)', mark: '' }
            : { bg: 'transparent', col: 'var(--ink-muted)', mark: ' ' };
      return html`<div key=${i} style=${{ display: 'flex', gap: 8, padding: '2px 10px', background: tone.bg, fontFamily: 'var(--font-mono)', fontSize: 12, lineHeight: 1.5 }}>
        <span style=${{ width: 8, flex: '0 0 auto', color: 'var(--ink-ghost)' }}>${tone.mark}</span>
        <span style=${{ color: tone.col, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>${r.text || ' '}</span>
      </div>`;
    })}
  </div>`;
}

function ChangesPanel({ dev }) {
  return html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 14, marginTop: 10 }}>
    ${(dev.targets || []).map((t) => html`<div key=${t.path}>
      <div style=${{ fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--ink-faint)', marginBottom: 5 }}>
        ${t.path}${t.before == null ? ' · new page' : ''}
      </div>
      <${DiffRows} before=${t.before} after=${t.after} />
    </div>`)}
  </div>`;
}

// Adjust edits the previewed bytes and the description. The target set itself
// is not editable here — restructuring belongs in the editor or the Keeper.
function AdjustPanel({ dev, onCancel, onSave }) {
  const [description, setDescription] = useState(dev.description || '');
  const [afters, setAfters] = useState(() => Object.fromEntries((dev.targets || []).map((t) => [t.path, t.after])));
  const [busy, setBusy] = useState(false);

  async function save() {
    const payload = adjustmentPayload(dev, { description, afters });
    if (changesNothing(payload)) { onCancel(); return; }
    setBusy(true);
    try { await onSave(payload); } finally { setBusy(false); }
  }

  return html`<div style=${{ marginTop: 12, padding: '14px 16px', background: 'var(--paper-deep)', border: '1px solid var(--rule)', borderRadius: 7 }}>
    <div style=${{ fontSize: 11.5, fontWeight: 600, color: 'var(--ink-muted)', marginBottom: 6 }}>Description</div>
    <${Textarea} rows=${2} value=${description} onInput=${setDescription} />
    <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 4, lineHeight: 1.45 }}>
      Editing the description alone changes nothing on the page.
    </div>
    ${(dev.targets || []).map((t) => html`<div key=${t.path} style=${{ marginTop: 14 }}>
      <div style=${{ fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--ink-faint)', marginBottom: 5 }}>${t.path}</div>
      <${Textarea} rows=${10} value=${afters[t.path]} onInput=${(v) => setAfters({ ...afters, [t.path]: v })} />
    </div>`)}
    <div style=${{ display: 'flex', gap: 8, marginTop: 14 }}>
      <${Btn} kind="ghost" size="sm" onClick=${onCancel}>Cancel</${Btn}>
      <${Btn} kind="primary" size="sm" icon="check" disabled=${busy} onClick=${save}>${busy ? 'Saving…' : 'Save changes'}</${Btn}>
    </div>
    <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 8, lineHeight: 1.45 }}>
      Saving clears this card's selection so you can read the updated preview before choosing it again.
    </div>
  </div>`;
}

function RecoveryNote({ run, dev, onRecover }) {
  const app = applicationOf(run, dev);
  if (!app || app.status === 'applied') return null;
  const { written, pending, conflicted } = targetProgress(app);
  if (app.status === 'partial') {
    return html`<div style=${{ marginTop: 10, padding: '10px 12px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.22)', borderRadius: 6, fontSize: 12, color: 'var(--ochre)', lineHeight: 1.5 }}>
      Partly applied: ${written.join(', ') || 'no pages'} written, the rest deferred.
    </div>`;
  }
  return html`<div style=${{ marginTop: 10, padding: '10px 12px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.22)', borderRadius: 6, fontSize: 12, color: 'var(--ochre)', lineHeight: 1.5 }}>
    ${conflicted.length ? html`<div>Changed outside this review, so nothing was written there: ${conflicted.join(', ')}.</div>` : ''}
    ${written.length ? html`<div>Already written: ${written.join(', ')}.</div>` : ''}
    ${pending.length ? html`<div>Still to write: ${pending.join(', ')}.</div>` : ''}
    <div style=${{ display: 'flex', gap: 8, marginTop: 8, flexWrap: 'wrap' }}>
      ${pending.length > 0 && html`<${Btn} kind="secondary" size="sm" icon="check" onClick=${() => onRecover(app.id, 'retry')}>Retry the remaining changes</${Btn}>`}
      <${Btn} kind="ghost" size="sm" onClick=${() => onRecover(app.id, 'keep_partial')}>Keep applied files, defer the rest</${Btn}>
      ${conflicted.length > 0 && html`<${Btn} kind="ghost" size="sm" icon="sparkle" onClick=${() => generateReview({ developmentIds: [dev.id] })}>Regenerate this update</${Btn}>`}
    </div>
  </div>`;
}

function DevelopmentCard({ run, dev, locked, onDecision, onAdjust, onRecover }) {
  const [panel, setPanel] = useState(null); // 'source' | 'changes' | 'adjust'
  const status = cardStatus(run, dev);
  const readOnly = locked || status === 'applied';
  const targets = dev.targets || [];
  const affects = targets.map((t) => t.path.replace(/\.md$/, '').split('/').pop());
  const toggle = (name) => setPanel(panel === name ? null : name);

  return html`<article style=${{ border: '1px solid var(--rule)', borderRadius: 8, background: 'var(--surface)', padding: '16px 18px', marginBottom: 12 }}>
    <div style=${{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
      <div style=${{ flex: 1, minWidth: 0 }}>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
          <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 15.5, fontWeight: 500, color: 'var(--ink)', margin: 0 }}>${dev.title}</h3>
          ${dev.compound && html`<span style=${{ fontSize: 11, fontWeight: 600, color: 'var(--ink-blue)', background: 'var(--ink-blue-50)', border: '1px solid rgba(53,83,112,.18)', borderRadius: 4, padding: '2px 8px' }}>Related updates</span>`}
          <${Pill} status=${status} />
        </div>
        <div style=${{ fontSize: 13, color: 'var(--ink-soft)', lineHeight: 1.6, marginTop: 6, whiteSpace: 'pre-wrap' }}>${dev.description}</div>
        ${affects.length > 0 && html`<div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 8 }}>Affects: ${affects.join(' · ')}</div>`}
      </div>
    </div>

    ${!readOnly && html`<div style=${{ display: 'flex', gap: 8, marginTop: 12, flexWrap: 'wrap' }}>
      <${Btn} kind=${dev.decision === 'selected' ? 'primary' : 'secondary'} size="sm"
        icon=${dev.decision === 'selected' ? 'check' : undefined}
        onClick=${() => onDecision(toggleSelection(dev))}>
        ${dev.decision === 'selected' ? 'Selected' : 'Use this update'}
      </${Btn}>
      <${Btn} kind="ghost" size="sm" icon="edit" onClick=${() => toggle('adjust')}>Adjust</${Btn}>
      <${Btn} kind="ghost" size="sm" onClick=${() => onDecision(skipEntry(dev))}>${dev.decision === 'skipped' ? 'Reopen' : 'Skip'}</${Btn}>
    </div>`}

    <div style=${{ display: 'flex', gap: 14, marginTop: 10, flexWrap: 'wrap' }}>
      <button onClick=${() => toggle('source')} style=${linkBtn}>${panel === 'source' ? 'Hide source' : 'View source'}</button>
      <button onClick=${() => toggle('changes')} style=${linkBtn}>
        ${panel === 'changes' ? 'Hide page changes' : `View ${targets.length} page change${targets.length === 1 ? '' : 's'}`}
      </button>
      ${readOnly && status === 'applied' && html`<span style=${{ fontSize: 12, color: 'var(--ink-faint)' }}>Written to the codex — see each page's history.</span>`}
    </div>

    ${panel === 'source' && html`<${SourcePanel} evidence=${dev.evidence} />`}
    ${panel === 'changes' && html`<${ChangesPanel} dev=${dev} />`}
    ${panel === 'adjust' && !readOnly && html`<${AdjustPanel} dev=${dev} onCancel=${() => setPanel(null)}
      onSave=${async (payload) => { await onAdjust(payload); setPanel('changes'); }} />`}

    <${RecoveryNote} run=${run} dev=${dev} onRecover=${onRecover} />
  </article>`;
}

const linkBtn = {
  background: 'none', border: 'none', padding: 0, cursor: 'pointer',
  fontSize: 12, color: 'var(--ink-blue)', fontFamily: 'inherit',
};

function QuestionCard({ q, onDefer, onClarify, busy }) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState('');
  const resolved = q.status === 'resolved';
  return html`<article style=${{ border: '1px solid var(--rule)', borderRadius: 8, background: 'var(--surface)', padding: '14px 16px', marginBottom: 10 }}>
    <div style=${{ fontSize: 13.5, color: 'var(--ink)', lineHeight: 1.55 }}>${q.text}</div>
    ${resolved
      ? html`<div style=${{ fontSize: 12, color: 'var(--moss)', marginTop: 8 }}>Resolved — you answered this, and the answer became an update above.</div>`
      : q.status === 'deferred'
        ? html`<div style=${{ fontSize: 12, color: 'var(--ink-faint)', marginTop: 8 }}>Left unresolved.</div>`
        : html`<div style=${{ display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap' }}>
            <${Btn} kind="secondary" size="sm" onClick=${() => setOpen(!open)}>${open ? 'Cancel' : 'Clarify'}</${Btn}>
            <${Btn} kind="ghost" size="sm" onClick=${() => onDefer(q)}>Leave unresolved</${Btn}>
          </div>`}
    ${open && !resolved && html`<div style=${{ marginTop: 10 }}>
      <${Textarea} rows=${3} value=${text} onInput=${setText} placeholder="What actually happened?" />
      <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', margin: '6px 0 8px', lineHeight: 1.45 }}>
        Your answer is recorded as the source. The transcript is not re-read, and nothing is invented for it.
      </div>
      <${Btn} kind="primary" size="sm" icon="check" disabled=${busy || !text.trim()}
        onClick=${async () => { await onClarify(q, text.trim()); setOpen(false); setText(''); }}>
        ${busy ? 'Working…' : 'Create an update from this'}
      </${Btn}>
    </div>`}
    ${(q.evidence || []).length > 0 && html`<${SourcePanel} evidence=${q.evidence} />`}
  </article>`;
}

function PossibilitySection({ run, onDismiss }) {
  const [open, setOpen] = useState(false);
  const items = possibilities(run).filter((p) => p.decision !== 'dismissed');
  if (!items.length) return null;
  return html`<div>
    <${SectionHead} label="Possible next developments" count=${items.length}
      right=${html`<${Btn} kind="ghost" size="sm" onClick=${() => setOpen(!open)}>${open ? 'Collapse' : 'Expand'}</${Btn}>`} />
    ${open && items.map((p) => html`<article key=${p.id} style=${{ border: '1px solid var(--rule)', borderRadius: 8, background: 'var(--surface)', padding: '14px 16px', marginBottom: 10 }}>
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 14, color: 'var(--ink)' }}>${p.title}</div>
      <div style=${{ fontSize: 13, color: 'var(--ink-soft)', lineHeight: 1.55, marginTop: 5 }}>${p.text}</div>
      ${(p.source_links || []).length > 0 && html`<div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 7 }}>From: ${p.source_links.join(' · ')}</div>`}
      <div style=${{ display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap' }}>
        <${Btn} kind="secondary" size="sm" disabled=${true} title="Arrives with the next-session handoff">Add to prep</${Btn}>
        <${Btn} kind="ghost" size="sm" onClick=${() => onDismiss(p)}>Dismiss</${Btn}>
      </div>
      <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 8 }}>This is prep material — it changes nothing in the world.</div>
    </article>`)}
    ${!open && html`<div style=${{ fontSize: 12, color: 'var(--ink-faint)' }}>${items.length} idea${items.length === 1 ? '' : 's'} for next time. They are never applied to the world.</div>`}
  </div>`;
}

function LegacyView({ run }) {
  return html`<${Card} title="Imported review">
    <div style=${{ fontSize: 12.5, color: 'var(--ink-soft)', lineHeight: 1.6 }}>${run.legacy_note}</div>
    ${(run.developments || []).map((d) => html`<div key=${d.id} style=${{ marginTop: 12, paddingTop: 12, borderTop: '1px solid var(--rule-soft)' }}>
      <div style=${{ fontFamily: 'var(--font-display)', fontSize: 14 }}>${d.title}</div>
      <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', marginTop: 4 }}>${d.description}</div>
    </div>`)}
    <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 14, lineHeight: 1.5 }}>
      These come from the older Update-the-Codex run. Generate a fresh review to work with sources and exact page changes.
    </div>
  </${Card}>`;
}

export function ReviewPane({ store, sess }) {
  const hasSummary = (store.summaries || []).length > 0;
  const review = store.review;
  const streaming = store.reviewStreaming;
  const run = review && review.run ? review.run : null;
  const flags = review?.flags || null;
  const legacy = isLegacy(review);
  const [confirmFinish, setConfirmFinish] = useState(false);
  const [applying, setApplying] = useState(false);
  const [clarifying, setClarifying] = useState(false);
  const live = useRef(null);

  useEffect(() => {
    if (!sess || review) return;
    loadReviewRun(sess.session_id).catch(() => {});
  }, [sess?.session_id]);

  if (!sess) return html`<div />`;

  if (!hasSummary) {
    return html`<${Card} title="World updates">
      <div style=${{ fontSize: 13, color: 'var(--ink-muted)', lineHeight: 1.6 }}>
        Add a transcript and summary to generate updates. Preparation and threads stay editable meanwhile.
      </div>
      <div style=${{ display: 'flex', gap: 8, marginTop: 12 }}>
        <${Btn} kind="secondary" size="sm" icon="arrow-r" onClick=${() => navigate('session', { id: sess.session_id, view: 'record' })}>Return to Record</${Btn}>
        <${Btn} kind="ghost" size="sm" onClick=${() => navigate('session', { id: sess.session_id, view: 'prepare' })}>Open Prepare</${Btn}>
      </div>
    </${Card}>`;
  }

  const devs = developments(run);
  const selected = selectedIds(run);
  const openQuestions = openQuestionCount(run);
  const finished = run && run.status === 'finished';
  const recovery = needsRecovery(run);
  const counts = summarizeRun(run);
  const applyable = canApply(run, flags) && !applying;

  const decide = (entry) => saveReviewDecisions({ decisions: [entry] }).catch(() => {});
  const adjust = (payload) => saveReviewDecisions({ adjustments: [payload] });
  const deferQuestion = (q) => saveReviewDecisions({ question_actions: [{ id: q.id, action: 'defer' }] }).catch(() => {});
  const dismissPossibility = (p) => saveReviewDecisions({ decisions: [{ id: p.id, decision: 'dismissed' }] }).catch(() => {});

  async function doApply() {
    if (!selected.length || applying) return;
    setApplying(true);
    // One receipt per attempt: a retry after a lost response returns the
    // recorded result instead of writing a second time.
    const requestId = newRequestId();
    try { await applySelectedUpdates(selected, requestId); } catch (_) {}
    setApplying(false);
    announce('Selected updates applied.');
  }

  async function doClarify(q, text) {
    setClarifying(true);
    try { await clarifyQuestion(q.id, text); } catch (_) {}
    setClarifying(false);
  }

  function announce(msg) {
    if (live.current) live.current.textContent = msg;
  }

  const generating = !!streaming;
  const primary = () => {
    if (generating) {
      return html`<${Btn} kind="secondary" size="sm" onClick=${() => cancelReviewGeneration()}>Cancel</${Btn}>`;
    }
    if (finished) {
      return html`<${Btn} kind="secondary" size="sm" icon="edit" onClick=${() => reopenReviewRun().catch(() => {})}>Reopen review</${Btn}>`;
    }
    if (!run) {
      return html`<${Btn} kind="primary" size="sm" icon="sparkle" onClick=${() => generateReview({})}>Find world updates</${Btn}>`;
    }
    if (selected.length) {
      return html`<${Btn} kind="primary" size="sm" icon="check" disabled=${!applyable} onClick=${doApply}>
        ${applying ? 'Applying…' : `Apply ${selected.length} selected update${selected.length === 1 ? '' : 's'}`}
      </${Btn}>`;
    }
    return html`<${Btn} kind="primary" size="sm" icon="check" disabled=${recovery.length > 0}
      onClick=${() => (counts.pending ? setConfirmFinish(true) : finishReviewRun(false).catch(() => {}))}>Finish review</${Btn}>`;
  };

  return html`<div style=${{ maxWidth: 860 }}>
    <div ref=${live} aria-live="polite" style=${{ position: 'absolute', width: 1, height: 1, overflow: 'hidden', clip: 'rect(0 0 0 0)' }} />

    <div style=${{ display: 'flex', alignItems: 'flex-start', gap: 12, flexWrap: 'wrap' }}>
      <div style=${{ flex: 1, minWidth: 220 }}>
        <h1 style=${{ fontFamily: 'var(--font-display)', fontSize: 20, fontWeight: 500, margin: 0 }}>World updates</h1>
        <div style=${{ fontSize: 12.5, color: 'var(--ink-muted)', marginTop: 4, lineHeight: 1.5 }}>
          ${run
            ? `${selected.length} selected · ${openQuestions} question${openQuestions === 1 ? '' : 's'} to consider`
            : 'Read what happened this session and choose which pages it changes. Nothing is written until you apply.'}
        </div>
      </div>
      <div style=${{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        ${run && !generating && !finished && html`<${Btn} kind="ghost" size="sm" icon="sparkle" onClick=${() => generateReview({})}>Regenerate</${Btn}>`}
        ${primary()}
      </div>
    </div>

    ${generating && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 16, padding: '12px 14px', background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 7, fontSize: 12.5, color: 'var(--ink-soft)' }}>
      <${Spinner} size=${14} /> ${STAGE_LABEL[streaming.stage] || 'Working…'}
      ${run && html`<span style=${{ color: 'var(--ink-faint)' }}>· the current review stays until this finishes</span>`}
    </div>`}

    ${flags?.stale && html`<div style=${{ marginTop: 16, padding: '10px 12px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.22)', borderRadius: 6, fontSize: 12, color: 'var(--ochre)', lineHeight: 1.5 }}>
      Out of date — the summary or transcript changed after this review was generated. It stays readable, but applying is disabled until you regenerate.
    </div>`}

    ${confirmFinish && html`<div style=${{ marginTop: 16, padding: '12px 14px', background: 'var(--paper-deep)', border: '1px solid var(--rule)', borderRadius: 7 }}>
      <div style=${{ fontSize: 13, color: 'var(--ink)', lineHeight: 1.55 }}>
        ${counts.pending} item${counts.pending === 1 ? '' : 's'} still undecided. Finishing records them as left for later — it neither applies nor rejects them.
      </div>
      <div style=${{ display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap' }}>
        <${Btn} kind="primary" size="sm" onClick=${async () => { setConfirmFinish(false); await finishReviewRun(true).catch(() => {}); announce('Review finished.'); }}>
          Finish and leave ${counts.pending} unresolved
        </${Btn}>
        <${Btn} kind="ghost" size="sm" onClick=${() => setConfirmFinish(false)}>Keep reviewing</${Btn}>
      </div>
    </div>`}

    ${finished && html`<div style=${{ marginTop: 16, padding: '12px 14px', background: 'var(--moss-50)', border: '1px solid rgba(74,93,58,.25)', borderRadius: 7, fontSize: 13, color: 'var(--ink)' }}>
      World review finished — ${counts.applied} applied · ${counts.skipped} skipped · ${counts.deferred} left for later.
    </div>`}

    ${legacy && html`<div style=${{ marginTop: 16 }}><${LegacyView} run=${review} /></div>`}

    ${run && html`<div>
      <${SectionHead} label="World updates" count=${devs.length} />
      ${devs.length === 0
        ? html`<${Empty} icon="feather" title="No world updates found">
            The session held nothing the codex needs. Finish the review, or return to Record and re-summarize.
          </${Empty}>`
        : devs.map((d) => html`<${DevelopmentCard} key=${d.id} run=${run} dev=${d} locked=${finished}
            onDecision=${decide} onAdjust=${adjust}
            onRecover=${(appId, action) => recoverApplication(appId, action).catch(() => {})} />`)}

      ${questions(run).length > 0 && html`<div>
        <${SectionHead} label="Questions to resolve" count=${openQuestions} />
        ${questions(run).map((q) => html`<${QuestionCard} key=${q.id} q=${q} busy=${clarifying}
          onDefer=${deferQuestion} onClarify=${doClarify} />`)}
      </div>`}

      <${PossibilitySection} run=${run} onDismiss=${dismissPossibility} />
    </div>`}

    ${!run && !legacy && !generating && html`<div style=${{ marginTop: 18 }}>
      <${Empty} icon="book" title="No review yet">
        Find world updates reads the summary, checks each claim against the transcript, and shows you exactly what would change.
      </${Empty}>
    </div>`}
  </div>`;
}
