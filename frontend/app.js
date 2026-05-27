const state = {
  apiBase: "http://127.0.0.1:8000",
  apiToken: null,
  configFromApi: null,
  campaigns: [],
  campaignDetails: [],
  currentCampaign: null,
  campaignSessions: [],
  currentSession: null,
  sessionTracks: [],
  sessionSpeakers: [],
  transcripts: [],
  summaries: [],
  selectedTranscriptId: null,
  providers: null,
  promptPresets: null,
  llmProviders: null,
  campaignWizard: { step: 1, mode: "create" },
  sessionWizard: { step: 1, sessionId: null },
};

const qs = (id) => document.getElementById(id);
const qsa = (selector) => document.querySelectorAll(selector);

function showScreen(screenName) {
  qsa(".main-screen").forEach((screen) => screen.classList.remove("active"));
  const next = qs(`${screenName}-screen`);
  if (next) next.classList.add("active");
  qsa(".nav-btn").forEach((button) => {
    button.classList.toggle("active", button.dataset.screen === screenName);
  });
}

function openWizard(screenId) {
  const screen = qs(`${screenId}-screen`);
  if (screen) screen.classList.add("active");
}

function closeWizard(screenId) {
  const screen = qs(`${screenId}-screen`);
  if (screen) screen.classList.remove("active");
}

function setStatus(el, message, isError = false) {
  if (!el) return;
  const text = message || "";
  el.textContent = text;
  el.className = `status${isError ? " error" : ""}${text ? "" : " hidden"}`;
}

function setSessionOpStatus(message, state = "") {
  const el = qs("session-op-status");
  if (!el) return;
  if (!message) {
    el.classList.add("hidden");
    el.textContent = "";
    return;
  }
  el.textContent = message;
  el.className = `session-op-status${state ? ` ${state}` : ""}`;
  if (state === "done" || state === "err") {
    setTimeout(() => setSessionOpStatus(null), 4000);
  }
}

const MB = 1024 * 1024;

// Render a short unicode progress bar, e.g. "███████░░░░░░░".
function progressBar(fraction, width = 14) {
  const filled = Math.max(0, Math.min(width, Math.round(fraction * width)));
  return "█".repeat(filled) + "░".repeat(width - filled);
}

// Poll GET /model-status and reflect download/extract progress in the status
// line. Returns a stop() that cancels the poll. No-op once the model is present
// (status stays "idle"/"ready", so the line keeps showing "Transcribing…").
function pollModelStatus() {
  let stopped = false;
  const tick = async () => {
    if (stopped) return;
    try {
      const p = await apiFetch("/model-status");
      if (stopped) return;
      if (p.phase === "downloading") {
        if (p.total > 0) {
          const pct = Math.round((p.downloaded / p.total) * 100);
          setSessionOpStatus(
            `Downloading model ${progressBar(p.downloaded / p.total)} ${pct}% ` +
              `(${(p.downloaded / MB).toFixed(0)} / ${(p.total / MB).toFixed(0)} MB)`
          );
        } else {
          setSessionOpStatus(`Downloading model… ${(p.downloaded / MB).toFixed(0)} MB`);
        }
      } else if (p.phase === "extracting") {
        setSessionOpStatus("Extracting model…");
      }
      // idle/ready/error: leave the surrounding "Transcribing…"/error message as-is.
    } catch {
      // Ignore poll errors; the transcribe call itself surfaces real failures.
    }
    if (!stopped) setTimeout(tick, 500);
  };
  tick();
  return () => {
    stopped = true;
  };
}

function apiUrl(path) {
  return `${state.apiBase}${path}`;
}

function authHeaders() {
  return state.apiToken ? { "X-CK-Token": state.apiToken } : {};
}

async function apiFetch(path, options = {}) {
  const opts = {
    ...options,
    headers: { ...(options.headers || {}), ...authHeaders() },
  };
  const response = await fetch(apiUrl(path), opts);
  if (!response.ok) {
    let detail = response.statusText;
    try {
      const data = await response.json();
      detail = data.detail || JSON.stringify(data);
    } catch (_) {
      // ignore
    }
    throw new Error(detail);
  }
  return response.json();
}

function loadApiBase() {
  // The Tauri shell injects the embedded server's base URL + per-launch token
  // before page scripts run. Fall back to localStorage for the standalone dev
  // server (browser against `ck-serve`).
  if (window.__CK_API_BASE__) {
    state.apiBase = window.__CK_API_BASE__;
  } else {
    const saved = localStorage.getItem("ck_api_base");
    if (saved) state.apiBase = saved;
  }
  if (window.__CK_TOKEN__) state.apiToken = window.__CK_TOKEN__;
  const apiField = qs("setting-api-base");
  if (apiField) apiField.value = state.apiBase;
}


function slugify(value) {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)+/g, "");
}

function generateCampaignId(name) {
  const base = slugify(name) || "campaign";
  const existing = new Set(state.campaigns.map((item) => item.campaign_id));
  if (!existing.has(base)) return base;
  let counter = 2;
  while (existing.has(`${base}-${counter}`)) {
    counter += 1;
  }
  return `${base}-${counter}`;
}

function renderCampaignCards() {
  const list = qs("campaign-list");
  const empty = qs("campaign-empty");
  list.innerHTML = "";
  if (!state.campaignDetails.length) {
    empty.classList.remove("hidden");
    return;
  }
  empty.classList.add("hidden");
  state.campaignDetails.forEach((campaign) => {
    const card = document.createElement("div");
    card.className = "card";
    card.innerHTML = `
      <div class="card-title">
        <h3>${campaign.name}</h3>
        <p class="muted">${campaign.system || "System"} · ${campaign.gm || "GM"}</p>
      </div>
      <div class="meta-list">
        <div>
          <dt>Players</dt>
          <dd>${campaign.players?.length || 0}</dd>
        </div>
        <div>
          <dt>Next session</dt>
          <dd>${campaign.next_session_number || 1}</dd>
        </div>
      </div>
    `;
    const button = document.createElement("button");
    button.className = "btn secondary";
    button.textContent = "Open";
    button.addEventListener("click", () => openCampaign(campaign.campaign_id));
    card.appendChild(button);
    list.appendChild(card);
  });
}

function renderCampaignOverview() {
  const campaign = state.currentCampaign;
  if (!campaign) return;
  qs("campaign-title").textContent = campaign.name || "Campaign";
  qs("campaign-subtitle").textContent = campaign.system || "";
  qs("campaign-system-display").textContent = campaign.system || "-";
  qs("campaign-gm-display").textContent = campaign.gm || "-";
  qs("campaign-setting-display").textContent = campaign.setting || "-";
  qs("campaign-language-display").textContent = campaign.default_language || "-";
  qs("campaign-extra-display").value = campaign.extra_info || "No additional info yet.";

  const playerList = qs("campaign-player-list");
  playerList.innerHTML = "";
  if (!campaign.players || campaign.players.length === 0) {
    const row = document.createElement("tr");
    row.innerHTML = `<td class="muted" colspan="2">No players yet</td>`;
    playerList.appendChild(row);
    return;
  }
  (campaign.players || []).forEach((player) => {
    const row = document.createElement("tr");
    row.innerHTML = `
      <td>${player.player_name || "-"}</td>
      <td>${player.character_name || "-"}</td>
    `;
    playerList.appendChild(row);
  });
}

