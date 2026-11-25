// The archive browser: one archive the person chose, paged explicitly (previous, next, reload) a page of metadata at a time, filtered
// and grouped by the folder each session recorded, and the form that adds a read-only archive. Browsing never starts or resumes an agent.
// The Sessions view reads every archive for its list; this is for looking at one closely, or at one that is not listed.
import { sessionsApi } from '../api/sessions.ts';
import type { Shinatama } from '../app.ts';
import {
  failureText, INVALID_ARCHIVE, pageValue, rootsValue, sameRoot, selectorOf,
  type SessionArchiveAddRequest, type SessionCursor, type SessionLibraryRoot, type SessionListPage, type SessionMetadataRow, type SessionRootSelector,
} from '../session-library-model.ts';
import { Store } from '../store.ts';

const HISTORY = 64;
export type ArchiveView = 'list' | 'projects';

/** The rows of a page that answer the filter, matched against id, title, folder and date. */
export function filterPage(page: SessionListPage | null, query: string): SessionMetadataRow[] {
  const wanted = query.toLocaleLowerCase();
  return page?.rows.filter(row => [row.nativeId, row.title, row.workingDirectory, row.updatedAt].some(value => value?.toLocaleLowerCase().includes(wanted))) ?? [];
}

/** The rows grouped by the exact folder they recorded (null: the reader gave none); no path is resolved or tied to a saved project. */
export function groupByFolder(rows: readonly SessionMetadataRow[]): { folder: string | null; rows: SessionMetadataRow[] }[] {
  const groups = new Map<string | null, SessionMetadataRow[]>();
  for (const row of rows) {
    const folder = row.workingDirectory?.trim() ? row.workingDirectory : null;
    groups.set(folder, [...(groups.get(folder) ?? []), row]);
  }
  return [...groups].map(([folder, members]) => ({ folder, rows: members }));
}

export class SessionArchives extends Store {
  roots: SessionLibraryRoot[] = [];
  /** The archive chosen, by its index in `roots` ('' for none). */
  chosen = '';
  page: SessionListPage | null = null;
  cursor: SessionCursor | null = null;
  history: (SessionCursor | null)[] = [];
  selected: SessionMetadataRow | null = null;
  filter = '';
  view: ArchiveView = 'list';
  busy: 'roots' | 'list' | 'archive' | null = null;
  message = 'Load archives to choose an explicit store.';
  failed = false;

  private generation = 0;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  get root(): SessionLibraryRoot | null {
    return this.chosen === '' ? null : this.roots[Number(this.chosen)] ?? null;
  }

  /** What `1–50 of 120` says for the page that is shown. */
  get position(): string {
    const { page, cursor } = this;
    if (!page) return 'No page loaded';
    const offset = cursor?.offset ?? 0;
    return `${page.total ? offset + 1 : 0}–${offset + page.rows.length} of ${page.total}${!this.history.length && offset > 0 ? ' · Reload for first page' : ''}`;
  }

  private reset(): void {
    this.page = null; this.cursor = null; this.history = []; this.selected = null; this.filter = ''; this.failed = false;
  }

  /** The panel closed: nothing of what it read is kept. */
  forget(): void {
    this.generation++;
    this.reset(); this.roots = []; this.chosen = ''; this.busy = null; this.message = 'Load archives to choose an explicit store.';
    this.changed();
  }

  setFilter(filter: string): void { this.filter = filter; this.changed(); }
  setView(view: ArchiveView): void { this.view = view; this.changed(); }
  select(row: SessionMetadataRow | null): void { this.selected = row; this.changed(); }

  private replaceRoots(received: SessionLibraryRoot[], keep: SessionRootSelector | null): void {
    this.roots = received; this.reset();
    const kept = keep ? received.findIndex(root => sameRoot(root, keep)) : -1;
    this.chosen = kept >= 0 ? String(kept) : '';
  }

