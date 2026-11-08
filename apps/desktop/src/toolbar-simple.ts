import './toolbar-simple.css';

type RecentProject = { id: string; path: string; available?: boolean };
const folderName = (path: string) => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;
const samePath = (a: string, b: string) => a.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').toLowerCase() === b.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').toLowerCase();

/** A small popover anchored under a toolbar control. Escape and outside clicks close it. */
function popover(anchor: HTMLElement, id: string, align: 'left' | 'right') {
  const menu = document.createElement('div'); menu.id = id; menu.className = `toolbar-menu toolbar-menu-${align}`; menu.hidden = true; menu.setAttribute('role', 'menu');
  document.querySelector('.toolbar')!.append(menu);
  const close = () => { menu.hidden = true; anchor.setAttribute('aria-expanded', 'false'); };
  const place = () => {
    const bar = (document.querySelector('.toolbar') as HTMLElement).getBoundingClientRect(); const box = anchor.getBoundingClientRect();
    // Fixed to the viewport: the toolbar clips its own overflow.
    menu.style.top = `${bar.bottom}px`;
    if (align === 'left') { menu.style.left = `${Math.max(4, box.left)}px`; menu.style.right = ''; }
    else { menu.style.right = `${Math.max(4, window.innerWidth - box.right)}px`; menu.style.left = ''; }
  };
  const toggle = (onOpen?: () => void) => {
    if (!menu.hidden) { close(); return; }
    for (const other of document.querySelectorAll<HTMLElement>('.toolbar-menu')) if (other !== menu) other.hidden = true;
    place(); menu.hidden = false; anchor.setAttribute('aria-expanded', 'true'); onOpen?.();
  };
  document.addEventListener('pointerdown', event => { if (!menu.hidden && event.target instanceof Node && !menu.contains(event.target) && !anchor.contains(event.target)) close(); });
  document.addEventListener('keydown', event => { if (event.key === 'Escape' && !menu.hidden) { close(); anchor.focus(); } });
  anchor.setAttribute('aria-haspopup', 'menu'); anchor.setAttribute('aria-expanded', 'false');
  return { menu, close, toggle };
}

/** One "⋯" menu for everything that is not needed to start working. Elements are moved, not
 * recreated, so every owner keeps its handlers and ids. */
export function organizeToolbar(): void {
  const toolbar = document.querySelector<HTMLElement>('.toolbar'); if (!toolbar) return;
  const newSession = toolbar.querySelector<HTMLElement>('#new-terminal'); const spring = toolbar.querySelector<HTMLElement>('.spring');
  if (newSession && spring) toolbar.insertBefore(newSession, spring); // primary action next to the project
  for (const hidden of toolbar.querySelectorAll<HTMLElement>('.workspace-name, .toolbar-divider')) hidden.hidden = true;
  const more = document.createElement('button'); more.type = 'button'; more.id = 'toolbar-more'; more.textContent = '⋯'; more.title = 'More'; more.setAttribute('aria-label', 'More options');
  toolbar.append(more);
  const { menu, close, toggle } = popover(more, 'toolbar-more-menu', 'right');
  const themeSelect = toolbar.querySelector('select'); const theme = themeSelect?.closest('label') ?? null;
  const appearance = [...toolbar.querySelectorAll<HTMLElement>('button')].find(button => button.textContent?.trim() === 'Appearance');
  const secondary = ['#find-terminal', '#layout-toggle', '#restore', '#desktop-alerts'].map(selector => toolbar.querySelector<HTMLElement>(selector));
  for (const item of [...secondary, appearance, theme]) if (item) { item.classList.add('toolbar-menu-item'); menu.append(item); }
  menu.addEventListener('click', event => { if (event.target instanceof HTMLElement && event.target.closest('button')) close(); });
  more.onclick = () => toggle();
}

/** The project button lists recent projects; one click selects. */
export function createProjectMenu(options: {
  button: HTMLElement;
  load: () => Promise<readonly RecentProject[]>;
  current: () => string;
  choose: (path: string) => Promise<void>;
  browse: () => void;
  create: () => void;
  manage: () => void;
  reportError: (message: string) => void;
}): void {
  const { menu, close, toggle } = popover(options.button, 'project-menu', 'left');
  let generation = 0;
  const action = (label: string, run: () => void, hint?: string) => {
    const item = document.createElement('button'); item.type = 'button'; item.className = 'toolbar-menu-action'; item.setAttribute('role', 'menuitem');
    item.textContent = label; if (hint) item.title = hint; item.onclick = () => { close(); run(); }; return item;
  };
  async function fill() {
    const mine = ++generation; menu.replaceChildren();
    const heading = document.createElement('p'); heading.className = 'toolbar-menu-heading'; heading.textContent = 'Projects'; menu.append(heading);
    const list = document.createElement('div'); list.className = 'toolbar-menu-list'; list.setAttribute('role', 'group'); menu.append(list);
    const loading = document.createElement('p'); loading.className = 'toolbar-menu-note'; loading.textContent = 'Loading…'; list.append(loading);
    menu.append(action('Browse for a folder…', options.browse), action('New project…', options.create), action('More…', options.manage, 'Project roots and the full folder list'));
    let projects: readonly RecentProject[] = [];
    try { projects = await options.load(); } catch { if (mine === generation) loading.textContent = 'Projects could not be loaded.'; return; }
    if (mine !== generation) return;
    list.replaceChildren();
    if (!projects.length) { const none = document.createElement('p'); none.className = 'toolbar-menu-note'; none.textContent = 'No projects yet. Browse for a folder.'; list.append(none); return; }
    for (const project of projects.slice(0, 12)) {
      const row = document.createElement('button'); row.type = 'button'; row.className = 'toolbar-menu-project'; row.setAttribute('role', 'menuitemradio');
      const active = samePath(project.path, options.current()); row.setAttribute('aria-checked', String(active)); row.title = project.path;
      row.disabled = project.available === false;
      const name = document.createElement('strong'); name.textContent = `${active ? '✓ ' : ''}${folderName(project.path)}`;
      const path = document.createElement('span'); path.textContent = project.path; row.append(name, path);
      row.onclick = () => { close(); void options.choose(project.path).catch(error => options.reportError(String(error?.message ?? error))); };
      list.append(row);
    }
  }
  options.button.onclick = () => toggle(() => { void fill(); });
}
