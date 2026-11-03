import { openModal } from './modal.ts';
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
};
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const scalar = (value: unknown, limit: number): value is string => typeof value === 'string' && value.length > 0 && value.length <= limit
  && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
const absoluteFolder = (value: unknown): value is string => scalar(value, 4096)
  && (/^[a-z]:[\\/]/i.test(value) || /^\\\\[^\\/]+[\\/][^\\/]+(?:[\\/]|$)/.test(value));
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

/** Drafting and folder selection never open a terminal. The single explicit
 * Open action owns its request until settlement; it cannot be hidden or repeated. */
export function createNewSession(options: Options) {
  let choices = options.harnesses.length <= 32 ? options.harnesses.filter((item, index, all) =>
    /^[a-z0-9_-]{1,64}$/i.test(item.id) && scalar(item.label, 128) && all.findIndex(candidate => candidate.id === item.id) === index
  ).map(item => ({ ...item })) : [];
  const dialog = document.createElement('dialog'); dialog.id = 'new-session-dialog'; dialog.setAttribute('aria-labelledby', 'new-session-title');
  const form = document.createElement('form'); form.noValidate = true;
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'new-session-title'; title.textContent = 'New session';
  const closeButton = document.createElement('button'); closeButton.type = 'button'; closeButton.textContent = '×'; closeButton.setAttribute('aria-label', 'Close new session'); heading.append(title, closeButton);
  const projectLabel = document.createElement('label'); projectLabel.htmlFor = 'new-session-project'; projectLabel.textContent = 'Project folder';
  const folderRow = document.createElement('div'); folderRow.className = 'new-session-folder';
  const project = document.createElement('input'); project.id = 'new-session-project'; project.type = 'text'; project.maxLength = 4096;
  project.autocomplete = 'off'; project.spellcheck = false; project.placeholder = 'C:\\Projects\\my-project';
  const browse = document.createElement('button'); browse.type = 'button'; browse.textContent = 'Browse…'; browse.hidden = !options.browse; folderRow.append(project, browse);
  const recent = document.createElement('select'); recent.id = 'new-session-recent'; recent.setAttribute('aria-label', 'Recent projects');
  const projectFields = document.createElement('div'); projectFields.className = 'new-session-project-fields'; projectFields.append(projectLabel, folderRow, recent);
  const projectContext = document.createElement('div'); projectContext.id = 'new-session-project-context';
  const contextLabel = document.createElement('span'); contextLabel.textContent = 'Project';
  const contextPath = document.createElement('span'); contextPath.id = 'new-session-project-path';
  const changeProject = document.createElement('button'); changeProject.type = 'button'; changeProject.textContent = 'Change project';
  projectContext.append(contextLabel, contextPath, changeProject);
  const harnessLabel = document.createElement('label'); harnessLabel.htmlFor = 'new-session-harness'; harnessLabel.textContent = 'Agent';
  const harness = document.createElement('select'); harness.id = 'new-session-harness';
  for (const choice of choices) { const item = document.createElement('option'); item.value = choice.id; item.textContent = choice.label; harness.append(item); }
  const status = document.createElement('p'); status.id = 'new-session-status'; status.setAttribute('role', 'status');
  const error = document.createElement('p'); error.id = 'new-session-error'; error.setAttribute('role', 'alert'); error.hidden = true;
  project.setAttribute('aria-describedby', error.id);
  const actions = document.createElement('div'); actions.className = 'dialog-actions';
  const cancel = document.createElement('button'); cancel.type = 'button'; cancel.textContent = 'Cancel';
  const submit = document.createElement('button'); submit.type = 'submit'; submit.className = 'primary'; submit.textContent = 'Open'; actions.append(cancel, submit);
  form.append(heading, projectContext, projectFields, harnessLabel, harness, status, error, actions); dialog.append(form);
  (document.querySelector('#app') ?? document.body).append(dialog);

  let projects: NewSessionProject[] = []; let pending: 'projects' | 'browse' | 'open' | null = null;
  let generation = 0; let destroyed = false; let choosingProject = true;
  function showError(message: string) { error.textContent = message; error.hidden = false; }
  function clearError() { error.textContent = ''; error.hidden = true; }
  function render() {
    if (destroyed) return;
    const opening = pending === 'open'; dialog.setAttribute('aria-busy', String(pending !== null));
    project.disabled = opening || pending === 'browse'; harness.disabled = opening || !choices.length;
    recent.disabled = pending !== null || !projects.length; browse.disabled = pending !== null || !options.browse;
    projectFields.hidden = !choosingProject; projectContext.hidden = choosingProject;
    contextPath.textContent = project.value; changeProject.disabled = pending !== null;
    submit.disabled = pending !== null || !choices.length; cancel.disabled = opening; closeButton.disabled = opening;
    status.textContent = opening ? 'Opening…' : pending === 'projects' ? 'Loading projects…' : pending === 'browse' ? 'Choose a project folder…' : '';
  }
  function fillRecent() {
    const placeholder = document.createElement('option'); placeholder.value = ''; placeholder.textContent = 'Recent projects';
    recent.replaceChildren(placeholder);
    projects.forEach((item, index) => { const option = document.createElement('option'); option.value = String(index); option.textContent = item.path; recent.append(option); });
    selectRecent();
  }
  function selectRecent() {
    const index = projects.findIndex(item => item.path === project.value);
    recent.value = index < 0 ? '' : String(index);
  }
  async function loadProjects() {
    if (destroyed || pending) return;
    const at = generation; pending = 'projects'; render();
    try {
      const received = projectList(await options.projects());
      if (destroyed || at !== generation || !dialog.open) return;
      projects = received; fillRecent();
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
      project.value = received; selectRecent();
    } catch (failure) {
      if (!destroyed && at === generation && dialog.open) showError(plainFailure(failure, 'The folder could not be selected. Enter its full path to continue.'));
    } finally { pending = null; render(); if (!destroyed && at === generation && dialog.open) project.focus(); }
  }
  function close(): boolean {
    if (pending === 'open') return false;
    generation++; if (dialog.open) dialog.close();
    project.value = ''; projects = []; fillRecent(); clearError(); render(); return true;
  }
  async function open() {
    if (destroyed || pending || !dialog.open) return;
    const path = project.value.trim(); const agent = choices.find(choice => choice.id === harness.value);
    if (!absoluteFolder(path)) { showError('Choose a project folder or enter its full path.'); project.focus(); return; }
    if (!agent) { showError('Choose an available agent.'); harness.focus(); return; }
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
    if (destroyed || dialog.open || document.querySelector('dialog[open]')) return false;
    generation++; project.value = absoluteFolder(defaultProject) ? defaultProject : ''; projects = []; fillRecent(); clearError();
    choosingProject = !project.value;
    if (choices.some(choice => choice.id === defaultHarness)) harness.value = defaultHarness!;
    if (!choices.some(choice => choice.id === harness.value)) harness.value = choices[0]?.id ?? '';
    if (!choices.length) showError('No agents are available yet.');
    render(); openModal(dialog); (choosingProject ? project : harness).focus();
    if (choosingProject && !pending) void loadProjects(); return true;
  }
  function setHarnesses(values: readonly NewSessionHarness[]): boolean {
    if (destroyed || pending === 'open' || !Array.isArray(values) || values.length > 32
      || values.some(item => !record(item) || !scalar(item.id, 64) || !/^[a-z0-9_-]+$/i.test(item.id) || !scalar(item.label, 128))
      || new Set(values.map(item => item.id)).size !== values.length) return false;
    const selected = harness.value; choices = values.map(item => ({ id: item.id, label: item.label }));
    harness.replaceChildren();
    for (const choice of choices) { const item = document.createElement('option'); item.value = choice.id; item.textContent = choice.label; harness.append(item); }
    harness.value = choices.some(choice => choice.id === selected) ? selected : choices[0]?.id ?? '';
    render(); return true;
  }
  changeProject.onclick = () => {
    if (pending) return;
    choosingProject = true; clearError(); render(); project.focus(); void loadProjects();
  };
  project.oninput = () => { clearError(); selectRecent(); };
  recent.onchange = () => {
    const selected = recent.value === '' ? null : projects[Number(recent.value)];
    if (selected && !pending) { project.value = selected.path; clearError(); project.focus(); }
  };
  harness.onchange = clearError; browse.onclick = () => { void browseFolder(); };
  form.onsubmit = event => { event.preventDefault(); void open(); };
  cancel.onclick = close; closeButton.onclick = close;
  dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
  dialog.addEventListener('close', () => {
    if (!dialog.open) { generation++; project.value = ''; projects = []; fillRecent(); clearError(); options.restoreFocus?.(); }
  });
  fillRecent(); render();
  return { show, close, setHarnesses, destroy: (): boolean => { if (!close()) return false; destroyed = true; dialog.remove(); return true; } };
}
