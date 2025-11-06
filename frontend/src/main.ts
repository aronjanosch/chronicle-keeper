// Using standard web APIs instead of Tauri

const API_BASE_URL = 'http://127.0.0.1:8000';

interface Track {
  id: string;
  filename: string;
  duration: number;
}

interface UploadResponse {
  tracks: Track[];
  session_id: string;
}

interface Settings {
  gemini_api_key?: string;
  system_prompt?: string;
  llm_preference?: 'local' | 'cloud';
  language?: string;
  transcription_language?: string;
  whisper_model?: string;
  ollama_model?: string;
  ollama_base_url?: string;
  available_languages?: Record<string, string>;
  available_transcription_languages?: Record<string, string>;
  available_whisper_models?: Record<string, string>;
  available_ollama_models?: Record<string, string>;
}

interface SessionMetadata {
  session_id: string;
  session_date?: string;
  session_number?: number;
  campaign_id?: string;
  campaign_name?: string;
  locations?: string[];
  characters_present?: string[];
  tags?: string[];
  notes?: string;
}

interface MetadataSuggestions {
  suggested_tags: string[];
  mentioned_characters: string[];
  mentioned_locations: string[];
  session_tone: string[];
  key_events: string[];
}

interface SpeakerInfo {
  playerName: string;
  characterName?: string;
  pronouns?: string;
}

class ChronicleKeeperApp {
  private currentSessionId: string | null = null;
  private tracks: Track[] = [];
  private speakerMappings: Record<string, SpeakerInfo> = {};
  private generatedNotes: string = '';

  constructor() {
    this.initializeEventListeners();
    this.loadSettings();
  }

  private initializeEventListeners() {
    // Settings modal
    document.getElementById('settings-btn')?.addEventListener('click', () => this.openSettings());
    document.getElementById('close-settings')?.addEventListener('click', () => this.closeSettings());
    document.getElementById('save-settings')?.addEventListener('click', () => this.saveSettings());
    document.getElementById('cancel-settings')?.addEventListener('click', () => this.closeSettings());
    document.getElementById('refresh-wizard-models-btn')?.addEventListener('click', () => this.refreshWizardModels());
    document.getElementById('test-ollama-connection-btn')?.addEventListener('click', () => this.testOllamaConnection());

    // File upload
    document.getElementById('file-select-btn')?.addEventListener('click', () => this.selectFile());
    document.getElementById('debug-upload-btn')?.addEventListener('click', () => this.debugUpload());
    
    // Drag and drop
    const dropZone = document.getElementById('file-drop-zone');
    if (dropZone) {
      dropZone.addEventListener('dragover', (e) => {
        e.preventDefault();
        dropZone.classList.add('drag-over');
      });
      
      dropZone.addEventListener('dragleave', () => {
        dropZone.classList.remove('drag-over');
      });
      
      dropZone.addEventListener('drop', (e) => {
        e.preventDefault();
        dropZone.classList.remove('drag-over');
        const files = e.dataTransfer?.files;
        if (files && files.length > 0) {
          this.handleFileUpload(files[0]);
        }
      });
    }

    // Navigation buttons
    document.getElementById('back-to-upload')?.addEventListener('click', () => this.showScreen('upload-screen'));
    document.getElementById('continue-to-metadata')?.addEventListener('click', () => this.proceedToMetadata());
    document.getElementById('back-to-speakers')?.addEventListener('click', () => this.showScreen('speakers-screen'));
    document.getElementById('continue-to-processing')?.addEventListener('click', () => this.proceedToTranscription());
    document.getElementById('back-to-metadata')?.addEventListener('click', () => this.showScreen('metadata-screen'));
    document.getElementById('continue-to-summarization')?.addEventListener('click', () => this.proceedToSummarization());
    document.getElementById('back-to-transcription')?.addEventListener('click', () => this.showScreen('transcription-screen'));
    document.getElementById('back-to-summarization')?.addEventListener('click', () => this.showScreen('summarization-screen'));
    document.getElementById('back-to-suggestions')?.addEventListener('click', () => this.showScreen('suggestions-screen'));
    document.getElementById('continue-to-export')?.addEventListener('click', () => this.proceedToExport());

    // Processing
    document.getElementById('generate-notes-btn')?.addEventListener('click', () => this.generateNotes());

    // Export
    document.getElementById('export-notes-btn')?.addEventListener('click', () => this.exportNotes());
    document.getElementById('start-new-session')?.addEventListener('click', () => this.startNewSession());
  }

