/** What a terminal no longer holds. xterm keeps only `LIVE_SCROLLBACK` lines, each stored as
 * twelve bytes a cell; a line about to fall out of that buffer is kept here instead, as text
 * with its colors (a few bytes a character), in Blobs the browser keeps outside the page's
 * memory. The history view reads it back on demand.
 *
 * A line is copied once, when it scrolls above the screen (nothing can rewrite it there), and
 * moves to the archive only when xterm drops it. Lines an application clears on purpose
 * (CSI 3 J) are never archived. A resize reflows the buffer: copies made before it are
 * discarded and taken again, so a line trimmed by the reflow itself can be missed. */
import type { IBufferCell, IBufferLine, IMarker, Terminal } from '@xterm/xterm';

export const LIVE_SCROLLBACK = 1000;
/** Lines per Blob. */
export const CHUNK_LINES = 500;
/** Archived lines kept per terminal; older chunks are dropped whole. */
export const MAX_ARCHIVED_LINES = 100_000;

/** Newlines per write, well under the live buffer: every line is copied before xterm can drop it. */
export const LINES_PER_WRITE = 400;
/** Output cut after every `lines`-th newline. xterm's parser keeps its state across writes, so
 * a cut inside an escape sequence is harmless. */
export function splitOutput(bytes: Uint8Array, lines = LINES_PER_WRITE): Uint8Array[] {
  const parts: Uint8Array[] = []; let start = 0; let count = 0;
  for (let index = 0; index < bytes.length; index++) {
    if (bytes[index] === 10 && ++count === lines) { parts.push(bytes.subarray(start, index + 1)); start = index + 1; count = 0; }
  }
  if (start < bytes.length || !parts.length) parts.push(bytes.subarray(start));
  return parts;
}

/** Archived lines the history view holds at once. */
export const WINDOW_LINES = 2000;
/** The first archived line of a window around `line`, in [0, total). */
export function windowStart(line: number, total: number, size = WINDOW_LINES): number {
  return Math.max(0, Math.min(Math.max(0, total - size), line - Math.floor(size / 2)));
}

type Staged ={ text: string; width: number; wrapped: boolean };
export type Chunk = { blob: Blob; lines: number; widths: Uint8Array };

/** One cell's SGR parameters, without the leading reset: '' for the default look. */
export function cellStyle(cell: IBufferCell): string {
  let style = '';
  if (cell.isBold()) style += ';1';
  if (cell.isDim()) style += ';2';
  if (cell.isItalic()) style += ';3';
  if (cell.isUnderline()) style += ';4';
  if (cell.isInverse()) style += ';7';
  if (cell.isInvisible()) style += ';8';
  if (cell.isStrikethrough()) style += ';9';
  style += color(cell.isFgPalette(), cell.isFgRGB(), cell.getFgColor(), 30);
  style += color(cell.isBgPalette(), cell.isBgRGB(), cell.getBgColor(), 40);
  return style;
}
function color(palette: boolean, rgb: boolean, value: number, base: 30 | 40): string {
  if (rgb) return `;${base + 8};2;${(value >> 16) & 255};${(value >> 8) & 255};${value & 255}`;
  if (!palette) return '';
  if (value < 8) return `;${base + value}`;
  if (value < 16) return `;${base + 60 + value - 8}`;
  return `;${base + 8};5;${value}`;
}

/** The line as text with its colors; trailing default blanks are dropped. */
export function serializeLine(line: IBufferLine, cols: number, cell: IBufferCell): Staged {
  let end = Math.min(cols, line.length);
  while (end > 0) {
    const last = line.getCell(end - 1, cell);
    if (last && (last.getChars() !== '' && last.getChars() !== ' ' || !last.isBgDefault() || last.isInverse())) break;
    end--;
  }
  let text = ''; let previous = ''; let width = 0;
  for (let x = 0; x < end; x++) {
    const current = line.getCell(x, cell); if (!current || current.getWidth() === 0) continue;
    const style = cellStyle(current);
    if (style !== previous) { text += `\x1b[0${style}m`; previous = style; }
    text += current.getChars() || ' '; width += current.getWidth();
  }
  if (previous) text += '\x1b[0m';
  return { text, width, wrapped: line.isWrapped };
}

