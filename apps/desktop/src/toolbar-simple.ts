import './toolbar-simple.css';

type RecentProject = { id: string; path: string; available?: boolean };
type FoundProject = { path: string; available?: boolean };
export const folderName = (path: string) => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;
export const samePath = (a: string, b: string) => a.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').toLowerCase() === b.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').toLowerCase();

/** A small popover anchored under a toolbar control. Escape and outside clicks close it. */
function popover(anchor: HTMLElement, id: string, align: 'left' | 'right', onClose?: () => void) {
  const menu = document.createElement('div'); menu.id = id; menu.className = `toolbar-menu toolbar-menu-${align}`; menu.hidden = true; menu.setAttribute('role', 'menu');
  document.querySelector('.toolbar')!.append(menu);
  const close = () => { if (menu.hidden) return; menu.hidden = true; anchor.setAttribute('aria-expanded', 'false'); onClose?.(); };
  const place = () => {
    const bar = (document.querySelector('.toolbar') as HTMLElement).getBoundingClientRect(); const box = anchor.getBoundingClientRect();
    // Fixed to the viewport: the toolbar clips its own overflow.
    menu.style.top = `${bar.bottom}px`;
    if (align === 'left') { menu.style.left = `${Math.max(4, box.left)}px`; menu.style.right = ''; }
    else { menu.style.right = `${Math.max(4, window.innerWidth - box.right)}px`; menu.style.left = ''; }
  };
  const toggle = (onOpen?: () => void) => {
    if (!menu.hidden) { close(); return; }
    for (const other of document.querySelectorAll<HTMLElement>('.toolbar-menu')) if (other !== menu && !other.hidden) other.dispatchEvent(new Event('toolbar-menu-close'));
    place(); menu.hidden = false; anchor.setAttribute('aria-expanded', 'true'); onOpen?.();
  };
  document.addEventListener('pointerdown', event => { if (!menu.hidden && event.target instanceof Node && !menu.contains(event.target) && !anchor.contains(event.target)) close(); });
  document.addEventListener('keydown', event => { if (event.key === 'Escape' && !menu.hidden) { close(); anchor.focus(); } });
  menu.addEventListener('toolbar-menu-close', close);
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
  const secondary = ['#find-terminal', '#layout-toggle', '#restore', '#desktop-alerts', '#open-settings'].map(selector => toolbar.querySelector<HTMLElement>(selector));
  for (const item of [...secondary, appearance, theme]) if (item) { item.classList.add('toolbar-menu-item'); menu.append(item); }
  menu.addEventListener('click', event => { if (event.target instanceof HTMLElement && event.target.closest('button')) close(); });
  more.onclick = () => toggle();
}

/** Quick filter for the tabs: one chip per project with open tabs. A chip shows only that
 * project's tabs; pressing it again, or "All", shows every tab. Hidden with fewer than two projects. */
export function createTabFilter(options: { tabs: () => readonly string[]; onChange: (project: string | null) => void }) {
  const bar = document.createElement('div'); bar.id = 'tab-filter'; bar.setAttribute('role', 'group'); bar.setAttribute('aria-label', 'Show the tabs of one project');
  const toolbar = document.querySelector<HTMLElement>('.toolbar')!; toolbar.insertBefore(bar, toolbar.querySelector('.spring'));
  let selected: string | null = null;
  const chip = (text: string, count: number, project: string | null, title: string) => {
    const button = document.createElement('button'); button.type = 'button'; button.className = 'tab-filter-chip'; button.title = title;
    const pressed = project === null ? selected === null : selected !== null && samePath(selected, project);
    button.setAttribute('aria-pressed', String(pressed));
    const name = document.createElement('span'); name.textContent = text; const number = document.createElement('small'); number.textContent = String(count);
    button.append(name, number);
    button.onclick = () => { selected = project === null || pressed ? null : project; options.onChange(selected); };
    return button;
  };
  function paint() {
    const tabs = options.tabs(); const projects: { path: string; count: number }[] = [];
    for (const path of tabs) { const known = projects.find(project => samePath(project.path, path)); if (known) known.count++; else projects.push({ path, count: 1 }); }
    // The filtered project's last tab closed: show everything again.
    if (selected !== null && !projects.some(project => samePath(project.path, selected!))) { selected = null; options.onChange(null); return; }
    bar.hidden = projects.length < 2 && selected === null;
    bar.replaceChildren(chip('All', tabs.length, null, 'Show the tabs of every project'),
      ...projects.map(project => chip(folderName(project.path), project.count, project.path, `Show only the tabs in ${project.path}`)));
  }
  return {
    paint,
    current: () => selected,
    /** A new tab outside the filtered project must not open hidden. */
    reveal(path: string) { if (selected !== null && !samePath(selected, path)) selected = null; },
  };
}

