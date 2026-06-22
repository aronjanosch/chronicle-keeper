// All data operations. Thin wrappers over the HTTP client that update the store.
// Ported 1:1 from the legacy app.js so the backend contract is unchanged.
import { store, setState, setOp, bump, navigate, apiFetch, apiJson, apiText, apiStream, apiUrl, slugify, toneFor, initials, loadWorldTabs, remapTabs, pruneTabs } from './core.js';

// ── Campaigns ─────────────────────────────────────────────────────
export async function loadCampaigns() {
  setState({ loading: true, error: null });
  try {
    const data = await apiFetch('/campaigns');
    const list = data.campaigns || [];
    const details = await Promise.all(list.map((c) => apiFetch(`/campaigns/${c.campaign_id}`).catch(() => null)));
    const campaigns = details.filter(Boolean).map(decorateCampaign);
    setState({ campaigns, loading: false });
  } catch (e) { setState({ error: e.message, loading: false }); }
}

function decorateCampaign(c) {
  return { ...c, sigil: initials(c.name).slice(0, 1), tone: toneFor(c.campaign_id || c.name) };
}

export async function openCampaign(id) {
  setState({ loading: true, error: null });
  try {
    const campaign = decorateCampaign(await apiFetch(`/campaigns/${id}`));
    const [sessions, vaultTree] = await Promise.all([
      apiFetch(`/campaigns/${id}/sessions`).catch(() => []),
      campaign.vault_path
        ? apiFetch(`/campaigns/${id}/vault/tree`).catch(() => null)
        : Promise.resolve(null),
    ]);
    // Chats are per-world: drop a previous world's chat binding so the Keeper
    // never posts a stale chatId to this world's root (→ "Chat not found").
    const keeper = store.keeper && store.keeper.campaignId !== id
      ? { ...store.keeper, campaignId: id, chatId: null, events: [], attachments: [], live: null, error: null }
      : store.keeper;
    setState({
      campaign,
      campaignSessions: sessions || [],
      vaultPages: vaultTree?.pages || [],
      vaultFolders: vaultTree?.folders || [],
      keeper,
      loading: false,
    });
    loadWorldTabs(id);
    if (vaultTree) pruneTabs(vaultTree.pages || []);
    if (campaign.vault_path) { loadVaultLinks(id); loadKindSchemas(id); loadAtlasMaps(id); loadTemplates(id); }
    navigate('campaign', { id });
  } catch (e) { setState({ error: e.message, loading: false }); }
}

export async function refreshCampaignSessions() {
  const id = store.campaign?.campaign_id;
  if (!id) return;
  const sessions = await apiFetch(`/campaigns/${id}/sessions`).catch(() => []);
  setState({ campaignSessions: sessions || [] });
}

export function generateCampaignId(name) {
  const base = slugify(name) || 'campaign';
  const existing = new Set(store.campaigns.map((c) => c.campaign_id));
  if (!existing.has(base)) return base;
  let n = 2;
  while (existing.has(`${base}-${n}`)) n += 1;
  return `${base}-${n}`;
}

export async function createCampaign(form) {
  const id = generateCampaignId(form.name);
  const start = form.start === '' || form.start == null ? 0 : Number(form.start);
  await apiJson('/campaigns', 'POST', {
    campaign_id: id,
    name: form.name,
    start_session_number: start,
    vault_path: form.vault_path || null,
    scaffold: form.scaffold || false,
    adopt: form.adopt || false,
  });
  await apiJson(`/campaigns/${id}`, 'PUT', {
    name: form.name, system: form.system, setting: form.setting,
    default_language: form.default_language, gm: form.gm, gm_pronouns: form.gm_pronouns,
    players: form.players, extra_info: form.extra_info,
  });
  await loadCampaigns();
  await openCampaign(id);
  return id;
}

// Re-add the demo world (New-World screen). 409 when it already exists.
export async function addExampleWorld() {
  const campaign = await apiJson('/seed-example', 'POST', {});
  await loadCampaigns();
  await openCampaign(campaign.campaign_id);
  return campaign.campaign_id;
}

export async function updateCampaign(patch) {
  const id = store.campaign.campaign_id;
  const updated = decorateCampaign(await apiJson(`/campaigns/${id}`, 'PUT', patch));
  setState({ campaign: updated });
  await loadCampaigns();
}

// Build/rebuild the campaign "story so far" recap from existing session
// summaries (one LLM call, server-side). Updates the loaded campaign in place.
export async function generateRecap() {
  const id = store.campaign?.campaign_id;
  if (!id) return;
  setOp('Weaving the story so far…');
  try {
    const r = await apiJson(`/campaigns/${id}/recap`, 'POST', {});
    setState({ campaign: { ...store.campaign, recap: r.recap, recap_updated_at: r.recap_updated_at } });
    setOp(`Recap woven from ${r.sessions_used} session${r.sessions_used === 1 ? '' : 's'}`, 'done');
  } catch (e) { setOp(e.message, 'err'); }
}

