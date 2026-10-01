// Overview "Unfinished" card: what the Keeper sees as open — stubs, `[?]`
// markers, broken links, cold threads — with per-row dismiss (per world, in
// .ck/prefs.json) and one-click hand-off to a Keeper chat.
import { html, useState, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { navigate, useStore } from './core.js';
import { Icon } from './ui.js';
import { loadGaps, loadPrefs, loadSkills, dismissGap, restoreGaps, createVaultPage } from './actions.js';
import { newChat, sendMessage, runSkillChat } from './keeperPanel.js';
import { KINDS, iconForKind } from './screens/codex.js';
import { GAP_TABS, gapCounts, visibleGaps, skillForGap } from './worldPrefs.js';

const LIMIT = 4;
const TAB_LABEL = { all: 'All', stubs: 'Stubs', open: 'Open ?', links: 'Links' };

function ask(row, skills) {
  const ref = `[[${row.title}]]`;
  const text = row.stub
    ? `${ref} is still a stub. Help me flesh it out.`
    : row.open_questions
      ? `${ref} has ${row.open_questions} open [?] marker${row.open_questions === 1 ? '' : 's'}. Help me resolve them.`
      : `${ref} hasn't been touched in ${row.stale_days} days. Check whether it needs an update.`;
  const skill = skillForGap(row, skills);
  if (skill) return runSkillChat(skill.name, text);
  return newChat().then((id) => { if (id) { navigate('keeper'); sendMessage(text); } });
}

function badges(row) {
  const b = [];
  if (row.stub) b.push('stub');
  if (row.open_questions) b.push(`${row.open_questions} open ?`);
  if (row.stale_days) b.push(`not edited in ${row.stale_days} days`);
  return b;
}

function GapRow({ row, skills, onCreate }) {
  const isLink = row.row === 'link';
  const kindLabel = isLink
    ? `Broken link · from ${row.from_count} page${row.from_count === 1 ? '' : 's'}`
    : (KINDS.find((k) => k.value === row.kind) || {}).label || '';
  const action = isLink ? 'Create page →' : row.stub ? 'Flesh out →' : 'Ask Keeper →';
  const run = () => (isLink ? onCreate(row) : ask(row, skills));
  return html`<div class="ck-gap-row">
    <${Icon} name=${isLink ? 'doc' : iconForKind(row.kind)} size=${14} className="ck-ink-muted" />
    <div style=${{ flex: 1, minWidth: 0, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
      <span onClick=${() => (isLink ? onCreate(row) : navigate('page', { path: row.path }))}
        style=${{ fontFamily: 'var(--font-display)', fontSize: 15, cursor: 'pointer' }}>${row.title}</span>
      ${kindLabel && html` <span style=${{ fontSize: 12, color: 'var(--ink-muted)' }}>${kindLabel}</span>`}
    </div>
    ${badges(row).map((b) => html`<span key=${b} class="chip chip-ochre" style=${{ fontSize: 11 }}>${b}</span>`)}
    <span onClick=${run} style=${{ fontSize: 12, color: 'var(--burgundy)', whiteSpace: 'nowrap', cursor: 'pointer' }}>${action}</span>
    <span class="ck-gap-x" title="Dismiss" onClick=${() => dismissGap(row.key)}><${Icon} name="x" size=${11} /></span>
  </div>`;
}

export function UnfinishedCard({ campaign }) {
  const s = useStore();
  const cid = campaign.campaign_id;
  const [tab, setTab] = useState('all');
  const [all, setAll] = useState(false);

  useEffect(() => { loadPrefs(cid); loadSkills(cid); }, [cid]);
  useEffect(() => { loadGaps(cid); }, [cid, s.vaultPages]);

  const rows = s.vaultGapsFor === cid ? (s.vaultGaps || []) : [];
  const prefsReady = s.worldPrefs?.campaignId === cid;
  const dismissed = prefsReady ? s.worldPrefs.gaps_dismissed : [];
  const counts = gapCounts(rows, dismissed);
  const list = visibleGaps(rows, dismissed, tab);
  const shown = all ? list : list.slice(0, LIMIT);
  const hiddenByUser = rows.length - gapCounts(rows, dismissed).all;

  const create = async (row) => {
    try {
      const p = await createVaultPage(row.title, 'lore', '');
      navigate('page', { path: p.path });
    } catch (_) { /* name clash or unwritable: leave the row */ }
  };

  if (!rows.length && s.vaultGapsFor !== cid) return null;
  return html`<div style=${{ background: 'var(--surface)', border: '1px solid var(--rule)', borderRadius: 8, overflow: 'hidden', marginBottom: 24 }}>
    <div style=${{ padding: '12px 16px 10px', display: 'flex', alignItems: 'baseline', gap: 10, flexWrap: 'wrap' }}>
      <h3 style=${{ fontFamily: 'var(--font-display)', fontSize: 17, fontWeight: 500, color: 'var(--ink)' }}>Unfinished</h3>
      <span style=${{ fontSize: 12, color: 'var(--ink-muted)', flex: 1 }}>what the Keeper sees as open</span>
      <div role="tablist" style=${{ display: 'flex', border: '1px solid var(--rule)', borderRadius: 6, overflow: 'hidden', fontSize: 12 }}>
        ${GAP_TABS.map((t) => html`<span key=${t} role="tab" aria-selected=${tab === t} onClick=${() => { setTab(t); setAll(false); }} style=${{
          padding: '4px 9px', cursor: 'pointer', background: tab === t ? 'var(--paper-deep)' : 'var(--surface)',
          fontWeight: tab === t ? 500 : 400, color: tab === t ? 'var(--ink)' : 'var(--ink-muted)',
        }}>${TAB_LABEL[t]} ${counts[t]}</span>`)}
      </div>
    </div>
    ${shown.length === 0
      ? html`<div style=${{ padding: '14px 16px', borderTop: '1px solid var(--rule-soft)', fontSize: 13, color: 'var(--ink-muted)' }}>
          ${rows.length ? 'Nothing open here.' : 'Nothing unfinished — every page has substance and every link lands.'}
        </div>`
      : shown.map((r) => html`<${GapRow} key=${r.key} row=${r} skills=${s.keeperSkills} onCreate=${create} />`)}
    ${(list.length > 0 || hiddenByUser > 0) && html`<div style=${{ padding: '9px 16px', borderTop: '1px solid var(--rule-soft)', fontSize: 12.5, color: 'var(--ink-muted)', display: 'flex', gap: 12 }}>
      <span style=${{ flex: 1 }}>${list.length ? `Showing ${shown.length} of ${list.length}` : ''}</span>
      ${hiddenByUser > 0 && html`<span onClick=${restoreGaps} style=${{ cursor: 'pointer' }}>Restore ${hiddenByUser} dismissed</span>`}
      ${list.length > LIMIT && html`<span onClick=${() => setAll((v) => !v)} style=${{ color: 'var(--burgundy)', cursor: 'pointer' }}>${all ? 'Show fewer' : 'View all'}</span>`}
    </div>`}
  </div>`;
}
