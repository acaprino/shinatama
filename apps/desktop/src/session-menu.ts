export type SessionMenuHarness = { id: string; label: string };
type Options = {
  /** Starts a session with this harness in the project the menu was opened for. */
  start: (harnessId: string) => void;
  /** The rare case: a session in a folder that is not the selected project. */
  otherFolder: () => void;
};
const rank = (id: string) => { const known = ['claude_code', 'codex', 'opencode'].indexOf(id); return known < 0 ? (id === 'powershell' ? 99 : 50) : known; };

/** New session without a dialog: a menu of harnesses hung from the control that was pressed.
 * One choice starts the session in the selected project. The last harness used is focused
 * first, so the shortcut followed by Enter repeats the previous choice. */
export function createSessionMenu(options: Options) {
  const menu = document.createElement('div'); menu.id = 'session-menu'; menu.className = 'toolbar-menu'; menu.hidden = true;
  menu.setAttribute('role', 'menu'); menu.setAttribute('aria-label', 'New session');
  (document.querySelector('#app') ?? document.body).append(menu);
  let anchor: HTMLElement | null = null; let last = '';
  function close(restoreFocus = false) {
    if (menu.hidden) return;
    menu.hidden = true; anchor?.setAttribute('aria-expanded', 'false');
    if (restoreFocus && anchor?.isConnected) anchor.focus();
  }
  const items = () => [...menu.querySelectorAll<HTMLButtonElement>('button')];
  function show(target: HTMLElement, harnesses: readonly SessionMenuHarness[], project: string, projectPath = project) {
    if (!menu.hidden && anchor === target) { close(true); return; }
    for (const other of document.querySelectorAll<HTMLElement>('.toolbar-menu')) if (other !== menu && !other.hidden) other.dispatchEvent(new Event('toolbar-menu-close'));
    close(); anchor = target; menu.replaceChildren();
    const where = document.createElement('p'); where.className = 'toolbar-menu-heading'; where.textContent = `New session in ${project}`; where.title = projectPath; menu.dataset.projectPath = projectPath; menu.append(where);
    for (const harness of [...harnesses].sort((a, b) => rank(a.id) - rank(b.id))) {
      const item = document.createElement('button'); item.type = 'button'; item.className = 'toolbar-menu-action session-menu-harness';
      item.setAttribute('role', 'menuitem'); item.dataset.harness = harness.id; item.textContent = harness.label;
      item.onclick = () => { last = harness.id; close(); options.start(harness.id); };
      menu.append(item);
    }
    const other = document.createElement('button'); other.type = 'button'; other.className = 'toolbar-menu-action session-menu-other';
    other.setAttribute('role', 'menuitem'); other.textContent = 'Other folder…'; other.onclick = () => { close(); options.otherFolder(); };
    menu.append(other);
    menu.hidden = false; target.setAttribute('aria-haspopup', 'menu'); target.setAttribute('aria-expanded', 'true');
    // Under the control, kept inside the window; above it when there is no room below.
    const box = target.getBoundingClientRect(); const width = menu.offsetWidth; const height = menu.offsetHeight;
    const below = box.bottom + 4 + height <= window.innerHeight - 8;
    menu.style.left = `${Math.max(4, Math.min(box.left, window.innerWidth - width - 4))}px`;
    menu.style.top = `${below ? box.bottom + 4 : Math.max(4, box.top - height - 4)}px`;
    (items().find(item => item.dataset.harness === last) ?? items()[0])?.focus();
  }
  menu.addEventListener('toolbar-menu-close', () => close());
  menu.addEventListener('keydown', event => {
    const all = items(); const index = all.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); all[(index + (event.key === 'ArrowDown' ? 1 : all.length - 1)) % all.length]?.focus(); }
    else if (event.key === 'Home') { event.preventDefault(); all[0]?.focus(); }
    else if (event.key === 'End') { event.preventDefault(); all.at(-1)?.focus(); }
    else if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(true); }
    else if (event.key === 'Tab') close();
  });
  document.addEventListener('pointerdown', event => {
    if (!menu.hidden && event.target instanceof Node && !menu.contains(event.target) && !anchor?.contains(event.target)) close();
  });
  window.addEventListener('resize', () => close());
  return { show, close, isOpen: () => !menu.hidden, remember(harnessId: string) { last = harnessId; } };
}
