// The files of the project the window works in: browse, search, read, edit and save, in the inspector (a tree and the preview of the
// file). Rust reads a folder at a time and a search can be cancelled by its request id. Opening a file never runs it. A save writes only
// what the person asked for and is checked against the version that was read: a file that changed on disk since is not overwritten
// without asking.
import type { TreeNode } from '@acaprino/sigrid/assemblies';
import { errorText } from '../api/core.ts';
import { filesApi } from '../api/files.ts';
import { projectsApi } from '../api/projects.ts';
import type { Shinatama } from '../app.ts';
import {
  absolutePath, dirty, fileStatus, isImage, isMarkdown, isNote, isPage, listingNodes, searchText, entryNode,
  type Buffer, type ImageFile, type Listing, type SaveOutcome, type SearchResult, type TextFile,
} from '../files-model.ts';
import { Store } from '../store.ts';

let sequence = 0;
/** An id for a read or a search that can be cancelled. */
export const requestId = (): string => `files-${Date.now().toString(36)}-${++sequence}`;

const SEARCH_MS = 250;
const message = (error: unknown): string => errorText(error) || 'The file operation failed.';

export type FileProject = { path: string; name: string };
export type PreviewMode = 'source' | 'preview';

export class FilesController extends Store {
  /** The projects that can be browsed (the recent ones that are still there), and the one shown. */
  projects: FileProject[] = [];
  project: string | null = null;
  /** The first level of the tree of `project`: a new array is a tree read again (Sigrid keeps the children it loaded while it gets the same nodes). */
  nodes: TreeNode[] = [];
  /** The branches the person opened. */
  expanded: string[] = [];
  /** The file chosen in the tree, relative to the project. */
  selected: string | null = null;
  /** What is typed in the search by name, and the flat rows of its answer (null: no search shown). */
  query = '';
  results: TreeNode[] | null = null;
  /** The file that is open as text; or the picture that is open instead. */
  current: Buffer | null = null;
  image: Extract<ImageFile, { state: 'ready' }>['image'] | null = null;
  mode: PreviewMode = 'source';
  /** What the status line says, and whether it says something went wrong. */
  status = 'Files open for reading; nothing is run.';
  failed = false;
  saving = false;
  /** The project list or the first folder is being read. */
  loading = false;
  /** The first read failed, and why. */
  problem: string | null = null;

  private readonly buffers = new Map<string, Buffer>();
  private opening = 0;
  private pending: string | null = null;
  private searchTimer: ReturnType<typeof setTimeout> | undefined;
  private searchRevision = 0;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  dispose(): void {
    clearTimeout(this.searchTimer);
  }

  /** The folder new sessions start in, when the panel has not chosen another. */
  get preferred(): string | null {
    return this.app.projects.path || null;
  }

  /** Whether some file has changes that are not saved. */
  get dirty(): boolean {
    return [...this.buffers.values()].some(dirty);
  }

  get unsaved(): Buffer[] {
    return [...this.buffers.values()].filter(dirty);
  }

  /** Shows the files panel. */
  open(): void {
    this.app.ui.openPanel({ kind: 'files' });
  }

  // --- the projects and the tree ----------------------------------------------------------------------------------------------

  /** Reads the projects that can be browsed and the first level of the one to show. Called when the panel is drawn. */
  async start(): Promise<void> {
    if (this.project !== null && this.projects.length) return;
    this.loading = true; this.problem = null; this.changed();
    try {
      const usable = (await projectsApi.recent().catch(() => [])).filter(project => project.available);
      if (!usable.length) {
        this.projects = []; this.project = null; this.nodes = [];
        this.problem = 'Open a project first; only your projects can be browsed.';
        return;
      }
      this.projects = usable.map(project => ({ path: project.path, name: project.name || project.path }));
      const wanted = this.preferred?.replace(/[\\/]+$/, '').toLowerCase();
      this.project = (this.projects.find(project => project.path.replace(/[\\/]+$/, '').toLowerCase() === wanted) ?? this.projects[0]).path;
      await this.readRoot();
    } finally {
      this.loading = false;
      this.changed();
    }
  }

  private async readRoot(): Promise<void> {
    const { project } = this;
    this.nodes = []; this.expanded = []; this.selected = null;
    if (!project) return;
    try {
      const listing = await filesApi.list(project, '', requestId()) as Listing;
      if (this.project !== project) return;
      this.nodes = listingNodes(listing);
      this.problem = null;
      this.say('Files open for reading; nothing is run.');
    } catch (error) {
      if (this.project === project) { this.problem = message(error); this.say(this.problem, true); }
    }
    this.changed();
  }

