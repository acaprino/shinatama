import { hasModal, openContentDialog, openDialogSheet, type SheetPlacement } from '@acaprino/imago/ui';

export type { SheetPlacement };
type Entry = { dialog: HTMLDialogElement; restore: HTMLElement | null; modal: boolean };
let stack: Entry[] = [];
const panels = new Set<() => void>();
const shell = () => document.querySelector<HTMLElement>('#app')!;

export function topModal(): HTMLDialogElement | null {
  return stack.filter(entry => entry.dialog.open).at(-1)?.dialog ?? null;
}

function sync() {
  stack = stack.filter(entry => entry.dialog.open);
  for (const [index, entry] of stack.entries()) {
    if (entry.modal) continue;
    entry.dialog.inert = index < stack.length - 1;
    entry.dialog.toggleAttribute('data-covered', stack.slice(index + 1).some(next => !next.modal));
  }
}

function dismiss(dialog: HTMLDialogElement) {
  if (dialog.dispatchEvent(new Event('cancel', { cancelable: true }))) dialog.close();
}

function closeDrawers() {
  for (const entry of [...stack].reverse()) if (!entry.modal && entry.dialog.open) dismiss(entry.dialog);
  sync();
}

export function sidePanel(close: () => void): () => void {
  panels.add(close);
  return () => {
    closeDrawers();
    for (const other of panels) if (other !== close) other();
  };
}

export function openModal(dialog: HTMLDialogElement) {
  if (dialog.open) return;
  const active = document.activeElement;
  const restore = active instanceof HTMLElement && active !== document.body ? active : stack.at(-1)?.restore ?? null;
  sync();
  const modal = dialog.matches('#confirm-dialog,#hop-preview-dialog') || hasModal();
  if (modal) {
    const home = dialog.parentElement ?? shell();
    const entry = { dialog, restore, modal };
    stack.push(entry);
    openContentDialog(shell(), dialog, {
      canDismiss() { dismiss(dialog); return !dialog.open; },
      onClose() { home.append(dialog); sync(); },
    });
    sync();
    return;
  }
  if (!stack.at(-1)?.dialog.contains(active)) closeDrawers();
  for (const close of panels) close();
  const entry = { dialog, restore, modal };
  stack.push(entry);
  dialog.classList.add('im-float', 'im-dialog', 'im-content-drawer');
  dialog.dataset.sheet = 'drawer';
  dialog.removeAttribute('aria-modal');
  dialog.show();
  sync();
  const observed = new MutationObserver(() => {
    if (dialog.open) return;
    observed.disconnect();
    sync();
    const focus = document.activeElement;
    if ((!focus || focus === document.body || dialog.contains(focus)) && restore?.isConnected && !restore.closest('[inert]')) restore.focus();
  });
  observed.observe(dialog, { attributes: true, attributeFilter: ['open'] });
}

export function openSheet(dialog: HTMLDialogElement, options: {
  anchor: () => Element | null; placement: SheetPlacement; lightDismiss?: () => boolean;
}): boolean {
  dialog.classList.remove('im-content-drawer', 'im-dialog');
  return openDialogSheet(dialog, { ...options, anchor: () => {
    const target = options.anchor();
    return target instanceof HTMLElement ? target : document.querySelector('#work-area');
  } });
}

document.addEventListener('click', event => {
  if (hasModal() || !(event.target instanceof Element)) return;
  const target = event.target;
  const entry = stack.find(entry => !entry.modal && entry.dialog.open && !entry.dialog.inert &&
    entry.restore instanceof HTMLButtonElement && !entry.restore.closest('dialog') && entry.restore.contains(target));
  if (!entry) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  dismiss(entry.dialog);
}, true);
document.addEventListener('keydown', event => {
  if (event.key !== 'Escape' || event.defaultPrevented || event.isComposing || hasModal()) return;
  const entry = stack.at(-1);
  if (!entry?.dialog.open || entry.modal || document.activeElement?.closest('.xterm')) return;
  event.preventDefault();
  dismiss(entry.dialog);
});