function toggleCampaignEdit(show) {
  if (show) {
    populateCampaignEdit();
    openModal("campaign-edit-modal");
  } else {
    closeModal("campaign-edit-modal");
  }
}

function populateCampaignEdit() {
  const campaign = state.currentCampaign;
  if (!campaign) return;
  qs("campaign-edit-system").value = campaign.system || "";
  qs("campaign-edit-gm").value = campaign.gm || "";
  qs("campaign-edit-setting").value = campaign.setting || "";
  qs("campaign-edit-language").value = campaign.default_language || "";
  qs("campaign-edit-extra").value = campaign.extra_info || "";
  renderPlayerRows("campaign-player-rows", campaign.players || []);
}

async function saveCampaignEdit() {
  const campaign = state.currentCampaign;
  if (!campaign) return;
  const payload = {
    system: qs("campaign-edit-system").value.trim(),
    gm: qs("campaign-edit-gm").value.trim(),
    setting: qs("campaign-edit-setting").value.trim(),
    default_language: qs("campaign-edit-language").value.trim(),
    extra_info: qs("campaign-edit-extra").value.trim(),
    players: collectPlayerRows("campaign-player-rows"),
  };
  const updated = await apiFetch(`/campaigns/${campaign.campaign_id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  state.currentCampaign = updated;
  renderPlayerOptions(updated.players || []);
  renderCampaignOverview();
  closeModal("campaign-edit-modal");
  setStatus(qs("campaign-edit-status"), "Campaign saved");
}

function renderCampaignSessions() {
  const list = qs("campaign-sessions");
  list.innerHTML = "";
  state.campaignSessions.forEach((session) => {
    const row = document.createElement("tr");
    const title = session.title || "-";
    const date = session.date || "-";
    const transcriptionStatus = session.has_transcription ? "Done" : "Pending";
    const summaryStatus = session.has_summary ? "Done" : "Pending";
    row.innerHTML = `
      <td>${session.session_number || "?"}</td>
      <td>${title}</td>
      <td>${date}</td>
      <td class="muted">${transcriptionStatus}</td>
      <td class="muted">${summaryStatus}</td>
    `;
    const openBtn = document.createElement("button");
    openBtn.className = "btn secondary";
    openBtn.textContent = "Open";
    openBtn.addEventListener("click", () => loadSession(session.session_id));
    const actionCell = document.createElement("td");
    actionCell.appendChild(openBtn);
    row.appendChild(actionCell);
    list.appendChild(row);
  });
}

function renderPlayerOptions(players = []) {
  const list = qs("player-options");
  if (!list) return;
  list.innerHTML = "";
  players.forEach((player) => {
    const option = document.createElement("option");
    option.value = player.player_name || "";
    list.appendChild(option);
  });
}

function renderPlayerRows(containerId, players = []) {
  const container = qs(containerId);
  if (!container) return;
  container.innerHTML = "";
  players.forEach((player, index) => {
    const row = document.createElement("div");
    row.className = "list-item";
    row.innerHTML = `
      <input data-field="player_name" data-index="${index}" placeholder="Player name" value="${
        player.player_name || ""
      }" />
      <input data-field="character_name" data-index="${index}" placeholder="PC name" value="${
        player.character_name || ""
      }" />
      <button type="button" class="btn ghost" data-action="remove-player" data-index="${index}">
        Remove
      </button>
    `;
    container.appendChild(row);
  });
}

function addPlayerRow(containerId) {
  const players = collectPlayerRows(containerId);
  players.push({ player_name: "", character_name: "" });
  renderPlayerRows(containerId, players);
}

function collectPlayerRows(containerId) {
  const container = qs(containerId);
  if (!container) return [];
  const rows = [];
  const inputs = container.querySelectorAll("input");
  const maxIndex = Math.max(
    ...Array.from(inputs).map((input) => Number(input.dataset.index || 0)),
    -1
  );
  for (let i = 0; i <= maxIndex; i += 1) {
    const playerInput = container.querySelector(
      `input[data-field="player_name"][data-index="${i}"]`
    );
    const characterInput = container.querySelector(
      `input[data-field="character_name"][data-index="${i}"]`
    );
    if (!playerInput && !characterInput) continue;
    const playerName = playerInput?.value.trim() || "";
    const characterName = characterInput?.value.trim() || "";
    if (!playerName && !characterName) continue;
    rows.push({ player_name: playerName, character_name: characterName });
  }
  return rows;
}

function renderSessionTracks() {
  const list = qs("session-track-list");
  list.innerHTML = "";
  state.sessionTracks.forEach((track) => {
    const li = document.createElement("li");
    li.textContent = `${track.filename} (${track.id})`;
    list.appendChild(li);
  });
}

function renderSpeakerTable() {
  const body = qs("session-speakers-body");
  body.innerHTML = "";
  const speakerMap = new Map(
    (state.sessionSpeakers || []).map((speaker) => [speaker.track_id, speaker])
  );
  state.sessionTracks.forEach((track, index) => {
    const existing = speakerMap.get(track.id) || {};
    const row = document.createElement("tr");
    row.innerHTML = `
      <td>${track.id}</td>
      <td><input data-field="player_name" data-index="${index}" list="player-options" value="${
        existing.player_name || ""
      }" /></td>
      <td><input data-field="character_name" data-index="${index}" value="${
        existing.character_name || ""
      }" /></td>
      <td><input data-field="pronouns" data-index="${index}" list="pronoun-options" value="${
        existing.pronouns || ""
      }" /></td>
    `;
    body.appendChild(row);
  });
}

function collectSpeakers() {
  const speakers = state.sessionTracks.map((track) => ({
    track_id: track.id,
    player_name: "",
    character_name: "",
    pronouns: "",
  }));
  qs("session-speakers-body")
    .querySelectorAll("input")
    .forEach((input) => {
      const index = Number(input.dataset.index);
      const field = input.dataset.field;
      speakers[index][field] = input.value.trim();
    });
  state.sessionSpeakers = speakers;
  return speakers;
}

function renderSessionOverview() {
  const session = state.currentSession;
  if (!session) return;
  const campaign = session.campaign || {};
  const title = campaign.title ? `: ${campaign.title}` : "";
  qs("session-title").textContent = `Session #${campaign.session_number || "?"}${title}`;
  qs("session-subtitle").textContent = campaign.date || "";
  qs("session-date-display").textContent = campaign.date || "-";
  qs("session-number-display").textContent = campaign.session_number || "-";
  qs("session-notes-display").textContent = campaign.notes || "No notes yet.";

  renderSessionMetadata();
  renderSessionSpeakers();
  renderSessionTranscripts();
  renderSessionSummaries();

  qs("open-transcribe-modal").disabled = !session.tracks?.length;
  qs("open-summarize-modal").disabled = !state.transcripts.length;
  qs("export-obsidian").disabled = !state.summaries.length;
}

