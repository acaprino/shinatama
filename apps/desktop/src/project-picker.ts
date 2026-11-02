import { invoke } from '@tauri-apps/api/core';
import { beginMutation, browseFolder, cancelMutation, createCatalogClient, createRequestId, orderedCandidates, saveRoots,
  type Candidate, type Catalog, type Project, type RootConfig, type RootDraft } from './project-catalog.ts';

type Options = { desktop: boolean; getPath: () => string; onSelected: (path: string) => void; reportError: (error: unknown) => void };
const $ = <T extends HTMLElement = HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const editable = (target: EventTarget | null) => target instanceof HTMLElement && !!target.closest('input,textarea,select,[contenteditable="true"]');
const stateLabels = { ready: 'Ready', scanning: 'Scanning…', missing: 'Folder unavailable', unavailable: 'Cannot read this root', timed_out: 'Scan timed out; waiting for the drive', truncated: 'Folder limit reached' };
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
  const editor = $('#roots-editor');
  const draftsElement = $('#root-drafts');
  const submit = form.querySelector<HTMLButtonElement>('button[type="submit"]')!;
  let recent: Project[] = [];
  let catalog: Catalog | null = null;
  let config: RootConfig | null = null;
  let drafts: RootDraft[] = [];
  let defaultIndex: number | null = null;
  let draftRevision = 0;
  let saving = false;
  let selectedAvailable = !options.desktop;
  let selectionRevision = 0;
  let dialogRevision = 0;
  let loading = false;
  let configLoading = true;
  let queryPending = false;
  let pageOffsets: number[] = [];
  let mutation: Mutation | null = null;
  let recentRead: Promise<{ entries: Project[]; revision: number }> | null = null;
  const client = createCatalogClient(value => {
    catalog = value; configLoading = value.configLoading;
    if (!configLoading) config = value.rootsConfig;
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
      else button.textContent = completing ? 'Close; save continues' : 'Cancel';
    }
    if (!value) {
      $('#add-root').toggleAttribute('disabled', drafts.length >= 16);
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
      dialog.close(); options.reportError('The folder operation continues in the background.'); return;
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
            : 'The folder operation finished before cancellation. Its completed result has been kept.');
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
    options.onSelected(project.path); dialog.close();
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
    $('#recent-project-heading').textContent = config?.defaultRootId ? 'Outside folders · last ten' : 'Recent folders';
    recentList.replaceChildren(...history.filter(inQuery).slice(0, 10).map(project => projectButton(project, 'recent-project')));
    if (!recentList.childElementCount) {
      const empty = document.createElement('p'); empty.className = 'hint';
      empty.textContent = query ? 'No matching recent folders.' : config?.defaultRootId ? 'Folders opened with Browse outside your roots appear here.' : 'Open a folder or choose a project root.';
      recentList.append(empty);
    }
    projectList.replaceChildren();
    if (catalog && !queryPending) for (const project of orderedCandidates(catalog).filter(inQuery)) projectList.append(projectButton(project, 'catalog-project'));
    const statusList = $('#catalog-root-statuses'); statusList.replaceChildren();
    for (const root of catalog?.roots ?? []) {
      const row = document.createElement('div'); row.className = `catalog-root-status ${root.state}`; row.dataset.rootId = root.id;
      const path = document.createElement('span'); path.textContent = root.path; path.title = root.path;
      const state = document.createElement('small'); state.textContent = `${config?.defaultRootId === root.id ? 'Default · ' : ''}${stateLabels[root.state]}`;
      row.append(path, state); if (root.message) row.title = root.message; statusList.append(row);
    }
    const defaultRootId = config?.defaultRootId;
    const defaultRoot = config?.roots.find(root => root.id === defaultRootId);
    $('#default-root-summary').textContent = defaultRoot ? `Default for new projects: ${defaultRoot.path}` : 'No default root yet. Choose Project roots to set one.';
    $<HTMLButtonElement>('#manage-roots').disabled = saving || configLoading || !config;
    $<HTMLButtonElement>('#save-roots').disabled = saving || configLoading || !config;
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
    if (!/^(?:[A-Za-z]:[\\/]|\/|\\\\)/.test(path)) {
      folder.setCustomValidity('Enter an absolute folder path.'); folder.reportValidity(); return;
    }
    await mutate(requestId => invoke<Project>('project_select', { path, requestId }), useProject);
  }
  function renderDrafts() {
    draftsElement.replaceChildren();
    drafts.forEach((draft, index) => {
      const row = document.createElement('div'); row.className = 'root-draft';
      const radioLabel = document.createElement('label'); radioLabel.className = 'root-default-label';
      const radio = document.createElement('input'); radio.type = 'radio'; radio.name = 'default-root'; radio.checked = defaultIndex === index;
      radio.setAttribute('aria-label', `Use root ${index + 1} as default`); radio.onchange = () => { defaultIndex = index; };
      radioLabel.append(radio, document.createTextNode('Default'));
      const path = document.createElement('input'); path.value = draft.path; path.placeholder = 'Absolute root folder';
      path.setAttribute('aria-label', `Project root ${index + 1}`); path.oninput = () => { draft.path = path.value; };
      const browse = document.createElement('button'); browse.type = 'button'; browse.textContent = 'Browse'; browse.setAttribute('aria-label', `Browse for root ${index + 1}`);
      browse.onclick = () => { void chooseRoot(index); };
      const remove = document.createElement('button'); remove.type = 'button'; remove.textContent = 'Remove'; remove.setAttribute('aria-label', `Remove root ${index + 1}`);
      remove.onclick = () => {
        drafts.splice(index, 1);
        if (defaultIndex === index) defaultIndex = null; else if (defaultIndex !== null && defaultIndex > index) defaultIndex--;
        renderDrafts();
      };
      row.append(radioLabel, path, browse, remove); draftsElement.append(row);
    });
    $<HTMLButtonElement>('#add-root').disabled = saving || drafts.length >= 16;
    if (!drafts.length) { const empty = document.createElement('p'); empty.className = 'hint'; empty.textContent = 'Add a root and select its default radio button. Nothing is created in that folder.'; draftsElement.append(empty); }
  }
  function editRoots() {
    if (saving || configLoading || !config) return;
    drafts = config?.roots.map(root => ({ ...root })) ?? [];
    defaultIndex = config?.defaultRootId ? drafts.findIndex(root => root.id === config!.defaultRootId) : null;
    if (defaultIndex === -1) defaultIndex = null;
    draftRevision = config?.revision ?? 0; renderDrafts(); editor.hidden = false;
    draftsElement.querySelector<HTMLInputElement>('input[type="text"],input:not([type])')?.focus();
  }
  async function chooseRoot(index?: number) {
    if (saving) return;
    await mutate(requestId => browseFolder('root', index === undefined ? undefined : drafts[index]?.id, requestId), result => {
      if (result.kind === 'root') {
        if (index === undefined) { if (drafts.length >= 16) throw new Error('Keep at most 16 project roots.'); drafts.push({ path: result.path }); }
        else drafts[index].path = result.path;
        editor.hidden = false; renderDrafts();
      }
    });
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
    loading = true; configLoading = true; error.hidden = true; render(); client.open(filter.value);
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
  editor.addEventListener('keydown', event => { if (event.key === 'Enter' && editable(event.target)) event.preventDefault(); });
  $('#browse-project').onclick = () => { void browseProject(); };
  $('#manage-roots').onclick = editRoots;
  $('#discard-roots').onclick = () => { if (!saving) { editor.hidden = true; error.hidden = true; } };
  $('#add-root').onclick = () => { if (!saving && drafts.length < 16) { drafts.push({ path: '' }); renderDrafts(); draftsElement.querySelector<HTMLElement>('.root-draft:last-child input:not([type="radio"])')?.focus(); } };
  $('#choose-root').onclick = () => { void chooseRoot(); };
  $('#save-roots').onclick = async () => {
    if (saving || configLoading || !config) return;
    if (drafts.length && defaultIndex === null) { showError('Select the default root before saving.'); return; }
    if (drafts.some(draft => !draft.path.trim())) { showError('Enter an absolute folder path for each root.'); return; }
    await mutate(requestId => saveRoots(draftRevision, drafts.map(draft => ({ ...draft, path: draft.path.trim() })), defaultIndex, requestId), saved => {
      config = saved;
      if (!dialog.open) options.reportError('Project roots saved.');
      editor.hidden = true; refresh();
    });
  };
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
  dialog.addEventListener('close', () => { dialogRevision++; client.close(); editor.hidden = true; });

  return {
    canStart() { return selectedAvailable; },
    async restore() {
      if (!options.desktop) return;
      const revision = selectionRevision;
      try {
        const result = await readRecent(); const history = result.entries;
        if (revision !== selectionRevision) return;
        recent = history; const latest = history[0]; selectedAvailable = !latest || latest.available;
        if (latest) {
          options.onSelected(latest.path);
          if (!latest.available) options.reportError(`Last project unavailable: ${latest.path}. Choose a folder before opening a new terminal.`);
        }
      } catch (failure) {
        if (revision !== selectionRevision) return;
        selectedAvailable = false; options.reportError(`Project history unavailable: ${failure}`);
      }
    },
    open(focusFilter = false) {
      if (document.querySelector('dialog[open]')) return;
      dialogRevision++;
      folder.value = options.getPath(); folder.setCustomValidity(''); filter.value = ''; error.hidden = true; editor.hidden = true;
      render(); dialog.showModal(); (focusFilter ? filter : folder).focus(); refresh();
    },
    browse: browseProject,
    handleKey(event: KeyboardEvent) {
      if (!dialog.open || event.ctrlKey || event.metaKey || event.altKey || saving) return false;
      if (event.key === '/' && !editable(event.target)) { event.preventDefault(); filter.focus(); return true; }
      if (event.key === 'F3' && editor.hidden) { event.preventDefault(); void browseProject(); return true; }
      return false;
    },
  };
}
