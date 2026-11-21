import { closeLayers, hasModal, openContentDialog, openDialogDrawer, openDialogSheet, topLayerOf, type SheetPlacement } from '@acaprino/imago/ui';

// Every surface of the window is a layer of Imago's one stack (attachLayers in shell-frame.ts): it decides what
// Escape closes, what covers what, and where focus goes back. This module only says which of the app's dialogs is
// a drawer beside the panels, which is modal, and which is a sheet.
export type { SheetPlacement };
const panels = new Set<() => void>();
const shell = () => document.querySelector<HTMLElement>('#app')!;

/** The app dialog on top: the top drawer or modal dialog, under any menu or sheet opened over it. */
export function topModal(): HTMLDialogElement | null {
  const top = topLayerOf(['drawer', 'dialog'])?.el;
  if (top instanceof HTMLDialogElement) return top;
  // A modal dialog sits in its scrim; Imago's own confirm is not an app dialog.
  const inner = top?.querySelector(':scope > dialog');
  return inner instanceof HTMLDialogElement ? inner : null;
}

/** A side panel that is not a dialog (the model proxy): opening it closes the drawers and the other side panels. */
export function sidePanel(close: () => void): () => void {
  panels.add(close);
  return () => {
    closeLayers(['drawer']);
    for (const other of panels) if (other !== close) other();
  };
}

/** Ask a dialog to close as its own Cancel would: a form that refuses `cancel` (a write in progress) stays. */
function dismiss(dialog: HTMLDialogElement) {
  if (dialog.dispatchEvent(new Event('cancel', { cancelable: true }))) dialog.close();
}

// Escape in a terminal belongs to the agent running in it.
const outsideTerminal = (target: EventTarget | null) => !(target instanceof Element && target.closest('.xterm'));

export function openModal(dialog: HTMLDialogElement) {
  if (dialog.open) return;
  // Closing with live terminals and the hop preview are modal; so is anything asked while a modal is open.
  if (dialog.matches('#confirm-dialog,#hop-preview-dialog') || hasModal()) {
    const home = dialog.parentElement ?? shell();
    openContentDialog(shell(), dialog, {
      canDismiss() { dismiss(dialog); return !dialog.open; },
      onClose() { home.append(dialog); },
    });
    return;
  }
  for (const close of panels) close();
  openDialogDrawer(dialog, { acceptsEscape: outsideTerminal });
}

export function openSheet(dialog: HTMLDialogElement, options: {
  anchor: () => Element | null; placement: SheetPlacement; lightDismiss?: () => boolean;
}): boolean {
  return openDialogSheet(dialog, { ...options, anchor: () => {
    const target = options.anchor();
    return target instanceof HTMLElement ? target : document.querySelector('#work-area');
  } });
}
