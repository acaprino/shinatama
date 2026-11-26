// The attention widget's controller: the notice Rust holds, what the person did with it, and the portrait's expression. The page of the
// widget (widget.tsx) has no `Shinatama`: this is its one store, and it follows the store contract (store.ts).
import { listen, type Unlisten } from '../api/core.ts';
import { settingsApi } from '../api/settings.ts';
import {
  nextExpression, noticeShown, parseSnapshot, WIDGET_WORDS, type AttentionNotice, type Expression,
} from '../attention-model.ts';
import { Store } from '../store.ts';

export class AttentionWidget extends Store {
  /** The notice on show, or null. */
  notice: AttentionNotice | null = null;
  expression: Expression = 'smile';
  /** An action is waiting for Rust: both buttons are off. */
  busy = false;
  /** What went wrong, in words, under the notice. */
  problem: string | null = null;

  private revision = 0;
  private shownBefore = false;
  private stop: Unlisten | null = null;
  private readonly random: () => number;

  constructor(random: () => number = Math.random) {
    super();
    this.random = random;
  }

  /** Listens first, then reads: a change between the start and the first read is kept. */
  async start(): Promise<void> {
    try { this.stop = await listen('attention-changed', () => { void this.refresh(); }); } catch { /* the first read still shows what there is */ }
    await this.refresh();
  }

  dispose(): void {
    this.stop?.();
    this.stop = null;
  }

  /** Reads the notice again. A read that is overtaken by a newer one is dropped. */
  async refresh(): Promise<void> {
    const revision = ++this.revision;
    try {
      const snapshot = parseSnapshot(await settingsApi.attentionCurrent());
      if (revision === this.revision) this.show(noticeShown(snapshot));
    } catch {
      // The main process may be briefly busy: a notice already on show stays readable, and says it may be old.
      if (revision === this.revision && this.notice) { this.problem = WIDGET_WORDS.stale; this.changed(); }
    }
  }

  open(): Promise<void> {
    return this.act(id => settingsApi.attentionOpen(id), WIDGET_WORDS.openFailed);
  }

  dismiss(): Promise<void> {
    return this.act(id => settingsApi.attentionDismiss(id), WIDGET_WORDS.dismissFailed);
  }

  private show(next: AttentionNotice | null): void {
    const fresh = next !== null && next.id !== this.notice?.id;
    this.notice = next;
    if (fresh) {
      this.expression = nextExpression(this.expression, !this.shownBefore, this.random);
      this.shownBefore = true;
      this.problem = null;
    }
    this.changed();
  }

  /** One action at a time, on the notice that is on show; it ends by reading Rust again, which says what is current now. */
  private async act(run: (id: string) => Promise<unknown>, failure: string): Promise<void> {
    if (!this.notice || this.busy) return;
    const id = this.notice.id;
    this.busy = true;
    this.problem = null;
    this.changed();
    try {
      await run(id);
      await this.refresh();
    } catch {
      if (this.notice?.id === id) this.problem = failure;
    } finally {
      this.busy = false;
      this.changed();
    }
  }
}
