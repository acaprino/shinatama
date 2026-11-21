import { closeLayers, inspector, uiButton, uiPart } from '@acaprino/imago/ui';

// The window's one side panel is Imago's inspector (canon v0.2.0): not modal, docked beside the panes from 1012 up and
// over their edge under it, one view at a time. The Model proxy, the thread's conversation and Leftovers show there.
// A view learns when it leaves the panel (closed, Escape, replaced by another view) through `onLeave`, once.

const shell = () => document.querySelector<HTMLElement>('#app')!;
type Shown = { view: HTMLElement; leave: () => void };
let shown: Shown | null = null;
let watching = false;

export type SideView = { root: HTMLElement; header: HTMLElement; body: HTMLElement; foot: HTMLElement; close: HTMLButtonElement };

/** A view of the side panel: its header (the title and the close button), a body that scrolls, a foot for its actions. */
export function sideView(id: string, title: string, closeLabel: string, titleId = `${id}-title`): SideView {
  const root = uiPart('stack', undefined, 'side-view'); root.id = id; root.setAttribute('aria-labelledby', titleId);
  const header = uiPart('row', undefined, 'im-panel-header');
  const heading = uiPart('minorHeading', title, 'im-panel-title'); heading.id = titleId; heading.tabIndex = -1;
  const close = uiButton('×'); close.setAttribute('aria-label', closeLabel); close.title = closeLabel;
  header.append(heading, uiPart('spacer'), close);
  const body = uiPart('scroll', undefined, 'im-panel-body');
  const foot = uiPart('row', undefined, 'im-panel-foot');
  root.append(header, body, foot);
  return { root, header, body, foot, close };
}

function watch(): void {
  if (watching) return;
  const region = shell().querySelector<HTMLElement>('.im-inspector');
  if (!region) return;
  watching = true;
  new MutationObserver(() => {
    if (!shown || (!region.hidden && region.contains(shown.view))) return;
    const left = shown; shown = null; left.leave();
  }).observe(region, { attributes: true, attributeFilter: ['hidden'], childList: true });
}

/** Shows `view` in the side panel; the view it replaces leaves (its `onLeave` runs). A drawer open beside closes. */
export function openSidePanel(view: HTMLElement, options: { opener?: HTMLElement; onLeave?: () => void } = {}): void {
  closeLayers(['drawer']);
  const before = shown;
  shown = { view, leave: options.onLeave ?? (() => {}) };
  inspector(shell()).open(view, options.opener);
  if (before && before.view !== view) before.leave();
  watch();
}

/** Closes the side panel when it shows `view`. */
export function closeSidePanel(view: HTMLElement): void {
  if (shown?.view === view) inspector(shell()).close();
}

/** Whether the side panel shows `view` now. */
export function sidePanelShows(view: HTMLElement): boolean {
  return shown?.view === view && view.isConnected && inspector(shell()).isOpen();
}

/** A drawer is opening: whatever the side panel shows leaves it. */
export function closeAnySidePanel(): void {
  if (shown) inspector(shell()).close();
}