  private async loadSettings() {
    try {
      const response = await window.fetch(`${API_BASE_URL}/settings`);
      if (response.ok) {
        const settings: Settings = await response.json();

        const apiKeyInput = document.getElementById('gemini-api-key') as HTMLInputElement;
        const promptInput = document.getElementById('system-prompt') as HTMLTextAreaElement;
        const languageSelect = document.getElementById('language-select') as HTMLSelectElement;
        const ollamaBaseUrlInput = document.getElementById('ollama-base-url') as HTMLInputElement;

        // Load available languages
        if (languageSelect && settings.available_languages) {
          languageSelect.innerHTML = '';
          Object.entries(settings.available_languages).forEach(([code, name]) => {
            const option = document.createElement('option');
            option.value = code;
            option.textContent = name;
            languageSelect.appendChild(option);
          });
        }

        if (languageSelect && settings.language) {
          languageSelect.value = settings.language;
        }

        if (apiKeyInput && settings.gemini_api_key) {
          apiKeyInput.value = settings.gemini_api_key;
        }

        if (promptInput && settings.system_prompt) {
          promptInput.value = settings.system_prompt;
        }

        if (ollamaBaseUrlInput && settings.ollama_base_url) {
          ollamaBaseUrlInput.value = settings.ollama_base_url;
        }

        // Set up language change handler
        if (languageSelect) {
          languageSelect.addEventListener('change', () => this.handleLanguageChange());
        }
      }
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  }

  private openSettings() {
    document.getElementById('settings-modal')?.classList.remove('hidden');
  }

  private closeSettings() {
    document.getElementById('settings-modal')?.classList.add('hidden');
  }

  private async saveSettings() {
    const apiKeyInput = document.getElementById('gemini-api-key') as HTMLInputElement;
    const promptInput = document.getElementById('system-prompt') as HTMLTextAreaElement;
    const languageSelect = document.getElementById('language-select') as HTMLSelectElement;
    const ollamaBaseUrlInput = document.getElementById('ollama-base-url') as HTMLInputElement;

    const settings: Settings = {
      gemini_api_key: apiKeyInput.value,
      system_prompt: promptInput.value,
      language: languageSelect.value,
      ollama_base_url: ollamaBaseUrlInput.value
    };

    try {
      const response = await window.fetch(`${API_BASE_URL}/settings`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(settings),
      });

      if (response.ok) {
        this.showStatus('notification', 'Settings saved successfully!', 'success');
        this.closeSettings();
      } else {
        this.showStatus('notification', 'Failed to save settings', 'error');
      }
    } catch (error) {
      console.error('Failed to save settings:', error);
      this.showStatus('notification', 'Failed to save settings', 'error');
    }
  }

  private async handleLanguageChange() {
    try {
      const languageSelect = document.getElementById('language-select') as HTMLSelectElement;
      const promptInput = document.getElementById('system-prompt') as HTMLTextAreaElement;
      
      if (!languageSelect || !promptInput) return;
      
      const selectedLanguage = languageSelect.value;
      
      // Fetch the default prompt for the selected language
      const response = await window.fetch(`${API_BASE_URL}/prompts`);
      if (response.ok) {
        const data = await response.json();
        const localizedPrompt = data.prompts[selectedLanguage];
        
        if (localizedPrompt) {
          // Ask user if they want to update the prompt
          const shouldUpdate = confirm(`Switch to ${data.languages[selectedLanguage]} default prompt?`);
          if (shouldUpdate) {
            promptInput.value = localizedPrompt;
          }
        }
      }
    } catch (error) {
      console.error('Failed to handle language change:', error);
    }
  }

  private async selectFile() {
    try {
      // Create a hidden file input element
      const input = document.createElement('input');
      input.type = 'file';
      input.accept = '.zip';
      input.style.display = 'none';
      
      input.onchange = (event) => {
        const file = (event.target as HTMLInputElement).files?.[0];
        if (file) {
          this.handleFileUpload(file);
        }
        document.body.removeChild(input);
      };
      
      document.body.appendChild(input);
      input.click();
    } catch (error) {
      console.error('File selection error:', error);
    }
  }

