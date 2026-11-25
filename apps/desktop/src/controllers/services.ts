// The services Shinatama keeps for its sessions and what the person decides about them: Muro's sync of the agents' plugins, the model
// proxy (the sidecar), the memory safety of terminals, the models the agents use, and the alerts. Each reads its state through the
// parser of its model module (`muro-status.ts`, `sidecar-status.ts`, `settings-model.ts`, `model-settings-model.ts`), which refuses what
// it does not know. The Settings screens ask for a read when they open and after each change.
import { errorText } from '../api/core.ts';
import { settingsApi } from '../api/settings.ts';
import type { Shinatama } from '../app.ts';
import { parseModelView, type View as ModelView } from '../model-settings-model.ts';
import { parseMuroStatus, type MuroStatus } from '../muro-status.ts';
import { DEFAULT_MEMORY, parseMemory, type MemorySafety } from '../settings-model.ts';
import { parseStatus, type ProxyStatus } from '../sidecar-status.ts';
import { Store } from '../store.ts';

export type AlertPreferences = { enabled: boolean; sound: boolean };
const ALERTS_KEY = 'shinatama.attention.v1';

function readAlertPreferences(): AlertPreferences {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(ALERTS_KEY) || 'null');
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      const record = value as Record<string, unknown>;
      if (typeof record.enabled === 'boolean' && typeof record.sound === 'boolean') return { enabled: record.enabled, sound: record.sound };
    }
  } catch { /* An unavailable or malformed preference uses the explicit defaults. */ }
  return { enabled: true, sound: true };
}

export class ServicesController extends Store {
  muro: MuroStatus | null = null;
  proxy: ProxyStatus | null = null;
  memory: MemorySafety = DEFAULT_MEMORY;
  models: ModelView | null = null;
  /** The desktop alerts the person chose (the attention widget), kept in this window's storage and applied to Rust at the start. */
  alerts: AlertPreferences = readAlertPreferences();
  /** What the last change of the alerts said, for the Alerts section of Settings. */
  alertsStatus = '';
  alertsBusy = false;
  alertsApplied = false;
  /** What a read said when it failed, by service. */
  readonly problems = new Map<string, string>();

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  private async read<T>(name: string, read: () => Promise<unknown>, parse: (value: unknown) => T | null, keep: (value: T) => void): Promise<void> {
    try {
      const value = parse(await read());
      if (value === null) throw new Error('an answer this version does not know');
      keep(value);
      this.problems.delete(name);
    } catch (error) { this.problems.set(name, errorText(error)); }
    this.changed();
  }

  refreshMuro(): Promise<void> {
    return this.read('muro', settingsApi.muroStatus, parseMuroStatus, value => { this.muro = value; });
  }

  async syncMuro(): Promise<void> {
    await this.read('muro', settingsApi.muroSyncNow, parseMuroStatus, value => { this.muro = value; });
  }

  refreshProxy(): Promise<void> {
    return this.read('proxy', settingsApi.sidecarStatus, parseStatus, value => { this.proxy = value; });
  }

  async startProxy(): Promise<void> { await this.read('proxy', settingsApi.sidecarStart, parseStatus, value => { this.proxy = value; }); }
  async stopProxy(force = false): Promise<void> { await this.read('proxy', () => settingsApi.sidecarStop(force), parseStatus, value => { this.proxy = value; }); }
  async retryProxy(): Promise<void> { await this.read('proxy', settingsApi.sidecarRetry, parseStatus, value => { this.proxy = value; }); }

  refreshMemory(): Promise<void> {
    return this.read('memory', settingsApi.memorySettingsLoad, parseMemory, value => { this.memory = value; });
  }

  async saveMemory(memory: MemorySafety): Promise<void> {
    await this.read('memory', () => settingsApi.memorySettingsSave(memory), parseMemory, value => { this.memory = value; });
    if (this.problems.has('memory')) this.app.ui.error(`Memory safety was not saved: ${this.problems.get('memory')}`);
  }

