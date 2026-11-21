import { uiPart, uiButton, uiInput } from '@acaprino/imago/ui';
import { splitter } from '@acaprino/imago/ui';
// ADR-0055: the sidebar lists threads. A thread is a set of sessions with its own layout; a click
// opens it, and the window then shows all and only its sessions. Nothing here starts an agent.
import type { CurrentThreadProject, ThreadSidebarState } from './thread-sidebar.ts';
import { closedSessionAction, sessionStateText, type ThreadSession } from './session-tabs.ts';

export type WorksetLead = { threadId: string; title: string | null; harness: string | null };
export type WorksetSummary = {
  id: string; projectId: string; title: string | null; archived: boolean; revision: number;
  usedAt: number; sessions: number; open: number; lead: WorksetLead | null;
};
/** Full list, a narrow rail of one square per thread, or no sidebar at all. */
export type SidebarMode = 'open' | 'compact' | 'hidden';
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
  /** Turns the thread on: its sessions join the window. It starts and stops nothing itself; a session that
   *  ran when Shinatama last closed resumes when its tab is shown. */
  onOpen: (thread: Readonly<WorksetSummary>) => Promise<boolean>;
  /** Turns the thread off, which closes it without deleting it: its tabs leave the window and its running
   *  sessions stop, after asking; they stay in the thread, not running. */
  onClose?: (thread: Readonly<WorksetSummary>) => Promise<boolean>;
  /** The sidebar was shown, made compact or closed: `mode` is what it is now. */
  onModeChange?: (mode: SidebarMode) => void;
  /** Returns keyboard focus to the external control that can reopen this sidebar. */
  restoreFocus?: () => void;
  /** Whether the thread is on. Without it, only `openId` is. */
  isOn?: (worksetId: string) => boolean;
  /** The thread the actions below the list apply to. */
  openId: () => string | null;
  /** The thread's colour, also drawn on its tabs. */
  tone?: (worksetId: string) => string | null;
  /** Sessions of this thread with a running terminal in this window. */
  running: (worksetId: string) => number;
  /** Every session of this thread, oldest first, closed ones included, named as its tab is: the thread's
   *  panel (⋯) lists them all. Whether one runs is asked again at each paint (`sessionRunning`). */
  sessions?: (worksetId: string) => Promise<readonly ThreadSession[]>;
  /** A terminal of this window runs the session now. */
  sessionRunning?: (threadId: string) => boolean;
  /** Brings the session's tab forward, turning its thread on first when it is off. */
  onShowSession?: (threadId: string) => Promise<void>;
  /** Gives the session a name of the user's; its tabs take it. */
  onRenameSession?: (threadId: string, title: string) => Promise<void>;
  /** Only the closed sessions, for a host without `sessions`: kept, not shown as tabs. */
  closed?: (worksetId: string) => Promise<readonly { id: string; name: string; updatedAt: number }[]>;
  /** Projects left the sidebar (×, or untick in the Project menu): their threads are turned off, which stops their running sessions after asking. */
  onProjectsRemoved?: (projectIds: readonly string[]) => void;
  /** The label of an agent, as the rest of the window shows it. */
  harnessLabel?: (harness: string) => string;
  /** Puts a closed session back among the tabs, and resumes it when its agent can (`ThreadSession.resumable`). */
  onRestore?: (threadId: string) => Promise<void>;
  /** Deletes one session for good: a closed one after a second click, one with nothing saved (Remove) at once. */
  onForgetSession?: (threadId: string) => Promise<void>;
  /** An agent of this thread is waiting for the user. */
  attention?: (worksetId: string) => boolean;
  /** ADR-0057: the sessions of a thread talk to each other through its Mukade room. */
  chat?: {
    enabled: (worksetId: string) => boolean;
    /** The room has events the user has not seen. */
    unread: (worksetId: string) => boolean;
    set: (thread: Readonly<WorksetSummary>, enabled: boolean) => Promise<void>;
    open: (thread: Readonly<WorksetSummary>) => void;
  };
  /** The thread's team map (the arrows between its agents), offered beside Chat on a thread whose sessions talk. */
  team?: { open: (thread: Readonly<WorksetSummary>) => void };
  names?: (threadIds: readonly string[]) => Promise<unknown>;
  loadState?: () => Promise<unknown>;
  saveState?: (state: Readonly<ThreadSidebarState>) => Promise<void>;
  reportError: (message: string) => void;
};
const AGENT_LABELS: Record<string, string> = { powershell: 'PowerShell', claude_code: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi', hermes: 'Hermes Agent', deepseek: 'DeepSeek Harness' };
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
  if (value.hidden !== undefined && typeof value.hidden !== 'boolean') return null;
  return { version: 1, width: value.width, collapsed: value.collapsed, activeProjects: [...listed], ...(value.hidden ? { hidden: true } : {}) };
}
function failureText(error: unknown, doing: string, what: 'thread' | 'session' = 'thread') {
  const code = typeof error === 'string' ? error : record(error) ? error.code ?? error.issue : null;
  if (code === 'project_missing') return 'The project is unavailable. Choose it again.';
  if (code === 'busy') return 'Thread storage is busy. Try again.';
  if (code === 'conflict') return `This ${what} changed. Reload and try again.`;
  if (code === 'owned_elsewhere') return 'A session of this thread is running. Stop it first.';
  // Nothing runs, but a session is still marked open by a Shinatama that closed badly, and its end could not be proven.
  if (code === 'open_unverified') return 'A session of this thread is still marked open by a Shinatama that is no longer running, and that could not be verified. The thread was kept.';
  return `The ${what} could not be ${doing}. Nothing was changed.`;
}
function agoText(timestamp: number) {
  const minutes = Math.floor((Date.now() - timestamp) / 60000);
  if (minutes < 1) return 'now'; if (minutes < 60) return `${minutes} min ago`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)} h ago`;
  const date = new Date(timestamp); return Number.isNaN(date.valueOf()) ? '' : date.toLocaleDateString();
}

export function createWorksetSidebar(options: WorksetSidebarOptions) {
  const element = uiPart('aside', undefined, 'thread-sidebar im-resizable-nav'); element.setAttribute('aria-label', 'Project threads');
  const heading = uiPart('header'); const collapse = uiButton('', { sm: true });
  collapse.setAttribute('aria-controls', 'thread-sidebar-body');
  const close = uiButton('×', { sm: true });
  close.title = 'Close the thread sidebar (the Threads button brings it back)'; close.setAttribute('aria-label', 'Close the thread sidebar');
  heading.append(uiPart('heading', 'Threads'), collapse, close);
  // Compact: one square per thread, its colour when on; a click turns it on or off.
  const rail = uiPart('list', undefined, 'im-nav-rail'); rail.setAttribute('aria-label', 'Threads');
  const body = uiPart('scroll', undefined, 'thread-sidebar-body im-stack'); body.id = 'thread-sidebar-body';
  const showArchivedLabel = uiPart('label'); const showArchived = uiInput('checkbox');
  showArchivedLabel.append(showArchived, ' Show archived threads'); showArchivedLabel.hidden = !options.update;
  const status = uiPart('paragraph', undefined, 'thread-sidebar-status'); status.setAttribute('role', 'status');
  const list = uiPart('list', undefined, 'thread-sidebar-list'); list.setAttribute('aria-label', 'Threads, by project');
  // The settings of one thread open in place, under its row (⋯): name, every session (open, then closed), archive, forget.
  const details = uiPart('item', undefined, 'thread-sidebar-details im-nav-detail im-stack');
  const renameForm = uiPart('form', undefined, 'thread-sidebar-rename');
  const renameLabel = uiPart('label', 'Name'); renameLabel.htmlFor = 'thread-rename-title';
  const renameInput = uiInput(); renameInput.id = 'thread-rename-title'; renameInput.type = 'text'; renameInput.maxLength = 256; renameInput.autocomplete = 'off';
  const renameSave = uiButton('Save'); renameSave.type = 'submit';
  renameForm.append(renameLabel, renameInput, renameSave);
  const actions = uiPart('actions');
  const archive = uiButton('');
  const removeLabel = 'Delete thread'; const remove = uiButton(removeLabel);
  actions.append(archive, remove);
  // Every session of the thread that is not closed, with its state and what can be done with it; the closed ones follow.
  // Classes of their own: the closed-session hooks (.thread-sidebar-closed*) keep meaning the closed ones only.
  const sessionsHead = uiPart('paragraph'); sessionsHead.id = 'thread-sidebar-sessions-head';
  const sessionsList = uiPart('list'); sessionsList.setAttribute('aria-labelledby', sessionsHead.id);
  // One rename field, moved into the row of the session being renamed.
  const sessionRename = uiPart('form');
  const sessionRenameInput = uiInput('text'); sessionRenameInput.maxLength = 256; sessionRenameInput.autocomplete = 'off';
  const sessionRenameSave = uiButton('Save'); sessionRenameSave.type = 'submit'; sessionRenameSave.dataset.key = 'rename-save';
  sessionRename.append(sessionRenameInput, sessionRenameSave);
  const closedHead = uiPart('paragraph', undefined, 'thread-sidebar-closed-head'); const closedList = uiPart('list', undefined, 'thread-sidebar-closed');
  const warning = uiPart('paragraph', undefined, 'im-text-err'); warning.hidden = true;
  // ADR-0057: the switch that lets the thread's sessions talk to each other.
  const chatLabel = uiPart('label'); const chatSwitch = uiInput('checkbox'); chatSwitch.id = 'thread-chat-switch';
  chatLabel.append(chatSwitch, ' Sessions talk to each other');
  const chatHint = uiPart('hint', 'Claude Code, Codex, OpenCode and Pi sessions opened from now on (Claude Code and Codex also when resumed) join the thread\'s Mukade room. Each reads the others at its turns and, after a turn, waits 5 minutes for a message to it. Chat opens the conversation.');
  chatHint.id = 'thread-chat-switch-hint'; chatSwitch.setAttribute('aria-describedby', chatHint.id);
  chatLabel.hidden = chatHint.hidden = !options.chat;
  details.id = 'thread-sidebar-details';
  details.append(renameForm, chatLabel, chatHint, sessionsHead, sessionsList, closedHead, closedList, actions, warning);
  body.append(status, list, showArchivedLabel);
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
  /** The mode last reported to `onModeChange`, also after the saved display is restored. */
  let shownMode: SidebarMode = 'open';
  /** The thread whose settings are open under its row, and its sessions as last read. */
  let expanded: string | null = null; let sessionsOf: { id: string; rows: readonly ThreadSession[] } | null = null;
  /** The session whose name is being edited in the thread's panel. */
  let renamingSession: string | null = null;
  const readSessions = options.sessions ?? (options.closed
    ? async (id: string): Promise<readonly ThreadSession[]> => (await options.closed!(id)).map(session => ({ ...session, state: 'closed' as const })) : undefined);

  const anchors = new Map<string, HTMLLIElement>();
  /** Names agents gave their conversations: they name a thread the user has not named. */
  const agentNames = new Map<string, string>(); const namedAt = new Map<string, number>(); let naming = false;
  /** The name a session gets from its agent before anyone names it; a shell session records no agent. */
  const defaultName = (harness: string | null) => harness ? options.harnessLabel?.(harness) ?? AGENT_LABELS[harness] ?? harness : AGENT_LABELS.powershell;
  const leadTitle = (lead: WorksetLead) => lead.title && lead.title !== defaultName(lead.harness) && lead.title !== AGENT_LABELS[lead.harness ?? ''] ? lead.title : null;
  const shownTitle = (row: WorksetSummary) => row.title || (row.lead && (leadTitle(row.lead) || agentNames.get(row.lead.threadId) || row.lead.title)) || 'New thread';
  /** The thread the settings under a row apply to: the one whose ⋯ is open. */
  const chosen = () => rows.find(row => row.id === expanded) ?? null;
  const isOn = (id: string) => options.isOn?.(id) ?? options.openId() === id;

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
  const namesTimer = window.setInterval(() => { if (!display.hidden) requestNames(); }, 15000);

  function render() {
    if (destroyed) return;
    element.hidden = !!display.hidden;
    element.classList.toggle('is-collapsed', display.collapsed); element.toggleAttribute('data-im-compact', display.collapsed);
    body.hidden = display.collapsed; rail.hidden = !display.collapsed; separator.hidden = display.collapsed;
    collapse.textContent = display.collapsed ? '›' : '‹'; collapse.setAttribute('aria-label', display.collapsed ? 'Expand threads' : 'Collapse threads');
    collapse.title = display.collapsed ? 'Show the full thread list' : 'Make the sidebar narrow: one square per thread';
    collapse.setAttribute('aria-expanded', String(!display.collapsed));
    if (display.collapsed && !display.hidden) renderRail();
    const now = display.hidden ? 'hidden' : display.collapsed ? 'compact' : 'open';
    if (now !== shownMode) { shownMode = now; options.onModeChange?.(now); }
    showArchived.disabled = busy || !active.length; showArchived.checked = archivedView;
    body.setAttribute('aria-busy', String(busy)); status.textContent = message; status.dataset.state = failed ? 'error' : 'ready'; status.classList.toggle('im-text-err', failed); status.hidden = !message;
    const retained = new Set(rows.map(row => row.id));
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
        let note = notes.get(project); if (!note) { note = uiPart('item'); notes.set(project, note); }
        note.textContent = group.failed ? 'Its threads could not be read.' : archivedView ? 'No archived threads.' : 'No threads yet.'; nodes.push(note);
      }
      for (const row of group.rows) { nodes.push(threadItem(row)); if (row.id === expanded) nodes.push(details); }
    }
    for (const child of [...list.children]) if (!nodes.includes(child as HTMLLIElement)) child.remove();
    // Moving the row that holds the rename field drops its focus: it is given back with the caret.
    const typing = document.activeElement === renameInput ? [renameInput.selectionStart, renameInput.selectionEnd] as const : null;
    nodes.forEach((item, position) => { if (list.children[position] !== item) list.insertBefore(item, list.children[position] ?? null); });
    if (typing && renameInput.isConnected && document.activeElement !== renameInput) { renameInput.focus({ preventScroll: true }); renameInput.setSelectionRange(typing[0], typing[1]); }
    const current = chosen();
    if (!current) { expanded = null; sessionsOf = null; editing = false; }
    const detailTone = current ? options.tone?.(current.id) : null; if (detailTone) details.style.setProperty('--im-item-color', detailTone); else details.style.removeProperty('--im-item-color');
    // The field holds a real name only; an unnamed thread shows what its row says as a hint.
    if (current && !editing && document.activeElement !== renameInput) {
      renameInput.value = current.title ?? '';
      renameInput.placeholder = shownTitle(current);
    }
    if (options.chat && current) { chatSwitch.checked = options.chat.enabled(current.id); chatSwitch.disabled = busy || current.archived; }
    renameForm.hidden = !options.update; archive.hidden = !options.update; remove.hidden = !options.remove;
    renameSave.disabled = archive.disabled = remove.disabled = busy;
    archive.textContent = current?.archived ? 'Unarchive' : 'Archive';
    archive.title = current?.archived ? 'Put this thread back in the list.' : 'Move this thread out of the list. Nothing is deleted.';
    if (remove.hidden && removeArmed) disarm();
    const loadedSessions = current && sessionsOf?.id === current.id ? sessionsOf.rows : null;
    const all = loadedSessions ?? [];
    const closed = all.filter(session => session.state === 'closed');
    // A host that gives only the closed sessions (`closed`) says nothing of the others.
    renderSessions(loadedSessions === null || !options.sessions ? null : all.filter(session => session.state !== 'closed'));
    closedHead.hidden = closedList.hidden = !closed.length; closedHead.textContent = `Closed sessions (${closed.length})`;
    if (!closed.some(session => session.id === forgetting)) forgetting = null;
    closedList.replaceChildren(...closed.map(session => {
      const item = uiPart('item'); const name = uiPart('text', session.name, 'thread-sidebar-closed-name'); name.title = `${session.name} · ${agoText(session.updatedAt)}`;
      const action = closedSessionAction(session);
      const restore = uiButton(action.label); restore.disabled = busy; restore.dataset.restore = session.id; restore.title = action.title;
      restore.onclick = () => { void operate('reopened', async () => { await options.onRestore?.(session.id); await loadSessions(); }); };
      const forget = uiButton(forgetting === session.id ? 'Sure?' : 'Delete', { danger: forgetting === session.id }); forget.disabled = busy;
      forget.title = 'Delete this session from Shinatama. The agent\'s own session files stay.';
      forget.onclick = () => {
        if (forgetting !== session.id) { forgetting = session.id; render(); return; }
        forgetting = null; void operate('deleted', async () => { await options.onForgetSession?.(session.id); await loadSessions(); }, 'session');
      };
      item.append(name, restore, forget); return item;
    }));
    requestNames();
  }
  /** The thread's sessions that are not closed (null: not read yet): name, state in words, and per state
   *  Show, Rename, and Remove for one with nothing saved. Rebuilt only when what it says changes, so a
   *  repaint never takes the focus from its buttons or its rename field. */
  let sessionsShown = '';
  function renderSessions(open: readonly ThreadSession[] | null) {
    const listed = (open ?? []).map(session => ({ ...session, state: options.sessionRunning?.(session.id) ? 'running' as const : session.state }));
    if (renamingSession && !listed.some(session => session.id === renamingSession)) renamingSession = null;
    const shown = JSON.stringify([open === null, busy, renamingSession, listed.map(session => [session.id, session.name, session.state])]);
    if (shown === sessionsShown) return;
    sessionsShown = shown;
    sessionsHead.hidden = open === null; sessionsList.hidden = !listed.length;
    sessionsHead.textContent = !listed.length ? 'No open sessions in this thread.' : `Sessions (${listed.length})`;
    const focused = document.activeElement instanceof HTMLElement && sessionsList.contains(document.activeElement) ? document.activeElement : null;
    const focusKey = focused?.dataset.key; const typing = focused === sessionRenameInput ? [sessionRenameInput.selectionStart, sessionRenameInput.selectionEnd] as const : null;
    sessionsList.replaceChildren(...listed.map(session => {
      const item = uiPart('item', undefined, 'im-inline-row'); item.dataset.threadId = session.id;
      const button = (label: string, key: string, title: string, act: () => void) => {
        const made = uiButton(label); made.dataset.key = `${key}:${session.id}`; made.disabled = busy;
        made.title = title; made.setAttribute('aria-label', `${label} ${session.name}`); made.onclick = act; return made;
      };
      if (renamingSession === session.id) {
        sessionRenameInput.setAttribute('aria-label', `New name of ${session.name}`);
        if (!typing) sessionRenameInput.placeholder = session.name;
        sessionRenameSave.disabled = busy; item.append(sessionRename); return item;
      }
      const words = uiPart('text', undefined, 'im-stack im-grow');
      words.append(uiPart('text', session.name), uiPart('text', sessionStateText(session.state), 'im-muted'));
      words.title = `${session.name} · ${sessionStateText(session.state)} · ${agoText(session.updatedAt)}`;
      item.append(words);
      if (options.onShowSession) item.append(button('Show', 'show', 'Show this session\'s tab, turning the thread on if it is off', () => {
        void options.onShowSession!(session.id).catch(() => options.reportError('That session could not be shown.')).finally(() => render());
      }));
      if (options.onRenameSession) item.append(button('Rename', 'rename', 'Give this session a name of your own', () => {
        renamingSession = session.id; sessionRenameInput.value = ''; render(); sessionRenameInput.focus();
      }));
      if (session.state === 'unbound' && options.onForgetSession) item.append(button('Remove', 'remove', 'Remove this session from the thread: nothing was saved in it. Nothing else is deleted.', () => {
        void operate('removed', async () => { await options.onForgetSession!(session.id); await loadSessions(); }, 'session');
      }));
      return item;
    }));
    if (typing && sessionRenameInput.isConnected) { sessionRenameInput.focus({ preventScroll: true }); sessionRenameInput.setSelectionRange(typing[0], typing[1]); }
    else if (focusKey) sessionsList.querySelector<HTMLElement>(`[data-key="${focusKey}"]`)?.focus({ preventScroll: true });
  }
  /** Leaves the session's rename field; the focus goes back to its Rename button. */
  function endSessionRename(id: string) {
    renamingSession = null; render();
    sessionsList.querySelector<HTMLButtonElement>(`[data-key="rename:${id}"]`)?.focus();
  }
  sessionRename.onsubmit = event => {
    event.preventDefault(); const id = renamingSession; const title = sessionRenameInput.value.trim();
    if (!id || busy || !options.onRenameSession || !title || !text(title, 256)) return;
    renamingSession = null;
    void operate('renamed', async () => { await options.onRenameSession!(id, title); await loadSessions(); }, 'session')
      .then(() => sessionsList.querySelector<HTMLButtonElement>(`[data-key="rename:${id}"]`)?.focus());
  };
  sessionRenameInput.onkeydown = event => {
    if (event.key !== 'Escape' || !renamingSession) return;
    event.preventDefault(); event.stopPropagation(); endSessionRename(renamingSession);
  };
  /** Two letters for a square: the first of the first two words, or the first two of one word. */
  const initials = (name: string) => {
    const words = name.match(/[\p{L}\p{N}]+/gu) ?? [];
    const [first = '', second] = words; const letters = second ? first.slice(0, 1) + second.slice(0, 1) : first.slice(0, 2);
    return (letters || '·').toUpperCase();
  };
  /** The compact rail: per listed project a line, then one square per thread with its initials. */
  function renderRail() {
    const items: HTMLElement[] = [];
    active.forEach((project, index) => {
      const group = groups.get(project); if (!group) return;
      if (index > 0) items.push(uiPart('item'));
      for (const row of group.rows) {
        const item = uiPart('item'); const square = uiButton('', { sm: true, cls: 'thread-sidebar-rail-item' });
        const name = shownTitle(row); const on = isOn(row.id); const tone = options.tone?.(row.id);
        square.textContent = initials(name);
        square.setAttribute('aria-pressed', String(on)); square.disabled = busy;
        square.setAttribute('aria-current', String(options.openId() === row.id));
        if (tone) square.style.setProperty('--im-item-color', tone);
        const running = options.running(row.id);
        square.title = `${name} · ${labelOf(project)}${running ? ` · ${running} running` : ''}\n${on ? 'Click to turn it off: its running sessions stop, after asking, and stay in it' : 'Click to show its tabs'}`;
        square.setAttribute('aria-label', `${name}, ${labelOf(project)}`);
        square.toggleAttribute('data-attention', options.attention?.(row.id) === true); square.toggleAttribute('data-im-badge', square.hasAttribute('data-attention'));
        square.onclick = () => { const actual = rows.find(candidate => candidate.id === row.id); if (actual) void openThread(actual); };
        item.append(square); items.push(item);
      }
    });
    rail.replaceChildren(...items);
  }
  function threadItem(row: WorksetSummary): HTMLLIElement {
    let item = anchors.get(row.id);
    if (!item) {
      item = uiPart('item', undefined, 'thread-sidebar-row im-nav-line'); const anchor = uiButton('', { sm: true, cls: 'thread-sidebar-anchor im-nav-entry' }); anchor.dataset.worksetId = row.id;
      // The square says whether the thread is shown: filled when its tabs are in the window.
      const mark = uiPart('text', undefined, 'thread-sidebar-switch im-nav-mark'); mark.setAttribute('aria-hidden', 'true');
      anchor.append(mark, uiPart('text', undefined, 'thread-sidebar-title'), uiPart('text', undefined, 'thread-sidebar-state'), uiPart('text', undefined, 'thread-sidebar-updated'));
      anchor.onclick = () => { const actual = rows.find(candidate => candidate.id === row.id); if (actual) void openThread(actual); };
      item.append(anchor);
      if (options.onAddSession) {
        const add = uiButton('+', { sm: true, cls: 'thread-sidebar-add-session' });
        add.onclick = () => { const actual = rows.find(candidate => candidate.id === row.id); if (actual && !busy) options.onAddSession!(Object.freeze({ ...actual }), add); };
        item.append(add);
      }
      if (options.chat) {
        const chat = uiButton('Chat', { sm: true, cls: 'thread-sidebar-chat' });
        chat.onclick = () => { const actual = rows.find(candidate => candidate.id === row.id); if (actual) options.chat!.open(Object.freeze({ ...actual })); };
        item.append(chat);
      }
      if (options.team) {
        const team = uiButton('Team', { sm: true, cls: 'thread-sidebar-team' });
        team.onclick = () => { const actual = rows.find(candidate => candidate.id === row.id); if (actual) options.team!.open(Object.freeze({ ...actual })); };
        item.append(team);
      }
      const more = uiButton('⋯', { sm: true, cls: 'thread-sidebar-more' }); more.setAttribute('aria-controls', 'thread-sidebar-details');
      more.onclick = () => { toggleDetails(row.id); };
      item.append(more);
      anchors.set(row.id, item);
    }
    const anchor = item.firstElementChild as HTMLButtonElement; anchor.disabled = busy;
    anchor.setAttribute('aria-current', String(options.openId() === row.id));
    const on = isOn(row.id);
    const name = shownTitle(row); const titleLine = anchor.querySelector<HTMLElement>('.thread-sidebar-title')!;
    if (titleLine.textContent !== name) titleLine.textContent = name;
    anchor.title = on ? `${name} is shown. Click to turn it off: its running sessions stop, after asking, and stay in the thread, not running.` : `Click to show the tabs of ${name}.`;
    const running = options.running(row.id); const waiting = options.attention?.(row.id) === true;
    const kind = row.archived ? 'archived' : waiting ? 'recover' : running ? 'open' : 'resumable';
    const count = Math.max(row.sessions, running);
    const sessions = `${count} session${count === 1 ? '' : 's'}`;
    const said = row.archived ? `Archived · ${sessions}` : waiting ? `${sessions} · waiting for you`
      : running ? (on ? `${sessions} · ${running} running` : `${running} running · tabs hidden`)
      : !count ? (on ? 'Empty · + adds a session' : 'Empty') : sessions;
    const stateLine = anchor.querySelector<HTMLElement>('.thread-sidebar-state')!;
    if (stateLine.dataset.state !== kind || stateLine.textContent !== said) { stateLine.dataset.state = kind; stateLine.textContent = said; }
    anchor.querySelector('.thread-sidebar-updated')!.textContent = agoText(row.usedAt);
    const pressed = String(on); if (anchor.getAttribute('aria-pressed') !== pressed) anchor.setAttribute('aria-pressed', pressed);
    const tone = options.tone?.(row.id); if (tone) item.style.setProperty('--im-item-color', tone); else item.style.removeProperty('--im-item-color');
    const add = item.querySelector<HTMLButtonElement>('.thread-sidebar-add-session');
    if (add) { add.disabled = busy; add.title = `Add a session to ${name}`; add.setAttribute('aria-label', add.title); }
    const chat = item.querySelector<HTMLButtonElement>('.thread-sidebar-chat');
    if (chat) {
      const talking = options.chat!.enabled(row.id) && !row.archived; const unread = talking && options.chat!.unread(row.id);
      chat.hidden = !talking; chat.toggleAttribute('data-unread', unread); chat.toggleAttribute('data-im-badge', unread);
      chat.title = `Conversation of ${name}${unread ? ' · new messages' : ''}`; chat.setAttribute('aria-label', chat.title);
    }
    const team = item.querySelector<HTMLButtonElement>('.thread-sidebar-team');
    if (team) {
      team.hidden = !(options.chat?.enabled(row.id) && !row.archived);
      team.title = `Team map of ${name}: who gives work to whom`; team.setAttribute('aria-label', team.title);
    }
    const more = item.querySelector<HTMLButtonElement>('.thread-sidebar-more')!;
    more.title = `Settings of ${name}`; more.setAttribute('aria-label', more.title); more.setAttribute('aria-expanded', String(expanded === row.id));
    return item;
  }
  function groupHead(project: string): HTMLLIElement {
    let head = heads.get(project); if (head) return head;
    head = uiPart('item', undefined, 'thread-sidebar-group im-nav-line'); head.dataset.projectId = project;
    const toggle = uiButton('', { sm: true, cls: 'thread-sidebar-group-toggle' });
    toggle.append(uiPart('text', undefined, 'thread-sidebar-group-name'), uiPart('text', undefined, 'thread-sidebar-group-count'));
    toggle.onclick = () => { if (!collapsedGroups.delete(project)) collapsedGroups.add(project); render(); };
    const add = uiButton('+', { sm: true, cls: 'thread-sidebar-group-add' }); add.hidden = !options.create; add.onclick = () => createIn(project);
    const hide = uiButton('×', { sm: true, cls: 'thread-sidebar-group-remove' }); hide.onclick = () => { if (!busy) setActive(active.filter(id => id !== project)); };
    head.append(toggle, add, hide); heads.set(project, head); return head;
  }
  function disarm() { window.clearTimeout(removeArmed); removeArmed = 0; remove.textContent = removeLabel; remove.classList.remove('danger', 'im-btn--danger'); warning.hidden = true; }
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
      if (expanded) void loadSessions();
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
  async function operate(doing: string, work: () => Promise<void>, what: 'thread' | 'session' = 'thread') {
    if (busy || destroyed) return;
    const at = generation; busy = true; failed = false; render();
    try { await work(); }
    catch (error) { if (!destroyed && at === generation) { failed = true; message = failureText(error, doing, what); options.reportError(message); } }
    finally { busy = false; if (!destroyed && at === generation) { rereadWanted = false; await read(); } else render(); }
  }
  /** A click turns a thread on, or off when it is on. */
  async function openThread(row: WorksetSummary) {
    const on = isOn(row.id);
    if (busy || destroyed || (on && !options.onClose)) return;
    busy = true; render();
    const refused = on ? 'The thread could not be turned off.' : 'The thread could not be opened. The window stays as it is.';
    try { if (await (on ? options.onClose! : options.onOpen)(Object.freeze({ ...row }))) { failed = false; message = ''; } else { failed = true; message = refused; } }
    catch { failed = true; message = refused; }
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
  /** Opens the settings of a thread under its row, or closes them; one thread at a time. */
  function toggleDetails(id: string) {
    disarm(); editing = false; forgetting = null; renamingSession = null;
    expanded = expanded === id ? null : id; sessionsOf = null; render();
    if (expanded) void loadSessions();
  }
  /** Reads the sessions of the thread whose settings are open; only the latest reply is shown. */
  let sessionsAsk = 0;
  async function loadSessions() {
    const id = expanded; if (!id || !readSessions) return;
    const ask = ++sessionsAsk;
    try { const found = await readSessions(id); if (expanded === id && ask === sessionsAsk) { sessionsOf = { id, rows: found }; render(); } } catch { /* the list stays as it was */ }
  }
  renameInput.oninput = () => { editing = true; };
  renameForm.onsubmit = event => {
    event.preventDefault(); const current = chosen(); const title = renameInput.value.trim();
    // While the list is being read the name waits in the field: Enter again once Save is enabled.
    if (busy || !current || !options.update || !title || !text(title, 256)) return;
    // Enter saves and folds the settings away, like Escape folds them without saving.
    editing = false; expanded = null;
    void operate('renamed', async () => { await options.update!({ worksetId: current.id, expectedRevision: current.revision, title }); });
  };
  renameInput.onkeydown = event => {
    if (event.key !== 'Escape') return;
    event.preventDefault(); const id = expanded; editing = false; expanded = null; render();
    if (id) anchors.get(id)?.querySelector<HTMLButtonElement>('.thread-sidebar-more')?.focus();
  };
  chatSwitch.onchange = () => {
    const current = chosen(); const wanted = chatSwitch.checked;
    if (!current || !options.chat || busy) { if (current && options.chat) chatSwitch.checked = options.chat.enabled(current.id); return; }
    void operate('changed', async () => { await options.chat!.set(Object.freeze({ ...current }), wanted); });
  };
  archive.onclick = () => {
    const current = chosen(); if (!current || !options.update) return;
    void operate('changed', async () => { await options.update!({ worksetId: current.id, expectedRevision: current.revision, archived: !current.archived }); });
  };
  remove.onclick = () => {
    const current = chosen(); if (!current || !options.remove || busy) return;
    if (!removeArmed) {
      remove.textContent = 'Click again to delete'; remove.classList.add('danger', 'im-btn--danger'); removeArmed = window.setTimeout(disarm, 5000);
      warning.textContent = `Deletes this thread and its ${current.sessions} session${current.sessions === 1 ? '' : 's'} from Shinatama, closed ones included. The agents' own files stay.`;
      warning.hidden = false; return;
    }
    disarm();
    void operate('deleted', async () => { await options.remove!(Object.freeze({ ...current })); expanded = null; });
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
    const leaving = active.filter(id => !unique.includes(id));
    generation++; active = unique; editing = false; failed = false; message = active.length ? '' : NO_PROJECTS;
    // Its threads go off with it: no tab is left without a row to turn it off or on again.
    if (leaving.length) options.onProjectsRemoved?.(leaving);
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
  const resize = splitter(document, {
    label: 'Thread sidebar width', controls: body.id, min: MIN_WIDTH, max: MAX_WIDTH,
    value: display.width, step: WIDTH_STEP,
    onResize(width) {
      display.width = width;
      element.style.setProperty('--thread-sidebar-width', width + 'px');
      element.style.setProperty('--im-nav-width', width + 'px');
    },
    onCommit: changedDisplay,
  });
  const separator = resize.element;
  separator.classList.add('thread-sidebar-resize');
  element.append(heading, rail, body, separator); options.host.append(element);
  collapse.onclick = () => { resize.cancel(); display.collapsed = !display.collapsed; changedDisplay(); };
  close.onclick = () => { resize.cancel(); setMode('hidden'); options.restoreFocus?.(); };
  const mode = (): SidebarMode => display.hidden ? 'hidden' : display.collapsed ? 'compact' : 'open';
  /** Shows the sidebar full or compact, or closes it; saved with the rest of its display. */
  function setMode(next: SidebarMode) {
    if (next === mode()) return;
    display = { ...display, hidden: next === 'hidden' ? true : undefined, collapsed: next === 'hidden' ? display.collapsed : next === 'compact' };
    if (!display.hidden) delete display.hidden;
    changedDisplay();
  }

  render();
  if (options.loadState) void options.loadState().then(value => {
    if (destroyed || value === null) return;
    const restored = stateValue(value); if (!restored) return;
    if (!resize.dragging) { display = { ...restored }; resize.setValue(restored.width); }
    const saved = restored.activeProjects ?? [];
    setActive([...saved, ...active.filter(id => !saved.includes(id))], false);
  }).catch(() => { if (!destroyed) options.reportError('Sidebar display could not be restored. Default dimensions are in use.'); })
    .finally(() => { loaded = true; if (saveWanted && !destroyed) { saveWanted = false; changedDisplay(); } });
  return {
    element, setProject, setProjects, currentProjectId: (): string | null => currentProject,
    activeIds: (): readonly string[] => [...active],
    toggleActive(id: string) { if (!destroyed) setActive(active.includes(id) ? active.filter(other => other !== id) : [...active, id]); },
    mode, setMode,
    /** Whether the sidebar comes back compact when it is shown again. */
    compactWhenShown: (): boolean => display.collapsed,
    /** Repaints counts and the open mark; reads nothing. */
    repaint: render,
    /** A saved change somewhere: read the list again, once, shortly. */
    changed() { if (destroyed || !active.length) return; window.clearTimeout(changeTimer); changeTimer = window.setTimeout(() => { void read(); }, 400); },
    refresh: read,
    row: (id: string): Readonly<WorksetSummary> | null => rows.find(row => row.id === id) ?? null,
    /** The thread's name, given by the user or its agent; null while it has none (the row then shows its agent). */
    title: (id: string): string | null => {
      const row = rows.find(candidate => candidate.id === id); if (!row) return null;
      return row.title || (row.lead && (leadTitle(row.lead) || agentNames.get(row.lead.threadId))) || null;
    },
    destroy() { destroyed = true; generation++; window.clearInterval(namesTimer); window.clearTimeout(saveTimer); window.clearTimeout(removeArmed); window.clearTimeout(changeTimer); resize.cancel(); anchors.clear(); element.remove(); },
  };
}
