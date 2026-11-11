import { openSheet } from './modal.ts';
import './new-session.css';

export type NewSessionProject = { id: string; path: string };
export type NewSessionHarness = { id: string; label: string };
export type NewSessionSelection = { projectPath: string; harnessId: string; rows: 24; cols: 80 };
type Options = {
  projects: () => Promise<readonly NewSessionProject[]>;
  harnesses: readonly NewSessionHarness[];
  browse?: () => Promise<string | null>;
  open: (selection: NewSessionSelection) => Promise<void>;
  reportError?: (message: string) => void;
  restoreFocus?: () => void;
  /** The pane the session will open in; the sheet docks to its top edge. */
  anchor?: () => Element | null;
};
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const scalar = (value: unknown, limit: number): value is string => typeof value === 'string' && value.length > 0 && value.length <= limit
  && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
const absoluteFolder = (value: unknown): value is string => scalar(value, 4096)
  && (/^[a-z]:[\\/]/i.test(value) || /^\\\\[^\\/]+[\\/][^\\/]+(?:[\\/]|$)/.test(value));
const folderName = (path: string) => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;
const samePath = (a: string, b: string) => a.replace(/[\\/]+$/, '').toLowerCase() === b.replace(/[\\/]+$/, '').toLowerCase();
function projectList(value: unknown): NewSessionProject[] {
  if (!Array.isArray(value) || value.length > 100) throw new Error('Recent projects could not be loaded. Choose a folder to continue.');
  const result: NewSessionProject[] = [];
  for (const item of value) {
    if (!record(item) || !scalar(item.id, 256) || !absoluteFolder(item.path) || result.some(project => project.id === item.id)) {
      throw new Error('Recent projects could not be loaded. Choose a folder to continue.');
    }
    result.push({ id: item.id, path: item.path });
  }
  return result;
}
function plainFailure(value: unknown, fallback: string): string {
  const message = typeof value === 'string' ? value : value instanceof Error ? value.message : record(value) ? value.message : null;
  return scalar(message, 512) ? message : fallback;
}

/** Shown in place, docked to the pane the session will open in; nothing else is blocked.
 * Drafting and folder selection never open a terminal. The single explicit Open action
 * (the button, Enter, or a double click on a project or agent) owns its request until
 * settlement; it cannot be hidden or repeated. */
