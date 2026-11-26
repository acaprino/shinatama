// What is done with a session that was read: search in it, read one raw entry of it, export it to a file, hand it to another agent (a
// hop). Each is a panel of the inspector with its own small state, here; the session they act on is named by what the detail read
// (its fingerprint), so a session that changed since is refused and the person reloads it first.
import { sessionsApi } from '../api/sessions.ts';
import type { Shinatama } from '../app.ts';
import {
  decodeFind, decodeRaw, findFailure, rawFailure, rawRequest, rawSelectable, searchableDetail, validQuery, FIND_ERRORS,
  type DetailRequest, type FindCursor, type FindPage, type FindRequest, type RawSelection,
} from '../session-detail-model.ts';
import {
  exportFailure, exportRefusal, transferable, type ExportFormat, type ExportMode, type ExportOutcome, type ExportSelection,
  type HopPreview, type HopPreviewOutcome, type HopSelection,
} from '../session-export-model.ts';
import { selectorOf } from '../session-library-model.ts';
import { Store } from '../store.ts';
import { SessionProgress } from './session-progress.ts';

const HISTORY = 64;

// --- a search in one session ---------------------------------------------------------------------------------------------------

export class SessionFind extends Store {
  readonly progress = new SessionProgress('search');
  selected: DetailRequest | null = null;
  query = '';
  page: FindPage | null = null;
  cursor: FindCursor | null = null;
  history: (FindCursor | null)[] = [];
  busy = false;
  failed = false;
  message = 'Enter text and select Search this session.';
  private generation = 0;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
    this.progress.subscribe(() => this.changed());
  }

  /** Opens the search for a session the detail verified. */
  open(detail: DetailRequest): boolean {
    if (!searchableDetail(detail)) { this.app.ui.error(FIND_ERRORS.fingerprint_required); return false; }
    this.clear();
    this.selected = { root: selectorOf(detail.root), nativeId: detail.nativeId, limit: 50, cursor: { offset: 0, fingerprint: detail.cursor!.fingerprint } };
    this.message = this.busy ? 'The previous search is still returning; no replacement was started.' : 'Enter text and select Search this session.';
    this.changed();
    return true;
  }

  clear(): void {
    this.generation++;
    this.progress.cancel();
    this.selected = null; this.page = null; this.cursor = null; this.history = []; this.query = ''; this.failed = false;
    this.message = 'Enter text and select Search this session.';
    this.changed();
  }

  get canSearch(): boolean {
    return !this.busy && !!this.selected && validQuery(this.query);
  }

  setQuery(query: string): void {
    this.generation++;
    this.query = query; this.page = null; this.cursor = null; this.history = []; this.failed = false;
    this.message = validQuery(query) ? 'Select Search this session.' : FIND_ERRORS.invalid_query;
    this.changed();
  }

  search(): Promise<void> { return this.read(null, 'new'); }
  next(): Promise<void> { return this.page?.next ? this.read(this.page.next, 'next') : Promise.resolve(); }
  previous(): Promise<void> { return this.history.length ? this.read(this.history.at(-1)!, 'previous') : Promise.resolve(); }

  private async read(target: FindCursor | null, direction: 'new' | 'next' | 'previous'): Promise<void> {
    const { selected } = this;
    if (this.busy || !selected || !validQuery(this.query)) return;
    const at = this.generation;
    const oldCursor = this.cursor;
    const request: FindRequest = { detail: { ...selected, root: { ...selected.root }, cursor: { ...selected.cursor! } }, query: this.query, cursor: target };
    const requestId = this.progress.begin();
    this.busy = true; this.failed = false; this.changed();
    try {
      const decoded = decodeFind(await sessionsApi.find(request, requestId), request);
      if (at !== this.generation) return;
      if (direction === 'new') this.history = [];
      else if (direction === 'next') { if (this.history.length === HISTORY) this.history.shift(); this.history.push(oldCursor); }
      else this.history.pop();
      this.page = decoded; this.cursor = target;
      this.message = decoded.totalHits ? 'Choose a match to open its saved transcript.' : 'No matching canonical text in this session.';
    } catch (error) {
      if (at === this.generation) {
        this.page = null; this.cursor = null; this.history = [];
        if (this.progress.wasCancelled) { this.failed = false; this.message = 'Search cancelled. Enter text and select Search this session.'; }
        else { this.failed = true; this.message = findFailure(error); this.app.ui.report(this.message, { error: true }); }
      }
    } finally {
      this.progress.end(requestId);
      this.busy = false;
      if (at !== this.generation && this.selected) this.message = 'The previous search finished. Search again explicitly.';
      this.changed();
    }
  }

  /** A match was chosen: the detail opens on the page of its event. */
  choose(hit: FindPage['hits'][number]): void {
    const { page, selected } = this;
    if (this.busy || !page || !selected || this.query !== page.query) return;
    this.app.ui.openPanel({ kind: 'session', root: selectorOf(page.root), nativeId: page.nativeId, at: { offset: hit.eventOrdinal, fingerprint: page.sourceFingerprint } });
  }

  dispose(): void {
    this.progress.dispose();
  }
}