function renderSessionMetadata() {
  const metadata = state.currentSession?.metadata || {};

  // Events — separate card as a list
  const events = metadata.events || [];
  const eventsCard = qs("session-events-card");
  const eventsList = qs("session-events-list");
  if (eventsList) {
    eventsList.innerHTML = "";
    events.forEach((value) => {
      const li = document.createElement("li");
      li.textContent = value;
      eventsList.appendChild(li);
    });
  }
  if (eventsCard) eventsCard.classList.toggle("hidden", !events.length);

  // Properties — chips for characters, locations, items, tags
  const categories = ["characters", "locations", "items", "tags"];
  const card = qs("session-metadata-card");
  let hasAny = false;

  categories.forEach((cat) => {
    const container = qs(`meta-${cat}`);
    if (!container) return;
    container.innerHTML = "";
    const values = metadata[cat] || [];
    values.forEach((value) => {
      const chip = document.createElement("span");
      chip.className = "metadata-chip";
      chip.textContent = value;
      container.appendChild(chip);
    });
    if (values.length) hasAny = true;
  });

  if (card) card.classList.toggle("hidden", !hasAny);
}

function renderSessionSpeakers() {
  const speakers = state.currentSession?.speakers || [];
  const body = qs("session-overview-speakers");
  const empty = qs("session-speakers-empty");
  body.innerHTML = "";
  if (!speakers.length) {
    empty.classList.remove("hidden");
    return;
  }
  empty.classList.add("hidden");
  speakers.forEach((speaker) => {
    const row = document.createElement("tr");
    row.innerHTML = `
      <td>${speaker.track_id || "-"}</td>
      <td>${speaker.player_name || "-"}</td>
      <td>${speaker.character_name || "-"}</td>
      <td>${speaker.pronouns || "-"}</td>
    `;
    body.appendChild(row);
  });
}

function renderSessionTranscripts() {
  const body = qs("session-overview-transcripts");
  const empty = qs("session-transcripts-empty");
  body.innerHTML = "";
  if (!state.transcripts.length) {
    empty.classList.remove("hidden");
    return;
  }
  empty.classList.add("hidden");
  state.transcripts.forEach((item) => {
    const row = document.createElement("tr");
    const date = new Date(item.created_at).toLocaleString();
    row.innerHTML = `
      <td>${item.provider} / ${item.model}</td>
      <td>${date}</td>
      <td>
        <button class="btn secondary" data-action="open-transcript" data-id="${item.id}">Open</button>
        <button class="btn ghost" data-action="delete-transcript" data-id="${item.id}">Delete</button>
      </td>
    `;
    body.appendChild(row);
  });
}

function renderSessionSummaries() {
  const body = qs("session-overview-summaries");
  const empty = qs("session-summaries-empty");
  body.innerHTML = "";
  if (!state.summaries.length) {
    empty.classList.remove("hidden");
    return;
  }
  empty.classList.add("hidden");
  state.summaries.forEach((item) => {
    const row = document.createElement("tr");
    const date = new Date(item.created_at).toLocaleString();
    row.innerHTML = `
      <td>${item.provider} / ${item.model}</td>
      <td>${date}</td>
      <td>
        <button class="btn secondary" data-action="open-summary" data-id="${item.id}">Open</button>
        <button class="btn ghost" data-action="delete-summary" data-id="${item.id}">Delete</button>
      </td>
    `;
    body.appendChild(row);
  });
}

function toggleSessionEdit(show) {
  if (show) {
    populateSessionEdit();
    openModal("session-edit-modal");
  } else {
    closeModal("session-edit-modal");
  }
}

function populateSessionEdit() {
  const session = state.currentSession;
  if (!session) return;
  const campaign = session.campaign || {};
  const metadata = session.metadata || {};
  qs("session-edit-title").value = campaign.title || "";
  qs("session-edit-date").value = campaign.date || "";
  qs("session-edit-number").value = campaign.session_number || "";
  qs("session-edit-notes").value = campaign.notes || "";
  qs("session-edit-characters").value = (metadata.characters || []).join(", ");
  qs("session-edit-locations").value = (metadata.locations || []).join(", ");
  qs("session-edit-items").value = (metadata.items || []).join(", ");
  qs("session-edit-tags").value = (metadata.tags || []).join(", ");
}

function collectMetadataFromInputs(prefix) {
  const parse = (id) =>
    qs(id).value.split(",").map((s) => s.trim()).filter(Boolean);
  // Preserve existing events (LLM-generated, not user-editable here)
  const existing = state.currentSession?.metadata || {};
  return {
    characters: parse(`${prefix}-characters`),
    locations: parse(`${prefix}-locations`),
    events: existing.events || [],
    items: parse(`${prefix}-items`),
    tags: parse(`${prefix}-tags`),
  };
}

