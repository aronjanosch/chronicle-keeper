// Entry: boot, router, global op banner + modal host.
import { html, render, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { useStore, loadApiBase, setOp } from './core.js';
import { loadCampaigns, loadConfig, refreshProviderStatus } from './actions.js';
import { Icon, Spinner } from './ui.js';
import { ModalHost } from './modals.js';
import { LibraryScreen } from './screens/library.js';
import { CampaignScreen } from './screens/campaign.js';
import { SessionScreen } from './screens/session.js';
import { NewSessionScreen } from './screens/newSession.js';
import { SummarizeScreen } from './screens/summarize.js';
import { SettingsScreen } from './screens/settings.js';
import { CodexScreen } from './screens/codex.js';
import { CodexEntryScreen } from './screens/codexEntry.js';

function OpBanner({ op }) {
  if (!op) return null;
  const tone = op.state === 'err' ? { c: 'var(--burgundy-700)', b: 'rgba(122,46,31,.25)', bg: '#FBEDE9' }
    : op.state === 'done' ? { c: 'var(--moss)', b: 'rgba(74,93,58,.3)', bg: 'var(--moss-50)' }
    : { c: 'var(--ink)', b: 'var(--rule)', bg: 'var(--surface-raised)' };
  const running = !op.state;
  return html`<div class="ck-toast" style=${{ display: 'flex', alignItems: 'center', gap: 12, padding: '12px 16px', background: tone.bg, border: `1px solid ${tone.b}`, borderRadius: 10, boxShadow: 'var(--shadow-raised)', color: tone.c, fontSize: 13 }}>
    ${running ? html`<${Spinner} size=${15} />` : html`<${Icon} name=${op.state === 'err' ? 'x' : 'check'} size=${15} />`}
    <span style=${{ fontFamily: op.msg.includes('█') ? 'var(--font-mono)' : 'inherit' }}>${op.msg}</span>
    ${!running && html`<button onClick=${() => setOp(null)} style=${{ marginLeft: 4, color: 'inherit', opacity: 0.6, cursor: 'pointer', background: 'none', border: 'none', display: 'flex' }}><${Icon} name="x" size=${13} /></button>`}
  </div>`;
}

function App() {
  const store = useStore();
  // Kick off boot loads here (not at module scope): guarantees the store
  // listener is registered before the local server's near-instant responses
  // resolve, so the first paint after data lands actually repaints.
  useEffect(() => { loadCampaigns(); loadConfig().then(() => refreshProviderStatus()).catch(() => {}); }, []);
  const r = store.route.name;
  let screen;
  switch (r) {
    case 'library': screen = html`<${LibraryScreen} store=${store} />`; break;
    case 'campaign': screen = html`<${CampaignScreen} store=${store} />`; break;
    case 'session': screen = html`<${SessionScreen} store=${store} />`; break;
    case 'newSession': screen = html`<${NewSessionScreen} store=${store} />`; break;
    case 'summarize': screen = html`<${SummarizeScreen} store=${store} />`; break;
    case 'settings': screen = html`<${SettingsScreen} store=${store} />`; break;
    case 'codex': screen = html`<${CodexScreen} store=${store} />`; break;
    case 'codexEntry': screen = html`<${CodexEntryScreen} store=${store} />`; break;
    default: screen = html`<${LibraryScreen} store=${store} />`;
  }
  return html`<div style=${{ height: '100%' }}>
    ${screen}
    <${OpBanner} op=${store.op} />
    <${ModalHost} modal=${store.modal} />
    ${store.error && store.route.name === 'library' && !store.campaigns.length && html`
      <div class="ck-toast" style=${{ background: '#FBEDE9', border: '1px solid rgba(122,46,31,.25)', color: 'var(--burgundy-700)', padding: '12px 16px', borderRadius: 10, fontSize: 13 }}>
        Can't reach the backend at ${store.apiBase}. Check it's running, then set the URL in Settings.
      </div>`}
  </div>`;
}

// ── Boot ──────────────────────────────────────────────────────────
loadApiBase();                       // sync: resolve API base/token before any fetch
render(html`<${App} />`, document.getElementById('root'));  // App's effect does the loads
