// Template editor — the normal CodeMirror 6 source editor over a `_templates/`
// file. Templates aren't entities, so there's no infobox/backlinks rail: just
// the editor, the codex file tree, and a save chip. `{{title}}` is replaced
// with the page title on create. Load/save go through the template API
// (`/vault/templates/:name`), since `_templates` is reserved from page routes.
import { html, useState, useEffect, useRef } from '../../vendor/htm-preact-standalone.mjs';
import { navigate, useStore, openModal } from '../core.js';
import { Shell, Topbar } from '../shell.js';
import { Icon, Empty } from '../ui.js';
import { loadVaultTree, loadTemplates, saveTemplate, deleteTemplate, uploadVaultAsset, loadSnippets } from '../actions.js';
import { mountEditor } from '../cm.js';
import { setEditorActive } from '../commands.js';
import { openPageEvt } from '../tabs.js';
import { FileTree, buildTree, makeVaultActions } from './codex.js';

// Bare CM6 host — same singleton editor as the page, minus page-only hooks.
function TemplateEditor({ content, cacheKey, pages, snippets, onSave, onState }) {
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
    }).then((c) => { if (dead) c.destroy(); else ctl = c; });
    setEditorActive(true);
    return () => { dead = true; if (ctl) ctl.destroy(); setEditorActive(false); };
  }, [cacheKey]);
  return html`<div ref=${hostRef} class="ck-cm" />`;
}

export function TemplateScreen() {
  const store = useStore();
  const c = store.campaign;
  const name = store.route.params.name;
  const cacheKey = `tpl:${c?.campaign_id}:${name}`;
  const [saveState, setSaveState] = useState('saved');

  useEffect(() => {
    if (!c) return;
    loadVaultTree(c.campaign_id);
    if (!(store.templates || []).length) loadTemplates(c.campaign_id);
    if (!(store.snippets || []).length) loadSnippets(c.campaign_id);
  }, [c?.campaign_id]);

  const tpl = (store.templates || []).find((t) => t.name === name);
  const pages = store.vaultPages || [];
  const folders = store.vaultFolders || [];
  const tree = buildTree(folders, pages);
  const act = makeVaultActions(c, folders);

  const doSave = async (content) => { await saveTemplate(name, content); };
  const remove = () => openModal('confirm', {
    title: `Delete template “${name}”?`,
    message: 'Pages already created from it are untouched. This only removes the template file.',
    confirmLabel: 'Delete template',
    onConfirm: async () => { await deleteTemplate(name); navigate('codex', { id: c.campaign_id }); },
  });

  const crumbs = [
    { label: c?.name || 'World', onClick: () => navigate('codex', { id: c.campaign_id }) },
    { label: 'Templates' },
    { label: name },
  ];
  const savedChip = html`<span style=${{ display: 'flex', alignItems: 'center', gap: 5, fontSize: 12,
    color: saveState === 'saved' ? 'var(--moss)' : 'var(--ink-faint)' }}>
    <${Icon} name=${saveState === 'saved' ? 'check' : 'feather'} size=${12} />
    ${saveState === 'saving' ? 'Saving…' : saveState === 'dirty' ? 'Unsaved' : 'Saved to vault'}
  </span>`;
  const topbar = html`<${Topbar} crumbs=${crumbs}
    right=${html`<div style=${{ display: 'flex', gap: 10, alignItems: 'center' }}>
      ${savedChip}
      <button onClick=${remove} title="Delete template"
        style=${{ padding: '6px 8px', color: 'var(--ink-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>
        <${Icon} name="trash" size=${14} />
      </button>
    </div>`} />`;

  const sidebar = html`<${FileTree} campaign=${c} tree=${tree} active=${null}
    onOpen=${(p, e) => openPageEvt(p.path, e)} act=${act} />`;

  return html`<${Shell} sidebar=${sidebar} topbar=${topbar} bodyStyle=${{ padding: 0 }}>
    ${tpl == null
      ? html`<div style=${{ flex: 1, padding: 40 }}><${Empty} icon="scroll" title="Template not found">
          The template “${name}” doesn’t exist. <a onClick=${() => navigate('codex', { id: c?.campaign_id })} style=${{ cursor: 'pointer', color: 'var(--burgundy)' }}>Back to the codex</a>.
        </${Empty}></div>`
      : html`<div style=${{ height: '100%', overflowY: 'auto' }}>
          <div style=${{ maxWidth: 720, margin: '0 auto', padding: '24px 52px' }}>
            <div style=${{ fontSize: 12, color: 'var(--ink-faint)', fontStyle: 'italic', marginBottom: 14 }}>
              Template${tpl.kind ? ` for ${tpl.kind} pages` : ''} · <code>{{title}}</code> becomes the page title on create
            </div>
            <${TemplateEditor} key=${cacheKey} content=${tpl.content} cacheKey=${cacheKey}
              pages=${pages} snippets=${store.snippets} onSave=${doSave} onState=${setSaveState} />
          </div>
        </div>`}
  </${Shell}>`;
}
