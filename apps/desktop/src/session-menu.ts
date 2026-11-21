import { openMenu, uiPart, type MenuItem } from '@acaprino/imago/ui';
export type SessionMenuHarness = { id: string; label: string };
/** A project the menu offers when more than one is listed in the sidebar. */
export type SessionMenuProject = { label: string; path: string };
type Options = {
  /** Starts a session with this harness in the project chosen in the menu. */
  start: (harnessId: string, projectPath: string) => void;
  /** The rare case: a session in a folder that is not one of the projects. */
  otherFolder: (projectPath: string) => void;
};
const rank = (id: string) => { const known = ['claude_code', 'codex', 'opencode'].indexOf(id); return known < 0 ? (id === 'powershell' ? 99 : 50) : known; };

/** New session without a dialog: a menu of harnesses hung from the control that was pressed.
 * One choice starts the session in the selected project. The last harness used is focused
 * first, so the shortcut followed by Enter repeats the previous choice. */
export function createSessionMenu(options: Options) {
  let active: { anchor: HTMLElement; close: () => void } | null = null;
  let last = '';
  function close() { active?.close(); }
  function show(target: HTMLElement, harnesses: readonly SessionMenuHarness[], project: string, projectPath = project, choices: readonly SessionMenuProject[] = []) {
    if (active?.anchor === target) { close(); return; }
    close();
    const heading = uiPart('hint');
    let menuElement: HTMLElement | undefined;
    let path = projectPath;
    const place = (label: string, value: string) => {
      path = value;
      if (menuElement) menuElement.dataset.projectPath = value;
      heading.textContent = `New session in ${label}`;
      heading.title = value;
    };
    place(project, projectPath);
    const items: MenuItem[] = [];
    if (choices.length > 1) {
      for (const choice of choices) items.push({
        label: choice.label, checked: () => path === choice.path,
        configure(row) { row.dataset.projectPath = choice.path; row.title = choice.path; row.classList.add('session-menu-project'); },
        run: () => place(choice.label, choice.path),
      });
      items.push('separator');
    }
    const ordered = [...harnesses].sort((a, b) => rank(a.id) - rank(b.id));
    const initial = ordered.find(harness => harness.id === last) ?? ordered[0];
    for (const harness of ordered) items.push({
      label: harness.label, initialFocus: harness === initial,
      configure(row) { row.dataset.harness = harness.id; },
      run() { last = harness.id; options.start(harness.id, path); },
    });
    items.push({ label: 'Other folder…', configure(row) { row.classList.add('session-menu-other'); }, run: () => options.otherFolder(path) });
    const dismiss = openMenu(target, items, {
      heading, configure(menu) { menuElement = menu; menu.dataset.projectPath = path; menu.id = 'session-menu'; menu.setAttribute('aria-label', 'New session'); },
      onClose() { active = null; },
    });
    active = { anchor: target, close: dismiss };
  }
  return { show, close, isOpen: () => !!active, remember(harnessId: string) { last = harnessId; } };
}
