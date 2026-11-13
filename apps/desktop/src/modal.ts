// Three ways to show a <dialog>, none of them a native showModal(): that makes the custom
// titlebar inert and covers it, which leaves a borderless window with no way to move,
// minimize or close.
//
//   drawer  The default for openModal(). Docked along the right edge of the work area, no
//           scrim, nothing inert: terminals keep working beside it. One at a time, except
//           that a dialog opened from inside a drawer stacks over it and returns to it.
//   modal   Only for decisions about the whole workspace (MODAL_ONLY). Everything below the
//           titlebar becomes inert; containment and Escape are provided explicitly.
//   sheet   openSheet(). Docked to the control or pane it belongs to; a press outside
//           dismisses it the way Escape does. At most one sheet is open at a time.
const MODAL_ONLY = '#confirm-dialog,#hop-preview-dialog';
const INERT_WHILE_OPEN = '.toolbar,#work-area,#workspace,.statusbar';
const FOCUSABLE = 'a[href],button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex]:not([tabindex="-1"])';

type Entry = { dialog: HTMLDialogElement; restore: HTMLElement | null; modal: boolean };
let stack: Entry[] = [];

export type SheetPlacement = 'below' | 'pane-top' | 'pane-bottom';
type SheetOptions = {
  /** The element the sheet is docked to; read again on every frame so layout changes follow. */
  anchor: () => Element | null;
  placement: SheetPlacement;
  /** Return false to keep the sheet open when the user presses outside it. */
  lightDismiss?: () => boolean;
};
type Sheet = SheetOptions & { dialog: HTMLDialogElement; restore: HTMLElement | null; frame: number; box: string };
let sheet: Sheet | null = null;

export function topModal(): HTMLDialogElement | null {
  return stack.filter(entry => entry.dialog.open).at(-1)?.dialog ?? null;
}

function sync() {
  const top = stack.at(-1)?.dialog;
  const blocking = stack.some(entry => entry.modal);
  for (const element of document.querySelectorAll<HTMLElement>(INERT_WHILE_OPEN)) element.inert = blocking;
  for (const [index, entry] of stack.entries()) {
    entry.dialog.inert = entry.dialog !== top;
    // Opening order, rather than document creation order, determines which
    // custom dialog is visible. The borderless frame remains available above.
    entry.dialog.style.zIndex = String((entry.modal ? 20 : 12) + index);
    // A drawer under a later drawer steps aside until that one closes.
    entry.dialog.toggleAttribute('data-covered', !entry.modal && stack.slice(index + 1).some(later => !later.modal));
  }
  // A modal raised over a sheet owns the input until it closes; the sheet stays where it was.
  if (sheet) sheet.dialog.inert = blocking;
  document.querySelector<HTMLElement>('#modal-scrim')!.hidden = !blocking;
}

/** Ask a dialog to close the way Escape does; its own cancel handler may refuse. */
function dismiss(dialog: HTMLDialogElement) {
  if (dialog.dispatchEvent(new Event('cancel', { cancelable: true }))) dialog.close();
}

export function openModal(dialog: HTMLDialogElement) {
  if (dialog.open) return;
  // A closed dialog's event may still be queued when another one replaces it.
  const replaced = stack.filter(entry => !entry.dialog.open).at(-1);
  stack = stack.filter(entry => entry.dialog.open);
  const active = document.activeElement;
  const modal = dialog.matches(MODAL_ONLY) || stack.some(entry => entry.modal);
  if (!modal && !stack.at(-1)?.dialog.contains(active)) {
    // Opened from the toolbar or a shortcut: it takes the drawer's place. A drawer that
    // refuses to close (work in flight) stays underneath and comes back afterwards.
    for (const entry of [...stack].reverse()) dismiss(entry.dialog);
    stack = stack.filter(entry => entry.dialog.open);
  }
  const restore = active instanceof HTMLElement && active !== document.body ? active : replaced?.restore ?? null;
  stack.push({ dialog, restore, modal });
  if (modal) { delete dialog.dataset.sheet; dialog.setAttribute('aria-modal', 'true'); }
  else { dialog.dataset.sheet = 'drawer'; dialog.removeAttribute('aria-modal'); }
  sync();
  dialog.show();
  // The close event is a later task. Releasing the page in the closing microtask, as a native
  // modal does, leaves no moment when input lands on a closed dialog's inert background.
  const released = new MutationObserver(() => {
    if (dialog.open) return;
    released.disconnect();
    const entry = stack.find(candidate => candidate.dialog === dialog);
    if (!entry) { sync(); return; }
    stack = stack.filter(candidate => candidate !== entry);
    sync();
    // A drawer never pulls focus back from wherever the user has gone on working.
    const focus = document.activeElement;
    const stranded = entry.modal || !focus || focus === document.body || dialog.contains(focus);
    if (stranded && entry.restore?.isConnected && !entry.restore.closest('[inert]')) entry.restore.focus();
  });
  released.observe(dialog, { attributes: true, attributeFilter: ['open'] });
}

