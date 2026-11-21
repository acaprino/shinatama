import { uiButton } from '@acaprino/imago/ui';
/** The strip at a terminal's right edge, in place of its scrollbar: the scrollback in miniature.
 * Each line is a bar from its indent to its end (80 cells fill the strip), a few pixels apart while
 * the scrollback is short and one pixel row for several lines once it is longer than the strip; a
 * frame marks the lines on screen. Once lines have been archived, the archive takes the top part,
 * dimmer.
 * A click on the live part scrolls the terminal there, the frame centered on the pointer; a drag
 * that starts on the frame moves it without a jump. A click on the archive, or on "Earlier lines"
 * at the top of the scrollback, opens the history view at that line. */
import type { Terminal } from '@xterm/xterm';
import type { TerminalHistory } from './terminal-history.ts';

/** The share of the strip the archive takes at most, however long it is. */
const ARCHIVE_SHARE = 0.3;
/** Pixels per line at most: a short scrollback reads as lines, not as stretched blocks. */
export const LINE_PITCH = 3;
/** A line this many cells long fills the strip's width, however wide the terminal is: scaled to a
 * 200-column terminal, a prompt would be a few pixels wide and the strip would look empty. */
export const MINIATURE_COLUMNS = 80;
/** The frame never gets thinner than this, so it stays visible over a long scrollback. */
const MIN_FRAME = 6;
const REDRAW_MS = 200;

export type MinimapGeometry = {
  /** Pixels from the top: the archive, then the live lines at `pitch` pixels each, then nothing. */
  archiveHeight: number; liveHeight: number; pitch: number;
  /** The frame around the lines on screen. */
  frameTop: number; frameHeight: number;
};
/** Where the strip's parts lie, in pixels from its top. Pure, so it can be tested. */
export function minimapGeometry(height: number, archived: number, live: number, viewportY: number, rows: number): MinimapGeometry {
  const archiveHeight = archived > 0 ? Math.min(Math.round(height * ARCHIVE_SHARE), Math.ceil(archived * LINE_PITCH)) : 0;
  const liveHeight = Math.max(1, height - archiveHeight);
  const lines = Math.max(1, live);
  const pitch = Math.min(LINE_PITCH, liveHeight / lines);
  const frameHeight = Math.min(liveHeight, Math.max(MIN_FRAME, Math.round(Math.min(rows, lines) * pitch)));
  const frameTop = Math.max(archiveHeight, Math.min(height - frameHeight, archiveHeight + Math.round(viewportY * pitch)));
  return { archiveHeight, liveHeight, pitch, frameTop, frameHeight };
}
/** The live line a pointer at `y` puts at the top of the screen; `hold` is how many lines below
 * the frame's top the pointer holds it (its middle for a click outside it). */
export function lineAt(y: number, geometry: MinimapGeometry, live: number, rows: number, hold = geometry.frameHeight / 2 / geometry.pitch): number {
  const line = Math.round((y - geometry.archiveHeight) / geometry.pitch - hold);
  return Math.max(0, Math.min(Math.max(0, live - rows), line));
}
/** Where a pointer at `y` holds a frame whose top line is `viewportY`, in lines below that top. */
export function holdAt(y: number, geometry: MinimapGeometry, viewportY: number): number {
  return (y - geometry.archiveHeight) / geometry.pitch - viewportY;
}
/** The archived line a pointer at `y` in the archive part stands for. */
export function archivedLineAt(y: number, geometry: MinimapGeometry, archived: number): number {
  return Math.max(0, Math.min(archived - 1, Math.floor(y / Math.max(1, geometry.archiveHeight) * archived)));
}

/** Paints `count` lines from `top`, `pitch` pixels each, as bars from `extent(index)` = [first, end)
 * cell over `cols` cells. Below one pixel a line, each pixel row shows the first of its lines, so the
 * cost is the strip's height, not the history's length. */
function paintLines(context: CanvasRenderingContext2D, top: number, pitch: number, count: number, width: number, cols: number,
  extent: (index: number) => readonly [number, number] | null) {
  const bar = (y: number, height: number, index: number) => {
    const found = extent(index); if (!found || found[1] <= found[0]) return;
    const left = Math.min(width - 1, found[0] / cols * width);
    context.fillRect(Math.floor(left), y, Math.max(1, Math.ceil(Math.min(1, found[1] / cols) * width - left)), height);
  };
  if (pitch >= 1) {
    // A gap of one pixel between lines once there is room for it.
    const height = pitch >= 2 ? Math.max(1, Math.floor(pitch) - 1) : 1;
    for (let index = 0; index < count; index++) bar(Math.floor(top + index * pitch), height, index);
  } else {
    const rows = Math.ceil(count * pitch);
    for (let row = 0; row < rows; row++) bar(top + row, 1, Math.min(count - 1, Math.floor(row / pitch)));
  }
}