  /** The rows of a folder, when the tree first opens it (Sigrid's `loadChildren`). A refusal leaves the folder closed and is said. */
  async children(node: TreeNode): Promise<TreeNode[]> {
    const { project } = this;
    if (!project) throw new Error('Choose a project first.');
    try {
      return listingNodes(await filesApi.list(project, node.id, requestId()) as Listing);
    } catch (error) {
      this.say(message(error), true);
      this.changed();
      throw error;
    }
  }

  /** The person chose another project: with unsaved changes they decide first. */
  async chooseProject(path: string): Promise<void> {
    if (path === this.project) return;
    if (this.dirty && !await this.confirmDiscard('Switching project discards unsaved changes in this panel.')) return;
    this.forgetAll();
    this.project = path;
    this.changed();
    await this.readRoot();
  }

  // --- the search by name ---------------------------------------------------------------------------------------------------------

  setQuery(query: string): void {
    this.query = query;
    const revision = ++this.searchRevision;
    clearTimeout(this.searchTimer);
    this.changed();
    this.searchTimer = setTimeout(() => { void this.runSearch(revision); }, SEARCH_MS);
  }

  private async runSearch(revision: number): Promise<void> {
    const wanted = this.query.trim();
    const { project } = this;
    await this.cancelPending();
    if (revision !== this.searchRevision) return;
    if (!wanted) { this.results = null; this.expanded = []; this.say(this.current ? fileStatus(this.current, false) : 'Files open for reading; nothing is run.'); this.changed(); return; }
    if (!project) return;
    const request = requestId();
    this.pending = request;
    this.say('Searching…'); this.changed();
    try {
      const result = await filesApi.search(project, wanted, request) as SearchResult;
      if (this.pending !== request || revision !== this.searchRevision) return;
      this.pending = null;
      // The tree is drawn again when the search ends: its folders are read again as they are opened, so none is open meanwhile.
      this.results = result.hits.map(hit => entryNode(hit, true));
      this.expanded = [];
      this.say(searchText(result));
    } catch (error) {
      if (this.pending === request && revision === this.searchRevision) { this.pending = null; this.say(message(error), true); }
    }
    this.changed();
  }

  private async cancelPending(): Promise<void> {
    if (!this.pending) return;
    const old = this.pending;
    this.pending = null;
    await filesApi.cancel(old).catch(() => {});
  }

  /** Empties the search and shows the tree again. */
  clearSearch(): void {
    ++this.searchRevision;
    clearTimeout(this.searchTimer);
    void this.cancelPending();
    this.query = ''; this.results = null; this.expanded = [];
    this.changed();
  }

  // --- the tree: choosing and opening --------------------------------------------------------------------------------------------

  setExpanded(expanded: string[]): void {
    this.expanded = expanded;
    this.changed();
  }

  /** A row of the tree was pressed. A file opens; a note says nothing; a folder is the tree's. */
  press(node: TreeNode): void {
    if (isNote(node.id) || node.hasChildren || node.children) { this.selected = node.id; this.changed(); return; }
    void this.openFile(node.id);
  }

  /** Reveals a file in the tree: its folders open, and the search is cleared. */
  reveal(path: string): void {
    this.clearSearch();
    this.selected = path;
    this.changed();
  }

  // --- the file ---------------------------------------------------------------------------------------------------------------------

  /** Opens a file as text (or a picture). Never runs it. A file that is not text is said, not opened. */
  async openFile(path: string): Promise<void> {
    const { project } = this;
    if (!project) { this.say('Choose a project first.', true); return; }
    const ticket = ++this.opening;
    this.image = null;
    try {
      if (isImage(path)) {
        const outcome = await filesApi.image(project, path) as ImageFile;
        if (ticket !== this.opening) return;
        this.current = null; this.selected = path;
        if (outcome.state === 'refused') { this.say(outcome.message, true); return; }
        this.image = outcome.image;
        this.say(`${outcome.image.width} × ${outcome.image.height} pixels, ${outcome.image.bytes.toLocaleString()} bytes. Read-only.`);
        return;
      }
      let buffer = this.buffers.get(path);
      let truncated = false;
      if (!buffer) {
        const file = await filesApi.read(project, path) as TextFile;
        if (ticket !== this.opening) return;
        if (file.binary || file.text === null) { this.say(`${file.path} is not text (${file.bytes.toLocaleString()} bytes); it is not opened or run.`); return; }
        buffer = { project, path: file.path, baseText: file.text, baseSha256: file.sha256, text: file.text, editable: !file.truncated };
        this.buffers.set(file.path, buffer);
        truncated = file.truncated;
      }
      this.current = buffer; this.selected = buffer.path;
      this.mode = isMarkdown(buffer.path) ? this.mode : 'source';
      this.say(fileStatus(buffer, truncated));
    } catch (error) {
      if (ticket === this.opening) this.say(message(error), true);
    } finally {
      this.changed();
    }
  }