/** The archive's bookkeeping, apart from xterm: lines in, whole Blobs out. */
export function createArchive(chunkLines = CHUNK_LINES, maxLines = MAX_ARCHIVED_LINES) {
  const chunks: Chunk[] = []; let pending: string[] = []; let widths: number[] = [];
  let dropped = 0; let missed = 0;
  const flush = () => {
    if (!pending.length) return;
    chunks.push({ blob: new Blob([pending.join('\n')], { type: 'text/plain' }), lines: pending.length, widths: Uint8Array.from(widths, value => Math.min(255, value)) });
    pending = []; widths = [];
    let kept = chunks.reduce((sum, chunk) => sum + chunk.lines, 0);
    while (kept > maxLines && chunks.length > 1) { const old = chunks.shift()!; kept -= old.lines; dropped += old.lines; }
  };
  return {
    /** A wrapped continuation joins its line, so the viewer wraps it at its own width. */
    add(line: Staged) {
      if (line.wrapped && pending.length) { pending[pending.length - 1] += line.text; widths[widths.length - 1] += line.width; return; }
      pending.push(line.text); widths.push(line.width);
      if (pending.length >= chunkLines) flush();
    },
    miss(count: number) { missed += count; },
    /** Lines held, in order: whole chunks, then those not yet in a Blob. */
    lines: () => chunks.reduce((sum, chunk) => sum + chunk.lines, 0) + pending.length,
    dropped: () => dropped, missed: () => missed,
    /** Each held line's width in cells, capped at 255, for the minimap. */
    width(index: number): number {
      for (const chunk of chunks) { if (index < chunk.lines) return chunk.widths[index]; index -= chunk.lines; }
      return Math.min(255, widths[index] ?? 0);
    },
    /** Held lines [from, to), read back from their Blobs. */
    async read(from: number, to: number): Promise<string[]> {
      const out: string[] = []; let start = 0;
      for (const chunk of chunks) {
        const end = start + chunk.lines;
        if (end > from && start < to) {
          const lines = (await chunk.blob.text()).split('\n');
          out.push(...lines.slice(Math.max(0, from - start), Math.min(chunk.lines, to - start)));
        }
        start = end;
      }
      if (to > start) out.push(...pending.slice(Math.max(0, from - start), to - start));
      return out;
    },
    clear() { chunks.length = 0; pending = []; widths = []; },
  };
}
export type Archive = ReturnType<typeof createArchive>;

export function createTerminalHistory(terminal: Terminal) {
  const archive = createArchive();
  const cell = terminal.buffer.normal.getNullCell();
  /** Copies of the lines above the screen, from absolute line `stagedFrom`. */
  let staged: Staged[] = []; let stagedFrom = 0;
  /** A marker on the cursor line, to count the lines xterm drops between two captures. */
  let anchor: IMarker | undefined; let anchorAt = 0;
  let top = 0; let lastLength = 0;
  const listeners = new Set<() => void>();
  const reanchor = () => {
    anchor?.dispose(); anchor = undefined;
    if (terminal.buffer.active.type !== 'normal') return;
    const buffer = terminal.buffer.normal;
    anchor = terminal.registerMarker(0); anchorAt = top + buffer.baseY + buffer.cursorY;
  };
  const changed = () => { for (const listener of listeners) listener(); };
  return {
    archive,
    /** After written output: archive what xterm dropped, copy what scrolled up. */
    capture() {
      if (terminal.buffer.active.type !== 'normal') return;
      const buffer = terminal.buffer.normal; const before = archive.lines();
      if (anchor && !anchor.isDisposed && anchor.line >= 0) {
        const nextTop = anchorAt - anchor.line;
        if (nextTop > top) {
          // Dropped at the limit: the buffer stayed full. Cleared: it shrank, nothing is kept.
          const gone = nextTop - top;
          if (buffer.length >= lastLength) {
            // stagedFrom >= top always holds, so the staged lines dropped come first.
            const kept = Math.max(0, Math.min(nextTop, stagedFrom + staged.length) - stagedFrom);
            for (const line of staged.slice(0, kept)) archive.add(line);
            if (gone > kept) archive.miss(gone - kept);
          }
          staged = staged.slice(Math.max(0, nextTop - stagedFrom)); stagedFrom = Math.max(stagedFrom, nextTop);
          top = nextTop;
        }
      } else if (anchor) {
        // The cursor line itself is gone: more output than the whole buffer in one write
        // (everything copied was dropped, and some lines were never seen), or a reset.
        if (buffer.length >= lastLength) { for (const line of staged) archive.add(line); archive.miss(1); }
        staged = []; stagedFrom = top;
      }
      let index = stagedFrom + staged.length - top;
      if (index < 0) { staged = []; stagedFrom = top; index = 0; }
      for (; index < buffer.baseY; index++) {
        const line = buffer.getLine(index); if (!line) break;
        staged.push(serializeLine(line, terminal.cols, cell));
      }
      lastLength = buffer.length; reanchor();
      if (archive.lines() !== before) changed();
    },
    /** After a reflow: the copies no longer match the buffer's lines. */
    reflowed() { staged = []; stagedFrom = top; lastLength = terminal.buffer.normal.length; reanchor(); },
    onChange(listener: () => void) { listeners.add(listener); return () => listeners.delete(listener); },
    dispose() { anchor?.dispose(); staged = []; archive.clear(); listeners.clear(); },
  };
}
export type TerminalHistory = ReturnType<typeof createTerminalHistory>;
