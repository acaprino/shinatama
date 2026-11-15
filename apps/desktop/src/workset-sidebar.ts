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
  /** Adds a session to this thread: opens it, then asks for the agent under `anchor`. */
  onAddSession?: (thread: Readonly<WorksetSummary>, anchor: HTMLElement) => void;
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
/** Projects the sidebar lists at once; matches the stored limit. */
const MAX_ACTIVE = 32;
const NO_PROJECTS = 'No project in the sidebar. Tick projects in the Project menu to see their threads.';
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
  if (!record(value) || value.version !== 1 || !integer(value.width) || value.width < MIN_WIDTH || value.width > MAX_WIDTH || typeof value.collapsed !== 'boolean') return null;
  const listed = value.activeProjects ?? [];
  if (!Array.isArray(listed) || listed.length > MAX_ACTIVE || !listed.every(projectId) || new Set(listed).size !== listed.length) return null;
  return { version: 1, width: value.width, collapsed: value.collapsed, activeProjects: [...listed] };
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
  heading.append(node('h2', '', 'Threads'), collapse);
  const body = node('div', 'thread-sidebar-body'); body.id = 'thread-sidebar-body';
  const showArchivedLabel = node('label', 'thread-sidebar-archived-toggle'); const showArchived = node('input', ''); showArchived.type = 'checkbox';
  showArchivedLabel.append(showArchived, ' Show archived threads'); showArchivedLabel.hidden = !options.update;
  const status = node('p', 'thread-sidebar-status'); status.setAttribute('role', 'status');
  const list = node('ul', 'thread-sidebar-list'); list.setAttribute('aria-label', 'Threads, by project');
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
  body.append(showArchivedLabel, status, list, closedBox, actions, renameForm);
  const separator = node('div', 'thread-sidebar-resize'); separator.tabIndex = 0; separator.setAttribute('role', 'separator');
  separator.setAttribute('aria-label', 'Thread sidebar width'); separator.setAttribute('aria-orientation', 'vertical');
  separator.setAttribute('aria-valuemin', String(MIN_WIDTH)); separator.setAttribute('aria-valuemax', String(MAX_WIDTH));
  element.append(heading, body, separator); options.host.append(element);

  let display: ThreadSidebarState = { version: 1, width: 264, collapsed: false };
  /** `active`: the projects listed, in order; `currentProject`: the toolbar's project, marked among them. */
  let projects: CurrentThreadProject[] = []; let active: string[] = []; let currentProject: string | null = null;
  const groups = new Map<string, { rows: WorksetSummary[]; more: boolean; failed: boolean }>(); const collapsedGroups = new Set<string>();
  const heads = new Map<string, HTMLLIElement>(); const notes = new Map<string, HTMLLIElement>();
  let rows: WorksetSummary[] = []; let archivedView = false;
  let busy = false; let generation = 0; let destroyed = false; let rereadWanted = false;
  // Nothing is saved before the saved state is read: an early save would overwrite it.
  let loaded = !options.loadState; let saveWanted = false;
  let message = NO_PROJECTS; let failed = false;
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
    const now = Date.now(); const at = generation;
    const ids = rows.filter(row => !row.title && row.lead && !leadTitle(row.lead) && row.lead.harness).map(row => row.lead!.threadId)
      .filter(id => { const at = namedAt.get(id); return at === undefined || now - at >= 60000; }).slice(0, 16);
    if (!ids.length) return;
    naming = true; for (const id of ids) namedAt.set(id, now);
    void (async () => {
      try {
        const value = await options.names!(ids);
        if (destroyed || at !== generation || !Array.isArray(value)) return;
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
    showArchived.disabled = busy || !active.length; showArchived.checked = archivedView;
    body.setAttribute('aria-busy', String(busy)); status.textContent = message; status.dataset.state = failed ? 'error' : 'ready'; status.hidden = !message;
    const open = options.openId(); const retained = new Set(rows.map(row => row.id));
    for (const [id, item] of anchors) if (!retained.has(id)) { item.remove(); anchors.delete(id); }
    const nodes: HTMLLIElement[] = [];
    for (const project of active) {
      const head = groupHead(project); const group = groups.get(project); const closed = collapsedGroups.has(project); const name = labelOf(project);
      const toggle = head.querySelector<HTMLButtonElement>('.thread-sidebar-group-toggle')!; toggle.setAttribute('aria-expanded', String(!closed));
      head.querySelector('.thread-sidebar-group-name')!.textContent = name;
      head.querySelector('.thread-sidebar-group-count')!.textContent = group ? `${group.rows.length}${group.more ? '+' : ''}` : '';
      head.toggleAttribute('data-current', project === currentProject);
      const add = head.querySelector<HTMLButtonElement>('.thread-sidebar-group-add')!; add.disabled = busy || archivedView;
      add.title = `New thread in ${name}`; add.setAttribute('aria-label', add.title);
      const hide = head.querySelector<HTMLButtonElement>('.thread-sidebar-group-remove')!; hide.disabled = busy;
      hide.title = `Remove ${name} from the sidebar`; hide.setAttribute('aria-label', hide.title);
      nodes.push(head);
      if (closed || !group) continue;
      if (!group.rows.length) {
        let note = notes.get(project); if (!note) { note = node('li', 'thread-sidebar-group-note'); notes.set(project, note); }
        note.textContent = group.failed ? 'Its threads could not be read.' : archivedView ? 'No archived threads.' : 'No threads yet.'; nodes.push(note);
      }
      for (const row of group.rows) nodes.push(threadItem(row, open));
    }
    for (const child of [...list.children]) if (!nodes.includes(child as HTMLLIElement)) child.remove();
    nodes.forEach((item, position) => { if (list.children[position] !== item) list.insertBefore(item, list.children[position] ?? null); });
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
  function threadItem(row: WorksetSummary, open: string | null): HTMLLIElement {
    let item = anchors.get(row.id);
    if (!item) {
      item = node('li', 'thread-sidebar-row'); const anchor = node('button', 'thread-sidebar-anchor'); anchor.type = 'button'; anchor.dataset.worksetId = row.id;
      anchor.append(node('span', 'thread-sidebar-title'), node('span', 'thread-sidebar-state'), node('span', 'thread-sidebar-updated'));
      anchor.onclick = () => { const actual = rows.find(candidate => candidate.id === row.id); if (actual) void openThread(actual); };
      item.append(anchor);
      if (options.onAddSession) {
        const add = node('button', 'thread-sidebar-add-session', '+'); add.type = 'button';
        add.onclick = () => { const actual = rows.find(candidate => candidate.id === row.id); if (actual && !busy) options.onAddSession!(Object.freeze({ ...actual }), add); };
        item.append(add);
      }
      anchors.set(row.id, item);
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
    const add = item.querySelector<HTMLButtonElement>('.thread-sidebar-add-session');
    if (add) { add.disabled = busy; add.title = `Add a session to ${name}`; add.setAttribute('aria-label', add.title); }
    return item;
  }
  function groupHead(project: string): HTMLLIElement {
    let head = heads.get(project); if (head) return head;
    head = node('li', 'thread-sidebar-group'); head.dataset.projectId = project;
    const toggle = node('button', 'thread-sidebar-group-toggle'); toggle.type = 'button';
    toggle.append(node('span', 'thread-sidebar-group-name'), node('span', 'thread-sidebar-group-count'));
    toggle.onclick = () => { if (!collapsedGroups.delete(project)) collapsedGroups.add(project); render(); };
    const add = node('button', 'thread-sidebar-group-add', '+'); add.type = 'button'; add.hidden = !options.create; add.onclick = () => createIn(project);
    const hide = node('button', 'thread-sidebar-group-remove', '×'); hide.type = 'button'; hide.onclick = () => { if (!busy) setActive(active.filter(id => id !== project)); };
    head.append(toggle, add, hide); heads.set(project, head); return head;
  }
  function disarm() { window.clearTimeout(removeArmed); removeArmed = 0; remove.textContent = removeLabel; remove.classList.remove('danger'); }
  async function read(): Promise<boolean> {
    if (destroyed || !active.length) return false;
    if (busy) { rereadWanted = true; return false; }
    const scope = [...active]; const at = generation; busy = true; render();
    try {
      const results = await Promise.allSettled(scope.map(async project => {
        const value = rowsValue(await options.list({ projectId: project, archived: archivedView }), project, archivedView);
        if (!value) throw new Error('invalid'); return value;
      }));
      if (destroyed || at !== generation) return false;
      let error: unknown = null;
      scope.forEach((project, index) => {
        const result = results[index];
        if (result.status === 'fulfilled') groups.set(project, { ...result.value, failed: false });
        else { groups.set(project, { rows: [], more: false, failed: true }); error ??= result.reason ?? 'failed'; }
      });
      rows = scope.flatMap(project => groups.get(project)?.rows ?? []);
      failed = error !== null;
      message = failed ? failureText(error, 'read') : [...groups.values()].some(group => group.more) ? 'Showing the 200 most recent threads of each project.' : '';
      if (failed) options.reportError(message);
      return !failed;
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
  function createIn(project: string) {
    if (!options.create || busy) return;
    void operate('created', async () => {
      const made = await options.create!({ projectId: project });
      if (!record(made) || !worksetId(made.id) || !integer(made.revision)) throw new Error('invalid');
      const row: WorksetSummary = { id: made.id, projectId: project, title: null, archived: false, revision: made.revision, usedAt: Date.now(), sessions: 0, open: 0, lead: null };
      await options.onOpen(Object.freeze(row));
    });
  }
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
  showArchived.onchange = () => { if (busy || !active.length) { showArchived.checked = archivedView; return; } archivedView = showArchived.checked; generation++; rows = []; groups.clear(); render(); void read(); };

  function validProject(value: unknown): value is CurrentThreadProject { return record(value) && projectId(value.projectId) && (value.label === undefined || text(value.label, 128)); }
  const labelOf = (id: string) => projects.find(project => project.projectId === id)?.label || 'Project';
  function setProjects(values: readonly CurrentThreadProject[]): boolean {
    if (destroyed || !Array.isArray(values) || values.length > 100 || values.some(value => !validProject(value))) return false;
    // A listed project that left the recent list keeps its name.
    const kept = projects.filter(project => active.includes(project.projectId) && !values.some(value => value.projectId === project.projectId));
    projects = [...values.map(value => ({ ...value })), ...kept]; render(); return true;
  }
  /** Lists exactly these projects, in this order, and reads their threads. */
  function setActive(next: readonly string[], save = true) {
    const unique = [...new Set(next)].filter(id => projectId(id)).slice(0, MAX_ACTIVE);
    if (unique.length === active.length && unique.every((id, index) => id === active[index])) { render(); return; }
    generation++; active = unique; editing = false; failed = false; message = active.length ? '' : NO_PROJECTS;
    for (const id of [...groups.keys()]) if (!active.includes(id)) groups.delete(id);
    for (const id of [...heads.keys()]) if (!active.includes(id)) { heads.delete(id); notes.delete(id); }
    rows = active.flatMap(id => groups.get(id)?.rows ?? []);
    if (save) changedDisplay(); else render();
    if (active.length) void read();
  }
  /** The toolbar's project: marked in the list, and listed if it was not. */
  function setProject(value: CurrentThreadProject | null) {
    if (destroyed) return false;
    if (value !== null && !validProject(value)) { options.reportError('The current project is unavailable. No other project was used.'); return false; }
    if (value) projects = [{ ...projects.find(project => project.projectId === value.projectId), ...value }, ...projects.filter(project => project.projectId !== value.projectId)];
    currentProject = value?.projectId ?? null;
    if (value && !active.includes(value.projectId)) setActive([...active, value.projectId]); else render();
    return true;
  }

  function changedDisplay() {
    render(); if (!options.saveState) return;
    if (!loaded) { saveWanted = true; return; }
    window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => { void options.saveState!(Object.freeze({ ...display, activeProjects: [...active] })).catch(() => options.reportError('The sidebar could not be saved.')); }, 300);
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

  render();
  if (options.loadState) void options.loadState().then(value => {
    if (destroyed || value === null) return;
    const restored = stateValue(value); if (!restored) return;
    if (!drag) display = { version: 1, width: restored.width, collapsed: restored.collapsed };
    const saved = restored.activeProjects ?? [];
    setActive([...saved, ...active.filter(id => !saved.includes(id))], false);
  }).catch(() => { if (!destroyed) options.reportError('Sidebar display could not be restored. Default dimensions are in use.'); })
    .finally(() => { loaded = true; if (saveWanted && !destroyed) { saveWanted = false; changedDisplay(); } });
  return {
    element, setProject, setProjects, currentProjectId: (): string | null => currentProject,
    activeIds: (): readonly string[] => [...active],
    toggleActive(id: string) { if (!destroyed) setActive(active.includes(id) ? active.filter(other => other !== id) : [...active, id]); },
    /** Repaints counts and the open mark; reads nothing. */
    repaint: render,
    /** A saved change somewhere: read the list again, once, shortly. */
    changed() { if (destroyed || !active.length) return; window.clearTimeout(changeTimer); changeTimer = window.setTimeout(() => { void read(); }, 400); },
    refresh: read,
    row: (id: string): Readonly<WorksetSummary> | null => rows.find(row => row.id === id) ?? null,
    title: (id: string): string | null => { const row = rows.find(candidate => candidate.id === id); return row ? shownTitle(row) : null; },
    destroy() { destroyed = true; generation++; window.clearInterval(namesTimer); window.clearTimeout(saveTimer); window.clearTimeout(removeArmed); window.clearTimeout(changeTimer); cancelDrag(); anchors.clear(); element.remove(); },
  };
}
