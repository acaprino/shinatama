import { invoke } from '@tauri-apps/api/core';
import { openModal } from './modal.ts';
import { beginMutation, cancelMutation, createRequestId, type Project } from './project-catalog.ts';

type CreationOptions = { revision: number; roots: { id: string; path: string }[]; defaultRootId: string | null };
type CreationFailure = { message: string; recoveryPath?: string; code?: string };
type Options = {
  desktop: boolean;
  onCreated: (project: Project) => void;
  notify: (message: string, tone?: 'info' | 'error') => void;
  /** Dismiss only an idle project picker before opening this modal. */
  prepareOpen?: () => boolean;
};
type Creation = {
  id: string; phase: 'pending' | 'cancelling' | 'committing' | 'cancelled';
  registration: Promise<void> | null; cancellation: Promise<void> | null;
  reserved: boolean; dispatched: boolean; cancelRequested: boolean; settled: boolean;
  failure: CreationFailure | null;
};

function failureDetails(value: unknown): CreationFailure {
  if (typeof value === 'object' && value !== null && 'message' in value && typeof value.message === 'string') {
    return { message: value.message,
      ...('recoveryPath' in value && typeof value.recoveryPath === 'string' ? { recoveryPath: value.recoveryPath } : {}),
      ...('code' in value && typeof value.code === 'string' ? { code: value.code } : {}) };
  }
  return { message: String(value) };
}
function failureText(failure: CreationFailure) {
  return failure.message + (failure.recoveryPath ? ` Preserved work: ${failure.recoveryPath}. Review this folder before trying again.` : '');
}
function nameProblem(name: string) {
  if (!name.trim()) return 'Enter a project folder name.';
  if (name === '.' || name === '..' || /[<>:"/\\|?*\u0000-\u001f]/.test(name)) return 'Use one folder name without path separators or Windows reserved characters.';
  if (/[. ]$/.test(name)) return 'The folder name cannot end with a dot or space.';
  if (/^(?:CON|PRN|AUX|NUL|CONIN\$|CONOUT\$|COM[1-9¹²³]|LPT[1-9¹²³])(?:\.|$)/i.test(name)) return 'Choose a name that Windows does not reserve for a device.';
  return '';
}

/** Project creation changes selection only after the backend returns its committed project. */
export function createNewProjectDialog(options: Options) {
  const dialog = document.createElement('dialog'); dialog.id = 'new-project-dialog';
  dialog.setAttribute('aria-labelledby', 'new-project-title');
  const form = document.createElement('form');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'new-project-title'; title.textContent = 'New project';
  const close = document.createElement('button'); close.type = 'button'; close.textContent = '×'; close.setAttribute('aria-label', 'Close new project dialog');
  heading.append(title, close);
  const description = document.createElement('p'); description.textContent = 'Create a project folder inside one of your configured roots.';
  const rootLabel = document.createElement('label'); rootLabel.htmlFor = 'new-project-root'; rootLabel.textContent = 'Project root';
  const root = document.createElement('select'); root.id = 'new-project-root'; root.required = true;
  const rootHelp = document.createElement('p'); rootHelp.className = 'hint'; rootHelp.id = 'new-project-root-help';
  root.setAttribute('aria-describedby', rootHelp.id);
  const nameLabel = document.createElement('label'); nameLabel.htmlFor = 'new-project-name'; nameLabel.textContent = 'Project folder name';
  const name = document.createElement('input'); name.id = 'new-project-name'; name.required = true; name.maxLength = 255;
  name.autocomplete = 'off'; name.spellcheck = false; name.placeholder = 'my-project';
  const destination = document.createElement('p'); destination.className = 'hint'; destination.id = 'new-project-destination';
  destination.setAttribute('aria-live', 'polite'); name.setAttribute('aria-describedby', destination.id);
  const gitLabel = document.createElement('label'); gitLabel.id = 'new-project-git-label';
  const git = document.createElement('input'); git.type = 'checkbox'; git.id = 'new-project-git'; git.checked = false;
  const gitText = document.createElement('span'); gitText.textContent = 'Initialize Git repository · default branch main'; gitLabel.append(git, gitText);
  const status = document.createElement('p'); status.className = 'hint'; status.id = 'new-project-status'; status.setAttribute('role', 'status');
  const error = document.createElement('p'); error.id = 'new-project-error'; error.setAttribute('role', 'alert'); error.hidden = true;
  const actions = document.createElement('div'); actions.className = 'dialog-actions';
  const reload = document.createElement('button'); reload.type = 'button'; reload.textContent = 'Reload roots';
  const cancel = document.createElement('button'); cancel.type = 'button'; cancel.textContent = 'Cancel';
  const submit = document.createElement('button'); submit.type = 'submit'; submit.className = 'primary'; submit.textContent = 'Create project';
  actions.append(reload, cancel, submit);
  form.append(heading, description, rootLabel, root, rootHelp, nameLabel, name, destination, gitLabel, status, error, actions);
  dialog.append(form); (document.querySelector('#app') ?? document.body).append(dialog);

  let configuration: CreationOptions | null = null;
  let optionsRead: Promise<CreationOptions> | null = null;
  let loading = false;
  let generation = 0;
  let operation: Creation | null = null;
  function showError(failure: CreationFailure) { error.textContent = failureText(failure); error.hidden = false; }
  function render() {
    const busy = operation !== null;
    const cancelling = operation?.phase === 'cancelling';
    const committing = operation?.phase === 'committing';
    form.setAttribute('aria-busy', String(busy || loading));
    root.disabled = busy || loading; name.disabled = busy; git.disabled = busy;
    reload.disabled = busy || loading;
    submit.disabled = busy || loading || !configuration || !root.value;
    cancel.disabled = cancelling; close.disabled = cancelling;
    cancel.textContent = committing ? 'Close; creation continues' : 'Cancel';
    close.setAttribute('aria-label', committing ? 'Close dialog; project creation continues' : 'Close new project dialog');
    const selected = configuration?.roots.find(entry => entry.id === root.value);
    const separator = selected?.path.endsWith('\\') || selected?.path.endsWith('/') ? '' : '\\';
    destination.textContent = selected ? `Destination: ${selected.path}${separator}${name.value || 'project-name'}` : '';
    status.textContent = cancelling ? 'Cancelling project creation…'
      : committing ? 'Creation has entered its final save. Closing this dialog keeps it running.'
      : busy ? 'Preparing the project folder…'
      : loading ? 'Reading project roots…' : '';
  }
  async function loadOptions() {
    if (operation || loading) return;
    const currentGeneration = generation; loading = true; error.hidden = true; render();
    try {
      // One outstanding read across close/reopen; a slow disk never adds more workers.
      if (!optionsRead) {
        const pending = invoke<CreationOptions>('new_project_options').finally(() => { if (optionsRead === pending) optionsRead = null; });
        optionsRead = pending;
      }
      const received = await optionsRead;
      if (!dialog.open || generation !== currentGeneration) return;
      configuration = received;
      const placeholder = document.createElement('option'); placeholder.value = ''; placeholder.disabled = true; placeholder.textContent = 'Choose a project root';
      const entries = received.roots.slice(0, 16).map(entry => {
        const option = document.createElement('option'); option.value = entry.id; option.textContent = entry.path; return option;
      });
      root.replaceChildren(placeholder, ...entries);
      root.value = received.roots.some(entry => entry.id === received.defaultRootId) ? received.defaultRootId! : '';
      rootHelp.textContent = received.roots.length ? 'The configured default is selected. Choose a different root explicitly if needed.'
        : 'Configure a project root in the project picker before creating a project.';
    } catch (failure) {
      if (dialog.open && generation === currentGeneration) { configuration = null; showError(failureDetails(failure)); }
    } finally { if (generation === currentGeneration) { loading = false; render(); } }
  }
  function complete(project: Project) {
    const wasOpen = dialog.open;
    if (wasOpen) dialog.close();
    try { options.onCreated(project); }
    catch {
      // The folder is already committed. A display callback cannot turn it into a failed create.
      options.notify(`Project created: ${project.path}. Refresh the project picker to open it.`); return;
    }
    if (!wasOpen) options.notify(`Project created: ${project.name}.`, 'info');
  }
  async function create() {
    if (operation || loading || !configuration) return;
    name.setCustomValidity(nameProblem(name.value));
    if (!form.reportValidity()) return;
    if (!configuration.roots.some(entry => entry.id === root.value)) return;
    // Send typed identifiers/revision and the exact input. Never read a path back from display text.
    const request = { expectedRevision: configuration.revision, rootId: root.value, name: name.value, gitInit: git.checked };
    const current: Creation = { id: createRequestId(), phase: 'pending', registration: null, cancellation: null,
      reserved: false, dispatched: false, cancelRequested: false, settled: false, failure: null };
    operation = current; error.hidden = true; render();
    try {
      current.registration = beginMutation(current.id);
      await current.registration; current.reserved = true;
      if (current.cancelRequested) { await current.cancellation; return; }
      current.dispatched = true;
      const project = await invoke<Project>('new_project_create', { request, requestId: current.id });
      if (operation === current && current.phase !== 'cancelled') complete(project);
    } catch (failure) {
      current.failure = failureDetails(failure);
      if (current.reserved && !current.cancelRequested) void cancelMutation(current.id).catch(() => {});
      if (current.failure.recoveryPath) options.notify(failureText(current.failure));
      if (operation === current && current.phase !== 'cancelled' && current.phase !== 'cancelling') {
        if (dialog.open) showError(current.failure);
        else if (!current.failure.recoveryPath) options.notify(failureText(current.failure));
      }
    } finally {
      current.settled = true;
      const reservedCancellation = current.reserved && !current.dispatched && current.cancelRequested && current.phase !== 'cancelled';
      if (operation === current && current.phase !== 'cancelling' && !reservedCancellation) { operation = null; render(); }
    }
  }
  async function cancelOrClose() {
    const current = operation;
    if (!current || current.phase === 'cancelled') { dialog.close(); return; }
    if (current.phase === 'committing') { dialog.close(); options.notify('Project creation continues. Its result will be reported here.', 'info'); return; }
    if (current.phase === 'cancelling') return;
    current.cancelRequested = true; current.phase = 'cancelling'; render();
    current.cancellation = (async () => {
      try {
        // Cancel during begin waits for its acknowledgement; Create is never dispatched between.
        await current.registration;
        if (operation !== current) return;
        const acknowledgement = await cancelMutation(current.id);
        if (operation !== current) return;
        if (acknowledgement.status === 'cancelled' || !current.dispatched) {
          current.phase = 'cancelled'; dialog.close();
          if (current.settled) operation = null;
        } else if (current.settled) {
          operation = null; dialog.close();
          options.notify(current.failure ? failureText(current.failure) : 'Project creation finished before cancellation. The completed project has been kept.', current.failure ? 'error' : 'info');
        } else current.phase = 'committing';
        render();
      } catch (failure) {
        if (operation !== current) return;
        if (!current.reserved) { current.phase = 'cancelled'; dialog.close(); if (current.settled) operation = null; }
        else if (current.settled && current.dispatched) operation = null;
        else current.phase = 'pending';
        if (current.reserved) showError(current.failure ?? failureDetails(failure));
        render();
      }
    })();
    await current.cancellation;
  }
  function open() {
    if (dialog.open) return false;
    if (!options.desktop) { options.notify('Open the desktop app to create a project.', 'info'); return false; }
    if (operation) { options.notify('The previous project creation is still returning. Existing terminals remain available.'); return false; }
    if (options.prepareOpen && !options.prepareOpen()) return false;
    if (document.querySelector('dialog[open][aria-modal="true"]')) return false;
    generation++; configuration = null; loading = false;
    name.value = ''; name.setCustomValidity(''); git.checked = false; error.hidden = true;
    root.replaceChildren(); rootHelp.textContent = ''; render(); openModal(dialog); name.focus(); void loadOptions(); return true;
  }
  form.onsubmit = event => { event.preventDefault(); void create(); };
  name.oninput = () => { name.setCustomValidity(''); error.hidden = true; render(); };
  root.onchange = () => { error.hidden = true; render(); };
  reload.onclick = () => { void loadOptions(); };
  close.onclick = cancel.onclick = () => { void cancelOrClose(); };
  dialog.addEventListener('cancel', event => { if (operation) { event.preventDefault(); void cancelOrClose(); } });
  dialog.addEventListener('close', () => { generation++; loading = false; });
  return {
    open,
    handleKey(event: KeyboardEvent) {
      if (event.key.toLowerCase() !== 'n' || event.ctrlKey || event.altKey || event.metaKey || event.repeat) return false;
      // A character key acts only while its control has focus, never on stray focus.
      const target = event.target instanceof HTMLElement ? event.target : null;
      if (!target?.closest('#project')) return false;
      if (!open()) return false;
      event.preventDefault(); return true;
    },
  };
}
