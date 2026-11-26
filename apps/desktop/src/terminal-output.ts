// Pure: how what a terminal printed is handed to the screen that draws it.

/** Lines of output written in one go: a long burst is cut so that the page draws between the parts. */
export const LINES_PER_WRITE = 400;

/** What a screen that is not there yet may hold of a terminal's output: the oldest is let go beyond it. */
export const BACKLOG_BYTES = 8 * 1024 * 1024;

/** Output cut after every `lines`-th newline. The terminal's parser keeps its state across writes, so a cut inside an escape
 *  sequence is harmless. */
export function splitOutput(bytes: Uint8Array, lines = LINES_PER_WRITE): Uint8Array[] {
  const parts: Uint8Array[] = [];
  let start = 0;
  let count = 0;
  for (let index = 0; index < bytes.length; index++) {
    if (bytes[index] === 10 && ++count === lines) { parts.push(bytes.subarray(start, index + 1)); start = index + 1; count = 0; }
  }
  if (start < bytes.length || !parts.length) parts.push(bytes.subarray(start));
  return parts;
}

/** The output held for a screen, bounded: adds `part` and lets the oldest parts go beyond `limit` bytes (the newest part always stays). */
export function holdOutput(held: Uint8Array[], part: Uint8Array, limit = BACKLOG_BYTES): Uint8Array[] {
  const next = [...held, part];
  let total = next.reduce((sum, one) => sum + one.length, 0);
  while (next.length > 1 && total > limit) total -= next.shift()!.length;
  return next;
}
