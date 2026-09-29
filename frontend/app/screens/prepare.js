// Session Prepare — a lens over the session's prep page. The page (`kind: prep`)
// is an ordinary Codex page written in the editor on the right (or in
// Obsidian); the outline on the left reads it back as cards and applies small
// edits — outcomes, links, threads, Keeper suggestions — through
// POST /sessions/:id/prep/ops. Every op first flushes the editor, then reloads
// it, so the page text stays the single source of truth.
import { html, useState, useEffect, useRef } from '../../vendor/htm-preact-standalone.mjs';
import { loadVaultTree, uploadVaultAsset, loadSnippets } from '../actions.js';
import { useStore, navigate, apiFetch, apiJson } from '../core.js';
import { Btn, Card, Icon, Spinner } from '../ui.js';
import { iconForKind, makeVaultActions } from './codex.js';
import { openPageEvt } from '../tabs.js';
import { mountEditor } from '../cm.js';
import { setEditorActive } from '../commands.js';
import { PREP_SECTIONS, cardsInSection, loadPrep, prepOps } from '../prep.js';
import { MAX_SUGGESTIONS, suggestPrep, canAcceptOpening, dismissSuggestion } from '../prepSuggest.js';

// A page reference chip. Existing pages open on click; a path with no page in
// the vault index renders as visibly unresolved — never guessed.
function RefChip({ path, pages }) {
  const p = pages.get(path);
  const unresolved = !p;
  const title = p?.title || path.replace(/\.md$/, '');
  return html`<span title=${unresolved ? `Not in the vault: ${path}` : path}
    onClick=${unresolved ? null : (e) => openPageEvt(path, e)}
    style=${{ display: 'inline-flex', alignItems: 'center', gap: 4, maxWidth: '100%', padding: '1px 7px', borderRadius: 999,
      background: unresolved ? 'var(--ochre-50)' : 'var(--paper-deep)',
      border: `1px solid ${unresolved ? 'rgba(168,115,40,.4)' : 'var(--rule-soft)'}`,
      color: unresolved ? 'var(--ochre)' : 'var(--ink-soft)', fontSize: 11, cursor: unresolved ? 'default' : 'pointer' }}>
    <${Icon} name=${unresolved ? 'flame' : iconForKind(p.kind)} size=${10} />
    <span style=${{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${title}</span>
  </span>`;
}

