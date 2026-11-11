import './thread-sidebar.css';

export type CurrentThreadProject = { projectId: string; label?: string };
export type ThreadCursor = { updatedAt: number; threadId: string };
export type ThreadSummary = {
  id: string; projectId: string; kind: 'work' | 'roundtable' | 'companion'; title: string | null;
  revision: number; updatedAt: number; harness: string | null; nativeAvailable: boolean | null;
  status: 'unbound' | 'inactive' | 'recorded_open' | 'archived';
};
export type ThreadListPage = { rows: ThreadSummary[]; next: ThreadCursor | null };
export type ThreadListRequest = { projectId: string; cursor: ThreadCursor | null; limit: 50 };
export type ThreadRenameRequest = { projectId: string; threadId: string; expectedRevision: number; title: string };
export type ThreadSidebarState = { version: 1; width: number; collapsed: boolean };
export type ThreadSidebarOptions = {
  host: HTMLElement;
  list: (request: ThreadListRequest) => Promise<unknown>;
  create?: (request: {projectId:string}) => Promise<unknown>;
  rename?: (request: ThreadRenameRequest) => Promise<unknown>;
  /** Request the shared New session dialog for this explicit project. */
  onNewSession?: (projectId: string) => void;
  loadState?: () => Promise<unknown>;
  saveState?: (state: Readonly<ThreadSidebarState>) => Promise<void>;
  /** Select already saved metadata only; this sidebar has no launch authority. */
  onSelect?: (thread: Readonly<ThreadSummary>) => void;
  /** Explicit Resume button only; selection and display restoration never call it. */
  onResume?: (thread: Readonly<ThreadSummary>) => Promise<void>;
  /** Explicit recovery closes only a proven stopped owner; it never starts an agent. */
  onRecover?: (thread: Readonly<ThreadSummary>) => Promise<void>;
  canRecover?: (thread: Readonly<ThreadSummary>) => boolean;
  /** Capability supplied by the backend harness catalog, never inferred from a name. */
  canResume?: (thread: Readonly<ThreadSummary>) => boolean;
  reportError: (message: string) => void;
};
const MIN_WIDTH = 180; const MAX_WIDTH = 480; const WIDTH_STEP = 16;
const initialState: ThreadSidebarState = { version: 1, width: 264, collapsed: false };
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const safeText = (value: unknown, cap: number): value is string => typeof value === 'string' && value.length <= cap && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
const threadId = (value: unknown): value is string => typeof value === 'string' && /^t_[a-z2-7]{12}$/.test(value);
const projectId = (value: unknown): value is string => typeof value === 'string' && /^p_[a-z2-7]{12}$/.test(value);
const utf8 = new TextEncoder();
function cursorValue(value: unknown): ThreadCursor | null {
  return record(value) && integer(value.updatedAt) && threadId(value.threadId) ? { updatedAt: value.updatedAt, threadId: value.threadId } : null;
}
function stateValue(value: unknown): ThreadSidebarState | null {
  return record(value) && value.version === 1 && integer(value.width) && value.width >= MIN_WIDTH && value.width <= MAX_WIDTH
    && typeof value.collapsed === 'boolean' ? { version: 1, width: value.width, collapsed: value.collapsed } : null;
}
function pageValue(value: unknown, scope: string, cursor: ThreadCursor | null): ThreadListPage | null {
  if (!record(value) || !Array.isArray(value.rows) || value.rows.length > 50) return null;
  const rows: ThreadSummary[] = []; const seen = new Set<string>();
  for (const row of value.rows) {
    if (!record(row) || !threadId(row.id) || seen.has(row.id) || row.projectId !== scope
      || !['work', 'roundtable', 'companion'].includes(String(row.kind))
      || (row.title !== null && (!safeText(row.title, 256) || utf8.encode(row.title).length > 256))
      || !integer(row.revision) || row.revision === 0 || !integer(row.updatedAt)
      || (row.harness !== null && (!safeText(row.harness, 64) || !/^[a-z0-9_]+$/.test(row.harness)))
      || (row.nativeAvailable !== null && typeof row.nativeAvailable !== 'boolean')
      || !['unbound', 'inactive', 'recorded_open', 'archived'].includes(String(row.status))) return null;
    const previous = rows.at(-1);
    if (previous && (row.updatedAt > previous.updatedAt || (row.updatedAt === previous.updatedAt && row.id <= previous.id))) return null;
    if (cursor && (row.updatedAt > cursor.updatedAt || (row.updatedAt === cursor.updatedAt && row.id <= cursor.threadId))) return null;
    seen.add(row.id);
    rows.push({ id: row.id, projectId: scope, kind: row.kind as ThreadSummary['kind'], title: row.title as string | null,
      revision: row.revision, updatedAt: row.updatedAt, harness: row.harness as string | null,
      nativeAvailable: row.nativeAvailable as boolean | null, status: row.status as ThreadSummary['status'] });
  }
  const next = value.next === null ? null : cursorValue(value.next);
  if (value.next !== null && (!next || !rows.length || next.threadId !== rows.at(-1)!.id || next.updatedAt !== rows.at(-1)!.updatedAt)) return null;
  return { rows, next };
}
const sourceText = (value: boolean | null) => value === null ? 'Unknown' : value ? 'Available' : 'Unavailable';
function updatedText(timestamp: number) {
  const date = new Date(timestamp); return Number.isNaN(date.valueOf()) ? 'Unknown' : date.toLocaleString();
}
function failureText(error: unknown) {
  const code = typeof error === 'string' ? error : record(error) ? error.code ?? error.issue : null;
  if (code === 'project_missing') return 'The current project is unavailable. Choose it again.';
  if (code === 'busy') return 'Thread storage is busy. Reload explicitly.';
  if (code === 'source_changed' || code === 'conflict') return 'Thread records changed. Reload this project explicitly.';
  return 'Thread records could not be loaded. Reload explicitly.';
}
function resumeFailureText(error: unknown) {
  const code = typeof error === 'string' ? error : record(error) ? error.code ?? error.issue : null;
  if (code === 'unbound' || code === 'native_missing') return 'No saved conversation yet';
  if (code === 'owned_elsewhere') return 'This conversation is already open.';
  if (code === 'source_changed' || code === 'conflict') return 'This conversation changed. Reload threads and try again.';
  return 'The conversation could not be resumed. Try again.';
}
function recoveryFailureText(error: unknown) {
  const code = typeof error === 'string' ? error : record(error) ? error.code ?? error.issue : null;
  if (code === 'session_alive') return 'This session is still open. Stop it in its terminal before resuming.';
  if (code === 'owner_alive') return 'The app instance that owns this session is still running. Close that instance before recovering it.';
  if (code === 'cleanup_unproven') return 'This session could not be recovered because its shutdown was not confirmed. Its saved conversation is preserved.';
  if (code === 'source_changed' || code === 'conflict') return 'This thread changed. Reload threads before recovering it.';
  if (code === 'busy') return 'Another thread operation is in progress. Try again when it finishes.';
  return 'The previous session could not be verified. Its saved conversation is preserved.';
}
function validTitle(value: string): boolean {
  return value.trim().length > 0 && utf8.encode(value).length <= 256 && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
}
function renamedValue(value: unknown, before: ThreadSummary, title: string): ThreadSummary | null {
  if (!record(value) || value.id !== before.id || value.projectId !== before.projectId || value.kind !== before.kind
    || value.title !== title || value.revision !== before.revision + 1 || !integer(value.revision)
    || !integer(value.updatedAt) || value.updatedAt < before.updatedAt || value.archived !== (before.status === 'archived')) return null;
  return { ...before, title, revision: value.revision, updatedAt: value.updatedAt };
}