async function saveSessionEdit() {
  const session = state.currentSession;
  if (!session) return;
  const metadata = collectMetadataFromInputs("session-edit");
  const payload = {
    session_id: session.session_id,
    campaign_id: session.campaign?.campaign_id || null,
    session_number: Number(qs("session-edit-number").value) || null,
    title: qs("session-edit-title").value.trim() || null,
    date: qs("session-edit-date").value || null,
    metadata,
    notes: qs("session-edit-notes").value.trim() || null,
  };
  await apiFetch("/session-metadata", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  await loadSession(session.session_id);
  closeModal("session-edit-modal");
  setStatus(qs("session-edit-status"), "Session saved");
}

function updateWizardStepper(containerId, step, total) {
  const container = qs(containerId);
  if (!container) return;
  container.innerHTML = "";
  for (let i = 1; i <= total; i += 1) {
    const dot = document.createElement("span");
    if (i === step) dot.classList.add("active");
    container.appendChild(dot);
  }
}

function updateWizardPanels(screenId, step) {
  const panels = qs(`${screenId}-screen`).querySelectorAll(".wizard-step-panel");
  panels.forEach((panel) => {
    const panelStep = Number(panel.dataset.step);
    panel.classList.toggle("active", panelStep === step);
  });
}

function updateCampaignWizardUI() {
  const step = state.campaignWizard.step;
  const total = 3;
  updateWizardStepper("campaign-wizard-steps", step, total);
  updateWizardPanels("campaign-wizard", step);
  qs("campaign-wizard-step-label").textContent = `Step ${step} of ${total}`;
  qs("campaign-wizard-back").disabled = step === 1;
  qs("campaign-wizard-next").textContent = step === total ? "Finish" : "Next";
}

function updateSessionWizardUI() {
  const step = state.sessionWizard.step;
  const total = 3;
  updateWizardStepper("session-wizard-steps", step, total);
  updateWizardPanels("session-wizard", step);
  qs("session-wizard-step-label").textContent = `Step ${step} of ${total}`;
  qs("session-wizard-back").disabled = step === 1;
  qs("session-wizard-next").textContent = step === total ? "Finish" : "Next";
}

async function loadCampaigns() {
  const data = await apiFetch("/campaigns");
  state.campaigns = data.campaigns || [];
  const details = await Promise.all(
    state.campaigns.map((campaign) => apiFetch(`/campaigns/${campaign.campaign_id}`))
  );
  state.campaignDetails = details;
  renderCampaignCards();
}

async function openCampaign(campaignId) {
  const campaign = await apiFetch(`/campaigns/${campaignId}`);
  state.currentCampaign = campaign;
  await refreshCampaignSessions();
  renderCampaignOverview();
  showScreen("campaign");
}

async function refreshCampaignSessions() {
  if (!state.currentCampaign?.campaign_id) return;
  const sessions = await apiFetch(`/campaigns/${state.currentCampaign.campaign_id}/sessions`);
  state.campaignSessions = sessions || [];
  renderCampaignSessions();
}

async function loadSession(sessionId) {
  const session = await apiFetch(`/session/${sessionId}`);
  state.currentSession = session;
  state.sessionTracks = session.tracks || [];
  state.sessionSpeakers = session.speakers || [];
  if (
    session.campaign?.campaign_id &&
    state.currentCampaign?.campaign_id !== session.campaign.campaign_id
  ) {
    await openCampaign(session.campaign.campaign_id);
  }
  await Promise.all([loadTranscripts(), loadSummaries()]);
  renderSessionOverview();
  showScreen("session");
}

function openCampaignWizard(mode = "create") {
  state.campaignWizard = { step: 1, mode };
  qs("campaign-wizard-title").textContent = mode === "edit" ? "Edit campaign" : "New campaign";
  qs("campaign-wizard-status").textContent = "";
  if (mode === "edit" && state.currentCampaign) {
    const campaign = state.currentCampaign;
    qs("wizard-campaign-name").value = campaign.name || "";
    qs("wizard-campaign-system").value = campaign.system || "";
    qs("wizard-campaign-setting").value = campaign.setting || "";
    qs("wizard-campaign-language").value = campaign.default_language || "";
    qs("wizard-campaign-start").value = campaign.next_session_number || "";
    qs("wizard-campaign-gm").value = campaign.gm || "";
    renderPlayerRows("wizard-player-rows", campaign.players || []);
    qs("wizard-campaign-extra").value = campaign.extra_info || "";
  } else {
    qsa("#campaign-wizard-screen input, #campaign-wizard-screen textarea").forEach((el) => {
      el.value = "";
    });
    qs("wizard-campaign-start").value = "1";
    renderPlayerRows("wizard-player-rows", [{ player_name: "", character_name: "" }]);
  }
  updateCampaignWizardUI();
  openWizard("campaign-wizard");
}

async function saveCampaignWizard() {
  const name = qs("wizard-campaign-name").value.trim();
  if (!name) {
    setStatus(qs("campaign-wizard-status"), "Name is required", true);
    return false;
  }
  const campaignId =
    state.campaignWizard.mode === "edit" && state.currentCampaign
      ? state.currentCampaign.campaign_id
      : generateCampaignId(name);
  const payload = {
    name,
    system: qs("wizard-campaign-system").value.trim(),
    setting: qs("wizard-campaign-setting").value.trim(),
    default_language: qs("wizard-campaign-language").value.trim(),
    gm: qs("wizard-campaign-gm").value.trim(),
    players: collectPlayerRows("wizard-player-rows"),
    extra_info: qs("wizard-campaign-extra").value.trim(),
  };

  if (state.campaignWizard.mode === "create") {
    const startNumber = Number(qs("wizard-campaign-start").value) || 1;
    await apiFetch("/campaigns", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        campaign_id: campaignId,
        name,
        start_session_number: startNumber,
      }),
    });
  }

  await apiFetch(`/campaigns/${campaignId}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });

  setStatus(qs("campaign-wizard-status"), "Campaign saved");
  await loadCampaigns();
  await openCampaign(campaignId);
  closeWizard("campaign-wizard");
  return true;
}

async function openSessionWizard() {
  if (!state.currentCampaign) return;
  state.sessionWizard = { step: 1, sessionId: null };
  setStatus(qs("session-upload-status"), "");
  setStatus(qs("session-speakers-status"), "");
  setStatus(qs("session-meta-status"), "");
  qs("session-track-list").innerHTML = "";
  qs("session-speakers-body").innerHTML = "";
  qs("session-meta-title").value = "";
  qs("session-meta-date").value = new Date().toISOString().slice(0, 10);
  qs("session-meta-number").value = "";
  qs("session-meta-notes").value = "";

  const created = await apiFetch(
    `/campaigns/${state.currentCampaign.campaign_id}/sessions`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    }
  );
  state.sessionWizard.sessionId = created.session_id;
  qs("session-meta-number").value = created.session_number || "";
  renderPlayerOptions(state.currentCampaign.players || []);
  updateSessionWizardUI();
  openWizard("session-wizard");
}

async function saveSessionMetadata() {
  if (!state.sessionWizard.sessionId) return false;
  const tagsRaw = qs("session-meta-tags").value.split(",").map((s) => s.trim()).filter(Boolean);
  const payload = {
    session_id: state.sessionWizard.sessionId,
    campaign_id: state.currentCampaign?.campaign_id || null,
    session_number: Number(qs("session-meta-number").value) || null,
    title: qs("session-meta-title").value.trim() || null,
    date: qs("session-meta-date").value || null,
    metadata: { characters: [], locations: [], events: [], items: [], tags: tagsRaw },
    notes: qs("session-meta-notes").value.trim() || null,
  };
  await apiFetch("/session-metadata", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  setStatus(qs("session-meta-status"), "Session saved");
  return true;
}

async function saveSpeakers() {
  if (!state.sessionWizard.sessionId) return false;
  const speakers = collectSpeakers();
  await apiFetch("/label-speakers", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ session_id: state.sessionWizard.sessionId, speakers }),
  });
  setStatus(qs("session-speakers-status"), "Speakers saved");
  return true;
}

function openModal(modalId) {
  qs("modal-backdrop").classList.remove("hidden");
  qs(modalId).classList.remove("hidden");
}

function closeModal(modalId) {
  qs(modalId).classList.add("hidden");
  qs("modal-backdrop").classList.add("hidden");
}

async function loadTranscripts() {
  if (!state.currentSession?.session_id) return [];
  const list = await apiFetch(`/sessions/${state.currentSession.session_id}/transcripts`);
  state.transcripts = list || [];
  return state.transcripts;
}

async function loadSummaries() {
  if (!state.currentSession?.session_id) return [];
  const list = await apiFetch(`/sessions/${state.currentSession.session_id}/summaries`);
  state.summaries = list || [];
  return state.summaries;
}

function populateTranscriptSelect() {
  const select = qs("modal-transcript-select");
  select.innerHTML = "";
  if (!state.transcripts.length) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = "No transcripts found";
    select.appendChild(option);
    select.disabled = true;
    state.selectedTranscriptId = null;
    return;
  }
  select.disabled = false;
  state.transcripts.forEach((item, index) => {
    const option = document.createElement("option");
    option.value = item.id;
    option.textContent = `${item.provider} / ${item.model} (${new Date(item.created_at).toLocaleString()})`;
    if (index === 0) option.selected = true;
    select.appendChild(option);
  });
  state.selectedTranscriptId = Number(select.value) || null;
}

function populateExportSummarySelect() {
  const select = qs("modal-export-summary-select");
  if (!select) return;
  select.innerHTML = "";
  if (!state.summaries.length) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = "No summaries found";
    select.appendChild(option);
    select.disabled = true;
    return;
  }
  select.disabled = false;
  state.summaries.forEach((item, index) => {
    const option = document.createElement("option");
    option.value = String(item.id);
    option.textContent = `${item.provider} / ${item.model} (${new Date(item.created_at).toLocaleString()})`;
    if (index === 0) option.selected = true;
    select.appendChild(option);
  });
}

async function loadPromptPresets() {
  if (state.promptPresets) return state.promptPresets;
  try {
    state.promptPresets = await apiFetch("/prompts");
  } catch {
    state.promptPresets = {};
  }
  return state.promptPresets;
}

function populatePromptPresetSelect() {
  const select = qs("modal-prompt-preset");
  const textarea = qs("modal-system-prompt");
  select.innerHTML = "";

  const presets = state.promptPresets || {};
  const keys = Object.keys(presets);

  if (!keys.length) {
    const opt = document.createElement("option");
    opt.value = "";
    opt.textContent = "No presets available";
    select.appendChild(opt);
    select.disabled = true;
    textarea.value = "";
    return;
  }

  select.disabled = false;

  keys.forEach((key, index) => {
    const opt = document.createElement("option");
    opt.value = key;
    opt.textContent = presets[key].label;
    if (index === 0) opt.selected = true;
    select.appendChild(opt);
  });

  // Add "Custom" option at the end
  const customOpt = document.createElement("option");
  customOpt.value = "__custom__";
  customOpt.textContent = "Custom";
  select.appendChild(customOpt);

  // Populate textarea with first preset
  textarea.value = presets[keys[0]].text;

  select.onchange = () => {
    const val = select.value;
    if (val && val !== "__custom__" && presets[val]) {
      textarea.value = presets[val].text;
    }
    // When "Custom" is selected, leave the textarea as-is for manual editing
  };
}

async function refreshConfigFromApi() {
  const config = await apiFetch("/config");
  state.configFromApi = config;
  return config;
}

function toggleSummarizeModalProviderFields() {
  populateSummarizeModalModels();
}

function populateSummarizeModalModels() {
  const providerSelect = qs("modal-summary-provider");
  const modelInput = qs("modal-summary-model");
  const datalist = qs("modal-summary-model-datalist");
  if (!providerSelect || !modelInput) return;

  const v = providerSelect.value;
  const provider = (state.llmProviders || []).find((p) => p.id === v);

  if (datalist) {
    datalist.innerHTML = (provider?.models || []).map((m) => `<option value="${m}">`).join("");
  }
  if (provider) {
    modelInput.value = provider.saved_model || provider.default_model;
    modelInput.placeholder = provider.default_model;
  } else {
    modelInput.value = "";
    modelInput.placeholder = "Model name";
  }
}

function populateProviderSelect(selectId, providers) {
  const sel = qs(selectId);
  if (!sel) return;
  const current = sel.value;
  sel.innerHTML = "";
  providers.forEach((p) => {
    const opt = document.createElement("option");
    opt.value = p.id;
    const label = p.needs_key ? (p.has_key ? `${p.name} ✓` : `${p.name} (no key)`) : p.name;
    opt.textContent = label;
    sel.appendChild(opt);
  });
  // restore selection if still valid
  if ([...sel.options].some((o) => o.value === current)) sel.value = current;
}

async function openProvidersModal() {
  if (!state.llmProviders) {
    try { state.llmProviders = await apiFetch("/llm-providers"); } catch (_) {}
  }
  renderProvidersList();
  showProvidersView("list");
  openModal("providers-modal");
}

function renderProvidersList() {
  const list = qs("providers-list");
  if (!list) return;
  list.innerHTML = "";
  (state.llmProviders || []).forEach((p) => {
    const li = document.createElement("li");
    li.className = "providers-list-item";
    li.dataset.id = p.id;
    const modelLabel = p.saved_model || p.default_model;
    const keyStatus = p.needs_key
      ? (p.has_key ? '<span class="provider-badge has-key">Key saved</span>' : '<span class="provider-badge no-key">No key</span>')
      : '<span class="provider-badge has-key">Local</span>';
    li.innerHTML = `
      <span class="providers-list-item-name">${p.name}</span>
      <div class="providers-list-item-right">
        <span class="muted">${modelLabel}</span>
        ${keyStatus}
        <span>›</span>
      </div>
    `;
    li.addEventListener("click", () => showProviderDetail(p.id));
    list.appendChild(li);
  });
}

function showProvidersView(view) {
  qs("providers-list-view").classList.toggle("hidden", view !== "list");
  qs("providers-detail-view").classList.toggle("hidden", view !== "detail");
  qs("providers-modal-title").textContent = view === "list" ? "LLM Providers" : "Configure provider";
}

function showProviderDetail(providerId) {
  const p = (state.llmProviders || []).find((x) => x.id === providerId);
  if (!p) return;
  state._editingProvider = p;

  qs("providers-detail-name").textContent = p.name;
  const badge = qs("providers-detail-badge");
  if (p.needs_key) {
    badge.textContent = p.has_key ? "Key saved" : "No key";
    badge.className = `provider-badge ${p.has_key ? "has-key" : "no-key"}`;
  } else {
    badge.textContent = "Local";
    badge.className = "provider-badge has-key";
  }
  qs("providers-detail-status").textContent = "";

  const fields = qs("providers-detail-fields");
  const currentModel = p.saved_model || p.default_model;
  const apiBasePlaceholder = p.default_api_base ? `Default: ${p.default_api_base}` : "Provider default";

  let html = "";

  const datalistId = `pdetail-models-${p.id}`;
  const datalistOptions = p.models.map((m) => `<option value="${m}">`).join("");
  const modelHint = p.id === "ollama"
    ? '<p class="field-hint">Must match a model pulled in Ollama. Type any model name.</p>'
    : '<p class="field-hint">Select a suggestion or type any model name.</p>';
  html += `
    <div class="field">
      <label for="pdetail-model">Default model</label>
      <input id="pdetail-model" type="text" list="${datalistId}" value="${currentModel}" spellcheck="false" autocomplete="off" />
      <datalist id="${datalistId}">${datalistOptions}</datalist>
      ${modelHint}
    </div>
  `;

  // API base (always shown)
  html += `
    <div class="field">
      <label for="pdetail-base">API base${p.default_api_base ? "" : " (optional)"}</label>
      <input id="pdetail-base" type="text" value="" placeholder="${apiBasePlaceholder}" spellcheck="false" />
    </div>
  `;

  // API key (only if provider needs one)
  if (p.needs_key) {
    html += `
      <div class="field">
        <label for="pdetail-key">API key${p.has_key ? " (saved — enter to replace)" : ""}</label>
        <input id="pdetail-key" type="password" autocomplete="off" placeholder="${p.has_key ? "••••••••" : "Paste API key"}" />
      </div>
    `;
  }

  fields.innerHTML = html;
  showProvidersView("detail");
}

async function saveCurrentProvider() {
  const p = state._editingProvider;
  if (!p) return;
  const statusEl = qs("providers-detail-status");

  const default_model = qs("pdetail-model")?.value.trim() || null;
  const api_base = qs("pdetail-base")?.value.trim() || null;
  const api_key = qs("pdetail-key")?.value.trim() || null;

  try {
    const result = await apiFetch(`/llm-providers/${p.id}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ api_key, api_base, default_model }),
    });
    // Update state
    state.llmProviders = (state.llmProviders || []).map((x) =>
      x.id === p.id ? { ...x, has_key: result.has_key, has_custom_base: result.has_custom_base, saved_model: result.saved_model } : x
    );
    state._editingProvider = { ...p, has_key: result.has_key, has_custom_base: result.has_custom_base, saved_model: result.saved_model };
    if (qs("pdetail-key")) qs("pdetail-key").value = "";
    // Update badge
    const badge = qs("providers-detail-badge");
    if (p.needs_key) {
      badge.textContent = result.has_key ? "Key saved" : "No key";
      badge.className = `provider-badge ${result.has_key ? "has-key" : "no-key"}`;
    }
    if (statusEl) {
      statusEl.textContent = "Saved";
      statusEl.className = "provider-card-status ok";
      setTimeout(() => { statusEl.textContent = ""; }, 2000);
    }
    // Refresh list view and selects
    renderProvidersList();
    populateProviderSelect("setting-summary-provider", state.llmProviders);
    populateProviderSelect("modal-summary-provider", state.llmProviders);
  } catch (err) {
    if (statusEl) {
      statusEl.textContent = err.message;
      statusEl.className = "provider-card-status err";
    }
  }
}

