// What the person sets for the window, as Rust keeps it: the terminal presentation (font, line height, whether agents draw with the
// terminal's palette), and where the data lives. The look (scheme, glass, zoom) is the shell's (`useGlass`, `useWindow`), never kept here.
// The other settings (alerts, models, memory, Muro, the proxy of the models) are `services.ts`.
import { errorCode, errorText, sleep } from '../api/core.ts';
import { settingsApi, type Presentation } from '../api/settings.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';

export const DEFAULT_PRESENTATION: Presentation = {
  version: 1, skin: 'shinatama', followSkin: false, fontFamily: 'profile', fontSize: 13, lineHeight: 115,
};

/** How long a change waits for the next before it is saved. */
const SAVE_DELAY_MS = 400;

export class SettingsController extends Store {
  presentation: Presentation = DEFAULT_PRESENTATION;
  /** Whether the saved preferences were read (until then they are the defaults, and nothing is saved). */
  loaded = false;
  /** A change that is not saved yet, or a save that failed. */
  dirty = false;
  private revision = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private saving: Promise<boolean> | null = null;

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  async start(): Promise<void> {
    try {
      const snapshot = await settingsApi.presentationLoad();
      this.revision = snapshot.revision;
      this.presentation = snapshot.settings;
      this.loaded = true;
    } catch (error) {
      this.app.ui.error(`Presentation preferences could not be read: ${errorText(error)}`);
    }
    this.changed();
  }

  dispose(): void {
    clearTimeout(this.timer);
  }

  /** Changes the terminal presentation: drawn at once, saved a moment later. The skin is always the suite's. */
  setPresentation(patch: Partial<Omit<Presentation, 'version' | 'skin'>>): void {
    this.presentation = { ...this.presentation, ...patch, skin: 'shinatama' };
    this.dirty = true;
    this.changed();
    clearTimeout(this.timer);
    this.timer = setTimeout(() => { void this.save(); }, SAVE_DELAY_MS);
  }

  /** Saves what is pending. Resolves to whether everything is saved (the window's close asks before it loses a change). */
  flush(): Promise<boolean> {
    clearTimeout(this.timer);
    return this.dirty || this.saving ? this.save() : Promise.resolve(true);
  }

  private save(): Promise<boolean> {
    if (this.saving) return this.saving.then(() => this.save());
    if (!this.dirty || !this.loaded) return Promise.resolve(!this.dirty);
    const saving = (async () => {
      const sent = this.presentation;
      for (let attempt = 0; attempt < 2; attempt++) {
        try {
          const snapshot = await settingsApi.presentationSave(this.revision, sent);
          this.revision = snapshot.revision;
          if (this.presentation === sent) this.dirty = false;
          return true;
        } catch (error) {
          // Another window saved first: its revision is taken, and this change goes over it once.
          if (attempt === 0 && errorCode(error) === 'conflict') {
            try { this.revision = (await settingsApi.presentationLoad()).revision; await sleep(0); continue; } catch { /* reported below */ }
          }
          this.app.ui.error(`Presentation preferences were not saved: ${errorText(error)}`);
          return false;
        }
      }
      return false;
    })().finally(() => { this.saving = null; this.changed(); });
    this.saving = saving;
    return saving;
  }

  /** Where the data lives and whether it can be changed: the status strip says so. */
  async readStorage(): Promise<void> {
    try {
      const storage = await settingsApi.storageLocation();
      this.app.ui.setRuntime({ title: `Private data: ${storage.directory}\nDatabase: ${storage.database_path}` });
      if (storage.read_only) {
        this.app.ui.setRuntime({ text: 'Read-only storage · update required', readOnly: true });
        this.app.ui.error(storage.message || 'Update Shinatama to modify this database.');
      }
    } catch (error) {
      this.app.ui.setRuntime({ text: 'Storage unavailable', failed: true });
      this.app.ui.error(error);
    }
  }
}
