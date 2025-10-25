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

class ChronicleKeeperApp {
  private currentSessionId: string | null = null;
  private tracks: Track[] = [];
  private speakerMappings: Record<string, string> = {};
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
    document.getElementById('continue-to-processing')?.addEventListener('click', () => this.proceedToProcessing());
    document.getElementById('back-to-metadata')?.addEventListener('click', () => this.showScreen('metadata-screen'));
    document.getElementById('back-to-processing')?.addEventListener('click', () => this.showScreen('processing-screen'));

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
        
        if (apiKeyInput && settings.gemini_api_key) {
          apiKeyInput.value = settings.gemini_api_key;
        }
        
        if (promptInput && settings.system_prompt) {
          promptInput.value = settings.system_prompt;
        }

        // Set LLM preference
        if (settings.llm_preference) {
          const radioButton = document.querySelector(`input[name="llm-engine"][value="${settings.llm_preference}"]`) as HTMLInputElement;
          if (radioButton) {
            radioButton.checked = true;
          }
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
    const llmEngine = document.querySelector('input[name="llm-engine"]:checked') as HTMLInputElement;

    const settings: Settings = {
      gemini_api_key: apiKeyInput.value,
      system_prompt: promptInput.value,
      llm_preference: llmEngine.value as 'local' | 'cloud'
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
        this.showStatus('Settings saved successfully!', 'success');
        this.closeSettings();
      } else {
        this.showStatus('Failed to save settings', 'error');
      }
    } catch (error) {
      console.error('Failed to save settings:', error);
      this.showStatus('Failed to save settings', 'error');
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
        <input 
          type="text" 
          class="speaker-input" 
          placeholder="Enter speaker name..."
          data-track-id="${track.id}"
        >
      `;
      container.appendChild(trackElement);
    });

    // Add event listeners to speaker inputs
    const speakerInputs = container.querySelectorAll('.speaker-input');
    speakerInputs.forEach(input => {
      input.addEventListener('input', () => this.validateSpeakerMappings());
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
    const inputs = document.querySelectorAll('.speaker-input') as NodeListOf<HTMLInputElement>;
    const continueBtn = document.getElementById('continue-to-metadata') as HTMLButtonElement;
    
    let allFilled = true;
    this.speakerMappings = {};

    inputs.forEach(input => {
      const trackId = input.getAttribute('data-track-id');
      const speakerName = input.value.trim();
      
      if (trackId) {
        if (speakerName) {
          this.speakerMappings[trackId] = speakerName;
        } else {
          allFilled = false;
        }
      }
    });

    continueBtn.disabled = !allFilled;
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

  private async proceedToProcessing() {
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
        this.showScreen('processing-screen');
      } else {
        const error = await response.json();
        alert(`Failed to save session metadata: ${error.detail}`);
      }
    } catch (error) {
      console.error('Failed to save session metadata:', error);
      alert('Failed to save session metadata');
    }
  }

  private async generateNotes() {
    if (!this.currentSessionId) return;

    const generateBtn = document.getElementById('generate-notes-btn') as HTMLButtonElement;
    const llmEngine = document.querySelector('input[name="llm-engine"]:checked') as HTMLInputElement;

    generateBtn.disabled = true;
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
        
        // Display notes in export screen
        const notesTextarea = document.getElementById('notes-content') as HTMLTextAreaElement;
        if (notesTextarea) {
          notesTextarea.value = this.generatedNotes;
        }
        
        setTimeout(() => this.showScreen('export-screen'), 1500);
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
        
        this.showStatus('Notes exported successfully!', 'success');
      } else {
        const error = await response.json();
        this.showStatus(`Export failed: ${error.detail}`, 'error');
      }
    } catch (error) {
      console.error('Export error:', error);
      this.showStatus('Export failed', 'error');
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

  private showUploadStatus(message: string, type: 'loading' | 'success' | 'error') {
    const statusEl = document.getElementById('upload-status');
    if (statusEl) {
      statusEl.textContent = message;
      statusEl.className = `upload-status ${type}`;
      statusEl.classList.remove('hidden');
    }
  }

  private showProcessingStatus(message: string, type: 'loading' | 'success' | 'error') {
    const statusEl = document.getElementById('processing-status');
    if (statusEl) {
      statusEl.textContent = message;
      statusEl.className = `processing-status ${type}`;
      statusEl.classList.remove('hidden');
    }
  }

  private showStatus(message: string, type: 'loading' | 'success' | 'error') {
    // Create temporary status notification
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
  }

  private hideStatus() {
    document.getElementById('upload-status')?.classList.add('hidden');
    document.getElementById('processing-status')?.classList.add('hidden');
  }
}

// Initialize the app when DOM is loaded
window.addEventListener('DOMContentLoaded', () => {
  new ChronicleKeeperApp();
});