// One folder operation that Rust can cancel (choosing a project, saving the roots, creating a project). It is registered first (`catalog_begin`)
// so that a cancel that arrives before the work was sent can be answered, then the work is sent with the same request id, and a close of the
// dialog while it runs asks to cancel it (`catalog_cancel`): the answer says whether it was cancelled or had already completed. Pure: the
// dialog's words and its closing are the hooks the controller gives.
import { errorText } from './api/core.ts';
import { beginMutation, cancelMutation, createRequestId } from './project-catalog.ts';

export type MutationPhase = 'pending' | 'cancelling' | 'committing' | 'cancelled';
type Mutation = {
  id: string; phase: MutationPhase; settled: boolean; failure: string | null;
  registration: Promise<void> | null; reserved: boolean; dispatched: boolean; cancelRequested: boolean; cancellation: Promise<void> | null;
};

export interface MutationHooks {
  /** The state moved (busy, phase): the screens draw it again. */
  changed(): void;
  /** The dialog the operation belongs to is on screen. */
  isOpen(): boolean;
  /** The dialog is closed for good (the operation was cancelled, or goes on without it). */
  close(): void;
  /** What the dialog says when the operation failed while it is open; a notice when it is not. */
  fail(text: string, open: boolean): void;
  /** The words of a failure (a failure may carry more than its message, such as the folder that kept the work). Default: the message. */
  describe?(failure: unknown): string;
  /** A notice about an operation that goes on or ended while its dialog is gone. */
  say(text: string, error?: boolean): void;
}

export class MutationRunner {
  private current: Mutation | null = null;
  private readonly hooks: MutationHooks;

  constructor(hooks: MutationHooks) {
    this.hooks = hooks;
  }

  get busy(): boolean { return this.current !== null; }
  get phase(): MutationPhase | null { return this.current?.phase ?? null; }

  /** Runs `work` once, if none is running. `completed` gets its value unless the operation was cancelled meanwhile. */
  async run<T>(work: (requestId: string) => Promise<T>, completed: (value: T) => void): Promise<void> {
    if (this.current) return;
    const mine: Mutation = {
      id: createRequestId(), phase: 'pending', settled: false, failure: null,
      registration: null, reserved: false, dispatched: false, cancelRequested: false, cancellation: null,
    };
    this.current = mine;
    this.hooks.changed();
    try {
      mine.registration = beginMutation(mine.id);
      await mine.registration;
      mine.reserved = true;
      if (mine.cancelRequested) { await mine.cancellation; return; }
      mine.dispatched = true;
      const value = await work(mine.id);
      if (this.current === mine && mine.phase !== 'cancelled') completed(value);
    } catch (failure) {
      mine.failure = this.hooks.describe ? this.hooks.describe(failure) : errorText(failure);
      if (mine.reserved && !mine.cancelRequested) void cancelMutation(mine.id).catch(() => {});
      if (this.current === mine && mine.phase !== 'cancelled' && mine.phase !== 'cancelling') this.hooks.fail(mine.failure, this.hooks.isOpen());
    } finally {
      mine.settled = true;
      const unresolved = mine.reserved && !mine.dispatched && mine.cancelRequested && mine.phase !== 'cancelled';
      if (this.current === mine && mine.phase !== 'cancelling' && !unresolved) { this.current = null; this.hooks.changed(); }
    }
  }

  /** The close (or Cancel) of the dialog: closes at once when nothing runs, asks Rust to cancel what runs, or lets a final save go on. */
  async cancelOrClose(): Promise<void> {
    const mine = this.current;
    if (!mine || mine.phase === 'cancelled') { this.hooks.close(); return; }
    if (mine.phase === 'committing') { this.hooks.close(); this.hooks.say('The folder operation continues in the background.'); return; }
    if (mine.phase === 'cancelling') return;
    mine.cancelRequested = true;
    mine.phase = 'cancelling';
    this.hooks.changed();
    mine.cancellation = (async () => {
      try {
        // The reservation must be acknowledged before a cancel or the work.
        await mine.registration;
        if (this.current !== mine) return;
        const acknowledgement = await cancelMutation(mine.id);
        if (this.current !== mine) return;
        if (acknowledgement.status === 'cancelled' || !mine.dispatched) {
          mine.phase = 'cancelled';
          this.hooks.close();
          if (mine.settled) this.current = null;
        } else if (mine.settled) {
          this.current = null;
          this.hooks.close();
          this.hooks.say(mine.failure ? `Folder operation did not complete: ${mine.failure}` : 'The folder operation finished before cancellation. Its completed result has been kept.', !!mine.failure);
        } else mine.phase = 'committing';
      } catch (failure) {
        if (this.current !== mine) return;
        if (!mine.reserved) {
          mine.phase = 'cancelled';
          this.hooks.close();
          if (mine.settled) this.current = null;
        } else if (mine.settled && mine.dispatched) this.current = null;
        else mine.phase = 'pending';
        if (mine.reserved) this.hooks.fail(mine.failure ?? (this.hooks.describe ? this.hooks.describe(failure) : errorText(failure)), this.hooks.isOpen());
      }
      this.hooks.changed();
    })();
    await mine.cancellation;
  }
}