  private async handleFileUpload(file: File) {
    this.showUploadStatus('Uploading and processing file...', 'loading');

    try {
      const formData = new FormData();
      formData.append('file', file);

      const response = await window.fetch(`${API_BASE_URL}/upload`, {
        method: 'POST',
        body: formData,
      });

      if (response.ok) {
        const data: UploadResponse = await response.json();
        this.currentSessionId = data.session_id;
        this.tracks = data.tracks;
        this.showUploadStatus(`File uploaded successfully! Found ${data.tracks.length} audio tracks.`, 'success');
        this.renderTracks();
        setTimeout(() => this.showScreen('speakers-screen'), 1500);
      } else {
        const error = await response.json();
        this.showUploadStatus(`Upload failed: ${error.detail}`, 'error');
      }
    } catch (error) {
      console.error('Upload error:', error);
      this.showUploadStatus('Upload failed: Network error', 'error');
    }
  }

  private async debugUpload() {
    this.showUploadStatus('Loading example recording...', 'loading');

    try {
      const response = await window.fetch(`${API_BASE_URL}/debug-upload`, {
        method: 'POST',
      });

      if (response.ok) {
        const data: UploadResponse = await response.json();
        this.currentSessionId = data.session_id;
        this.tracks = data.tracks;
        this.showUploadStatus(`Example recording loaded! Found ${data.tracks.length} audio tracks.`, 'success');
        this.renderTracks();
        setTimeout(() => this.showScreen('speakers-screen'), 1500);
      } else {
        const error = await response.json();
        this.showUploadStatus(`Debug upload failed: ${error.detail}`, 'error');
      }
    } catch (error) {
      console.error('Debug upload error:', error);
      this.showUploadStatus('Debug upload failed: Network error', 'error');
    }
  }

  private renderTracks() {
    const container = document.getElementById('tracks-container');
    if (!container) return;

    container.innerHTML = '';

    this.tracks.forEach(track => {
      const trackElement = document.createElement('div');
      trackElement.className = 'track-item';
      trackElement.innerHTML = `
        <div class="track-info">
          <div class="track-name">${track.filename}</div>
          <div class="track-duration">${this.formatDuration(track.duration)}</div>
        </div>
        <div class="speaker-fields">
          <div class="speaker-field">
            <label class="speaker-label">Player Name *</label>
            <input
              type="text"
              class="speaker-input player-name-input"
              placeholder="e.g., Alex"
              data-track-id="${track.id}"
              data-field="playerName"
              required
            >
          </div>
          <div class="speaker-field">
            <label class="speaker-label">Character Name</label>
            <input
              type="text"
              class="speaker-input character-name-input"
              placeholder="e.g., Gandalf"
              data-track-id="${track.id}"
              data-field="characterName"
            >
          </div>
          <div class="speaker-field">
            <label class="speaker-label">Pronouns</label>
            <select
              class="speaker-input pronouns-select"
              data-track-id="${track.id}"
              data-field="pronouns"
            >
              <option value="">Select pronouns...</option>
              <option value="he/him">he/him</option>
              <option value="she/her">she/her</option>
              <option value="they/them">they/them</option>
              <option value="custom">Other/Custom</option>
            </select>
            <input
              type="text"
              class="speaker-input pronouns-custom-input hidden"
              placeholder="Enter custom pronouns"
              data-track-id="${track.id}"
              data-field="pronounsCustom"
            >
          </div>
        </div>
      `;
      container.appendChild(trackElement);
    });

    // Add event listeners to all speaker inputs
    const allInputs = container.querySelectorAll('.speaker-input');
    allInputs.forEach(input => {
      input.addEventListener('input', () => this.validateSpeakerMappings());
      input.addEventListener('change', () => this.validateSpeakerMappings());
    });

    // Add event listeners for custom pronouns handling
    const pronounsSelects = container.querySelectorAll('.pronouns-select');
    pronounsSelects.forEach(select => {
      select.addEventListener('change', (e) => {
        const target = e.target as HTMLSelectElement;
        const trackId = target.getAttribute('data-track-id');
        const customInput = container.querySelector(
          `.pronouns-custom-input[data-track-id="${trackId}"]`
        ) as HTMLInputElement;

        if (target.value === 'custom') {
          customInput.classList.remove('hidden');
          customInput.focus();
        } else {
          customInput.classList.add('hidden');
          customInput.value = '';
        }
        this.validateSpeakerMappings();
      });
    });
  }

  private formatDuration(seconds: number): string {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const remainingSeconds = Math.floor(seconds % 60);

    if (hours > 0) {
      return `${hours}:${minutes.toString().padStart(2, '0')}:${remainingSeconds.toString().padStart(2, '0')}`;
    } else {
      return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
    }
  }

