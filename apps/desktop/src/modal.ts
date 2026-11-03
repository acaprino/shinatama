// A native showModal() makes the custom titlebar inert and covers it, which leaves a
// borderless window with no way to move, minimize or close. Only the content below the
// titlebar becomes inert here; containment and Escape are provided explicitly instead.
const INERT_WHILE_OPEN = '.toolbar,#work-area,#workspace,.statusbar';
const FOCUSABLE = 'a[href],button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex]:not([tabindex="-1"])';

type Entry = { dialog: HTMLDialogElement; restore: HTMLElement | null };
let stack: Entry[] = [];

function sync() {
  const top = stack.at(-1)?.dialog;
  for (const element of document.querySelectorAll<HTMLElement>(INERT_WHILE_OPEN)) element.inert = !!top;
  for (const [index, { dialog }] of stack.entries()) {
    dialog.inert = dialog !== top;
    // Opening order, rather than document creation order, determines which
    // custom dialog is visible. The borderless frame remains available above.
    dialog.style.zIndex = String(20 + index);
  }
  document.querySelector<HTMLElement>('#modal-scrim')!.hidden = !top;
}

export function openModal(dialog: HTMLDialogElement) {
  if (dialog.open) return;
  // A closed dialog's event may still be queued when another one replaces it.
  const replaced = stack.filter(entry => !entry.dialog.open).at(-1);
  stack = stack.filter(entry => entry.dialog.open);
  const active = document.activeElement;
  const restore = active instanceof HTMLElement && active !== document.body ? active : replaced?.restore ?? null;
  stack.push({ dialog, restore });
  sync();
  dialog.setAttribute('aria-modal', 'true');
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
    if (entry.restore?.isConnected && !entry.restore.closest('[inert]')) entry.restore.focus();
  });
  released.observe(dialog, { attributes: true, attributeFilter: ['open'] });
}

document.addEventListener('keydown', event => {
  const top = stack.at(-1)?.dialog;
  if (!top?.open) return;
  if (event.key === 'Escape') {
    event.preventDefault();
    if (top.dispatchEvent(new Event('cancel', { cancelable: true }))) top.close();
    return;
  }
  if (event.key !== 'Tab') return;
  const items = [...top.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(element => element.getClientRects().length > 0);
  const active = document.activeElement;
  const edge = event.shiftKey ? items[0] : items.at(-1);
  if (!items.length || !top.contains(active) || active === edge) {
    event.preventDefault();
    (event.shiftKey ? items.at(-1) : items[0])?.focus();
  }
});