function place(current: Sheet) {
  const target = current.anchor() ?? document.querySelector('#work-area');
  if (!target) return;
  const rect = target.getBoundingClientRect();
  const view = { width: document.documentElement.clientWidth, height: document.documentElement.clientHeight };
  const edge = 8; let left: number; let width: number | null = null; let top: number | null = null; let bottom: number | null = null; let room: number;
  if (current.placement === 'below') {
    const wanted = current.dialog.offsetWidth;
    left = Math.max(edge, Math.min(rect.left, view.width - wanted - edge));
    top = rect.bottom + 6; room = view.height - top - 36;
  } else if (current.placement === 'pane-top') {
    // Narrow panes cannot hold the sheet; it then overhangs the pane and stays inside the window.
    width = Math.min(view.width - edge * 2, Math.max(360, Math.min(rect.width - 24, 620)));
    left = Math.max(edge, Math.min(rect.left + (rect.width - width) / 2, view.width - width - edge));
    top = rect.top; room = Math.max(260, view.height - top - 36);
  } else {
    width = Math.max(240, rect.width); left = Math.max(0, Math.min(rect.left, view.width - width));
    bottom = Math.max(0, view.height - rect.bottom); room = Math.max(120, rect.height);
  }
  const box = [left, top, bottom, width, room].map(value => value === null ? 'auto' : `${Math.round(value)}px`).join(' ');
  if (box === current.box) return;
  current.box = box; const style = current.dialog.style; const [x, y, b, w, h] = box.split(' ');
  style.setProperty('--sheet-left', x); style.setProperty('--sheet-top', y); style.setProperty('--sheet-bottom', b);
  style.setProperty('--sheet-width', w); style.setProperty('--sheet-room', h);
}

/** Show a dialog in place. Returns false when a modal owns the page or the open sheet refuses to close. */
export function openSheet(dialog: HTMLDialogElement, options: SheetOptions): boolean {
  if (dialog.open) return sheet?.dialog === dialog;
  if (stack.some(entry => entry.modal && entry.dialog.open)) return false;
  if (sheet) {
    // The closing sheet is released here; its own observer runs a microtask later.
    const previous = sheet; dismiss(previous.dialog);
    if (previous.dialog.open) return false;
    cancelAnimationFrame(previous.frame); sheet = null;
  }
  const active = document.activeElement;
  const current: Sheet = { ...options, dialog, restore: active instanceof HTMLElement && active !== document.body ? active : null, frame: 0, box: '' };
  sheet = current;
  dialog.removeAttribute('aria-modal'); dialog.removeAttribute('data-covered'); dialog.dataset.sheet = options.placement; dialog.style.zIndex = '18';
  dialog.show(); place(current);
  // The anchor moves with splitters, the sidebar and window resizes; following it per frame
  // costs one rectangle read and writes only when the position changes.
  const follow = () => { if (sheet !== current) return; place(current); current.frame = requestAnimationFrame(follow); };
  current.frame = requestAnimationFrame(follow);
  const released = new MutationObserver(() => {
    if (dialog.open) return;
    released.disconnect(); cancelAnimationFrame(current.frame);
    if (sheet === current) sheet = null;
    const focus = document.activeElement;
    const stranded = !focus || focus === document.body || dialog.contains(focus);
    if (stranded && current.restore?.isConnected && !current.restore.closest('[inert]')) current.restore.focus();
  });
  released.observe(dialog, { attributes: true, attributeFilter: ['open'] });
  return true;
}

// The control that opened a drawer or a sheet closes it when pressed again, instead of opening it anew.
const opened = (restore: HTMLElement | null, target: Element) => restore instanceof HTMLButtonElement && !restore.closest('dialog') && restore.contains(target);
let closedByPress: HTMLElement | null = null;

// The window frame is not "outside": moving or maximizing the window keeps the sheet.
document.addEventListener('pointerdown', event => {
  closedByPress = null;
  const current = sheet; const target = event.target;
  if (!current?.dialog.open || current.dialog.inert || !(target instanceof Element)) return;
  if (current.dialog.contains(target) || target.closest('.titlebar')) return;
  if (current.lightDismiss && !current.lightDismiss()) return;
  dismiss(current.dialog);
  // The click that follows this press would otherwise reopen the sheet it has just closed.
  if (!current.dialog.open && opened(current.restore, target)) closedByPress = current.restore;
}, true);

document.addEventListener('click', event => {
  const target = event.target; const pressed = closedByPress; closedByPress = null;
  if (!(target instanceof Element)) return;
  const drawer = pressed?.contains(target) ? null : stack.find(entry => !entry.modal && entry.dialog.open && !entry.dialog.inert && opened(entry.restore, target));
  if (!pressed?.contains(target) && !drawer) return;
  event.preventDefault(); event.stopImmediatePropagation();
  if (drawer) dismiss(drawer.dialog);
}, true);

document.addEventListener('keydown', event => {
  const active = document.activeElement;
  if (event.key === 'Escape' && sheet?.dialog.open && !sheet.dialog.inert && sheet.dialog.contains(active)) {
    event.preventDefault(); dismiss(sheet.dialog); return;
  }
  const top = stack.at(-1);
  if (!top?.dialog.open) return;
  if (event.key === 'Escape') {
    // A terminal keeps its own Escape while a drawer is open beside it.
    if (!top.modal && active instanceof Element && active.closest('.xterm')) return;
    event.preventDefault();
    dismiss(top.dialog);
    return;
  }
  if (event.key !== 'Tab' || !top.modal) return;
  const items = [...top.dialog.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(element => element.getClientRects().length > 0);
  const edge = event.shiftKey ? items[0] : items.at(-1);
  if (!items.length || !top.dialog.contains(active) || active === edge) {
    event.preventDefault();
    (event.shiftKey ? items.at(-1) : items[0])?.focus();
  }
});
