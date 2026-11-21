import { componentElement, textFieldHtml, uiButton, uiPart } from '@acaprino/imago/ui';
import type { Terminal } from '@xterm/xterm';
import { openModal, openSheet } from './modal.ts';

/** Searches the existing bounded emulator buffer. It never pauses its parser. */
export function createTerminalFind(options: {
  current(): { id: string; terminal: Terminal } | null;
  returnFocus(id: string): void;
  notify(message: string): void;
}) {
  const dialog = uiPart('dialog'); dialog.id = 'terminal-find-dialog';
  dialog.setAttribute('aria-labelledby', 'terminal-find-title');
  const form = uiPart('form');
  const heading = uiPart('header');
  const title = uiPart('title', 'Find in terminal'); title.id = 'terminal-find-title';
  const close = uiButton('×', { attrs: { 'aria-label': 'Close search' } });
  heading.append(title, close);
  const field = componentElement(textFieldHtml('Text in this terminal', 'terminal-find', '', {
    id: 'terminal-find-query', type: 'search', error: false, attrs: { autocomplete: 'off', maxlength: '128' },
  }));
  const query = field.querySelector('input')!;
  const status = uiPart('status');
  const actions = uiPart('actions');
  const previous = uiButton('Previous');
  const next = uiButton('Next', { primary: true }); next.type = 'submit';
  actions.append(previous, next);
  form.append(heading, field, status, actions);
  dialog.append(form);
  (document.querySelector('#app') ?? document.body).append(dialog);
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
  form.onsubmit = event => { event.preventDefault(); select(1); };
  previous.onclick = () => select(-1);
  close.onclick = () => dialog.close();
  dialog.addEventListener('close', () => {
    if (dialog.open) return;
    ++generation; cancelAnimationFrame(frame);
    const id = origin?.id; origin = null; hits = [];
    if (id) queueMicrotask(() => options.returnFocus(id));
  });
  return {
    open() {
      if (dialog.open || document.querySelector('dialog[open][aria-modal="true"]')) return;
      origin = options.current();
      if (!origin) { options.notify('Open or select a terminal to search its retained output.'); return; }
      const pane = origin.terminal.element?.closest('.im-tile-body') ?? null;
      if (!openSheet(dialog, { anchor: () => pane?.isConnected ? pane : null, placement: 'pane-top' })) openModal(dialog);
      query.focus(); query.select(); search();
    },
  };
}
