// Every session Shinatama can read, in every store: the library the Sessions view lists (the ones that run now and the ones saved in
// the agents' archives), the text search and its index, the archives the person added, and the details of one session. A session the
// window runs is the terminals' and the threads'; these are the ones on disk, read by a worker of their own that refuses a read when it
// is busy. The conversation of one session, and what is done with it (search in it, a raw entry, export, hop), have a small store each
// (session-detail.ts, session-tools.ts), reached as `app.sessions.detail`, `.find`, `.raw`, `.exporter` and `.hopPreview`; the archive browser
// that pages one archive is `.archiveBrowser` (session-archives.ts).
import { errorText } from '../api/core.ts';
import { projectsApi } from '../api/projects.ts';
import { sessionsApi } from '../api/sessions.ts';
import { threadsApi } from '../api/threads.ts';
import type { Shinatama } from '../app.ts';
import type { HopLine, InstanceRow, Titles } from '../instances-model.ts';
import { decodeDetail, detailFailure, type DetailRequest } from '../session-detail-model.ts';
import { indexReason, indexValue, isRunning, type IndexState } from '../session-index-model.ts';
import {
  failureText, hitsValue, pageValue, rootsValue, searchFailureText, selectorOf, sessionKey,
  type SearchHit, type SearchRequest, type SessionCursor, type SessionLibraryRoot, type SessionRootSelector,
} from '../session-library-model.ts';
import { agentCounts, rootId, runningRows, savedRows, type Archive, type Narrowing, type SavedRow } from '../sessions-model.ts';
import { Store } from '../store.ts';
import { SessionArchives } from './session-archives.ts';
import { SessionDetail } from './session-detail.ts';
import { SessionExport, SessionFind, SessionHop, SessionRaw } from './session-tools.ts';

const RUNNING_MS = 2000;
const INDEX_MS = 1000;
const PAGE = 50;

/** What the library is read as: the whole of it (all the projects), or the one the window works in. */
export type ProjectScope = 'project' | 'all';

export class SessionsController extends Store {
  readonly detail: SessionDetail;
  readonly archiveBrowser: SessionArchives;
  readonly find: SessionFind;
  readonly raw: SessionRaw;
  readonly exporter: SessionExport;
  readonly hopPreview: SessionHop;

  // --- the saved sessions ---------------------------------------------------------------------------------------------------
  archives: Archive[] = [];
  loadState: 'idle' | 'loading' | 'ready' | 'failed' = 'idle';
  /** What the last read of the roots said when it failed. */
  problem: string | null = null;

  // --- what narrows the list --------------------------------------------------------------------------------------------------
  /** The text typed: it filters as it is typed, and a press of Enter searches the text of the conversations. */
  query = '';
  scope: ProjectScope = 'all';
  agent: string | null = null;
  archive: SessionRootSelector | null = null;

  // --- the text search ----------------------------------------------------------------------------------------------------------
  hits: SearchHit[] | null = null;
  searched = '';
  searching = false;
  /** What the last search said when it failed. */
  searchProblem: string | null = null;

  // --- the ones that run now -------------------------------------------------------------------------------------------------------
  running: InstanceRow[] = [];
  titles: Titles | null = null;
  hops: HopLine[] = [];
  runningRead = false;
  runningProblem: string | null = null;

  // --- the index of session text ---------------------------------------------------------------------------------------------------
  index: IndexState | null = null;
  indexBusy = false;