export async function deleteCampaign(id) {
  await apiFetch(`/campaigns/${id}`, { method: 'DELETE' });
  setState({ campaign: null, campaignSessions: [] });
  await loadCampaigns();
  navigate('library');
}

// ── Campaign tags ─────────────────────────────────────────────────
// Tags are session metadata; these manage the campaign-wide vocabulary
// (rename/merge/delete across all sessions) so it stays one consistent set.
export async function loadCampaignTags(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return [];
  const r = await apiFetch(`/campaigns/${id}/tags`).catch((e) => {
    console.warn('loadCampaignTags failed:', e);
    return { tags: [] };
  });
  setState({ campaignTags: r.tags || [] });
  return r.tags || [];
}

export async function renameCampaignTag(from, to) {
  const id = store.campaign.campaign_id;
  await apiJson(`/campaigns/${id}/tags/rename`, 'POST', { from, to });
  await loadCampaignTags(id);
  await refreshCampaignSessions();
}

export async function deleteCampaignTag(tag) {
  const id = store.campaign.campaign_id;
  await apiJson(`/campaigns/${id}/tags/delete`, 'POST', { tag });
  await loadCampaignTags(id);
  await refreshCampaignSessions();
}

// Distill pasted notes into proposed entries (not saved yet — the user reviews).
export async function importCodex(text) {
  const id = store.campaign.campaign_id;
  const r = await apiJson(`/campaigns/${id}/codex/import`, 'POST', { text });
  return r.entries || [];
}

// Save the reviewed entries as vault pages; returns { created, skipped }.
export async function commitCodexImport(entries) {
  const id = store.campaign.campaign_id;
  const r = await apiJson(`/campaigns/${id}/codex/import/commit`, 'POST', { entries });
  await loadVaultTree(id);
  return r;
}

// Zip the whole world folder on the backend; returns { path }.
export async function exportWorld(includeAudio) {
  const id = store.campaign.campaign_id;
  return apiJson(`/campaigns/${id}/export-world`, 'POST', { include_audio: includeAudio });
}

// Reveal a path in Finder/Explorer (Tauri shell only; no-op in browser dev).
export function revealPath(path) {
  const opener = window.__TAURI__?.opener;
  if (opener?.revealItemInDir && path) opener.revealItemInDir(path);
}

// ── Vault pages ───────────────────────────────────────────────────
// Native folder picker in the Tauri shell; null in browser-dev (path typed instead).
export async function pickVaultFolder() {
  const dialog = window.__TAURI__?.dialog;
  if (!dialog?.open) return null;
  const picked = await dialog.open({ directory: true, multiple: false, title: 'Choose a vault folder' });
  return typeof picked === 'string' ? picked : null;
}

// Layout sniff for the New-World "open existing" preview. Null on any error
// (bad path, server down) — the preview just stays generic.
export async function sniffVault(path) {
  try {
    return await apiJson('/vault/sniff', 'POST', { path });
  } catch {
    return null;
  }
}

// Copy-in import: .md pages + media assets from a folder (e.g. an Obsidian
// vault) into this world's Codex. Returns { imported, renamed, assets }.
export async function importVaultFolder(path) {
  const id = store.campaign.campaign_id;
  const r = await apiJson(`/campaigns/${id}/vault/import`, 'POST', { path });
  await loadVaultTree(id);
  return r;
}

// AI enhancement: assign kind + generate summary frontmatter for pages that lack them.
// `folders` is an array of top-level folder names to process; empty → all folders.
// Returns { enhanced, skipped, failed }.
export async function enhanceVaultPages(folders) {
  const id = store.campaign.campaign_id;
  setOp('Enhancing pages with AI…');
  try {
    const r = await apiJson(`/campaigns/${id}/vault/enhance`, 'POST', { folders: folders || [] });
    await loadVaultTree(id);
    if (r.enhanced === 0 && r.failed === 0) {
      setOp('All pages already enhanced — nothing to do', 'done');
    } else {
      const msg = `Enhanced ${r.enhanced} page${r.enhanced === 1 ? '' : 's'}${r.failed ? ` · ${r.failed} failed` : ''}`;
      setOp(msg, r.failed ? 'err' : 'done');
    }
    return r;
  } catch (e) {
    setOp(e.message, 'err');
    throw e;
  }
}

export async function attachVault(path) {
  const id = store.campaign.campaign_id;
  const campaign = decorateCampaign(await apiJson(`/campaigns/${id}/vault`, 'PUT', { path: path || null }));
  setState({ campaign, vaultPages: [], currentPage: null });
  await loadCampaigns();
  if (campaign.vault_path) await loadVaultPages(id);
  return campaign;
}

export async function loadVaultPages(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return [];
  const r = await apiFetch(`/campaigns/${id}/vault/pages`).catch((e) => {
    console.warn('loadVaultPages failed:', e);
    return { pages: [] };
  });
  setState({ vaultPages: r.pages || [] });
  return r.pages || [];
}

