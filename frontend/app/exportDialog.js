// Export a page or folder (incl. subfolders) to <world>/Exports/ as PDF,
// Markdown or HTML, optionally without GM-only content. Opened via
// openModal('exportPages', { path, isFolder, title, count }).
import { html, useState } from '../vendor/htm-preact-standalone.mjs';
import { store, closeModal, openModal } from './core.js';
import { Btn, Spinner } from './ui.js';
import { ModalShell } from './modals.js';
import { exportPages, revealPath } from './actions.js';

const FORMATS = [['pdf', 'PDF'], ['markdown', 'Markdown'], ['html', 'HTML']];

function Choice({ on, title, sub, disabled, onClick }) {
  return html`<div onClick=${disabled ? null : onClick} style=${{
    display: 'flex', alignItems: 'center', gap: 10, padding: '10px 12px', borderRadius: 8, cursor: disabled ? 'default' : 'pointer',
    border: `1px solid ${on ? 'var(--burgundy-300)' : 'var(--rule)'}`, background: on ? 'var(--burgundy-50)' : 'var(--surface)',
    opacity: disabled ? 0.5 : 1,
  }}>
    <span style=${{
      width: 14, height: 14, borderRadius: '50%', flex: '0 0 14px',
      border: `1px solid ${on ? 'var(--burgundy)' : 'var(--rule-strong)'}`,
      background: on ? 'radial-gradient(var(--burgundy) 45%, transparent 50%)' : 'var(--surface-raised)',
    }} />
    <div style=${{ minWidth: 0 }}>
      <div style=${{ fontSize: 13.5, fontWeight: 500 }}>${title}</div>
      <div style=${{ fontSize: 12, color: 'var(--ink-muted)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${sub}</div>
    </div>
  </div>`;
}

// `page` (the open page, if any) and `folder` ({ path, name, count }) are the
// two things the dialog can export; either may be absent depending on where it opened.
export function ExportPagesModal({ page, folder }) {
  const [scope, setScope] = useState(page ? 'page' : 'folder');
  const [format, setFormat] = useState('pdf');
  const [leaveOut, setLeaveOut] = useState(true);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(null);
  const [done, setDone] = useState(null);

  async function run() {
    setBusy(true); setErr(null);
    try {
      const target = scope === 'page' ? page.path : folder.path;
      setDone(await exportPages({ scope, path: target, format, leaveOutGm: leaveOut }));
    } catch (e) { setErr(e.message || 'Export failed'); }
    finally { setBusy(false); }
  }

  const footer = done
    ? html`<span style=${{ flex: 1 }} /><${Btn} kind="ghost" icon="folder" onClick=${() => revealPath(done.path)}>Show in folder</${Btn}><${Btn} kind="primary" onClick=${closeModal}>Done</${Btn}>`
    : html`<span style=${{ fontSize: 12, color: 'var(--ink-muted)', flex: 1 }}>Saves to Exports/</span>
        <${Btn} kind="ghost" onClick=${closeModal}>Cancel</${Btn}>
        <${Btn} kind="primary" icon="upload" disabled=${busy} onClick=${run}>${busy ? html`<${Spinner} size=${14} /> Exporting…` : 'Export'}</${Btn}>`;

  return html`<${ModalShell} title="Export" footer=${footer}>
    ${done
      ? html`<div style=${{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <div style=${{ fontSize: 13.5 }}>Exported ${done.files} page${done.files === 1 ? '' : 's'}${done.skipped ? ` (${done.skipped} GM-only left out)` : ''}.</div>
          <div style=${{ fontSize: 12, fontFamily: 'var(--font-mono)', color: 'var(--ink-soft)', wordBreak: 'break-all' }}>${done.path}</div>
        </div>`
      : html`
        <div style=${{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div class="label-sm">What</div>
          ${page && html`<${Choice} on=${scope === 'page'} title="This page" sub=${page.title} onClick=${() => setScope('page')} />`}
          ${folder && html`<${Choice} on=${scope === 'folder'} title="This folder"
            sub=${`${folder.name}${folder.count != null ? ` · ${folder.count} page${folder.count === 1 ? '' : 's'}` : ''}, including subfolders`} onClick=${() => setScope('folder')} />`}
        </div>
        <div style=${{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div class="label-sm">Format</div>
          <div style=${{ display: 'flex', border: '1px solid var(--rule)', borderRadius: 6, overflow: 'hidden', fontSize: 13 }}>
            ${FORMATS.map(([v, l]) => html`<span key=${v} onClick=${() => setFormat(v)} style=${{
              flex: 1, textAlign: 'center', padding: '7px 0', cursor: 'pointer',
              background: format === v ? 'var(--paper-deep)' : 'var(--surface)', fontWeight: format === v ? 500 : 400,
              color: format === v ? 'var(--ink)' : 'var(--ink-muted)',
            }}>${l}</span>`)}
          </div>
        </div>
        <label style=${{ display: 'flex', alignItems: 'flex-start', gap: 8, cursor: 'pointer' }}>
          <input type="checkbox" checked=${leaveOut} onChange=${() => setLeaveOut(!leaveOut)} style=${{ width: 14, height: 14, marginTop: 3, cursor: 'pointer' }} />
          <span>
            <span style=${{ fontSize: 13.5, fontWeight: 500 }}>Leave out GM-only content</span>
            <span style=${{ display: 'block', fontSize: 12, color: 'var(--ink-muted)' }}>GM-only pages, infobox fields and secret callouts</span>
          </span>
        </label>
        ${err && html`<div style=${{ fontSize: 12.5, color: 'var(--burgundy)' }}>${err}</div>`}`}
  </${ModalShell}>`;
}

// Open the dialog for a page and/or a folder (`folderPath` '' = none offered).
export function openExport({ page, folderPath }) {
  const pages = store.vaultPages || [];
  const folder = folderPath
    ? { path: folderPath, name: folderPath.split('/').pop(), count: pages.filter((p) => p.path.startsWith(`${folderPath}/`)).length }
    : null;
  openModal('exportPages', { page: page ? { path: page.path, title: page.title } : null, folder });
}
