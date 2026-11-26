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
import { adoptStopReply, confirmStillNeeded, parseStatus, restartOffered, stopConsequence, type ProxyStatus } from '../sidecar-status.ts';
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
  /** Muro's Sync now or Open Muro is under way; the model proxy has a command under way; the status reads that failed in a row. */
  muroBusy = false;
  proxyBusy = false;
  failedPolls = 0;
  private watching = false;
  private proxyTimer: ReturnType<typeof setInterval> | undefined;
  private muroTimer: ReturnType<typeof setTimeout> | undefined;

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
    return this.read('muro', settingsApi.muroStatus, parseMuroStatus, value => { this.muro = value; }).then(() => this.watchMuro());
  }

  /** A Muro that is still starting is asked again while Settings is shown. */
  private watchMuro(): void {
    clearTimeout(this.muroTimer);
    if (this.watching && this.muro?.state === 'starting') this.muroTimer = setTimeout(() => { void this.refreshMuro(); }, 2000);
  }

  async syncMuro(): Promise<void> {
    if (this.muroBusy) return;
    this.muroBusy = true;
    this.changed();
    try { await this.read('muro', settingsApi.muroSyncNow, parseMuroStatus, value => { this.muro = value; }); }
    finally { this.muroBusy = false; this.changed(); }
    this.watchMuro();
  }

  /** Shows Muro's window on the list of Shinatama's agents. */
  async openMuro(): Promise<void> {
    if (this.muroBusy) return;
    this.muroBusy = true;
    this.changed();
    try { await settingsApi.muroOpen(); this.problems.delete('muro'); }
    catch (error) { this.problems.set('muro', errorText(error)); }
    finally { this.muroBusy = false; this.changed(); }
    void this.refreshMuro();
  }

  // --- the model proxy ---------------------------------------------------------------------------------------

  /** While Settings is shown the proxy's status is read every 1.5 s, and Muro's while it starts. */
  startWatching(): void {
    this.watching = true;
    void this.refreshProxy();
    clearInterval(this.proxyTimer);
    this.proxyTimer = setInterval(() => { void this.refreshProxy(); }, 1500);
  }

  stopWatching(): void {
    this.watching = false;
    clearInterval(this.proxyTimer);
    clearTimeout(this.muroTimer);
  }

  async refreshProxy(): Promise<void> {
    if (this.proxyBusy) return;
    try {
      const status = parseStatus(await settingsApi.sidecarStatus());
      if (!status) throw new Error('Invalid status');
      this.proxy = status;
      this.failedPolls = 0;
      this.problems.delete('proxy');
    } catch (error) {
      this.failedPolls += 1;
      this.proxy = null;
      this.problems.set('proxy', errorText(error));
    }
    this.changed();
  }

  /** A command of the proxy (start, retry, stop): one at a time. A stop that would cut agents off asks first, with what it costs. */
  private async command(run: () => Promise<unknown>, stop = false): Promise<void> {
    if (this.proxyBusy) return;
    this.proxyBusy = true;
    this.changed();
    const before = this.proxy?.phase;
    try {
      const status = parseStatus(await run());
      if (!status) throw new Error('Invalid status');
      // A stop's reply is only the answer when it says something new; otherwise the status is read again.
      if (stop && !adoptStopReply(before)) { this.proxyBusy = false; await this.refreshProxy(); return; }
      this.proxy = status;
      this.problems.delete('proxy');
      this.proxyBusy = false;
      // The server saw agents the page had not: the stop it refused is asked again, with what it costs.
      if (status.refused) await this.stopProxy();
    } catch { this.ui.error('Model proxy action did not complete. Check its status before retrying.'); }
    finally { this.proxyBusy = false; this.changed(); }
  }

  private get ui() { return this.app.ui; }

  startProxy(): Promise<void> { return this.command(settingsApi.sidecarStart); }
  retryProxy(): Promise<void> { return this.command(settingsApi.sidecarRetry); }

  async stopProxy(): Promise<void> {
    const cost = this.proxy ? stopConsequence(this.proxy) : null;
    if (cost !== null && !await this.ui.ask({ title: 'Stop the model proxy?', description: cost, confirm: 'Stop model proxy' })) return;
    await this.command(() => settingsApi.sidecarStop(cost === null ? false : confirmStillNeeded(this.proxy)), true);
  }

  /** Whether the proxy's owner no longer answers, so Shinatama itself is offered a restart. */
  get restartOffered(): boolean {
    return restartOffered(this.proxy, this.failedPolls);
  }

  /** Closes the workspace and starts Shinatama again: the exit that works when the supervisor no longer answers. Asks first. */
  async restart(): Promise<void> {
    if (!await this.ui.ask({
      title: 'Restart Shinatama?', description: 'Shinatama will close and start again. Running terminals stop; their sessions are restored at the next start.', confirm: 'Restart Shinatama',
    })) return;
    try { await this.app.terminals.closeWorkspace(true); } catch (error) { this.ui.error(error); }
  }

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
    this.stopWatching();
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
