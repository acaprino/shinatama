// How a session comes to run, said once for the whole window. Two parts:
//  - the open queue: every open (a new session, a shell, a resume) takes its turn, one at a time.
//    Nothing is refused or greyed out because another open is under way.
//  - the wants: the saved sessions asked to run, by the user (Resume) or by the start (the ones the
//    saved layout holds as running). A want stays until its session runs, is given up or is cancelled;
//    a refusal that can pass a moment later (the model proxy still starting, memory, a busy reader)
//    is tried again by itself. What a tab's card says is read from here, never from a flag of its own.

/** One open at a time, in the order asked. `changed` runs when the open under way changes. */
export function createOpenQueue(changed: () => void = () => {}) {
  let tail: Promise<unknown> = Promise.resolve(); let current: string | null = null; let pending = 0;
  return {
    /** Runs `job` after every open asked before it; its result or its failure is the caller's. */
    run<T>(key: string, job: () => Promise<T>): Promise<T> {
      pending += 1;
      const turn = tail.then(async () => {
        current = key; changed();
        try { return await job(); } finally { current = null; pending -= 1; changed(); }
      });
      tail = turn.catch(() => undefined);
      return turn;
    },
    /** The key of the open under way. */
    current: () => current,
    /** An open is under way or waiting. */
    busy: () => pending > 0,
  };
}
export type OpenQueue = ReturnType<typeof createOpenQueue>;

/** A saved session asked to run. `attempts`: refusals so far. `notBefore`: when it is tried next (ms).
 *  `reason`: what the last refusal said. `restored`: asked by the start, so it opens in the background. */
export type Want = { attempts: number; notBefore: number; reason: string | null; restored: boolean };
export const newWant = (restored: boolean): Want => ({ attempts: 0, notBefore: 0, reason: null, restored });
/** The waits before each new try. The first start of the model proxy took 40 to 77 s (shinatama.log,
 *  2026-11-18), and an open waits for it by itself, so four more tries cover a slow start. */
export const RETRY_DELAYS_MS: readonly number[] = [3000, 10000, 30000, 60000];
/** The want after a refusal: kept and tried again later, or null when it is given up (a refusal that
 *  cannot pass, or every try spent). */
export function afterRefusal(want: Want, retryable: boolean, reason: string, now: number): Want | null {
  if (!retryable || want.attempts >= RETRY_DELAYS_MS.length) return null;
  return { ...want, attempts: want.attempts + 1, notBefore: now + RETRY_DELAYS_MS[want.attempts], reason };
}
/** The wanted session to open now, or how long until one is due, or null when nothing is wanted.
 *  The ones the user asked for go first, in the order asked; then the restored ones, in the order of
 *  the panes and their tabs (`restoredOrder`). */
export function nextWant(wants: ReadonlyMap<string, Want>, restoredOrder: readonly string[], now: number): { id: string } | { waitMs: number } | null {
  if (!wants.size) return null;
  const rank = (id: string) => { const at = restoredOrder.indexOf(id); return at < 0 ? restoredOrder.length : at; };
  const ids = [...wants.keys()];
  const order = [...ids.filter(id => !wants.get(id)!.restored), ...ids.filter(id => wants.get(id)!.restored).sort((a, b) => rank(a) - rank(b))];
  const due = order.find(id => wants.get(id)!.notBefore <= now);
  if (due) return { id: due };
  return { waitMs: Math.min(...ids.map(id => wants.get(id)!.notBefore)) - now };
}

/** What one try to open a saved session came to: it runs, there is nothing to open (its card says why),
 *  or it was refused, with what to say and whether a later try can pass. */
