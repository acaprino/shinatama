import { invoke } from '@tauri-apps/api/core';

type Project = { path: string; name: string; last_opened: number; available: boolean };
type Options = { desktop: boolean; getPath: () => string; onSelected: (path: string) => void; reportError: (error: unknown) => void };

export function createProjectPicker(options: Options) {
  const dialog = document.querySelector<HTMLDialogElement>('#project-dialog')!;
  const form = document.querySelector<HTMLFormElement>('#project-form')!;
  const folder = document.querySelector<HTMLInputElement>('#folder')!;
  const filter = document.querySelector<HTMLInputElement>('#project-filter')!;
  const list = document.querySelector<HTMLDivElement>('#recent-project-list')!;
  const error = document.querySelector<HTMLParagraphElement>('#project-error')!;
  const submit = form.querySelector<HTMLButtonElement>('button[type="submit"]')!;
  let projects: Project[] = [];
  let saving = false;
  let selectedAvailable = !options.desktop;
  let selectionRevision = 0;

  function showError(message: unknown) { error.textContent = String(message); error.hidden = false; }
  function render() {
    list.replaceChildren();
    const query = filter.value.trim().toLocaleLowerCase();
    const matches = projects.filter(project => `${project.name} ${project.path}`.toLocaleLowerCase().includes(query));
    for (const project of matches) {
      const row = document.createElement('div'); row.setAttribute('role', 'listitem');
      const button = document.createElement('button'); button.type = 'button'; button.className = 'recent-project';
      button.disabled = !project.available || saving;
      const name = document.createElement('strong'); name.textContent = project.name;
      const path = document.createElement('span'); path.textContent = project.path;
      button.append(name, path); button.title = project.path;
      if (!project.available) { const missing = document.createElement('small'); missing.textContent = 'Folder unavailable'; button.append(missing); }
      button.onclick = () => { void select(project.path); };
      row.append(button); list.append(row);
    }
    if (!matches.length) {
      const empty = document.createElement('p'); empty.className = 'hint';
      empty.textContent = projects.length ? 'No matching recent folders.' : 'Choose a folder below to start your project history.';
      list.append(empty);
    }
  }

  async function select(path: string) {
    if (saving) return;
    if (!/^(?:[A-Za-z]:[\\/]|\/|\\\\)/.test(path)) {
      folder.setCustomValidity('Enter an absolute folder path.'); folder.reportValidity(); return;
    }
    saving = true; submit.disabled = true; folder.disabled = true; filter.disabled = true;
    form.setAttribute('aria-busy', 'true');
    form.querySelectorAll<HTMLButtonElement>('[data-cancel]').forEach(button => { button.disabled = true; });
    error.hidden = true; render();
    try {
      if (!options.desktop) throw new Error('Open the desktop app to choose and remember project folders.');
      const project = await invoke<Project>('project_select', { path });
      selectionRevision++;
      selectedAvailable = true;
      options.onSelected(project.path); dialog.close();
    } catch (failure) { showError(failure); }
    finally {
      saving = false; submit.disabled = false; folder.disabled = false; filter.disabled = false;
      form.removeAttribute('aria-busy');
      form.querySelectorAll<HTMLButtonElement>('[data-cancel]').forEach(button => { button.disabled = false; });
      render();
    }
  }

  form.onsubmit = event => { event.preventDefault(); void select(folder.value.trim()); };
  folder.oninput = () => { folder.setCustomValidity(''); error.hidden = true; };
  filter.oninput = render;
  form.querySelectorAll<HTMLButtonElement>('[data-cancel]').forEach(button => { button.onclick = () => { if (!saving) dialog.close(); }; });
  dialog.addEventListener('cancel', event => { if (saving) event.preventDefault(); });

  return {
    canStart() { return selectedAvailable; },
    async restore() {
      if (!options.desktop) return;
      const revision = selectionRevision;
      try {
        const history = await invoke<Project[]>('project_recent');
        if (revision !== selectionRevision) return;
        projects = history;
        const latest = projects[0];
        selectedAvailable = !latest || latest.available;
        if (latest) {
          selectedAvailable = latest.available;
          options.onSelected(latest.path);
          if (!latest.available) options.reportError(`Last project unavailable: ${latest.path}. Choose a folder before opening a new terminal.`);
        }
      } catch (failure) {
        if (revision !== selectionRevision) return;
        selectedAvailable = false; options.reportError(`Project history unavailable: ${failure}`);
      }
    },
    open() {
      if (document.querySelector('dialog[open]')) return;
      folder.value = options.getPath(); folder.setCustomValidity(''); filter.value = ''; error.hidden = true;
      render(); dialog.showModal(); folder.focus();
      if (options.desktop) void invoke<Project[]>('project_recent').then(value => { projects = value; render(); }).catch(showError);
    },
  };
}
