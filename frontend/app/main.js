// Entry: boot, router, global op banner + modal host.
import { html, render, useEffect } from '../vendor/htm-preact-standalone.mjs';
import { useStore, loadApiBase, setOp, setState } from './core.js';
import { loadCampaigns, loadConfig, refreshProviderStatus, checkMigration } from './actions.js';
import { Icon, Spinner, ContextMenuHost } from './ui.js';
import { ModalHost } from './modals.js';
import { useGlobalHotkeys } from './screens/palette.js';
import { initMenuBridge } from './commands.js';
import { LibraryScreen } from './screens/library.js';
import { CampaignScreen } from './screens/campaign.js';
import { SessionScreen } from './screens/session.js';
import { NewSessionScreen } from './screens/newSession.js';
import { SummarizeScreen } from './screens/summarize.js';
import { CodexUpdateScreen } from './screens/codexUpdate.js';
import { SettingsScreen } from './screens/settings.js';
import { CodexScreen } from './screens/codex.js';
import { SearchScreen } from './screens/search.js';
import { PageScreen } from './screens/page.js';
import { SessionsScreen } from './screens/sessions.js';
import { NewWorldScreen } from './screens/newWorld.js';
import { AtlasScreen } from './screens/atlas.js';
import { TimelineScreen } from './screens/timeline.js';
import { GraphScreen } from './screens/graph.js';
import { KeeperScreen } from './screens/keeper.js';
import { MigrationScreen } from './screens/migration.js';

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
  useGlobalHotkeys();
  // Kick off boot loads here (not at module scope): guarantees the store
  // listener is registered before the local server's near-instant responses
  // resolve, so the first paint after data lands actually repaints.
  useEffect(() => {
    loadCampaigns();
    loadConfig().then(() => refreshProviderStatus()).catch(() => {});
    checkMigration();
  }, []);
  // Show migration screen if unmigrated sessions detected. Result view stays
  // until user dismisses it (onSkip clears migrationStatus).
  const needsMigration = store.migrationStatus?.needs_migration || store.migrationResult != null;
  if (needsMigration) {
    const dismiss = () => setState({ migrationStatus: null, migrationResult: null });
    return html`<div style=${{ height: '100%' }}>
      <${MigrationScreen} store=${store} onSkip=${dismiss} />
    </div>`;
  }

  const r = store.route.name;
  let screen;
  switch (r) {
    case 'library': screen = html`<${LibraryScreen} store=${store} />`; break;
    case 'campaign': screen = html`<${CampaignScreen} store=${store} />`; break;
    case 'session': screen = html`<${SessionScreen} store=${store} />`; break;
    case 'newSession': screen = html`<${NewSessionScreen} store=${store} />`; break;
    case 'summarize': screen = html`<${SummarizeScreen} store=${store} />`; break;
    case 'codexUpdate': screen = html`<${CodexUpdateScreen} store=${store} />`; break;
    case 'settings': screen = html`<${SettingsScreen} store=${store} />`; break;
    case 'codex': screen = html`<${CodexScreen} store=${store} />`; break;
    case 'search': screen = html`<${SearchScreen} store=${store} />`; break;
    case 'sessions': screen = html`<${SessionsScreen} store=${store} />`; break;
    case 'page': screen = html`<${PageScreen} store=${store} />`; break;
    case 'newWorld': screen = html`<${NewWorldScreen} store=${store} />`; break;
    case 'atlas': screen = html`<${AtlasScreen} store=${store} />`; break;
    case 'timeline': screen = html`<${TimelineScreen} store=${store} />`; break;
    case 'graph': screen = html`<${GraphScreen} store=${store} />`; break;
    case 'keeper': screen = html`<${KeeperScreen} store=${store} />`; break;
    default: screen = html`<${LibraryScreen} store=${store} />`;
  }
  return html`<div style=${{ height: '100%' }}>
    ${screen}
    <${OpBanner} op=${store.op} />
    <${ModalHost} modal=${store.modal} />
    <${ContextMenuHost} />
    ${store.error && store.route.name === 'library' && !store.campaigns.length && html`
      <div class="ck-toast" style=${{ background: '#FBEDE9', border: '1px solid rgba(122,46,31,.25)', color: 'var(--burgundy-700)', padding: '12px 16px', borderRadius: 10, fontSize: 13 }}>
        Can't reach the backend at ${store.apiBase}. Check it's running, then set the URL in Settings.
      </div>`}
  </div>`;
}

// ── Boot ──────────────────────────────────────────────────────────
loadApiBase();                       // sync: resolve API base/token before any fetch
initMenuBridge();                    // native menu → runCommand (no-op in the browser)
render(html`<${App} />`, document.getElementById('root'));  // App's effect does the loads