// Folders + pages in one shot — the Explorer's source of truth.
// Also refreshes the link graph (non-blocking) so backlinks + diagnostics track mutations.
export async function loadVaultTree(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return { folders: [], pages: [] };
  let failed = false;
  const r = await apiFetch(`/campaigns/${id}/vault/tree`).catch((e) => {
    console.warn('loadVaultTree failed:', e);
    failed = true;
    return { folders: [], pages: [] };
  });
  setState({ vaultFolders: r.folders || [], vaultPages: r.pages || [] });
  if (!failed && id === store.campaign?.campaign_id) pruneTabs(r.pages || []);
  loadVaultLinks(id);
  return r;
}

// Keeper skills (app-global, same list for every world). Cached once — feeds the
// composer /command menu and the per-kind suggestion chips. Pull, never push.
export async function loadSkills(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id || store.keeperSkills) return;
  const r = await apiFetch(`/campaigns/${id}/agent/skills`).catch(() => null);
  if (r) setState({ keeperSkills: r.skills || [], skillsPath: r.path || '' });
}

// The app-global Skills folder path — for the Settings "Open folder" button.
// Campaign-free route; works even with no world open.
export async function loadSkillsPath() {
  const r = await apiFetch('/skills').catch(() => null);
  if (r) setState({ skillsPath: r.path || '' });
}

// Per-kind infobox field schemas (.ck/config.toml overrides merged with built-ins).
export async function loadKindSchemas(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return;
  const r = await apiFetch(`/campaigns/${id}/vault/kinds`).catch(() => null);
  if (r) setState({ kindSchemas: r.kinds || [] });
}

// Poll the vault change counter; call onChange when files changed outside CK
// (Obsidian, Finder, sync). Returns a stop function — use as a useEffect cleanup.
export function watchVault(campaignId, onChange) {
  let stopped = false;
  let last = null;
  const tick = async () => {
    if (stopped) return;
    try {
      const r = await apiFetch(`/campaigns/${campaignId}/vault/seq`);
      if (stopped) return;
      if (last !== null && r.seq !== last) onChange();
      last = r.seq;
    } catch (_) { /* core restarting — keep polling */ }
    if (!stopped) setTimeout(tick, 1500);
  };
  tick();
  return () => { stopped = true; };
}

// ── Vault index (links, search, tags) ─────────────────────────────
export async function loadVaultLinks(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return null;
  const r = await apiFetch(`/campaigns/${id}/vault/index/links`).catch(() => null);
  if (r) setState({ vaultLinks: r });
  return r;
}

// Grouped vault diagnostics: broken links/media, orphans, conflicts, errors.
export async function loadVaultDiagnostics(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return null;
  const r = await apiFetch(`/campaigns/${id}/vault/diagnostics`).catch(() => null);
  if (r) setState({ vaultDiag: r });
  return r;
}

export async function searchVault(q, facets) {
  const id = store.campaign?.campaign_id;
  if (!id || !q || !q.trim()) return [];
  const params = new URLSearchParams({ q });
  if (facets) {
    if (facets.kind) params.set('kind', facets.kind);
    if (facets.tag) params.set('tag', facets.tag);
    if (facets.folder) params.set('folder', facets.folder);
    if (facets.edited_after) params.set('edited_after', String(facets.edited_after));
    if (facets.edited_before) params.set('edited_before', String(facets.edited_before));
  }
  const r = await apiFetch(`/campaigns/${id}/vault/search?${params}`).catch(() => null);
  return (r && r.results) || [];
}

export async function searchSessions(q, scope) {
  const id = store.campaign?.campaign_id;
  if (!id || !q || !q.trim()) return [];
  const params = new URLSearchParams({ q, scope: scope || 'summaries' });
  const r = await apiFetch(`/campaigns/${id}/sessions/search?${params}`).catch(() => null);
  return (r && r.results) || [];
}

export async function loadVaultTags(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return [];
  const r = await apiFetch(`/campaigns/${id}/vault/index/tags`).catch(() => null);
  if (r) setState({ vaultTags: r.tags || [] });
  return (r && r.tags) || [];
}

export async function createVaultFolder(path) {
  const id = store.campaign.campaign_id;
  await apiJson(`/campaigns/${id}/vault/folders`, 'POST', { path });
  await loadVaultTree(id);
}

export async function moveVaultEntry(from, to) {
  const id = store.campaign.campaign_id;
  await apiJson(`/campaigns/${id}/vault/move`, 'POST', { from, to });
  remapTabs(from, to);
  await loadVaultTree(id);
}

export async function deleteVaultPage(path) {
  const id = store.campaign.campaign_id;
  await apiFetch(`/campaigns/${id}/vault/pages/${encodeURI(path)}`, { method: 'DELETE' });
  await loadVaultTree(id);
}

export async function deleteVaultFolder(path) {
  const id = store.campaign.campaign_id;
  await apiFetch(`/campaigns/${id}/vault/folders/${encodeURI(path)}`, { method: 'DELETE' });
  await loadVaultTree(id);
}

export async function readVaultPage(path) {
  const id = store.campaign.campaign_id;
  const page = await apiFetch(`/campaigns/${id}/vault/pages/${encodeURI(path)}`);
  setState({ currentPage: page });
  return page;
}