// Pick an existing page (never creates one).
function PagePicker({ pages, placeholder, onPick, onClose }) {
  const [q, setQ] = useState('');
  const ql = q.trim().toLowerCase();
  const matches = (pages || [])
    .filter((p) => !ql || p.title.toLowerCase().includes(ql) || p.path.toLowerCase().includes(ql) || (p.aliases || []).some((a) => a.toLowerCase().includes(ql)))
    .sort((a, b) => a.title.localeCompare(b.title))
    .slice(0, 20);
  return html`<div style=${{ border: '1px solid var(--rule)', borderRadius: 7, background: 'var(--surface-raised)', padding: 6, marginTop: 6 }}>
    <input autofocus value=${q} placeholder=${placeholder || 'Find an existing page…'}
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
        : html`<div style=${{ padding: '5px 7px', fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>No matching page.</div>`}
    </div>
    <div style=${{ marginTop: 4 }}><${Btn} kind="ghost" size="sm" onClick=${onClose}>Cancel</${Btn}></div>
  </div>`;
}

// The singleton CodeMirror editor over the prep page (TemplateEditor pattern).
// `ctlRef` exposes flush() so outline ops never race unsaved typing.
function PrepEditor({ content, cacheKey, pages, snippets, onSave, onState, ctlRef }) {
  const hostRef = useRef(null);
  const pagesRef = useRef(pages); pagesRef.current = pages;
  const snippetsRef = useRef(snippets); snippetsRef.current = snippets;
  useEffect(() => {
    let ctl = null, dead = false;
    mountEditor(hostRef.current, {
      doc: content,
      cacheKey,
      getPages: () => pagesRef.current,
      getSnippets: () => snippetsRef.current,
      onUploadAsset: uploadVaultAsset,
      onSave,
      onState,
    }).then((c) => { if (dead) c.destroy(); else { ctl = c; ctlRef.current = c; } });
    setEditorActive(true);
    return () => {
      dead = true;
      if (ctlRef.current === ctl) ctlRef.current = null;
      if (ctl) ctl.destroy();
      setEditorActive(false);
    };
  }, [cacheKey]);
  return html`<div ref=${hostRef} class="ck-cm" style=${{ minHeight: '55vh' }} />`;
}

const OUTCOME_BUTTONS = [
  { key: 'happened', icon: 'check', label: 'Happened' },
  { key: 'changed', icon: 'edit', label: 'Changed' },
  { key: 'unused', icon: 'x', label: 'Unused' },
];

function OutlineCard({ card, pages, busy, onOutcome, onNote }) {
  const [note, setNote] = useState(null);
  const label = card.title || (card.text || '').split('\n')[0] || 'Untitled';
  return html`<div style=${{ padding: '7px 0', borderTop: '1px solid var(--rule-soft)' }}>
    <div style=${{ display: 'flex', alignItems: 'flex-start', gap: 6 }}>
      <div style=${{ flex: 1, minWidth: 0, fontSize: 12.5, lineHeight: 1.4, color: card.outcome === 'unused' ? 'var(--ink-faint)' : 'var(--ink)',
        textDecoration: card.outcome === 'unused' ? 'line-through' : 'none' }}>${label}</div>
      <div style=${{ display: 'flex', gap: 1, flex: '0 0 auto' }}>
        ${OUTCOME_BUTTONS.map((o) => {
          const on = card.outcome === o.key;
          return html`<button type="button" key=${o.key} disabled=${busy} aria-pressed=${on}
            title=${on ? `${o.label} — click to clear` : `Mark ${o.label.toLowerCase()}`}
            onClick=${() => { onOutcome(on ? 'unmarked' : o.key); if (!on && o.key === 'changed') setNote(card.outcome_note || ''); }}
            style=${{ display: 'flex', alignItems: 'center', justifyContent: 'center', width: 22, height: 22, padding: 0, borderRadius: 4,
              border: `1px solid ${on ? 'var(--rule-strong)' : 'transparent'}`, background: on ? 'var(--paper-deep)' : 'transparent',
              color: on ? 'var(--ink)' : 'var(--ink-ghost)', cursor: busy ? 'default' : 'pointer' }}>
            <${Icon} name=${o.icon} size=${11} />
          </button>`;
        })}
      </div>
    </div>
    ${card.outcome === 'changed' && (note !== null
      ? html`<input autofocus value=${note} placeholder="What changed?" onInput=${(e) => setNote(e.target.value)}
          onBlur=${() => { onNote(note); setNote(null); }}
          onKeyDown=${(e) => { if (e.key === 'Enter') { onNote(note); setNote(null); } else if (e.key === 'Escape') setNote(null); }}
          style=${{ width: '100%', boxSizing: 'border-box', marginTop: 4, padding: '3px 7px', border: '1px solid var(--rule)', borderRadius: 4, background: 'var(--surface-raised)', color: 'var(--ink)', fontFamily: 'inherit', fontSize: 12, outline: 'none' }} />`
      : html`<div onClick=${() => setNote(card.outcome_note || '')} style=${{ marginTop: 3, fontSize: 11.5, fontStyle: 'italic', color: card.outcome_note ? 'var(--ink-muted)' : 'var(--burgundy)', cursor: 'pointer' }}>
          ${card.outcome_note || 'Add what changed'}
        </div>`)}
    ${(card.links || []).length > 0 && html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 3, marginTop: 4 }}>
      ${card.links.map((lp) => html`<${RefChip} key=${lp} path=${lp} pages=${pages} />`)}
    </div>`}
  </div>`;
}

const SUGGEST_STAGE_LABEL = { reading: 'Reading session', grounding: 'Checking sources', building: 'Preparing ideas' };

// One Keeper suggestion. Ephemeral until added; an `is_idea` suggestion is
// labeled as a creative idea, never presented as fact.
function SuggestionRow({ s, pages, busy, openingBlocked, onAdd, onReplace, onDismiss }) {
  const [expanded, setExpanded] = useState(false);
  const section = PREP_SECTIONS.find((x) => x.key === s.section);
  const links = s.links || [];
  return html`<div style=${{ border: '1px solid var(--rule-soft)', borderRadius: 7, background: 'var(--surface-raised)', padding: '10px 12px' }}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap', marginBottom: 6 }}>
      <span style=${{ fontSize: 10, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase', color: 'var(--ink-faint)', border: '1px solid var(--rule-soft)', borderRadius: 999, padding: '1px 7px' }}>${section?.label || s.section}</span>
      ${s.is_idea && html`<span title="A creative prompt, not a fact from your notes" style=${{ display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 10.5, fontWeight: 600, letterSpacing: '0.03em', textTransform: 'uppercase', color: 'var(--ochre)', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.3)', borderRadius: 999, padding: '1px 7px' }}>
        <${Icon} name="sparkle" size=${10} /> Creative idea
      </span>`}
    </div>
    ${s.title && html`<div style=${{ fontFamily: 'var(--font-display)', fontWeight: 500, fontSize: 13.5, color: 'var(--ink)', marginBottom: 2 }}>${s.title}</div>`}
    <div style=${{ fontSize: 13, color: 'var(--ink)', lineHeight: 1.5, whiteSpace: 'pre-wrap' }}>${s.text}</div>
    ${(s.rationale || links.length > 0) && html`<div style=${{ marginTop: 7 }}>
      <button type="button" aria-expanded=${expanded} onClick=${() => setExpanded((v) => !v)}
        style=${{ display: 'inline-flex', alignItems: 'center', gap: 5, padding: 0, border: 'none', background: 'none', color: 'var(--ink-muted)', fontSize: 11.5, cursor: 'pointer' }}>
        <${Icon} name=${expanded ? 'chev-d' : 'chev-r'} size=${11} /> Why this fits
      </button>
      ${expanded && html`<div style=${{ marginTop: 6, paddingLeft: 16 }}>
        ${s.rationale && html`<div style=${{ fontSize: 12, color: 'var(--ink-muted)', lineHeight: 1.5, fontStyle: 'italic' }}>${s.rationale}</div>`}
        ${links.length > 0 && html`<div style=${{ display: 'flex', flexWrap: 'wrap', gap: 4, marginTop: 6 }}>
          ${links.map((p) => html`<${RefChip} key=${p} path=${p} pages=${pages} />`)}
        </div>`}
      </div>`}
    </div>`}
    <div style=${{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 9 }}>
      ${openingBlocked
        ? html`<${Btn} kind="secondary" size="sm" icon="undo" disabled=${busy} onClick=${onReplace}>Replace opening</${Btn}>`
        : html`<${Btn} kind="secondary" size="sm" icon="plus" disabled=${busy} onClick=${onAdd}>Add to prep</${Btn}>`}
      <span style=${{ flex: 1 }} />
      <${Btn} kind="ghost" size="sm" onClick=${onDismiss}>Dismiss</${Btn}>
    </div>
  </div>`;
}

export function SessionPrepare({ session, campaign }) {
  const sessionId = session?.session_id;
  const campaignId = campaign?.campaign_id || session?.campaign?.campaign_id;
  const store = useStore();
  // { revision, page, cards, selected_threads, notes } + `content` of the page.
  const [prep, setPrep] = useState(null);
  const [status, setStatus] = useState('loading'); // loading | ready | unavailable | error
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);
  const [editorGen, setEditorGen] = useState(0);
  const [saveState, setSaveState] = useState('saved');
  const [picker, setPicker] = useState(null);       // 'adopt' | 'thread' | null
  const [ideasOpen, setIdeasOpen] = useState(false);
  const [suggest, setSuggest] = useState({ phase: 'idle', items: [] });
  const [focusText, setFocusText] = useState('');
  const [confirmReplace, setConfirmReplace] = useState(null);
  const ctlRef = useRef(null);
  const prepRef = useRef(null); prepRef.current = prep;
  const saveStateRef = useRef(saveState); saveStateRef.current = saveState;
  const suggestAbortRef = useRef(null);
  const refreshTimer = useRef(null);

  async function fetchAll() {
    const loaded = await loadPrep(sessionId);
    let content = '';
    if (loaded.page) {
      const page = await apiFetch(`/campaigns/${campaignId}/vault/pages/${encodeURI(loaded.page)}`);
      content = page.content;
    }
    return { ...loaded, content };
  }

  // Full reload: outline and editor. `remount` swaps in the on-disk text.
  async function reload({ remount = true } = {}) {
    if (!sessionId || !campaignId) { setStatus('unavailable'); return; }
    try {
      const next = await fetchAll();
      const changed = !prepRef.current || prepRef.current.content !== next.content || prepRef.current.page !== next.page;
      setPrep(next);
      setStatus('ready');
      if (remount && changed) setEditorGen((g) => g + 1);
    } catch (e) {
      setStatus(e.status === 422 || e.status === 404 ? 'unavailable' : 'error');
      setError(e.message);
    }
  }

  useEffect(() => {
    setPrep(null);
    setStatus('loading');
    setSuggest({ phase: 'idle', items: [] });
    setFocusText('');
    setConfirmReplace(null);
    setPicker(null);
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, campaignId]);

  useEffect(() => () => {
    const c = suggestAbortRef.current;
    suggestAbortRef.current = null;
    if (c) c.abort();
  }, [sessionId]);

  useEffect(() => {
    if (campaignId && !(store.vaultPages || []).length) loadVaultTree(campaignId);
    if (campaignId && !(store.snippets || []).length) loadSnippets(campaignId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [campaignId]);

  // Edited outside the app (Obsidian, the Keeper): pick it up unless the GM is
  // mid-typing — their unsaved text wins until it saves.
  useEffect(() => {
    if (store.dirty_vault && status === 'ready' && saveStateRef.current === 'saved') reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [store.dirty_vault]);

  const pages = store.vaultPages || [];
  const pagesByPath = new Map(pages.map((p) => [p.path, p]));
  const vaultAct = makeVaultActions(campaign, store.vaultFolders || []);

  // Editor save = ordinary page save; the outline re-reads shortly after.
  async function saveContent(content) {
    const page = prepRef.current?.page;
    if (!page) return;
    await apiJson(`/campaigns/${campaignId}/vault/pages/${encodeURI(page)}`, 'PUT', { content });
    setPrep((cur) => (cur ? { ...cur, content } : cur));
    clearTimeout(refreshTimer.current);
    refreshTimer.current = setTimeout(async () => {
      try {
        const loaded = await loadPrep(sessionId);
        setPrep((cur) => (cur ? { ...cur, ...loaded } : cur));
      } catch (_) { /* outline refresh is best effort */ }
    }, 250);
  }

  // Flush the editor, apply `ops` against the current revision, then reload
  // the editor from disk. A 409 means the page changed under us: reload, keep
  // nothing half-applied, and say so.
  async function runOps(ops) {
    if (!sessionId || busy) return false;
    setBusy(true);
    setError(null);
    try {
      if (ctlRef.current && !(await ctlRef.current.flush())) throw new Error('Could not save the page first.');
      const current = await loadPrep(sessionId);
      const hadPage = !!current.page;
      await prepOps(sessionId, current.revision, ops);
      await reload();
      if (!hadPage) loadVaultTree(campaignId);
      return true;
    } catch (e) {
      setError(e.status === 409 ? 'The prep page changed while saving — reloaded it, please try again.' : e.message);
      await reload();
      return false;
    } finally {
      setBusy(false);
    }
  }

  // ── Keeper suggestions (SC-07). Always an explicit click. ──────────
  async function runSuggest() {
    if (!sessionId || suggestAbortRef.current) return;
    const controller = new AbortController();
    suggestAbortRef.current = controller;
    setSuggest((cur) => ({ phase: 'streaming', stage: 'reading', items: cur.items || [] }));
    try {
      const res = await suggestPrep(sessionId, {
        instruction: focusText,
        linkedPaths: prepRef.current?.selected_threads || [],
        onEvent: (ev) => {
          if (ev?.stage === 'reading' || ev?.stage === 'grounding' || ev?.stage === 'building') {
            setSuggest((cur) => ({ ...cur, stage: ev.stage }));
          }
        },
      }, { signal: controller.signal });
      if (suggestAbortRef.current !== controller) return;
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

  async function addSuggestion(s) {
    const ok = await runOps([{ op: 'add_card', section: s.section, title: s.title || null, text: s.text || '', links: s.links || [] }]);
    if (ok) setSuggest((cur) => ({ ...cur, phase: cur.phase === 'streaming' ? cur.phase : 'idle', items: dismissSuggestion(cur.items, s.id) }));
  }

  async function confirmReplaceOpening() {
    const pending = confirmReplace;
    setConfirmReplace(null);
    if (!pending) return;
    const ok = await runOps([{ op: 'replace_opening', text: pending.suggestion.text || '' }]);
    if (ok) setSuggest((cur) => ({ ...cur, items: dismissSuggestion(cur.items, pending.suggestion.id) }));
  }

  function suggestionPanel() {
    if (!ideasOpen) {
      return html`<${Btn} kind="ghost" size="sm" icon="sparkle" onClick=${() => setIdeasOpen(true)}>Ask Keeper for ideas</${Btn}>`;
    }
    const streaming = suggest.phase === 'streaming';
    const providerReady = !Array.isArray(store.llmProviders)
      || store.llmProviders.some((p) => !p.needs_key || p.has_key);
    const close = html`<${Btn} kind="ghost" size="sm" onClick=${() => setIdeasOpen(false)}>Close</${Btn}>`;
    if (!providerReady) {
      return html`<${Card} title="Ask Keeper for ideas" right=${close}>
        <div style=${{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap', fontSize: 12.5, color: 'var(--ink-soft)' }}>
          <span style=${{ flex: 1, minWidth: 200, lineHeight: 1.5 }}>No AI provider is configured yet. Manual prep works either way.</span>
          <${Btn} kind="secondary" size="sm" icon="cog" onClick=${() => navigate('settings')}>Open Settings</${Btn}>
        </div>
      </${Card}>`;
    }
    return html`<${Card} title="Ask Keeper for ideas" right=${close}>
      <div style=${{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <input value=${focusText} placeholder="What should we focus on?" disabled=${streaming}
          onInput=${(e) => setFocusText(e.target.value)}
          onKeyDown=${(e) => { if (e.key === 'Enter' && !streaming) runSuggest(); }}
          style=${{ flex: 1, minWidth: 0, padding: '7px 10px', border: '1px solid var(--rule)', borderRadius: 5, background: 'var(--surface-raised)', color: 'var(--ink)', fontFamily: 'inherit', fontSize: 13, outline: 'none' }} />
        ${streaming
          ? html`<${Btn} kind="secondary" size="sm" onClick=${cancelSuggest}>Cancel</${Btn}>`
          : html`<${Btn} kind="primary" size="sm" icon="sparkle" onClick=${runSuggest}>${suggest.items.length ? 'Regenerate' : 'Ask Keeper'}</${Btn}>`}
      </div>
      <div style=${{ fontSize: 11.5, color: 'var(--ink-faint)', marginTop: 6, lineHeight: 1.45 }}>
        Grounded in your threads, linked pages, and recent summaries. Nothing is written until you add it.
      </div>
      ${streaming && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 12, fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic' }}>
        <${Spinner} size=${12} /> ${SUGGEST_STAGE_LABEL[suggest.stage] || 'Thinking'}…
      </div>`}
      ${suggest.phase === 'empty' && html`<div style=${{ marginTop: 12, fontSize: 12.5, color: 'var(--ink-muted)', fontStyle: 'italic' }}>The Keeper had no suggestions this time.</div>`}
      ${suggest.phase === 'error' && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap', marginTop: 12, padding: '10px 12px', background: 'var(--burgundy-50)', border: '1px solid rgba(122,46,31,.22)', borderRadius: 7, fontSize: 12.5, color: 'var(--ink-soft)' }}>
        <span style=${{ flex: 1, minWidth: 180, lineHeight: 1.45 }}>
          ${suggest.code === 'provider' ? 'No AI provider is configured yet. Set one up, then try again.' : `Keeper suggestions failed: ${suggest.error || 'unknown error'}`}
        </span>
        <${Btn} kind="secondary" size="sm" icon="undo" onClick=${runSuggest}>Retry</${Btn}>
      </div>`}
      ${suggest.items.length > 0 && html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 8, marginTop: 12 }}>
        ${suggest.items.slice(0, MAX_SUGGESTIONS).map((s) => html`<${SuggestionRow} key=${s.id} s=${s} pages=${pagesByPath} busy=${busy}
          openingBlocked=${s.section === 'opening' && !canAcceptOpening(prep?.cards || [], s)}
          onAdd=${() => addSuggestion(s)}
          onReplace=${() => setConfirmReplace({ suggestion: s, existing: (prep?.cards || []).find((c) => c.section === 'opening') || null })}
          onDismiss=${() => setSuggest((cur) => ({ ...cur, items: dismissSuggestion(cur.items, s.id) }))} />`)}
      </div>`}
    </${Card}>`;
  }

  if (status === 'loading') {
    return html`<div style=${{ display: 'flex', alignItems: 'center', gap: 9, padding: '30px 4px', color: 'var(--ink-muted)', fontStyle: 'italic' }}>
      <${Spinner} size=${14} /> Loading preparation…
    </div>`;
  }
  if (status === 'unavailable') {
    return html`<${Card} title="Preparation unavailable">
      <div style=${{ fontSize: 13, color: 'var(--ink-muted)', lineHeight: 1.6 }}>
        This session isn't attached to a world, so it has no prep page. Add it to a world first, then prepare here.
      </div>
    </${Card}>`;
  }
  if (status === 'error') {
    return html`<${Card} title="Could not load preparation">
      <div style=${{ fontSize: 13, color: 'var(--ink-muted)' }}>${error}</div>
      <div style=${{ marginTop: 10 }}><${Btn} kind="secondary" size="sm" icon="undo" onClick=${() => reload()}>Retry</${Btn}></div>
    </${Card}>`;
  }

  const banner = error && html`<div style=${{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 14, padding: '9px 12px', background: 'var(--burgundy-50)', border: '1px solid rgba(122,46,31,.22)', borderRadius: 7, fontSize: 12.5, color: 'var(--ink-soft)' }}>
    <${Icon} name="flame" size=${13} style=${{ color: 'var(--burgundy)' }} />
    <span style=${{ flex: 1 }}>${error}</span>
    <${Btn} kind="ghost" size="sm" onClick=${() => setError(null)}>Dismiss</${Btn}>
  </div>`;

  const confirmBar = confirmReplace && html`<div style=${{ display: 'flex', alignItems: 'flex-start', gap: 12, flexWrap: 'wrap', marginBottom: 14, padding: '11px 14px', background: 'var(--ochre-50)', border: '1px solid rgba(168,115,40,.28)', borderRadius: 8, fontSize: 12.5, color: 'var(--ink-soft)' }}>
    <div style=${{ flex: 1, minWidth: 200, lineHeight: 1.5 }}>
      <div style=${{ fontWeight: 500, marginBottom: 3 }}>Replace your current opening?</div>
      <div style=${{ color: 'var(--ink-muted)', whiteSpace: 'pre-wrap' }}>${confirmReplace.existing?.text || '(empty opening)'}</div>
    </div>
    <${Btn} kind="secondary" size="sm" onClick=${() => setConfirmReplace(null)}>Keep current</${Btn}>
    <${Btn} kind="primary" size="sm" icon="check" onClick=${confirmReplaceOpening}>Replace opening</${Btn}>
  </div>`;

  // Not prepared yet: start a page, or adopt one the GM already wrote.
  if (!prep?.page) {
    return html`<div style=${{ maxWidth: 760 }}>
      ${banner}
      <${Card} title="Prepare this session">
        <div style=${{ fontSize: 13, color: 'var(--ink-soft)', lineHeight: 1.6 }}>
          Prep is a page in your Codex — write it here, in the page editor, or in Obsidian. Nothing in it counts as canon until it happens at the table.
        </div>
        <div style=${{ display: 'flex', gap: 8, marginTop: 12, flexWrap: 'wrap' }}>
          <${Btn} kind="primary" size="sm" icon="plus" disabled=${busy} onClick=${() => runOps([{ op: 'create' }])}>Start prep page</${Btn}>
          <${Btn} kind="secondary" size="sm" icon="link" disabled=${busy} onClick=${() => setPicker('adopt')}>Use an existing page…</${Btn}>
        </div>
        ${picker === 'adopt' && html`<${PagePicker} pages=${pages} placeholder="Find the page you prepared in…"
          onPick=${(path) => { setPicker(null); runOps([{ op: 'adopt', page: path }]); }} onClose=${() => setPicker(null)} />`}
      </${Card}>
      <div style=${{ marginTop: 16 }}>${confirmBar}${suggestionPanel()}</div>
    </div>`;
  }

  const indicator = saveState === 'saving' || busy
    ? html`<span style=${{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, color: 'var(--ink-muted)' }}><${Spinner} size=${12} /> Saving…</span>`
    : saveState === 'dirty'
      ? html`<span style=${{ fontSize: 12, color: 'var(--ink-faint)' }}>Unsaved</span>`
      : html`<span style=${{ display: 'flex', alignItems: 'center', gap: 5, fontSize: 12, color: 'var(--moss)' }}><${Icon} name="check" size=${12} /> Saved</span>`;

  const threads = prep.selected_threads || [];
  const outline = html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 14 }}>
    ${PREP_SECTIONS.map((section) => {
      const list = cardsInSection(prep.cards, section.key);
      return html`<div key=${section.key}>
        <div style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)', marginBottom: 2 }}>${section.label}</div>
        ${list.length
          ? list.map((c) => html`<${OutlineCard} key=${c.id} card=${c} pages=${pagesByPath} busy=${busy}
              onOutcome=${(outcome) => runOps([{ op: 'set_outcome', id: c.id, outcome, note: outcome === 'changed' ? (c.outcome_note || '') : '' }])}
              onNote=${(note) => { if (note !== (c.outcome_note || '')) runOps([{ op: 'set_outcome', id: c.id, outcome: 'changed', note }]); }} />`)
          : html`<div style=${{ fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic', padding: '4px 0' }}>${section.hint}</div>`}
      </div>`;
    })}
    <div>
      <div style=${{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
        <span style=${{ fontSize: 10.5, fontWeight: 600, letterSpacing: '0.1em', textTransform: 'uppercase', color: 'var(--ink-faint)' }}>Threads in focus</span>
        <span style=${{ flex: 1 }} />
        <${Btn} kind="ghost" size="sm" icon="link" disabled=${busy} onClick=${() => setPicker('thread')}>Link</${Btn}>
      </div>
      <div style=${{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
        ${threads.map((p) => html`<${RefChip} key=${p} path=${p} pages=${pagesByPath} />`)}
        ${!threads.length && html`<span style=${{ fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic' }}>None yet.</span>`}
      </div>
      ${picker === 'thread' && html`<${PagePicker} pages=${pages} placeholder="Find a thread or page…"
        onPick=${(path) => { setPicker(null); runOps([{ op: 'add_thread', page: path }]); }} onClose=${() => setPicker(null)} />`}
      ${picker === 'thread' && html`<div style=${{ marginTop: 4 }}>
        <${Btn} kind="ghost" size="sm" icon="feather" onClick=${() => { setPicker(null); vaultAct.newThread(null, (p) => runOps([{ op: 'add_thread', page: p.path }])); }}>Create thread…</${Btn}>
      </div>`}
    </div>
    <div style=${{ fontSize: 11, color: 'var(--ink-faint)', lineHeight: 1.5 }}>
      Mark outcomes here or by hand: <span style=${{ fontFamily: 'var(--font-mono)' }}>- [x]</span> happened,
      <span style=${{ fontFamily: 'var(--font-mono)' }}> [~]</span> changed, <span style=${{ fontFamily: 'var(--font-mono)' }}> [-]</span> unused;
      scenes take a <span style=${{ fontFamily: 'var(--font-mono)' }}>> outcome: …</span> line.
    </div>
  </div>`;

  return html`<div>
    ${banner}
    ${confirmBar}
    <div style=${{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 12, flexWrap: 'wrap' }}>
      <button type="button" title="Open in the Codex" onClick=${(e) => openPageEvt(prep.page, e)}
        style=${{ display: 'inline-flex', alignItems: 'center', gap: 6, padding: '3px 9px', border: '1px solid var(--rule-soft)', borderRadius: 999, background: 'var(--paper-deep)', color: 'var(--ink-soft)', fontFamily: 'var(--font-mono)', fontSize: 11.5, cursor: 'pointer' }}>
        <${Icon} name="feather" size=${11} /> ${prep.page}
      </button>
      <span style=${{ fontSize: 12.5, color: 'var(--ink-muted)', fontFamily: 'var(--font-display)', fontStyle: 'italic' }}>
        Nothing here is fixed until it happens at the table.
      </span>
      <span style=${{ flex: 1 }} />
      ${indicator}
    </div>
    <div style=${{ marginBottom: 14 }}>${suggestionPanel()}</div>
    <div style=${{ display: 'flex', gap: 20, alignItems: 'flex-start', flexWrap: 'wrap' }}>
      <div style=${{ flex: '0 1 280px', minWidth: 240 }}>${outline}</div>
      <div style=${{ flex: '1 1 480px', minWidth: 320 }}>
        <${PrepEditor} key=${`${prep.page}:${editorGen}`} content=${prep.content}
          cacheKey=${`prep:${campaignId}:${prep.page}:${editorGen}`}
          pages=${pages} snippets=${store.snippets || []} ctlRef=${ctlRef}
          onSave=${saveContent} onState=${setSaveState} />
      </div>
    </div>
  </div>`;
}
