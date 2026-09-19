// Session Prepare — manual, AI-free preparation for one session. Editable cards
// in three sections, saved to `prep.md` via GET/PUT /sessions/:id/prep with an
// 800 ms debounce, a serialized save queue, and explicit stale-save handling.
// Navigating away flushes pending edits first (see setLeaveGuard); a failed save
// keeps the draft and blocks the leave until the user retries or leaves anyway.
// SC-03 adds page/thread linking: existing vault pages are picked (never created
// by linking), and an unresolved reference stays visible instead of being guessed.
import { html, useState, useEffect, useRef } from '../../vendor/htm-preact-standalone.mjs';
import { copyText, loadVaultTree } from '../actions.js';
import { useStore, setLeaveGuard, navigate } from '../core.js';
import { Btn, Card, Menu, Icon, Spinner } from '../ui.js';
import { iconForKind, makeVaultActions } from './codex.js';
import { openPageEvt } from '../tabs.js';
import { createSaveQueue } from '../prepSave.js';
import {
  PREP_SECTIONS, PREP_OUTCOMES, newCard, cardsInSection, hasOpening,
  moveCard, removeCard, restoreCard, duplicateCard, applyOutcome, loadPrep, savePrep,
} from '../prep.js';
import {
  MAX_SUGGESTIONS, suggestPrep, acceptSuggestion, canAcceptOpening, replaceOpening,
  dismissSuggestion,
} from '../prepSuggest.js';

function serializeDraftText(cards) {
  const out = [];
  for (const s of PREP_SECTIONS) {
    const list = cardsInSection(cards, s.key);
    if (!list.length) continue;
    out.push(s.label);
    for (const c of list) out.push(c.text || '');
    out.push('');
  }
  return out.join('\n').trim();
}

// A page reference chip. Existing pages open on click; a path with no page in
// the vault index (moved/deleted externally, or awaiting reindex) renders as a
// visibly unresolved chip with a remove affordance — it is never auto-retargeted
// or guessed from a matching title.
function RefChip({ path, pages, onRemove }) {
  const p = pages.get(path);
  const unresolved = !p;
  const title = p?.title || path;
  return html`<span title=${unresolved ? `Not in the vault: ${path}` : path}
    style=${{ display: 'inline-flex', alignItems: 'center', gap: 4, maxWidth: '100%', padding: '2px 4px 2px 7px', borderRadius: 999,
      background: unresolved ? 'var(--ochre-50)' : 'var(--paper-deep)',
      border: `1px solid ${unresolved ? 'rgba(168,115,40,.4)' : 'var(--rule-soft)'}`,
      color: unresolved ? 'var(--ochre)' : 'var(--ink-soft)', fontSize: 11.5 }}>
    <span onClick=${unresolved ? null : (e) => openPageEvt(path, e)}
      style=${{ display: 'inline-flex', alignItems: 'center', gap: 4, minWidth: 0, cursor: unresolved ? 'default' : 'pointer' }}>
      <${Icon} name=${unresolved ? 'flame' : iconForKind(p.kind)} size=${11} />
      <span style=${{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${title}</span>
    </span>
    ${unresolved && html`<span style=${{ fontSize: 10, letterSpacing: '0.04em', textTransform: 'uppercase' }}>missing</span>`}
    ${onRemove && html`<button type="button" aria-label=${`Remove link to ${title}`} title="Remove link"
      onClick=${onRemove}
      style=${{ display: 'flex', padding: 1, border: 'none', background: 'none', color: 'inherit', cursor: 'pointer', opacity: 0.7 }}>
      <${Icon} name="x" size=${10} />
    </button>`}
  </span>`;
}

