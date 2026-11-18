// The map asks the engine (`team_evaluate`) about every formation it edits, and about the trial formations behind a struck-out
// option. Editing is fast (typing, dragging an arrow), the engine's answer is only wanted for the latest state: calls start at
// least `gap` milliseconds apart, and a request still waiting is replaced by a newer one of the same kind (its promise ends null).
export type Evaluator<F, T> = {
  /** Resolves with the answer, or null when a newer request of the same `kind` took its place. Rejects with the call's own error. */
  request(kind: string, formation: F): Promise<T | null>;
  /** Drops every request still waiting (their promises end null). A call in flight is not stopped. */
  cancel(): void;
};

export function createEvaluator<F, T>(call: (formation: F) => Promise<T>, gap = 150, now: () => number = () => Date.now()): Evaluator<F, T> {
  type Job = { formation: F; resolve: (value: T | null) => void; reject: (reason: unknown) => void };
  const waiting = new Map<string, Job>();
  let last = -Infinity; let timer: ReturnType<typeof setTimeout> | null = null;
  function pump() {
    if (timer !== null || !waiting.size) return;
    timer = setTimeout(() => {
      timer = null;
      const first = waiting.entries().next().value; if (!first) return;
      const [kind, job] = first; waiting.delete(kind);
      last = now();
      call(job.formation).then(job.resolve, job.reject).finally(pump);
    }, Math.max(0, last + gap - now()));
  }
  return {
    request(kind, formation) {
      return new Promise<T | null>((resolve, reject) => {
        waiting.get(kind)?.resolve(null);
        waiting.delete(kind); // re-queued behind the others of its turn
        waiting.set(kind, { formation, resolve, reject });
        pump();
      });
    },
    cancel() { for (const job of waiting.values()) job.resolve(null); waiting.clear(); },
  };
}
