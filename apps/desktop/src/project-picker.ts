import { invoke } from '@tauri-apps/api/core';
import { openModal, openSheet } from './modal.ts';
import { beginMutation, browseFolder, cancelMutation, createCatalogClient, createRequestId, orderedCandidates, saveRoots,
  type Candidate, type Catalog, type CatalogRoot, type Project, type RootConfig, type RootDraft } from './project-catalog.ts';

type Options = { desktop: boolean; getPath: () => string; onSelected: (path: string, chosen: boolean) => void; onCreate: () => void; reportError: (error: unknown, tone?: 'info' | 'error') => void };
const $ = <T extends HTMLElement = HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const editable = (target: EventTarget | null) => target instanceof HTMLElement && !!target.closest('input,textarea,select,[contenteditable="true"]');
const stateLabels = { ready: 'Ready', scanning: 'Scanning…', missing: 'Folder unavailable', unavailable: 'Cannot read this root', timed_out: 'Scan timed out; waiting for the drive', truncated: 'Folder limit reached' };
const sameFolder = (left: string, right: string) => left.replace(/[\\/]+$/, '').toLowerCase() === right.replace(/[\\/]+$/, '').toLowerCase();
const absolute = (path: string) => /^(?:[A-Za-z]:[\\/]|\/|\\\\)/.test(path);
type Mutation = { id: string; phase: 'pending' | 'cancelling' | 'committing' | 'cancelled'; settled: boolean; failure: string | null;
  registration: Promise<void> | null; reserved: boolean; dispatched: boolean; cancelRequested: boolean; cancellation: Promise<void> | null };