  private validateSpeakerMappings() {
    const continueBtn = document.getElementById('continue-to-metadata') as HTMLButtonElement;

    let allRequiredFilled = true;
    this.speakerMappings = {};

    // For each track, gather all the speaker info
    this.tracks.forEach(track => {
      const trackId = track.id;

      // Get player name (required)
      const playerNameInput = document.querySelector(
        `.player-name-input[data-track-id="${trackId}"]`
      ) as HTMLInputElement;
      const playerName = playerNameInput?.value.trim() || '';

      // Get character name (optional)
      const characterNameInput = document.querySelector(
        `.character-name-input[data-track-id="${trackId}"]`
      ) as HTMLInputElement;
      const characterName = characterNameInput?.value.trim() || '';

      // Get pronouns (optional)
      const pronounsSelect = document.querySelector(
        `.pronouns-select[data-track-id="${trackId}"]`
      ) as HTMLSelectElement;
      const pronounsCustomInput = document.querySelector(
        `.pronouns-custom-input[data-track-id="${trackId}"]`
      ) as HTMLInputElement;

      let pronouns = pronounsSelect?.value || '';
      if (pronouns === 'custom') {
        pronouns = pronounsCustomInput?.value.trim() || '';
      }

      // Validate that at least player name is filled
      if (!playerName) {
        allRequiredFilled = false;
      } else {
        // Store speaker info
        this.speakerMappings[trackId] = {
          playerName,
          characterName: characterName || undefined,
          pronouns: pronouns || undefined
        };
      }
    });

    continueBtn.disabled = !allRequiredFilled;
  }

