import { invoke } from '@tauri-apps/api/core';

type Preferences = { enabled: boolean; sound: boolean };
const STORAGE_KEY = 'shinatama.attention.v1';

function readPreferences(): Preferences {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      const record = value as Record<string, unknown>;
      if (typeof record.enabled === 'boolean' && typeof record.sound === 'boolean') {
        return { enabled: record.enabled, sound: record.sound };
      }
    }
  } catch { /* An unavailable or malformed preference uses the explicit defaults. */ }
  return { enabled: true, sound: true };
}

/** The desktop alert controls, shown in the Alerts section of Settings (settings-panel.ts). Each
 * switch applies at once; this UI never answers an agent prompt. */
export function createAttentionControl(desktop: boolean, report: (message: string) => void) {
  let preferences = readPreferences();
  let busy = false;
  let configured = false;
  let initialization: Promise<void> | null = null;

  const fields = document.createElement('div'); fields.className = 'attention-fields';
  const description = document.createElement('p');
  description.textContent = 'Shinatama shows its face and a speech bubble over the desktop when a terminal raises a native alert or a process ends. Alerts work while the app is open.';
  const limits = document.createElement('p'); limits.className = 'hint';
  limits.textContent = 'Which signals arrive depends on the agent. Read the question and answer it in the session: the alert opens the terminal and grants nothing.';

  function checkbox(id: string, text: string) {
    const label = document.createElement('label'); label.htmlFor = id; label.className = 'settings-check';
    const input = document.createElement('input'); input.id = id; input.type = 'checkbox';
    label.append(input, document.createTextNode(` ${text}`));
    return { label, input };
  }
  const enabled = checkbox('attention-enabled', 'Show desktop alerts');
  const sound = checkbox('attention-sound', 'Play a short sound');
  const status = document.createElement('p'); status.id = 'attention-settings-status';
  status.className = 'hint'; status.setAttribute('role', 'status');
  const actions = document.createElement('div'); actions.className = 'settings-row';
  const preview = document.createElement('button'); preview.id = 'attention-test';
  preview.type = 'button'; preview.textContent = 'Test alert';
  actions.append(preview);
  fields.append(description, enabled.label, sound.label, actions, status, limits);
  fields.setAttribute('aria-busy', 'false');

  function render() {
    enabled.input.checked = preferences.enabled;
    sound.input.checked = preferences.sound;
    enabled.input.disabled = !desktop || busy;
    sound.input.disabled = !desktop || busy;
    preview.disabled = !desktop || busy || !configured || !preferences.enabled;
    fields.setAttribute('aria-busy', String(busy));
  }

  async function configure(next: Preferences, persist: boolean) {
    if (!desktop || busy) { render(); return; }
    busy = true; render();
    // Keep the requested toggle visible while native application is pending.
    enabled.input.checked = next.enabled;
    sound.input.checked = next.sound;
    status.textContent = 'Updating alerts…';
    try {
      await invoke('attention_configure', { enabled: next.enabled, muted: !next.sound });
      preferences = next;
      configured = true;
      status.textContent = next.enabled
        ? (next.sound ? 'Alerts and sound on.' : 'Alerts on; sound off.')
        : 'Desktop alerts off.';
      if (persist) {
        try { localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences)); }
        catch {
          status.textContent = 'Applied until Shinatama closes. This setting could not be saved.';
          report(status.textContent);
        }
      }
    } catch {
      status.textContent = 'The alert settings were not applied. Change the option again to retry.';
      report(status.textContent);
    } finally { busy = false; render(); }
  }

  enabled.input.addEventListener('change', () => {
    void configure({ enabled: enabled.input.checked, sound: preferences.sound }, true);
  });
  sound.input.addEventListener('change', () => {
    void configure({ enabled: preferences.enabled, sound: sound.input.checked }, true);
  });
  preview.addEventListener('click', () => {
    if (busy || !configured || !preferences.enabled || !desktop) return;
    busy = true; render(); status.textContent = 'Opening a test alert…';
    void invoke('attention_preview', { kind: 'input' }).then(() => {
      status.textContent = 'Test alert shown.';
    }).catch(() => {
      status.textContent = 'The test alert did not open. Try again.';
      report(status.textContent);
    }).finally(() => { busy = false; render(); });
  });
  if (!desktop) status.textContent = 'Open the desktop app to change alerts.';
  render();
  return {
    /** The Alerts section's controls. */
    fields,
    /** Shows the applied preferences again. */
    reset: render,
    initialize(): Promise<void> {
      if (!initialization) initialization = configure({ ...preferences }, false);
      return initialization;
    },
  };
}