  refreshModels(): Promise<void> {
    return this.read('models', settingsApi.modelSettingsLoad, parseModelView, value => { this.models = value; });
  }

  // --- the alerts -----------------------------------------------------------------------------------------

  /** Applies the alert preferences to Rust (at the start, and at each change the person makes). Each applies at once; the alert never
   *  answers an agent's prompt. */
  async configureAlerts(next: AlertPreferences, persist: boolean): Promise<void> {
    if (this.alertsBusy) return;
    this.alertsBusy = true;
    this.alertsStatus = 'Updating alerts…';
    this.changed();
    try {
      await settingsApi.attentionConfigure(next.enabled, !next.sound);
      this.alerts = next;
      this.alertsApplied = true;
      this.alertsStatus = next.enabled ? (next.sound ? 'Alerts and sound on.' : 'Alerts on; sound off.') : 'Desktop alerts off.';
      if (persist) {
        try { localStorage.setItem(ALERTS_KEY, JSON.stringify(next)); }
        catch {
          this.alertsStatus = 'Applied until Shinatama closes. This setting could not be saved.';
          this.app.ui.error(this.alertsStatus);
        }
      }
    } catch {
      this.alertsStatus = 'The alert settings were not applied. Change the option again to retry.';
      this.app.ui.error(this.alertsStatus);
    } finally { this.alertsBusy = false; this.changed(); }
  }

  /** Shows a test alert in the widget. */
  async previewAlert(): Promise<void> {
    if (this.alertsBusy || !this.alertsApplied || !this.alerts.enabled) return;
    this.alertsBusy = true;
    this.alertsStatus = 'Opening a test alert…';
    this.changed();
    try { await settingsApi.attentionPreview('input'); this.alertsStatus = 'Test alert shown.'; }
    catch { this.alertsStatus = 'The test alert did not open. Try again.'; this.app.ui.error(this.alertsStatus); }
    finally { this.alertsBusy = false; this.changed(); }
  }

  // --- what runs by itself --------------------------------------------------------------------------------------

  private timer: ReturnType<typeof setInterval> | undefined;
  private lastHint = -Infinity;
  private lastNotice: string | null = null;
  private checking = false;
  private readonly hint = (): void => {
    const now = performance.now();
    if (now - this.lastHint < 1000) return;
    this.lastHint = now;
    void settingsApi.maintenanceActivity().catch(() => {});
  };

  async start(): Promise<void> {
    await this.configureAlerts({ ...this.alerts }, false);
    // The archive maintenance waits for the person to stop: hints of activity say nothing of what is typed or shown (no keys, no text,
    // no paths).
    for (const name of ['keydown', 'pointerdown', 'pointermove', 'wheel']) document.addEventListener(name, this.hint, { capture: true, passive: true });
    window.addEventListener('focus', this.hint);
    this.hint();
    this.timer = setInterval(() => { void this.watchMaintenance(); }, 60_000);
  }

  dispose(): void {
    clearInterval(this.timer);
    for (const name of ['keydown', 'pointerdown', 'pointermove', 'wheel']) document.removeEventListener(name, this.hint, { capture: true });
    window.removeEventListener('focus', this.hint);
  }

  private async watchMaintenance(): Promise<void> {
    if (this.checking || document.hidden) return;
    this.checking = true;
    try {
      const status = await settingsApi.maintenanceStatus() as { phase: string; message: string | null };
      const message = status.message || 'The saved archive needs attention. Your saved work has been kept.';
      if (status.phase === 'attention' && message !== this.lastNotice) { this.lastNotice = message; this.app.ui.error(message); }
      if (status.phase === 'idle') this.lastNotice = null;
    } catch { /* A closed or unavailable runtime does not start retries or block controls. */ }
    finally { this.checking = false; }
  }
}
