import { openMenu, openSheet, searchPicker, uiButton, uiPart } from '@acaprino/imago/ui';
type RecentProject = { id: string; path: string; available?: boolean };
type FoundProject = { path: string; available?: boolean };
export const folderName = (path: string) => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;
export const samePath = (a: string, b: string) => a.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').toLowerCase() === b.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').toLowerCase();

export function organizeToolbar(): void {
  const toolbar = document.querySelector<HTMLElement>('.toolbar')!;
  const more = uiButton('More', { attrs: { id: 'toolbar-more', 'aria-haspopup': 'menu', 'aria-label': 'More options' } });
  toolbar.append(more);
  const secondary = ['find-terminal', 'layout-toggle', 'restore', 'sessions', 'session-search'];
  const folding = ['open-sessions', 'open-projects', 'open-files'];
  // The shell's width, not the screen's (the canon's breakpoint of 850): a narrow shell folds the toolbar into More.
  const shell = toolbar.closest<HTMLElement>('.im-shell') ?? document.documentElement;
  const narrow = { matches: shell.getBoundingClientRect().width < 850 };
  // Original buttons retain their handlers and disabled state when their commands move to More.
  const reserve = (id: string) => {
    const control = toolbar.querySelector<HTMLButtonElement>('#' + id);
    if (!control) return [];
    const home = document.createComment(id);
    control.before(home);
    return [{ control, home }];
  };
  const fixed = secondary.flatMap(reserve);
  const responsive = folding.flatMap(reserve);
  const parked = uiPart('body');
  parked.hidden = true;
  toolbar.append(parked);
  parked.append(...fixed.map(entry => entry.control));
  let closeMore: (() => void) | undefined;
  function fold() {
    closeMore?.();
    for (const { home, control } of responsive) {
      if (narrow.matches) parked.append(control);
      else home.after(control);
    }
  }
  new ResizeObserver(([entry]) => {
    const matches = entry.contentRect.width < 850;
    if (matches !== narrow.matches) { narrow.matches = matches; fold(); }
  }).observe(shell);
  fold();
  more.onclick = () => {
    if (more.getAttribute('aria-expanded') === 'true') { closeMore?.(); return; }
    const entries = [...(narrow.matches ? responsive : []), ...fixed];
    closeMore = openMenu(more, entries.filter(({ control }) => !control.hidden).map(({ control }) => ({
      control, label: control.textContent?.trim() || control.title || control.getAttribute('aria-label') || control.id,
      disabled: control.disabled ? control.title || 'Unavailable' : undefined,
      run: () => control.click(),
    })), { configure: menu => { menu.id = 'toolbar-more-menu'; } });
  };
}
/** Quick filter for the tabs: one chip per project with open tabs. A chip shows only that
 * project's tabs; pressing it again, or "All", shows every tab. Hidden with fewer than two projects. */