async function testCurrentProvider() {
  const p = state._editingProvider;
  if (!p) return;
  const statusEl = qs("providers-detail-status");
  if (statusEl) { statusEl.textContent = "Testing…"; statusEl.className = "provider-card-status"; }
  try {
    const model = qs("pdetail-model")?.value.trim() || null;
    const result = await apiFetch(`/llm-providers/${p.id}/test`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model }),
    });
    if (statusEl) {
      statusEl.textContent = result.ok ? `OK (${result.latency_ms}ms)` : (result.error || "Failed");
      statusEl.className = `provider-card-status ${result.ok ? "ok" : "err"}`;
    }
  } catch (err) {
    if (statusEl) {
      statusEl.textContent = err.message;
      statusEl.className = "provider-card-status err";
    }
  }
}

async function loadConfig() {
  const config = await refreshConfigFromApi();
  qs("setting-api-base").value = state.apiBase;
  qs("setting-output-root").value = config.output_root || "";
  const providerSel = qs("setting-transcription-provider");
  const storedProvider = config.transcription_provider || "auto";
  // Old configs may hold the retired onnx-asr/mlx-audio values; fall back to auto.
  providerSel.value = [...providerSel.options].some((o) => o.value === storedProvider)
    ? storedProvider
    : "auto";
  const accelSel = qs("setting-transcription-accelerator");
  const storedAccel = config.transcription_accelerator || "cpu";
  accelSel.value = [...accelSel.options].some((o) => o.value === storedAccel)
    ? storedAccel
    : "cpu";
  qs("setting-default-language").value = config.default_language || "";
  qs("setting-whisperx-model").value = config.whisperx_model || "";

  // Populate default provider select from registry
  if (!state.llmProviders) {
    try { state.llmProviders = await apiFetch("/llm-providers"); } catch (_) {}
  }
  populateProviderSelect("setting-summary-provider", state.llmProviders || []);
  const sp = (config.summary_provider || "ollama").toLowerCase();
  const sel = qs("setting-summary-provider");
  if (sel && [...sel.options].some((o) => o.value === sp)) sel.value = sp;

  // Multi-device sync
  qs("setting-sync-url").value = config.sync_url || "";
  // Token is write-only on the server; show a placeholder when one is saved.
  const tokenInput = qs("setting-sync-token");
  tokenInput.value = "";
  tokenInput.placeholder = config.has_sync_token ? "•••••••• (saved)" : "(none)";
  renderSyncStatus(config);
}