  async loadRoots(): Promise<void> {
    if (this.busy) return;
    const at = this.generation;
    const before = this.root;
    this.busy = 'roots'; this.failed = false; this.changed();
    try {
      const received = rootsValue(await sessionsApi.roots());
      if (!received) throw new Error('Invalid archive projection');
      if (at !== this.generation) return;
      this.replaceRoots(received, before);
      this.message = received.length ? 'Choose an archive to read its metadata.' : 'No registered archives. No ambient home was searched.';
    } catch (error) {
      if (at === this.generation) { this.reset(); this.failed = true; this.message = failureText(error); this.app.ui.report(this.message, { error: true }); }
    } finally { this.busy = null; this.changed(); }
  }

  /** The person chose an archive (by its index): its first page is read. */
  choose(index: string): void {
    this.generation++;
    this.reset(); this.chosen = index;
    const root = this.root;
    this.message = root ? root.available ? 'Ready to read this explicit archive.' : 'This archive is unavailable. No other archive was used.' : 'Choose an archive explicitly.';
    this.changed();
    if (!this.busy && root?.available) void this.read(null, 'reload');
  }

  reload(): Promise<void> { return this.read(null, 'reload'); }
  next(): Promise<void> { return this.page?.next ? this.read(this.page.next, 'next') : Promise.resolve(); }
  previous(): Promise<void> { return this.history.length ? this.read(this.history.at(-1) ?? null, 'previous') : Promise.resolve(); }

  private async read(requested: SessionCursor | null, direction: 'reload' | 'next' | 'previous'): Promise<void> {
    const root = this.root;
    if (this.busy || !root?.available) return;
    const at = this.generation;
    this.busy = 'list'; this.failed = false; this.changed();
    try {
      const received = pageValue(await sessionsApi.list({ root: selectorOf(root), cursor: requested, limit: 50, includeChildren: true }), root, requested);
      if (!received) throw new Error('Invalid metadata projection');
      const now = this.root;
      if (at !== this.generation || !now || !sameRoot(root, now)) return;
      if (direction === 'reload') this.history = [];
      else if (direction === 'next') { this.history.push(this.cursor); if (this.history.length > HISTORY) this.history.shift(); }
      else this.history.pop();
      this.page = received; this.cursor = requested; this.selected = null;
      this.message = received.total ? 'Metadata only. Select a session to inspect the listed fields.' : 'This archive contains no listed sessions.';
    } catch (error) {
      if (at === this.generation) { this.reset(); this.failed = true; this.message = failureText(error); this.app.ui.report(this.message, { error: true }); }
    } finally { this.busy = null; this.changed(); }
  }

  /** Registers a read-only archive; the list of archives is read again and the new one chosen. */
  async add(harness: string, path: string): Promise<boolean> {
    const folder = path.trim();
    if (this.busy) return false;
    if (!['claude_code', 'codex', 'pi'].includes(harness) || !folder || folder.length > 4096) {
      this.failed = true; this.message = INVALID_ARCHIVE; this.changed(); this.app.ui.report(this.message, { error: true });
      return false;
    }
    const at = this.generation;
    this.busy = 'archive'; this.failed = false; this.changed();
    let registered = false;
    try {
      const added = rootsValue([await sessionsApi.archiveAdd({ harness: harness as SessionArchiveAddRequest['harness'], path: folder })]);
      if (!added || added[0].harness !== harness) throw new Error('Invalid archive projection');
      registered = true;
      const received = rootsValue(await sessionsApi.roots());
      if (!received) throw new Error('Invalid archive projection');
      if (at !== this.generation) return true;
      this.replaceRoots(received, added[0]);
      this.message = added[0].available ? 'Archive registered. Use Reload sessions to browse metadata; the archive files remain unchanged.'
        : 'This archive is already registered but unavailable. Reconnect it explicitly; its history was preserved.';
      // The Sessions view lists it too.
      void this.app.sessions.loadSaved();
      return true;
    } catch (error) {
      if (at === this.generation) {
        this.failed = true;
        this.message = registered ? 'The archive was registered, but its list could not be refreshed. Load archives explicitly.' : failureText(error);
        this.app.ui.report(this.message, { error: true });
      }
      return false;
    } finally { this.busy = null; this.changed(); }
  }
}
