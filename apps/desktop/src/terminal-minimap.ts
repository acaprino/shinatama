/** The strip at a terminal's right edge, in place of its scrollbar: one row of pixels per few
 * lines, each as long as its text. The live buffer takes the strip; once lines have been
 * archived, the archive takes its top part, dimmer. Drag on the live part to scroll; a click on
 * the archive, or on "Earlier lines" at the top of the scrollback, opens the history view. */
import type { Terminal } from '@xterm/xterm';
import type { TerminalHistory } from './terminal-history.ts';

/** The share of the strip the archive takes, however long it is. */
const ARCHIVE_SHARE = 0.3;
const REDRAW_MS = 200;

/** Where the strip's parts lie, in pixels from its top. Pure, so it can be tested. */
export function minimapGeometry(height: number, archived: number, live: number, viewportY: number, rows: number) {
  const archiveHeight = archived > 0 ? Math.round(height * ARCHIVE_SHARE) : 0;
  const liveHeight = Math.max(1, height - archiveHeight);
  const lines = Math.max(1, live);
  const thumbTop = archiveHeight + Math.floor(viewportY / lines * liveHeight);
  const thumbHeight = Math.max(6, Math.ceil(Math.min(rows, lines) / lines * liveHeight));
  return { archiveHeight, liveHeight, thumbTop, thumbHeight: Math.min(thumbHeight, height - thumbTop) };
}
/** The live line a pointer at `y` puts at the top of the screen, the thumb centered on it. */
export function lineAt(y: number, geometry: ReturnType<typeof minimapGeometry>, live: number, rows: number): number {
  const fraction = (y - geometry.archiveHeight - geometry.thumbHeight / 2) / geometry.liveHeight;
  return Math.max(0, Math.min(Math.max(0, live - rows), Math.round(fraction * live)));
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
  const canvas = document.createElement('canvas'); strip.append(canvas);
  const earlier = document.createElement('button'); earlier.type = 'button'; earlier.className = 'terminal-earlier'; earlier.hidden = true;
  earlier.onclick = () => options.openHistory(Math.max(0, history.archive.lines() - 1));
  host.classList.add('has-minimap'); host.append(strip, earlier);
  const tokens = getComputedStyle(document.documentElement);
  const token = (name: string, fallback: string) => tokens.getPropertyValue(name).trim() || fallback;
  const ink = { live: token('--ice', '#a9c8ff'), archive: token('--muted', '#8fa6c9'), thumb: token('--line-strong', '#4a78e6'), edge: token('--accent', '#f48fb8') };
  let timer = 0; let disposed = false; let dragging = false;

  function draw() {
    timer = 0;
    if (disposed || !options.shown()) return;
    const ratio = window.devicePixelRatio || 1;
    const width = strip.clientWidth; const height = strip.clientHeight; if (width < 2 || height < 8) return;
    if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) { canvas.width = Math.round(width * ratio); canvas.height = Math.round(height * ratio); }
    const context = canvas.getContext('2d'); if (!context) return;
    context.setTransform(ratio, 0, 0, ratio, 0, 0); context.clearRect(0, 0, width, height);
    const buffer = terminal.buffer.active; const archived = history.archive.lines();
    const geometry = minimapGeometry(height, archived, buffer.length, buffer.viewportY, terminal.rows);
    const cols = Math.max(1, terminal.cols);
    // One sampled line per pixel row: the drawing costs the strip's height, not the history's length.
    context.globalAlpha = 0.45; context.fillStyle = ink.archive;
    for (let y = 0; y < geometry.archiveHeight; y++) {
      const cells = history.archive.width(Math.floor(y / geometry.archiveHeight * archived));
      if (cells) context.fillRect(0, y, Math.max(1, Math.min(1, cells / cols) * width), 1);
    }
    context.globalAlpha = 0.7; context.fillStyle = ink.live;
    for (let y = 0; y < geometry.liveHeight; y++) {
      const line = buffer.getLine(Math.floor(y / geometry.liveHeight * buffer.length));
      const cells = line ? line.translateToString(true).length : 0;
      if (cells) context.fillRect(0, geometry.archiveHeight + y, Math.max(1, Math.min(1, cells / cols) * width), 1);
    }
    if (geometry.archiveHeight) { context.globalAlpha = 1; context.fillStyle = ink.edge; context.fillRect(0, geometry.archiveHeight - 1, width, 1); }
    context.globalAlpha = 0.28; context.fillStyle = ink.thumb; context.fillRect(0, geometry.thumbTop, width, geometry.thumbHeight);
    context.globalAlpha = 1; context.strokeStyle = ink.thumb; context.lineWidth = 1;
    context.strokeRect(0.5, geometry.thumbTop + 0.5, width - 1, Math.max(1, geometry.thumbHeight - 1));
    earlier.hidden = !(archived > 0 && buffer.type === 'normal' && buffer.viewportY === 0 && buffer.baseY > 0);
    earlier.textContent = `↑ Earlier lines · ${archived.toLocaleString()}`;
  }
  /** At most one drawing per interval, however fast the output arrives. */
  const schedule = () => { if (!timer && !disposed) timer = window.setTimeout(() => requestAnimationFrame(draw), REDRAW_MS); };
  const now = () => { clearTimeout(timer); timer = 0; requestAnimationFrame(draw); };

  function scrollTo(event: PointerEvent) {
    const box = strip.getBoundingClientRect(); const buffer = terminal.buffer.active;
    const geometry = minimapGeometry(box.height, history.archive.lines(), buffer.length, buffer.viewportY, terminal.rows);
    terminal.scrollToLine(lineAt(event.clientY - box.top, geometry, buffer.length, terminal.rows)); now();
  }
  strip.addEventListener('pointerdown', event => {
    if (event.button !== 0) return;
    event.preventDefault();
    const box = strip.getBoundingClientRect(); const y = event.clientY - box.top; const archived = history.archive.lines();
    const geometry = minimapGeometry(box.height, archived, terminal.buffer.active.length, 0, terminal.rows);
    if (y < geometry.archiveHeight) { options.openHistory(Math.min(archived - 1, Math.floor(y / geometry.archiveHeight * archived))); return; }
    dragging = true; strip.setPointerCapture(event.pointerId); scrollTo(event);
  });
  strip.addEventListener('pointermove', event => { if (dragging) scrollTo(event); });
  strip.addEventListener('wheel', event => { event.preventDefault(); terminal.scrollLines(Math.sign(event.deltaY) * 3); }, { passive: false });
  const release = () => { dragging = false; terminal.focus(); };
  strip.addEventListener('pointerup', release); strip.addEventListener('pointercancel', () => { dragging = false; });

  const subscriptions = [terminal.onScroll(schedule), terminal.onWriteParsed(schedule), terminal.onResize(now), terminal.buffer.onBufferChange(now)];
  const unsubscribe = history.onChange(schedule);
  const observer = new ResizeObserver(now); observer.observe(strip);
  now();
  return {
    redraw: now,
    dispose() {
      disposed = true; clearTimeout(timer); observer.disconnect(); unsubscribe();
      for (const subscription of subscriptions) subscription.dispose();
      strip.remove(); earlier.remove(); host.classList.remove('has-minimap');
    },
  };
}
export type TerminalMinimap = ReturnType<typeof createTerminalMinimap>;