  private generation = 0;
  private readingRunning = false;
  private runningRevision = 0;
  private runningWatchers = 0;
  private runningTimer: ReturnType<typeof setInterval> | null = null;
  private indexTimer: ReturnType<typeof setInterval> | null = null;
  private searchRevision = 0;

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
    this.detail = new SessionDetail(app);
    this.archiveBrowser = new SessionArchives(app);
    this.find = new SessionFind(app);
    this.raw = new SessionRaw(app);
    this.exporter = new SessionExport(app);
    this.hopPreview = new SessionHop(app);
  }

  dispose(): void {
    if (this.runningTimer !== null) clearInterval(this.runningTimer);
    if (this.indexTimer !== null) clearInterval(this.indexTimer);
    this.runningTimer = this.indexTimer = null;
    this.find.dispose();
    this.exporter.dispose();
  }

  // --- reading the lists -------------------------------------------------------------------------------------------------------

  /** What narrows the list now. The project is the one the window works in, when the scope says so. */
  get narrowing(): Narrowing {
    return { project: this.scope === 'project' ? this.app.projects.path || null : null, archive: this.archive, agent: this.agent, text: this.hits ? '' : this.query };
  }

  /** The saved sessions the list shows, newest first. */
  get saved(): SavedRow[] {
    return savedRows(this.archives, this.narrowing);
  }

  /** The running sessions the list shows. */
  get shown(): InstanceRow[] {
    const { agent, text } = this.narrowing;
    return runningRows(this.running, this.titles, { agent, text });
  }

  /** How many sessions each agent has, for the sidebar. */
  get agents(): { agent: string; count: number }[] {
    return agentCounts(this.archives, this.running);
  }

  get more(): boolean {
    return this.loadState === 'ready' && this.archives.some(archive => archive.next);
  }

  /** Archives that could not be read, with why. */
  get failures(): { root: SessionLibraryRoot; failure: string }[] {
    return this.archives.filter(archive => archive.failure).map(archive => ({ root: archive.root, failure: archive.failure! }));
  }

  /** The saved session of a row key, if it is among those read. */
  row(root: SessionRootSelector, nativeId: string): SavedRow | undefined {
    for (const archive of this.archives) {
      const row = archive.rows.find(candidate => candidate.nativeId === nativeId);
      if (row && archive.root.storeRootId === root.storeRootId && archive.root.configurationRevision === root.configurationRevision) return { root: archive.root, row };
    }
    return undefined;
  }

  setQuery(query: string): void {
    this.query = query;
    if (this.hits) { this.hits = null; this.searched = ''; }
    this.searchProblem = null;
    this.changed();
  }

  setAgent(agent: string | null): void {
    this.agent = agent;
    this.rerunSearch();
    this.changed();
  }

  setArchive(archive: SessionRootSelector | null): void {
    this.archive = archive;
    this.rerunSearch();
    this.changed();
  }

  setScope(scope: ProjectScope): void {
    this.scope = scope;
    this.rerunSearch();
    this.changed();
  }

  private rerunSearch(): void {
    if (this.hits) { this.hits = null; void this.runSearch(); }
  }

  // --- the saved sessions: read ----------------------------------------------------------------------------------------------------

  /** Reads the archives, one after the other (the reader is a contained worker, never two at once), and the first page of each. */
  async loadSaved(): Promise<void> {
    const mine = ++this.generation;
    this.loadState = 'loading'; this.archives = []; this.problem = null; this.changed();
    try {
      const roots = rootsValue(await sessionsApi.roots());
      if (mine !== this.generation) return;
      if (!roots) throw { issue: 'read_unavailable' };
      this.archives = roots.map(root => ({ root, rows: [], next: null, failure: root.available ? null : 'This archive is unavailable.' }));
      this.changed();
      for (const archive of this.archives) {
        if (!archive.root.available) continue;
        await this.readPage(archive, null, mine);
        if (mine !== this.generation) return;
        this.changed();
      }
      this.loadState = 'ready';
    } catch (error) {
      if (mine !== this.generation) return;
      this.loadState = 'failed'; this.problem = failureText(error);
    }
    this.changed();
  }

  /** The first read of the window: only when the list has none yet. */
  ensureSaved(): Promise<void> {
    return this.loadState === 'idle' || this.loadState === 'failed' ? this.loadSaved() : Promise.resolve();
  }

  /** The next page of every archive that has one. */
  async loadOlder(): Promise<void> {
    if (this.loadState !== 'ready') return;
    const mine = this.generation;
    this.loadState = 'loading'; this.changed();
    for (const archive of this.archives) {
      if (archive.next) await this.readPage(archive, archive.next, mine);
      if (mine !== this.generation) return;
    }
    this.loadState = 'ready';
    this.changed();
  }

  private async readPage(archive: Archive, cursor: SessionCursor | null, mine: number): Promise<void> {
    try {
      const page = pageValue(await sessionsApi.list({ root: selectorOf(archive.root), cursor, limit: PAGE, includeChildren: true }), archive.root, cursor);
      if (mine !== this.generation) return;
      if (!page) { archive.failure = 'The archive answered in an unexpected shape.'; archive.next = null; return; }
      const known = new Set(archive.rows.map(row => row.nativeId));
      archive.rows.push(...page.rows.filter(row => !known.has(row.nativeId)));
      archive.next = page.next; archive.failure = null;
    } catch (error) {
      if (mine === this.generation) { archive.failure = failureText(error); archive.next = null; }
    }
  }

  // --- the text search -------------------------------------------------------------------------------------------------------------

  /** Searches the text of the conversations the index holds (Enter in the field). */
  async runSearch(): Promise<void> {
    const wanted = this.query.trim();
    if (!wanted || this.searching) return;
    const revision = ++this.searchRevision;
    this.searching = true; this.searchProblem = null; this.changed();
    const request: SearchRequest = {
      query: wanted,
      ...(this.agent ? { harness: this.agent } : {}),
      ...(this.scope === 'project' && this.app.projects.path ? { project: this.app.projects.path } : {}),
    };
    try {
      const found = hitsValue(await sessionsApi.search(request));
      if (this.query.trim() !== wanted || revision !== this.searchRevision) return;
      if (!found) throw { issue: 'read_unavailable' };
      this.hits = found; this.searched = wanted;
    } catch (error) {
      if (revision === this.searchRevision) this.searchProblem = searchFailureText(error);
    } finally {
      if (revision === this.searchRevision) this.searching = false;
      this.changed();
    }
  }

  clearSearch(): void {
    this.searchRevision++;
    this.query = ''; this.hits = null; this.searched = ''; this.searching = false; this.searchProblem = null;
    this.changed();
  }

  // --- the ones that run now ---------------------------------------------------------------------------------------------------------

  /** Reads the running sessions every RUNNING_MS while a screen shows them (the Sessions view, the instances panel); the function
   *  this gives lets go. Reading starts nothing: it asks the backend what it observes. */
  watchRunning(): () => void {
    this.runningWatchers++;
    void this.refreshRunning(true);
    if (this.runningTimer === null) this.runningTimer = setInterval(() => { if (typeof document === 'undefined' || !document.hidden) void this.refreshRunning(); }, RUNNING_MS);
    let stopped = false;
    return () => {
      if (stopped) return;
      stopped = true;
      this.runningWatchers--;
      if (this.runningWatchers === 0 && this.runningTimer !== null) { clearInterval(this.runningTimer); this.runningTimer = null; }
    };
  }

  /** A refresh never overlaps a slow read (Git can take a while); a forced one does, and the newest wins. */
  async refreshRunning(force = false): Promise<void> {
    if (this.readingRunning && !force) return;
    this.readingRunning = true;
    const mine = ++this.runningRevision;
    try {
      const rows = await projectsApi.instances() as InstanceRow[];
      const titles = await threadsApi.instanceTitles(rows.map(row => ({ instanceId: row.id, threadId: this.app.terminals.get(row.id)?.threadId ?? null, state: row.state }))).catch(() => null) as Titles | null;
      const hops = await sessionsApi.hopsRecent().catch(() => []) as HopLine[];
      if (mine !== this.runningRevision) return;
      const signature = JSON.stringify([rows, titles, hops]);
      if (signature !== JSON.stringify([this.running, this.titles, this.hops]) || this.runningProblem !== null || !this.runningRead) {
        this.running = rows; this.titles = titles; this.hops = hops; this.runningProblem = null; this.runningRead = true;
        this.changed();
      }
    } catch (error) {
      if (mine === this.runningRevision) { this.runningProblem = errorText(error); this.runningRead = true; this.changed(); }
    } finally {
      if (mine === this.runningRevision) this.readingRunning = false;
    }
  }

  /** Takes the person to the pane of a running session. False when this window has none. */
  focusRunning(id: string): boolean {
    const session = this.app.terminals.get(id);
    if (!session || session.disposed) return false;
    this.app.ui.leaveToWorkspace();
    void this.app.threads.focusTab(id);
    return true;
  }

  /** Pauses or resumes the summaries of what a session of a thread is doing (the instances panel). */
  async pauseSummaries(threadId: string, paused: boolean): Promise<void> {
    try { await threadsApi.summariesPause(threadId, paused); } catch { /* the list says what it says */ }
    await this.refreshRunning(true);
  }

  // --- the index ------------------------------------------------------------------------------------------------------------------------

  async refreshIndex(): Promise<void> {
    try {
      this.index = indexValue(await sessionsApi.indexStatus());
      if (isRunning(this.index)) this.pollIndex(); else this.stopIndexPolling();
    } catch (error) {
      this.stopIndexPolling();
      this.app.ui.report(indexReason(error), { error: true });
    }
    this.changed();
  }

  private pollIndex(): void {
    if (this.indexTimer === null) this.indexTimer = setInterval(() => { void this.refreshIndex(); }, INDEX_MS);
  }

  private stopIndexPolling(): void {
    if (this.indexTimer !== null) clearInterval(this.indexTimer);
    this.indexTimer = null;
  }

  async startIndex(): Promise<void> {
    if (this.indexBusy) return;
    this.indexBusy = true; this.changed();
    try { await sessionsApi.indexStart(); this.pollIndex(); }
    catch (error) { this.app.ui.report(indexReason(error), { error: true }); }
    this.indexBusy = false;
    await this.refreshIndex();
  }

  async stopIndex(): Promise<void> {
    try { await sessionsApi.indexCancel(); } catch { /* the status says what is running */ }
    await this.refreshIndex();
  }

  /** Forgetting cannot be undone: it asks first and names what goes and what stays. */
  async forgetIndex(): Promise<void> {
    if (this.indexBusy) return;
    const yes = await this.app.ui.ask({
      title: 'Forget the indexed session text?',
      description: 'All the text indexed for search is removed; searching needs a new index. Your sessions and threads are not changed.',
      confirm: 'Forget indexed text',
    });
    if (!yes) return;
    this.indexBusy = true; this.changed();
    try {
      await sessionsApi.indexForget(null);
      this.hits = null; this.searched = '';
      this.app.ui.info('Indexed text forgotten.');
    } catch (error) { this.app.ui.report(indexReason(error), { error: true }); }
    this.indexBusy = false;
    await this.refreshIndex();
  }

  // --- the panels ----------------------------------------------------------------------------------------------------------------------------

  /** Shows the facts of one saved session in the inspector (the person chose its row). */
  select(root: SessionRootSelector, nativeId: string): void {
    this.app.ui.openPanel({ kind: 'summary', root: selectorOf(root), nativeId });
  }

  /** Shows one session's conversation in the inspector. */
  show(root: SessionRootSelector, nativeId: string, at?: { offset: number; fingerprint: string }): void {
    this.app.ui.openPanel(at ? { kind: 'session', root: selectorOf(root), nativeId, at } : { kind: 'session', root: selectorOf(root), nativeId });
  }

  /** Hands a session to another agent: its conversation is read once, to name the exact version the preview is made of, and the
   *  hop's preview opens. */
  async hop(root: SessionRootSelector, nativeId: string): Promise<void> {
    const request: DetailRequest = { root: selectorOf(root), nativeId, cursor: null, limit: 50 };
    try {
      const page = decodeDetail(await sessionsApi.detail(request), request);
      this.app.ui.openPanel({ kind: 'hop', selection: { root: selectorOf(root), nativeId, fingerprint: page.fingerprint, harness: page.harness } });
    } catch (error) {
      this.app.ui.report(detailFailure(error), { error: true });
    }
  }

  /** Whether the row of the session is the one the inspector shows now. */
  isShown(root: SessionRootSelector, nativeId: string): boolean {
    const panel = this.app.ui.panel;
    return !!panel && (panel.kind === 'summary' || panel.kind === 'session') && panel.nativeId === nativeId && sessionKey(panel.root, nativeId) === sessionKey(root, nativeId);
  }

  /** The archive filter's id for a root. */
  archiveId(root: SessionRootSelector): string {
    return rootId(root);
  }
}