  private async proceedToMetadata() {
    if (!this.currentSessionId) return;

    try {
      const response = await window.fetch(`${API_BASE_URL}/label-speakers`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          session_id: this.currentSessionId,
          mappings: this.speakerMappings
        }),
      });

      if (response.ok) {
        await this.loadSessionDefaults();
        this.showScreen('metadata-screen');
      } else {
        const error = await response.json();
        alert(`Failed to save speaker mappings: ${error.detail}`);
      }
    } catch (error) {
      console.error('Failed to save speaker mappings:', error);
      alert('Failed to save speaker mappings');
    }
  }

  private async loadSessionDefaults() {
    try {
      // Get next session number
      const sessionResponse = await window.fetch(`${API_BASE_URL}/next-session-number`);
      if (sessionResponse.ok) {
        const data = await sessionResponse.json();
        const sessionNumberInput = document.getElementById('session-number') as HTMLInputElement;
        if (sessionNumberInput) {
          sessionNumberInput.value = data.next_session_number.toString();
        }
      }

      // Set today's date as default
      const dateInput = document.getElementById('session-date') as HTMLInputElement;
      if (dateInput) {
        dateInput.value = new Date().toISOString().split('T')[0];
      }

      // Load campaigns if available
      const campaignsResponse = await window.fetch(`${API_BASE_URL}/campaigns`);
      if (campaignsResponse.ok) {
        const campaignData = await campaignsResponse.json();
        const currentCampaign = campaignData.campaigns[campaignData.current_campaign];
        if (currentCampaign) {
          const campaignInput = document.getElementById('campaign-name') as HTMLInputElement;
          if (campaignInput) {
            campaignInput.value = currentCampaign.name;
          }
        }
      }
    } catch (error) {
      console.error('Failed to load session defaults:', error);
    }
  }

  private async proceedToTranscription() {
    if (!this.currentSessionId) return;

    try {
      // Collect metadata from form
      const sessionDate = (document.getElementById('session-date') as HTMLInputElement)?.value;
      const sessionNumber = parseInt((document.getElementById('session-number') as HTMLInputElement)?.value || '1');
      const campaignName = (document.getElementById('campaign-name') as HTMLInputElement)?.value;
      const locationsText = (document.getElementById('session-locations') as HTMLInputElement)?.value;
      const charactersText = (document.getElementById('characters-present') as HTMLInputElement)?.value;
      const tagsText = (document.getElementById('session-tags') as HTMLInputElement)?.value;
      const notes = (document.getElementById('session-notes') as HTMLTextAreaElement)?.value;

      // Parse comma-separated values
      const locations = locationsText ? locationsText.split(',').map(l => l.trim()).filter(l => l) : [];
      const characters = charactersText ? charactersText.split(',').map(c => c.trim()).filter(c => c) : [];
      const tags = tagsText ? tagsText.split(',').map(t => t.trim()).filter(t => t) : [];

      const metadata: SessionMetadata = {
        session_id: this.currentSessionId,
        session_date: sessionDate || undefined,
        session_number: sessionNumber || undefined,
        campaign_name: campaignName || undefined,
        locations: locations,
        characters_present: characters,
        tags: tags,
        notes: notes || undefined
      };

      // Save metadata
      const response = await window.fetch(`${API_BASE_URL}/session-metadata`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(metadata),
      });

      if (response.ok) {
        await this.loadTranscriptionSettings();
        this.showScreen('transcription-screen');
      } else {
        const error = await response.json();
        alert(`Failed to save session metadata: ${error.detail}`);
      }
    } catch (error) {
      console.error('Failed to save session metadata:', error);
      alert('Failed to save session metadata');
    }
  }

  private async loadTranscriptionSettings() {
    try {
      const response = await window.fetch(`${API_BASE_URL}/settings`);
      if (response.ok) {
        const settings: Settings = await response.json();

        const whisperModelSelect = document.getElementById('whisper-model-select') as HTMLSelectElement;
        const transcriptionLanguageSelect = document.getElementById('transcription-language-select-wizard') as HTMLSelectElement;

        // Populate Whisper models if available
        if (whisperModelSelect && settings.available_whisper_models) {
          whisperModelSelect.innerHTML = '';
          Object.entries(settings.available_whisper_models).forEach(([model, description]) => {
            const option = document.createElement('option');
            option.value = model;
            option.textContent = description;
            whisperModelSelect.appendChild(option);
          });

          // Set current value
          if (settings.whisper_model) {
            whisperModelSelect.value = settings.whisper_model;
          }
        }

        // Set transcription language
        if (transcriptionLanguageSelect && settings.transcription_language) {
          transcriptionLanguageSelect.value = settings.transcription_language;
        }
      }
    } catch (error) {
      console.error('Failed to load transcription settings:', error);
    }
  }

  private async proceedToSummarization() {
    // Optionally save transcription settings if checkbox is checked
    const saveCheckbox = document.getElementById('save-transcription-settings') as HTMLInputElement;
    if (saveCheckbox?.checked) {
      await this.saveTranscriptionSettingsToBackend();
    }

    await this.loadSummarizationSettings();
    this.showScreen('summarization-screen');
  }

  private async saveTranscriptionSettingsToBackend() {
    try {
      const whisperModelSelect = document.getElementById('whisper-model-select') as HTMLSelectElement;
      const transcriptionLanguageSelect = document.getElementById('transcription-language-select-wizard') as HTMLSelectElement;

      // Get current settings
      const response = await window.fetch(`${API_BASE_URL}/settings`);
      if (!response.ok) return;

      const currentSettings: Settings = await response.json();

      // Update only transcription-related settings
      const updates = {
        ...currentSettings,
        whisper_model: whisperModelSelect?.value,
        transcription_language: transcriptionLanguageSelect?.value
      };

      await window.fetch(`${API_BASE_URL}/settings`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(updates),
      });
    } catch (error) {
      console.error('Failed to save transcription settings:', error);
    }
  }

  private async loadSummarizationSettings() {
    try {
      const response = await window.fetch(`${API_BASE_URL}/settings`);
      if (response.ok) {
        const settings: Settings = await response.json();

        // Set LLM preference
        if (settings.llm_preference) {
          const radioButton = document.querySelector(`input[name="llm-engine-wizard"][value="${settings.llm_preference}"]`) as HTMLInputElement;
          if (radioButton) {
            radioButton.checked = true;
          }
        }

        // Load Ollama models
        await this.refreshWizardModels();
      }
    } catch (error) {
      console.error('Failed to load summarization settings:', error);
    }
  }

  private async saveSummarizationSettingsToBackend() {
    try {
      const llmEngine = document.querySelector('input[name="llm-engine-wizard"]:checked') as HTMLInputElement;
      const ollamaModelSelect = document.getElementById('ollama-model-select-wizard') as HTMLSelectElement;

      // Get current settings
      const response = await window.fetch(`${API_BASE_URL}/settings`);
      if (!response.ok) return;

      const currentSettings: Settings = await response.json();

      // Update only summarization-related settings
      const updates = {
        ...currentSettings,
        llm_preference: llmEngine?.value as 'local' | 'cloud',
        ollama_model: ollamaModelSelect?.value
      };

      await window.fetch(`${API_BASE_URL}/settings`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(updates),
      });
    } catch (error) {
      console.error('Failed to save summarization settings:', error);
    }
  }

  private async refreshWizardModels() {
    const modelSelect = document.getElementById('ollama-model-select-wizard') as HTMLSelectElement;
    if (!modelSelect) return;

    try {
      const response = await window.fetch(`${API_BASE_URL}/ollama-models`);

      if (response.ok) {
        const data = await response.json();

        if (!data.server_running) {
          console.warn('Ollama server not running');
          return;
        }

        if (data.models && data.models.length > 0) {
          // Store current selection
          const currentValue = modelSelect.value;

          // Clear existing options
          modelSelect.innerHTML = '';

          // Add models as options
          data.models.forEach((model: string) => {
            const option = document.createElement('option');
            option.value = model;
            option.textContent = model;
            modelSelect.appendChild(option);
          });

          // Restore previous selection if it exists
          if (data.models.includes(currentValue)) {
            modelSelect.value = currentValue;
          }
        }
      }
    } catch (error) {
      console.error('Failed to refresh wizard models:', error);
    }
  }

  private async generateNotes() {
    if (!this.currentSessionId) return;

    const generateBtn = document.getElementById('generate-notes-btn') as HTMLButtonElement;
    const llmEngine = document.querySelector('input[name="llm-engine-wizard"]:checked') as HTMLInputElement;

    generateBtn.disabled = true;

    // Optionally save summarization settings if checkbox is checked
    const saveSummarizationCheckbox = document.getElementById('save-summarization-settings') as HTMLInputElement;
    if (saveSummarizationCheckbox?.checked) {
      await this.saveSummarizationSettingsToBackend();
    }

    this.showProcessingStatus('Transcribing audio and generating notes... This may take several minutes.', 'loading');

    try {
      const response = await window.fetch(`${API_BASE_URL}/generate-notes`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          session_id: this.currentSessionId,
          llm_engine: llmEngine.value
        }),
      });

      if (response.ok) {
        const data = await response.json();
        this.generatedNotes = data.summary;
        this.showProcessingStatus('Notes generated successfully!', 'success');
        
        // Display metadata suggestions if available
        if (data.metadata_suggestions) {
          this.showProcessingStatus('Notes and metadata suggestions generated successfully!', 'success');
          // Go to suggestions screen to show AI suggestions
          setTimeout(() => {
            this.showSuggestionsScreen(data.metadata_suggestions);
          }, 1500);
        } else {
          // No suggestions generated, go directly to export
          const notesTextarea = document.getElementById('notes-content') as HTMLTextAreaElement;
          if (notesTextarea) {
            notesTextarea.value = this.generatedNotes;
          }
          
          setTimeout(() => this.showScreen('export-screen'), 1500);
        }
      } else {
        const error = await response.json();
        this.showProcessingStatus(`Failed to generate notes: ${error.detail}`, 'error');
      }
    } catch (error) {
      console.error('Failed to generate notes:', error);
      this.showProcessingStatus('Failed to generate notes: Network error', 'error');
    } finally {
      generateBtn.disabled = false;
    }
  }

  private async exportNotes() {
    if (!this.currentSessionId) return;

    try {
      const useObsidianFormat = (document.getElementById('use-obsidian-format') as HTMLInputElement)?.checked || false;
      const customFilename = (document.getElementById('custom-filename') as HTMLInputElement)?.value || undefined;

      // Get formatted content from backend
      const response = await window.fetch(`${API_BASE_URL}/export-obsidian`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          session_id: this.currentSessionId,
          use_obsidian_format: useObsidianFormat,
          custom_filename: customFilename
        }),
      });

      if (response.ok) {
        const data = await response.json();
        
        // Create a blob with the formatted content
        const blob = new Blob([data.content], { type: 'text/markdown' });
        const url = URL.createObjectURL(blob);
        
        // Create a temporary download link
        const link = document.createElement('a');
        link.href = url;
        link.download = data.filename;
        link.style.display = 'none';
        
        // Trigger download
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        
        // Clean up the URL object
        URL.revokeObjectURL(url);

        this.showStatus('notification', 'Notes exported successfully!', 'success');
      } else {
        const error = await response.json();
        this.showStatus('notification', `Export failed: ${error.detail}`, 'error');
      }
    } catch (error) {
      console.error('Export error:', error);
      this.showStatus('notification', 'Export failed', 'error');
    }
  }

  private startNewSession() {
    this.currentSessionId = null;
    this.tracks = [];
    this.speakerMappings = {};
    this.generatedNotes = '';
    
    // Reset UI
    const notesTextarea = document.getElementById('notes-content') as HTMLTextAreaElement;
    if (notesTextarea) {
      notesTextarea.value = '';
    }

    // Reset metadata form
    const metadataInputs = ['session-date', 'session-number', 'campaign-name', 'session-locations', 'characters-present', 'session-tags', 'session-notes'];
    metadataInputs.forEach(id => {
      const element = document.getElementById(id) as HTMLInputElement | HTMLTextAreaElement;
      if (element) {
        element.value = '';
      }
    });

    // Reset export options
    const obsidianCheckbox = document.getElementById('use-obsidian-format') as HTMLInputElement;
    if (obsidianCheckbox) {
      obsidianCheckbox.checked = false;
    }

    const filenameInput = document.getElementById('custom-filename') as HTMLInputElement;
    if (filenameInput) {
      filenameInput.value = '';
    }
    
    this.showScreen('upload-screen');
    this.hideStatus();
  }

  private showScreen(screenId: string) {
    // Hide all screens
    document.querySelectorAll('.screen').forEach(screen => {
      screen.classList.remove('active');
    });

    // Show target screen
    document.getElementById(screenId)?.classList.add('active');
  }

  /**
   * Unified status display method
   * @param elementId - ID of the status element (or 'notification' for temporary popup)
   * @param message - Status message to display
   * @param type - Type of status: loading, success, or error
   */
  private showStatus(elementId: string, message: string, type: 'loading' | 'success' | 'error') {
    // Handle temporary notification (no specific element)
    if (elementId === 'notification') {
      const notification = document.createElement('div');
      notification.className = `upload-status ${type}`;
      notification.textContent = message;
      notification.style.position = 'fixed';
      notification.style.top = '20px';
      notification.style.right = '20px';
      notification.style.zIndex = '9999';
      notification.style.minWidth = '300px';

      document.body.appendChild(notification);

      setTimeout(() => {
        document.body.removeChild(notification);
      }, 3000);
      return;
    }

    // Handle persistent status in specific element
    const statusEl = document.getElementById(elementId);
    if (statusEl) {
      statusEl.textContent = message;
      statusEl.className = `${elementId.replace('-status', '')}-status ${type}`;
      statusEl.classList.remove('hidden');
    }
  }

  // Convenience wrappers for backward compatibility
  private showUploadStatus(message: string, type: 'loading' | 'success' | 'error') {
    this.showStatus('upload-status', message, type);
  }

  private showProcessingStatus(message: string, type: 'loading' | 'success' | 'error') {
    this.showStatus('processing-status', message, type);
  }

  private hideStatus() {
    document.getElementById('upload-status')?.classList.add('hidden');
    document.getElementById('processing-status')?.classList.add('hidden');
  }

  private addSuggestionToInput(suggestion: string, inputId: string) {
    const input = document.getElementById(inputId) as HTMLInputElement;
    if (!input) return;

    const currentValue = input.value.trim();
    const currentItems = currentValue ? currentValue.split(',').map(s => s.trim()) : [];

    // Don't add if already present
    if (currentItems.includes(suggestion)) return;

    currentItems.push(suggestion);
    input.value = currentItems.join(', ');

    // Add visual feedback (note: event is not available in arrow functions, so this won't work perfectly)
    // We'll leave it as is since the main functionality works
  }


  private async testOllamaConnection() {
    const urlInput = document.getElementById('ollama-base-url') as HTMLInputElement;
    const testBtn = document.getElementById('test-ollama-connection-btn') as HTMLButtonElement;
    const statusEl = document.getElementById('connection-status');

    if (!urlInput || !statusEl) return;

    const baseUrl = urlInput.value.trim() || 'http://127.0.0.1:11434';

    testBtn.disabled = true;
    statusEl.textContent = 'Testing connection...';
    statusEl.className = 'model-status loading';
    statusEl.classList.remove('hidden');

    try {
      const response = await window.fetch(`${API_BASE_URL}/test-ollama-connection`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ base_url: baseUrl }),
      });

      if (response.ok) {
        const data = await response.json();

        if (data.status === 'success') {
          statusEl.textContent = `✓ Connected! Found ${data.models_count} models`;
          statusEl.className = 'model-status success';
        } else {
          statusEl.textContent = `✗ ${data.message || 'Connection failed'}`;
          statusEl.className = 'model-status error';
        }
      } else {
        statusEl.textContent = '✗ Failed to test connection';
        statusEl.className = 'model-status error';
      }
    } catch (error) {
      console.error('Connection test error:', error);
      statusEl.textContent = '✗ Network error';
      statusEl.className = 'model-status error';
    } finally {
      testBtn.disabled = false;
      setTimeout(() => {
        statusEl.classList.add('hidden');
      }, 5000);
    }
  }

  private showSuggestionsScreen(metadataSuggestions: MetadataSuggestions) {
    this.showScreen('suggestions-screen');
    
    // Load current metadata values from the previous step
    this.loadCurrentMetadataToReview();
    
    // Display AI suggestions
    this.displayReviewSuggestions(metadataSuggestions);
    
    // Prepare notes for export screen
    const notesTextarea = document.getElementById('notes-content') as HTMLTextAreaElement;
    if (notesTextarea) {
      notesTextarea.value = this.generatedNotes;
    }
  }

  private loadCurrentMetadataToReview() {
    // Load current values from the metadata screen into the review screen
    const sessionTags = (document.getElementById('session-tags') as HTMLInputElement)?.value || '';
    const charactersPresent = (document.getElementById('characters-present') as HTMLInputElement)?.value || '';
    const sessionLocations = (document.getElementById('session-locations') as HTMLInputElement)?.value || '';
    
    // Set values in review screen
    (document.getElementById('review-session-tags') as HTMLInputElement).value = sessionTags;
    (document.getElementById('review-characters-present') as HTMLInputElement).value = charactersPresent;
    (document.getElementById('review-session-locations') as HTMLInputElement).value = sessionLocations;
  }

  private displayReviewSuggestions(suggestions: MetadataSuggestions) {
    // Display suggested tags
    this.displayReviewChipSuggestions('review-tags-suggestions', suggestions.suggested_tags, 'review-session-tags');
    
    // Display mentioned characters
    this.displayReviewChipSuggestions('review-characters-suggestions', suggestions.mentioned_characters, 'review-characters-present');
    
    // Display mentioned locations
    this.displayReviewChipSuggestions('review-locations-suggestions', suggestions.mentioned_locations, 'review-session-locations');
  }

  private displayReviewChipSuggestions(containerId: string, suggestions: string[], targetInputId: string) {
    const container = document.getElementById(containerId);
    if (!container) return;

    container.innerHTML = '';
    
    suggestions.forEach(suggestion => {
      const chip = document.createElement('span');
      chip.className = 'suggestion-chip';
      chip.textContent = suggestion;
      chip.addEventListener('click', () => this.addSuggestionToInput(suggestion, targetInputId));
      container.appendChild(chip);
    });
  }

  private async proceedToExport() {
    if (!this.currentSessionId) return;

    try {
      // Get updated metadata from review screen
      const sessionTags = (document.getElementById('review-session-tags') as HTMLInputElement)?.value || '';
      const charactersPresent = (document.getElementById('review-characters-present') as HTMLInputElement)?.value || '';
      const sessionLocations = (document.getElementById('review-session-locations') as HTMLInputElement)?.value || '';

      // Parse comma-separated values
      const tags = sessionTags ? sessionTags.split(',').map(t => t.trim()).filter(t => t) : [];
      const characters = charactersPresent ? charactersPresent.split(',').map(c => c.trim()).filter(c => c) : [];
      const locations = sessionLocations ? sessionLocations.split(',').map(l => l.trim()).filter(l => l) : [];

      // Get the original metadata and update with new values
      const currentMetadata = await this.getCurrentMetadata();
      const updatedMetadata: SessionMetadata = {
        ...currentMetadata,
        session_id: this.currentSessionId,
        tags: tags,
        characters_present: characters,
        locations: locations
      };

      // Save updated metadata
      const response = await window.fetch(`${API_BASE_URL}/session-metadata`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(updatedMetadata),
      });

      if (response.ok) {
        this.showScreen('export-screen');
      } else {
        const error = await response.json();
        console.error(`Failed to save updated metadata: ${error.detail}`);
        // Continue to export anyway
        this.showScreen('export-screen');
      }
    } catch (error) {
      console.error('Failed to save updated metadata:', error);
      // Continue to export anyway
      this.showScreen('export-screen');
    }
  }

  private async getCurrentMetadata(): Promise<Partial<SessionMetadata>> {
    if (!this.currentSessionId) return {};

    try {
      const response = await window.fetch(`${API_BASE_URL}/session-metadata/${this.currentSessionId}`);
      if (response.ok) {
        return await response.json();
      }
    } catch (error) {
      console.error('Failed to get current metadata:', error);
    }
    return {};
  }
}

// Initialize the app when DOM is loaded
window.addEventListener('DOMContentLoaded', () => {
  new ChronicleKeeperApp();
});