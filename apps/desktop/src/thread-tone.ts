// ADR-0055 v3: a thread that is on has a colour, shown on its sidebar row and on each of its tabs (an 8px dot of Sigrid's `Dot`),
// so the tabs of the threads that are on can be told apart inside the panes they share. A thread that is off has none: its row
// has an empty ring. The colours are Sigrid's eighteen (`COLORS`, from red to burgundy): the person's colours, the same in both schemes.
import { COLORS } from '@acaprino/sigrid';

/** Hands each thread that is on a colour of its own, in the order of the palette, and keeps it while the thread stays on. */
export function createToneKeeper() {
  const held = new Map<string, number>();
  function sync(onIds: readonly string[]) {
    for (const id of [...held.keys()]) if (!onIds.includes(id)) held.delete(id);
    for (const id of onIds) {
      if (held.has(id)) continue;
      const used = new Set(held.values());
      // More threads on than colours: the least used colour is shared.
      let index = COLORS.findIndex((_, candidate) => !used.has(candidate));
      if (index < 0) { const count = COLORS.map((_, candidate) => [...held.values()].filter(value => value === candidate).length); index = count.indexOf(Math.min(...count)); }
      held.set(id, index);
    }
  }
  return {
    /** The colour of a thread, null while it is off. `onIds`: the threads that are on, in the order they were turned on. */
    tone(onIds: readonly string[], id: string): string | null { sync(onIds); const index = held.get(id); return index === undefined ? null : COLORS[index]; },
  };
}
