// What the person sets for the window, as Rust keeps it: the terminal presentation (font, line height, whether agents draw with the
// terminal's palette), and where the data lives. The look (scheme, glass, zoom) is the shell's (`useGlass`, `useWindow`), never kept here.
// The other settings (alerts, models, memory, Muro, the proxy of the models) are `services.ts`.
import { errorCode, errorText, sleep } from '../api/core.ts';
import { sessionsApi } from '../api/sessions.ts';
import { settingsApi, type Presentation } from '../api/settings.ts';
import type { Shinatama } from '../app.ts';
import { parseCheckup, PROVIDERS, type Checkup } from '../agent-checkup.ts';
import {
  addConnection, addModel, buildModelSave, emptyForm, fromView, parseModelView, PRESETS, removeConnection, removeModel, renameModel, useConnection,
  type Edit, type Form as ModelForm, type Harness, type Selection,
} from '../model-settings-model.ts';
import {
  buildMemory, buildSave, DEFAULT_MEMORY, formFromView, memorySummary, parseMemory, parseView, summary, MAX_VARIABLES,
  type Form as LaunchForm, type Row,
} from '../settings-model.ts';
import { Store } from '../store.ts';

export const DEFAULT_PRESENTATION: Presentation = {
  version: 1, skin: 'shinatama', followSkin: false, fontFamily: 'profile', fontSize: 13, lineHeight: 115,
};

/** How long a change waits for the next before it is saved. */
const SAVE_DELAY_MS = 400;

/** The fonts the terminal can use, and the ranges of its size and line height (what Rust accepts). */
export const TERMINAL_FONTS = ['profile', 'Cascadia Mono', 'Cascadia Code', 'Consolas', 'monospace'] as const;
export const FONT_SIZE = { min: 8, max: 32 } as const;
export const LINE_HEIGHT = { min: 100, max: 180 } as const;
/** The sections of the Settings page, in order: the id, the name in the index and the heading. */
export const SECTIONS = [
  ['appearance', 'Appearance'], ['agents', 'Agents'], ['models', 'Models & API keys'], ['memory', 'Memory'], ['alerts', 'Alerts'],
  ['github', 'GitHub'], ['variables', 'Environment variables'], ['sessions', 'Sessions'], ['plugins', 'Plugins'], ['proxy', 'Model proxy'],
] as const;
export type SectionId = typeof SECTIONS[number][0];
export type SaidLine = { text: string; error: boolean };
const emptyLaunch = (): LaunchForm => ({ fullAccess: true, githubUsername: '', tokenInput: '', removeToken: false, hasToken: false, rows: [] });

