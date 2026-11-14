import './thread-sidebar.css';
import { resumeFailureCode, resumeFailureText } from './resume-failure.ts';

export type CurrentThreadProject = { projectId: string; label?: string };
export type ThreadCursor = { updatedAt: number; threadId: string };
export type ThreadSummary = {
  id: string; projectId: string; kind: 'work' | 'roundtable' | 'companion'; title: string | null;
  revision: number; updatedAt: number; harness: string | null; nativeAvailable: boolean | null;
  status: 'unbound' | 'inactive' | 'recorded_open' | 'archived';
};
export type ThreadListPage = { rows: ThreadSummary[]; next: ThreadCursor | null };
export type ThreadListRequest = { projectId: string; cursor: ThreadCursor | null; limit: 50; archived: boolean };
export type ThreadArchiveRequest = { projectId: string; threadId: string; expectedRevision: number; archived: boolean };
export type ThreadForgetRequest = { projectId: string; threadId: string; expectedRevision: number };
export type ThreadRenameRequest = { projectId: string; threadId: string; expectedRevision: number; title: string };
export type ThreadSidebarState = { version: 1; width: number; collapsed: boolean };
export type ThreadSidebarOptions = {
  host: HTMLElement;
  list: (request: ThreadListRequest) => Promise<unknown>;
  create?: (request: {projectId:string}) => Promise<unknown>;
  rename?: (request: ThreadRenameRequest) => Promise<unknown>;
  archive?: (request: ThreadArchiveRequest) => Promise<unknown>;
  /** Permanent deletion of a thread without an open session (FRD-0014 R27), after a second click. */
  forget?: (request: ThreadForgetRequest) => Promise<unknown>;
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
  /** True while a running tab in this window holds the thread. */
  isOpen?: (thread: Readonly<ThreadSummary>) => boolean;
  /** The names agents gave their own conversations, for threads the user has not named (at most 16 ids). */
  names?: (threadIds: readonly string[]) => Promise<unknown>;
  reportError: (message: string) => void;
};
/** A new agent thread is saved under its agent's name; that is not a name the user gave it. */
const AGENT_LABELS: Record<string, string> = { claude_code: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi', hermes: 'Hermes Agent', deepseek: 'DeepSeek Harness' };
const userTitle = (row: ThreadSummary): string | null => row.title && !(row.harness && AGENT_LABELS[row.harness] === row.title) ? row.title : null;
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
function pageValue(value: unknown, scope: string, cursor: ThreadCursor | null, archived: boolean): ThreadListPage | null {
  if (!record(value) || !Array.isArray(value.rows) || value.rows.length > 50) return null;
  const rows: ThreadSummary[] = []; const seen = new Set<string>();
  for (const row of value.rows) {
    if (!record(row) || !threadId(row.id) || seen.has(row.id) || row.projectId !== scope
      || !['work', 'roundtable', 'companion'].includes(String(row.kind))
      || (row.title !== null && (!safeText(row.title, 256) || utf8.encode(row.title).length > 256))
      || !integer(row.revision) || row.revision === 0 || !integer(row.updatedAt)
      || (row.harness !== null && (!safeText(row.harness, 64) || !/^[a-z0-9_]+$/.test(row.harness)))
      || (row.nativeAvailable !== null && typeof row.nativeAvailable !== 'boolean')
      || !['unbound', 'inactive', 'recorded_open', 'archived'].includes(String(row.status))
      || (row.status === 'archived') !== archived) return null;
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
  const code = resumeFailureCode(error);
  if (code === 'project_missing') return 'The current project is unavailable. Choose it again.';
  if (code === 'busy') return 'Thread storage is busy. Reload explicitly.';
  if (code === 'source_changed' || code === 'conflict') return 'Thread records changed. Reload this project explicitly.';
  return 'Thread records could not be loaded. Reload explicitly.';
}
function recoveryFailureText(error: unknown) {
  const code = resumeFailureCode(error);
  if (code === 'session_alive') return 'This session is still open. Stop it in its terminal before resuming.';
  if (code === 'owner_alive') return 'The app instance that owns this session is still running. Close that instance before recovering it.';
  if (code === 'cleanup_unproven') return 'This session could not be recovered because its shutdown was not confirmed. Its saved conversation is preserved.';
  if (code === 'source_changed' || code === 'conflict') return 'This thread changed. Reload threads before recovering it.';
  if (code === 'busy') return 'Another thread operation is in progress. Try again when it finishes.';
  return 'The previous session could not be verified. Its saved conversation is preserved.';
}
function archiveFailureText(error: unknown, forgetting: boolean) {
  const code = resumeFailureCode(error);
  if (code === 'owned_elsewhere') return forgetting ? 'A session of this thread is open. Close it, then forget the thread.' : 'This conversation is open. Close it before archiving.';
  if (code === 'not_found') return 'This thread is already gone. Reload threads.';
  if (code === 'index_not_updated') return 'Thread forgotten, but the search index could not be updated; its text may still appear in search.';
  if (code === 'hop_in_progress') return 'A hop of this thread has not finished. Let it finish, then forget the thread.';
  if (code === 'source_changed' || code === 'conflict') return 'This thread changed. Reload threads and try again.';
  if (code === 'busy') return 'Another thread operation is in progress. Try again when it finishes.';
  return forgetting ? 'The thread could not be forgotten. It is still saved; try again.' : 'The thread could not be changed. Try again.';
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
  const showArchivedLabel = document.createElement('label'); showArchivedLabel.className = 'thread-sidebar-archived-toggle';
  const showArchived = document.createElement('input'); showArchived.type = 'checkbox'; showArchived.id = 'thread-sidebar-show-archived';
  showArchivedLabel.append(showArchived, ' Show archived threads'); showArchivedLabel.hidden = !options.archive;
  const status = document.createElement('p'); status.className = 'thread-sidebar-status'; status.setAttribute('role', 'status');
  const list = document.createElement('ul'); list.className = 'thread-sidebar-list'; list.setAttribute('aria-label', 'Saved threads');
  const pagination = document.createElement('div'); pagination.className = 'thread-sidebar-pagination';
  const previous = document.createElement('button'); previous.type = 'button'; previous.textContent = 'Previous';
  const pageLabel = document.createElement('span'); const next = document.createElement('button'); next.type = 'button'; next.textContent = 'Next';
  pagination.append(previous, pageLabel, next);
  const detail = document.createElement('dl'); detail.className = 'thread-sidebar-detail'; detail.setAttribute('aria-label', 'Selected thread metadata');
  const limitation = document.createElement('p'); limitation.className = 'thread-sidebar-limitation';
  limitation.textContent = 'Click an open thread to go to its tab, a closed one to resume it in a new tab.';
  const actions = document.createElement('div'); actions.className = 'thread-sidebar-actions';
  const recover = document.createElement('button'); recover.type = 'button'; recover.textContent = 'Recover session'; recover.hidden = true; actions.append(recover);
  const rename = document.createElement('button'); rename.type = 'button'; rename.textContent = 'Rename'; rename.hidden = true; actions.append(rename);
  const archive = document.createElement('button'); archive.type = 'button'; archive.hidden = true; actions.append(archive);
  const forgetLabel = 'Forget permanently…';
  const forget = document.createElement('button'); forget.type = 'button'; forget.textContent = forgetLabel; forget.hidden = true; actions.append(forget);
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
  body.append(scopeLabel, scopeSelect, create, reload, showArchivedLabel, status, list, pagination, detail, limitation, actions, renameForm);
  const separator = document.createElement('div'); separator.className = 'thread-sidebar-resize'; separator.tabIndex = 0;
  separator.setAttribute('role', 'separator'); separator.setAttribute('aria-label', 'Thread sidebar width'); separator.setAttribute('aria-orientation', 'vertical');
  separator.setAttribute('aria-valuemin', String(MIN_WIDTH)); separator.setAttribute('aria-valuemax', String(MAX_WIDTH));
  element.append(heading, body, separator); options.host.append(element);
  let display = { ...initialState }; let projects: CurrentThreadProject[] = []; let selectedProject: string | null = null;
  let page: ThreadListPage | null = null; let cursor: ThreadCursor | null = null; let history: (ThreadCursor | null)[] = [];
  let selected: string | null = null; let generation = 0; let uiGeneration = 0; let destroyed = false;
  let busy: 'load_state' | 'list' | 'save_state' | 'create' | 'resume' | 'recover' | 'rename' | 'archive' | 'forget' | null = null; let message = 'Choose a project, then load its saved threads.'; let failed = false;
  let editing: ThreadSummary | null = null; let renameNeedsReload = false;
  let archivedView = false; let forgetArmed = 0;
  let pendingState: ThreadSidebarState | null = null; let saveTimer = 0;
  let pendingProjectRead = false;
  let pendingChangedProject: string | null = null; let changeTimer = 0;
  let drag: { pointer: number; x: number; before: ThreadSidebarState } | null = null;
  const anchors = new Map<string, HTMLLIElement>();
  /** Names agents gave their conversations; a name the user gives (`title`) always wins. */
  const agentNames = new Map<string, string>(); const namedAt = new Map<string, number>(); let naming = false;
  const shownTitle = (row: ThreadSummary) => userTitle(row) || agentNames.get(row.id) || row.title || 'Untitled thread';
  // Each name is a contained session read that shares capacity with launches: ask rarely.
  const NAME_EVERY_MS = 60000;
  /** Asks once per thread, again after it changed, and once a minute while its tab runs (the agent renames as the work goes on). */
  function requestNames() {
    if (!options.names || !page || naming || destroyed || busy) return;
    const now = Date.now(); const project = selectedProject;
    const ids = page.rows.filter(row => !userTitle(row) && row.harness && row.status !== 'archived' && (row.status !== 'unbound' || options.isOpen?.(row) === true)).filter(row => {
      const at = namedAt.get(row.id); if (at === undefined || row.updatedAt > at) return true;
      return options.isOpen?.(row) === true && now - at >= NAME_EVERY_MS;
    }).map(row => row.id);
    if (!ids.length) return;
    naming = true; for (const id of ids) namedAt.set(id, now);
    void (async () => {
      try {
        for (let start = 0; start < ids.length; start += 16) {
          let value: unknown;
          try { value = await options.names!(ids.slice(start, start + 16)); }
          catch { for (const id of ids.slice(start)) namedAt.delete(id); return; } // asked again on the next tick
          if (destroyed || selectedProject !== project || !Array.isArray(value)) return;
          for (const item of value) {
            if (!record(item) || !threadId(item.threadId)) continue;
            if (typeof item.name === 'string' && safeText(item.name, 512) && item.name.trim()) agentNames.set(item.threadId, item.name.trim());
            else agentNames.delete(item.threadId);
          }
          render();
        }
      } finally { naming = false; }
    })();
  }
  const namesTimer = window.setInterval(() => { if (!display.collapsed) requestNames(); }, 15000);
  const renderedRows = new WeakMap<HTMLLIElement, ThreadSummary>();
  function metadata(label: string, value: string) {
    const term = document.createElement('dt'); term.textContent = label; const valueNode = document.createElement('dd'); valueNode.textContent = value; detail.append(term, valueNode);
  }
  /** What a click on the row does, said on the row itself so a click is never silent. */
  function rowState(row: ThreadSummary): { kind: 'open' | 'resumable' | 'recover' | 'blocked' | 'archived'; text: string } {
    if (row.status === 'archived') return { kind: 'archived', text: 'Archived' };
    if (options.isOpen?.(row) === true) return { kind: 'open', text: 'Open in a tab' };
    if (row.status === 'recorded_open') return { kind: 'recover', text: 'Marked open, no tab here · Recover session below' };
    if (row.status === 'unbound') return { kind: 'blocked', text: 'Nothing saved yet' };
    if (row.nativeAvailable === false) return { kind: 'blocked', text: 'Saved conversation not found' };
    if (row.nativeAvailable !== true) return { kind: 'blocked', text: 'Saved conversation not reachable now' };
    if (!options.onResume || options.canResume?.(row) !== true) return { kind: 'blocked', text: `${row.harness ?? 'This agent'} cannot reopen its sessions` };
    return { kind: 'resumable', text: 'Closed · click to resume' };
  }
  function render() {
    if (destroyed) return;
    element.classList.toggle('is-collapsed', display.collapsed); element.style.setProperty('--thread-sidebar-width', `${display.width}px`);
    body.hidden = display.collapsed; separator.hidden = display.collapsed; separator.setAttribute('aria-valuenow', String(display.width));
    collapse.textContent = display.collapsed ? '›' : '‹'; collapse.setAttribute('aria-label', display.collapsed ? 'Expand threads' : 'Collapse threads');
    collapse.setAttribute('aria-expanded', String(!display.collapsed));
    reload.disabled = busy !== null || selectedProject === null; showArchived.disabled = busy !== null || selectedProject === null; showArchived.checked = archivedView; scopeSelect.disabled = !projects.length || busy !== null;
    create.disabled = busy !== null || selectedProject === null;
    previous.disabled = busy !== null || !history.length; next.disabled = busy !== null || !page?.next;
    body.setAttribute('aria-busy', String(busy !== null));
    status.textContent = busy === 'list' ? 'Reading saved threads…' : busy === 'create' ? 'Saving a new thread…' : busy === 'resume' ? 'Resuming conversation…' : busy === 'recover' ? 'Recovering stopped session…' : busy === 'rename' ? 'Saving thread name…' : busy === 'archive' ? 'Saving archive state…' : busy === 'forget' ? 'Forgetting thread…' : busy === 'load_state' ? 'Restoring sidebar display…' : message;
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
        const state = document.createElement('span'); state.className = 'thread-sidebar-state';
        anchor.append(label, state, id, agent, updated, source); node.append(anchor); anchors.set(row.id, node);
        anchor.onclick = () => {
          if (busy) return;
          const actual = page?.rows.find(thread => thread.id === row.id); if (!actual || actual.projectId !== selectedProject) return;
          editing = null;
          selected = actual.id;
          // A closed thread opens on the click; any other thread shows its tab when it has one.
          const clicked = rowState(actual);
          if (clicked.kind === 'resumable') { render(); resumeThread(actual); return; }
          failed = false; message = clicked.kind === 'open' ? 'Showing its tab.' : clicked.text; render();
          try { options.onSelect?.(Object.freeze({ ...actual })); } catch { options.reportError('Thread selection could not be shown. No agent was started.'); }
          scheduleThreadRead();
        };
      }
      const anchor = node.firstElementChild as HTMLButtonElement;
      anchor.disabled = busy !== null;
      const name = shownTitle(row); const titleLine = anchor.querySelector<HTMLElement>('.thread-sidebar-title')!;
      if (titleLine.textContent !== name) { titleLine.textContent = name; anchor.setAttribute('aria-label', `${name} · ${row.id}`); }
      titleLine.classList.toggle('is-agent-name', !userTitle(row) && agentNames.has(row.id));
      const shown = rowState(row); const stateLine = anchor.querySelector<HTMLElement>('.thread-sidebar-state')!;
      if (stateLine.dataset.state !== shown.kind || stateLine.textContent !== shown.text) { stateLine.dataset.state = shown.kind; stateLine.textContent = shown.text; }
      const pressed = String(selected === row.id);
      if (anchor.getAttribute('aria-pressed') !== pressed) anchor.setAttribute('aria-pressed', pressed);
      if (renderedRows.get(node) !== row) {
        anchor.querySelector('.thread-sidebar-id')!.textContent = row.id;
        const agentLine = anchor.querySelector<HTMLElement>('.thread-sidebar-agent')!; agentLine.textContent = row.harness ?? 'Agent unknown'; agentLine.hidden = !row.harness;
        anchor.querySelector('.thread-sidebar-updated')!.textContent = updatedText(row.updatedAt);
        const sourceLine = anchor.querySelector<HTMLElement>('.thread-sidebar-source')!; sourceLine.textContent = `Source: ${sourceText(row.nativeAvailable)}`; sourceLine.hidden = true; // the state line already says whether the conversation can be reached
        renderedRows.set(node, row);
      }
      // Preserve actual DOM/focus when only width or selection changes.
      if (list.children[position] !== node) list.insertBefore(node, list.children[position] ?? null);
      position++;
    }
    pageLabel.textContent = page ? `${page.rows.length} on this page` : 'No page loaded';
    requestNames();
    detail.replaceChildren(); const chosen = page?.rows.find(row => row.id === selected);
    recover.hidden = !options.onRecover || !chosen || chosen.status !== 'recorded_open' || options.canRecover?.(chosen) !== true;
    recover.disabled = busy !== null || recover.hidden;
    rename.hidden = !options.rename || !chosen || editing !== null;
    rename.disabled = busy !== null || renameNeedsReload;
    archive.hidden = !options.archive || !chosen || editing !== null || chosen.status === 'recorded_open';
    archive.textContent = chosen?.status === 'archived' ? 'Unarchive' : 'Archive';
    archive.disabled = busy !== null || archive.hidden;
    forget.hidden = !options.forget || !chosen || editing !== null || chosen.status === 'recorded_open';
    forget.disabled = busy !== null || forget.hidden;
    if (forget.hidden && forgetArmed) disarmForget();
    renameForm.hidden = editing === null;
    renameInput.disabled = busy !== null || renameNeedsReload;
    renameSave.disabled = busy !== null || renameNeedsReload || !validTitle(renameInput.value);
    renameCancel.disabled = busy !== null;
    if (chosen) {
      metadata('Thread ID', chosen.id); metadata('Agent', chosen.harness ?? 'Unknown'); metadata('Title', userTitle(chosen) || (agentNames.has(chosen.id) ? `${agentNames.get(chosen.id)} (named by the agent)` : 'Unknown'));
      metadata('Updated', updatedText(chosen.updatedAt)); metadata('Source', sourceText(chosen.nativeAvailable));
      metadata('Native activity', 'Unknown');
      metadata('Saved status', { unbound: 'No native session linked', inactive: 'Recorded inactive', recorded_open: 'Recorded open; activity unknown', archived: 'Archived' }[chosen.status]);
    }
  }
  function disarmForget() { window.clearTimeout(forgetArmed); forgetArmed = 0; forget.textContent = forgetLabel; forget.classList.remove('danger'); }
  function listPage(project: string, requested: ThreadCursor | null) {
    return options.list({ projectId: project, cursor: requested, limit: 50, archived: archivedView }).then(value => pageValue(value, project, requested, archivedView));
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
      const result = await listPage(project, requested);
      if (!result) throw new Error('Invalid bounded thread projection');
      if (destroyed || at !== generation || selectedProject !== project) return false;
      if (direction === 'reload') history = [];
      else if (direction === 'next') { history.push(cursor); if (history.length > 64) history.shift(); }
      else history.pop();
      page = result; cursor = requested;
      selected = preserveSelection && result.rows.some(row => row.id === previousSelection) ? previousSelection : null;
      renameNeedsReload = false;
      message = selected ? 'Saved threads updated.' : page.rows.length ? 'Click a thread: an open one shows its tab, a closed one resumes.' : 'No saved threads in this project.';
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
  showArchived.onchange = () => {
    if (busy || destroyed || !selectedProject) { showArchived.checked = archivedView; return; }
    archivedView = showArchived.checked; page = null; cursor = null; history = []; selected = null; editing = null;
    void read(null, 'reload');
  };
  archive.onclick = () => {
    if (destroyed || busy || !options.archive || !selectedProject) return;
    const chosen = page?.rows.find(row => row.id === selected && row.projectId === selectedProject);
    if (!chosen || chosen.status === 'recorded_open') return;
    const archived = chosen.status !== 'archived'; const project = selectedProject; const at = generation;
    busy = 'archive'; failed = false; render();
    void (async () => {
      try {
        await options.archive!({ projectId: project, threadId: chosen.id, expectedRevision: chosen.revision, archived });
        if (destroyed || at !== generation || selectedProject !== project) return;
        // The thread leaves this view; the first page is re-read without it.
        cursor = null; history = []; selected = null;
        message = archived ? 'Thread archived. Turn on Show archived threads to find it.' : 'Thread restored to the thread list.';
        const received = await listPage(project, null);
        if (destroyed || at !== generation || selectedProject !== project) return;
        if (received) page = received; else { page = null; message += ' Reload threads to refresh the list.'; }
      } catch (error) {
        if (!destroyed && at === generation && selectedProject === project) { failed = true; message = archiveFailureText(error, false); options.reportError(message); }
      } finally { afterOperation(); }
    })();
  };
  forget.onblur = disarmForget;
  forget.onclick = () => {
    if (destroyed || busy || !options.forget || !selectedProject) return;
    const chosen = page?.rows.find(row => row.id === selected && row.projectId === selectedProject);
    if (!chosen || chosen.status === 'recorded_open') return;
    if (!forgetArmed) {
      forget.textContent = 'Click again: delete forever'; forget.classList.add('danger');
      message = `Forgetting deletes ${userTitle(chosen) || agentNames.get(chosen.id) || chosen.title || 'this thread'}, its companion, hops and hand-off files from Shinatama. The agent's own session files stay.`; render();
      forgetArmed = window.setTimeout(disarmForget, 5000); return;
    }
    disarmForget();
    const project = selectedProject; const at = generation;
    busy = 'forget'; failed = false; render();
    void (async () => {
      try {
        await options.forget!({ projectId: project, threadId: chosen.id, expectedRevision: chosen.revision });
        if (destroyed || at !== generation || selectedProject !== project) return;
        cursor = null; history = []; selected = null;
        message = 'Thread forgotten.';
        const received = await listPage(project, null);
        if (destroyed || at !== generation || selectedProject !== project) return;
        if (received) page = received; else { page = null; message = 'Thread forgotten. Reload threads to refresh the list.'; }
      } catch (error) {
        if (!destroyed && at === generation && selectedProject === project) {
          failed = true; message = archiveFailureText(error, true); options.reportError(message);
          // The thread is gone in this case; drop it from the shown page.
          const code = record(error) ? error.code : error;
          if (code === 'index_not_updated' && page) { page = { ...page, rows: page.rows.filter(row => row.id !== chosen.id) }; selected = null; }
        }
      } finally { afterOperation(); }
    })();
  };
  rename.onclick = () => {
    if (destroyed || busy || renameNeedsReload || !options.rename) return;
    const chosen = page?.rows.find(row => row.id === selected && row.projectId === selectedProject);
    if (!chosen) return;
    editing = { ...chosen }; renameInput.value = userTitle(chosen) ?? agentNames.get(chosen.id) ?? chosen.title ?? ''; renameError.hidden = true;
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
          const received = await listPage(before.projectId, null);
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
  /** Only a user's click on a closed row lands here; selection restoring and refreshes never do. */
  function resumeThread(clicked: ThreadSummary) {
    if (destroyed || busy || !options.onResume || !selectedProject) return;
    const chosen = page?.rows.find(row => row.id === clicked.id && row.projectId === selectedProject);
    if (!chosen || chosen.status === 'recorded_open' || chosen.status === 'archived') return;
    if (chosen.status === 'unbound' || chosen.nativeAvailable !== true) {
      failed = true; message = resumeFailureText({ code: chosen.status === 'unbound' ? 'unbound' : chosen.nativeAvailable === false ? 'native_missing' : 'native_unreachable' });
      render(); options.reportError(message); return;
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
          const received = await listPage(project, requested);
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
  }
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
        const received = await listPage(project, null);
        if (destroyed || at !== generation || selectedProject !== project) return;
        if (!received) throw new Error('Invalid thread page');
        page = received;
        if (!page.rows.some(row => row.id === selected)) selected = null;
        message = 'Session recovered. Click the thread to resume it.';
      } catch (error) {
        if (!destroyed && at === generation && selectedProject === project) {
          failed = !recovered;
          message = recovered ? 'Session recovered. Reload threads, then click the thread to resume it.' : recoveryFailureText(error);
          if (!recovered) options.reportError(message);
        }
      } finally {
        afterOperation();
        if (!destroyed && at === generation && selectedProject === project && recovered) {
          const row = selected ? anchors.get(selected)?.firstElementChild : null;
          if (row instanceof HTMLButtonElement) row.focus(); else reload.focus();
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
        const received=await listPage(project,null);
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
    destroy() { destroyed = true; generation++; window.clearInterval(namesTimer); window.clearTimeout(saveTimer); window.clearTimeout(forgetArmed); window.clearTimeout(changeTimer); pendingState = null; pendingChangedProject = null; cancelDrag(); anchors.clear(); element.remove(); },
  };
}
