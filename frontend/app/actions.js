// All data operations. Thin wrappers over the HTTP client that update the store.
// Ported 1:1 from the legacy app.js so the backend contract is unchanged.
import { store, setState, setOp, navigate, apiFetch, apiJson, apiText, apiUrl, slugify, toneFor, initials } from './core.js';

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
    const [sessions, codexEntries] = await Promise.all([
      apiFetch(`/campaigns/${id}/sessions`).catch(() => []),
      apiFetch(`/campaigns/${id}/codex/entries`).catch(() => []),
    ]);
    setState({
      campaign,
      campaignSessions: sessions || [],
      codexEntries: codexEntries || [],
      loading: false,
    });
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
  await apiJson('/campaigns', 'POST', { campaign_id: id, name: form.name, start_session_number: Number(form.start) || 1 });
  await apiJson(`/campaigns/${id}`, 'PUT', {
    name: form.name, system: form.system, setting: form.setting,
    default_language: form.default_language, gm: form.gm, gm_pronouns: form.gm_pronouns,
    players: form.players, extra_info: form.extra_info,
  });
  await loadCampaigns();
  await openCampaign(id);
  return id;
}

export async function updateCampaign(patch) {
  const id = store.campaign.campaign_id;
  const updated = decorateCampaign(await apiJson(`/campaigns/${id}`, 'PUT', patch));
  setState({ campaign: updated });
  await loadCampaigns();
  // Roster edits auto-create `pc` codex entries server-side — refresh so they show.
  if (patch && patch.players) await loadCodexEntries(id);
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
  setState({ campaign: null, campaignSessions: [], codexEntries: [] });
  await loadCampaigns();
  navigate('library');
}

// ── Codex entries ─────────────────────────────────────────────────
export async function loadCodexEntries(campaignId) {
  const id = campaignId || store.campaign?.campaign_id;
  if (!id) return [];
  const entries = await apiFetch(`/campaigns/${id}/codex/entries`).catch((e) => {
    console.warn('loadCodexEntries failed:', e);
    return [];
  });
  setState({ codexEntries: entries || [] });
  return entries || [];
}

export async function createCodexEntry({ name, kind, body, detail }) {
  const id = store.campaign.campaign_id;
  await apiJson(`/campaigns/${id}/codex/entries`, 'POST', { name, kind, body: body || '', detail: detail || '' });
  await loadCodexEntries(id);
}

export async function updateCodexEntry(entryId, patch) {
  const id = store.campaign.campaign_id;
  await apiJson(`/campaigns/${id}/codex/entries/${entryId}`, 'PUT', patch);
  await loadCodexEntries(id);
}

export async function deleteCodexEntry(entryId) {
  const id = store.campaign.campaign_id;
  await apiFetch(`/campaigns/${id}/codex/entries/${entryId}`, { method: 'DELETE' });
  await loadCodexEntries(id);
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

// Sessions whose saved metadata (characters/locations/items) name this entry.
// Pure client-side over already-loaded campaignSessions — no telemetry, no new
// backend. Case-insensitive exact match on a metadata value.
export function mentionsOf(name) {
  const needle = String(name || '').trim().toLowerCase();
  if (!needle) return [];
  return (store.campaignSessions || [])
    .filter((s) => {
      const md = s.metadata || {};
      return ['characters', 'locations', 'items'].some((k) =>
        (md[k] || []).some((v) => String(v).trim().toLowerCase() === needle));
    })
    .map((s) => ({ session_id: s.session_id, session_number: s.session_number, title: s.title }));
}

// Distill pasted notes into proposed entries (not saved yet — the user reviews).
export async function importCodex(text) {
  const id = store.campaign.campaign_id;
  const r = await apiJson(`/campaigns/${id}/codex/import`, 'POST', { text });
  return r.entries || [];
}

// Save the reviewed entries; returns { created, skipped }.
export async function commitCodexImport(entries) {
  const id = store.campaign.campaign_id;
  const r = await apiJson(`/campaigns/${id}/codex/import/commit`, 'POST', { entries });
  await loadCodexEntries(id);
  return r;
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
    const [transcripts, summaries, codexEntries] = await Promise.all([
      apiFetch(`/sessions/${id}/transcripts`).catch((e) => { loadErr = e; return []; }),
      apiFetch(`/sessions/${id}/summaries`).catch((e) => { loadErr = e; return []; }),
      campId ? apiFetch(`/campaigns/${campId}/codex/entries`).catch(() => []) : Promise.resolve([]),
    ]);
    let summaryPreview = null;
    if ((summaries || []).length) {
      const latest = summaries[0];
      try { summaryPreview = { id: latest.id, text: await apiText(`/sessions/${id}/summaries/${latest.id}/content`) }; } catch (_) {}
    }
    setState({ session, campaign, transcripts: transcripts || [], summaries: summaries || [], summaryPreview, codexEntries: codexEntries || [], loading: false });
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

// ── Summarize ─────────────────────────────────────────────────────
export async function loadPromptPresets() {
  if (store.promptPresets) return store.promptPresets;
  let p = {};
  try { p = await apiFetch('/prompts'); } catch (e) { console.warn('loadPromptPresets failed:', e); }
  setState({ promptPresets: p });
  return p;
}

export async function runSummarize({ transcriptId, provider, model, title, context, systemPrompt }) {
  const sid = store.session?.session_id;
  if (!sid) return;
  setOp(`Summarizing with ${provider}…`);
  try {
    await apiJson('/summarize', 'POST', {
      session_id: sid, transcript_id: transcriptId || null, provider, model: model || null,
      base_url: null, title: title || null, context: context || null, system_prompt: systemPrompt || null,
    });
    await loadSession(sid);
    await refreshCampaignSessions();
    // Auto-extract may have grown the codex; refresh so a later codex visit
    // shows the new entries without a manual reload.
    await loadCodexEntries(store.campaign?.campaign_id);
    setOp('Summary complete', 'done');
  } catch (e) { setOp(e.message, 'err'); }
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
