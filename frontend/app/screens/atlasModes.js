// Atlas HUD pieces: the active-tool chip, the shortcut sheet, and a one-time tip.
import { html, useState } from '../../vendor/htm-preact-standalone.mjs';
import { Icon } from '../ui.js';
import { SHORTCUTS } from './atlasNav.js';

const pill = { position: 'absolute', left: '50%', transform: 'translateX(-50%)', zIndex: 94, display: 'inline-flex', alignItems: 'center', gap: 9,
  padding: '5px 12px', borderRadius: 999, whiteSpace: 'nowrap', fontSize: 12.5, pointerEvents: 'auto' };

// "Drawing · Pen — Esc to leave". One place that always says what a click will do.
export function ModeChip({ label, onExit }) {
  if (!label) return null;
  return html`<div onMouseDown=${(e) => e.stopPropagation()} style=${{ ...pill, top: 62, background: 'var(--burgundy)', color: '#FBF6E9',
    boxShadow: '0 6px 18px rgba(92,35,23,.3)' }}>
    <span style=${{ fontWeight: 600 }}>${label}</span>
    <button onClick=${onExit} style=${{ color: '#F7E8E2', fontSize: 11.5, opacity: 0.85, fontFamily: 'var(--font-mono)', background: 'none', border: 'none', cursor: 'pointer' }}>Esc to leave</button>
  </div>`;
}

export function ShortcutSheet({ onClose }) {
  return html`<div onMouseDown=${(e) => { if (e.target === e.currentTarget) onClose(); }} style=${{ position: 'absolute', inset: 0, zIndex: 130,
    display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'rgba(31,24,19,.30)' }}>
    <div style=${{ width: 360, background: 'var(--surface-raised)', border: '1px solid var(--rule-strong)', borderRadius: 10, boxShadow: 'var(--shadow-raised)', overflow: 'hidden' }}>
      <div style=${{ padding: '12px 16px', borderBottom: '1px solid var(--rule-soft)', display: 'flex', alignItems: 'center' }}>
        <span style=${{ flex: 1, fontFamily: 'var(--font-display)', fontSize: 15.5, fontWeight: 500 }}>Atlas shortcuts</span>
        <button onClick=${onClose} style=${{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ink-muted)' }}><${Icon} name="x" size=${13} /></button>
      </div>
      <div style=${{ padding: '10px 16px 14px', display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '7px 16px', fontSize: 13 }}>
        ${SHORTCUTS.map(([k, , label]) => html`<span key=${k} style=${{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--burgundy-700)' }}>${k}</span><span key=${k + label}>${label}</span>`)}
      </div>
    </div>
  </div>`;
}

const TIP_KEY = 'ck_atlas_tip_seen';
const seen = () => { try { return localStorage.getItem(TIP_KEY) === '1'; } catch (_) { return false; } };

// Shown once, until dismissed: the two things you can't discover by looking.
export function TipBar({ show }) {
  const [gone, setGone] = useState(seen);
  if (!show || gone) return null;
  const dismiss = () => { try { localStorage.setItem(TIP_KEY, '1'); } catch (_) { /* private mode */ } setGone(true); };
  return html`<div onMouseDown=${(e) => e.stopPropagation()} style=${{ ...pill, bottom: 22, background: 'var(--surface-raised)', color: 'var(--ink-soft)',
    border: '1px solid var(--rule-strong)', boxShadow: 'var(--shadow-card)' }}>
    <span>Tip: hold <b>⌘</b>/<b>Ctrl</b> over a pin to preview its page · press <b>?</b> for shortcuts</span>
    <button onClick=${dismiss} style=${{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ink-muted)' }}><${Icon} name="x" size=${11} /></button>
  </div>`;
}