// Pick an existing page (never creates one). Typing filters store.vaultPages;
// picking a title that exactly matches more than one page lists each path so the
// user chooses explicitly rather than the UI guessing an identity.
function PagePicker({ pages, onPick, onClose }) {
  const [q, setQ] = useState('');
  const ql = q.trim().toLowerCase();
  const matches = (pages || [])
    .filter((p) => !ql || p.title.toLowerCase().includes(ql) || p.path.toLowerCase().includes(ql) || (p.aliases || []).some((a) => a.toLowerCase().includes(ql)))
    .sort((a, b) => a.title.localeCompare(b.title))
    .slice(0, 20);
  return html`<div style=${{ border: '1px solid var(--rule)', borderRadius: 7, background: 'var(--surface-raised)', padding: 6, marginTop: 6 }}>
    <input autofocus value=${q} placeholder="Find an existing page or thread…"
      onInput=${(e) => setQ(e.target.value)}
      onKeyDown=${(e) => { if (e.key === 'Escape') onClose(); }}
      style=${{ width: '100%', boxSizing: 'border-box', fontSize: 12.5, padding: '5px 8px', marginBottom: 4, borderRadius: 5,
        border: '1px solid var(--rule)', background: 'var(--surface)', color: 'var(--ink)', outline: 'none' }} />
    <div style=${{ maxHeight: 200, overflow: 'auto' }}>
      ${matches.length
        ? matches.map((p) => html`<button type="button" key=${p.path} onClick=${() => onPick(p.path)}
            style=${{ display: 'flex', alignItems: 'center', gap: 7, width: '100%', textAlign: 'left', padding: '5px 7px', border: 'none',
              background: 'transparent', borderRadius: 5, cursor: 'pointer', fontSize: 12.5, color: 'var(--ink)', fontFamily: 'inherit' }}
            onMouseEnter=${(e) => { e.currentTarget.style.background = 'var(--paper-deep)'; }}
            onMouseLeave=${(e) => { e.currentTarget.style.background = 'transparent'; }}>
            <${Icon} name=${iconForKind(p.kind)} size=${12} className="ck-ink-muted" />
            <span style=${{ flex: 1, minWidth: 0, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${p.title}</span>
            <span style=${{ fontFamily: 'var(--font-mono)', fontSize: 10, color: 'var(--ink-faint)' }}>${p.path}</span>
          </button>`)
        : html`<div style=${{ padding: '5px 7px', fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>No matching page. Linking never creates a page.</div>`}
    </div>
  </div>`;
}

// Small icon-only button with an accessible label (menu/move affordances).
function IconBtn({ name, label, onClick, disabled }) {
  return html`<button type="button" aria-label=${label} title=${label} disabled=${disabled}
    onClick=${disabled ? undefined : onClick}
    style=${{
      display: 'flex', alignItems: 'center', justifyContent: 'center', width: 26, height: 26,
      padding: 0, border: 'none', borderRadius: 5, background: 'transparent',
      color: disabled ? 'var(--ink-ghost)' : 'var(--ink-muted)',
      cursor: disabled ? 'default' : 'pointer',
    }}
    onMouseEnter=${disabled ? null : (e) => { e.currentTarget.style.background = 'var(--paper-deep)'; e.currentTarget.style.color = 'var(--ink)'; }}
    onMouseLeave=${disabled ? null : (e) => { e.currentTarget.style.background = 'transparent'; e.currentTarget.style.color = 'var(--ink-muted)'; }}>
    <${Icon} name=${name} size=${13} />
  </button>`;
}

const SUGGEST_STAGE_LABEL = { reading: 'Reading session', grounding: 'Checking sources', building: 'Preparing ideas' };

// One Keeper suggestion. Ephemeral until "Add to prep"; Adjust edits the text in
// place, and "Why this fits" hides the rationale + source links behind a toggle.
// An `is_idea` suggestion is labeled as a creative idea, never presented as fact.
function SuggestionRow({ s, pages, adjusting, adjustText, openingBlocked, onStartAdjust, onAdjustDraft, onCommitAdjust, onCancelAdjust, onAdd, onReplace, onDismiss }) {
  const [expanded, setExpanded] = useState(false);
  const section = PREP_SECTIONS.find((x) => x.key === s.section);
  const links = s.links || [];
  return html`<div style=${{ border: '1px solid var(--rule-soft)', borderRadius: 7, background: 'var(--surface-raised)', padding: '10px 12px' }}>    <div style=${{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap', marginBottom: 6 }}>
      <span style=${{ fontSize: 10, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase', color: 'var(--ink-faint)', border: '1px solid var(--rule-soft)', borderRadius: 999, padding: '1px 7px' }}>${section?.label || s.section}</span>
      ${s.is_idea && html`<span title="A creative prompt, not a fact from your notes" style=${{ display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 10.5, fontWeight: 600, letterSpacing: '0.03em', textTransform: 'uppercase', color: 'var(--ochre)', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.3)', borderRadius: 999, padding: '1px 7px' }}>
        <${Icon} name="sparkle" size=${10} /> Creative idea
      </span>`}
    </div>
    ${s.title && !adjusting && html`<div style=${{ fontFamily: 'var(--font-display)', fontWeight: 500, fontSize: 13.5, color: 'var(--ink)', marginBottom: 2 }}>${s.title}</div>`}
    ${adjusting
      ? html`<textarea autofocus value=${adjustText} onInput=${(e) => onAdjustDraft(e.target.value)}
          onKeyDown=${(e) => {
            if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); onCommitAdjust(); }
            else if (e.key === 'Escape') { e.preventDefault(); onCancelAdjust(); }
          }}
          rows=${3}
          style=${{ width: '100%', boxSizing: 'border-box', resize: 'vertical', padding: '7px 9px', border: '1px solid var(--rule-strong)', borderRadius: 5, background: 'var(--surface)', color: 'var(--ink)', fontFamily: 'inherit', fontSize: 13, lineHeight: 1.5, outline: 'none' }} />`
      : html`<div style=${{ fontSize: 13, color: 'var(--ink)', lineHeight: 1.5, whiteSpace: 'pre-wrap' }}>${s.text}</div>`}
    ${(s.rationale || links.length) && html`<div style=${{ marginTop: 7 }}>
      <button type="button" aria-expanded=${expanded}
        onClick=${() => setExpanded((v) => !v)}
        style=${{ display: 'inline-flex', alignItems: 'center', gap: 5, padding: 0, border: 'none', background: 'none', color: 'var(--ink-muted)', fontSize: 11.5, cursor: 'pointer' }}>
        <${Icon} name=${expanded ? 'chev-d' : 'chev-r'} size=${11} /> Why this fits
      </button>
      ${expanded && html`<div style=${{ marginTop: 6, paddingLeft: 16 }}>
        ${s.rationale && html`<div style=${{ fontSize: 12, color: 'var(--ink-muted)', lineHeight: 1.5, fontStyle: 'italic' }}>${s.rationale}</div>`}
        ${links.length && html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 4, marginTop: 6 }}>
          ${links.map((p) => html`<${RefChip} key=${p} path=${p} pages=${pages} />`)}
        </div>`}
      </div>`}
    </div>`}
    <div style=${{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 9 }}>
      ${adjusting
        ? html`
          <${Btn} kind="primary" size="sm" icon="check" onClick=${onCommitAdjust}>Save wording</${Btn}>
          <${Btn} kind="ghost" size="sm" onClick=${onCancelAdjust}>Cancel</${Btn}>`
        : html`
          ${openingBlocked
            ? html`<${Btn} kind="secondary" size="sm" icon="undo" onClick=${onReplace}>Replace opening</${Btn}>`
            : html`<${Btn} kind="secondary" size="sm" icon="plus" onClick=${onAdd}>Add to prep</${Btn}>`}
          <${Btn} kind="ghost" size="sm" icon="edit" onClick=${onStartAdjust}>Adjust</${Btn}>
          <span style=${{ flex: 1 }} />
          <${Btn} kind="ghost" size="sm" onClick=${onDismiss}>Dismiss</${Btn}>`}
    </div>
  </div>`;
}

function CardRow({ card, first, last, editing, draft, noteEditing, pages, onMove, onMenu, onCommit, onCancel, onDraft, onStartNote, onCommitNote, onAddLink, onRemoveLink }) {
  const [note, setNote] = useState(card.outcome_note || '');
  const done = card.outcome && card.outcome !== 'unmarked';
  return html`<div style=${{ display: 'flex', alignItems: 'flex-start', gap: 8, padding: '10px 18px', borderTop: first ? 'none' : '1px solid var(--rule-soft)' }}>
    <div style=${{ display: 'flex', flexDirection: 'column', gap: 0, flex: '0 0 auto', paddingTop: 2 }}>
      <${IconBtn} name="chev-u" label="Move up" disabled=${first} onClick=${() => onMove(-1)} />
      <${IconBtn} name="chev-d" label="Move down" disabled=${last} onClick=${() => onMove(1)} />
    </div>
    <div style=${{ flex: 1, minWidth: 0 }}>
      ${editing
        ? html`<textarea autofocus value=${draft} onInput=${(e) => onDraft(e.target.value)}
            onKeyDown=${(e) => {
              if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); onCommit(); }
              else if (e.key === 'Escape') { e.preventDefault(); onCancel(); }
              else if (e.key === 'Backspace' && draft === '') { e.preventDefault(); onCancel(true); }
            }}
            rows=${2}
            style=${{ width: '100%', boxSizing: 'border-box', resize: 'vertical', padding: '7px 9px', border: '1px solid var(--rule-strong)', borderRadius: 5, background: 'var(--surface-raised)', color: 'var(--ink)', fontFamily: 'inherit', fontSize: 13, lineHeight: 1.5, outline: 'none' }} />`
        : html`<div style=${{ fontSize: 13.5, color: 'var(--ink)', lineHeight: 1.5, whiteSpace: 'pre-wrap' }}>
            ${card.title && html`<div style=${{ fontFamily: 'var(--font-display)', fontWeight: 500, marginBottom: 1 }}>${card.title}</div>`}
            ${card.text}
          </div>`}
      ${done && html`<span style=${{ display: 'inline-flex', alignItems: 'center', gap: 4, marginTop: 5, padding: '1px 7px', borderRadius: 999, fontSize: 10.5, fontWeight: 600, letterSpacing: '0.04em', textTransform: 'uppercase', background: 'var(--paper-deep)', border: '1px solid var(--rule-soft)', color: 'var(--ink-muted)' }}>${card.outcome}</span>`}
      ${card.outcome === 'changed' && (noteEditing
        ? html`<textarea autofocus value=${note} onInput=${(e) => setNote(e.target.value)}
            placeholder="What changed?" rows=${1}
            onBlur=${() => onCommitNote(note)}
            onKeyDown=${(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); onCommitNote(note); } else if (e.key === 'Escape') { e.preventDefault(); onCommitNote(card.outcome_note || ''); } }}
            style=${{ width: '100%', boxSizing: 'border-box', marginTop: 6, resize: 'none', padding: '5px 8px', border: '1px solid var(--rule)', borderRadius: 4, background: 'var(--surface-raised)', color: 'var(--ink)', fontFamily: 'inherit', fontSize: 12.5, outline: 'none' }} />`
        : card.outcome_note
          ? html`<div style=${{ fontSize: 12, color: 'var(--ink-muted)', fontStyle: 'italic', marginTop: 5 }}>${card.outcome_note}</div>`
          : html`<button type="button" onClick=${onStartNote} style=${{ marginTop: 5, padding: 0, border: 'none', background: 'none', color: 'var(--burgundy)', fontSize: 12, cursor: 'pointer' }}>Add outcome note</button>`)}
      ${!editing && html`<div style=${{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 4, marginTop: 6 }}>
        ${(card.links || []).map((lp) => html`<${RefChip} key=${lp} path=${lp} pages=${pages} onRemove=${() => onRemoveLink(card.uid, lp)} />`)}
        <button type="button" onClick=${() => onAddLink(card)} title="Link an existing page or thread"
          style=${{ display: 'inline-flex', alignItems: 'center', gap: 4, padding: '2px 8px', border: '1px dashed var(--rule-strong)', borderRadius: 999,
            background: 'transparent', color: 'var(--ink-muted)', fontSize: 11.5, cursor: 'pointer' }}>
          <${Icon} name="link" size=${11} /> Link page/thread
        </button>
      </div>`}
    </div>
    ${!editing && html`<${Menu} items=${onMenu(card)} />`}
  </div>`;
}

export function SessionPrepare({ session, campaign }) {
  const sessionId = session?.session_id;
  const [draft, setDraft] = useState(null);        // { revision, cards, selected_threads, notes }
  const [status, setStatus] = useState('loading'); // loading | idle | saving | saved | error | conflict | unavailable
  const [announce, setAnnounce] = useState('');
  const [editingUid, setEditingUid] = useState(null);
  const [editDraft, setEditDraft] = useState('');
  const [noteUid, setNoteUid] = useState(null);
  // { card, index } — local remove-undo snapshot, cleared on the next edit.
  const [removed, setRemoved] = useState(null);
  // Which list an open page picker feeds: { kind: 'card', uid } | { kind: 'selected' } | null.
  const [pickTarget, setPickTarget] = useState(null);
  // Set when a navigation was blocked because the save failed: the intended
  // navigation resume fn plus the error, resolved by Retry / Leave without saving.
  const [leaveIssue, setLeaveIssue] = useState(null);
  // Keeper suggestions (SC-07). `suggest` = { phase: 'idle'|'streaming'|'error',
  // stage?, items[], error?, code? }; nothing generates until the first click.
  const [suggest, setSuggest] = useState({ phase: 'idle', items: [] });
  const [ideasOpen, setIdeasOpen] = useState(false);
  const [focusText, setFocusText] = useState('');
  const [adjustId, setAdjustId] = useState(null);
  const [adjustDraft, setAdjustDraft] = useState('');
  // The explicit "Replace opening" confirmation, with the old text shown.
  const [confirmReplace, setConfirmReplace] = useState(null);
  const store = useStore();

  const draftRef = useRef(null);
  const sessionIdRef = useRef(sessionId);
  const editingUidRef = useRef(editingUid);
  const editDraftRef = useRef(editDraft);
  const pendingNav = useRef(null);
  const statusRef = useRef(status);
  const suggestAbortRef = useRef(null);
  sessionIdRef.current = sessionId;
  editingUidRef.current = editingUid;
  editDraftRef.current = editDraft;
  statusRef.current = status;

  const hasWorld = !!(campaign?.campaign_id || session?.campaign?.campaign_id);

  // Apply server-assigned ids back onto the sent cards by position (the server
  // preserves display order). Local text/outcome edits made during the flight
  // are preserved; only the id is adopted.
  function adoptIds(sent, returned) {
    const d = draftRef.current;
    if (!d) return;
    const byUid = new Map();
    sent.forEach((c, i) => { const r = returned.cards?.[i]; if (r) byUid.set(c.uid, r); });
    const cards = d.cards.map((c) => {
      const r = byUid.get(c.uid);
      return r && r.id ? { ...c, id: r.id } : c;
    });
    draftRef.current = { ...d, revision: returned.revision, cards };
    setDraft(draftRef.current);
  }

  function applyStatus(next, e) {
    setStatus(next);
    if (next === 'saving') return;
    if (next === 'saved') { setAnnounce('Preparation saved'); setLeaveIssue(null); return; }
    if (next === 'conflict') { setAnnounce('Preparation was changed elsewhere. Your draft is kept; copy it or reload the saved version.'); return; }
    if (next === 'error') setAnnounce(`Could not save preparation: ${e?.message || 'unknown error'}`);
  }

  // One serialized save queue per component: coalesces the debounce, never
  // overlaps PUTs, and exposes isDirty/conflict so the leave guard can wait.
  const queueRef = useRef(null);
  if (!queueRef.current) {
    queueRef.current = createSaveQueue({
      save: async () => {
        const d = draftRef.current;
        // Never send an unsaved, still-empty card (the server rejects empty
        // text); it stays in the local draft until the user types something.
        const sent = d.cards.filter((c) => c.id || (c.text || '').trim());
        const res = await savePrep(sessionIdRef.current, {
          base_revision: d.revision,
          cards: sent,
          selected_threads: d.selected_threads,
          notes: d.notes,
        });
        adoptIds(sent, res);
      },
      onStatus: applyStatus,
    });
  }

  async function reload({ quiet = false } = {}) {
    if (!sessionId || !hasWorld) { setStatus('unavailable'); return; }
    if (!quiet) setStatus('loading');
    try {
      const loaded = await loadPrep(sessionId);
      draftRef.current = loaded;
      setDraft(loaded);
      queueRef.current.markClean();
      queueRef.current.resolveConflict();
      setLeaveIssue(null);
      setStatus('idle');
      setAnnounce('');
    } catch (e) {
      // 422 = the session has no world; 404 = gone. Either way, no editor.
      setStatus(e.status === 422 || e.status === 404 ? 'unavailable' : 'error');
      setAnnounce(`Could not load preparation: ${e.message}`);
    }
  }

  useEffect(() => {
    // A new session must not inherit the previous one's suggestions or prompt.
    setSuggest({ phase: 'idle', items: [] });
    setFocusText('');
    setAdjustId(null);
    setConfirmReplace(null);
    reload();
    // Best-effort flush on unmount (a guarded navigation already flushed; this
    // covers app teardown and hot reloads).
    return () => { queueRef.current && queueRef.current.flush(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, hasWorld]);

  // A suggestion stream belongs to one session. Switching sessions (or unmount)
  // drops the request and its suggestions so it can never land on a new screen.
  useEffect(() => () => {
    const c = suggestAbortRef.current;
    suggestAbortRef.current = null;
    if (c) c.abort();
  }, [sessionId]);

  // Leave guard: fold the active editor back into the draft, flush pending work,
  // and only then allow the navigation. A failed save blocks the leave and shows
  // Retry / Copy my text / Leave without saving.
  useEffect(() => {
    const unregister = setLeaveGuard((resume) => {
      commitActiveEdit();
      const q = queueRef.current;
      if (!q || (!q.isDirty() && !q.isBusy())) return true;
      pendingNav.current = resume;
      (async () => {
        const ok = await q.flush();
        if (ok) {
          const go = pendingNav.current;
          pendingNav.current = null;
          setLeaveIssue(null);
          if (go) go();
        } else {
          setLeaveIssue({ conflict: q.isConflict(), message: q.isConflict() ? 'The saved preparation changed elsewhere.' : 'Could not save before leaving.' });
        }
      })();
      return false;
    });
    return unregister;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Linking picks from the already-loaded vault index — never a create endpoint.
  // Fetch once if this session was opened without the campaign's tree loaded.
  const campaignId = campaign?.campaign_id || session?.campaign?.campaign_id;
  useEffect(() => {
    if (campaignId && !(store.vaultPages || []).length) loadVaultTree(campaignId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [campaignId]);
  const pagesByPath = new Map((store.vaultPages || []).map((p) => [p.path, p]));

  // Explicit thread creation routes through the shared vault action so the
  // default `Threads/` folder lives in exactly one place.
  const vaultAct = makeVaultActions(campaign, store.vaultFolders || []);

  // App-driven move/rename rewrites prep references server-side, so reload to
  // surface the new paths (and any now-unresolved ones) — never while a local
  // draft is unsaved, and never over an unresolved conflict. The page-path
  // signature changes on move/rename/create/delete.
  const vaultSig = (store.vaultPages || []).map((p) => p.path).sort().join('\n');
  const vaultSigRef = useRef(vaultSig);
  useEffect(() => {
    if (vaultSigRef.current === vaultSig) return;
    vaultSigRef.current = vaultSig;
    if (!sessionId) return;
    if (statusRef.current === 'loading') return;
    if (queueRef.current.isDirty() || queueRef.current.isConflict()) return;
    reload({ quiet: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vaultSig]);

  function schedule() { queueRef.current.schedule(); }

  // Fold an in-progress inline edit into the draft so a save or navigation
  // never drops text the user has typed but not yet committed with Enter.
  function commitActiveEdit() {
    const uid = editingUidRef.current;
    if (!uid) return;
    const text = (editDraftRef.current || '').trim();
    const d = draftRef.current;
    setEditingUid(null);
    if (!d) return;
    if (!text) {
      const existing = d.cards.find((c) => c.uid === uid);
      if (existing && !existing.id) {
        // Never-saved blank: drop it locally rather than send empty text.
        update((cards) => removeCard(cards, uid), { keepUndo: true, save: false });
      } else if (existing) {
        doRemove(uid);
      }
      return;
    }
    if (d.cards.some((c) => c.uid === uid && c.text !== text)) {
      update((cards) => cards.map((c) => (c.uid === uid ? { ...c, text } : c)));
    }
  }

  // Synchronous draft mutation. `save` schedules the debounce (true for real
  // edits); adding an empty card does not, so an untouched card never triggers
  // a save the server would reject.
  function update(fn, { keepUndo = false, save = true } = {}) {
    const d = draftRef.current;
    if (!d) return;
    if (!keepUndo) setRemoved(null);
    const next = { ...d, cards: fn(d.cards) };
    draftRef.current = next;
    setDraft(next);
    if (save) schedule();
  }

  // Whole-draft mutation (used by selected_threads; cards pass through).
  function updateDoc(patch) {
    const d = draftRef.current;
    if (!d) return;
    setRemoved(null);
    const next = { ...d, ...patch };
    draftRef.current = next;
    setDraft(next);
    schedule();
  }

  function addLinkToCard(uid, path) {
    update((cards) => cards.map((c) => (c.uid === uid
      ? ((c.links || []).includes(path) ? c : { ...c, links: [...(c.links || []), path] })
      : c)));
  }

  function removeLinkFromCard(uid, path) {
    update((cards) => cards.map((c) => (c.uid === uid ? { ...c, links: (c.links || []).filter((x) => x !== path) } : c)));
  }

  function addSelectedThread(path) {
    if ((draftRef.current?.selected_threads || []).includes(path)) return;
    updateDoc({ selected_threads: [...(draftRef.current?.selected_threads || []), path] });
  }

  function removeSelectedThread(path) {
    updateDoc({ selected_threads: (draftRef.current?.selected_threads || []).filter((x) => x !== path) });
  }

  // An explicitly separate "create thread" action — never a side effect of
  // linking. Opens the ordinary newPage modal preset to kind: thread.
  function createThread() {
    const target = pickTarget;
    vaultAct.newThread(null, (p) => {
      if (target?.kind === 'card') addLinkToCard(target.uid, p.path);
      else addSelectedThread(p.path);
      setPickTarget(null);
    });
  }

  function addCard(section) {
    const card = newCard(section, '');
    update((cards) => [...cards, card], { save: false });
    setEditingUid(card.uid);
    setEditDraft('');
  }

  function startEdit(card) {
    setEditingUid(card.uid);
    setEditDraft(card.text || '');
  }

  // Persisted id for a card, if it has already round-tripped. An unsaved card
  // (id null, never typed) can be dropped locally instead of sent.
  function cardId(uid) {
    return draftRef.current?.cards.find((c) => c.uid === uid)?.id ?? null;
  }

  // Enter in the inline editor: commit the text (empty on a saved card removes it).
  function commitEdit() {
    const uid = editingUid;
    const text = editDraft.trim();
    setEditingUid(null);
    if (!uid) return;
    if (!text) {
      if (cardId(uid)) doRemove(uid);
      else update((cards) => removeCard(cards, uid), { keepUndo: true, save: false });
      return;
    }
    update((cards) => cards.map((c) => (c.uid === uid ? { ...c, text } : c)));
  }

  function doRemove(uid) {
    const d = draftRef.current;
    if (!d) return;
    const index = d.cards.findIndex((c) => c.uid === uid);
    if (index < 0) return;
    setRemoved({ card: d.cards[index], index });
    update((cards) => removeCard(cards, uid), { keepUndo: true });
  }

  function undoRemove() {
    const snap = removed;
    if (!snap) return;
    setRemoved(null);
    // Restore as a fresh card; if its removal already saved, the old id is gone,
    // so the server would reject a re-used id as unknown.
    update((cards) => {
      const next = cards.slice();
      next.splice(Math.min(snap.index, next.length), 0, restoreCard(snap.card));
      return next;
    });
  }

  // Keeper suggestion actions (SC-07). Nothing here runs on mount: generation
  // is always an explicit click. Manual edits are untouched by all of it.
  async function runSuggest() {
    if (!sessionId || suggestAbortRef.current) return;
    const linkedPaths = draftRef.current?.selected_threads || [];
    const controller = new AbortController();
    suggestAbortRef.current = controller;
    setAdjustId(null);
    // Keep the previous run visible while a new one streams; only replace it on
    // a successful done. A failure or cancel never discards prior suggestions.
    setSuggest((cur) => ({ phase: 'streaming', stage: 'reading', items: cur.items || [] }));
    try {
      const res = await suggestPrep(sessionId, {
        instruction: focusText,
        linkedPaths,
        onEvent: (ev) => {
          if (ev?.stage === 'reading' || ev?.stage === 'grounding' || ev?.stage === 'building') {
            setSuggest((cur) => ({ ...cur, stage: ev.stage }));
          }
        },
      }, { signal: controller.signal });
      if (suggestAbortRef.current !== controller) return; // superseded
      suggestAbortRef.current = null;
      if (res.ok) {
        const items = (res.suggestions || []).slice(0, MAX_SUGGESTIONS);
        setSuggest({ phase: items.length ? 'idle' : 'empty', stage: null, items });
      } else {
        setSuggest((cur) => ({ ...cur, phase: 'error', stage: null, error: res.message || 'The Keeper could not prepare ideas.', code: res.code }));
      }
    } catch (e) {
      if (suggestAbortRef.current !== controller) return;
      suggestAbortRef.current = null;
      // An abort is a deliberate cancel, not a failure: keep whatever arrived.
      if (e?.name === 'AbortError') { setSuggest((cur) => ({ ...cur, phase: 'idle', stage: null })); return; }
      setSuggest((cur) => ({ ...cur, phase: 'error', stage: null, error: e?.message || 'The Keeper could not prepare ideas.' }));
    }
  }

  function cancelSuggest() {
    const c = suggestAbortRef.current;
    suggestAbortRef.current = null;
    if (c) c.abort();
    setSuggest((cur) => ({ ...cur, phase: 'idle', stage: null }));
  }

  function addSuggestion(s) {
    update((cards) => acceptSuggestion(cards, s));
    // Drop an errored/empty panel back to idle; leave a live stream untouched.
    setSuggest((cur) => ({
      phase: cur.phase === 'streaming' ? cur.phase : 'idle',
      stage: cur.stage, items: dismissSuggestion(cur.items, s.id),
    }));
    setAdjustId(null);
  }

  function startAdjust(s) {
    setAdjustId(s.id);
    setAdjustDraft(s.text || '');
  }

  function commitAdjust() {
    const id = adjustId;
    const text = adjustDraft.trim();
    setAdjustId(null);
    if (!id || !text) return;
    setSuggest((cur) => ({ ...cur, items: cur.items.map((s) => (s.id === id ? { ...s, text } : s)) }));
  }

  // An opening suggestion never silently adds or replaces: the caller must open
  // the confirmation (old text shown) and then run this.
  function requestReplaceOpening(s) {
    const existing = (draftRef.current?.cards || []).find((c) => c.section === 'opening');
    setConfirmReplace({ suggestion: s, existing: existing || null });
  }

  function confirmReplaceOpening() {
    const pending = confirmReplace;
    setConfirmReplace(null);
    if (!pending) return;
    update((cards) => replaceOpening(cards, pending.suggestion));
    setSuggest((cur) => ({ ...cur, items: dismissSuggestion(cur.items, pending.suggestion.id) }));
  }

  function onMenu(card) {
    const items = [
      { label: 'Edit', icon: 'edit', onClick: () => startEdit(card) },
      // A second opening is invalid; only offer duplicate for non-opening cards.
      { label: 'Duplicate', icon: 'copy', hidden: card.section === 'opening', onClick: () => update((cards) => duplicateCard(cards, card.uid)) },
      { label: 'Remove', icon: 'trash', danger: true, onClick: () => doRemove(card.uid) },
      ...PREP_OUTCOMES.map((o) => ({
        label: o.key === card.outcome ? `${o.label} ✓` : o.label,
        onClick: () => { update((cards) => applyOutcome(cards, card.uid, o.key)); if (o.key === 'changed') setNoteUid(card.uid); },
      })),
    ];
    return items;
  }

  function sectionCard(section) {
    const list = cardsInSection(draft?.cards || [], section.key);
    const isOpening = section.key === 'opening';
    const canAdd = !(isOpening && hasOpening(draft?.cards || []));
    const addBtn = canAdd && html`<${Btn} kind="ghost" size="sm" icon="plus" onClick=${() => addCard(section.key)}>Add</${Btn}>`;
    // overflow visible so each row's kebab dropdown isn't clipped by the card.
    return html`<${Card} key=${section.key} title=${section.label} right=${addBtn} bodyPad=${false} style=${{ overflow: 'visible' }}>
      ${list.length
        ? list.map((c, i) => html`<${CardRow} key=${c.uid} card=${c} first=${i === 0} last=${i === list.length - 1}
            editing=${editingUid === c.uid} draft=${editDraft} noteEditing=${noteUid === c.uid} pages=${pagesByPath}
            onMove=${(dir) => update((cards) => moveCard(cards, c.uid, dir))}
            onMenu=${onMenu}
            onDraft=${setEditDraft} onCommit=${commitEdit}
            onCancel=${(remove) => { setEditingUid(null); if (remove) doRemove(c.uid); }}
            onStartNote=${() => setNoteUid(c.uid)}
            onCommitNote=${(note) => { setNoteUid(null); update((cards) => applyOutcome(cards, c.uid, 'changed', note)); }}
            onAddLink=${(card) => setPickTarget({ kind: 'card', uid: card.uid })}
            onRemoveLink=${(uid, path) => removeLinkFromCard(uid, path)} />`)
        : html`<div style=${{ padding: '14px 18px', fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic' }}>${section.hint}</div>`}
    </${Card}>`;
  }

  // Selected threads: document-level page references. Rendered like card links
  // so an unresolved path (page moved/deleted outside the app) stays visible.
  function selectedThreadsCard() {
    const list = draft?.selected_threads || [];
    return html`<${Card} title="Selected threads" bodyPad=${false} style=${{ overflow: 'visible' }}
      right=${html`<${Btn} kind="ghost" size="sm" icon="link" onClick=${() => setPickTarget({ kind: 'selected' })}>Link</${Btn}>`}>
      <div style=${{ padding: '12px 18px' }}>
        <div style=${{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 5 }}>
          ${list.map((p) => html`<${RefChip} key=${p} path=${p} pages=${pagesByPath} onRemove=${() => removeSelectedThread(p)} />`)}
          ${!list.length && html`<span style=${{ fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic' }}>No threads selected. Link pages from a card above, or here for the whole session.</span>`}
        </div>
        <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 8 }}>
          Threads the session should stay aware of. Linking never creates a page.
        </div>
      </div>
    </${Card}>`;
  }

  // "Ask Keeper for ideas" (SC-07): a compact affordance inside Prepare that
  // expands into a panel. Expanding never generates; the button does. Manual
  // cards stay fully usable while streaming and after a failure — only this
  // panel's contents change.
  function suggestionPanel() {
    if (!ideasOpen) {
      return html`<div style=${{ display: 'flex' }}>
        <${Btn} kind="ghost" size="sm" icon="sparkle" onClick=${() => setIdeasOpen(true)}>Ask Keeper for ideas</${Btn}>
      </div>`;
    }
    const streaming = suggest.phase === 'streaming';
    const streamLabel = SUGGEST_STAGE_LABEL[suggest.stage] || 'Thinking';
    // Undefined until /llm-providers loads: don't block on unknown state, let
    // the request surface the real error. Once known, guide the user to Settings.
    const providerReady = !Array.isArray(store.llmProviders)
      || store.llmProviders.some((p) => !p.needs_key || p.has_key);
    if (!providerReady) {
      return html`<${Card} title="Ask Keeper for ideas" bodyPad=${false}
        right=${html`<${Btn} kind="ghost" size="sm" onClick=${() => setIdeasOpen(false)}>Close</${Btn}>`}>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap', padding: '14px 18px', fontSize: 12.5, color: 'var(--ink-soft)' }}>
          <${Icon} name="sparkle" size=${14} style=${{ color: 'var(--ink-muted)' }} />
          <span style=${{ flex: 1, minWidth: 200, lineHeight: 1.5 }}>
            No AI provider is configured yet. Add a local Ollama model or a cloud key, then ask the Keeper for ideas here. Manual prep works either way.
          </span>
          <${Btn} kind="secondary" size="sm" icon="cog" onClick=${() => navigate('settings')}>Open Settings</${Btn}>
        </div>
      </${Card}>`;
    }
    const body = html`<div style=${{ padding: '12px 18px' }}>
      <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <input value=${focusText} placeholder="What should we focus on?" disabled=${streaming}
          onInput=${(e) => setFocusText(e.target.value)}
          onKeyDown=${(e) => { if (e.key === 'Enter' && !streaming) runSuggest(); }}
          style=${{ flex: 1, minWidth: 0, padding: '7px 10px', border: '1px solid var(--rule)', borderRadius: 5, background: 'var(--surface-raised)', color: 'var(--ink)', fontFamily: 'inherit', fontSize: 13, outline: 'none' }} />
        ${streaming
          ? html`<${Btn} kind="secondary" size="sm" onClick=${cancelSuggest}>Cancel</${Btn}>`
          : html`<${Btn} kind="primary" size="sm" icon="sparkle" onClick=${runSuggest}>${suggest.phase === 'idle' && suggest.items.length ? 'Regenerate' : 'Ask Keeper'}</${Btn}>`}
      </div>
      <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 6, lineHeight: 1.45 }}>
        Grounded in your selected threads, linked pages, and recent summaries. Nothing is added to prep until you choose it.
      </div>

      ${streaming && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 12, fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic' }}>
        <${Spinner} size=${12} /> ${streamLabel}…
      </div>`}

      ${suggest.phase === 'empty' && html`<div style=${{ marginTop: 12, fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic' }}>
        The Keeper had no suggestions this time. Manual prep below is unchanged.
      </div>`}

      ${suggest.phase === 'error' && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap', marginTop: 12, padding: '10px 12px', background: 'var(--burgundy-50)', border: '1px solid rgba(122,46,31,.22)', borderRadius: 7, fontSize: 12.5, color: 'var(--ink-soft)' }}>
        <${Icon} name="flame" size=${13} style=${{ color: 'var(--burgundy)' }} />
        <span style=${{ flex: 1, minWidth: 180, lineHeight: 1.45 }}>
          ${suggest.code === 'provider'
            ? 'No AI provider is configured yet. Set one up, then try again.'
            : `Keeper suggestions failed: ${suggest.error || 'unknown error'}`}
        </span>
        ${suggest.code === 'provider' && html`<${Btn} kind="ghost" size="sm" icon="cog" onClick=${() => navigate('settings')}>Open Settings</${Btn}>`}
        <${Btn} kind="secondary" size="sm" icon="undo" onClick=${runSuggest}>Retry</${Btn}>
      </div>`}

      ${suggest.items.length > 0 && html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 8, marginTop: 12 }}>
        ${suggest.items.slice(0, MAX_SUGGESTIONS).map((s) => html`<${SuggestionRow} key=${s.id} s=${s} pages=${pagesByPath}
          adjusting=${adjustId === s.id} adjustText=${adjustDraft}
          openingBlocked=${s.section === 'opening' && !canAcceptOpening(draft?.cards || [], s)}
          onStartAdjust=${() => startAdjust(s)}
          onAdjustDraft=${setAdjustDraft}
          onCommitAdjust=${commitAdjust}
          onCancelAdjust=${() => setAdjustId(null)}
          onAdd=${() => addSuggestion(s)}
          onReplace=${() => requestReplaceOpening(s)}
          onDismiss=${() => setSuggest((cur) => ({ ...cur, items: dismissSuggestion(cur.items, s.id) }))} />`)}
      </div>`}
    </div>`;
    return html`<${Card} title="Ask Keeper for ideas" bodyPad=${false}
      right=${html`<${Btn} kind="ghost" size="sm" onClick=${() => setIdeasOpen(false)}>Close</${Btn}>`}>${body}</${Card}>`;
  }

  const confirmBar = confirmReplace && html`<div style=${{ display: 'flex', alignItems: 'flex-start', gap: 12, flexWrap: 'wrap', marginBottom: 16, padding: '11px 14px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.28)', borderRadius: 8, fontSize: 12.5, color: 'var(--ink-soft)' }}>
    <${Icon} name="undo" size=${14} style=${{ color: 'var(--ochre)', marginTop: 2 }} />
    <div style=${{ flex: 1, minWidth: 200, lineHeight: 1.5 }}>
      <div style=${{ fontWeight: 500, marginBottom: 3 }}>Replace your current opening?</div>
      <div style=${{ color: 'var(--ink-muted)', whiteSpace: 'pre-wrap' }}>${confirmReplace.existing?.text || '(empty opening)'}</div>
    </div>
    <${Btn} kind="secondary" size="sm" onClick=${() => setConfirmReplace(null)}>Keep current</${Btn}>
    <${Btn} kind="primary" size="sm" icon="check" onClick=${confirmReplaceOpening}>Replace opening</${Btn}>
  </div>`;

  if (status === 'loading') {
    return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 9, padding: '30px 4px', color: 'var(--ink-muted)', fontStyle: 'italic' }}>
      <${Spinner} size=${14} /> Loading preparation…
    </div>`;
  }
  if (status === 'unavailable') {
    return html`<${Card} title="Preparation unavailable">
      <div style=${{ fontSize: 13, color: 'var(--ink-muted)', lineHeight: 1.6 }}>
        This session isn't attached to a world, so it has no preparation file. Add it to a world first, then prepare here.
      </div>
    </${Card}>`;
  }

  const indicator = status === 'saving'
    ? html`<span style=${{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, color: 'var(--ink-muted)' }}><${Spinner} size=${12} /> Saving…</span>`
    : status === 'saved'
      ? html`<span style=${{ display: 'flex', alignItems: 'center', gap: 5, fontSize: 12, color: 'var(--moss)' }}><${Icon} name="check" size=${12} /> Saved</span>`
      : status === 'error'
        ? html`<span style=${{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, color: 'var(--burgundy-700)' }}>
            Couldn't save
            <${Btn} kind="ghost" size="sm" onClick=${() => queueRef.current.schedule()}>Retry</${Btn}>
          </span>`
        : null;

  return html`<div style=${{ maxWidth: 760 }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 18 }}>
      <div style=${{ fontSize: 13, color: 'var(--ink-muted)', fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>
        Jot what might happen. Nothing here is fixed until it happens at the table.
      </div>
      <span style=${{ flex: 1 }} />
      ${indicator}
    </div>

    ${leaveIssue && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap', marginBottom: 16, padding: '11px 14px', background: 'var(--burgundy-50)', border: '1px solid rgba(122,46,31,.24)', borderRadius: 8, fontSize: 12.5, color: 'var(--ink-soft)' }}>
      <${Icon} name="flame" size=${14} style=${{ color: 'var(--burgundy)' }} />
      <span style=${{ flex: 1, minWidth: 200, lineHeight: 1.45 }}>${leaveIssue.message} You're still here so nothing is lost.</span>
      <${Btn} kind="ghost" size="sm" icon="copy" onClick=${() => copyText(serializeDraftText(draft.cards), 'Preparation copied')}>Copy my text</${Btn}>
      ${leaveIssue.conflict
        ? html`<${Btn} kind="secondary" size="sm" icon="undo" onClick=${async () => {
            const go = pendingNav.current;
            pendingNav.current = null;
            await reload();
            setLeaveIssue(null);
            if (go) go();
          }}>Reload saved version</${Btn}>`
        : html`
          <${Btn} kind="ghost" size="sm" onClick=${() => { const go = pendingNav.current; pendingNav.current = null; queueRef.current.markClean(); setLeaveIssue(null); if (go) go(); }}>Leave without saving</${Btn}>
          <${Btn} kind="primary" size="sm" onClick=${async () => {
            const q = queueRef.current;
            const go = pendingNav.current;
            pendingNav.current = null;
            setLeaveIssue(null);
            const ok = await q.flush();
            if (ok && go) go();
            else setLeaveIssue({ conflict: q.isConflict(), message: q.isConflict() ? 'The saved preparation changed elsewhere.' : 'Could not save before leaving.' });
          }}>Retry save</${Btn}>`}
    </div>`}

    ${status === 'conflict' && !leaveIssue && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap', marginBottom: 16, padding: '11px 14px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.28)', borderRadius: 8, fontSize: 12.5, color: 'var(--ink-soft)' }}>
      <${Icon} name="flame" size=${14} style=${{ color: 'var(--ochre)' }} />
      <span style=${{ flex: 1, minWidth: 200, lineHeight: 1.45 }}>The saved preparation changed elsewhere. Your draft is kept — copy it somewhere safe, or reload the saved version (this discards your draft).</span>
      <${Btn} kind="ghost" size="sm" icon="copy" onClick=${() => copyText(serializeDraftText(draft.cards), 'Preparation copied')}>Copy my text</${Btn}>
      <${Btn} kind="secondary" size="sm" icon="undo" onClick=${() => reload()}>Reload saved version</${Btn}>
    </div>`}

    <div class="ck-sr-only" role="status" aria-live="polite">${announce}</div>

    ${confirmBar}

    <div style=${{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      ${PREP_SECTIONS.map(sectionCard)}
      ${selectedThreadsCard()}
      ${suggestionPanel()}
    </div>

    ${pickTarget && html`<div style=${{ marginTop: 12 }}>
      <div style=${{ fontSize: 11.5, color: 'var(--ink-muted)', marginBottom: 2 }}>
        Link to ${pickTarget.kind === 'card' ? 'this card' : 'selected threads'} — choosing an existing page, or create a thread:
      </div>
      <${PagePicker} pages=${store.vaultPages || []} onPick=${(path) => {
        if (pickTarget.kind === 'card') addLinkToCard(pickTarget.uid, path); else addSelectedThread(path);
        setPickTarget(null);
      }} onClose=${() => setPickTarget(null)} />
      <div style=${{ marginTop: 6 }}>
        <${Btn} kind="ghost" size="sm" icon="feather" onClick=${createThread}>Create thread…</${Btn}>
        <${Btn} kind="ghost" size="sm" onClick=${() => setPickTarget(null)}>Cancel</${Btn}>
      </div>
    </div>`}

    ${removed && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 16, padding: '9px 12px', background: 'var(--paper-deep)', border: '1px solid var(--rule-soft)', borderRadius: 6, fontSize: 12.5, color: 'var(--ink-muted)' }}>
      <${Icon} name="trash" size=${12} /> Removed “${removed.card.text || 'card'}”.
      <span style=${{ flex: 1 }} />
      <${Btn} kind="ghost" size="sm" icon="undo" onClick=${undoRemove}>Undo</${Btn}>
    </div>`}
  </div>`;
}