  /** The person typed: the buffer holds the text; nothing is written until Save. */
  edit(text: string): void {
    const buffer = this.current;
    if (!buffer?.editable || buffer.text === text) return;
    buffer.text = text;
    this.changed();
  }

  setMode(mode: PreviewMode): void {
    this.mode = mode;
    this.changed();
  }

  /** Opens a local page in a browser tab, with its own folder only. */
  openInBrowser(): void {
    const { current, project } = this;
    if (!current || !project || !isPage(current.path)) return;
    void this.app.browser.open({ file: absolutePath(project, current.path) });
  }

  get canSave(): boolean {
    return !!this.current?.editable && dirty(this.current) && !this.saving;
  }

  /** Writes the file, checked against the version that was read. A file that changed on disk since asks before it is replaced. */
  async save(): Promise<void> {
    const buffer = this.current;
    if (!buffer?.editable || this.saving) return;
    this.saving = true; this.changed();
    const report = (text: string, error = false) => { if (this.current === buffer) this.say(text, error); };
    try {
      if (this.buffers.get(buffer.path) !== buffer || buffer.text === buffer.baseText) return;
      const request = { project: buffer.project, path: buffer.path, text: buffer.text, baseSha256: buffer.baseSha256, overwrite: false };
      let outcome = await filesApi.save(request) as SaveOutcome;
      if (outcome.state === 'conflict') {
        if (this.buffers.get(buffer.path) !== buffer) return;
        const replace = await this.app.ui.ask({
          title: 'The file changed on disk',
          description: `${buffer.path} was changed by someone else (an agent?) after you opened it. Overwrite it with your version? Cancel keeps your text here unsaved.`,
          confirm: `Overwrite ${buffer.path}`,
        });
        if (!replace) { report('Not saved: the file on disk is newer. Your text is still here.', true); return; }
        if (this.buffers.get(buffer.path) !== buffer) return;
        if (buffer.text !== request.text) { report('The draft changed during confirmation. Save again to write it.'); return; }
        // Authorize replacement of the reviewed disk version only.
        outcome = await filesApi.save({ ...request, baseSha256: outcome.disk_sha256 }) as SaveOutcome;
      }
      if (outcome.state === 'saved') {
        buffer.baseSha256 = outcome.sha256;
        buffer.baseText = request.text;
        report(dirty(buffer) ? 'Saved. Newer edits are still unsaved.' : 'Saved.');
      } else report(outcome.state === 'failed' ? outcome.message : 'The file changed again. Save to review it.', true);
    } catch (error) {
      report(message(error), true);
    } finally {
      this.saving = false;
      this.changed();
    }
  }

  // --- leaving ---------------------------------------------------------------------------------------------------------------------

  private confirmDiscard(consequence: string): Promise<boolean> {
    const names = this.unsaved.map(buffer => buffer.path).join(', ');
    return this.app.ui.ask({ title: 'Discard unsaved changes?', description: `${names ? `${names}: ` : ''}${consequence}`, confirm: 'Discard changes' });
  }

  /** The panel closes: false when the person keeps the unsaved changes. Everything the panel holds goes when it closes (buffers, the
   *  tree, the search). */
  async release(): Promise<boolean> {
    if (this.dirty && !await this.confirmDiscard('Closing discards them; Cancel keeps the files open.')) return false;
    this.clearSearch();
    this.forgetAll();
    this.project = null; this.projects = []; this.nodes = [];
    this.changed();
    return true;
  }

  private forgetAll(): void {
    this.opening++;
    this.buffers.clear();
    this.current = null; this.image = null; this.selected = null; this.expanded = []; this.mode = 'source';
    this.say('Files open for reading; nothing is run.');
  }

  private say(text: string, failed = false): void {
    this.status = text;
    this.failed = failed;
  }
}