export class SettingsController extends Store {
  presentation: Presentation = DEFAULT_PRESENTATION;
  /** Whether the saved preferences were read (until then they are the defaults, and nothing is saved). */
  loaded = false;
  /** A change that is not saved yet, or a save that failed. */
  dirty = false;
  // --- the page: what waits for Save (agents, models, memory, GitHub, variables) and what applies at once (the sessions' switches) ---
  launch: LaunchForm = emptyLaunch();
  models: ModelForm = emptyForm();
  memoryEnabled = DEFAULT_MEMORY.enabled;
  memoryMargin = String(DEFAULT_MEMORY.marginMib);
  /** Whether each part was read: Save is offered only when the models and the launch settings were. */
  launchLoaded = false;
  modelsLoaded = false;
  /** The page is reading or saving. */
  busy = false;
  /** The line of the actions row: what the page is doing, and what was said last. */
  line: SaidLine = { text: '', error: false };
  /** The switches that apply at once: null while unread. */
  titles: boolean | null = null;
  indexing: boolean | null = null;
  switchesBusy = false;
  switchesLine: SaidLine = { text: '', error: false };
  /** The checkup of each agent, and the agent being checked. */
  readonly checkups = new Map<string, Checkup | { error: string }>();
  checking: string | null = null;
  /** The connections whose card is open, and the model options the next new connection starts from. */
  readonly open = new Set<string>();
  newPreset = PRESETS.findIndex(preset => preset.name === 'Custom endpoint');
  /** The section the index marks. */
  section: SectionId = 'appearance';
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
    this.timer = setTimeout(() => { void this.savePresentation(); }, SAVE_DELAY_MS);
  }

  /** Saves what is pending. Resolves to whether everything is saved (the window's close asks before it loses a change). */
  flush(): Promise<boolean> {
    clearTimeout(this.timer);
    return this.dirty || this.saving ? this.savePresentation() : Promise.resolve(true);
  }

  private savePresentation(): Promise<boolean> {
    if (this.saving) return this.saving.then(() => this.savePresentation());
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

  // --- the terminal ----------------------------------------------------------------------------------------

  /** The font the terminals ask Sigrid's `Terminal` for: `profile` (the default) asks for none, so the toolkit's own monospace stays. */
  static terminalFont(family: string): string | undefined {
    return family === 'profile' || family.trim() === '' ? undefined : family;
  }

  /** The size is typed: a whole number in range changes it; anything else leaves it as it was (the field says why). */
  static sizeProblem(text: string): string {
    const n = Number(text);
    return Number.isInteger(n) && n >= FONT_SIZE.min && n <= FONT_SIZE.max ? '' : `Use a whole number from ${FONT_SIZE.min} to ${FONT_SIZE.max}.`;
  }

  static heightProblem(text: string): string {
    const n = Number(text);
    return Number.isInteger(n) && n >= LINE_HEIGHT.min && n <= LINE_HEIGHT.max ? '' : `Use a whole number from ${LINE_HEIGHT.min} to ${LINE_HEIGHT.max}.`;
  }

  setFontSize(text: string): void {
    if (!SettingsController.sizeProblem(text)) this.setPresentation({ fontSize: Number(text) });
  }

  setLineHeight(text: string): void {
    if (!SettingsController.heightProblem(text)) this.setPresentation({ lineHeight: Number(text) });
  }

  // --- the page ------------------------------------------------------------------------------------------------

  say(text: string, error = false): void {
    this.line = { text, error };
    this.changed();
  }

  get startsWith(): string {
    return `${summary(this.launch)} · ${memorySummary({ enabled: this.memoryEnabled, marginMib: Number(this.memoryMargin) || 0 })}`;
  }

  /** Settings became the shown view: every setting is read again. */
  enter(): void {
    void this.reload();
  }

  /** Settings is no longer the shown view: what was typed in a secret field goes. */
  leave(): void {
    this.launch.tokenInput = '';
    for (const edit of this.models.connections) { edit.keyInput = ''; edit.removeKey = false; }
    this.app.services.stopWatching();
    this.changed();
  }

  /** Reads every setting again (Reload, and the page opening). */
  async reload(): Promise<void> {
    const { services } = this.app;
    void this.readSwitches();
    void services.refreshMuro();
    services.startWatching();
    if (this.busy) return;
    this.busy = true;
    this.say('Loading…');
    let memoryProblem = '';
    await services.refreshMemory();
    if (services.problems.has('memory')) {
      this.memoryEnabled = DEFAULT_MEMORY.enabled;
      this.memoryMargin = String(DEFAULT_MEMORY.marginMib);
      memoryProblem = ` ${services.problems.get('memory')} Memory safety stays on until you save.`;
    } else {
      this.memoryEnabled = services.memory.enabled;
      this.memoryMargin = String(services.memory.marginMib);
    }
    this.modelsLoaded = false;
    try {
      const view = parseModelView(await settingsApi.modelSettingsLoad());
      if (!view) throw new Error('The shared model settings could not be read. Reload to try again.');
      this.models = fromView(view);
      this.open.clear();
      this.modelsLoaded = true;
      const saved = parseView(await settingsApi.launchSettingsLoad());
      if (!saved) throw new Error('The saved settings are not in a form this version understands.');
      this.launch = formFromView(saved);
      this.launchLoaded = true;
      this.say(`New sessions start with: ${this.startsWith}.${memoryProblem}`, !!memoryProblem);
    } catch (error) {
      this.launchLoaded = false;
      this.say(`${errorText(error)} ${this.modelsLoaded ? 'Agents start with their own protections until you save.' : 'Reload before saving.'}`, true);
    } finally { this.busy = false; this.changed(); }
  }

  /** Whether Save can run, and why not. */
  get saveBlock(): string {
    if (this.busy) return 'The settings are loading or saving.';
    if (!this.modelsLoaded) return 'Reload before saving.';
    return '';
  }

  /** Saves what waits for Save: the models, the launch settings, then the memory. The first problem stops it and says what is wrong. */
  async save(): Promise<boolean> {
    if (this.saveBlock) return false;
    const built = buildSave(this.launch);
    if ('problem' in built) { this.say(built.problem, true); return false; }
    const memory = buildMemory(this.memoryEnabled, this.memoryMargin);
    if ('problem' in memory) { this.say(memory.problem, true); return false; }
    const models = buildModelSave(this.models);
    if ('problem' in models) { this.say(models.problem, true); return false; }
    this.busy = true;
    this.say('Saving…');
    let modelsSaved = false;
    try {
      const view = parseModelView(await settingsApi.modelSettingsSave(models.request));
      if (!view) throw new Error('The shared model settings could not be confirmed. Reload before saving again.');
      this.models = fromView(view);
      modelsSaved = true;
      const launch = parseView(await settingsApi.launchSettingsSave(built.request));
      if (!launch) throw new Error('The saved settings could not be confirmed.');
      this.launch = formFromView(launch);
      this.launchLoaded = true;
      const saved = parseMemory(await settingsApi.memorySettingsSave(memory.request));
      if (!saved) throw new Error('The memory settings could not be confirmed.');
      this.memoryEnabled = saved.enabled;
      this.memoryMargin = String(saved.marginMib);
      await this.app.services.refreshMemory();
      this.say(`Saved. Shared models apply to new sessions. New sessions start with: ${this.startsWith}.`);
      this.app.ui.info('Settings saved.');
      return true;
    } catch (error) {
      const problem = `${modelsSaved ? 'Models were saved; the remaining settings were not all saved. ' : ''}${errorText(error)}`;
      this.say(problem, true);
      this.app.ui.error(problem);
      return false;
    } finally { this.busy = false; this.changed(); }
  }

  private edited(what: string): void {
    this.line = { text: `Not saved yet: ${what}.`, error: false };
    this.changed();
  }

  // --- agents --------------------------------------------------------------------------------------------------

  async checkAgent(agentId: string): Promise<void> {
    const provider = PROVIDERS.find(one => one.agentId === agentId && one.checkable);
    if (!provider || this.checking) return;
    this.checking = agentId;
    this.changed();
    try {
      const checkup = parseCheckup(await settingsApi.agentCheck(agentId), provider.agentId);
      if (!checkup) throw new Error('Invalid checkup');
      this.checkups.set(agentId, checkup);
    } catch { this.checkups.set(agentId, { error: 'Agent check did not complete. Retry explicitly.' }); }
    finally { this.checking = null; this.changed(); }
  }

  setFullAccess(on: boolean): void {
    this.launch.fullAccess = on;
    this.edited(summary(this.launch));
  }

  // --- memory --------------------------------------------------------------------------------------------------

  setMemoryEnabled(on: boolean): void {
    this.memoryEnabled = on;
    this.edited(memorySummary({ enabled: on, marginMib: Number(this.memoryMargin) || 0 }));
  }

  setMemoryMargin(text: string): void {
    this.memoryMargin = text;
    this.edited(memorySummary({ enabled: this.memoryEnabled, marginMib: Number(text) || 0 }));
  }

  /** What is wrong with the margin as typed, in a sentence; empty when it is fine. */
  get marginProblem(): string {
    const built = buildMemory(this.memoryEnabled, this.memoryMargin);
    return 'problem' in built ? built.problem : '';
  }

  // --- GitHub and variables ----------------------------------------------------------------------------------

  setGithubUser(text: string): void {
    this.launch.githubUsername = text;
    this.changed();
  }

  setToken(text: string): void {
    this.launch.tokenInput = text;
    this.changed();
  }

  removeToken(): void {
    this.launch.removeToken = true;
    this.launch.tokenInput = '';
    this.say('The token will be removed when you save.');
  }

  addVariable(): void {
    if (this.launch.rows.length >= MAX_VARIABLES) return;
    this.launch.rows.push({ name: '', secret: false, value: '', hasValue: false, touched: true });
    this.changed();
  }

  setVariable(index: number, change: Partial<Pick<Row, 'name' | 'value' | 'secret'>>): void {
    const row = this.launch.rows[index];
    if (!row) return;
    Object.assign(row, change);
    if ('value' in change) row.touched = true;
    this.changed();
  }

  removeVariable(index: number): void {
    this.launch.rows.splice(index, 1);
    this.changed();
  }

  // --- models --------------------------------------------------------------------------------------------------

  private modelsEdited(): void {
    this.edited('model changes wait for Save to apply to new sessions');
  }

  connection(id: string): Edit | undefined {
    return this.models.connections.find(edit => edit.connection.id === id);
  }

  markSection(id: SectionId): void {
    if (this.section === id) return;
    this.section = id;
    this.changed();
  }

  setNewPreset(index: number): void {
    this.newPreset = index;
    this.changed();
  }

  /** A new connection from the preset chosen; its card is open. */
  addConnection(): void {
    const preset = PRESETS[this.newPreset] ?? PRESETS[0];
    const edit = addConnection(this.models, preset, crypto.randomUUID());
    if (!edit) return;
    this.open.add(edit.connection.id);
    this.modelsEdited();
  }

  removeConnection(id: string): void {
    removeConnection(this.models, id);
    this.open.delete(id);
    this.modelsEdited();
  }

  setConnectionOpen(id: string, open: boolean): void {
    if (open) this.open.add(id); else this.open.delete(id);
    this.changed();
  }

  /** The connection's name, protocol, endpoint or key is typed. */
  editConnection(id: string, change: { name?: string; api?: Edit['connection']['api']; baseUrl?: string; keyInput?: string }): void {
    const edit = this.connection(id);
    if (!edit) return;
    if (change.name !== undefined) edit.connection.name = change.name;
    if (change.api !== undefined) edit.connection.api = change.api;
    if (change.baseUrl !== undefined) edit.connection.baseUrl = change.baseUrl;
    if (change.keyInput !== undefined) edit.keyInput = change.keyInput;
    this.modelsEdited();
  }

  removeKey(id: string): void {
    const edit = this.connection(id);
    if (!edit) return;
    edit.removeKey = true;
    edit.keyInput = '';
    this.modelsEdited();
  }

  addModel(id: string): void {
    const edit = this.connection(id);
    if (edit && addModel(edit.connection)) this.modelsEdited();
  }

  removeModel(id: string, index: number): void {
    const edit = this.connection(id);
    if (edit && removeModel(edit.connection, index)) this.modelsEdited();
  }

  editModel(id: string, index: number, change: { id?: string; contextWindow?: number; maxTokens?: number; reasoning?: boolean; vision?: boolean }): void {
    const edit = this.connection(id);
    const model = edit?.connection.models[index];
    if (!edit || !model) return;
    if (change.id !== undefined) renameModel(this.models, edit.connection, index, change.id);
    if (change.contextWindow !== undefined) model.contextWindow = change.contextWindow;
    if (change.maxTokens !== undefined) model.maxTokens = change.maxTokens;
    if (change.reasoning !== undefined) model.reasoning = change.reasoning;
    if (change.vision !== undefined) model.vision = change.vision;
    this.modelsEdited();
  }

  assign(harness: Harness, selection: Selection | null): void {
    this.models.assignments[harness] = selection;
    this.modelsEdited();
  }

  /** The one-click assignment of a connection that no agent uses. */
  useFor(id: string, harness: Harness): void {
    const edit = this.connection(id);
    const modelId = edit ? useConnection(this.models, edit.connection, harness) : null;
    if (modelId) this.edited(`${harness} will start with ${modelId}; Save applies it to new sessions`);
  }

  // --- the switches that apply at once -------------------------------------------------------------------------

  async readSwitches(): Promise<void> {
    this.switchesBusy = true;
    this.changed();
    const [titles, index] = await Promise.allSettled([settingsApi.titlesEnabled(), sessionsApi.indexStatus()]);
    this.titles = titles.status === 'fulfilled' && typeof titles.value === 'boolean' ? titles.value : null;
    const status = index.status === 'fulfilled' ? index.value as { index?: { enabled?: unknown } } | null : null;
    this.indexing = typeof status?.index?.enabled === 'boolean' ? status.index.enabled : null;
    this.switchesBusy = false;
    this.switchesLine = this.titles === null || this.indexing === null
      ? { text: 'Part of this section could not be read. Reload to try again.', error: true } : { text: '', error: false };
    this.changed();
  }

  async setTitles(on: boolean): Promise<void> {
    await this.applySwitch(() => settingsApi.titlesSetEnabled(on), () => { this.titles = on; }, on ? 'Activity titles on.' : 'Activity titles off.');
  }

  async setIndexing(on: boolean): Promise<void> {
    await this.applySwitch(() => sessionsApi.indexEnable(on), () => { this.indexing = on; }, on ? 'Indexing allowed.' : 'Indexing off.');
  }

  private async applySwitch(call: () => Promise<unknown>, keep: () => void, said: string): Promise<void> {
    if (this.switchesBusy) return;
    this.switchesBusy = true;
    this.changed();
    try { await call(); keep(); this.switchesLine = { text: said, error: false }; }
    catch (error) { this.switchesLine = { text: errorText(error), error: true }; }
    finally { this.switchesBusy = false; this.changed(); }
  }
}
