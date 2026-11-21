import { uiButton, uiPart } from '@acaprino/imago/ui';
/** The archived lines of one terminal, shown in its own pane over the live screen: a read-only
 * terminal holding a window of `WINDOW_LINES` around the line asked for. "Earlier" and "Later"
 * move the window; Esc or "Back to live" closes it and frees everything it held. The live
 * terminal keeps running and receiving output underneath. */
import { Terminal, type ITerminalOptions } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WINDOW_LINES, windowStart, type TerminalHistory } from './terminal-history.ts';

/** How far one step moves the window: it keeps a quarter of what was shown, for context. */
const STEP = Math.round(WINDOW_LINES * 0.75);

export function openTerminalHistory(options: {
  host: HTMLElement; history: TerminalHistory; line: number;
  terminalOptions: ITerminalOptions; background: string;
  /** Called once it has closed, to give the keyboard back to the live terminal. */
  closed(): void;
}) {
  const { host, history } = options;
  host.querySelector('.terminal-history')?.dispatchEvent(new Event('history-close'));
  const panel = uiPart('region'); panel.classList.add('terminal-history');
  panel.setAttribute('aria-label', 'Earlier terminal lines'); panel.style.background = options.background;
  const bar = uiPart('toolbar'); bar.classList.add('terminal-history-bar');
  const title = uiPart('status'); title.classList.add('terminal-history-title'); title.setAttribute('role', 'status');
  const earlier = uiButton('Earlier'); const later = uiButton('Later'); const live = uiButton('Back to live', { primary: true });
  bar.setAttribute('aria-label', 'Terminal history'); bar.append(title, earlier, later, live);
  const screen = document.createElement('div'); screen.className = 'terminal-history-screen';
  panel.append(bar, screen); host.append(panel);
  const terminal = new Terminal({ ...options.terminalOptions, disableStdin: true, cursorBlink: false,
    cursorInactiveStyle: 'none', scrollback: WINDOW_LINES, screenReaderMode: true });
  const fit = new FitAddon(); terminal.loadAddon(fit); terminal.open(screen);
  for (const osc of [0, 1, 2, 7, 8, 9, 52, 777]) terminal.parser.registerOscHandler(osc, () => true);
  // xterm keeps the keys it reads; Esc is taken before it does.
  terminal.attachCustomKeyEventHandler(event => {
    if (event.type === 'keydown' && event.key === 'Escape') { close(); return false; }
    return true;
  });
  let start = 0; let shown = 0; let revision = 0; let closed = false;

  async function show(line: number, anchor: 'top' | 'bottom' | 'line') {
    const total = history.archive.lines(); const current = ++revision;
    start = windowStart(line, total); const end = Math.min(total, start + WINDOW_LINES);
    earlier.disabled = later.disabled = true;
    const lines = await history.archive.read(start, end);
    if (closed || current !== revision) return;
    shown = lines.length;
    fit.fit(); terminal.reset();
    await new Promise<void>(resolve => terminal.write('\x1b[?25l' + lines.join('\r\n') + '\x1b[0m', resolve));
    if (closed || current !== revision) return;
    const lost = history.archive.dropped() + history.archive.missed();
    title.textContent = `Earlier lines ${(start + 1).toLocaleString()}–${(start + shown).toLocaleString()} of ${total.toLocaleString()}${lost ? ` · ${lost.toLocaleString()} older not kept` : ''}`;
    earlier.disabled = start === 0; later.disabled = start + shown >= total;
    if (anchor === 'top') terminal.scrollToTop();
    else if (anchor === 'bottom') terminal.scrollToBottom();
    else terminal.scrollToLine(Math.max(0, line - start - Math.floor(terminal.rows / 2)));
  }
  earlier.onclick = () => void show(start - STEP + Math.floor(WINDOW_LINES / 2), 'bottom');
  later.onclick = () => void show(start + STEP + Math.floor(WINDOW_LINES / 2), 'top');
  function close() {
    if (closed) return; closed = true;
    observer.disconnect(); terminal.dispose(); panel.remove(); options.closed();
  }
  live.onclick = close;
  panel.addEventListener('history-close', close);
  panel.addEventListener('keydown', event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); } });
  const observer = new ResizeObserver(() => { if (!closed) fit.fit(); }); observer.observe(screen);
  void show(options.line, 'line').then(() => { if (!closed) terminal.focus(); });
  return { close };
}