/** One explicit operation, one latest pending UI-state save, no launch replay. */
export function createThreadSidebar(options: ThreadSidebarOptions) {
  const element = document.createElement('aside'); element.className = 'thread-sidebar'; element.setAttribute('aria-label', 'Project threads');
  const heading = document.createElement('header'); heading.className = 'thread-sidebar-heading';
  const title = document.createElement('h2'); title.textContent = 'Threads';
  const collapse = document.createElement('button'); collapse.type = 'button'; collapse.className = 'thread-sidebar-toggle';
  collapse.setAttribute('aria-controls', 'thread-sidebar-body'); heading.append(title, collapse);
  const body = document.createElement('div'); body.id = 'thread-sidebar-body'; body.className = 'thread-sidebar-body';
  const scopeLabel = document.createElement('label'); scopeLabel.className = 'thread-sidebar-scope'; scopeLabel.htmlFor = 'thread-sidebar-project'; scopeLabel.textContent = 'Project';
  const scopeSelect = document.createElement('select'); scopeSelect.id = 'thread-sidebar-project'; scopeSelect.className = 'thread-sidebar-scope';
  const reload = document.createElement('button'); reload.type = 'button'; reload.className = 'thread-sidebar-reload'; reload.textContent = 'Reload threads';
  const create = document.createElement('button'); create.type = 'button'; create.className = 'thread-sidebar-create'; create.textContent = options.onNewSession ? 'New session' : 'New thread'; create.hidden = !options.onNewSession && !options.create;
  const status = document.createElement('p'); status.className = 'thread-sidebar-status'; status.setAttribute('role', 'status');
  const list = document.createElement('ul'); list.className = 'thread-sidebar-list'; list.setAttribute('aria-label', 'Saved threads');
  const pagination = document.createElement('div'); pagination.className = 'thread-sidebar-pagination';
  const previous = document.createElement('button'); previous.type = 'button'; previous.textContent = 'Previous';
  const pageLabel = document.createElement('span'); const next = document.createElement('button'); next.type = 'button'; next.textContent = 'Next';
  pagination.append(previous, pageLabel, next);
  const detail = document.createElement('dl'); detail.className = 'thread-sidebar-detail'; detail.setAttribute('aria-label', 'Selected thread metadata');
  const limitation = document.createElement('p'); limitation.className = 'thread-sidebar-limitation';
  limitation.textContent = 'Native session activity is unknown. Selecting a thread does not start an agent.';
  const actions = document.createElement('div'); actions.className = 'thread-sidebar-actions';
  const resume = document.createElement('button'); resume.type = 'button'; resume.textContent = 'Resume'; resume.hidden = true; actions.append(resume);
  const recover = document.createElement('button'); recover.type = 'button'; recover.textContent = 'Recover session'; recover.hidden = true; actions.append(recover);
  const rename = document.createElement('button'); rename.type = 'button'; rename.textContent = 'Rename'; rename.hidden = true; actions.append(rename);
  const renameForm = document.createElement('form'); renameForm.className = 'thread-sidebar-rename'; renameForm.hidden = true;
  const renameLabel = document.createElement('label'); renameLabel.htmlFor = 'thread-rename-title'; renameLabel.textContent = 'Thread name';
  const renameInput = document.createElement('input'); renameInput.id = 'thread-rename-title'; renameInput.type = 'text'; renameInput.maxLength = 256; renameInput.autocomplete = 'off';
  const renameError = document.createElement('p'); renameError.id = 'thread-rename-error'; renameError.setAttribute('role', 'alert'); renameError.hidden = true;
  renameInput.setAttribute('aria-describedby', renameError.id);
  const renameButtons = document.createElement('div');
  const renameSave = document.createElement('button'); renameSave.type = 'submit'; renameSave.textContent = 'Save';
  const renameCancel = document.createElement('button'); renameCancel.type = 'button'; renameCancel.textContent = 'Cancel';
  renameButtons.append(renameSave, renameCancel); renameForm.append(renameLabel, renameInput, renameError, renameButtons);
  for (const text of ['Hop unavailable']) { const button = document.createElement('button'); button.type = 'button'; button.disabled = true; button.textContent = text; actions.append(button); }
  body.append(scopeLabel, scopeSelect, create, reload, status, list, pagination, detail, limitation, actions, renameForm);
  const separator = document.createElement('div'); separator.className = 'thread-sidebar-resize'; separator.tabIndex = 0;
  separator.setAttribute('role', 'separator'); separator.setAttribute('aria-label', 'Thread sidebar width'); separator.setAttribute('aria-orientation', 'vertical');
  separator.setAttribute('aria-valuemin', String(MIN_WIDTH)); separator.setAttribute('aria-valuemax', String(MAX_WIDTH));
  element.append(heading, body, separator); options.host.append(element);
  let display = { ...initialState }; let projects: CurrentThreadProject[] = []; let selectedProject: string | null = null;
  let page: ThreadListPage | null = null; let cursor: ThreadCursor | null = null; let history: (ThreadCursor | null)[] = [];
  let selected: string | null = null; let generation = 0; let uiGeneration = 0; let destroyed = false;
  let busy: 'load_state' | 'list' | 'save_state' | 'create' | 'resume' | 'recover' | 'rename' | null = null; let message = 'Choose a project, then load its saved threads.'; let failed = false;
  let editing: ThreadSummary | null = null; let renameNeedsReload = false;
  let pendingState: ThreadSidebarState | null = null; let saveTimer = 0;
  let pendingProjectRead = false;
  let pendingChangedProject: string | null = null; let changeTimer = 0;
  let drag: { pointer: number; x: number; before: ThreadSidebarState } | null = null;
  const anchors = new Map<string, HTMLLIElement>();
  const renderedRows = new WeakMap<HTMLLIElement, ThreadSummary>();
  function metadata(label: string, value: string) {
    const term = document.createElement('dt'); term.textContent = label; const valueNode = document.createElement('dd'); valueNode.textContent = value; detail.append(term, valueNode);
  }
  function render() {
    if (destroyed) return;
    element.classList.toggle('is-collapsed', display.collapsed); element.style.setProperty('--thread-sidebar-width', `${display.width}px`);
    body.hidden = display.collapsed; separator.hidden = display.collapsed; separator.setAttribute('aria-valuenow', String(display.width));
    collapse.textContent = display.collapsed ? '›' : '‹'; collapse.setAttribute('aria-label', display.collapsed ? 'Expand threads' : 'Collapse threads');
    collapse.setAttribute('aria-expanded', String(!display.collapsed));
    reload.disabled = busy !== null || selectedProject === null; scopeSelect.disabled = !projects.length || busy !== null;
    create.disabled = busy !== null || selectedProject === null;
    previous.disabled = busy !== null || !history.length; next.disabled = busy !== null || !page?.next;
    body.setAttribute('aria-busy', String(busy !== null));
    status.textContent = busy === 'list' ? 'Reading saved threads…' : busy === 'create' ? 'Saving a new thread…' : busy === 'resume' ? 'Resuming conversation…' : busy === 'recover' ? 'Recovering stopped session…' : busy === 'rename' ? 'Saving thread name…' : busy === 'load_state' ? 'Restoring sidebar display…' : message;
    status.dataset.state = failed ? 'error' : 'ready';
    const retained = new Set(page?.rows.map(row => row.id) ?? []);
    for (const [id, node] of anchors) if (!retained.has(id)) { node.remove(); anchors.delete(id); }
    let position = 0;
    for (const row of page?.rows ?? []) {
      let node = anchors.get(row.id);
      if (!node) {
        node = document.createElement('li'); node.dataset.threadId = row.id;
        const anchor = document.createElement('button'); anchor.type = 'button'; anchor.dataset.threadId = row.id; anchor.className = 'thread-sidebar-anchor';
        const label = document.createElement('span'); label.className = 'thread-sidebar-title';
        const id = document.createElement('span'); id.className = 'thread-sidebar-id';
        const agent = document.createElement('span'); agent.className = 'thread-sidebar-agent';
        const updated = document.createElement('span'); updated.className = 'thread-sidebar-updated';
        const source = document.createElement('span'); source.className = 'thread-sidebar-source';
        anchor.append(label, id, agent, updated, source); node.append(anchor); anchors.set(row.id, node);
        anchor.onclick = () => {
          if (busy) return;
          const actual = page?.rows.find(thread => thread.id === row.id); if (!actual || actual.projectId !== selectedProject) return;
          editing = null;
          selected = actual.id; render();
          try { options.onSelect?.(Object.freeze({ ...actual })); } catch { options.reportError('Thread selection could not be shown. No agent was started.'); }
          scheduleThreadRead();
        };
      }
      const anchor = node.firstElementChild as HTMLButtonElement;
      anchor.disabled = busy !== null;
      const pressed = String(selected === row.id);
      if (anchor.getAttribute('aria-pressed') !== pressed) anchor.setAttribute('aria-pressed', pressed);
      if (renderedRows.get(node) !== row) {
        anchor.setAttribute('aria-label', `${row.title || 'Untitled thread'} · ${row.id}`);
        anchor.querySelector('.thread-sidebar-title')!.textContent = row.title || 'Untitled thread';
        anchor.querySelector('.thread-sidebar-id')!.textContent = row.id;
        const agentLine = anchor.querySelector<HTMLElement>('.thread-sidebar-agent')!; agentLine.textContent = row.harness ?? 'Agent unknown'; agentLine.hidden = !row.harness;
        anchor.querySelector('.thread-sidebar-updated')!.textContent = updatedText(row.updatedAt);
        const sourceLine = anchor.querySelector<HTMLElement>('.thread-sidebar-source')!; sourceLine.textContent = `Source: ${sourceText(row.nativeAvailable)}`; sourceLine.hidden = /unknown/i.test(sourceText(row.nativeAvailable));
        renderedRows.set(node, row);
      }
      // Preserve actual DOM/focus when only width or selection changes.
      if (list.children[position] !== node) list.insertBefore(node, list.children[position] ?? null);
      position++;
    }
    pageLabel.textContent = page ? `${page.rows.length} on this page` : 'No page loaded';
    detail.replaceChildren(); const chosen = page?.rows.find(row => row.id === selected);
    resume.hidden = !options.onResume || !chosen || chosen.status !== 'inactive' || options.canResume?.(chosen) !== true || chosen.nativeAvailable !== true;
    resume.disabled = busy !== null || resume.hidden;
    recover.hidden = !options.onRecover || !chosen || chosen.status !== 'recorded_open' || options.canRecover?.(chosen) !== true;
    recover.disabled = busy !== null || recover.hidden;
    rename.hidden = !options.rename || !chosen || editing !== null;
    rename.disabled = busy !== null || renameNeedsReload;
    renameForm.hidden = editing === null;
    renameInput.disabled = busy !== null || renameNeedsReload;
    renameSave.disabled = busy !== null || renameNeedsReload || !validTitle(renameInput.value);
    renameCancel.disabled = busy !== null;
    if (chosen) {
      metadata('Thread ID', chosen.id); metadata('Agent', chosen.harness ?? 'Unknown'); metadata('Title', chosen.title || 'Unknown');
      metadata('Updated', updatedText(chosen.updatedAt)); metadata('Source', sourceText(chosen.nativeAvailable));
      metadata('Native activity', 'Unknown');
      metadata('Saved status', { unbound: 'No native session linked', inactive: 'Recorded inactive', recorded_open: 'Recorded open; activity unknown', archived: 'Archived' }[chosen.status]);
    }
  }
  function reset() { page = null; cursor = null; history = []; selected = null; editing = null; renameNeedsReload = false; failed = false; pendingChangedProject = null; window.clearTimeout(changeTimer); changeTimer = 0; }
  function afterOperation() {
    busy = null; render();
    if (!destroyed && pendingProjectRead && selectedProject) {
      pendingProjectRead = false; void read(null, 'reload'); return;
    }
    if (!destroyed && pendingState) scheduleSave();
    scheduleThreadRead();
  }
  // One pending refresh per selected project, driven only by committed backend
  // notifications. Editing and in-flight operations keep their current state;
  // after they finish, a single bounded read consumes the pending change.
  function scheduleThreadRead() {
    if (destroyed || busy || editing || renameNeedsReload || changeTimer || !selectedProject || pendingChangedProject !== selectedProject) return;
    changeTimer = window.setTimeout(() => {
      changeTimer = 0;
      if (destroyed || busy || editing || renameNeedsReload || !selectedProject || pendingChangedProject !== selectedProject) return;
      pendingChangedProject = null;
      void read(null, 'reload', true);
    }, 100);
  }
  function notifyThreadChanged(value: unknown) {
    if (destroyed || !record(value) || !projectId(value.projectId) || !threadId(value.threadId) || value.projectId !== selectedProject) return;
    pendingChangedProject = value.projectId;
    scheduleThreadRead();
  }
  function scheduleSave() {
    if (destroyed || !options.saveState || saveTimer) return;
    saveTimer = window.setTimeout(() => { saveTimer = 0; void saveDisplay(); }, 180);
  }
  function changedDisplay() {
    uiGeneration++; render();
    if (options.saveState) { pendingState = { ...display }; scheduleSave(); }
  }
  async function saveDisplay() {
    if (destroyed || busy || !pendingState || !options.saveState) return;
    const value = pendingState; pendingState = null; busy = 'save_state'; render();
    try { await options.saveState(Object.freeze(value)); }
    catch { if (!destroyed) options.reportError('Sidebar display could not be saved. The current display is preserved.'); }
    finally { afterOperation(); }
  }
  async function read(requested: ThreadCursor | null, direction: 'reload' | 'previous' | 'next', preserveSelection = false) {
    if (destroyed || busy || !selectedProject) return false;
    const project = selectedProject; const at = generation; const previousSelection = selected;
    pendingChangedProject = null; window.clearTimeout(changeTimer); changeTimer = 0;
    editing = null; busy = 'list'; failed = false; render();
    try {
      const result = pageValue(await options.list({ projectId: project, cursor: requested, limit: 50 }), project, requested);
      if (!result) throw new Error('Invalid bounded thread projection');
      if (destroyed || at !== generation || selectedProject !== project) return false;
      if (direction === 'reload') history = [];
      else if (direction === 'next') { history.push(cursor); if (history.length > 64) history.shift(); }
      else history.pop();
      page = result; cursor = requested;
      selected = preserveSelection && result.rows.some(row => row.id === previousSelection) ? previousSelection : null;
      renameNeedsReload = false;
      message = selected ? 'Saved threads updated.' : page.rows.length ? 'Select a thread to view its details.' : 'No saved threads in this project.';
      return true;
    } catch (error) {
      if (!destroyed && at === generation && selectedProject === project) {
        if (!preserveSelection) reset();
        failed = true; message = preserveSelection ? 'Saved thread updates are unavailable. Reload threads.' : failureText(error); options.reportError(message);
      }
      return false;
    } finally { afterOperation(); }
  }
  function validProject(value: unknown): value is CurrentThreadProject {
    return record(value) && projectId(value.projectId) && (value.label === undefined || safeText(value.label, 128));
  }
  function renderProjects() {
    const placeholder = document.createElement('option'); placeholder.value = ''; placeholder.textContent = 'Choose a project'; scopeSelect.replaceChildren(placeholder);
    for (const project of projects) {
      const option = document.createElement('option'); option.value = project.projectId; option.textContent = project.label || project.projectId; scopeSelect.append(option);
    }
    scopeSelect.value = selectedProject ?? '';
  }
  function setProjects(values: readonly CurrentThreadProject[]): boolean {
    if (destroyed) return false;
    if (!Array.isArray(values) || values.length > 100 || values.some(value => !validProject(value))
      || new Set(values.map(value => value.projectId)).size !== values.length) {
      options.reportError('The project list could not be shown. The selected project is preserved.'); return false;
    }
    const retained = projects.find(project => project.projectId === selectedProject);
    projects = values.map(value => ({ ...value }));
    // A recent-project refresh does not silently change an explicitly chosen
    // scope, including one that has fallen outside the recent-project window.
    if (retained && !projects.some(project => project.projectId === retained.projectId)) {
      if (projects.length === 100) projects.pop(); projects.push(retained);
    }
    renderProjects(); render(); return true;
  }
  function setProject(value: CurrentThreadProject | null) {
    if (destroyed) return false;
    const valid = value === null || validProject(value);
    generation++; selectedProject = valid && value ? value.projectId : null; reset();
    if (valid && value) {
      const previous = projects.find(project => project.projectId === value.projectId);
      const next = { ...previous, ...value };
      projects = [next, ...projects.filter(project => project.projectId !== value.projectId)].slice(0, 100);
    }
    pendingProjectRead = selectedProject !== null;
    renderProjects(); message = selectedProject ? 'Loading threads…' : 'Choose a project to see its threads.';
    render();
    if (pendingProjectRead && !busy) { pendingProjectRead = false; void read(null, 'reload'); }
    if (!valid) options.reportError('The current project is unavailable. No other project was used.'); return valid;
  }
  function cancelDrag() {
    if (!drag) return; const before = drag; drag = null; display = before.before;
    if (separator.hasPointerCapture(before.pointer)) separator.releasePointerCapture(before.pointer); render();
  }
  separator.onpointerdown = event => {
    if (event.button !== 0 || drag || display.collapsed) return;
    uiGeneration++; // A late display restore cannot replace an active gesture.
    drag = { pointer: event.pointerId, x: event.clientX, before: { ...display } }; separator.setPointerCapture(event.pointerId); separator.focus(); event.preventDefault();
  };
  separator.onpointermove = event => {
    if (drag?.pointer !== event.pointerId) return;
    display.width = Math.round(Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, drag.before.width + event.clientX - drag.x))); render();
  };
  separator.onpointerup = event => {
    if (drag?.pointer !== event.pointerId) return; drag = null; separator.releasePointerCapture(event.pointerId); changedDisplay();
  };
  separator.onpointercancel = cancelDrag; separator.onlostpointercapture = cancelDrag;
  separator.onkeydown = event => {
    if (event.key === 'Escape' && drag) { event.preventDefault(); cancelDrag(); return; }
    if (drag || event.altKey || event.ctrlKey || event.metaKey) return;
    let width = display.width;
    if (event.key === 'ArrowRight') width += WIDTH_STEP;
    else if (event.key === 'ArrowLeft') width -= WIDTH_STEP;
    else if (event.key === 'Home') width = MIN_WIDTH;
    else if (event.key === 'End') width = MAX_WIDTH;
    else return;
    event.preventDefault(); display.width = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, width)); changedDisplay();
  };
  collapse.onclick = () => { cancelDrag(); display.collapsed = !display.collapsed; changedDisplay(); };
  scopeSelect.onchange = () => {
    if (busy || destroyed) return;
    generation++; selectedProject = projects.find(project => project.projectId === scopeSelect.value)?.projectId ?? null; reset();
    message = selectedProject ? 'Load threads for this project.' : 'Choose a project explicitly.'; render();
    if (selectedProject) void read(null, 'reload');
  };
  reload.onclick = () => { void read(null, 'reload'); };
  rename.onclick = () => {
    if (destroyed || busy || renameNeedsReload || !options.rename) return;
    const chosen = page?.rows.find(row => row.id === selected && row.projectId === selectedProject);
    if (!chosen) return;
    editing = { ...chosen }; renameInput.value = chosen.title ?? ''; renameError.hidden = true;
    render(); renameInput.focus(); renameInput.select();
  };
  renameInput.oninput = () => { renameError.hidden = true; render(); };
  function cancelRename() {
    if (busy) return;
    editing = null; render();
    if (renameNeedsReload) reload.focus(); else rename.focus();
    scheduleThreadRead();
  }
  renameCancel.onclick = cancelRename;
  renameForm.onkeydown = event => {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); cancelRename(); }
  };
  renameForm.onsubmit = event => {
    event.preventDefault();
    if (destroyed || busy || renameNeedsReload || !editing || !options.rename || !validTitle(renameInput.value)) return;
    const before = editing; const title = renameInput.value.trim(); const at = generation;
    if (before.projectId !== selectedProject || before.id !== selected) return;
    busy = 'rename'; failed = false; renameError.hidden = true; render();
    void (async () => {
      let saved = false;
      try {
        const response = await options.rename!({projectId:before.projectId,threadId:before.id,expectedRevision:before.revision,title});
        if (destroyed || at !== generation || selectedProject !== before.projectId) return;
        const renamed = renamedValue(response, before, title);
        if (!renamed) { renameNeedsReload = true; throw {code:'conflict'}; }
        saved = true; editing = null; selected = renamed.id;
        // Keep the acknowledged title even if the subsequent bounded refresh
        // fails. Old page cursors cannot be reused after its sort key changes.
        page = {rows: (page?.rows ?? []).map(row => row.id === renamed.id ? renamed : row), next:null}; cursor = null; history = [];
        message = 'Thread renamed.';
        try {
          const received = pageValue(await options.list({projectId:before.projectId,cursor:null,limit:50}), before.projectId, null);
          if (destroyed || at !== generation || selectedProject !== before.projectId) return;
          if (!received || !received.rows.some(row => row.id === renamed.id)) throw new Error('Reload required');
          page = received;
        } catch {
          if (!destroyed && at === generation) message = 'Thread renamed. Reload threads to refresh the list.';
        }
      } catch (error) {
        if (!destroyed && at === generation && selectedProject === before.projectId) {
          const code = record(error) ? error.code ?? error.issue : error;
          renameNeedsReload = code === 'conflict' || code === 'source_changed';
          message = renameNeedsReload ? 'This thread changed. Reload threads before renaming again.'
            : code === 'invalid' ? 'Use a shorter name without line breaks.' : 'The thread name could not be saved. Try again.';
          failed = true; renameError.textContent = message; renameError.hidden = false; options.reportError(message);
        }
      } finally {
        afterOperation();
        if (!destroyed && at === generation && selectedProject === before.projectId) {
          if (renameNeedsReload) reload.focus(); else if (saved) rename.focus(); else renameInput.focus();
        }
      }
    })();
  };
  resume.onclick = () => {
    if (destroyed || busy || !options.onResume || !selectedProject) return;
    const chosen = page?.rows.find(row => row.id === selected && row.projectId === selectedProject);
    if (!chosen || chosen.status === 'recorded_open' || chosen.status === 'archived') return;
    if (chosen.status === 'unbound' || chosen.nativeAvailable !== true) {
      failed = true; message = 'No saved conversation yet'; render(); options.reportError(message); return;
    }
    if (options.canResume?.(chosen) !== true) return;
    const project = selectedProject; const at = generation; const requested = cursor;
    busy = 'resume'; failed = false; render();
    void (async () => {
      try {
        await options.onResume!(Object.freeze({ ...chosen }));
        if (destroyed || at !== generation || selectedProject !== project) return;
        message = 'Conversation resumed.';
        try {
          const received = pageValue(await options.list({ projectId: project, cursor: requested, limit: 50 }), project, requested);
          if (destroyed || at !== generation || selectedProject !== project) return;
          if (!received) throw new Error('Invalid thread page');
          page = received;
          if (!page.rows.some(row => row.id === selected)) selected = null;
        } catch {
          if (!destroyed && at === generation && selectedProject === project) message = 'Conversation resumed. Reload threads to update the list.';
        }
      } catch (error) {
        if (!destroyed && at === generation && selectedProject === project) {
          failed = true; message = resumeFailureText(error); options.reportError(message);
        }
      } finally { afterOperation(); }
    })();
  };
  recover.onclick = () => {
    if (destroyed || busy || !selectedProject || !options.onRecover) return;
    const chosen = page?.rows.find(row => row.id === selected && row.projectId === selectedProject);
    if (!chosen || chosen.status !== 'recorded_open' || options.canRecover?.(chosen) !== true) return;
    const project = selectedProject; const at = generation;
    busy = 'recover'; failed = false; render();
    void (async () => {
      let recovered = false;
      try {
        await options.onRecover!(Object.freeze({ ...chosen }));
        recovered = true;
        if (destroyed || at !== generation || selectedProject !== project) return;
        // Finalization updates the sort key. Old page cursors cannot locate the
        // recovered thread reliably; keep it selected on the refreshed first page.
        cursor = null; history = [];
        const received = pageValue(await options.list({ projectId: project, cursor: null, limit: 50 }), project, null);
        if (destroyed || at !== generation || selectedProject !== project) return;
        if (!received) throw new Error('Invalid thread page');
        page = received;
        if (!page.rows.some(row => row.id === selected)) selected = null;
        message = 'Session recovered. Choose Resume to reopen the saved conversation.';
      } catch (error) {
        if (!destroyed && at === generation && selectedProject === project) {
          failed = !recovered;
          message = recovered ? 'Session recovered. Reload threads to show Resume.' : recoveryFailureText(error);
          if (!recovered) options.reportError(message);
        }
      } finally {
        afterOperation();
        if (!destroyed && at === generation && selectedProject === project && recovered) {
          if (!resume.hidden) resume.focus(); else reload.focus();
        }
      }
    })();
  };
  create.onclick = () => {
    if (busy || !selectedProject || destroyed) return;
    if (options.onNewSession) {
      try { options.onNewSession(selectedProject); }
      catch { options.reportError('The new session window could not be opened.'); }
      return;
    }
    if (!options.create) return;
    const project=selectedProject; const at=generation; busy='create'; failed=false; render();
    void (async()=>{
      try {
        await options.create!({projectId:project});
        if (destroyed || at!==generation || selectedProject!==project) return;
        const received=pageValue(await options.list({projectId:project,cursor:null,limit:50}),project,null);
        if (!received) throw new Error('Invalid thread page');
        if (destroyed || at!==generation || selectedProject!==project) return;
        page=received; cursor=null; history=[]; selected=null;
        message='Thread saved. No agent was started.';
      } catch(error) {
        if (!destroyed && at===generation && selectedProject===project) {failed=true; message=failureText(error); options.reportError(message);}
      } finally { afterOperation(); }
    })();
  };
  previous.onclick = () => { if (history.length) void read(history.at(-1) ?? null, 'previous'); };
  next.onclick = () => { if (page?.next) void read(page.next, 'next'); };
  setProject(null);
  if (options.loadState) {
    const at = uiGeneration; busy = 'load_state'; render();
    void (async () => {
      try {
        const value = await options.loadState!();
        if (destroyed || at !== uiGeneration) return;
        if (value === null) return;
        const restored = stateValue(value); if (!restored) throw new Error('Invalid sidebar display'); display = restored;
      } catch { if (!destroyed) options.reportError('Sidebar display could not be restored. Default dimensions are in use.'); }
      finally { afterOperation(); }
    })();
  }
  return {
    element, setProject, setProjects, currentProjectId: (): string | null => selectedProject,
    refreshCapabilities: render,
    notifyThreadChanged,
    refresh: () => read(null, 'reload'), state: (): Readonly<ThreadSidebarState> => Object.freeze({ ...display }),
    destroy() { destroyed = true; generation++; window.clearTimeout(saveTimer); window.clearTimeout(changeTimer); pendingState = null; pendingChangedProject = null; cancelDrag(); anchors.clear(); element.remove(); },
  };
}
