// "Change icon…" for a Codex folder: pick a glyph and a colour, or go back to
// the icon guessed from the folder's name/kind. Saved in the world's prefs.
import { html, useState } from '../vendor/htm-preact-standalone.mjs';
import { closeModal } from './core.js';
import { Btn, Icon, toneGlyph } from './ui.js';
import { FOLDER_ICON_GROUPS, FOLDER_TONES } from './folderIcons.js';
import { setFolderIcon } from './actions.js';
import { ModalShell } from './modals.js';

const toneColor = (tone) => `var(--${tone})`;

export function FolderIconModal({ node, current, auto }) {
  const [icon, setIcon] = useState(current?.icon || auto?.icon || 'folder');
  const [tone, setTone] = useState(current?.tone || auto?.tone || 'burgundy');
  const [busy, setBusy] = useState(false);
  const save = async (value) => {
    setBusy(true);
    await setFolderIcon(node.path, value);
    closeModal();
  };
  return html`<${ModalShell} wide title="Folder icon" footer=${html`
    ${current && html`<${Btn} kind="ghost" disabled=${busy} onClick=${() => save(null)}>Use automatic icon</${Btn}>`}
    <span style=${{ flex: 1 }} />
    <${Btn} kind="ghost" disabled=${busy} onClick=${closeModal}>Cancel</${Btn}>
    <${Btn} kind="primary" disabled=${busy} onClick=${() => save({ icon, tone })}>Save icon</${Btn}>`}>
    <div style=${{ display: 'flex', alignItems: 'center', gap: 14 }}>
      ${toneGlyph(icon, tone, 44)}
      <div style=${{ flex: 1, minWidth: 0 }}>
        <div style=${{ fontFamily: 'var(--font-display)', fontSize: 17, color: 'var(--ink)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>${node.name}</div>
        <div style=${{ fontSize: 12, color: 'var(--ink-muted)', marginTop: 2 }}>Saved with this world, so the icon travels with the folder.</div>
      </div>
      <div role="radiogroup" aria-label="Colour" style=${{ display: 'flex', gap: 6 }}>
        ${FOLDER_TONES.map((t) => html`<button key=${t} role="radio" aria-checked=${tone === t} title=${t} onClick=${() => setTone(t)}
          style=${{ width: 24, height: 24, borderRadius: 999, cursor: 'pointer', background: toneColor(t),
            border: '2px solid var(--surface-raised)', boxShadow: tone === t ? `0 0 0 2px ${toneColor(t)}` : '0 0 0 1px var(--rule)' }} />`)}
      </div>
    </div>
    ${FOLDER_ICON_GROUPS.map((g) => html`<div key=${g.label}>
      <div class="ck-label" style=${{ color: 'var(--ink-muted)', marginBottom: 6 }}>${g.label}</div>
      <div style=${{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(76px, 1fr))', gap: 6 }}>
        ${g.icons.map((it) => html`<button key=${it.icon} title=${it.label} aria-pressed=${icon === it.icon} onClick=${() => setIcon(it.icon)}
          style=${{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 5, padding: '8px 4px', borderRadius: 6, cursor: 'pointer',
            background: icon === it.icon ? `var(--${tone}-50)` : 'transparent',
            border: `1px solid ${icon === it.icon ? toneColor(tone) : 'var(--rule-soft)'}`,
            color: icon === it.icon ? toneColor(tone) : 'var(--ink-soft)' }}>
          <${Icon} name=${it.icon} size=${20} />
          <span style=${{ fontSize: 11, color: 'var(--ink-muted)' }}>${it.label}</span>
        </button>`)}
      </div>
    </div>`)}
  </${ModalShell}>`;
}