export function createNewSession(options: Options) {
  let choices = options.harnesses.length <= 32 ? options.harnesses.filter((item, index, all) =>
    /^[a-z0-9_-]{1,64}$/i.test(item.id) && scalar(item.label, 128) && all.findIndex(candidate => candidate.id === item.id) === index
  ).map(item => ({ ...item })) : [];
  const dialog = document.createElement('dialog'); dialog.id = 'new-session-dialog'; dialog.setAttribute('aria-labelledby', 'new-session-title');
  const form = document.createElement('form'); form.noValidate = true;
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'new-session-title'; title.textContent = 'New session';
  const closeButton = document.createElement('button'); closeButton.type = 'button'; closeButton.textContent = '×'; closeButton.setAttribute('aria-label', 'Close new session'); heading.append(title, closeButton);

  // 1. Project: pick from the list, or browse / type a folder.
  const projectLabel = document.createElement('label'); projectLabel.htmlFor = 'new-session-project'; projectLabel.textContent = 'Project';
  const list = document.createElement('div'); list.id = 'new-session-projects'; list.setAttribute('role', 'listbox'); list.setAttribute('aria-label', 'Recent projects');
  const folderRow = document.createElement('div'); folderRow.className = 'new-session-folder';
  const project = document.createElement('input'); project.id = 'new-session-project'; project.type = 'text'; project.maxLength = 4096;
  project.autocomplete = 'off'; project.spellcheck = false; project.placeholder = 'Folder path, or pick one above';
  const browse = document.createElement('button'); browse.type = 'button'; browse.textContent = 'Browse…'; browse.hidden = !options.browse; folderRow.append(project, browse);

  // 2. Agent: one tap. The select stays in the DOM (visually hidden) as the single source of truth.
  const harnessLabel = document.createElement('label'); harnessLabel.htmlFor = 'new-session-harness'; harnessLabel.textContent = 'Agent';
  const harness = document.createElement('select'); harness.id = 'new-session-harness'; harness.className = 'sr-only';
  const tiles = document.createElement('div'); tiles.id = 'new-session-agents'; tiles.setAttribute('role', 'radiogroup'); tiles.setAttribute('aria-label', 'Agent');
  const status = document.createElement('p'); status.id = 'new-session-status'; status.setAttribute('role', 'status');
  const error = document.createElement('p'); error.id = 'new-session-error'; error.setAttribute('role', 'alert'); error.hidden = true;
  project.setAttribute('aria-describedby', error.id);
  const actions = document.createElement('div'); actions.className = 'dialog-actions';
  const cancel = document.createElement('button'); cancel.type = 'button'; cancel.textContent = 'Cancel';
  const submit = document.createElement('button'); submit.type = 'submit'; submit.className = 'primary'; submit.textContent = 'Open'; actions.append(cancel, submit);
  const hint = document.createElement('span'); hint.className = 'new-session-keys'; hint.textContent = 'Enter opens · Esc closes'; actions.prepend(hint);
  form.append(heading, harnessLabel, harness, tiles, projectLabel, folderRow, list, status, error, actions); dialog.append(form);
  (document.querySelector('#app') ?? document.body).append(dialog);

  let projects: NewSessionProject[] = []; let pending: 'projects' | 'browse' | 'open' | null = null;
  let generation = 0; let destroyed = false; let lastHarness = '';
  function showError(message: string) { error.textContent = message; error.hidden = false; }
  function clearError() { error.textContent = ''; error.hidden = true; }
  function syncHarness() {
    harness.replaceChildren();
    for (const choice of choices) { const item = document.createElement('option'); item.value = choice.id; item.textContent = choice.label; harness.append(item); }
  }
  const agentOrder = (id: string) => { const rank = ['claude_code', 'codex', 'opencode'].indexOf(id); return rank < 0 ? (id === 'powershell' ? 99 : 50) : rank; };
  function paintTiles() {
    tiles.replaceChildren();
    for (const choice of [...choices].sort((a, b) => agentOrder(a.id) - agentOrder(b.id))) {
      const tile = document.createElement('button'); tile.type = 'button'; tile.className = 'new-session-agent'; tile.textContent = choice.label;
      tile.dataset.agent = choice.id; tile.setAttribute('role', 'radio'); tile.setAttribute('aria-checked', String(harness.value === choice.id));
      tile.disabled = pending === 'open';
      // The tiles are repainted on every choice, so a double click is read from the click count.
      tile.onclick = event => {
        if (pending === 'open') return; harness.value = choice.id; lastHarness = choice.id; clearError(); paintTiles();
        tiles.querySelector<HTMLButtonElement>('[aria-checked="true"]')?.focus();
        if (event.detail >= 2) void open();
      };
      tiles.append(tile);
    }
  }
  function paintProjects() {
    list.replaceChildren();
    if (!projects.length) {
      const empty = document.createElement('p'); empty.className = 'hint new-session-empty';
      empty.textContent = pending === 'projects' ? 'Loading projects…' : 'No projects yet. Browse for a folder.'; list.append(empty); return;
    }
    for (const item of projects) {
      const row = document.createElement('button'); row.type = 'button'; row.className = 'new-session-project-row'; row.setAttribute('role', 'option');
      const selected = samePath(item.path, project.value.trim()); row.setAttribute('aria-selected', String(selected));
      row.disabled = pending === 'open'; row.title = item.path;
      const name = document.createElement('strong'); name.textContent = folderName(item.path);
      const path = document.createElement('span'); path.textContent = item.path; row.append(name, path);
      row.onclick = event => { if (pending) return; project.value = item.path; clearError(); paintProjects(); submit.focus(); if (event.detail >= 2) void open(); };
      list.append(row);
    }
  }
  function render() {
    if (destroyed) return;
    const opening = pending === 'open'; dialog.setAttribute('aria-busy', String(pending !== null));
    project.disabled = opening || pending === 'browse'; harness.disabled = opening || !choices.length;
    browse.disabled = pending !== null || !options.browse;
    submit.disabled = pending !== null || !choices.length; cancel.disabled = opening; closeButton.disabled = opening;
    status.textContent = opening ? 'Opening… the first session can take a few seconds.' : pending === 'projects' ? '' : pending === 'browse' ? 'Choose a project folder…' : '';
    paintProjects(); paintTiles();
  }
  async function loadProjects() {
    if (destroyed || pending) return;
    const at = generation; pending = 'projects'; render();
    try {
      const received = projectList(await options.projects());
      if (destroyed || at !== generation || !dialog.open) return;
      projects = received;
      // Nothing chosen yet: preselect the most recent project.
      if (!project.value.trim() && projects[0]) project.value = projects[0].path;
    } catch {
      if (!destroyed && at === generation && dialog.open) showError('Recent projects could not be loaded. Choose a folder to continue.');
    } finally { pending = null; render(); }
  }
  async function browseFolder() {
    if (destroyed || pending || !options.browse) return;
    const at = generation; pending = 'browse'; clearError(); render();
    try {
      const received = await options.browse();
      if (destroyed || at !== generation || !dialog.open || received === null) return;
      if (!absoluteFolder(received)) throw new Error('Choose an existing project folder.');
      project.value = received;
      if (!projects.some(item => samePath(item.path, received))) projects = [{ id: `browsed:${received}`, path: received }, ...projects];
    } catch (failure) {
      if (!destroyed && at === generation && dialog.open) showError(plainFailure(failure, 'The folder could not be selected. Enter its full path to continue.'));
    } finally { pending = null; render(); if (!destroyed && at === generation && dialog.open) submit.focus(); }
  }
  function close(): boolean {
    if (pending === 'open') return false;
    generation++; if (dialog.open) dialog.close();
    project.value = ''; projects = []; clearError(); render(); return true;
  }
  async function open() {
    if (destroyed || pending || !dialog.open) return;
    const path = project.value.trim(); const agent = choices.find(choice => choice.id === harness.value);
    if (!absoluteFolder(path)) { showError('Choose a project from the list, or browse for a folder.'); project.focus(); return; }
    if (!agent) { showError('Choose an available agent.'); return; }
    const at = generation; pending = 'open'; clearError(); render(); let opened = false;
    try {
      await options.open({ projectPath: path, harnessId: agent.id, rows: 24, cols: 80 }); opened = true;
    } catch (failure) {
      if (!destroyed && at === generation && dialog.open) {
        const message = plainFailure(failure, 'The session could not be opened. Check the project folder and try again.');
        showError(message); options.reportError?.(message);
      }
    } finally {
      pending = null;
      if (opened && !destroyed && at === generation) close(); else render();
    }
  }
  function show(defaultProject?: string, defaultHarness?: string): boolean {
    if (destroyed || dialog.open) return false;
    generation++; project.value = absoluteFolder(defaultProject) ? defaultProject : ''; projects = []; clearError();
    if (choices.some(choice => choice.id === defaultHarness)) harness.value = defaultHarness!;
    else if (choices.some(choice => choice.id === lastHarness)) harness.value = lastHarness;
    else if (!lastHarness && choices.some(choice => choice.id === 'claude_code')) harness.value = 'claude_code';
    if (!choices.some(choice => choice.id === harness.value)) harness.value = choices[0]?.id ?? '';
    if (!choices.length) showError('No agents are available yet.');
    render();
    if (!openSheet(dialog, { anchor: () => options.anchor?.() ?? null, placement: 'pane-top', lightDismiss: () => pending !== 'open' })) return false;
    submit.focus();
    if (!pending) void loadProjects(); return true;
  }
  function setHarnesses(values: readonly NewSessionHarness[]): boolean {
    if (destroyed || pending === 'open' || !Array.isArray(values) || values.length > 32
      || values.some(item => !record(item) || !scalar(item.id, 64) || !/^[a-z0-9_-]+$/i.test(item.id) || !scalar(item.label, 128))
      || new Set(values.map(item => item.id)).size !== values.length) return false;
    const selected = harness.value; choices = values.map(item => ({ id: item.id, label: item.label }));
    syncHarness();
    harness.value = choices.some(choice => choice.id === selected) ? selected : choices[0]?.id ?? '';
    render(); return true;
  }
  project.oninput = () => { clearError(); paintProjects(); };
  harness.onchange = () => { clearError(); lastHarness = harness.value; paintTiles(); };
  browse.onclick = () => { void browseFolder(); };
  form.onsubmit = event => { event.preventDefault(); void open(); };
  // Enter on a project or an agent takes that choice and opens: one key instead of select, then Open.
  form.addEventListener('keydown', event => {
    const target = event.target instanceof HTMLElement ? event.target.closest<HTMLButtonElement>('.new-session-project-row,.new-session-agent') : null;
    if (event.key !== 'Enter' || !target || event.repeat) return;
    event.preventDefault(); target.click(); void open();
  });
  cancel.onclick = close; closeButton.onclick = close;
  dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
  dialog.addEventListener('close', () => {
    if (!dialog.open) { generation++; project.value = ''; projects = []; clearError(); options.restoreFocus?.(); }
  });
  syncHarness(); render();
  return { show, close, setHarnesses, destroy: (): boolean => { if (!close()) return false; destroyed = true; dialog.remove(); return true; } };
}