export async function saveVaultPage(path, content) {
  const id = store.campaign.campaign_id;
  const page = await apiJson(`/campaigns/${id}/vault/pages/${encodeURI(path)}`, 'PUT', { content });
  if (page.path && page.path !== path) remapTabs(path, page.path);
  setState({
    currentPage: store.currentPage?.path === path ? page : store.currentPage,
    vaultPages: (store.vaultPages || []).map((p) => (p.path === path
      ? { ...p, path: page.path, title: page.title, kind: page.kind, summary: page.summary } : p)),
  });
  loadVaultLinks(id);
  return page;
}

// ── Trust & bulk (Phase 13): history, trash, bulk ops, backup ─────

// Version list for one page: [{ ts, origin }] oldest-first.
export async function loadPageHistory(path) {
  const id = store.campaign?.campaign_id;
  if (!id) return [];
  const r = await apiFetch(`/campaigns/${id}/vault/history/${encodeURI(path)}`).catch(() => null);
  return (r && r.versions) || [];
}

// One snapshot's content (null = the page did not exist before that save).
export function readPageVersion(path, ts) {
  const id = store.campaign.campaign_id;
  return apiFetch(`/campaigns/${id}/vault/history/${encodeURI(path)}?ts=${ts}`);
}

export async function restorePageVersion(path, ts) {
  const id = store.campaign.campaign_id;
  const r = await apiJson(`/campaigns/${id}/vault/history-restore`, 'POST', { page: path, ts });
  await loadVaultTree(id);
  return r; // { ok, deleted }
}

// World-wide recent versions; origin 'keeper' = "everything the Keeper changed".
export async function loadWorldHistory(origin, limit) {
  const id = store.campaign?.campaign_id;
  if (!id) return [];
  const params = new URLSearchParams();
  if (origin) params.set('origin', origin);
  if (limit) params.set('limit', String(limit));
  const r = await apiFetch(`/campaigns/${id}/vault/history?${params}`).catch(() => null);
  return (r && r.versions) || [];
}

// Multi-select operations. extra: { tag } | { folder }. Returns { done, errors }.
export async function bulkVault(action, pages, extra = {}) {
  const id = store.campaign.campaign_id;
  const r = await apiJson(`/campaigns/${id}/vault/bulk`, 'POST', { action, pages, ...extra });
  await loadVaultTree(id);
  return r;
}

export async function loadTrash() {
  const id = store.campaign?.campaign_id;
  if (!id) return [];
  const r = await apiFetch(`/campaigns/${id}/vault/trash`).catch(() => null);
  return (r && r.groups) || [];
}

export async function restoreTrash(groupId) {
  const id = store.campaign.campaign_id;
  const r = await apiJson(`/campaigns/${id}/vault/trash/restore`, 'POST', { id: groupId });
  await loadVaultTree(id);
  return r; // { restored: [paths] }
}

// Omit groupId to empty everything.
export function emptyTrash(groupId) {
  const id = store.campaign.campaign_id;
  return apiJson(`/campaigns/${id}/vault/trash/empty`, 'POST', { id: groupId || null });
}

// One-click world zip → Backups/ (server prunes to the last 10).
export async function backupWorld() {
  const id = store.campaign?.campaign_id;
  if (!id) return;
  setOp('Backing up the world…');
  try {
    const r = await apiJson(`/campaigns/${id}/backup`, 'POST', {});
    setOp(`World backed up to ${r.path}`, 'done');
    return r;
  } catch (e) { setOp(e.message, 'err'); }
}

// Typed relations (Phase 9A): frontmatter [[link]] values, predicate = key.
export async function loadRelations(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return [];
  const r = await apiFetch(`/campaigns/${id}/vault/relations`).catch(() => ({ relations: [] }));
  setState({ vaultRelations: r.relations || [] });
  return r.relations || [];
}

// Slash-menu snippets (.ck/templates/snippets/).
export async function loadSnippets(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return [];
  const r = await apiFetch(`/campaigns/${id}/vault/snippets`).catch(() => ({ snippets: [] }));
  setState({ snippets: r.snippets || [] });
  return r.snippets || [];
}

// User-editable page templates (_templates/<name>.md). Each: { name, kind, content }.
export async function loadTemplates(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return [];
  const r = await apiFetch(`/campaigns/${id}/vault/templates`).catch(() => ({ templates: [] }));
  setState({ templates: r.templates || [] });
  return r.templates || [];
}

export async function saveTemplate(name, content) {
  const id = store.campaign.campaign_id;
  await apiJson(`/campaigns/${id}/vault/templates/${encodeURIComponent(name)}`, 'PUT', { content });
  await loadTemplates(id);
}

export async function deleteTemplate(name) {
  const id = store.campaign.campaign_id;
  await apiFetch(`/campaigns/${id}/vault/templates/${encodeURIComponent(name)}`, { method: 'DELETE' });
  await loadTemplates(id);
}

// Pasted/dropped editor media → <vault>/Assets/. Returns { path, name }.
export function uploadVaultAsset(name, blob) {
  const id = store.campaign.campaign_id;
  return apiFetch(`/campaigns/${id}/vault/assets?name=${encodeURIComponent(name)}`, { method: 'POST', body: blob });
}