export function createTerminalMinimap(options: {
  host: HTMLElement; terminal: Terminal; history: TerminalHistory;
  /** Opens the history view at an archived line. */
  openHistory(line: number): void;
  /** False while the terminal is not on screen: nothing is drawn. */
  shown(): boolean;
}) {
  const { host, terminal, history } = options;
  const strip = document.createElement('div'); strip.className = 'terminal-minimap';
  strip.setAttribute('aria-hidden', 'true');
  const canvas = document.createElement('canvas');
  // The archive's band (its color is the archive's ink) and the frame are elements, so the skin's
  // tokens style them; the canvas only holds the lines.
  const archiveBand = document.createElement('div'); archiveBand.className = 'terminal-minimap-archive'; archiveBand.hidden = true;
  const frame = document.createElement('div'); frame.className = 'terminal-minimap-view';
  strip.append(archiveBand, canvas, frame);
  const earlier = uiButton('Earlier', { sm: true, cls: 'terminal-earlier' }); earlier.hidden = true;
  earlier.onclick = () => options.openHistory(Math.max(0, history.archive.lines() - 1));
  host.classList.add('has-minimap'); host.append(strip, earlier);
  let timer = 0; let disposed = false;
  /** While a drag lasts: where the pointer holds the frame, in lines below its top. */
  let hold: number | null = null;

  const geometry = () => {
    const buffer = terminal.buffer.active;
    return minimapGeometry(strip.clientHeight, history.archive.lines(), buffer.length, buffer.viewportY, terminal.rows);
  };
  /** The frame follows the viewport at once; the lines are repainted at most every REDRAW_MS. */
  function place() {
    if (disposed || strip.clientHeight < 8) return;
    const { frameTop, frameHeight } = geometry();
    frame.style.top = `${frameTop}px`; frame.style.height = `${frameHeight}px`;
    const buffer = terminal.buffer.active; const archived = history.archive.lines();
    earlier.hidden = !(archived > 0 && buffer.type === 'normal' && buffer.viewportY === 0 && buffer.baseY > 0);
    if (!earlier.hidden) earlier.textContent = `↑ Earlier lines · ${archived.toLocaleString()}`;
  }
  function draw() {
    timer = 0;
    if (disposed || !options.shown()) return;
    // Parked or not laid out yet: the ResizeObserver calls again once the strip has a size.
    const width = canvas.clientWidth; const height = canvas.clientHeight; if (width < 2 || height < 8) return;
    const ratio = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) { canvas.width = Math.round(width * ratio); canvas.height = Math.round(height * ratio); }
    const context = canvas.getContext('2d'); if (!context) return;
    context.setTransform(ratio, 0, 0, ratio, 0, 0); context.clearRect(0, 0, width, height);
    const buffer = terminal.buffer.active; const archived = history.archive.lines();
    const shape = minimapGeometry(height, archived, buffer.length, buffer.viewportY, terminal.rows);
    const cols = Math.max(1, Math.min(terminal.cols, MINIATURE_COLUMNS));
    archiveBand.hidden = !shape.archiveHeight; archiveBand.style.height = `${shape.archiveHeight}px`;
    if (shape.archiveHeight) {
      // Computed colors are plain rgb(): the canvas reads them whatever the skin wrote.
      context.globalAlpha = 0.6; context.fillStyle = getComputedStyle(archiveBand).color;
      paintLines(context, 0, shape.archiveHeight / archived, archived, width, cols, index => {
        const cells = history.archive.width(index); return cells ? [0, cells] : null;
      });
    }
    context.globalAlpha = 1; context.fillStyle = getComputedStyle(strip).color;
    paintLines(context, shape.archiveHeight, shape.pitch, buffer.length, width, cols, index => {
      const text = buffer.getLine(index)?.translateToString(true) ?? '';
      return text ? [text.length - text.trimStart().length, text.length] : null;
    });
    place();
  }
  /** At most one painting per interval, however fast the output arrives. */
  const schedule = () => { if (!timer && !disposed) timer = window.setTimeout(() => requestAnimationFrame(draw), REDRAW_MS); };
  const now = () => { clearTimeout(timer); timer = 0; requestAnimationFrame(draw); };

  function scrollTo(event: PointerEvent) {
    const box = strip.getBoundingClientRect(); const buffer = terminal.buffer.active;
    terminal.scrollToLine(lineAt(event.clientY - box.top, geometry(), buffer.length, terminal.rows, hold ?? undefined)); place();
  }
  strip.addEventListener('pointerdown', event => {
    if (event.button !== 0) return;
    event.preventDefault();
    const y = event.clientY - strip.getBoundingClientRect().top; const shape = geometry(); const archived = history.archive.lines();
    if (y < shape.archiveHeight) { options.openHistory(archivedLineAt(y, shape, archived)); return; }
    // On the frame, the drag holds it where it was taken; elsewhere the frame jumps to the pointer.
    const onFrame = y >= shape.frameTop && y < shape.frameTop + shape.frameHeight;
    hold = onFrame ? holdAt(y, shape, terminal.buffer.active.viewportY) : shape.frameHeight / 2 / shape.pitch;
    strip.classList.add('dragging'); strip.setPointerCapture(event.pointerId); scrollTo(event);
  });
  strip.addEventListener('pointermove', event => { if (hold !== null) scrollTo(event); });
  strip.addEventListener('wheel', event => { event.preventDefault(); terminal.scrollLines(Math.sign(event.deltaY) * 3); }, { passive: false });
  const release = (focus: boolean) => { if (hold === null) return; hold = null; strip.classList.remove('dragging'); if (focus) terminal.focus(); };
  strip.addEventListener('pointerup', () => release(true)); strip.addEventListener('pointercancel', () => release(false));

  const subscriptions = [terminal.onScroll(() => { place(); schedule(); }), terminal.onWriteParsed(schedule), terminal.onResize(now), terminal.buffer.onBufferChange(now)];
  const unsubscribe = history.onChange(schedule);
  const observer = new ResizeObserver(now); observer.observe(strip);
  // A skin writes its tokens on the root element: the lines take its ink at the next painting.
  const skin = new MutationObserver(schedule); skin.observe(document.documentElement, { attributes: true, attributeFilter: ['style', 'class'] });
  now();
  return {
    redraw: now,
    dispose() {
      disposed = true; clearTimeout(timer); observer.disconnect(); skin.disconnect(); unsubscribe();
      for (const subscription of subscriptions) subscription.dispose();
      strip.remove(); earlier.remove(); host.classList.remove('has-minimap');
    },
  };
}
export type TerminalMinimap = ReturnType<typeof createTerminalMinimap>;
