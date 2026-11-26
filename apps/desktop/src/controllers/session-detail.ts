// The conversation of one saved session, read a page at a time (never started, never resumed): the page, the way back to the pages before
// it, the lineage, what the model proxy can do for the session (polled while its panel is open and visible). Metadata and text only; the
// raw entries, the search in the session, the export and the hop have their own state beside this (session-tools.ts).
import { sessionsApi } from '../api/sessions.ts';
import type { Shinatama } from '../app.ts';
import { nextCapabilityLines, parseCapabilities, type CapabilityState } from '../sidecar-status.ts';
import {
  decodeDetail, detailFailure, type DetailCursor, type DetailPage, type DetailRequest,
} from '../session-detail-model.ts';
import { nativeIdValue, rootValue, sameRoot, selectorOf, digest, integer, type SessionRootSelector } from '../session-library-model.ts';
import { Store } from '../store.ts';

const CAPABILITY_POLL_MS = 5000;
const CAPABILITY_TIMEOUT_MS = 4000;
const HISTORY = 64;
/** The window is not shown (a minimised window reads nothing it does not need). */
const pageHidden = (): boolean => typeof document !== 'undefined' && document.hidden;

export class SessionDetail extends Store {
  current: { root: SessionRootSelector; nativeId: string } | null = null;
  page: DetailPage | null = null;
  cursor: DetailCursor | null = null;
  history: (DetailCursor | null)[] = [];
  busy = false;
  failed = false;
  message = 'Choose a session explicitly.';
  /** The sentences of "Model proxy" in the overview, or null while nothing is known. */
  proxyLines: string[] | null = null;
  /** The ordinal of the event a search hit named: its row opens. */
  jump: number | null = null;

  private generation = 0;
  private proxyGeneration = 0;
  private proxyInFlight = -1;
  private proxyTimer: ReturnType<typeof setInterval> | null = null;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  /** The request that reads the page the detail is on: what the search, the export and the hop are asked for. */
  get request(): DetailRequest | null {
    const { current, page } = this;
    return current && page ? { root: selectorOf(current.root), nativeId: current.nativeId, cursor: { offset: 0, fingerprint: page.fingerprint }, limit: 50 } : null;
  }

  /** Shows one session, from its first page or from the page of the event `jump` names. Refuses a selector that is not safe to read. */
  show(root: SessionRootSelector, nativeId: string, jump: DetailCursor | null = null): boolean {
    if (!rootValue(root) || !nativeIdValue(nativeId) || (jump && (!integer(jump.offset) || !digest(jump.fingerprint)))) {
      this.app.ui.error('This session selector cannot be read safely.');
      return false;
    }
    this.generation++;
    this.current = { root: selectorOf(root), nativeId };
    this.page = null; this.cursor = null; this.history = []; this.failed = false; this.jump = jump ? jump.offset : null;
    this.message = 'Reading the selected session…';
    this.changed();
    void this.load(jump, 'reload');
    return true;
  }

  /** Whether this session is the one shown, read from this page (so that a panel that comes back after another covered it keeps its place). */
  isShowing(root: SessionRootSelector, nativeId: string, jump: DetailCursor | null): boolean {
    const { current } = this;
    return !!current && current.nativeId === nativeId && sameRoot(current.root, root) && (jump === null || this.jump === jump.offset)
      && (this.page !== null || this.busy);
  }

  /** Lets go of what was read (the panel closed or shows another session). */
  close(): void {
    this.generation++;
    this.stopProxyReads();
    this.current = null; this.page = null; this.cursor = null; this.history = []; this.failed = false; this.jump = null;
    this.changed();
  }

  reload(): Promise<void> { return this.load(null, 'reload'); }
  next(): Promise<void> { return this.page?.next ? this.load(this.page.next, 'next') : Promise.resolve(); }
  previous(): Promise<void> { return this.history.length ? this.load(this.history.at(-1) ?? null, 'previous') : Promise.resolve(); }

  private async load(requested: DetailCursor | null, direction: 'reload' | 'next' | 'previous'): Promise<void> {
    if (!this.current || (this.busy && direction !== 'reload')) return;
    const at = this.generation;
    const request: DetailRequest = { root: selectorOf(this.current.root), nativeId: this.current.nativeId, cursor: requested, limit: 50 };
    this.busy = true; this.failed = false; this.changed();
    try {
      const value = decodeDetail(await sessionsApi.detail(request), request);
      if (at !== this.generation) return;
      if (direction === 'reload') this.history = [];
      else if (direction === 'next') { this.history.push(this.cursor); if (this.history.length > HISTORY) this.history.shift(); }
      else this.history.pop();
      this.page = value; this.cursor = requested;
      this.message = value.totalEvents ? 'Read-only projection. Raw and omitted payloads are not requested.' : 'No canonical events were returned.';
    } catch (error) {
      if (at === this.generation) {
        this.page = null; this.cursor = null; this.history = []; this.failed = true;
        this.message = detailFailure(error);
        this.app.ui.report(this.message, { error: true });
      }
    } finally {
      // A read that was replaced (another session was chosen meanwhile) leaves the state to the one that replaced it.
      if (at === this.generation) { this.busy = false; this.changed(); }
    }
  }

  // --- what the model proxy can do for this session ----------------------------------------------------------------------------

  /** Reads the proxy's capabilities for the session on open, when the window is shown again and every CAPABILITY_POLL_MS, until the
   *  function this gives is called. A reply for a session that is no longer the open one, or one that came after the timeout, is
   *  dropped; a tick changes only the "Model proxy" lines, and only when they changed. */
  watchProxy(): () => void {
    this.stopProxyReads();
    const onVisible = () => { if (!pageHidden()) void this.readProxy(); };
    if (typeof document !== 'undefined') document.addEventListener('visibilitychange', onVisible);
    this.proxyTimer = setInterval(() => { void this.readProxy(); }, CAPABILITY_POLL_MS);
    void this.readProxy();
    return () => { if (typeof document !== 'undefined') document.removeEventListener('visibilitychange', onVisible); this.stopProxyReads(); };
  }

  private stopProxyReads(): void {
    this.proxyGeneration++; this.proxyInFlight = -1;
    if (this.proxyTimer !== null) clearInterval(this.proxyTimer);
    this.proxyTimer = null;
    if (this.proxyLines !== null) { this.proxyLines = null; this.changed(); }
  }

  private async readProxy(): Promise<void> {
    if (!this.current || pageHidden() || this.proxyInFlight === this.proxyGeneration) return;
    const at = this.proxyGeneration;
    const id = this.current.nativeId;
    this.proxyInFlight = at;
    let read: CapabilityState;
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const expired = new Promise<unknown>(resolve => { timer = setTimeout(() => resolve(null), CAPABILITY_TIMEOUT_MS); });
      read = parseCapabilities(await Promise.race([sessionsApi.capabilities(id), expired]));
    } catch { read = { state: 'unavailable' }; } finally { clearTimeout(timer); }
    if (this.proxyInFlight === at) this.proxyInFlight = -1;
    if (at !== this.proxyGeneration || !this.current || this.current.nativeId !== id) return;
    const lines = nextCapabilityLines(this.proxyLines, read);
    if (lines) { this.proxyLines = lines; this.changed(); }
  }
}