// `pick` is either a kind (schema-derived body) or { template } to seed from a
// named template file — its frontmatter then dictates the page's kind.
export async function createVaultPage(title, pick, folder) {
  const id = store.campaign.campaign_id;
  const body = { title, folder: folder || null };
  if (pick && typeof pick === 'object' && pick.template) body.template = pick.template;
  else body.kind = pick;
  const page = await apiJson(`/campaigns/${id}/vault/pages`, 'POST', body);
  await loadVaultTree(id);
  return page;
}

// Phase 16: capture → kinded page. Sets kind, fills missing infobox fields,
// drops #inbox, appends the kind's template headings, optionally moves.
export async function promoteVaultPage(path, kind, folder) {
  const id = store.campaign.campaign_id;
  const page = await apiJson(`/campaigns/${id}/vault/promote`, 'POST', { page: path, kind, folder: folder ?? null });
  await loadVaultTree(id);
  return page;
}

// Copy of a page next to the original, first free "Title (copy [n])" name.
export async function duplicateVaultPage(path) {
  const id = store.campaign.campaign_id;
  const src = await apiFetch(`/campaigns/${id}/vault/pages/${encodeURI(path)}`);
  const dir = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
  const base = path.slice(path.lastIndexOf('/') + 1).replace(/\.md$/, '');
  const taken = new Set((store.vaultPages || []).map((p) => p.path));
  let name = `${base} (copy)`;
  for (let n = 2; taken.has(dir ? `${dir}/${name}.md` : `${name}.md`); n++) name = `${base} (copy ${n})`;
  const dest = dir ? `${dir}/${name}.md` : `${name}.md`;
  const page = await apiJson(`/campaigns/${id}/vault/pages/${encodeURI(dest)}`, 'PUT', { content: src.content });
  await loadVaultTree(id);
  return page;
}

export function copyText(text, label = 'Copied') {
  return navigator.clipboard.writeText(text)
    .then(() => setOp(label, 'done'), (e) => setOp(`Copy failed: ${e.message}`, 'err'));
}

// ── Atlas maps (files-as-truth: <world>/Atlas/<id>.json) ──────────
export async function loadAtlasMaps(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return [];
  const r = await apiFetch(`/campaigns/${id}/atlas/maps`).catch((e) => {
    console.warn('loadAtlasMaps failed:', e);
    return { maps: [] };
  });
  setState({ atlasMaps: r.maps || [] });
  return r.maps || [];
}

export async function createAtlasMap(name, imagePath, parent, page) {
  const id = store.campaign.campaign_id;
  const map = await apiJson(`/campaigns/${id}/atlas/maps`, 'POST',
    { name, image_path: imagePath, parent: parent || null, page: page || null });
  setState({ atlasMaps: [...(store.atlasMaps || []), map] });
  return map;
}

// Native image picker in the Tauri shell; null in browser-dev (path typed instead).
export async function pickMapImage() {
  const dialog = window.__TAURI__?.dialog;
  if (!dialog?.open) return null;
  const picked = await dialog.open({
    multiple: false, title: 'Choose map art',
    filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }],
  });
  return typeof picked === 'string' ? picked : null;
}

export async function saveAtlasMap(map) {
  const id = store.campaign.campaign_id;
  const saved = await apiJson(`/campaigns/${id}/atlas/maps/${encodeURIComponent(map.id)}`, 'PUT', map);
  setState({ atlasMaps: (store.atlasMaps || []).map((m) => (m.id === saved.id ? { ...saved, art_seq: m.art_seq } : m)) });
  return saved;
}

export async function replaceAtlasMapArt(mapId, imagePath) {
  const id = store.campaign.campaign_id;
  const saved = await apiJson(`/campaigns/${id}/atlas/maps/${encodeURIComponent(mapId)}/image`, 'PUT', { image_path: imagePath });
  // art_seq (client-only): same-extension replace keeps doc.image identical,
  // so viewers need another signal to refetch the blob.
  setState({ atlasMaps: (store.atlasMaps || []).map((m) => (m.id === saved.id ? { ...saved, art_seq: Date.now() } : m)) });
  return saved;
}

// Delete heals references on other maps server-side — reload the whole list.
export async function deleteAtlasMap(mapId) {
  const id = store.campaign.campaign_id;
  await apiFetch(`/campaigns/${id}/atlas/maps/${encodeURIComponent(mapId)}`, { method: 'DELETE' });
  return loadAtlasMaps(id);
}

