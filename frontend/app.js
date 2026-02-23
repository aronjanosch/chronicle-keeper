const state = {
  apiBase: "http://127.0.0.1:8000",
  campaigns: [],
  campaignDetails: [],
  currentCampaign: null,
  campaignSessions: [],
  currentSession: null,
  sessionTracks: [],
  sessionSpeakers: [],
  transcripts: [],
  summaries: [],
  selectedTranscriptPath: null,
  providers: null,
  promptPresets: null,
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

function apiUrl(path) {
  return `${state.apiBase}${path}`;
}

async function apiFetch(path, options = {}) {
  const response = await fetch(apiUrl(path), options);
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
  const saved = localStorage.getItem("ck_api_base");
  if (saved) state.apiBase = saved;
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
    state.selectedTranscriptPath = null;
    return;
  }
  select.disabled = false;
  state.transcripts.forEach((item, index) => {
    const option = document.createElement("option");
    option.value = item.file_path;
    option.textContent = `${item.provider} / ${item.model} (${new Date(item.created_at).toLocaleString()})`;
    if (index === 0) option.selected = true;
    select.appendChild(option);
  });
  state.selectedTranscriptPath = select.value;
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

async function loadConfig() {
  const config = await apiFetch("/config");
  qs("setting-api-base").value = state.apiBase;
  qs("setting-output-root").value = config.output_root || "";
  qs("setting-summary-provider").value = config.summary_provider || "";
  qs("setting-ollama-base-url").value = config.ollama_base_url || "";
  qs("setting-ollama-model").value = config.ollama_model || "";
  qs("setting-ollama-timeout").value = config.ollama_timeout_seconds || "";
  qs("setting-gemini-model").value = config.gemini_model || "";
  qs("setting-default-language").value = config.default_language || "";
  qs("setting-whisperx-model").value = config.whisperx_model || "";
}

async function saveConfig() {
  const apiBaseValue = qs("setting-api-base").value.trim();
  if (apiBaseValue) {
    state.apiBase = apiBaseValue;
    localStorage.setItem("ck_api_base", state.apiBase);
  }
  const payload = {
    output_root: qs("setting-output-root").value.trim(),
    summary_provider: qs("setting-summary-provider").value.trim(),
    ollama_base_url: qs("setting-ollama-base-url").value.trim(),
    ollama_model: qs("setting-ollama-model").value.trim(),
    ollama_timeout_seconds: Number(qs("setting-ollama-timeout").value) || undefined,
    gemini_model: qs("setting-gemini-model").value.trim(),
    default_language: qs("setting-default-language").value.trim(),
    whisperx_model: qs("setting-whisperx-model").value.trim(),
  };

  const geminiKey = qs("setting-gemini-api-key").value.trim();
  const hfToken = qs("setting-hf-token").value.trim();
  if (geminiKey) payload.gemini_api_key = geminiKey;
  if (hfToken) payload.hf_token = hfToken;

  await apiFetch("/config", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  qs("setting-gemini-api-key").value = "";
  qs("setting-hf-token").value = "";
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
      const populateModels = () => {
        const provider = state.providers.find((p) => p.name === providerSelect.value);
        modelSelect.innerHTML = "";
        if (!provider) return;
        provider.models.forEach((m) => {
          const opt = document.createElement("option");
          opt.value = m.id;
          opt.textContent = m.name;
          if (m.id === provider.default_model) opt.selected = true;
          modelSelect.appendChild(opt);
        });
      };
      populateModels();
      providerSelect.onchange = populateModels;
    } catch (error) {
      setStatus(qs("modal-transcribe-status"), error.message, true);
    }
    openModal("transcribe-modal");
  });
  qs("open-summarize-modal").addEventListener("click", async () => {
    qs("modal-summary-status").textContent = "";
    qs("modal-summary-preview").textContent = "";
    qs("metadata-suggestions").classList.add("hidden");
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

  qs("run-transcribe").addEventListener("click", async () => {
    if (!state.currentSession?.session_id) return;
    setStatus(qs("modal-transcribe-status"), "Transcribing...");
    const payload = {
      session_id: state.currentSession.session_id,
      provider: qs("modal-transcribe-provider").value || null,
      model: qs("modal-transcribe-model").value || null,
      language: qs("modal-transcribe-language").value.trim() || null,
      hf_token: qs("modal-transcribe-hf").value.trim() || null,
    };
    try {
      await apiFetch("/transcribe", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      setStatus(qs("modal-transcribe-status"), "Transcription complete");
      await loadSession(state.currentSession.session_id);
      await refreshCampaignSessions();
    } catch (error) {
      setStatus(qs("modal-transcribe-status"), error.message, true);
    }
  });

  qs("run-summarize").addEventListener("click", async () => {
    if (!state.currentSession?.session_id) return;
    setStatus(qs("modal-summary-status"), "Summarizing...");
    qs("metadata-suggestions").classList.add("hidden");
    const payload = {
      session_id: state.currentSession.session_id,
      transcript_path: state.selectedTranscriptPath || null,
      provider: qs("modal-summary-provider").value,
      model: qs("modal-summary-model").value.trim() || null,
      base_url: qs("modal-summary-base-url").value.trim() || null,
      title: qs("modal-summary-title").value.trim() || null,
      context: qs("modal-summary-context").value.trim() || null,
      system_prompt: qs("modal-system-prompt").value.trim() || null,
    };
    try {
      const data = await apiFetch("/summarize", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      qs("modal-summary-preview").textContent = data.summary || "";
      setStatus(qs("modal-summary-status"), "Summary ready");
      renderMetadataSuggestions(data.metadata);
      await loadSession(state.currentSession.session_id);
      await refreshCampaignSessions();
    } catch (error) {
      setStatus(qs("modal-summary-status"), error.message, true);
    }
  });

  qs("modal-transcript-select").addEventListener("change", () => {
    state.selectedTranscriptPath = qs("modal-transcript-select").value || null;
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

  qsa(".nav-btn").forEach((button) => {
    button.addEventListener("click", () => {
      const screen = button.dataset.screen;
      if (screen) showScreen(screen);
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
          apiUrl(`/sessions/${sessionId}/transcripts/${artifactId}/content`)
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
          apiUrl(`/sessions/${sessionId}/summaries/${artifactId}/content`)
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
    try {
      const data = await apiFetch("/export", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          session_id: state.currentSession.session_id,
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