export type OpenOutcome = { opened: true } | { nothing: true } | { refused: string; retry: boolean };
export interface WantsOptions {
  /** One try to make the session run; called at its turn of the open queue. `restored`: asked by the start. */
  attempt(sessionId: string, restored: boolean): Promise<OpenOutcome>;
  /** The sessions the saved layout holds as running and that do not run yet, in the order of the panes and their tabs. */
  restoredOrder(): readonly string[];
  /** A want is gone: its session runs, has nothing to open or was cancelled (`refused` null), or it was
   *  given up (`refused`: what the last try said). */
  settled(sessionId: string, refused: string | null): void;
  /** What the cards say may have changed. */
  changed(): void;
  now?(): number;
  /** Waits `ms`, or less when `wake` is called. */
  wait?(ms: number): { done: Promise<void>; wake(): void };
}
/** The saved sessions asked to run, and the one loop that opens them: the ones the user asked for first,
 *  then the restored ones in the order of their tabs, each at its turn of the open queue. A refusal that
 *  can pass keeps the want for a later try (afterRefusal); while every want waits for its next try the
 *  loop sleeps until the first is due or a new one is asked. */
export function createWants(opens: OpenQueue, options: WantsOptions) {
  const wants = new Map<string, Want>();
  const now = options.now ?? (() => Date.now());
  const wait = options.wait ?? ((ms: number) => {
    let wake = () => {};
    const done = new Promise<void>(resolve => { const timer = setTimeout(resolve, ms); wake = () => { clearTimeout(timer); resolve(); }; });
    return { done, wake };
  });
  let pumping = false; let pumped: Promise<void> = Promise.resolve(); let wake: (() => void) | null = null;
  async function settle(sessionId: string) {
    const want = wants.get(sessionId); if (!want) return;
    let outcome: OpenOutcome;
    // Cancelled while it waited its turn: nothing is tried.
    try { outcome = await opens.run(sessionId, async () => wants.has(sessionId) ? options.attempt(sessionId, want.restored) : { nothing: true }); }
    catch (error) { outcome = { refused: error instanceof Error ? error.message : String(error), retry: false }; }
    const left = wants.get(sessionId); if (!left) return;
    if ('refused' in outcome) {
      const kept = afterRefusal(left, outcome.retry, outcome.refused, now());
      if (kept) { wants.set(sessionId, kept); options.changed(); return; }
    }
    wants.delete(sessionId); options.settled(sessionId, 'refused' in outcome ? outcome.refused : null); options.changed();
  }
  async function pump() {
    // `pumping` falls in the same step that finds nothing left, so a want asked right after starts a new loop.
    try {
      for (;;) {
        const next = nextWant(wants, options.restoredOrder(), now());
        if (!next) return;
        if ('waitMs' in next) { const sleep = wait(next.waitMs); wake = sleep.wake; await sleep.done; wake = null; continue; }
        await settle(next.id);
      }
    } finally { pumping = false; options.changed(); }
  }
  // The loop starts a step later, so sessions asked together (the restored ones) are ordered together.
  const run = () => { wake?.(); if (!pumping) { pumping = true; pumped = Promise.resolve().then(pump); } };
  return {
    /** Asks for a session to run. Asked by the user while it waits for a new try, it is tried now and no longer in the background. */
    ask(sessionId: string, restored = false) {
      const waiting = wants.get(sessionId);
      if (!waiting) wants.set(sessionId, newWant(restored));
      else if (!restored) wants.set(sessionId, { ...waiting, notBefore: 0, restored: false });
      options.changed(); run();
    },
    /** Leaves a waiting session not running. False when it was not wanted. */
    cancel(sessionId: string): boolean {
      if (!wants.delete(sessionId)) return false;
      options.settled(sessionId, null); options.changed(); wake?.(); return true;
    },
    /** Every want, by session, for the cards (openCard). */
    all: (): ReadonlyMap<string, Want> => wants,
    /** Resolves when nothing is wanted any more. */
    idle: (): Promise<void> => pumped,
  };
}

/** What the card of a session that is not running says: it is `opening` now, `queued` for its turn,
 *  `retrying` after a refusal that can pass, or it `offer`s its button. */
export type OpenCard = 'opening' | 'queued' | 'retrying' | 'offer';
export function openCard(sessionId: string, state: { current: string | null; wants: ReadonlyMap<string, Want>; now: number }): OpenCard {
  if (state.current === sessionId) return 'opening';
  const want = state.wants.get(sessionId);
  if (!want) return 'offer';
  return want.notBefore > state.now ? 'retrying' : 'queued';
}
