import { closeLayers, inspector, uiPart } from '@acaprino/imago/ui';
import { closeControl } from './controls.ts';

// The window's one side panel is Imago's inspector (canon v0.2.0): not modal, docked beside the panes from 1012 up and
// over their edge under it, one view at a time. The views the toolbar and the sessions open show there (the Model proxy,
// the thread's conversation, Leftovers, Sessions, the project overview, New project, Agents, a session's detail and the
// views opened from it). A view opened from inside the side panel covers the one that opened it (`push`) and gives it
// back when it closes; a view opened from outside replaces the whole stack. A view learns when it leaves the panel
// (closed, Escape, replaced by another view) through `onLeave`, once for each time it was shown.

const shell = () => document.querySelector<HTMLElement>('#app')!;
const region = () => shell().querySelector<HTMLElement>('.im-inspector');
type Shown = { view: HTMLElement; leave: () => void };
/** The views the panel shows or has covered, the outermost first. */
const stack: Shown[] = [];
/** A view the inspector has not shown yet (a guard is asking whether to leave the current one): tracked once it appears. */
let pending: Shown | null = null;
let watching = false;

export type SideView = { root: HTMLElement; header: HTMLElement; heading: HTMLElement; body: HTMLElement; foot: HTMLElement; close: HTMLButtonElement };

/** A view of the side panel: its header (the title and the close button), a body that scrolls, a foot for its actions. */
export function sideView(id: string, title: string, closeLabel: string, titleId = `${id}-title`): SideView {
  const root = uiPart('stack', undefined, 'side-view'); root.id = id; root.setAttribute('aria-labelledby', titleId);
  const header = uiPart('row', undefined, 'im-panel-header');
  const heading = uiPart('minorHeading', title, 'im-panel-title'); heading.id = titleId; heading.tabIndex = -1;
  const close = closeControl(closeLabel);
  header.append(heading, uiPart('spacer'), close);
  const body = uiPart('scroll', undefined, 'im-panel-body');
  const foot = uiPart('row', undefined, 'im-panel-foot'); foot.hidden = true;
  // The foot is an empty bordered strip until a view puts its actions there.
  new MutationObserver(() => { foot.hidden = !foot.hasChildNodes(); }).observe(foot, { childList: true });
  root.append(header, body, foot);
  return { root, header, heading, body, foot, close };
}

const covered = (view: HTMLElement) => view.hasAttribute('data-im-covered');

/** Ends what left the panel: a view that is no longer in it, or the whole stack when the panel is hidden. */
function sync(): void {
  const home = region();
  if (pending && home && !home.hidden && home.contains(pending.view) && pending.view.isConnected) { stack.push(pending); pending = null; }
  for (const entry of [...stack]) {
    if (home && !home.hidden && home.contains(entry.view)) continue;
    stack.splice(stack.indexOf(entry), 1);
    entry.leave();
  }
}

function watch(): void {
  if (watching) return;
  const home = region();
  if (!home) return;
  watching = true;
  new MutationObserver(sync).observe(home, { attributes: true, attributeFilter: ['hidden'], childList: true });
}

/** Whether `view` is in the side panel now, covered or not. */
function inPanel(view: HTMLElement): boolean {
  const home = region();
  return !!home && !home.hidden && view.isConnected && home.contains(view) && stack.some(entry => entry.view === view);
}

/** Shows `view` in the side panel. From inside the panel it covers the current view; from outside it replaces them all. */
export function openSidePanel(view: HTMLElement, options: { opener?: HTMLElement; onLeave?: () => void } = {}): void {
  const home = region();
  if (!home) return;
  closeLayers(['drawer']);
  const panel = inspector(shell());
  if (inPanel(view) && !covered(view)) return;
  const from = options.opener ?? (document.activeElement instanceof HTMLElement ? document.activeElement : undefined);
  const inside = !home.hidden && !!from && home.contains(from) && !home.contains(view);
  if (!inside) {
    // One view at a time: whatever the panel shows, covered views included, goes.
    for (let depth = stack.filter(entry => covered(entry.view)).length; depth > 0; depth--) { panel.close(); sync(); }
  }
  const entry: Shown = { view, leave: options.onLeave ?? (() => {}) };
  const known = stack.findIndex(other => other.view === view);
  if (known >= 0) stack.splice(known, 1);
  if (inside) panel.push(view, options.opener); else panel.open(view, options.opener);
  // A replacement a dirty guard has not answered yet shows the view later: it is tracked when it appears (sync).
  pending = null;
  if (view.isConnected && !home.hidden && home.contains(view)) stack.push(entry); else pending = entry;
  sync();
  watch();
}

/** Closes `view`, and the views opened over it. A view that the panel does not show is left alone. */
export function closeSidePanel(view: HTMLElement): void {
  if (!inPanel(view)) return;
  const panel = inspector(shell());
  for (let guard = stack.length; guard > 0 && covered(view); guard--) { panel.close(); sync(); }
  panel.close();
  sync();
}

/** Whether the side panel shows `view` now (a view that something covers is shown, but not on top). */
export function sidePanelShows(view: HTMLElement): boolean {
  return inPanel(view);
}

/** Whether `view` is the one on top: shown and not covered. */
export function sidePanelTop(view: HTMLElement): boolean {
  return inPanel(view) && !covered(view);
}

/** Whether the side panel shows any view. */
export function sidePanelOpen(): boolean {
  return stack.length > 0;
}

/** A drawer is opening: whatever the side panel shows leaves it. */
export function closeAnySidePanel(): void {
  const home = region();
  if (!home || home.hidden) return;
  const panel = inspector(shell());
  for (let guard = stack.length + 1; guard > 0 && !home.hidden; guard--) { panel.close(); sync(); }
}

/**
 * A view that is a screen of its own: opened with `open(opener)`, closed with `close()` (its × and Escape do the same),
 * shown while it is in the panel, and told through `onClose` each time it leaves. `isOpen()` is true while it is in the
 * panel, even covered by a view it opened.
 */
export type SideSurface = Omit<SideView, 'close'> & {
  /** The × of the header. */
  closeButton: HTMLButtonElement;
  open(opener?: HTMLElement): void;
  close(): void;
  isOpen(): boolean;
  /** Nothing covers the surface: it is the view the person sees. */
  onTop(): boolean;
  onClose(listener: () => void): void;
  /** While `isDirty()` holds, closing or replacing the view asks `ask()` once and keeps it on false. Set after `open()`. */
  guard(isDirty: () => boolean, ask: () => Promise<boolean>): void;
};

export function sideSurface(id: string, title: string, closeLabel: string, titleId?: string): SideSurface {
  const { close: closeButton, ...view } = sideView(id, title, closeLabel, titleId);
  const listeners: Array<() => void> = [];
  const left = () => { for (const listener of listeners) listener(); };
  const surface: SideSurface = {
    ...view, closeButton,
    open: opener => openSidePanel(view.root, { opener, onLeave: left }),
    close: () => closeSidePanel(view.root),
    isOpen: () => sidePanelShows(view.root),
    onTop: () => sidePanelTop(view.root),
    onClose: listener => { listeners.push(listener); },
    guard: (isDirty, ask) => { if (sidePanelTop(view.root)) inspector(shell()).guard(isDirty, ask); },
  };
  closeButton.onclick = () => surface.close();
  return surface;
}
