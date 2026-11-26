// The folders that can be projects: the picker (the recent ones, the ones found under the roots that are scanned, the roots themselves) and
// the creation of a new project in one of those roots. Both are folder operations Rust can cancel: they share one runner (project-mutation.ts)
// and the dialog that is open is the one a close or a Cancel asks about. The dialogs (screens/dialogs) draw this and call its methods.
import { errorText } from '../api/core.ts';
import { projectsApi, type ProjectEntry } from '../api/projects.ts';
import type { Shinatama } from '../app.ts';
import { MutationRunner, type MutationPhase } from '../project-mutation.ts';
import {
  beginMutation, browseFolder, createCatalogClient, createRequestId, orderedCandidates, saveRoots,
  type Candidate, type Catalog, type RootConfig, type RootDraft,
} from '../project-catalog.ts';
import { Store } from '../store.ts';

const absolute = (path: string): boolean => /^(?:[A-Za-z]:[\\/]|\/|\\\\)/.test(path);
const sameFolder = (a: string, b: string): boolean => a.replace(/[\\/]+$/, '').toLowerCase() === b.replace(/[\\/]+$/, '').toLowerCase();
export const MAX_ROOTS = 16;

/** What a project name may not be: empty, a path, a name Windows reserves. Empty when fine. */
export function nameProblem(name: string): string {
  if (!name.trim()) return 'Enter a project folder name.';
  if (name === '.' || name === '..' || /[<>:"/\\|?*\u0000-\u001f]/.test(name)) return 'Use one folder name without path separators or Windows reserved characters.';
  if (/[. ]$/.test(name)) return 'The folder name cannot end with a dot or space.';
  if (/^(?:CON|PRN|AUX|NUL|CONIN\$|CONOUT\$|COM[1-9¹²³]|LPT[1-9¹²³])(?:\.|$)/i.test(name)) return 'Choose a name that Windows does not reserve for a device.';
  return '';
}

export type CreationOptions = { revision: number; roots: { id: string; path: string }[]; defaultRootId: string | null };
type CreationFailure = { message: string; recoveryPath?: string };

function failureOf(value: unknown): CreationFailure {
  if (typeof value === 'object' && value !== null && 'message' in value && typeof value.message === 'string') {
    return { message: value.message, ...('recoveryPath' in value && typeof value.recoveryPath === 'string' ? { recoveryPath: value.recoveryPath } : {}) };
  }
  return { message: errorText(value) };
}
export const failureText = (failure: CreationFailure): string => failure.message + (failure.recoveryPath ? ` Preserved work: ${failure.recoveryPath}. Review this folder before trying again.` : '');

export class CatalogController extends Store {
  // --- the picker -------------------------------------------------------------------------------------------
  catalog: Catalog | null = null;
  config: RootConfig | null = null;
  /** The recent folders when no root is the default (the catalogue holds them when one is). */
  recent: ProjectEntry[] = [];
  filter = '';
  loading = false;
  configLoading = true;
  queryPending = false;
  /** False from opening until the roots are read again: opening a folder elsewhere moves their revision. */
  configFresh = false;
  error = '';
  // --- a new project ------------------------------------------------------------------------------------------
  creation: CreationOptions | null = null;
  creationLoading = false;
  rootId = '';
  name = '';
  git = false;
  nameShown = false;

  private readonly app: Shinatama;
  private readonly runner: MutationRunner;
  private readonly client = createCatalogClient(value => this.onSnapshot(value), failure => this.onFailure(failure));
  private pageOffsets: number[] = [];
  private pickerOpen = false;
  private optionsRead: Promise<CreationOptions> | null = null;
  private generation = 0;

  constructor(app: Shinatama) {
    super();
    this.app = app;
    this.runner = new MutationRunner({
      changed: () => this.changed(),
      describe: failure => failureText(failureOf(failure)),
      isOpen: () => this.app.ui.dialog?.kind === 'project-picker' || this.app.ui.dialog?.kind === 'new-project',
      close: () => this.app.ui.closeDialog(),
      fail: (text, open) => { if (open) { this.error = text; this.changed(); } else this.app.ui.error(`Folder operation did not complete: ${text}`); },
      say: (text, error) => { if (error) this.app.ui.error(text); else this.app.ui.info(text); },
    });
  }

  get saving(): boolean { return this.runner.busy; }
  get phase(): MutationPhase | null { return this.runner.phase; }

  /** What the dialog says about the operation under way, or about the scan. */
  get state(): string {
    switch (this.runner.phase) {
      case 'committing': return 'The save has started. Closing this window keeps it running.';
      case 'cancelled': return 'The previous folder check is still returning. Existing terminals remain available.';
      case 'cancelling': return 'Cancelling the folder operation…';
      case 'pending': return 'Checking folder paths…';
      default: break;
    }
    if (this.loading || this.configLoading) return 'Reading project roots…';
    if (this.queryPending) return 'Filtering projects…';
    if (this.catalog?.scanning) return 'Scanning roots. You can keep filtering or close this window.';
    return this.catalog?.roots.length ? `${this.catalog.total} matching projects` : '';
  }

  private onSnapshot(value: Catalog): void {
    this.catalog = value;
    this.configLoading = value.configLoading;
    // A scan that started before the last save carries the older roots; never step back to them.
    if (!this.configLoading && (!this.config || value.rootsConfig.revision >= this.config.revision)) this.config = value.rootsConfig;
    if (!this.configLoading) this.configFresh = true;
    this.loading = false;
    this.queryPending = false;
    this.changed();
  }

  private onFailure(failure: unknown): void {
    this.loading = false;
    this.queryPending = false;
    if (this.pickerOpen) this.error = errorText(failure);
    this.changed();
  }

  // --- the picker ---------------------------------------------------------------------------------------------

  /** The picker is on screen: the catalogue is read (and scanned), and the recent folders when no root is the default. */
  openPicker(): void {
    this.pickerOpen = true;
    this.configFresh = false;
    this.filter = '';
    this.error = '';
    this.refresh();
  }

  /** The picker is gone: its scan is cancelled. */
  closePicker(): void {
    this.pickerOpen = false;
    this.client.close();
  }

  refresh(): void {
    if (!this.pickerOpen) return;
    this.pageOffsets = [];
    this.loading = true;
    this.configLoading = true;
    this.changed();
    this.client.open(this.filter);
    if (!this.config?.defaultRootId) void this.readRecent();
  }

  private async readRecent(): Promise<void> {
    try { this.recent = await projectsApi.recent(); this.changed(); }
    catch (failure) { if (this.pickerOpen) { this.error = errorText(failure); this.changed(); } }
  }

  setFilter(text: string): void {
    this.filter = text;
    this.pageOffsets = [];
    this.queryPending = true;
    this.changed();
    this.client.query(text, 0);
  }

  get canPrevious(): boolean { return !!this.catalog && this.catalog.offset > 0 && !this.queryPending && !this.loading && !this.saving; }
  get canNext(): boolean { return !!this.catalog && this.catalog.offset + this.catalog.limit < this.catalog.total && !this.queryPending && !this.loading && !this.saving; }

  previousPage(): void {
    if (!this.catalog || !this.canPrevious) return;
    const offset = this.pageOffsets.pop() ?? Math.max(0, this.catalog.offset - this.catalog.limit);
    this.queryPending = true;
    this.changed();
    this.client.query(this.filter, offset);
  }

  nextPage(): void {
    if (!this.catalog || !this.canNext) return;
    this.pageOffsets.push(this.catalog.offset);
    this.queryPending = true;
    this.changed();
    this.client.query(this.filter, this.catalog.offset + this.catalog.limit);
  }

  /** The folders to list: the single ones outside the roots (or the recent ones), and the candidates under the roots; both filtered. */
  get shown(): { history: { path: string; name: string; available: boolean }[]; projects: Candidate[]; historyHeading: string } {
    const query = this.filter.trim().toLocaleLowerCase();
    const inQuery = (project: { name: string; path: string }) => `${project.name} ${project.path}`.toLocaleLowerCase().includes(query);
    const rooted = !!this.config?.defaultRootId;
    const history = (rooted ? this.catalog?.history ?? [] : this.recent).filter(inQuery).slice(0, 10);
    const projects = this.catalog && !this.queryPending ? orderedCandidates(this.catalog).filter(inQuery) : [];
    return { history, projects, historyHeading: rooted ? 'Single folders outside your roots' : 'Recent folders' };
  }

  /** A folder is chosen as the project: Rust checks it (a folder operation that can be cancelled), then it is the project of the window. */
  async select(path: string): Promise<void> {
    if (this.saving) return;
    if (!absolute(path)) { this.error = 'Enter an absolute folder path.'; this.changed(); return; }
    this.error = '';
    await this.runner.run(requestId => projectsApi.select(path, requestId), project => this.useProject(project));
  }

  private useProject(project: ProjectEntry): void {
    this.app.projects.adopt(project);
    this.app.ui.closeDialog();
  }

  /** Opens the native picker for a project folder, and takes what the backend validated. */
  async chooseProject(): Promise<void> {
    if (this.saving) return;
    this.error = '';
    await this.runner.run(requestId => browseFolder('project', this.config?.defaultRootId ?? undefined, requestId), result => {
      if (result.kind === 'project') this.useProject(result.project);
    });
  }

  /** The native folder picker for a project, for a dialog that only wants the path (a new session). Null when cancelled; says what went wrong. */
  async browse(): Promise<string | null> {
    const id = createRequestId();
    try {
      await beginMutation(id);
      const result = await browseFolder('project', this.config?.defaultRootId ?? undefined, id);
      return result.kind === 'project' ? result.project.path : null;
    } catch (failure) { this.app.ui.error(errorText(failure)); return null; }
  }

  /** Opens the native picker for a root, and adds it. */
  async chooseRoot(): Promise<void> {
    if (this.saving || !this.config) return;
    this.error = '';
    let picked: string | null = null;
    await this.runner.run(requestId => browseFolder('root', this.config?.defaultRootId ?? undefined, requestId), result => { if (result.kind === 'root') picked = result.path; });
    if (picked) await this.addRoot(picked);
  }

  /** Root edits are saved at once: there is no draft to keep, discard or lose. */
  private async writeRoots(roots: RootDraft[], defaultRootIndex: number | null): Promise<boolean> {
    // The saved configuration is current even while the list is being rescanned.
    if (this.saving || !this.config || !this.configFresh) return false;
    const revision = this.config.revision;
    let saved = false;
    await this.runner.run(requestId => saveRoots(revision, roots, defaultRootIndex, requestId), value => {
      this.config = value;
      this.configFresh = true;
      saved = true;
      if (this.app.ui.dialog?.kind !== 'project-picker') this.app.ui.info('Project roots saved.');
      this.refresh();
    });
    return saved;
  }

  async addRoot(path: string): Promise<boolean> {
    if (!this.config) return false;
    const trimmed = path.trim();
    if (!absolute(trimmed)) { this.error = 'Enter the absolute path of the folder that holds your projects.'; this.changed(); return false; }
    const roots: RootDraft[] = this.config.roots.map(root => ({ ...root }));
    if (roots.some(root => sameFolder(root.path, trimmed))) { this.error = 'That root is already in the list.'; this.changed(); return false; }
    if (roots.length >= MAX_ROOTS) { this.error = `Keep at most ${MAX_ROOTS} project roots.`; this.changed(); return false; }
    const current = roots.findIndex(root => root.id === this.config!.defaultRootId);
    // The first root, or a root added while none is the default, becomes the default.
    this.error = '';
    return this.writeRoots([...roots, { path: trimmed }], current >= 0 ? current : roots.length);
  }

  makeDefault(index: number): Promise<boolean> {
    if (!this.config) return Promise.resolve(false);
    return this.writeRoots(this.config.roots.map(entry => ({ ...entry })), index);
  }

  /** Removing a root only stops listing it (the folder is not touched), so it happens at once and its notice offers Undo, which lists it
   *  again where it was. The store gives the root a new id: the old one no longer exists. */
  async removeRoot(index: number): Promise<void> {
    if (!this.config) return;
    const before: RootDraft[] = this.config.roots.map(root => ({ ...root }));
    const removed = before[index];
    if (!removed) return;
    const wasDefault = before.findIndex(root => root.id === this.config!.defaultRootId);
    const roots = before.filter((_, position) => position !== index);
    const kept = roots.findIndex(root => root.id === this.config!.defaultRootId);
    if (!await this.writeRoots(roots, kept >= 0 ? kept : roots.length ? 0 : null)) return;
    const restored = before.map((root, position) => position === index ? { path: root.path } : root);
    this.app.ui.report(`${removed.path} is no longer a project root. The folder is not touched.`, { undo: () => { void this.writeRoots(restored, wasDefault >= 0 ? wasDefault : null); } });
  }

  /** The close, or Cancel, of the picker or the creation: closes, or asks Rust to cancel what runs. */
  cancelOrClose(): Promise<void> {
    return this.runner.cancelOrClose();
  }

  // --- a new project --------------------------------------------------------------------------------------------

  /** The creation dialog is on screen: the roots are read (one read at a time, however often it is opened). */
  async openCreation(): Promise<void> {
    const mine = ++this.generation;
    this.creation = null;
    this.name = '';
    this.git = false;
    this.nameShown = false;
    this.error = '';
    this.creationLoading = true;
    this.changed();
    await this.loadOptions(mine);
  }

  async loadOptions(mine: number = this.generation): Promise<void> {
    if (this.saving) return;
    this.creationLoading = true;
    this.error = '';
    this.changed();
    try {
      if (!this.optionsRead) {
        const pending = projectsApi.newProjectOptions().then(value => value as CreationOptions).finally(() => { if (this.optionsRead === pending) this.optionsRead = null; });
        this.optionsRead = pending;
      }
      const received = await this.optionsRead;
      if (mine !== this.generation) return;
      this.creation = received;
      this.rootId = received.roots.some(entry => entry.id === received.defaultRootId) ? received.defaultRootId! : '';
    } catch (failure) {
      if (mine === this.generation) { this.creation = null; this.error = failureText(failureOf(failure)); }
    } finally { if (mine === this.generation) { this.creationLoading = false; this.changed(); } }
  }

  closeCreation(): void {
    this.generation += 1;
    this.creationLoading = false;
  }

  setRoot(id: string): void { this.rootId = id; this.error = ''; this.changed(); }
  setName(text: string): void { this.name = text; this.nameShown = true; this.error = ''; this.changed(); }
  setGit(on: boolean): void { this.git = on; this.changed(); }

  get nameError(): string { return this.nameShown ? nameProblem(this.name) : ''; }

  /** Where the folder will be made. */
  get destination(): string {
    const root = this.creation?.roots.find(entry => entry.id === this.rootId);
    if (!root) return '';
    return `${root.path}${/[\\/]$/.test(root.path) ? '' : '\\'}${this.name || 'project-name'}`;
  }

  get canCreate(): boolean {
    return !this.saving && !this.creationLoading && !!this.creation && !!this.rootId && !nameProblem(this.name);
  }

  /** Creates the project in the root chosen. The selection changes only after the backend returns its committed project. */
  async create(): Promise<void> {
    this.nameShown = true;
    if (!this.canCreate || !this.creation) { this.changed(); return; }
    // Typed identifiers and the exact input are sent; a path is never read back from display text.
    const request = { expectedRevision: this.creation.revision, rootId: this.rootId, name: this.name, gitInit: this.git };
    this.error = '';
    await this.runner.run(requestId => projectsApi.newProjectCreate(request, requestId) as Promise<ProjectEntry>, project => {
      const open = this.app.ui.dialog?.kind === 'new-project';
      if (open) this.app.ui.closeDialog();
      try { this.app.projects.adopt(project); }
      catch { this.app.ui.info(`Project created: ${project.path}. Refresh the project picker to open it.`); return; }
      if (!open) this.app.ui.info(`Project created: ${project.name}.`);
    });
  }
}