// --- one raw entry -------------------------------------------------------------------------------------------------------------

/** Opening never reads RAW: only a checked warning and an explicit Load do. Nothing is kept, logged or saved. */
export class SessionRaw extends Store {
  selected: RawSelection | null = null;
  confirmed = false;
  content = '';
  busy = false;
  failed = false;
  message = 'RAW has not been requested.';
  private generation = 0;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  open(selection: RawSelection): boolean {
    if (!rawSelectable(selection)) { this.app.ui.error('This RAW selector is invalid. Reload the session detail.'); return false; }
    this.clear();
    this.selected = { root: selectorOf(selection.root), nativeId: selection.nativeId, entryOrdinal: selection.entryOrdinal, fingerprint: selection.fingerprint };
    this.message = this.busy ? 'The previous read is still returning; no replacement was started.' : 'Confirm the privacy warning before loading RAW.';
    this.changed();
    return true;
  }

  /** Clears the view: a closed panel keeps nothing of what it showed. */
  clear(): void {
    this.generation++;
    this.selected = null; this.confirmed = false; this.content = ''; this.message = 'RAW has not been requested.'; this.failed = false;
    this.changed();
  }

  /** The warning was checked or unchecked; unchecked, what was loaded is cleared. */
  confirm(confirmed: boolean): void {
    this.confirmed = confirmed;
    if (!confirmed) { this.generation++; this.content = ''; this.message = 'RAW has been cleared.'; }
    this.changed();
  }

  async load(): Promise<void> {
    const { selected } = this;
    if (this.busy || !selected || !this.confirmed) return;
    const at = this.generation;
    this.busy = true; this.content = ''; this.failed = false; this.changed();
    try {
      const reply = await sessionsApi.raw(rawRequest(selected));
      if (at !== this.generation || !this.confirmed) return;
      this.content = decodeRaw(reply, selected);
      this.message = 'Only this native entry is shown. RAW is not saved by this view.';
    } catch (error) {
      if (at === this.generation) { this.content = ''; this.failed = true; this.message = rawFailure(error); this.app.ui.report(this.message, { error: true }); }
    } finally {
      this.busy = false;
      if (at !== this.generation && this.selected) this.message = 'The previous read finished. Confirm and load again explicitly.';
      this.changed();
    }
  }
}

// --- export --------------------------------------------------------------------------------------------------------------------

export class SessionExport extends Store {
  readonly progress = new SessionProgress('export');
  selected: ExportSelection | null = null;
  format: ExportFormat = 'stx';
  mode: ExportMode = 'full';
  withAttachments = false;
  busy = false;
  failed = false;
  message = 'Choose a format, then Save as…';
  saved: Extract<ExportOutcome, { state: 'saved' }> | null = null;
  private generation = 0;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
    this.progress.subscribe(() => this.changed());
  }

  open(selection: ExportSelection): boolean {
    if (!transferable(selection)) { this.app.ui.error('This session cannot be exported safely.'); return false; }
    this.generation++;
    this.selected = { root: selectorOf(selection.root), nativeId: selection.nativeId, fingerprint: selection.fingerprint, harness: selection.harness };
    this.saved = null; this.failed = false; this.busy = false;
    this.message = 'Choose a format, then Save as…';
    this.changed();
    return true;
  }

  close(): void {
    this.progress.cancel();
    this.generation++;
    this.busy = false; this.selected = null; this.saved = null; this.message = ''; this.failed = false;
    this.changed();
  }

  setFormat(format: ExportFormat): void {
    this.format = format;
    // Portable JSON never copies attachment content.
    if (format === 'mukade') this.withAttachments = false;
    this.changed();
  }

  setMode(mode: ExportMode): void { this.mode = mode; this.changed(); }
  setAttachments(on: boolean): void { this.withAttachments = on; this.changed(); }

  async save(): Promise<void> {
    const { selected } = this;
    if (this.busy || !selected) return;
    const at = ++this.generation;
    const requestId = this.progress.begin();
    this.busy = true; this.saved = null; this.failed = false; this.message = 'Reading and redacting the session…'; this.changed();
    try {
      const outcome = await sessionsApi.export({
        root: selectorOf(selected.root), nativeId: selected.nativeId, fingerprint: selected.fingerprint,
        format: this.format, mode: this.mode, withAttachments: this.withAttachments,
      }, requestId) as ExportOutcome;
      if (at !== this.generation) return;
      if (outcome.state === 'cancelled') { this.message = this.progress.wasCancelled ? 'Export cancelled. Nothing was written.' : 'Nothing was written.'; return; }
      if (outcome.state === 'refused') { this.failed = true; this.message = exportRefusal(outcome.issue); this.app.ui.report(this.message, { error: true }); return; }
      this.saved = outcome;
      this.message = `Saved ${outcome.bytes.toLocaleString()} bytes.`;
    } catch (error) {
      if (at === this.generation) { this.failed = true; this.message = exportFailure(error); this.app.ui.report(this.message, { error: true }); }
    } finally {
      this.progress.end(requestId);
      if (at === this.generation) this.busy = false;
      this.changed();
    }
  }

  dispose(): void {
    this.progress.dispose();
  }
}

