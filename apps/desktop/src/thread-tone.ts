// ADR-0055 v3: a thread that is on has a colour, shown on its sidebar row and on each of its tabs,
// so the tabs of the threads that are on can be told apart inside the panes they share. A thread
// that is off has none: its row's square stays empty and grey.
/** Far-apart hues, in the order they are handed out: the first two threads on differ the most. */
const TONES = ['#4cb8ff', '#ffc23d', '#5be37d', '#b98cff', '#ff8f40', '#2ee6c8', '#ff5c8a', '#e8f064'] as const;

/** Hands each thread that is on a colour of its own and keeps it while the thread stays on. */
export function createToneKeeper() {
  const held = new Map<string, number>();
  function sync(onIds: readonly string[]) {
    for (const id of [...held.keys()]) if (!onIds.includes(id)) held.delete(id);
    for (const id of onIds) {
      if (held.has(id)) continue;
      const used = new Set(held.values());
      // More threads on than colours: the least used colour is shared.
      let index = TONES.findIndex((_, candidate) => !used.has(candidate));
      if (index < 0) { const count = TONES.map((_, candidate) => [...held.values()].filter(value => value === candidate).length); index = count.indexOf(Math.min(...count)); }
      held.set(id, index);
    }
  }
  return {
    /** The colour of a thread, null while it is off. `onIds`: the threads that are on, in the order they were turned on. */
    tone(onIds: readonly string[], id: string): string | null { sync(onIds); const index = held.get(id); return index === undefined ? null : TONES[index]; },
  };
}
