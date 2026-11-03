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
export type ThreadSidebarState = { version: 1; width: number; collapsed: boolean };
export type ThreadSidebarOptions = {
  host: HTMLElement;
  list: (request: ThreadListRequest) => Promise<unknown>;
  create?: (request: {projectId:string}) => Promise<unknown>;
  loadState?: () => Promise<unknown>;
  saveState?: (state: Readonly<ThreadSidebarState>) => Promise<void>;
  /** Select already saved metadata only; this sidebar has no launch authority. */
  onSelect?: (thread: Readonly<ThreadSummary>) => void;
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

/** Saved metadata only. One operation, one latest pending UI-state save, no replay. */
export function createThreadSidebar(options: ThreadSidebarOptions) {
  const element = document.createElement('aside'); element.className = 'thread-sidebar'; element.setAttribute('aria-label', 'Project threads');
  const heading = document.createElement('header'); heading.className = 'thread-sidebar-heading';
  const title = document.createElement('h2'); title.textContent = 'Threads';
  const collapse = document.createElement('button'); collapse.type = 'button'; collapse.className = 'thread-sidebar-toggle';
  collapse.setAttribute('aria-controls', 'thread-sidebar-body'); heading.append(title, collapse);
  const body = document.createElement('div'); body.id = 'thread-sidebar-body'; body.className = 'thread-sidebar-body';
  const scopeLabel = document.createElement('label'); scopeLabel.htmlFor = 'thread-sidebar-project'; scopeLabel.textContent = 'Project';
  const scopeSelect = document.createElement('select'); scopeSelect.id = 'thread-sidebar-project';
  const reload = document.createElement('button'); reload.type = 'button'; reload.textContent = 'Reload threads';
  const create = document.createElement('button'); create.type = 'button'; create.textContent = 'New thread'; create.hidden = !options.create;
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
  for (const text of ['Resume unavailable', 'Hop unavailable']) { const button = document.createElement('button'); button.type = 'button'; button.disabled = true; button.textContent = text; actions.append(button); }
  body.append(scopeLabel, scopeSelect, create, reload, status, list, pagination, detail, limitation, actions);
  const separator = document.createElement('div'); separator.className = 'thread-sidebar-resize'; separator.tabIndex = 0;
  separator.setAttribute('role', 'separator'); separator.setAttribute('aria-label', 'Thread sidebar width'); separator.setAttribute('aria-orientation', 'vertical');
  separator.setAttribute('aria-valuemin', String(MIN_WIDTH)); separator.setAttribute('aria-valuemax', String(MAX_WIDTH));
  element.append(heading, body, separator); options.host.append(element);
  let display = { ...initialState }; let current: CurrentThreadProject | null = null; let selectedProject: string | null = null;
  let page: ThreadListPage | null = null; let cursor: ThreadCursor | null = null; let history: (ThreadCursor | null)[] = [];
  let selected: string | null = null; let generation = 0; let uiGeneration = 0; let destroyed = false;
  let busy: 'load_state' | 'list' | 'save_state' | 'create' | null = null; let message = 'Choose a project, then load its saved threads.'; let failed = false;
  let pendingState: ThreadSidebarState | null = null; let saveTimer = 0;
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
    reload.disabled = busy !== null || selectedProject === null; scopeSelect.disabled = current === null || busy === 'list';
    create.disabled = busy !== null || selectedProject === null;
    previous.disabled = busy !== null || !history.length; next.disabled = busy !== null || !page?.next;
    body.setAttribute('aria-busy', String(busy !== null));
    status.textContent = busy === 'list' ? 'Reading saved threads…' : busy === 'create' ? 'Saving a new thread…' : busy === 'load_state' ? 'Restoring sidebar display…' : message;
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
          const actual = page?.rows.find(thread => thread.id === row.id); if (!actual || actual.projectId !== selectedProject) return;
          selected = actual.id; render();
          try { options.onSelect?.(Object.freeze({ ...actual })); } catch { options.reportError('Thread selection could not be shown. No agent was started.'); }
        };
      }
      const anchor = node.firstElementChild as HTMLButtonElement;
      const pressed = String(selected === row.id);
      if (anchor.getAttribute('aria-pressed') !== pressed) anchor.setAttribute('aria-pressed', pressed);
      if (renderedRows.get(node) !== row) {
        anchor.setAttribute('aria-label', `${row.title || 'Untitled thread'} · ${row.id}`);
        anchor.querySelector('.thread-sidebar-title')!.textContent = row.title || 'Untitled thread';
        anchor.querySelector('.thread-sidebar-id')!.textContent = row.id;
        anchor.querySelector('.thread-sidebar-agent')!.textContent = row.harness ?? 'Agent unknown';
        anchor.querySelector('.thread-sidebar-updated')!.textContent = `Updated: ${updatedText(row.updatedAt)}`;
        anchor.querySelector('.thread-sidebar-source')!.textContent = `Source: ${sourceText(row.nativeAvailable)}`;
        renderedRows.set(node, row);
      }
      // Preserve actual DOM/focus when only width or selection changes.
      if (list.children[position] !== node) list.insertBefore(node, list.children[position] ?? null);
      position++;
    }
    pageLabel.textContent = page ? `${page.rows.length} on this page` : 'No page loaded';
    detail.replaceChildren(); const chosen = page?.rows.find(row => row.id === selected);
    if (chosen) {
      metadata('Thread ID', chosen.id); metadata('Agent', chosen.harness ?? 'Unknown'); metadata('Title', chosen.title || 'Unknown');
      metadata('Updated', updatedText(chosen.updatedAt)); metadata('Source', sourceText(chosen.nativeAvailable));
      metadata('Native activity', 'Unknown');
      metadata('Saved status', { unbound: 'No native session linked', inactive: 'Recorded inactive', recorded_open: 'Recorded open; activity unknown', archived: 'Archived' }[chosen.status]);
    }
  }
  function reset() { page = null; cursor = null; history = []; selected = null; failed = false; }
  function afterOperation() {
    busy = null; render();
    if (!destroyed && pendingState) scheduleSave();
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
  async function read(requested: ThreadCursor | null, direction: 'reload' | 'previous' | 'next') {
    if (destroyed || busy || !selectedProject) return false;
    const project = selectedProject; const at = generation; busy = 'list'; failed = false; render();
    try {
      const result = pageValue(await options.list({ projectId: project, cursor: requested, limit: 50 }), project, requested);
      if (!result) throw new Error('Invalid bounded thread projection');
      if (destroyed || at !== generation || selectedProject !== project) return false;
      if (direction === 'reload') history = [];
      else if (direction === 'next') { history.push(cursor); if (history.length > 64) history.shift(); }
      else history.pop();
      page = result; cursor = requested; selected = null;
      message = page.rows.length ? 'Saved records only. Select a thread to inspect its metadata.' : 'No saved threads in this project.';
      return true;
    } catch (error) {
      if (!destroyed && at === generation && selectedProject === project) {
        reset(); failed = true; message = failureText(error); options.reportError(message);
      }
      return false;
    } finally { afterOperation(); }
  }
  function setProject(value: CurrentThreadProject | null) {
    const valid = value === null || (record(value) && projectId(value.projectId) && (value.label === undefined || safeText(value.label, 128)));
    generation++; current = valid ? value : null; selectedProject = current?.projectId ?? null; reset();
    scopeSelect.replaceChildren(); const placeholder = document.createElement('option'); placeholder.value = ''; placeholder.textContent = 'Choose current project'; scopeSelect.append(placeholder);
    if (current) {
      const option = document.createElement('option'); option.value = current.projectId; option.textContent = `Current project: ${current.label || current.projectId}`; scopeSelect.append(option);
    }
    scopeSelect.value = selectedProject ?? ''; message = current ? 'Load threads for this project.' : 'Choose a project, then load its saved threads.';
    render(); if (!valid) options.reportError('The current project is unavailable. No other project was used.'); return valid;
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
  scopeSelect.onchange = () => { generation++; selectedProject = current && scopeSelect.value === current.projectId ? current.projectId : null; reset(); message = selectedProject ? 'Load threads for this project.' : 'Choose the current project explicitly.'; render(); };
  reload.onclick = () => { void read(null, 'reload'); };
  create.onclick = () => {
    if (busy || !selectedProject || !options.create) return;
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
    element, setProject, refresh: () => read(null, 'reload'), state: (): Readonly<ThreadSidebarState> => Object.freeze({ ...display }),
    destroy() { destroyed = true; generation++; window.clearTimeout(saveTimer); pendingState = null; cancelDrag(); anchors.clear(); element.remove(); },
  };
}