/** The project button opens a finder: type to narrow, arrows to move, Enter to switch.
 * Recent projects come first; every project inside the configured roots follows, so the
 * full picker is needed only to manage roots. */
export function createProjectMenu(options: {
  button: HTMLElement;
  load: () => Promise<readonly RecentProject[]>;
  /** Searches the project roots. Results arrive through the callback, possibly more than once. */
  search?: (onResults: (projects: readonly FoundProject[]) => void) => { open(filter?: string): void; query(filter: string): void; close(): void };
  current: () => string;
  /** Folder of every running session, one entry per session. */
  running?: () => readonly string[];
  choose: (path: string) => Promise<void>;
  browse: () => void;
  create: () => void;
  manage: () => void;
  reportError: (message: string) => void;
}): void {
  let generation = 0; let recent: readonly RecentProject[] = []; let found: readonly FoundProject[] = [];
  let active = 0; let state: 'loading' | 'ready' | 'failed' = 'loading'; let searching = false;
  const client = options.search?.(projects => { found = projects; searching = false; paint(); });
  const { menu, close, toggle } = popover(options.button, 'project-menu', 'left', () => { generation++; client?.close(); });
  menu.setAttribute('role', 'dialog'); menu.setAttribute('aria-label', 'Projects'); options.button.setAttribute('aria-haspopup', 'dialog');
  const filter = document.createElement('input'); filter.id = 'project-menu-filter'; filter.type = 'search'; filter.autocomplete = 'off'; filter.spellcheck = false;
  filter.maxLength = 128; filter.placeholder = 'Find a project…'; filter.setAttribute('aria-label', 'Find a project'); filter.setAttribute('aria-controls', 'project-menu-list');
  const list = document.createElement('div'); list.id = 'project-menu-list'; list.className = 'toolbar-menu-list'; list.setAttribute('role', 'listbox'); list.setAttribute('aria-label', 'Projects');
  // Stays on between openings: "only projects with a running session".
  let onlyRunning = false;
  const runningToggle = document.createElement('button'); runningToggle.type = 'button'; runningToggle.id = 'project-menu-running';
  runningToggle.title = 'Show only projects with at least one running session';
  runningToggle.onclick = () => { onlyRunning = !onlyRunning; active = 0; paint(); filter.focus(); };
  // Counted once per paint. A session in a subfolder counts for the known project that holds it.
  let running = new Map<string, { path: string; count: number }>();
  const folderKey = (path: string) => path.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').replace(/\//g, '\\').toLowerCase();
  function countRunning() {
    const known = [...recent, ...found].map(project => project.path);
    running = new Map();
    for (const folder of options.running?.() ?? []) {
      const at = folderKey(folder);
      const owner = known.filter(path => at === folderKey(path) || at.startsWith(`${folderKey(path)}\\`)).sort((a, b) => b.length - a.length)[0] ?? folder;
      const entry = running.get(folderKey(owner)); if (entry) entry.count++; else running.set(folderKey(owner), { path: owner, count: 1 });
    }
  }
  const top = document.createElement('div'); top.className = 'project-menu-top'; top.append(filter);
  if (options.running) top.append(runningToggle);
  const actions = document.createElement('div'); actions.className = 'toolbar-menu-actions';
  const action = (label: string, run: () => void, hint?: string) => {
    const item = document.createElement('button'); item.type = 'button'; item.className = 'toolbar-menu-action';
    item.textContent = label; if (hint) item.title = hint; item.onclick = () => { close(); run(); }; return item;
  };
  actions.append(action('Project roots…', options.manage, 'Folders that hold your projects, such as D:\\Projects'),
    action('Open one folder…', options.browse, 'Open a single folder as a project, without adding a root'), action('New project…', options.create));
  menu.append(top, list, actions);

  function rows(): { path: string; available: boolean }[] {
    const query = filter.value.trim().toLocaleLowerCase();
    const matches = (path: string) => !query || `${folderName(path)} ${path}`.toLocaleLowerCase().includes(query);
    if (onlyRunning) {
      return [...running.values()].map(entry => entry.path).filter(matches).map(path => ({ path, available: true }));
    }
    const first = recent.filter(project => matches(project.path)).map(project => ({ path: project.path, available: project.available !== false }));
    const rest = found.filter(project => matches(project.path) && !first.some(known => samePath(known.path, project.path)))
      .map(project => ({ path: project.path, available: project.available !== false }));
    return [...first, ...rest].slice(0, 60);
  }
  function pick(path: string) { close(); options.button.focus(); void options.choose(path).catch(error => options.reportError(String(error?.message ?? error))); }
  function paint() {
    countRunning(); const items = rows(); active = Math.max(0, Math.min(active, items.length - 1)); list.replaceChildren();
    runningToggle.textContent = `Running ${running.size}`; runningToggle.setAttribute('aria-pressed', String(onlyRunning));
    if (!items.length) {
      const note = document.createElement('p'); note.className = 'toolbar-menu-note';
      note.textContent = onlyRunning ? (filter.value.trim() ? 'No running project matches.' : 'No project has a running session.')
        : state === 'loading' || searching ? 'Loading…' : state === 'failed' && !found.length ? 'Projects could not be loaded.'
        : filter.value.trim() ? 'No project matches. Open one folder… opens any folder.' : 'No projects yet. Project roots… adds the folder that holds them.';
      list.append(note); filter.removeAttribute('aria-activedescendant'); return;
    }
    items.forEach((project, index) => {
      const row = document.createElement('button'); row.type = 'button'; row.className = 'toolbar-menu-project'; row.id = `project-menu-row-${index}`;
      row.setAttribute('role', 'option'); row.tabIndex = -1; row.title = project.path; row.disabled = !project.available;
      if (samePath(project.path, options.current())) row.dataset.current = '';
      const name = document.createElement('strong'); name.textContent = folderName(project.path);
      const sessions = running.get(folderKey(project.path))?.count ?? 0;
      const path = document.createElement('span');
      path.textContent = `${sessions ? `● ${sessions} running · ` : ''}${project.available ? project.path : `${project.path} · unavailable`}`; row.append(name, path);
      row.onclick = () => pick(project.path);
      row.onpointermove = () => { if (active !== index) { active = index; mark(); } };
      list.append(row);
    });
    mark();
  }
  // Moving the highlight repaints nothing: only the selected flag changes.
  function mark() {
    [...list.querySelectorAll<HTMLElement>('.toolbar-menu-project')].forEach((row, index) => row.setAttribute('aria-selected', String(index === active)));
    const row = list.querySelector<HTMLElement>(`#project-menu-row-${active}`);
    if (row) filter.setAttribute('aria-activedescendant', row.id);
  }
  filter.oninput = () => { active = 0; if (client) { searching = true; client.query(filter.value); } paint(); };
  filter.onkeydown = event => {
    const count = list.querySelectorAll('.toolbar-menu-project').length;
    if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && count) {
      event.preventDefault(); active = (active + (event.key === 'ArrowDown' ? 1 : count - 1)) % count; mark();
      list.querySelector<HTMLElement>(`#project-menu-row-${active}`)?.scrollIntoView({ block: 'nearest' });
    } else if (event.key === 'Enter') {
      event.preventDefault(); const row = list.querySelector<HTMLButtonElement>(`#project-menu-row-${active}`);
      if (row && !row.disabled) row.click();
    }
  };
  async function load() {
    const mine = ++generation; filter.value = ''; found = []; active = 0; state = 'loading'; searching = !!client; paint(); filter.focus();
    client?.open('');
    try { const received = await options.load(); if (mine !== generation) return; recent = received; state = 'ready'; }
    catch { if (mine !== generation) return; state = 'failed'; }
    paint();
  }
  options.button.onclick = () => toggle(() => { void load(); });
}