// ── Sessions ──────────────────────────────────────────────────────
export async function loadSession(id) {
  setState({ loading: true, error: null });
  try {
    const session = await apiFetch(`/session/${id}`);
    // ensure parent campaign is loaded for the sidebar
    const campId = session.campaign?.campaign_id;
    let campaign = store.campaign;
    if (campId && campaign?.campaign_id !== campId) {
      campaign = decorateCampaign(await apiFetch(`/campaigns/${campId}`));
    }
    // Don't swallow artifact-load failures: an empty list from a real error
    // would render a misleading red "not transcribed" badge. Surface it.
    let loadErr = null;
    const [transcripts, summaries] = await Promise.all([
      apiFetch(`/sessions/${id}/transcripts`).catch((e) => { loadErr = e; return []; }),
      apiFetch(`/sessions/${id}/summaries`).catch((e) => { loadErr = e; return []; }),
    ]);
    let summaryPreview = null;
    if ((summaries || []).length) {
      const latest = summaries[0];
      try { summaryPreview = { id: latest.id, text: await apiText(`/sessions/${id}/summaries/${latest.id}/content`) }; } catch (_) {}
    }
    const codexUpdate = (summaries || []).length
      ? await apiFetch(`/sessions/${id}/codex-update`).catch(() => null)
      : null;
    setState({ session, campaign, transcripts: transcripts || [], summaries: summaries || [], summaryPreview, codexUpdate, loading: false });
    if (loadErr) setOp(`Couldn't load this session's artifacts: ${loadErr.message}`, 'err');
    navigate('session', { id });
  } catch (e) { setState({ error: e.message, loading: false }); }
}

export async function createSession() {
  const id = store.campaign.campaign_id;
  const created = await apiJson(`/campaigns/${id}/sessions`, 'POST', {});
  return created; // { session_id, session_number }
}

// Fetch a full session object without touching the store/route (used to
// prefill the upload screen when attaching a recording to an existing session).
export function fetchSession(id) {
  return apiFetch(`/session/${id}`);
}

export async function deleteSession(sessionId) {
  const campId = store.session?.campaign?.campaign_id || store.campaign?.campaign_id;
  await apiFetch(`/sessions/${sessionId}`, { method: 'DELETE' });
  setState({ session: null, transcripts: [], summaries: [], summaryPreview: null });
  if (campId) await openCampaign(campId);
  else { await loadCampaigns(); navigate('library'); }
}

export async function uploadZip(sessionId, file) {
  const fd = new FormData();
  fd.append('file', file);
  fd.append('session_id', sessionId);
  const data = await apiFetch('/upload', { method: 'POST', body: fd });
  return data.tracks || [];
}

export function saveSpeakers(sessionId, speakers) {
  return apiJson('/label-speakers', 'POST', { session_id: sessionId, speakers });
}

export function saveSessionMetadata(payload) {
  return apiJson('/session-metadata', 'POST', payload);
}

// Save a manually-edited summary body. The next re-summary still overwrites it.
export async function saveSummaryEdit(artifactId, content) {
  const sid = store.session?.session_id;
  if (!sid) return;
  await apiJson(`/sessions/${sid}/summaries/${artifactId}`, 'PUT', { content });
  await loadSession(sid);
}

export async function deleteArtifact(kind, artifactId) {
  const sid = store.session?.session_id;
  if (!sid) return;
  await apiFetch(`/sessions/${sid}/${kind}/${artifactId}`, { method: 'DELETE' });
  await loadSession(sid);
}

export function artifactContent(kind, artifactId) {
  const sid = store.session?.session_id;
  return apiText(`/sessions/${sid}/${kind}/${artifactId}/content`);
}

// ── Transcription ─────────────────────────────────────────────────
const MB = 1024 * 1024;
function bar(frac, w = 14) { const f = Math.max(0, Math.min(w, Math.round(frac * w))); return '█'.repeat(f) + '░'.repeat(w - f); }
function pollModelStatus() {
  let stopped = false;
  const tick = async () => {
    if (stopped) return;
    try {
      const p = await apiFetch('/model-status');
      if (stopped) return;
      if (p.phase === 'downloading') {
        if (p.total > 0) {
          const pct = Math.round((p.downloaded / p.total) * 100);
          setOp(`Downloading model ${bar(p.downloaded / p.total)} ${pct}% (${(p.downloaded / MB).toFixed(0)}/${(p.total / MB).toFixed(0)} MB)`);
        } else setOp(`Downloading model… ${(p.downloaded / MB).toFixed(0)} MB`);
      } else if (p.phase === 'extracting') setOp('Extracting model…');
      else if (p.phase === 'error') {
        setOp(p.message || 'Model download failed.', 'err');
        stopped = true; return;
      }
      else if (p.phase === 'transcribing') {
        const tot = p.track_total || 0;
        if (tot > 0) {
          const done = p.track_done || 0;
          const cur = Math.min(done + 1, tot);
          const frac = done / tot;
          const who = p.message ? ` — ${p.message}` : '';
          setOp(`Transcribing track ${cur}/${tot}${who} ${bar(frac)} ${Math.round(frac * 100)}%`);
        } else setOp('Transcribing…');
      }
    } catch (_) {}
    if (!stopped) setTimeout(tick, 500);
  };
  tick();
  return () => { stopped = true; };
}

// On-device, single engine, language from the campaign — no options, just run.
export async function runTranscribe() {
  const sid = store.session?.session_id;
  if (!sid) return;
  setOp('Transcribing…');
  const stop = pollModelStatus();
  try {
    await apiJson('/transcribe', 'POST', { session_id: sid });
    await loadSession(sid);
    await refreshCampaignSessions();
    setOp('Transcription complete', 'done');
  } catch (e) { setOp(e.message, 'err'); }
  finally { stop(); }
}

