import type { Terminal } from '@xterm/xterm';
import { openModal } from './modal.ts';

/** Searches the existing bounded emulator buffer. It never pauses its parser. */
export function createTerminalFind(options: {
  current(): { id: string; terminal: Terminal } | null;
  returnFocus(id: string): void;
  notify(message: string): void;
}) {
  const dialog = document.createElement('dialog'); dialog.id = 'terminal-find-dialog';
  dialog.setAttribute('aria-labelledby', 'terminal-find-title');
  dialog.innerHTML = '<form><div class="dialog-heading"><h1 id="terminal-find-title">Find in terminal</h1><button type="button" aria-label="Close search">×</button></div><label for="terminal-find-query">Text in this terminal</label><input id="terminal-find-query" type="search" autocomplete="off" maxlength="128"><p class="hint" role="status"></p><div class="dialog-actions"><button type="button" data-previous>Previous</button><button type="submit">Next</button></div></form>';
  document.querySelector('main')!.append(dialog);
  const query = dialog.querySelector<HTMLInputElement>('input')!;
  const status = dialog.querySelector<HTMLElement>('[role="status"]')!;
  const previous = dialog.querySelector<HTMLButtonElement>('[data-previous]')!;
  const next = dialog.querySelector<HTMLButtonElement>('[type="submit"]')!;
  let origin: ReturnType<typeof options.current> = null;
  let generation = 0; let frame = 0; let hits: number[] = []; let selected = -1; let overflow = false;
  function select(direction: number) {
    if (!origin || !hits.length) return;
    selected = (selected + direction + hits.length) % hits.length;
    const row = hits[selected];
    try { origin.terminal.select(0, row, origin.terminal.cols); origin.terminal.scrollToLine(row); }
    catch { return; }
    status.textContent = `${selected + 1} of ${hits.length}${overflow ? '+' : ''} matching lines`;
  }
  function search() {
    const token = ++generation; cancelAnimationFrame(frame);
    hits = []; selected = -1; overflow = false; previous.disabled = next.disabled = true;
    const text = query.value.toLocaleLowerCase();
    if (!origin || !text) { status.textContent = 'Search the terminal’s retained screen and scrollback.'; return; }
    const buffer = origin.terminal.buffer.active;
    let row = Math.max(0, buffer.length - 5500);
    const end = buffer.length;
    status.textContent = 'Searching…';
    const scan = () => {
      if (token !== generation || !dialog.open) return;
      const until = Math.min(end, row + 32);
      for (; row < until; row++) {
        const line = buffer.getLine(row)?.translateToString(true).toLocaleLowerCase();
        if (line?.includes(text)) { hits.push(row); if (hits.length === 200) { overflow = row + 1 < end; break; } }
      }
      if (row < end && hits.length < 200) { frame = requestAnimationFrame(scan); return; }
      previous.disabled = next.disabled = !hits.length;
      if (hits.length) select(1); else status.textContent = 'No matching lines in the retained buffer.';
    };
    frame = requestAnimationFrame(scan);
  }
  query.addEventListener('input', search);
  dialog.querySelector('form')!.onsubmit = event => { event.preventDefault(); select(1); };
  previous.onclick = () => select(-1);
  dialog.querySelector<HTMLButtonElement>('[aria-label="Close search"]')!.onclick = () => dialog.close();
  dialog.addEventListener('close', () => {
    ++generation; cancelAnimationFrame(frame);
    const id = origin?.id; origin = null; hits = [];
    if (id) queueMicrotask(() => options.returnFocus(id));
  });
  return {
    open() {
      if (document.querySelector('dialog[open]')) return;
      origin = options.current();
      if (!origin) { options.notify('Open or select a terminal to search its retained output.'); return; }
      openModal(dialog); query.focus(); query.select(); search();
    },
  };
}
