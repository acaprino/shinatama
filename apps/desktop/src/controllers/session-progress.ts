// The progress of a long search or export: the worker reports at most four times a second (`session-progress`), and the line says
// "Exporting… 12,400 entries · 38 MB written · 14 s" with a Cancel, but only once the work has lasted past SHOW_AFTER_MS (a short one never
// flashes a line). An event of another request, a closed or a replaced one, never updates this one.
import { listen, type Unlisten } from '../api/core.ts';
import { sessionsApi } from '../api/sessions.ts';
import { acceptsEvent, describeProgress, SHOW_AFTER_MS, type ProgressEvent } from '../session-progress-core.ts';
import { Store } from '../store.ts';

const TICK_MS = 1000;

function newId(): string {
  const random = globalThis.crypto?.randomUUID?.() ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`;
  return `r-${random}`.slice(0, 64);
}

export class SessionProgress extends Store {
  private id: string | null = null;
  private startedAt: number | null = null;
  private last: ProgressEvent | null = null;
  private stopped = false;
  private visible = false;
  private showTimer: ReturnType<typeof setTimeout> | null = null;
  private tick: ReturnType<typeof setInterval> | null = null;
  private unlisten: Unlisten | null = null;
  private listening = false;

  private readonly kind: 'export' | 'search';
  private readonly now: () => number;

  constructor(kind: 'export' | 'search', now: () => number = () => performance.now()) {
    super();
    this.kind = kind;
    this.now = now;
  }

  /** What the line says now, or null while it is hidden. */
  get line(): string | null {
    if (!this.visible || this.startedAt === null) return null;
    return this.stopped ? 'Cancelling…' : describeProgress(this.kind, this.last, this.now() - this.startedAt);
  }

  /** Whether the work that just ended was stopped by `cancel`. Valid until the next `begin`. */
  get wasCancelled(): boolean {
    return this.stopped;
  }

  /** The work starts: the id its events and its cancel are tagged with. */
  begin(): string {
    this.clear();
    this.stopped = false;
    this.id = newId();
    void this.listen();
    return this.id;
  }

  /** The work `requestId` ended, however it ended. A request already replaced by a newer one is left alone. */
  end(requestId: string): void {
    if (this.id === requestId) { this.clear(); this.changed(); }
  }

  /** Stops the running work (the Cancel button, or the panel closing). */
  cancel(): void {
    if (this.id === null || this.stopped) return;
    this.stopped = true;
    this.changed();
    void sessionsApi.cancel(this.id).catch(() => {});
  }

  /** A report of the worker (public for the tests, which have no event source). */
  report(event: ProgressEvent): void {
    if (!acceptsEvent(this.id, this.kind, event)) return;
    this.last = event;
    if (this.startedAt === null) {
      this.startedAt = this.now();
      this.showTimer = setTimeout(() => { this.showTimer = null; if (this.id !== null) { this.visible = true; this.changed(); } }, SHOW_AFTER_MS);
      this.tick = setInterval(() => { if (this.visible) this.changed(); }, TICK_MS);
    }
    if (this.visible) this.changed();
  }

  dispose(): void {
    this.clear();
    this.listening = false;
    this.unlisten?.();
    this.unlisten = null;
  }

  private async listen(): Promise<void> {
    if (this.listening) return;
    this.listening = true;
    try {
      const off = await listen<ProgressEvent>('session-progress', event => this.report(event));
      if (this.listening) this.unlisten = off; else off();
    } catch { this.listening = false; }
  }

  private clear(): void {
    if (this.showTimer !== null) clearTimeout(this.showTimer);
    if (this.tick !== null) clearInterval(this.tick);
    this.showTimer = null; this.tick = null; this.visible = false; this.id = null; this.startedAt = null; this.last = null;
  }
}