// ── Summary prompt templates ──────────────────────────────────────
// The user-managed library of system prompts. Two builtins (EN/DE) are seeded
// server-side; the user can add, edit, delete (even the builtins) and restore.
export async function loadPromptTemplates(force) {
  if (store.promptTemplates && !force) return store.promptTemplates;
  let list = [];
  try { list = await apiFetch('/prompt-templates'); } catch (e) { console.warn('loadPromptTemplates failed:', e); }
  setState({ promptTemplates: list });
  return list;
}

export async function createPromptTemplate({ label, text }) {
  const created = await apiJson('/prompt-templates', 'POST', { label, text });
  await loadPromptTemplates(true);
  return created;
}

export async function updatePromptTemplate(id, patch) {
  const updated = await apiJson(`/prompt-templates/${id}`, 'PUT', patch);
  await loadPromptTemplates(true);
  return updated;
}

export async function deletePromptTemplate(id) {
  await apiFetch(`/prompt-templates/${id}`, { method: 'DELETE' });
  await loadPromptTemplates(true);
}

export async function restorePromptDefaults() {
  const list = await apiJson('/prompt-templates/restore-defaults', 'POST', {});
  setState({ promptTemplates: list });
  return list;
}

export async function runSummarize({ transcriptId, provider, model, title, context, systemPrompt }) {
  const sid = store.session?.session_id;
  if (!sid) return;
  setOp(`Summarizing with ${provider}…`);
  setState({ summaryStreaming: { stage: 'reading', text: '' } });
  try {
    let acc = '';
    let failure = null;
    await apiStream('/summarize/stream', {
      session_id: sid, transcript_id: transcriptId || null, provider, model: model || null,
      base_url: null, title: title || null, context: context || null, system_prompt: systemPrompt || null,
    }, (ev) => {
      switch (ev.stage) {
        case 'reading':
          setState({ summaryStreaming: { stage: 'reading', text: acc } });
          break;
        case 'writing':
          acc += ev.token || '';
          setState({ summaryStreaming: { stage: 'writing', text: acc } });
          break;
        case 'metadata':
          setState({ summaryStreaming: { stage: 'metadata', text: acc } });
          break;
        case 'done':
          setState({ summaryStreaming: null });
          break;
        case 'error':
          failure = ev.message || 'Summarization failed.';
          break;
      }
    });
    if (failure) throw new Error(failure);
    await loadSession(sid);
    await refreshCampaignSessions();
    // Auto-extract may have created stub pages; refresh so a later codex visit
    // shows them without a manual reload.
    await loadVaultTree(store.campaign?.campaign_id);
    bump('vault'); // an open extracted page should refresh its body
    setOp('Summary complete', 'done');
  } catch (e) {
    setState({ summaryStreaming: null });
    setOp(e.message, 'err');
  }
}

// ── Update the Codex (Phase 5) ────────────────────────────────────
// Proposals are ephemeral until commit; the backend keeps one JSON run per
// session (Sessions/NNN/codex-proposals.json). Decisions persist as the user
// reviews, so a half-reviewed run survives a restart.
export async function loadCodexUpdate(sessionId) {
  const sid = sessionId || store.session?.session_id;
  if (!sid) return null;
  const run = await apiFetch(`/sessions/${sid}/codex-update`).catch(() => null);
  setState({ codexUpdate: run });
  return run;
}

export async function runCodexUpdate() {
  const sid = store.session?.session_id;
  if (!sid) return;
  setState({ codexUpdateStreaming: { stage: 'candidates' } });
  try {
    let failure = null;
    await apiStream(`/sessions/${sid}/codex-update`, {}, (ev) => {
      switch (ev.stage) {
        case 'candidates':
        case 'grounding':
          setState({ codexUpdateStreaming: { stage: ev.stage } });
          break;
        case 'done':
          setState({ codexUpdate: ev.run, codexUpdateStreaming: null });
          break;
        case 'error':
          failure = ev.message || 'Codex update failed.';
          break;
      }
    });
    if (failure) throw new Error(failure);
  } catch (e) {
    setState({ codexUpdateStreaming: null });
    setOp(e.message, 'err');
  }
}

// Persist decisions / edited changes / skip. patch: { status?, proposals?: [{id, decision?, changes?}] }
export async function saveCodexUpdateDecisions(patch) {
  const sid = store.session?.session_id;
  if (!sid) return;
  const run = await apiJson(`/sessions/${sid}/codex-update`, 'PUT', patch);
  setState({ codexUpdate: run });
  return run;
}