function renderSyncStatus(config) {
  const el = qs("sync-status");
  if (!el) return;
  if (!config.sync_url) {
    el.textContent = "Sync off — everything stays on this device.";
    el.className = "status";
  } else if (!config.has_sync_token) {
    el.textContent = "Sync URL set but no token saved — add a token to enable sync.";
    el.className = "status err";
  } else {
    el.textContent = `Sync on — ${config.sync_url}`;
    el.className = "status done";
  }
}

async function saveConfig() {
  const apiBaseValue = qs("setting-api-base").value.trim();
  if (apiBaseValue) {
    state.apiBase = apiBaseValue;
    localStorage.setItem("ck_api_base", state.apiBase);
  }
  const payload = {
    output_root: qs("setting-output-root").value.trim(),
    transcription_provider: qs("setting-transcription-provider").value.trim() || "auto",
    transcription_accelerator: qs("setting-transcription-accelerator").value.trim() || "cpu",
    summary_provider: qs("setting-summary-provider")?.value.trim() || "ollama",
    default_language: qs("setting-default-language").value.trim(),
    whisperx_model: qs("setting-whisperx-model").value.trim(),
    sync_url: qs("setting-sync-url").value.trim(),
  };
  // Only send the token when the user actually typed one — an empty field means
  // "leave the saved token unchanged", not "clear it".
  const syncToken = qs("setting-sync-token").value;
  if (syncToken) payload.sync_token = syncToken;
  const updated = await apiFetch("/config", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  state.configFromApi = updated;
  // Reflect the saved sync state (and reset the write-only token field).
  if (updated) {
    qs("setting-sync-token").value = "";
    qs("setting-sync-token").placeholder = updated.has_sync_token ? "•••••••• (saved)" : "(none)";
    renderSyncStatus(updated);
  }
  setStatus(qs("settings-result"), "Settings saved");
}

function renderMetadataSuggestions(metadata) {
  const container = qs("metadata-suggestions");
  const content = qs("metadata-suggestions-content");
  if (!metadata || !content) {
    container.classList.add("hidden");
    return;
  }
  const categories = ["characters", "locations", "events", "items", "tags"];
  const hasAny = categories.some((cat) => (metadata[cat] || []).length > 0);
  if (!hasAny) {
    container.classList.add("hidden");
    return;
  }
  content.innerHTML = "";
  categories.forEach((cat) => {
    const values = metadata[cat] || [];
    if (!values.length) return;
    const row = document.createElement("div");
    row.className = "metadata-row";
    row.dataset.category = cat;
    row.innerHTML = `<span class="metadata-label">${cat}</span>`;
    const chips = document.createElement("div");
    chips.className = "metadata-chips";
    values.forEach((v) => {
      const chip = document.createElement("span");
      chip.className = "metadata-chip";
      chip.textContent = v;
      chips.appendChild(chip);
    });
    row.appendChild(chips);
    content.appendChild(row);
  });
  container.classList.remove("hidden");
}

document.addEventListener("DOMContentLoaded", () => {
  loadApiBase();
  showScreen("start");
  loadCampaigns().catch(() => {});
  loadConfig().catch(() => {});

  qs("open-campaign-wizard").addEventListener("click", () => openCampaignWizard("create"));
  qs("open-session-wizard").addEventListener("click", () => openSessionWizard());

  qs("campaign-wizard-cancel").addEventListener("click", () => closeWizard("campaign-wizard"));
  qs("campaign-wizard-back").addEventListener("click", () => {
    state.campaignWizard.step = Math.max(1, state.campaignWizard.step - 1);
    updateCampaignWizardUI();
  });
  qs("campaign-wizard-next").addEventListener("click", async () => {
    const total = 3;
    if (state.campaignWizard.step < total) {
      state.campaignWizard.step += 1;
      updateCampaignWizardUI();
      return;
    }
    try {
      await saveCampaignWizard();
    } catch (error) {
      setStatus(qs("campaign-wizard-status"), error.message, true);
    }
  });
  qs("wizard-add-player").addEventListener("click", () => addPlayerRow("wizard-player-rows"));
  qs("wizard-player-rows").addEventListener("click", (event) => {
    const button = event.target.closest('button[data-action="remove-player"]');
    if (!button) return;
    const index = Number(button.dataset.index);
    const players = collectPlayerRows("wizard-player-rows");
    players.splice(index, 1);
    renderPlayerRows("wizard-player-rows", players);
  });

  qs("session-wizard-cancel").addEventListener("click", () => closeWizard("session-wizard"));
  qs("session-wizard-back").addEventListener("click", () => {
    state.sessionWizard.step = Math.max(1, state.sessionWizard.step - 1);
    updateSessionWizardUI();
  });
  qs("session-wizard-next").addEventListener("click", async () => {
    const total = 3;
    if (state.sessionWizard.step === 1 && !state.sessionTracks.length) {
      setStatus(qs("session-upload-status"), "Upload a ZIP to continue", true);
      return;
    }
    if (state.sessionWizard.step === 2) {
      try {
        await saveSpeakers();
      } catch (error) {
        setStatus(qs("session-speakers-status"), error.message, true);
        return;
      }
    }
    if (state.sessionWizard.step === total) {
      try {
        await saveSessionMetadata();
        closeWizard("session-wizard");
        await refreshCampaignSessions();
        await loadSession(state.sessionWizard.sessionId);
      } catch (error) {
        setStatus(qs("session-meta-status"), error.message, true);
      }
      return;
    }
    state.sessionWizard.step += 1;
    updateSessionWizardUI();
  });

  qs("session-speakers-body").addEventListener("input", (event) => {
    const input = event.target;
    if (input.dataset.field !== "player_name") return;
    const players = state.currentCampaign?.players || [];
    const match = players.find(
      (p) => p.player_name && p.player_name.toLowerCase() === input.value.trim().toLowerCase()
    );
    if (!match) return;
    const row = input.closest("tr");
    const charInput = row?.querySelector('input[data-field="character_name"]');
    if (charInput && !charInput.value.trim()) {
      charInput.value = match.character_name || "";
    }
  });

  qs("session-upload-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!state.sessionWizard.sessionId) return;
    const fileInput = qs("session-zip-file");
    if (!fileInput.files.length) return;
    setStatus(qs("session-upload-status"), "Uploading...");
    const formData = new FormData();
    formData.append("file", fileInput.files[0]);
    formData.append("session_id", state.sessionWizard.sessionId);
    try {
      const data = await apiFetch("/upload", { method: "POST", body: formData });
      state.sessionTracks = data.tracks || [];
      state.sessionSpeakers = [];
      renderSessionTracks();
      renderSpeakerTable();
      setStatus(qs("session-upload-status"), "Upload complete");
    } catch (error) {
      setStatus(qs("session-upload-status"), error.message, true);
    }
  });

  qs("open-transcribe-modal").addEventListener("click", async () => {
    qs("modal-transcribe-status").textContent = "";
    try {
      await refreshConfigFromApi();
      if (!state.providers) {
        state.providers = await apiFetch("/providers");
      }
      const providerSelect = qs("modal-transcribe-provider");
      const modelSelect = qs("modal-transcribe-model");
      providerSelect.innerHTML = "";
      state.providers.forEach((p) => {
        const opt = document.createElement("option");
        opt.value = p.name;
        opt.textContent = p.display_name;
        providerSelect.appendChild(opt);
      });
      const cfg = state.configFromApi || {};
      const explicit = cfg.transcription_provider;
      const wantProvider =
        explicit && explicit !== "auto"
          ? explicit
          : cfg.transcription_provider_effective || explicit || "sherpa";
      if ([...providerSelect.options].some((o) => o.value === wantProvider)) {
        providerSelect.value = wantProvider;
      }
      const populateModels = () => {
        const provider = state.providers.find((p) => p.name === providerSelect.value);
        modelSelect.innerHTML = "";
        if (!provider) return;
        provider.models.forEach((m) => {
          const opt = document.createElement("option");
          opt.value = m.id;
          opt.textContent = m.name;
          modelSelect.appendChild(opt);
        });
        const preferred = cfg.whisperx_model;
        const preferredOk =
          preferred && provider.models.some((m) => m.id === preferred);
        modelSelect.value = preferredOk ? preferred : provider.default_model;
      };
      populateModels();
      providerSelect.onchange = populateModels;
      qs("modal-transcribe-language").value = (cfg.default_language || "").trim();
    } catch (error) {
      setStatus(qs("modal-transcribe-status"), error.message, true);
    }
    openModal("transcribe-modal");
  });
  qs("open-summarize-modal").addEventListener("click", async () => {
    qs("modal-summary-status").textContent = "";
    qs("modal-summary-preview").textContent = "";
    qs("metadata-suggestions").classList.add("hidden");
    try { await refreshConfigFromApi(); } catch (_) {}

    if (!state.llmProviders) {
      try { state.llmProviders = await apiFetch("/llm-providers"); } catch (_) {}
    }

    const cfg = state.configFromApi || {};
    populateProviderSelect("modal-summary-provider", state.llmProviders || []);
    const savedProvider = (cfg.summary_provider || "ollama").toLowerCase();
    const sel = qs("modal-summary-provider");
    if (sel && [...sel.options].some((o) => o.value === savedProvider)) sel.value = savedProvider;

    populateSummarizeModalModels();

    const sessionTitle = state.currentSession?.campaign?.title;
    qs("modal-summary-title").value = (sessionTitle || "").trim();
    qs("modal-summary-context").value = "";
    populateTranscriptSelect();
    await loadPromptPresets();
    populatePromptPresetSelect();
    openModal("summarize-modal");
  });

  qs("close-transcribe-modal").addEventListener("click", () =>
    closeModal("transcribe-modal")
  );
  qs("close-summarize-modal").addEventListener("click", () =>
    closeModal("summarize-modal")
  );
  qs("close-export-modal").addEventListener("click", () =>
    closeModal("export-modal")
  );

  qs("run-transcribe").addEventListener("click", async () => {
    if (!state.currentSession?.session_id) return;
    const payload = {
      session_id: state.currentSession.session_id,
      provider: qs("modal-transcribe-provider").value || null,
      model: qs("modal-transcribe-model").value || null,
      language: qs("modal-transcribe-language").value.trim() || null,
    };
    closeModal("transcribe-modal");
    setSessionOpStatus("Transcribing…");
    // Poll model-download progress while the (blocking) transcribe runs. The
    // first transcribe on a fresh install downloads the ~465 MB model once.
    const stopPolling = pollModelStatus();
    try {
      await apiFetch("/transcribe", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      await loadSession(state.currentSession.session_id);
      await refreshCampaignSessions();
      setSessionOpStatus("Transcription complete", "done");
    } catch (error) {
      setSessionOpStatus(error.message, "err");
    } finally {
      stopPolling();
    }
  });

  qs("run-summarize").addEventListener("click", async () => {
    if (!state.currentSession?.session_id) return;
    const providerVal = qs("modal-summary-provider").value;
    const modelVal = qs("modal-summary-model")?.value.trim() || null;
    const payload = {
      session_id: state.currentSession.session_id,
      transcript_id: state.selectedTranscriptId || null,
      provider: providerVal,
      model: modelVal,
      base_url: null,
      title: qs("modal-summary-title").value.trim() || null,
      context: qs("modal-summary-context").value.trim() || null,
      system_prompt: qs("modal-system-prompt").value.trim() || null,
    };
    closeModal("summarize-modal");
    setSessionOpStatus(`Summarizing with ${providerVal}…`);
    try {
      const data = await apiFetch("/summarize", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      await loadSession(state.currentSession.session_id);
      await refreshCampaignSessions();
      setSessionOpStatus("Summary complete", "done");
    } catch (error) {
      setSessionOpStatus(error.message, "err");
    }
  });

  qs("modal-transcript-select").addEventListener("change", () => {
    state.selectedTranscriptId = Number(qs("modal-transcript-select").value) || null;
  });

  qs("accept-metadata").addEventListener("click", () => {
    // Metadata has already been merged server-side during summarization;
    // just close the suggestions panel and confirm to the user.
    qs("metadata-suggestions").classList.add("hidden");
    setStatus(qs("modal-summary-status"), "Metadata saved to session");
  });

  qs("save-settings").addEventListener("click", async () => {
    try {
      await saveConfig();
    } catch (error) {
      setStatus(qs("settings-result"), error.message, true);
    }
  });

  qs("modal-summary-provider")?.addEventListener("change", toggleSummarizeModalProviderFields);
  qs("open-providers-modal")?.addEventListener("click", () => openProvidersModal().catch(console.error));
  qs("close-providers-modal")?.addEventListener("click", () => closeModal("providers-modal"));
  qs("providers-back")?.addEventListener("click", () => showProvidersView("list"));
  qs("providers-detail-save")?.addEventListener("click", () => saveCurrentProvider().catch(console.error));
  qs("providers-detail-test")?.addEventListener("click", () => testCurrentProvider().catch(console.error));

  qsa(".nav-btn").forEach((button) => {
    button.addEventListener("click", () => {
      const screen = button.dataset.screen;
      if (screen) showScreen(screen);
      if (screen === "settings") {
        loadConfig().catch(console.error);
      }
    });
  });

  qs("toggle-campaign-edit").addEventListener("click", () => {
    toggleCampaignEdit(true);
  });
  qs("close-campaign-edit").addEventListener("click", () =>
    closeModal("campaign-edit-modal")
  );
  qs("save-campaign-edit").addEventListener("click", async () => {
    try {
      await saveCampaignEdit();
    } catch (error) {
      setStatus(qs("campaign-edit-status"), error.message, true);
    }
  });
  qs("add-campaign-player").addEventListener("click", () =>
    addPlayerRow("campaign-player-rows")
  );
  qs("campaign-player-rows").addEventListener("click", (event) => {
    const button = event.target.closest('button[data-action="remove-player"]');
    if (!button) return;
    const index = Number(button.dataset.index);
    const players = collectPlayerRows("campaign-player-rows");
    players.splice(index, 1);
    renderPlayerRows("campaign-player-rows", players);
  });

  qs("close-transcript-viewer").addEventListener("click", () =>
    closeModal("transcript-viewer-modal")
  );

  qs("session-overview-transcripts").addEventListener("click", async (event) => {
    const button = event.target.closest("button[data-action]");
    if (!button) return;
    const action = button.dataset.action;
    const artifactId = button.dataset.id;
    const sessionId = state.currentSession?.session_id;
    if (!sessionId || !artifactId) return;

    if (action === "open-transcript") {
      try {
        const response = await fetch(
          apiUrl(`/sessions/${sessionId}/transcripts/${artifactId}/content`),
          { headers: authHeaders() }
        );
        if (!response.ok) throw new Error("Failed to load transcript");
        const text = await response.text();
        const item = state.transcripts.find((t) => String(t.id) === artifactId);
        qs("transcript-viewer-title").textContent = item
          ? `${item.provider} / ${item.model}`
          : "Transcript";
        qs("transcript-viewer-content").textContent = text;
        openModal("transcript-viewer-modal");
      } catch (error) {
        alert(error.message);
      }
    }

    if (action === "delete-transcript") {
      if (!confirm("Delete this transcript?")) return;
      try {
        await apiFetch(`/sessions/${sessionId}/transcripts/${artifactId}`, {
          method: "DELETE",
        });
        await loadSession(sessionId);
      } catch (error) {
        alert(error.message);
      }
    }
  });

  qs("session-overview-summaries").addEventListener("click", async (event) => {
    const button = event.target.closest("button[data-action]");
    if (!button) return;
    const action = button.dataset.action;
    const artifactId = button.dataset.id;
    const sessionId = state.currentSession?.session_id;
    if (!sessionId || !artifactId) return;

    if (action === "open-summary") {
      try {
        const response = await fetch(
          apiUrl(`/sessions/${sessionId}/summaries/${artifactId}/content`),
          { headers: authHeaders() }
        );
        if (!response.ok) throw new Error("Failed to load summary");
        const text = await response.text();
        const item = state.summaries.find((s) => String(s.id) === artifactId);
        qs("transcript-viewer-title").textContent = item
          ? `${item.provider} / ${item.model}`
          : "Summary";
        qs("transcript-viewer-content").textContent = text;
        openModal("transcript-viewer-modal");
      } catch (error) {
        alert(error.message);
      }
    }

    if (action === "delete-summary") {
      if (!confirm("Delete this summary?")) return;
      try {
        await apiFetch(`/sessions/${sessionId}/summaries/${artifactId}`, {
          method: "DELETE",
        });
        await loadSession(sessionId);
      } catch (error) {
        alert(error.message);
      }
    }
  });

  qs("toggle-session-edit").addEventListener("click", () => {
    toggleSessionEdit(true);
  });
  qs("close-session-edit").addEventListener("click", () =>
    closeModal("session-edit-modal")
  );
  qs("save-session-edit").addEventListener("click", async () => {
    try {
      await saveSessionEdit();
    } catch (error) {
      setStatus(qs("session-edit-status"), error.message, true);
    }
  });

  qs("export-obsidian").addEventListener("click", async () => {
    if (!state.currentSession?.session_id) return;
    await loadSummaries();
    populateExportSummarySelect();
    openModal("export-modal");
  });

  qs("run-export").addEventListener("click", async () => {
    if (!state.currentSession?.session_id) return;
    const summarySelect = qs("modal-export-summary-select");
    const summaryId = Number(summarySelect?.value);
    if (!Number.isFinite(summaryId) || summaryId <= 0) {
      alert("Select a summary to export.");
      return;
    }
    try {
      closeModal("export-modal");
      const data = await apiFetch("/export", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          session_id: state.currentSession.session_id,
          summary_id: summaryId,
          use_obsidian_format: true,
        }),
      });
      // Trigger download
      const blob = new Blob([data.content], { type: "text/markdown" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = data.filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (error) {
      alert(error.message);
    }
  });
});
