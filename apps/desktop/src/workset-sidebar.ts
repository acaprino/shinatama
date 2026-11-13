// ADR-0055: the sidebar lists threads. A thread is a set of sessions with its own layout; a click
// opens it, and the window then shows all and only its sessions. Nothing here starts an agent.
import './thread-sidebar.css';
import './workset-sidebar.css';
import type { CurrentThreadProject, ThreadSidebarState } from './thread-sidebar.ts';

export type WorksetLead = { threadId: string; title: string | null; harness: string | null };
export type WorksetSummary = {
  id: string; projectId: string; title: string | null; archived: boolean; revision: number;
  usedAt: number; sessions: number; open: number; lead: WorksetLead | null;
};
export type WorksetUpdate = { worksetId: string; expectedRevision: number; title?: string; archived?: boolean };
export type WorksetSidebarOptions = {
  host: HTMLElement;
  list: (request: { projectId: string; archived: boolean }) => Promise<unknown>;
  create?: (request: { projectId: string }) => Promise<unknown>;
  update?: (request: WorksetUpdate) => Promise<unknown>;
  /** Forgets every session of the thread, then the thread; refused while one of them runs. */
  remove?: (thread: Readonly<WorksetSummary>) => Promise<void>;
  /** Shows the thread's layout. Starts, resumes and stops nothing. */
  onOpen: (thread: Readonly<WorksetSummary>) => Promise<boolean>;
  openId: () => string | null;
  /** Sessions of this thread with a running terminal in this window. */
  running: (worksetId: string) => number;
  /** Sessions of the open thread that were closed: kept, not shown as tabs. */
  closed?: () => readonly { id: string; name: string; updatedAt: number }[];
  /** Puts a closed session back among the tabs; starts nothing. */
  onRestore?: (threadId: string) => Promise<void>;
  /** Deletes one closed session for good, after a second click. */
  onForgetSession?: (threadId: string) => Promise<void>;
  /** An agent of this thread is waiting for the user. */
  attention?: (worksetId: string) => boolean;
  names?: (threadIds: readonly string[]) => Promise<unknown>;
  loadState?: () => Promise<unknown>;
  saveState?: (state: Readonly<ThreadSidebarState>) => Promise<void>;
  reportError: (message: string) => void;
};
const AGENT_LABELS: Record<string, string> = { claude_code: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi', hermes: 'Hermes Agent', deepseek: 'DeepSeek Harness' };
const MIN_WIDTH = 180; const MAX_WIDTH = 480; const WIDTH_STEP = 16;
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, cap: number): value is string => typeof value === 'string' && value.length <= cap && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
const worksetId = (value: unknown): value is string => typeof value === 'string' && /^w_[a-z2-7]{12}$/.test(value);
const threadId = (value: unknown): value is string => typeof value === 'string' && /^t_[a-z2-7]{12}$/.test(value);
const projectId = (value: unknown): value is string => typeof value === 'string' && /^p_[a-z2-7]{12}$/.test(value);
function rowsValue(value: unknown, scope: string, archived: boolean): { rows: WorksetSummary[]; more: boolean } | null {
  if (!record(value) || !Array.isArray(value.rows) || value.rows.length > 200 || typeof value.more !== 'boolean') return null;
  const rows: WorksetSummary[] = []; const seen = new Set<string>();
  for (const row of value.rows) {
    if (!record(row) || !worksetId(row.id) || seen.has(row.id) || row.projectId !== scope || row.archived !== archived
      || (row.title !== null && !text(row.title, 256)) || !integer(row.revision) || row.revision === 0 || !integer(row.usedAt)
      || !integer(row.sessions) || !integer(row.open) || row.open > row.sessions) return null;
    let lead: WorksetLead | null = null;
    if (row.lead !== null) {
      if (!record(row.lead) || !threadId(row.lead.threadId) || (row.lead.title !== null && !text(row.lead.title, 256))
        || (row.lead.harness !== null && !text(row.lead.harness, 64))) return null;
      lead = { threadId: row.lead.threadId, title: row.lead.title as string | null, harness: row.lead.harness as string | null };
    }
    seen.add(row.id);
    rows.push({ id: row.id, projectId: scope, title: row.title as string | null, archived, revision: row.revision, usedAt: row.usedAt, sessions: row.sessions, open: row.open, lead });
  }
  return { rows, more: value.more };
}
function stateValue(value: unknown): ThreadSidebarState | null {
  return record(value) && value.version === 1 && integer(value.width) && value.width >= MIN_WIDTH && value.width <= MAX_WIDTH
    && typeof value.collapsed === 'boolean' ? { version: 1, width: value.width, collapsed: value.collapsed } : null;
}
function failureText(error: unknown, doing: string) {
  const code = typeof error === 'string' ? error : record(error) ? error.code ?? error.issue : null;
  if (code === 'project_missing') return 'The project is unavailable. Choose it again.';
  if (code === 'busy') return 'Thread storage is busy. Try again.';
  if (code === 'conflict') return 'This thread changed. Reload and try again.';
  if (code === 'owned_elsewhere') return 'A session of this thread is running. Stop it first.';
  return `The thread could not be ${doing}. Nothing was changed.`;
}
function agoText(timestamp: number) {
  const minutes = Math.floor((Date.now() - timestamp) / 60000);
  if (minutes < 1) return 'now'; if (minutes < 60) return `${minutes} min ago`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)} h ago`;
  const date = new Date(timestamp); return Number.isNaN(date.valueOf()) ? '' : date.toLocaleDateString();
}
const node = <K extends keyof HTMLElementTagNameMap>(tag: K, className: string, content = ''): HTMLElementTagNameMap[K] => {
  const created = document.createElement(tag); created.className = className; if (content) created.textContent = content; return created;
};

export function createWorksetSidebar(options: WorksetSidebarOptions) {
  const element = node('aside', 'thread-sidebar'); element.setAttribute('aria-label', 'Project threads');
  const heading = node('header', 'thread-sidebar-heading'); const collapse = node('button', 'thread-sidebar-toggle'); collapse.type = 'button';
  collapse.setAttribute('aria-controls', 'thread-sidebar-body');
  const create = node('button', 'thread-sidebar-toggle thread-sidebar-new', '+'); create.type = 'button'; create.hidden = !options.create;
  create.title = 'New thread'; create.setAttribute('aria-label', 'New thread');
  heading.append(node('h2', '', 'Threads'), create, collapse);
  const body = node('div', 'thread-sidebar-body'); body.id = 'thread-sidebar-body';
  const scopeLabel = node('label', 'thread-sidebar-scope', 'Project'); scopeLabel.htmlFor = 'thread-sidebar-project';
  const scopeSelect = node('select', 'thread-sidebar-scope'); scopeSelect.id = 'thread-sidebar-project';
  const showArchivedLabel = node('label', 'thread-sidebar-archived-toggle'); const showArchived = node('input', ''); showArchived.type = 'checkbox';
  showArchivedLabel.append(showArchived, ' Show archived threads'); showArchivedLabel.hidden = !options.update;
  const status = node('p', 'thread-sidebar-status'); status.setAttribute('role', 'status');
  const list = node('ul', 'thread-sidebar-list'); list.setAttribute('aria-label', 'Threads of this project');
  const actions = node('div', 'thread-sidebar-actions');
  const rename = node('button', '', 'Rename'); rename.type = 'button';
  const archive = node('button', ''); archive.type = 'button';
  const removeLabel = 'Forget thread…'; const remove = node('button', '', removeLabel); remove.type = 'button';
  actions.append(rename, archive, remove);
  const renameForm = node('form', 'thread-sidebar-rename'); renameForm.hidden = true;
  const renameLabel = node('label', '', 'Thread name'); renameLabel.htmlFor = 'thread-rename-title';
  const renameInput = node('input', ''); renameInput.id = 'thread-rename-title'; renameInput.type = 'text'; renameInput.maxLength = 256; renameInput.autocomplete = 'off';
  const renameButtons = node('div', ''); const renameSave = node('button', '', 'Save'); renameSave.type = 'submit';
  const renameCancel = node('button', '', 'Cancel'); renameCancel.type = 'button';
  renameButtons.append(renameSave, renameCancel); renameForm.append(renameLabel, renameInput, renameButtons);
  const closedBox = node('details', 'thread-sidebar-closed'); const closedSummary = node('summary', ''); const closedList = node('ul', '');
  closedBox.append(closedSummary, closedList);
  body.append(scopeLabel, scopeSelect, showArchivedLabel, status, list, closedBox, actions, renameForm);
  const separator = node('div', 'thread-sidebar-resize'); separator.tabIndex = 0; separator.setAttribute('role', 'separator');
  separator.setAttribute('aria-label', 'Thread sidebar width'); separator.setAttribute('aria-orientation', 'vertical');
  separator.setAttribute('aria-valuemin', String(MIN_WIDTH)); separator.setAttribute('aria-valuemax', String(MAX_WIDTH));
  element.append(heading, body, separator); options.host.append(element);

  let display: ThreadSidebarState = { version: 1, width: 264, collapsed: false };
  let projects: CurrentThreadProject[] = []; let selectedProject: string | null = null;
  let rows: WorksetSummary[] = []; let more = false; let archivedView = false;
  let busy = false; let generation = 0; let destroyed = false; let rereadWanted = false;
  let message = 'Choose a project to see its threads.'; let failed = false;
  let editing = false; let removeArmed = 0; let forgetting: string | null = null; let saveTimer = 0; let changeTimer = 0;
  let drag: { pointer: number; x: number; before: ThreadSidebarState } | null = null;
  const anchors = new Map<string, HTMLLIElement>();
  /** Names agents gave their conversations: they name a thread the user has not named. */
  const agentNames = new Map<string, string>(); const namedAt = new Map<string, number>(); let naming = false;
  const leadTitle = (lead: WorksetLead) => lead.title && !(lead.harness && AGENT_LABELS[lead.harness] === lead.title) ? lead.title : null;
  const shownTitle = (row: WorksetSummary) => row.title || (row.lead && (leadTitle(row.lead) || agentNames.get(row.lead.threadId) || row.lead.title)) || 'New thread';
  const chosen = () => rows.find(row => row.id === options.openId()) ?? null;

  function requestNames() {
    if (!options.names || naming || destroyed || busy) return;
    const now = Date.now(); const project = selectedProject;
    const ids = rows.filter(row => !row.title && row.lead && !leadTitle(row.lead) && row.lead.harness).map(row => row.lead!.threadId)
      .filter(id => { const at = namedAt.get(id); return at === undefined || now - at >= 60000; }).slice(0, 16);
    if (!ids.length) return;
    naming = true; for (const id of ids) namedAt.set(id, now);
    void (async () => {
      try {
        const value = await options.names!(ids);
        if (destroyed || selectedProject !== project || !Array.isArray(value)) return;
        for (const item of value) {
          if (!record(item) || !threadId(item.threadId)) continue;
          if (text(item.name, 512) && item.name.trim()) agentNames.set(item.threadId, item.name.trim()); else agentNames.delete(item.threadId);
        }
        render();
      } catch { for (const id of ids) namedAt.delete(id); }
      finally { naming = false; }
    })();
  }
  const namesTimer = window.setInterval(() => { if (!display.collapsed) requestNames(); }, 15000);

  function render() {
    if (destroyed) return;
    element.classList.toggle('is-collapsed', display.collapsed); element.style.setProperty('--thread-sidebar-width', `${display.width}px`);
    body.hidden = display.collapsed; separator.hidden = display.collapsed; separator.setAttribute('aria-valuenow', String(display.width));
    collapse.textContent = display.collapsed ? '›' : '‹'; collapse.setAttribute('aria-label', display.collapsed ? 'Expand threads' : 'Collapse threads');
    collapse.setAttribute('aria-expanded', String(!display.collapsed));
    scopeSelect.disabled = !projects.length || busy; create.disabled = busy || selectedProject === null || archivedView; create.hidden = !options.create || display.collapsed;
    showArchived.disabled = busy || selectedProject === null; showArchived.checked = archivedView;
    body.setAttribute('aria-busy', String(busy)); status.textContent = message; status.dataset.state = failed ? 'error' : 'ready'; status.hidden = !message;
    const open = options.openId(); const retained = new Set(rows.map(row => row.id));
    for (const [id, item] of anchors) if (!retained.has(id)) { item.remove(); anchors.delete(id); }
    rows.forEach((row, position) => {
      let item = anchors.get(row.id);
      if (!item) {
        item = document.createElement('li'); const anchor = node('button', 'thread-sidebar-anchor'); anchor.type = 'button'; anchor.dataset.worksetId = row.id;
        anchor.append(node('span', 'thread-sidebar-title'), node('span', 'thread-sidebar-state'), node('span', 'thread-sidebar-updated'));
        anchor.onclick = () => { const actual = rows.find(candidate => candidate.id === row.id); if (actual) void openThread(actual); };
        item.append(anchor); anchors.set(row.id, item);
      }
      const anchor = item.firstElementChild as HTMLButtonElement; anchor.disabled = busy;
      const name = shownTitle(row); const titleLine = anchor.querySelector<HTMLElement>('.thread-sidebar-title')!;
      if (titleLine.textContent !== name) { titleLine.textContent = name; anchor.title = name; }
      const running = options.running(row.id); const waiting = options.attention?.(row.id) === true;
      const kind = row.archived ? 'archived' : waiting ? 'recover' : running ? 'open' : 'resumable';
      const sessions = `${row.sessions} session${row.sessions === 1 ? '' : 's'}`;
      const said = row.archived ? `Archived · ${sessions}` : waiting ? `${sessions} · waiting for you` : running ? `${sessions} · ${running} running` : row.sessions ? sessions : 'No session yet';
      const stateLine = anchor.querySelector<HTMLElement>('.thread-sidebar-state')!;
      if (stateLine.dataset.state !== kind || stateLine.textContent !== said) { stateLine.dataset.state = kind; stateLine.textContent = said; }
      anchor.querySelector('.thread-sidebar-updated')!.textContent = agoText(row.usedAt);
      const pressed = String(open === row.id); if (anchor.getAttribute('aria-pressed') !== pressed) anchor.setAttribute('aria-pressed', pressed);
      if (list.children[position] !== item) list.insertBefore(item, list.children[position] ?? null);
    });
    const current = chosen();
    actions.hidden = !current || editing; rename.hidden = !options.update; archive.hidden = !options.update; remove.hidden = !options.remove;
    rename.disabled = archive.disabled = remove.disabled = busy;
    archive.textContent = current?.archived ? 'Unarchive' : 'Archive';
    if ((actions.hidden || remove.hidden) && removeArmed) disarm();
    renameForm.hidden = !editing; renameInput.disabled = renameSave.disabled = renameCancel.disabled = busy;
    const closed = current && options.closed ? options.closed() : [];
    closedBox.hidden = !closed.length; closedSummary.textContent = `Closed sessions (${closed.length})`;
    if (!closed.some(session => session.id === forgetting)) forgetting = null;
    closedList.replaceChildren(...closed.map(session => {
      const item = document.createElement('li'); const name = node('span', 'thread-sidebar-closed-name', session.name); name.title = `${session.name} · ${agoText(session.updatedAt)}`;
      const restore = node('button', '', 'Reopen'); restore.type = 'button'; restore.disabled = busy; restore.dataset.restore = session.id;
      restore.onclick = () => { void operate('reopened', async () => { await options.onRestore?.(session.id); }); };
      const forget = node('button', forgetting === session.id ? 'danger' : '', forgetting === session.id ? 'Sure?' : 'Forget'); forget.type = 'button'; forget.disabled = busy;
      forget.title = 'Delete this session from Shinatama. The agent\'s own session files stay.';
      forget.onclick = () => {
        if (forgetting !== session.id) { forgetting = session.id; render(); return; }
        forgetting = null; void operate('forgotten', async () => { await options.onForgetSession?.(session.id); });
      };
      item.append(name, restore, forget); return item;
    }));
    requestNames();
  }
  function disarm() { window.clearTimeout(removeArmed); removeArmed = 0; remove.textContent = removeLabel; remove.classList.remove('danger'); }
  async function read(): Promise<boolean> {
    if (destroyed || !selectedProject) return false;
    if (busy) { rereadWanted = true; return false; }
    const project = selectedProject; const at = generation; busy = true; render();
    try {
      const value = rowsValue(await options.list({ projectId: project, archived: archivedView }), project, archivedView);
      if (!value) throw new Error('invalid');
      if (destroyed || at !== generation) return false;
      rows = value.rows; more = value.more; failed = false;
      message = rows.length ? more ? 'Showing the 200 most recent threads.' : '' : archivedView ? 'No archived threads.' : 'No threads yet. Start a session or make a new thread.';
      return true;
    } catch (error) {
      if (!destroyed && at === generation) { failed = true; message = failureText(error, 'read'); options.reportError(message); }
      return false;
    } finally { done(); }
  }
  function done() {
    busy = false; render();
    if (!destroyed && rereadWanted) { rereadWanted = false; void read(); }
  }
  /** One operation at a time; the list is read again after every change. */
  async function operate(doing: string, work: () => Promise<void>) {
    if (busy || destroyed) return;
    const at = generation; busy = true; failed = false; render();
    try { await work(); }
    catch (error) { if (!destroyed && at === generation) { failed = true; message = failureText(error, doing); options.reportError(message); } }
    finally { busy = false; if (!destroyed && at === generation) { rereadWanted = false; await read(); } else render(); }
  }
  async function openThread(row: WorksetSummary) {
    if (busy || destroyed || options.openId() === row.id) return;
    editing = false; busy = true; render();
    try { if (await options.onOpen(Object.freeze({ ...row }))) { failed = false; message = ''; } else { failed = true; message = 'The thread could not be opened. The current one stays open.'; } }
    catch { failed = true; message = 'The thread could not be opened. The current one stays open.'; }
    finally { done(); }
  }
  create.onclick = () => {
    const project = selectedProject; if (!project || !options.create) return;
    void operate('created', async () => {
      const made = await options.create!({ projectId: project });
      if (!record(made) || !worksetId(made.id) || !integer(made.revision)) throw new Error('invalid');
      const row: WorksetSummary = { id: made.id, projectId: project, title: null, archived: false, revision: made.revision, usedAt: Date.now(), sessions: 0, open: 0, lead: null };
      await options.onOpen(Object.freeze(row));
    });
  };
  rename.onclick = () => { const current = chosen(); if (!current || busy) return; editing = true; renameInput.value = current.title ?? shownTitle(current); render(); renameInput.focus(); renameInput.select(); };
  renameCancel.onclick = () => { editing = false; render(); };
  renameForm.onsubmit = event => {
    event.preventDefault(); const current = chosen(); const title = renameInput.value.trim();
    if (!current || !options.update || !title || !text(title, 256)) return;
    editing = false;
    void operate('renamed', async () => { await options.update!({ worksetId: current.id, expectedRevision: current.revision, title }); });
  };
  renameInput.onkeydown = event => { if (event.key === 'Escape') { event.preventDefault(); editing = false; render(); } };
  archive.onclick = () => {
    const current = chosen(); if (!current || !options.update) return;
    void operate('changed', async () => { await options.update!({ worksetId: current.id, expectedRevision: current.revision, archived: !current.archived }); });
  };
  remove.onclick = () => {
    const current = chosen(); if (!current || !options.remove || busy) return;
    if (!removeArmed) {
      remove.textContent = 'Click again to forget'; remove.classList.add('danger'); removeArmed = window.setTimeout(disarm, 5000); failed = false;
      message = `Forgetting deletes ${shownTitle(current)} and its ${current.sessions} session${current.sessions === 1 ? '' : 's'}, closed ones included, from Shinatama. The agents' own session files stay.`; render(); return;
    }
    disarm();
    void operate('forgotten', async () => { await options.remove!(Object.freeze({ ...current })); message = ''; });
  };
  showArchived.onchange = () => { if (busy || !selectedProject) { showArchived.checked = archivedView; return; } archivedView = showArchived.checked; rows = []; void read(); };

  function validProject(value: unknown): value is CurrentThreadProject { return record(value) && projectId(value.projectId) && (value.label === undefined || text(value.label, 128)); }
  function renderProjects() {
    const placeholder = document.createElement('option'); placeholder.value = ''; placeholder.textContent = 'Choose a project'; scopeSelect.replaceChildren(placeholder);
    for (const project of projects) { const option = document.createElement('option'); option.value = project.projectId; option.textContent = project.label || project.projectId; scopeSelect.append(option); }
    scopeSelect.value = selectedProject ?? '';
  }
  function setProjects(values: readonly CurrentThreadProject[]): boolean {
    if (destroyed || !Array.isArray(values) || values.length > 100 || values.some(value => !validProject(value))) return false;
    const retained = projects.find(project => project.projectId === selectedProject);
    projects = values.map(value => ({ ...value }));
    if (retained && !projects.some(project => project.projectId === retained.projectId)) { if (projects.length === 100) projects.pop(); projects.push(retained); }
    renderProjects(); render(); return true;
  }
  function choose(next: string | null) {
    generation++; selectedProject = next; rows = []; more = false; editing = false; failed = false; archivedView = false;
    message = next ? '' : 'Choose a project to see its threads.'; renderProjects(); render();
    if (next) void read();
  }
  function setProject(value: CurrentThreadProject | null) {
    if (destroyed) return false;
    if (value !== null && !validProject(value)) { options.reportError('The current project is unavailable. No other project was used.'); return false; }
    if (value) projects = [{ ...projects.find(project => project.projectId === value.projectId), ...value }, ...projects.filter(project => project.projectId !== value.projectId)].slice(0, 100);
    if ((value?.projectId ?? null) === selectedProject) { renderProjects(); return true; }
    choose(value?.projectId ?? null); return true;
  }
  scopeSelect.onchange = () => { if (!busy && !destroyed) choose(projects.find(project => project.projectId === scopeSelect.value)?.projectId ?? null); else scopeSelect.value = selectedProject ?? ''; };

  function changedDisplay() {
    render(); if (!options.saveState) return;
    window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => { void options.saveState!(Object.freeze({ ...display })).catch(() => options.reportError('The sidebar size could not be saved.')); }, 300);
  }
  function cancelDrag() {
    if (!drag) return; const before = drag; drag = null; display = before.before;
    if (separator.hasPointerCapture(before.pointer)) separator.releasePointerCapture(before.pointer); render();
  }
  separator.onpointerdown = event => {
    if (event.button !== 0 || drag || display.collapsed) return;
    drag = { pointer: event.pointerId, x: event.clientX, before: { ...display } }; separator.setPointerCapture(event.pointerId); separator.focus(); event.preventDefault();
  };
  separator.onpointermove = event => { if (drag?.pointer !== event.pointerId) return; display.width = Math.round(Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, drag.before.width + event.clientX - drag.x))); render(); };
  separator.onpointerup = event => { if (drag?.pointer !== event.pointerId) return; drag = null; separator.releasePointerCapture(event.pointerId); changedDisplay(); };
  separator.onpointercancel = cancelDrag; separator.onlostpointercapture = cancelDrag;
  separator.onkeydown = event => {
    if (event.key === 'Escape' && drag) { event.preventDefault(); cancelDrag(); return; }
    if (drag || event.altKey || event.ctrlKey || event.metaKey) return;
    let width = display.width;
    if (event.key === 'ArrowRight') width += WIDTH_STEP; else if (event.key === 'ArrowLeft') width -= WIDTH_STEP;
    else if (event.key === 'Home') width = MIN_WIDTH; else if (event.key === 'End') width = MAX_WIDTH; else return;
    event.preventDefault(); display.width = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, width)); changedDisplay();
  };
  collapse.onclick = () => { cancelDrag(); display.collapsed = !display.collapsed; changedDisplay(); };

  renderProjects(); render();
  if (options.loadState) void options.loadState().then(value => {
    if (destroyed || value === null) return;
    const restored = stateValue(value); if (restored && !drag) { display = restored; render(); }
  }).catch(() => { if (!destroyed) options.reportError('Sidebar display could not be restored. Default dimensions are in use.'); });
  return {
    element, setProject, setProjects, currentProjectId: (): string | null => selectedProject,
    /** Repaints counts and the open mark; reads nothing. */
    repaint: render,
    /** A saved change somewhere: read the list again, once, shortly. */
    changed() { if (destroyed || !selectedProject) return; window.clearTimeout(changeTimer); changeTimer = window.setTimeout(() => { void read(); }, 400); },
    refresh: read,
    row: (id: string): Readonly<WorksetSummary> | null => rows.find(row => row.id === id) ?? null,
    title: (id: string): string | null => { const row = rows.find(candidate => candidate.id === id); return row ? shownTitle(row) : null; },
    destroy() { destroyed = true; generation++; window.clearInterval(namesTimer); window.clearTimeout(saveTimer); window.clearTimeout(removeArmed); window.clearTimeout(changeTimer); cancelDrag(); anchors.clear(); element.remove(); },
  };
}