export async function commitCodexUpdate(ids) {
  const sid = store.session?.session_id;
  if (!sid) return;
  setOp('Writing to the Codex…');
  try {
    const r = await apiJson(`/sessions/${sid}/codex-update/commit`, 'POST', { ids });
    await loadCodexUpdate(sid);
    await loadVaultTree(store.campaign?.campaign_id);
    bump('vault'); // a committed page open in a tab should refresh its body
    const stale = r.stale?.length ? ` · ${r.stale.length} stale (page changed)` : '';
    setOp(`Updated ${r.applied} page${r.applied === 1 ? '' : 's'}${stale}`, r.stale?.length ? 'err' : 'done');
    return r;
  } catch (e) {
    setOp(e.message, 'err');
    throw e;
  }
}

// ── Export ────────────────────────────────────────────────────────
export async function runExport(summaryId) {
  const sid = store.session?.session_id;
  if (!sid) return;
  try {
    const data = await apiJson('/export', 'POST', { session_id: sid, summary_id: summaryId, use_obsidian_format: true });
    setOp(data.path ? `Exported to ${data.path}` : `Exported ${data.filename}`, 'done');
  } catch (e) { setOp(e.message, 'err'); }
}

// ── Migration ─────────────────────────────────────────────────────
export async function checkMigration() {
  try {
    const status = await apiFetch('/migrations/status');
    setState({ migrationStatus: status });
  } catch (e) {
    // Non-fatal — if status check fails, don't block the app.
    console.warn('checkMigration failed:', e);
  }
}

export async function runMigration() {
  setState({ migrationRunning: true, migrationResult: null });
  try {
    const result = await apiJson('/migrations/run', 'POST', {});
    setState({ migrationRunning: false, migrationResult: result, migrationStatus: { needs_migration: false, campaigns: [] } });
    return result;
  } catch (e) {
    setState({ migrationRunning: false, migrationResult: { ok: false, errors: [e.message] } });
    throw e;
  } finally {
    // Boot loads ran against an empty DB — refetch what migration changed.
    loadCampaigns().catch(() => {});
    loadConfig().then(() => refreshProviderStatus()).catch(() => {});
  }
}

// ── Config + LLM providers ────────────────────────────────────────
export async function loadConfig() {
  const config = await apiFetch('/config');
  setState({ config });
  if (!store.llmProviders) {
    try { setState({ llmProviders: await apiFetch('/llm-providers') }); }
    catch (e) { console.warn('loadConfig: llm-providers fetch failed:', e); }
  }
  return config;
}

export async function saveConfig(payload, apiBaseValue) {
  if (apiBaseValue) { store.apiBase = apiBaseValue; localStorage.setItem('ck_api_base', apiBaseValue); }
  const updated = await apiJson('/config', 'PUT', payload);
  setState({ config: updated });
  refreshProviderStatus();
  return updated;
}

// Foundry bridge (Phase 23) — codex → Journals projection.
export async function loadFoundrySettings() {
  return apiFetch('/foundry/settings');
}
export async function saveFoundrySettings(payload) {
  return apiJson('/foundry/settings', 'PUT', payload);
}
export async function testFoundry() {
  return apiJson('/foundry/test', 'POST', {});
}
export async function syncFoundry(campaignId) {
  return apiJson(`/campaigns/${campaignId}/foundry/sync`, 'POST', {});
}

export async function loadLlmProviders(force) {
  if (store.llmProviders && !force) return store.llmProviders;
  let list = [];
  try { list = await apiFetch('/llm-providers'); } catch (e) { console.warn('loadLlmProviders failed:', e); }
  setState({ llmProviders: list });
  return list;
}

export async function saveLlmProvider(id, body) {
  const result = await apiJson(`/llm-providers/${id}`, 'PUT', body);
  const llmProviders = (store.llmProviders || []).map((x) =>
    x.id === id ? { ...x, has_key: result.has_key, has_custom_base: result.has_custom_base, saved_model: result.saved_model } : x);
  setState({ llmProviders });
  refreshProviderStatus();
  return result;
}

export function testLlmProvider(id, model) {
  return apiJson(`/llm-providers/${id}/test`, 'POST', { model: model || null });
}

export function pingLlmProvider(id) {
  return apiFetch(`/llm-providers/${id}/ping`);
}

// Live model list (Ollama only); empty array for other providers.
export async function fetchLlmModels(id) {
  try { return (await apiFetch(`/llm-providers/${id}/models`)).models || []; }
  catch (_) { return []; }
}

// Status of the active summary provider for the sidebar badge. Only flags real
// problems: nothing selected, a keyed provider with no key, or Ollama down.
export async function refreshProviderStatus() {
  const cfg = store.config;
  if (!cfg) return;
  const id = (cfg.summary_provider || 'ollama').toLowerCase();
  const p = (store.llmProviders || []).find((x) => x.id === id);
  let status = { ok: true };
  if (!p) {
    status = { ok: false, reason: 'No LLM provider selected' };
  } else if (p.needs_key && !p.has_key) {
    status = { ok: false, reason: `${p.name}: no API key set` };
  } else if (id === 'ollama' || id === 'ollama-cloud') {
    try {
      const r = await pingLlmProvider(id);
      if (!r.ok) status = { ok: false, reason: `${p.name} not reachable` };
    } catch (_) { status = { ok: false, reason: `${p.name} not reachable` }; }
  }
  setState({ providerStatus: status });
}