// --- the hop's preview ---------------------------------------------------------------------------------------------------------

/** The preview is the exact seed: what is shown is byte for byte what a hop sends. The hop itself is `app.hop.run`. */
export class SessionHop extends Store {
  selected: HopSelection | null = null;
  destination = '';
  preview: HopPreview | null = null;
  busy = false;
  failed = false;
  message = 'Choose a destination, then Prepare preview.';
  private generation = 0;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  open(selection: HopSelection, destination: string): boolean {
    if (!transferable(selection)) { this.app.ui.error('This session cannot be handed off safely.'); return false; }
    this.generation++;
    this.selected = { root: selectorOf(selection.root), nativeId: selection.nativeId, fingerprint: selection.fingerprint, harness: selection.harness };
    this.destination = destination; this.preview = null; this.busy = false; this.failed = false;
    this.message = 'Choose a destination, then Prepare preview.';
    this.changed();
    return true;
  }

  close(): void {
    this.generation++;
    this.busy = false; this.selected = null; this.preview = null; this.message = ''; this.failed = false;
    this.changed();
  }

  setDestination(destination: string): void {
    this.destination = destination; this.preview = null; this.message = ''; this.failed = false;
    this.changed();
  }

  async prepare(): Promise<void> {
    const { selected, destination } = this;
    if (this.busy || !selected || !destination) return;
    const at = ++this.generation;
    this.busy = true; this.preview = null; this.failed = false; this.message = 'Reading, redacting and budgeting the conversation…'; this.changed();
    try {
      const outcome = await sessionsApi.hopPreview({ root: selectorOf(selected.root), nativeId: selected.nativeId, fingerprint: selected.fingerprint, destination }) as HopPreviewOutcome;
      if (at !== this.generation) return;
      if (outcome.state === 'refused') { this.failed = true; this.message = outcome.message; return; }
      this.preview = outcome.preview;
      this.message = 'This is exactly what the destination receives.';
    } catch {
      if (at === this.generation) { this.failed = true; this.message = 'The preview could not be prepared. Retry explicitly.'; this.app.ui.report(this.message, { error: true }); }
    } finally {
      if (at === this.generation) this.busy = false;
      this.changed();
    }
  }

  /** Confirmed: opens the destination, pastes the seed that was shown and waits for it to answer (`HopController.run`). The panel
   *  is given back when the destination needs the person (a folder-trust question in its pane). */
  async send(): Promise<void> {
    const { preview } = this;
    if (this.busy || !preview) return;
    const at = ++this.generation;
    this.busy = true; this.failed = false; this.changed();
    const progress = (message: string) => { if (at === this.generation) { this.failed = false; this.message = message; this.changed(); } };
    const release = () => { if (at === this.generation) this.app.ui.closePanel(); };
    try {
      const done = await this.app.hop.run(preview, progress, release);
      progress(done);
      if (at === this.generation) { this.preview = null; this.app.ui.info(done); }
    } catch (error) {
      if (at === this.generation) { this.failed = true; this.message = typeof error === 'string' ? error : 'The hop could not start; nothing changed in the source.'; }
    } finally {
      if (at === this.generation) this.busy = false;
      this.changed();
    }
  }
}
