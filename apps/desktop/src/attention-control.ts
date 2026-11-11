import { invoke } from '@tauri-apps/api/core';
import { openModal } from './modal.ts';

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

/** Settings for the early desktop alert window; this UI never answers an agent prompt. */
export function createAttentionControl(desktop: boolean, report: (message: string) => void) {
  let preferences = readPreferences();
  let busy = false;
  let configured = false;
  let initialization: Promise<void> | null = null;

  const button = document.createElement('button');
  button.id = 'desktop-alerts'; button.type = 'button'; button.textContent = 'Shinatama alerts';
  button.disabled = !desktop;
  document.querySelector('.toolbar')?.append(button);

  const dialog = document.createElement('dialog'); dialog.id = 'attention-settings';
  dialog.lang = 'it'; dialog.setAttribute('aria-labelledby', 'attention-settings-title');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1');
  title.id = 'attention-settings-title'; title.textContent = 'Avvisi desktop di Shinatama';
  const close = document.createElement('button'); close.type = 'button'; close.textContent = '×';
  close.setAttribute('aria-label', 'Chiudi impostazioni avvisi');
  heading.append(title, close);

  const description = document.createElement('p');
  description.textContent = 'Shinatama mostra il viso e un fumetto sopra il desktop quando un terminale segnala un avviso nativo o un processo termina. Questa anteprima funziona mentre l’app è aperta.';
  const limits = document.createElement('p'); limits.className = 'hint';
  limits.textContent = 'La disponibilità dei segnali dipende dall’harness. Leggi il contenuto della domanda e rispondi nella sessione: il widget apre il terminale e non concede permessi.';

  function checkbox(id: string, text: string) {
    const label = document.createElement('label'); label.htmlFor = id;
    label.style.display = 'flex'; label.style.alignItems = 'center'; label.style.gap = '9px';
    label.style.letterSpacing = 'normal'; label.style.textTransform = 'none';
    const input = document.createElement('input'); input.id = id; input.type = 'checkbox';
    input.style.width = '16px'; input.style.height = '16px'; input.style.padding = '0'; input.style.margin = '0';
    input.style.accentColor = 'var(--accent)';
    const caption = document.createElement('span'); caption.textContent = text;
    label.append(input, caption);
    return { label, input };
  }
  const enabled = checkbox('attention-enabled', 'Mostra gli avvisi desktop');
  const sound = checkbox('attention-sound', 'Riproduci un breve suono');
  const status = document.createElement('p'); status.id = 'attention-settings-status';
  status.className = 'hint'; status.setAttribute('role', 'status');
  const actions = document.createElement('div'); actions.className = 'dialog-actions';
  const preview = document.createElement('button'); preview.id = 'attention-test';
  preview.type = 'button'; preview.className = 'primary'; preview.textContent = 'Prova viso e suono';
  actions.append(preview);
  dialog.append(heading, description, limits, enabled.label, sound.label, status, actions);
  (document.querySelector('#app') ?? document.body).append(dialog);

  function render() {
    enabled.input.checked = preferences.enabled;
    sound.input.checked = preferences.sound;
    enabled.input.disabled = !desktop || busy;
    sound.input.disabled = !desktop || busy;
    preview.disabled = !desktop || busy || !configured || !preferences.enabled;
    button.disabled = !desktop || busy;
    close.disabled = busy;
    dialog.setAttribute('aria-busy', String(busy));
  }

  async function configure(next: Preferences, persist: boolean) {
    if (!desktop || busy) { render(); return; }
    busy = true; render();
    // Keep the requested toggle visible while native application is pending.
    enabled.input.checked = next.enabled;
    sound.input.checked = next.sound;
    status.textContent = 'Aggiornamento degli avvisi…';
    try {
      await invoke('attention_configure', { enabled: next.enabled, muted: !next.sound });
      preferences = next;
      configured = true;
      status.textContent = next.enabled
        ? (next.sound ? 'Avvisi e suono attivi.' : 'Avvisi visivi attivi; suono disattivato.')
        : 'Avvisi desktop disattivati.';
      if (persist) {
        try { localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences)); }
        catch {
          status.textContent = 'Impostazioni applicate per questa apertura. Il salvataggio locale non è disponibile.';
          report(status.textContent);
        }
      }
    } catch {
      status.textContent = 'Le impostazioni non sono state applicate. Riprova a cambiare l’opzione.';
      report(status.textContent);
    } finally { busy = false; render(); }
  }

  enabled.input.addEventListener('change', () => {
    void configure({ enabled: enabled.input.checked, sound: preferences.sound }, true);
  });
  sound.input.addEventListener('change', () => {
    void configure({ enabled: preferences.enabled, sound: sound.input.checked }, true);
  });
  button.addEventListener('click', () => {
    if (document.querySelector('dialog[open][aria-modal="true"]')) return;
    render(); openModal(dialog); close.focus();
  });
  close.addEventListener('click', () => { if (!busy) dialog.close(); });
  dialog.addEventListener('cancel', (event) => { if (busy) event.preventDefault(); });
  preview.addEventListener('click', () => {
    if (busy || !configured || !preferences.enabled || !desktop) return;
    busy = true; render(); status.textContent = 'Apertura dell’avviso di prova…';
    void invoke('attention_preview', { kind: 'input' }).then(() => {
      status.textContent = 'Avviso di prova aperto.';
      dialog.close();
    }).catch(() => {
      status.textContent = 'L’avviso di prova non è stato aperto. Riprova.';
      report(status.textContent);
    }).finally(() => { busy = false; render(); });
  });
  render();
  return {
    initialize(): Promise<void> {
      if (!initialization) initialization = configure({ ...preferences }, false);
      return initialization;
    },
  };
}
