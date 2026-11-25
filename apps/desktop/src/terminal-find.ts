// The search of a terminal's retained output: which lines of its buffer hold the text. Pure; the dialog (screens/workspace/terminal-find.tsx)
// reads the lines through the terminal's handle and selects the one the person is at.

/** How many lines from the end of the buffer are searched (the retained screen and scrollback). */
export const SEARCH_LINES = 5500;
/** How many matching lines are kept; more is said as «200+». */
export const MAX_HITS = 200;

export interface Hits { rows: number[]; overflow: boolean }

/** The lines (by index) of `length` lines that hold `query`, ignoring case, the last `SEARCH_LINES` of them, at most `MAX_HITS`. */
export function findLines(lineAt: (index: number) => string, length: number, query: string): Hits {
  const text = query.toLocaleLowerCase();
  const rows: number[] = [];
  if (!text) return { rows, overflow: false };
  for (let row = Math.max(0, length - SEARCH_LINES); row < length; row++) {
    if (lineAt(row).toLocaleLowerCase().includes(text)) {
      rows.push(row);
      if (rows.length === MAX_HITS) return { rows, overflow: row + 1 < length };
    }
  }
  return { rows, overflow: false };
}

/** The hit after (`direction` 1) or before (-1) the one chosen, round the list; the first when none is chosen yet. */
export function stepHit(count: number, chosen: number, direction: 1 | -1): number {
  if (count === 0) return -1;
  return chosen < 0 ? (direction === 1 ? 0 : count - 1) : (chosen + direction + count) % count;
}

/** What the dialog says under its field. */
export function findStatus(query: string, hits: Hits, chosen: number): string {
  if (!query) return 'Search the terminal’s retained screen and scrollback.';
  if (!hits.rows.length) return 'No matching lines in the retained buffer.';
  return `${chosen + 1} of ${hits.rows.length}${hits.overflow ? '+' : ''} matching lines`;
}