export function createTabFilter(options: { tabs: () => readonly string[]; onChange: (project: string | null) => void }) {
  const bar = uiPart('row'); bar.id = 'tab-filter'; bar.setAttribute('role', 'group'); bar.setAttribute('aria-label', 'Show the tabs of one project');
  const toolbar = document.querySelector<HTMLElement>('.toolbar')!; toolbar.insertBefore(bar, toolbar.querySelector('.spring'));
  let selected: string | null = null;
  const chip = (text: string, count: number, project: string | null, title: string) => {
    const button = uiButton('', { sm: true }); button.replaceChildren(); button.title = title;
    const pressed = project === null ? selected === null : selected !== null && samePath(selected, project);
    button.setAttribute('aria-pressed', String(pressed));
    const name = uiPart('text', text); const number = uiPart('hint', String(count));
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
  /** Ids of the projects listed in the thread sidebar. */
  active?: () => ReadonlySet<string>;
  /** Lists or unlists a project in the thread sidebar; the current project does not change. */
  toggleActive?: (projectId: string) => void | Promise<void>;
  /** Folder of every running session, one entry per session. */
  running?: () => readonly string[];
  choose: (path: string) => Promise<void>;
  browse: () => void;
  create: () => void;
  manage: () => void;
  reportError: (message: string) => void;
}): void {
  let onlyRunning = false;
  let generation = 0;
  const key = (path: string) => path.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').replace(/\//g, '\\').toLowerCase();
  options.button.setAttribute('aria-haspopup', 'dialog');
  options.button.setAttribute('aria-expanded', 'false');
  let activeClose: (() => void) | undefined;
  options.button.onclick = () => {
    if (activeClose) { activeClose(); return; }
    const current = ++generation;
    let recent: readonly RecentProject[] = [];
    let found: readonly FoundProject[] = [];
    let lastQuery = '';
    let ready = false;
    let failed = false;
    const pane = uiPart('stack');
    pane.id = 'project-menu';
    const runningButton = uiButton('Running');
    runningButton.id = 'project-menu-running';
    let close: () => void = () => {};
    type Row = { id: string; label: string; detail: string; path: string; recentId?: string; disabled?: boolean };
    const client = options.search?.(projects => {
      if (current !== generation || !pane.isConnected) return;
      found = projects;
      void picker.refresh();
    });
    function rows(query: string): Row[] {
      const known = [...recent, ...found].map(project => project.path);
      const counts = new Map<string, { path: string; count: number }>();
      for (const folder of options.running?.() ?? []) {
        const at = key(folder);
        const owner = known.filter(path => at === key(path) || at.startsWith(key(path) + '\\')).sort((a, b) => b.length - a.length)[0] ?? folder;
        const count = counts.get(key(owner));
        if (count) count.count++; else counts.set(key(owner), { path: owner, count: 1 });
      }
      runningButton.textContent = 'Running ' + counts.size;
      runningButton.setAttribute('aria-pressed', String(onlyRunning));
      const candidates = onlyRunning ? [...counts.values()] : [...recent, ...found];
      const seen = new Set<string>();
      return candidates.flatMap(project => {
        const id = key(project.path);
        if (seen.has(id) || !project.path.toLowerCase().includes(query.toLowerCase().trim())) return [];
        seen.add(id);
        const count = counts.get(id)?.count;
        const disabled = 'available' in project && project.available === false;
        return [{ id, label: folderName(project.path), path: project.path, disabled,
          recentId: recent.find(entry => samePath(entry.path, project.path))?.id,
          detail: [samePath(project.path, options.current()) ? 'Current' : '', count ? count + ' running' : '', project.path, disabled ? 'Unavailable' : ''].filter(Boolean).join(' · ') }];
      }).slice(0, 60);
    }
    const picker = searchPicker<Row>(document, {
      label: 'Find a project', loading: 'Loading projects…', empty: 'No matching projects.', errorText: String,
      search: async query => {
        if (lastQuery !== query) { lastQuery = query; client?.query(query); }
        const results = rows(query);
        if (!results.length && !ready) throw new Error('Loading projects…');
        if (!results.length && failed) throw new Error('Projects could not be loaded.');
        return results;
      },
      onChoose: item => {
        close();
        return options.choose(item.path).catch(error => options.reportError(String(error?.message ?? error)));
      },
      mark: options.toggleActive ? {
        label: 'List in thread sidebar', key: item => item.recentId!, available: item => !!item.recentId,
        checked: item => !!item.recentId && !!options.active?.().has(item.recentId),
        toggle: item => { if (item.recentId) return options.toggleActive?.(item.recentId); },
      } : undefined,
    });
    picker.input.id = 'project-menu-filter';
    picker.input.maxLength = 128;
    picker.input.spellcheck = false;
    pane.append(picker.element);
    if (options.running) pane.append(runningButton);
    runningButton.onclick = () => { onlyRunning = !onlyRunning; void picker.refresh(); picker.input.focus(); };
    const actions = uiPart('actions');
    for (const [label, run, hint] of [
      ['Project roots…', options.manage, 'Folders that hold your projects, such as D:\\Projects'],
      ['Open one folder…', options.browse, 'Open a single folder as a project, without adding a root'],
      ['New project…', options.create, undefined],
    ] as const) {
      const action = uiButton(label);
      if (hint) action.title = hint;
      action.onclick = () => { close(); run(); };
      actions.append(action);
    }
    pane.append(actions);
    close = openSheet(options.button, pane, { onClose() {
      activeClose = undefined;
      client?.close();
      picker.destroy();
    } });
    if (!pane.isConnected) { picker.destroy(); return; }
    activeClose = close;
    picker.input.focus();
    client?.open('');
    void options.load().then(value => { recent = value; }, () => { failed = true; }).finally(() => {
      ready = true;
      if (current === generation && pane.isConnected) void picker.refresh();
    });
  };
}