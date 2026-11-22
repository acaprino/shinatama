import { openMenu, roving } from '@acaprino/imago/ui';

// The window's views are the tabs of the title bar (Imago's canon): the Workspace, which is the tiling workspace with
// its toolbar, and Settings, the last tab. A view that is not shown is hidden, never removed: the terminals keep running
// (a hidden terminal is not fitted, see scheduleFit), and Settings keeps what was typed in it until it is left.
export type ViewId = 'workspace' | 'settings';
export const VIEW_TABS: readonly { id: ViewId; label: string; controls: string }[] = [
  { id: 'workspace', label: 'Workspace', controls: 'desktop-content' },
  { id: 'settings', label: 'Settings', controls: 'desktop-content' },
];
const LABELS: Record<ViewId, string> = { workspace: 'Terminal workspace', settings: 'Settings' };

export type Views = {
  current(): ViewId;
  /** Shows a view. The tab and the content follow; `listen`ers hear the change once. */
  select(id: ViewId): void;
  /** Heard when the view changes, with the view left. */
  listen(listener: (shown: ViewId, left: ViewId) => void): void;
};

/**
 * Wires the tabs the title bar was written with (`tabNavigationHtml`, `titlebarHtml`'s `views` button) to the views of the
 * content region. `hosts` holds, for each view, the elements it shows: the other view's are hidden.
 */
export function createViews(shell: HTMLElement, content: HTMLElement, hosts: Record<ViewId, readonly HTMLElement[]>): Views {
  const titlebar = shell.querySelector<HTMLElement>('.im-titlebar')!;
  const nav = titlebar.querySelector<HTMLElement>('.im-tabs')!;
  const tabs = [...nav.querySelectorAll<HTMLElement>('.im-tab')];
  const listeners: Array<(shown: ViewId, left: ViewId) => void> = [];
  let current: ViewId = 'workspace';

  function select(id: ViewId) {
    if (id === current) return;
    const left = current; current = id;
    for (const tab of tabs) {
      const on = tab.dataset.id === id;
      tab.setAttribute('aria-selected', String(on));
      if (on) tab.setAttribute('aria-current', 'page'); else tab.removeAttribute('aria-current');
    }
    for (const [view, elements] of Object.entries(hosts) as [ViewId, readonly HTMLElement[]][]) for (const element of elements) element.hidden = view !== id;
    content.setAttribute('aria-label', LABELS[id]);
    for (const listener of listeners) listener(id, left);
  }
  nav.addEventListener('click', event => {
    const tab = event.target instanceof Element ? event.target.closest<HTMLElement>('.im-tab') : null;
    const id = VIEW_TABS.find(view => view.id === tab?.dataset.id)?.id;
    if (id) select(id);
  });
  // Under 620 the tabs are hidden and the Views button stands for them: nothing in the package opens it.
  const button = titlebar.querySelector<HTMLButtonElement>('.im-views-menu');
  if (button) {
    let close: (() => void) | undefined;
    button.setAttribute('aria-haspopup', 'menu');
    button.onclick = () => {
      if (button.getAttribute('aria-expanded') === 'true') { close?.(); return; }
      close = openMenu(button, VIEW_TABS.map(view => ({ label: view.label, checked: () => current === view.id, run: () => select(view.id) })));
    };
  }
  roving(nav, { item: '.im-tab', orientation: 'horizontal' });
  return { current: () => current, select, listen: listener => { listeners.push(listener); } };
}