export function createProjectPicker(options: Options) {
  const dialog = $<HTMLDialogElement>('#project-dialog');
  const form = $<HTMLFormElement>('#project-form');
  const folder = $<HTMLInputElement>('#folder');
  const filter = $<HTMLInputElement>('#project-filter');
  const recentList = $('#recent-project-list');
  const projectList = $('#catalog-project-list');
  const error = $('#project-error');
  const rootList = $('#root-list');
  const rootPath = $<HTMLInputElement>('#root-path');
  let recent: Project[] = [];
  let catalog: Catalog | null = null;
  let config: RootConfig | null = null;
  let saving = false;
  let selectedAvailable = !options.desktop;
  let selectionRevision = 0;
  let dialogRevision = 0;
  let loading = false;
  let configLoading = true;
  // False from opening until the roots are read again: opening a folder elsewhere moves their revision.
  let configFresh = false;
  let queryPending = false;
  let pageOffsets: number[] = [];
  let mutation: Mutation | null = null;
  let recentRead: Promise<{ entries: Project[]; revision: number }> | null = null;
  const client = createCatalogClient(value => {
    catalog = value; configLoading = value.configLoading;
    // A scan that started before the last save carries the older roots; never step back to them.
    if (!configLoading && (!config || value.rootsConfig.revision >= config.revision)) config = value.rootsConfig;
    if (!configLoading) configFresh = true;
    loading = false; queryPending = false; render();
  }, failure => { loading = false; queryPending = false; if (dialog.open) { showError(failure); render(); } });

  function showError(message: unknown) { error.textContent = String(message); error.hidden = false; }
  function readRecent() {
    if (!recentRead) {
      const revision = selectionRevision;
      const pending = invoke<Project[]>('project_recent').then(entries => ({ entries, revision }))
        .finally(() => { if (recentRead === pending) recentRead = null; });
      recentRead = pending;
    }
    return recentRead;
  }
  async function refreshRecent(revision: number) {
    try {
      const result = await readRecent();
      if (revision !== dialogRevision || !dialog.open) return;
      if (result.revision !== selectionRevision) { void refreshRecent(revision); return; }
      recent = result.entries; render();
    } catch (failure) { if (revision === dialogRevision && dialog.open) showError(failure); }
  }
  function setBusy(value: boolean) {
    saving = value; form.setAttribute('aria-busy', String(value));
    for (const element of form.querySelectorAll<HTMLInputElement | HTMLButtonElement>('input,button')) element.disabled = value;
    for (const button of form.querySelectorAll<HTMLButtonElement>('[data-cancel]')) {
      button.disabled = mutation?.phase === 'cancelling';
      const completing = mutation?.phase === 'committing';
      if (button.hasAttribute('aria-label')) button.setAttribute('aria-label', completing ? 'Close dialog; save continues' : 'Close dialog');
      else button.textContent = completing ? 'Close; save continues' : 'Close';
    }
    if (!value) {
      $<HTMLButtonElement>('#catalog-previous').disabled = !catalog || catalog.offset <= 0;
      $<HTMLButtonElement>('#catalog-next').disabled = !catalog || catalog.offset + catalog.limit >= catalog.total;
    }
    render();
  }
  async function mutate<T>(work: (requestId: string) => Promise<T>, completed: (value: T) => void) {
    if (mutation) return;
    const current: Mutation = { id: createRequestId(), phase: 'pending', settled: false, failure: null,
      registration: null, reserved: false, dispatched: false, cancelRequested: false, cancellation: null };
    mutation = current; setBusy(true); error.hidden = true;
    try {
      if (!options.desktop) throw new Error('Open the desktop app to choose project folders.');
      current.registration = beginMutation(current.id);
      await current.registration; current.reserved = true;
      if (current.cancelRequested) { await current.cancellation; return; }
      current.dispatched = true;
      const value = await work(current.id);
      if (mutation === current && current.phase !== 'cancelled') completed(value);
    } catch (failure) {
      current.failure = String(failure);
      if (current.reserved && !current.cancelRequested) void cancelMutation(current.id).catch(() => {});
      if (mutation === current && current.phase !== 'cancelled' && current.phase !== 'cancelling') {
        if (dialog.open) showError(failure); else options.reportError(`Folder operation did not complete: ${failure}`);
      }
    }
    finally {
      current.settled = true;
      const unresolvedReservation = current.reserved && !current.dispatched && current.cancelRequested && current.phase !== 'cancelled';
      if (mutation === current && current.phase !== 'cancelling' && !unresolvedReservation) { mutation = null; setBusy(false); }
    }
  }
  async function cancelOrClose() {
    const current = mutation;
    if (!current || current.phase === 'cancelled') { dialog.close(); return; }
    if (current.phase === 'committing') {
      dialog.close(); options.reportError('The folder operation continues in the background.', 'info'); return;
    }
    if (current.phase === 'cancelling') return;
    current.cancelRequested = true; current.phase = 'cancelling'; setBusy(true);
    current.cancellation = (async () => {
      try {
        // The synchronous reservation must be acknowledged before cancel or save.
        await current.registration;
        if (mutation !== current) return;
        const acknowledgement = await cancelMutation(current.id);
        if (mutation !== current) return;
        if (acknowledgement.status === 'cancelled' || !current.dispatched) {
          current.phase = 'cancelled'; setBusy(true); dialog.close();
          if (current.settled) { mutation = null; setBusy(false); }
        } else if (current.settled) {
          mutation = null; setBusy(false); dialog.close();
          options.reportError(current.failure ? `Folder operation did not complete: ${current.failure}`
            : 'The folder operation finished before cancellation. Its completed result has been kept.', current.failure ? 'error' : 'info');
        } else { current.phase = 'committing'; setBusy(true); }
      } catch (failure) {
        if (mutation === current) {
          if (!current.reserved) {
            current.phase = 'cancelled'; dialog.close();
            if (current.settled) { mutation = null; setBusy(false); }
          } else if (current.settled && current.dispatched) { mutation = null; setBusy(false); }
          else { current.phase = 'pending'; setBusy(true); }
          if (current.reserved) showError(current.failure ?? failure);
        }
      }
    })();
    await current.cancellation;
  }
  function useProject(project: Project) {
    selectionRevision++; selectedAvailable = project.available;
    recent = [project, ...recent.filter(entry => entry.id !== project.id)].slice(0, 10);
    options.onSelected(project.path, true); dialog.close();
  }
  function projectButton(project: Project | Candidate, className: string) {
    const row = document.createElement('div'); row.setAttribute('role', 'listitem');
    const button = document.createElement('button'); button.type = 'button'; button.className = className;
    button.dataset.path = project.path;
    button.disabled = !project.available || saving; button.title = project.path;
    const name = document.createElement('strong'); name.textContent = project.name;
    const path = document.createElement('span'); path.textContent = project.path;
    button.append(name, path);
    if (!project.available) { const label = document.createElement('small'); label.textContent = 'Folder unavailable'; button.append(label); }
    button.onclick = () => { void select(project.path); };
    row.append(button); return row;
  }
  function render() {
    const focused = document.activeElement instanceof HTMLButtonElement && document.activeElement.closest('#catalog-project-list,#recent-project-list')
      ? document.activeElement.dataset.path : null;
    const query = filter.value.trim().toLocaleLowerCase();
    const inQuery = (project: Pick<Project, 'name' | 'path'>) => `${project.name} ${project.path}`.toLocaleLowerCase().includes(query);
    const history = config?.defaultRootId ? catalog?.history ?? [] : recent;
    $('#recent-project-heading').textContent = config?.defaultRootId ? 'Single folders outside your roots' : 'Recent folders';
    recentList.replaceChildren(...history.filter(inQuery).slice(0, 10).map(project => projectButton(project, 'recent-project')));
    // Nothing opened one folder at a time yet: the section stays out of the way.
    const noHistory = !recentList.childElementCount && !query;
    $('#recent-project-heading').hidden = noHistory; recentList.hidden = noHistory;
    if (!recentList.childElementCount && query) {
      const empty = document.createElement('p'); empty.className = 'hint'; empty.textContent = 'No matching folders.';
      recentList.append(empty);
    }
    projectList.replaceChildren();
    if (catalog && !queryPending) for (const project of orderedCandidates(catalog).filter(inQuery)) projectList.append(projectButton(project, 'catalog-project'));
    renderRoots();
    $('#catalog-state').textContent = mutation?.phase === 'committing' ? 'The save has started. Closing this window keeps it running.'
      : mutation?.phase === 'cancelled' ? 'The previous folder check is still returning. Existing terminals remain available.'
      : mutation?.phase === 'cancelling' ? 'Cancelling the folder operation…'
      : mutation?.phase === 'pending' ? 'Checking folder paths…'
      : loading || configLoading ? 'Reading project roots…' : queryPending ? 'Filtering projects…' : catalog?.scanning ? 'Scanning roots. You can keep filtering or close this window.' : catalog?.roots.length ? `${catalog.total} matching projects` : '';
    const paginated = !!catalog && (catalog.total > catalog.limit || catalog.offset > 0);
    $('#catalog-pagination').hidden = !paginated;
    if (catalog) {
      $('#catalog-page').textContent = `${Math.min(catalog.offset + 1, catalog.total)}–${Math.min(catalog.offset + catalog.limit, catalog.total)} of ${catalog.total}`;
      $<HTMLButtonElement>('#catalog-previous').disabled = saving || loading || queryPending || catalog.offset <= 0;
      $<HTMLButtonElement>('#catalog-next').disabled = saving || loading || queryPending || catalog.offset + catalog.limit >= catalog.total;
    }
    if (focused && dialog.open) {
      [...projectList.querySelectorAll<HTMLButtonElement>('button'), ...recentList.querySelectorAll<HTMLButtonElement>('button')]
        .find(button => button.dataset.path === focused && !button.disabled)?.focus({ preventScroll: true });
    }
  }
  async function select(path: string) {
    if (saving) return;
    if (!absolute(path)) {
      folder.setCustomValidity('Enter an absolute folder path.'); folder.reportValidity(); return;
    }
    await mutate(requestId => invoke<Project>('project_select', { path, requestId }), useProject);
  }
  function rootRow(root: { id: string; path: string }, index: number, scanned: CatalogRoot | undefined) {
    const isDefault = config?.defaultRootId === root.id;
    const row = document.createElement('div'); row.setAttribute('role', 'listitem');
    row.className = `catalog-root-status ${scanned?.state ?? 'scanning'}`; row.dataset.rootId = root.id;
    if (scanned?.message) row.title = scanned.message;
    const text = document.createElement('div'); text.className = 'root-text';
    const path = document.createElement('span'); path.textContent = root.path; path.title = root.path;
    const state = document.createElement('small');
    state.textContent = !scanned ? 'Scanning…' : scanned.state === 'ready' ? `${scanned.projectCount} ${scanned.projectCount === 1 ? 'project' : 'projects'}` : stateLabels[scanned.state];
    text.append(path, state);
    const star = document.createElement('button'); star.type = 'button'; star.className = 'root-default';
    star.textContent = isDefault ? 'Default' : 'Make default'; star.setAttribute('aria-pressed', String(isDefault));
    star.title = isDefault ? 'New projects are created in this root' : 'Create new projects in this root';
    star.setAttribute('aria-label', `${isDefault ? 'Default root' : 'Make default'}: ${root.path}`);
    star.disabled = saving || !configFresh || isDefault; star.onclick = () => { void writeRoots(config!.roots.map(entry => ({ ...entry })), index); };
    const remove = document.createElement('button'); remove.type = 'button'; remove.className = 'root-remove'; remove.textContent = 'Remove';
    remove.setAttribute('aria-label', `Remove root ${root.path}`); remove.title = 'Stop listing this root. The folder itself is not touched.';
    remove.disabled = saving || !configFresh; remove.onclick = () => { void removeRoot(index); };
    row.append(text, star, remove); return row;
  }
  function renderRoots() {
    const roots = config?.roots ?? [];
    rootList.replaceChildren(...roots.map((root, index) => rootRow(root, index, catalog?.roots.find(entry => entry.id === root.id))));
    if (!roots.length) {
      const empty = document.createElement('p'); empty.className = 'root-empty';
      empty.textContent = configLoading ? 'Reading project roots…' : 'No root yet. Choose the folder that holds your projects.';
      rootList.append(empty);
    }
    const defaultRoot = roots.find(root => root.id === config?.defaultRootId);
    $('#default-root-summary').textContent = defaultRoot ? `New projects are created in ${defaultRoot.path}.` : roots.length ? 'Make one root the default to create new projects in it.' : '';
    const blocked = saving || !config || !configFresh;
    $<HTMLButtonElement>('#choose-root').disabled = blocked || roots.length >= 16;
    // Choosing a root is the call to action only until there is one; then picking a project is.
    $('#choose-root').classList.toggle('primary', !roots.length);
    $<HTMLButtonElement>('#add-root').disabled = blocked || roots.length >= 16;
    form.classList.toggle('no-roots', !configLoading && !roots.length);
  }
  /** Root edits are saved at once: there is no draft to keep, discard or lose. */
  async function writeRoots(roots: RootDraft[], defaultRootIndex: number | null) {
    // The saved configuration is current even while the list is being rescanned.
    if (saving || !config || !configFresh) return false;
    const revision = config.revision; let saved = false;
    await mutate(requestId => saveRoots(revision, roots, defaultRootIndex, requestId), value => {
      config = value; configFresh = true; saved = true;
      if (!dialog.open) options.reportError('Project roots saved.', 'info');
      refresh();
    });
    return saved;
  }
  async function addRoot(path: string) {
    if (!config) return false;
    const trimmed = path.trim();
    if (!absolute(trimmed)) { showError('Enter the absolute path of the folder that holds your projects.'); return false; }
    const roots: RootDraft[] = config.roots.map(root => ({ ...root }));
    if (roots.some(root => sameFolder(root.path, trimmed))) { showError('That root is already in the list.'); return false; }
    if (roots.length >= 16) { showError('Keep at most 16 project roots.'); return false; }
    const current = roots.findIndex(root => root.id === config!.defaultRootId);
    // The first root, or a root added while none is the default, becomes the default.
    return writeRoots([...roots, { path: trimmed }], current >= 0 ? current : roots.length);
  }
  async function removeRoot(index: number) {
    if (!config) return;
    const roots: RootDraft[] = config.roots.map(root => ({ ...root })).filter((_, position) => position !== index);
    const kept = roots.findIndex(root => root.id === config!.defaultRootId);
    await writeRoots(roots, kept >= 0 ? kept : roots.length ? 0 : null);
  }
  async function chooseRoot() {
    if (saving || !config) return;
    let picked: string | null = null;
    await mutate(requestId => browseFolder('root', config?.defaultRootId ?? undefined, requestId), result => {
      if (result.kind === 'root') picked = result.path;
    });
    if (picked) await addRoot(picked);
  }
  async function browseProject() {
    if (saving) return;
    await mutate(requestId => browseFolder('project', config?.defaultRootId ?? undefined, requestId), result => {
      if (result.kind === 'project') useProject(result.project);
    });
  }
  function refresh() {
    if (!options.desktop || !dialog.open) return;
    pageOffsets = [];
    loading = true; configLoading = true; render(); client.open(filter.value);
    if (!config?.defaultRootId) void refreshRecent(dialogRevision);
  }
  form.onsubmit = event => { event.preventDefault(); void select(folder.value.trim()); };
  folder.oninput = () => { folder.setCustomValidity(''); error.hidden = true; };
  filter.oninput = () => { pageOffsets = []; queryPending = true; render(); client.query(filter.value, 0); };
  filter.onkeydown = event => {
    if (event.key === 'Enter') {
      event.preventDefault(); (projectList.querySelector<HTMLButtonElement>('button:not(:disabled)') || recentList.querySelector<HTMLButtonElement>('button:not(:disabled)'))?.click();
    }
  };
  const addTypedRoot = async () => { if (await addRoot(rootPath.value)) rootPath.value = ''; };
  rootPath.oninput = () => { error.hidden = true; };
  rootPath.onkeydown = event => { if (event.key === 'Enter') { event.preventDefault(); void addTypedRoot(); } };
  $('#add-root').onclick = () => { void addTypedRoot(); };
  $('#browse-project').onclick = () => { void browseProject(); };
  $('#create-project').onclick = options.onCreate;
  $('#choose-root').onclick = () => { void chooseRoot(); };
  $('#refresh-projects').onclick = refresh;
  $('#catalog-previous').onclick = () => {
    if (catalog && catalog.offset > 0 && !queryPending && !loading && !saving) {
      const offset = pageOffsets.pop() ?? Math.max(0, catalog.offset - catalog.limit);
      queryPending = true; render(); client.query(filter.value, offset);
    }
  };
  $('#catalog-next').onclick = () => {
    if (catalog && catalog.offset + catalog.limit < catalog.total && !queryPending && !loading && !saving) {
      pageOffsets.push(catalog.offset); const offset = catalog.offset + catalog.limit;
      queryPending = true; render(); client.query(filter.value, offset);
    }
  };
  form.querySelectorAll<HTMLButtonElement>('[data-cancel]').forEach(button => { button.onclick = () => { void cancelOrClose(); }; });
  dialog.addEventListener('cancel', event => { if (mutation) { event.preventDefault(); void cancelOrClose(); } });
  dialog.addEventListener('close', () => { dialogRevision++; client.close(); });

  return {
    canStart() { return selectedAvailable; },
    acceptCreated(project: Project) { useProject(project); },
    prepareForCreation() {
      if (mutation || saving) {
        options.reportError('The current folder operation is still returning. Finish or cancel it before creating a project.'); return false;
      }
      if (dialog.open) dialog.close();
      return true;
    },
    async restore() {
      if (!options.desktop) return;
      const revision = selectionRevision;
      try {
        const result = await readRecent(); const history = result.entries;
        if (revision !== selectionRevision) return;
        recent = history; const latest = history[0]; selectedAvailable = !latest || latest.available;
        if (latest) {
          options.onSelected(latest.path, false);
          if (!latest.available) options.reportError(`Last project unavailable: ${latest.path}. Choose a folder before opening a new terminal.`);
        }
      } catch (failure) {
        if (revision !== selectionRevision) return;
        selectedAvailable = false; options.reportError(`Project history unavailable: ${failure}`);
      }
    },
    open(focusFilter = false) {
      if (dialog.open || document.querySelector('dialog[open][aria-modal="true"]')) return;
      dialogRevision++; configFresh = false;
      folder.value = options.getPath(); folder.setCustomValidity(''); filter.value = ''; rootPath.value = ''; error.hidden = true;
      render();
      // Hung from the project button, with the workspace still usable. A running folder
      // operation is never dropped by a stray press outside.
      if (!openSheet(dialog, { anchor: () => document.querySelector('#project'), placement: 'below', lightDismiss: () => !mutation })) openModal(dialog);
      (focusFilter || config?.roots.length ? filter : rootPath).focus(); refresh();
    },
    browse: browseProject,
    handleKey(event: KeyboardEvent) {
      if (!dialog.open || dialog.inert || event.ctrlKey || event.metaKey || event.altKey || saving) return false;
      if (event.key.toLowerCase() === 'n' && !event.repeat && !editable(event.target)) { event.preventDefault(); options.onCreate(); return true; }
      if (event.key === '/' && !editable(event.target)) { event.preventDefault(); filter.focus(); return true; }
      if (event.key === 'F3') { event.preventDefault(); void browseProject(); return true; }
      return false;
    },
  };
}
