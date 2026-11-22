import { componentElement, iconButtonHtml } from '@acaprino/imago/ui';

/** An icon-only control, the canon's `im-icon-btn`: its words are its accessible name and its tooltip. */
export function iconControl(label: string, icon: string, action: string): HTMLButtonElement {
  return componentElement<HTMLButtonElement>(iconButtonHtml(label, action, icon));
}

/** The × that closes a panel or a dialog, named for what it closes ("Close settings"). */
export function closeControl(label: string): HTMLButtonElement {
  return iconControl(label, 'close', 'close');
}
