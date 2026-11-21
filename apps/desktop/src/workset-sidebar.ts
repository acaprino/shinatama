import {
  avatarHtml, buttonHtml, checkboxOptionHtml, claimKey, componentElement, confirm, iconButtonHtml, iconHtml, openMenu, roving,
  sidebarHtml, sidebarResize, textFieldHtml, uiButton, uiInput, uiPart, uiTone, uiTraits, type MenuItem,
} from '@acaprino/imago/ui';
// ADR-0055: the sidebar lists threads. A thread is a set of sessions with its own layout; a click
// opens it, and the window then shows all and only its sessions. Nothing here starts an agent.
// The region is Imago's sidebar (sidebarHtml): its rows are im-row options of one listbox, grouped under a
// row per project; the rail, the drawer under 620 and the fold are the package's (mountApp runs sidebarCollapse
// with collapsed() and setCollapsed), the width is sidebarResize's.
import type { CurrentThreadProject, ThreadSidebarState } from './thread-sidebar.ts';
import { closedSessionAction, sessionStateText, type ThreadSession } from './session-tabs.ts';
import { confirmsField } from './desktop-shell-model.ts';

export type WorksetLead = { threadId: string; title: string | null; harness: string | null };
export type WorksetSummary = {
  id: string; projectId: string; title: string | null; archived: boolean; revision: number;
  usedAt: number; sessions: number; open: number; lead: WorksetLead | null;
};
/** Full list, the rail (one avatar per thread), or no sidebar at all. */
export type SidebarMode = 'open' | 'compact' | 'hidden';
export type WorksetUpdate = { worksetId: string; expectedRevision: number; title?: string; archived?: boolean };
export type WorksetSidebarOptions = {
  /** The `.im-shell` the sidebar is placed in (the host puts `element` in its `.im-body`). */
  shell: HTMLElement;
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
  /** The thread's colour while it is on, also drawn on its tabs: a category of the package (`'3'`,
   *  `'var(--im-cat-3)'`), or any other value, which takes a category of its own (1 to 8) the first time it is seen. */
  tone?: (worksetId: string) => string | null;
  /** Sessions of this thread with a running terminal in this window. */
  running: (worksetId: string) => number;
  /** Every session of this thread, oldest first, closed ones included, named as its tab is: the thread's
   *  settings list them all. Whether one runs is asked again at each paint (`sessionRunning`). */
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
  /** Settles once the projects' threads are off, or kept on after a cancelled confirmation. */
  onProjectsRemoved?: (projectIds: readonly string[]) => void | Promise<void>;
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
/** The width the state stores is 180 to 480; sidebarResize moves it from 168 to 420, so it is kept to 180 to 420. */
const MIN_WIDTH = 180; const MAX_WIDTH = 480; const SHOWN_MAX_WIDTH = 420;
/** Projects the sidebar lists at once; matches the stored limit. */
const MAX_ACTIVE = 32;
const NO_PROJECTS = 'No project in the sidebar. Tick projects in the Project menu to see their threads.';
/** What a row's menu (right click, the Menu key, Shift+F10) calls each of the row's buttons. */
const MENU_LABELS: Record<string, string> = {
  'add-session': 'Add a session', chat: 'Chat', team: 'Team map', more: 'Thread settings',
  'new-thread': 'New thread', 'remove-project': 'Remove from the sidebar',
};
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, cap: number): value is string => typeof value === 'string' && value.length <= cap && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
const worksetId = (value: unknown): value is string => typeof value === 'string' && /^w_[a-z2-7]{12}$/.test(value);
const threadId = (value: unknown): value is string => typeof value === 'string' && /^t_[a-z2-7]{12}$/.test(value);
const projectId = (value: unknown): value is string => typeof value === 'string' && /^p_[a-z2-7]{12}$/.test(value);
const shownWidth = (width: number) => Math.min(SHOWN_MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(width)));
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
/** Two letters for an avatar: the first of the first two words, or the first two of one word. */
function initials(name: string) {
  const words = name.match(/[\p{L}\p{N}]+/gu) ?? [];
  const [first = '', second] = words; const letters = second ? first.slice(0, 1) + second.slice(0, 1) : first.slice(0, 2);
  return (letters || '·').toUpperCase();
}
/** Writes an attribute only when it changes: the list's roving tab stop watches some of them. */
function attr(element: Element, name: string, value: string | null) {
  if (value === null) { if (element.hasAttribute(name)) element.removeAttribute(name); }
  else if (element.getAttribute(name) !== value) element.setAttribute(name, value);
}
const flag = (element: Element, name: string, on: boolean) => attr(element, name, on ? '' : null);
const setText = (element: Element, value: string) => { if (element.textContent !== value) element.textContent = value; };

export function createWorksetSidebar(options: WorksetSidebarOptions) {
  const { shell } = options;
  // The canon's sidebar region: the title, its count, the fold button, the list of rows and the foot.
  const element = componentElement<HTMLElement>(sidebarHtml({
    id: 'thread-sidebar', label: 'Threads', rowsHtml: '', count: 0, footHtml: '',
    collapse: { collapse: 'Collapse threads', expand: 'Expand threads' },
  }));
  element.classList.add('thread-sidebar');
  const fold = element.querySelector<HTMLButtonElement>('.im-sidebar-collapse')!;
  const count = element.querySelector<HTMLElement>('.im-panel-header .im-count')!;
  const close = componentElement<HTMLButtonElement>(iconButtonHtml('Close the thread sidebar', 'close-sidebar', 'close'));
  close.title = 'Close the thread sidebar (the Threads button brings it back)';
  fold.before(close);
  // The same listbox as a list element: its rows are li options (the window's smokes find them so).
  const canonList = element.querySelector<HTMLElement>('.im-list')!;
  const list = uiTraits(uiPart('orderedList', undefined, 'im-panel-body thread-sidebar-list thread-sidebar-body'), 'list');
  list.setAttribute('role', canonList.getAttribute('role') ?? 'listbox');
  list.id = 'thread-sidebar-body'; list.setAttribute('aria-label', 'Threads, by project'); list.setAttribute('aria-multiselectable', 'true');
  canonList.replaceWith(list);
  const foot = element.querySelector<HTMLElement>('.im-panel-foot')!;
  const status = uiPart('status', undefined, 'thread-sidebar-status');
  const showArchivedLabel = componentElement<HTMLLabelElement>(checkboxOptionHtml({ name: 'show-archived', checked: false, label: 'Show archived threads' }));
  const showArchived = showArchivedLabel.querySelector('input')!;
  showArchivedLabel.hidden = !options.update;
  const footStack = uiTraits(uiPart('stack'), 'grow'); footStack.append(status, showArchivedLabel); foot.append(footStack);
  // The settings of one thread (⋯), under the list and outside it, hidden in the rail: its name, every session (open,
  // then closed), the chat switch, archive, delete.
  const details = uiTraits(uiPart('stack', undefined, 'im-inset thread-sidebar-details'), 'scroll', 'bounded');
  details.id = 'thread-sidebar-details'; details.setAttribute('role', 'group'); details.setAttribute('data-im-rail-detail', ''); details.hidden = true;
  const detailsTitle = uiPart('strong', undefined, 'thread-sidebar-details-title');
  // Enter in the name saves it, Escape folds the settings away.
  const renameForm = uiPart('stack', undefined, 'thread-sidebar-rename');
  const renameField = componentElement(textFieldHtml('Name', 'thread-title', '', { id: 'thread-rename-title', error: false, attrs: { maxlength: '256', autocomplete: 'off' } }));
  const renameInput = renameField.querySelector('input')!;
  const renameSave = uiButton('Save', { sm: true });
  const renameActions = uiPart('actions'); renameActions.append(renameSave); renameForm.append(renameField, renameActions);
  // ADR-0057: the switch that lets the thread's sessions talk to each other; it acts at once.
  const chatLabel = componentElement<HTMLLabelElement>(checkboxOptionHtml({ name: 'thread-chat', checked: false, label: 'Sessions talk to each other' }));
  const chatSwitch = chatLabel.querySelector('input')!;
  chatSwitch.className = 'im-switch'; chatSwitch.setAttribute('role', 'switch'); chatSwitch.id = 'thread-chat-switch';
  const chatHint = uiPart('hint', 'Claude Code, Codex, OpenCode and Pi sessions opened from now on (Claude Code and Codex also when resumed) join the thread\'s Mukade room. Each reads the others at its turns and, after a turn, waits 5 minutes for a message to it. Chat opens the conversation.');
  chatHint.id = 'thread-chat-switch-hint'; chatSwitch.setAttribute('aria-describedby', chatHint.id);
  chatLabel.hidden = chatHint.hidden = !options.chat;
  // Every session of the thread that is not closed, with its state and what can be done with it; the closed ones follow.
  // Classes of their own: the closed-session hooks (.thread-sidebar-closed*) keep meaning the closed ones only.
  const sessionsHead = uiPart('paragraph'); sessionsHead.id = 'thread-sidebar-sessions-head';
  const sessionsList = uiPart('list'); sessionsList.setAttribute('aria-labelledby', sessionsHead.id);
  const closedHead = uiPart('paragraph', undefined, 'thread-sidebar-closed-head'); const closedList = uiPart('list', undefined, 'thread-sidebar-closed');
  const archive = uiButton('Archive', { sm: true }); const archiveLabel = archive.querySelector('span')!;
  const remove = uiButton('Delete thread', { sm: true, danger: true });
  const actions = uiPart('actions'); actions.append(archive, remove);
  details.append(detailsTitle, renameForm, chatLabel, chatHint, sessionsHead, sessionsList, closedHead, closedList, actions);
  foot.before(details);
  // One rename field, moved into the row of the session being renamed: Enter saves, Escape leaves it.
  const sessionRename = uiTraits(uiPart('row'), 'grow');
  const sessionRenameInput = uiTraits(uiInput('text'), 'grow'); sessionRenameInput.maxLength = 256; sessionRenameInput.autocomplete = 'off';
  const sessionRenameSave = uiButton('Save', { sm: true }); sessionRenameSave.dataset.key = 'rename-save';
  sessionRename.append(sessionRenameInput, sessionRenameSave);

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
  let editing = false; let saveTimer = 0; let changeTimer = 0;
  /** The mode last reported to `onModeChange`, also after the saved display is restored. */
  let shownMode: SidebarMode = 'open';
  /** The thread whose settings are open, and its sessions as last read. */
  let expanded: string | null = null; let sessionsOf: { id: string; rows: readonly ThreadSession[] } | null = null;
  /** The session whose name is being edited in the thread's settings. */
  let renamingSession: string | null = null;
  const readSessions = options.sessions ?? (options.closed
    ? async (id: string): Promise<readonly ThreadSession[]> => (await options.closed!(id)).map(session => ({ ...session, state: 'closed' as const })) : undefined);

  /** The row of each thread, kept while the thread is listed, and the thread of each row. */
  const anchors = new Map<string, HTMLLIElement>(); const threadOfRow = new WeakMap<Element, string>();
  /** Names agents gave their conversations: they name a thread the user has not named. */
  const agentNames = new Map<string, string>(); const namedAt = new Map<string, number>(); let naming = false;
  /** The name a session gets from its agent before anyone names it; a shell session records no agent. */
  const defaultName = (harness: string | null) => harness ? options.harnessLabel?.(harness) ?? AGENT_LABELS[harness] ?? harness : AGENT_LABELS.powershell;
  const leadTitle = (lead: WorksetLead) => lead.title && lead.title !== defaultName(lead.harness) && lead.title !== AGENT_LABELS[lead.harness ?? ''] ? lead.title : null;
  const shownTitle = (row: WorksetSummary) => row.title || (row.lead && (leadTitle(row.lead) || agentNames.get(row.lead.threadId) || row.lead.title)) || 'New thread';
  /** The thread the settings apply to: the one whose ⋯ is open. */
  const chosen = () => rows.find(row => row.id === expanded) ?? null;
  const isOn = (id: string) => options.isOn?.(id) ?? options.openId() === id;
  /** A thread's colour as a category of the package, 1 to 8, while it is on; a tone that is not one takes the next
   *  category the first time it is seen, and keeps it. */
  const categories = new Map<string, number>();
  function categoryOf(id: string): string | null {
    const tone = options.tone?.(id)?.trim(); if (!tone) return null;
    const named = /^(?:var\(\s*--im-cat-)?([1-8])\s*\)?$/.exec(tone); if (named) return named[1];
    let category = categories.get(tone);
    if (category === undefined) { category = categories.size % 8 + 1; categories.set(tone, category); }
    return String(category);
  }

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

  const mode = (): SidebarMode => display.hidden ? 'hidden' : display.collapsed ? 'compact' : 'open';
  /** Hidden is no sidebar in the shell (`data-im-sidebar="off"`); under 620 the drawer still opens it. */
  function applyDisplay() {
    attr(shell, 'data-im-sidebar', display.hidden ? 'off' : 'on');
    const hide = !!display.hidden && shell.dataset.imSidebarOpen !== 'true';
    if (element.hidden !== hide) element.hidden = hide;
    element.classList.toggle('is-collapsed', display.collapsed);
    // The width the state holds, for the window's smokes; the shell's own is --im-w-sidebar, which sidebarResize sets.
    const width = `${display.width}px`;
    if (element.style.getPropertyValue('--thread-sidebar-width') !== width) element.style.setProperty('--thread-sidebar-width', width);
  }
  /** The rail is sidebarCollapse's: a fold the display wants and the shell does not show goes through its button. */
  function syncFold() {
    if ((shell.dataset.imSidebarCollapsed === 'true') !== display.collapsed) fold.click();
  }

  function render() {
    if (destroyed) return;
    applyDisplay();
    const now = mode();
    if (now !== shownMode) { shownMode = now; options.onModeChange?.(now); }
    showArchived.disabled = busy || !active.length; showArchived.checked = archivedView;
    attr(list, 'aria-busy', String(busy));
    setText(status, message); status.dataset.state = failed ? 'error' : 'ready'; uiTone(status, failed ? 'err' : null); status.hidden = !message;
    foot.hidden = status.hidden && showArchivedLabel.hidden;
    setText(count, String(rows.length));
    const retained = new Set(rows.map(row => row.id));
    for (const [id, item] of anchors) if (!retained.has(id)) { item.remove(); anchors.delete(id); }
    const nodes: HTMLLIElement[] = [];
    for (const project of active) {
      const group = groups.get(project); const closed = collapsedGroups.has(project);
      nodes.push(groupHead(project, group, closed));
      if (closed || !group) continue;
      if (!group.rows.length) {
        let note = notes.get(project);
        if (!note) {
          // A line in the list, not a thread: an option that cannot be chosen, and not in the rail.
          note = rowPart('im-row--1 thread-sidebar-note'); note.setAttribute('aria-disabled', 'true'); note.setAttribute('data-im-rail-detail', '');
          const line = uiPart('text', undefined, 'im-row-text'); line.append(uiPart('text', undefined, 'im-row-sub')); note.append(line);
          notes.set(project, note);
        }
        setText(note.querySelector('.im-row-sub')!, group.failed ? 'Its threads could not be read.' : archivedView ? 'No archived threads.' : 'No threads yet.');
        nodes.push(note);
      }
      for (const row of group.rows) nodes.push(threadItem(row));
    }
    // A row that moves keeps the focus it had.
    const focused = document.activeElement instanceof HTMLElement && list.contains(document.activeElement) ? document.activeElement : null;
    for (const child of [...list.children]) if (!nodes.includes(child as HTMLLIElement)) child.remove();
    nodes.forEach((item, position) => { if (list.children[position] !== item) list.insertBefore(item, list.children[position] ?? null); });
    if (focused?.isConnected && document.activeElement !== focused) focused.focus({ preventScroll: true });
    const current = chosen();
    if (!current) { expanded = null; sessionsOf = null; editing = false; }
    if (details.hidden !== !current) details.hidden = !current;
    if (current) { setText(detailsTitle, `Settings of ${shownTitle(current)}`); attr(details, 'aria-label', `Settings of ${shownTitle(current)}`); }
    // The field holds a real name only; an unnamed thread shows what its row says as a hint.
    if (current && !editing && document.activeElement !== renameInput) {
      renameInput.value = current.title ?? '';
      renameInput.placeholder = shownTitle(current);
    }
    if (options.chat && current) { chatSwitch.checked = options.chat.enabled(current.id); chatSwitch.disabled = busy || current.archived; }
    renameForm.hidden = !options.update; archive.hidden = !options.update; remove.hidden = !options.remove;
    renameSave.disabled = archive.disabled = remove.disabled = busy;
    setText(archiveLabel, current?.archived ? 'Unarchive' : 'Archive');
    archive.title = current?.archived ? 'Put this thread back in the list.' : 'Move this thread out of the list. Nothing is deleted.';
    remove.title = 'Delete this thread and its sessions from Shinatama, after asking.';
    const loadedSessions = current && sessionsOf?.id === current.id ? sessionsOf.rows : null;
    const all = loadedSessions ?? [];
    const closedSessions = all.filter(session => session.state === 'closed');
    // A host that gives only the closed sessions (`closed`) says nothing of the others.
    renderSessions(loadedSessions === null || !options.sessions ? null : all.filter(session => session.state !== 'closed'));
    closedHead.hidden = closedList.hidden = !closedSessions.length; closedHead.textContent = `Closed sessions (${closedSessions.length})`;
    closedList.replaceChildren(...closedSessions.map(session => {
      const item = uiPart('item'); const name = uiTraits(uiPart('text', session.name, 'thread-sidebar-closed-name'), 'grow');
      name.title = `${session.name} · ${agoText(session.updatedAt)}`; item.append(name);
      const action = closedSessionAction(session);
      const restore = uiButton(action.label, { sm: true }); restore.disabled = busy; restore.dataset.restore = session.id; restore.title = action.title;
      restore.onclick = () => { void operate('reopened', async () => { await options.onRestore?.(session.id); await loadSessions(); }); };
      // Deleting a closed session cannot be undone: a dialog names it, and the focus starts on Cancel.
      const forget = uiButton('Delete', { sm: true, danger: true }); forget.disabled = busy;
      forget.title = 'Delete this session from Shinatama. The agent\'s own session files stay.';
      forget.onclick = () => {
        if (busy) return;
        void confirm(shell, {
          title: `Delete the session ${session.name}?`,
          body: 'It is deleted from Shinatama with its place in this thread. The agent\'s own session files stay. This cannot be undone.',
          confirm: 'Delete session', cancel: 'Cancel', danger: true,
        }).then(yes => {
          if (!yes || destroyed || busy) return;
          void operate('deleted', async () => { await options.onForgetSession?.(session.id); await loadSessions(); }, 'session');
        });
      };
      item.append(restore, forget); return item;
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
    const focusedControl = document.activeElement instanceof HTMLElement && sessionsList.contains(document.activeElement) ? document.activeElement : null;
    const focusKey = focusedControl?.dataset.key; const typing = focusedControl === sessionRenameInput ? [sessionRenameInput.selectionStart, sessionRenameInput.selectionEnd] as const : null;
    sessionsList.replaceChildren(...listed.map(session => {
      const item = uiTraits(uiPart('item'), 'row'); item.dataset.threadId = session.id;
      const button = (label: string, key: string, title: string, act: () => void) => {
        const made = uiButton(label, { sm: true }); made.dataset.key = `${key}:${session.id}`; made.disabled = busy;
        made.title = title; made.setAttribute('aria-label', `${label} ${session.name}`); made.onclick = act; return made;
      };
      if (renamingSession === session.id) {
        sessionRenameInput.setAttribute('aria-label', `New name of ${session.name}`);
        if (!typing) sessionRenameInput.placeholder = session.name;
        sessionRenameSave.disabled = busy; item.append(sessionRename); return item;
      }
      const words = uiTraits(uiPart('text'), 'stack', 'grow');
      words.append(uiPart('text', session.name), uiTraits(uiPart('text', sessionStateText(session.state)), 'muted'));
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
  function saveSessionRename() {
    const id = renamingSession; const title = sessionRenameInput.value.trim();
    if (!id || busy || !options.onRenameSession || !title || !text(title, 256)) return;
    renamingSession = null;
    void operate('renamed', async () => { await options.onRenameSession!(id, title); await loadSessions(); }, 'session')
      .then(() => sessionsList.querySelector<HTMLButtonElement>(`[data-key="rename:${id}"]`)?.focus());
  }
  sessionRenameSave.onclick = saveSessionRename;
  sessionRenameInput.onkeydown = event => {
    if (!renamingSession) return;
    if (confirmsField(event)) { event.preventDefault(); claimKey(event); saveSessionRename(); }
    else if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); claimKey(event); endSessionRename(renamingSession); }
  };
  /** A row of the listbox: an li option drawn as Imago's im-row, with the classes the window's smokes find it by. */
  function rowPart(className: string): HTMLLIElement {
    const item = uiPart('item', undefined, `im-row ${className}`); item.setAttribute('role', 'option'); return item;
  }
  /** The text of a row as the button that acts on it: the row draws it (im-row-text), not a button's own drawing. Out of
   *  the tab order, as every control in a row is: the row is the list's stop, and its menu holds its actions. */
  function rowText(className: string, action: string): HTMLButtonElement {
    const button = uiButton(''); button.className = `im-row-text ${className}`; button.replaceChildren();
    button.dataset.action = action; button.tabIndex = -1; return button;
  }
  function rowAction(button: HTMLButtonElement, className: string): HTMLButtonElement {
    button.classList.add(className); button.tabIndex = -1; return button;
  }

  /** A thread's row: its avatar (all the rail shows), the button that turns it on and off with its name and state,
   *  then its actions. Made once, then kept up to date. */
  function threadItem(row: WorksetSummary): HTMLLIElement {
    let item = anchors.get(row.id);
    if (!item) {
      item = rowPart('thread-sidebar-row'); item.setAttribute('aria-selected', 'false');
      // In the rail the row shows only its avatar: the thread's initials, filled with its category while it is on.
      const avatar = componentElement(avatarHtml('')); avatar.classList.add('im-rail-item', 'thread-sidebar-rail-item', 'thread-sidebar-switch');
      const anchor = rowText('thread-sidebar-anchor', 'toggle'); anchor.dataset.worksetId = row.id;
      anchor.setAttribute('aria-pressed', 'false'); anchor.setAttribute('aria-current', 'false');
      const sub = uiPart('text', undefined, 'im-row-sub');
      sub.append(uiPart('text', undefined, 'thread-sidebar-state'), uiPart('text', undefined, 'thread-sidebar-updated'));
      anchor.append(uiPart('text', undefined, 'im-row-title thread-sidebar-title'), sub);
      item.append(avatar, anchor);
      if (options.onAddSession) item.append(rowAction(componentElement(iconButtonHtml('Add a session', 'add-session', 'plus')), 'thread-sidebar-add-session'));
      if (options.chat) {
        const chat = rowAction(componentElement(buttonHtml('Chat', 'chat', { sm: true, cls: 'im-btn--ghost' })), 'thread-sidebar-chat'); chat.hidden = true;
        const dot = uiPart('text', undefined, 'im-dot'); dot.dataset.imTone = 'info'; dot.setAttribute('aria-hidden', 'true'); dot.hidden = true;
        chat.prepend(dot); item.append(chat);
      }
      if (options.team) {
        const team = rowAction(componentElement(buttonHtml('Team', 'team', { sm: true, cls: 'im-btn--ghost' })), 'thread-sidebar-team'); team.hidden = true;
        item.append(team);
      }
      item.append(rowAction(componentElement(iconButtonHtml('Thread settings', 'more', 'sliders', { attrs: { 'aria-controls': 'thread-sidebar-details', 'aria-expanded': 'false' } })), 'thread-sidebar-more'));
      anchors.set(row.id, item); threadOfRow.set(item, row.id);
    }
    const anchor = item.querySelector<HTMLButtonElement>('.thread-sidebar-anchor')!;
    const on = isOn(row.id); const name = shownTitle(row); const project = labelOf(row.projectId);
    if (anchor.disabled !== busy) anchor.disabled = busy;
    attr(anchor, 'aria-current', String(options.openId() === row.id));
    attr(anchor, 'aria-pressed', String(on)); attr(item, 'aria-selected', String(on));
    setText(item.querySelector('.thread-sidebar-title')!, name);
    const running = options.running(row.id); const waiting = options.attention?.(row.id) === true;
    const kind = row.archived ? 'archived' : waiting ? 'recover' : running ? 'open' : 'resumable';
    const total = Math.max(row.sessions, running);
    const sessions = `${total} session${total === 1 ? '' : 's'}`;
    const said = row.archived ? `Archived · ${sessions}` : waiting ? `${sessions} · waiting for you`
      : running ? (on ? `${sessions} · ${running} running` : `${running} running · tabs hidden`)
      : !total ? (on ? 'Empty · + adds a session' : 'Empty') : sessions;
    const stateLine = item.querySelector<HTMLElement>('.thread-sidebar-state')!;
    if (stateLine.dataset.state !== kind) stateLine.dataset.state = kind;
    setText(stateLine, said);
    const ago = agoText(row.usedAt); setText(item.querySelector('.thread-sidebar-updated')!, ago ? ` · ${ago}` : '');
    // The thread's colour while it is on: the row's mark and the rail's avatar. A thread waiting for the user has the
    // badge, and its title and its line say so.
    const category = categoryOf(row.id); const avatar = item.querySelector('.thread-sidebar-rail-item')!;
    attr(item, 'data-im-cat', category); attr(avatar, 'data-im-cat', category); setText(avatar, initials(name));
    flag(item, 'data-im-badge', waiting); attr(item, 'data-im-tone', waiting ? 'warn' : null);
    attr(item, 'aria-label', `${name}, ${said}`);
    attr(item, 'title', `${name} · ${project}${running ? ` · ${running} running` : ''}${waiting ? ' · waiting for you' : ''}\n`
      + (on ? 'Click to turn it off: its running sessions stop, after asking, and stay in the thread, not running.' : 'Click to show its tabs.'));
    const add = item.querySelector<HTMLButtonElement>('.thread-sidebar-add-session');
    if (add) { if (add.disabled !== busy) add.disabled = busy; attr(add, 'title', `Add a session to ${name}`); attr(add, 'aria-label', `Add a session to ${name}`); }
    const talking = !!options.chat?.enabled(row.id) && !row.archived;
    const chat = item.querySelector<HTMLButtonElement>('.thread-sidebar-chat');
    if (chat) {
      const unread = talking && options.chat!.unread(row.id);
      if (chat.hidden !== !talking) chat.hidden = !talking;
      flag(chat, 'data-unread', unread); const dot = chat.querySelector<HTMLElement>('.im-dot')!; if (dot.hidden !== !unread) dot.hidden = !unread;
      const label = `Conversation of ${name}${unread ? ' · new messages' : ''}`; attr(chat, 'title', label); attr(chat, 'aria-label', label);
    }
    const team = item.querySelector<HTMLButtonElement>('.thread-sidebar-team');
    if (team) {
      if (team.hidden !== !talking) team.hidden = !talking;
      const label = `Team map of ${name}: who gives work to whom`; attr(team, 'title', label); attr(team, 'aria-label', label);
    }
    const more = item.querySelector<HTMLButtonElement>('.thread-sidebar-more')!;
    attr(more, 'title', `Settings of ${name}`); attr(more, 'aria-label', `Settings of ${name}`); attr(more, 'aria-expanded', String(expanded === row.id));
    return item;
  }
  /** A project's row: its name and how many threads it lists, a click folds them; + makes a thread, × takes it out. */
  function groupHead(project: string, group: { rows: WorksetSummary[]; more: boolean; failed: boolean } | undefined, closed: boolean): HTMLLIElement {
    let head = heads.get(project);
    if (!head) {
      head = rowPart('im-row--1 thread-sidebar-group'); head.dataset.projectId = project;
      // In the rail a project is its folder.
      const rail = uiPart('text', undefined, 'im-rail-item'); rail.setAttribute('aria-hidden', 'true'); rail.innerHTML = iconHtml('folder');
      const toggle = rowText('thread-sidebar-group-toggle', 'fold'); toggle.setAttribute('aria-expanded', 'true');
      toggle.append(uiPart('text', undefined, 'im-row-title thread-sidebar-group-name'));
      head.append(rail, toggle, uiPart('text', undefined, 'im-row-meta thread-sidebar-group-count'));
      if (options.create) head.append(rowAction(componentElement(iconButtonHtml('New thread', 'new-thread', 'plus')), 'thread-sidebar-group-add'));
      head.append(rowAction(componentElement(iconButtonHtml('Remove from the sidebar', 'remove-project', 'close')), 'thread-sidebar-group-remove'));
      heads.set(project, head);
    }
    const name = labelOf(project);
    const listed = group ? `${group.rows.length}${group.more ? '+' : ''}` : '';
    attr(head.querySelector('.thread-sidebar-group-toggle')!, 'aria-expanded', String(!closed));
    setText(head.querySelector('.thread-sidebar-group-name')!, name);
    setText(head.querySelector('.thread-sidebar-group-count')!, closed && listed ? `${listed} folded` : listed);
    flag(head, 'data-current', project === currentProject);
    const threads = group ? `${listed} thread${listed === '1' ? '' : 's'}` : 'threads not read yet';
    attr(head, 'aria-label', `${name}, ${threads}${closed ? ', folded' : ''}`);
    attr(head, 'title', `${name} · ${threads}${project === currentProject ? ' · the toolbar\'s project' : ''}\n${closed ? 'Click to show its threads.' : 'Click to fold its threads.'}`);
    const add = head.querySelector<HTMLButtonElement>('.thread-sidebar-group-add');
    if (add) { const off = busy || archivedView; if (add.disabled !== off) add.disabled = off; attr(add, 'title', `New thread in ${name}`); attr(add, 'aria-label', `New thread in ${name}`); }
    const hide = head.querySelector<HTMLButtonElement>('.thread-sidebar-group-remove')!;
    if (hide.disabled !== busy) hide.disabled = busy;
    attr(hide, 'title', `Remove ${name} from the sidebar`); attr(hide, 'aria-label', `Remove ${name} from the sidebar`);
    return head;
  }

  /** What a row does: a click on it (or Enter, Space) turns its thread on or off, or folds its project; its
   *  buttons, and the same items in its menu, do the rest. */
  function act(item: HTMLElement, action: string, control: HTMLButtonElement | null, fromMenu = false) {
    if (destroyed) return;
    const project = item.classList.contains('thread-sidebar-group') ? item.dataset.projectId : undefined;
    if (project) {
      if (action === 'fold') { if (!collapsedGroups.delete(project)) collapsedGroups.add(project); render(); }
      else if (action === 'new-thread') createIn(project);
      else if (action === 'remove-project') { if (!busy) void setActive(active.filter(id => id !== project)); }
      return;
    }
    const id = threadOfRow.get(item); const row = id ? rows.find(candidate => candidate.id === id) : undefined;
    if (!row) return;
    if (action === 'toggle') void openThread(row);
    else if (action === 'add-session') { if (!busy && options.onAddSession) options.onAddSession(Object.freeze({ ...row }), control ?? item); }
    else if (action === 'chat') options.chat?.open(Object.freeze({ ...row }));
    else if (action === 'team') options.team?.open(Object.freeze({ ...row }));
    else if (action === 'more') {
      toggleDetails(row.id);
      if (fromMenu && expanded === row.id) (renameForm.hidden ? details.querySelector<HTMLElement>('button:not([disabled])') : renameInput)?.focus();
    }
  }
  /** The row's buttons as a menu, for the keyboard (the Menu key, Shift+F10) and the right click. */
  function openRowMenu(item: HTMLElement) {
    const entries: MenuItem[] = [];
    for (const button of item.querySelectorAll<HTMLButtonElement>(':scope > button[data-action]')) {
      const action = button.dataset.action!; const label = MENU_LABELS[action];
      if (!label || button.hidden) continue;
      entries.push({ label, ...(button.disabled ? { disabled: 'The list is being read. Try again in a moment.' } : {}), run: () => act(item, action, button, true) });
    }
    if (!entries.length) return;
    openMenu(item, entries, { onClose: () => { item.removeAttribute('aria-expanded'); item.removeAttribute('aria-haspopup'); } });
  }
  // The list's listeners are attached once; each reads its row from the event's target.
  list.addEventListener('click', event => {
    const target = event.target instanceof Element ? event.target : null;
    const item = target?.closest<HTMLElement>('li.im-row');
    if (!target || !item || !list.contains(item) || item.getAttribute('aria-disabled') === 'true') return;
    const control = target.closest<HTMLButtonElement>('button');
    act(item, control?.dataset.action ?? (item.classList.contains('thread-sidebar-group') ? 'fold' : 'toggle'), control);
  });
  list.addEventListener('keydown', event => {
    const menuKey = event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey);
    if (!menuKey || event.defaultPrevented || !(event.target instanceof HTMLElement) || !event.target.matches('li.im-row')) return;
    event.preventDefault(); claimKey(event); openRowMenu(event.target);
  });
  list.addEventListener('contextmenu', event => {
    const item = event.target instanceof Element ? event.target.closest<HTMLElement>('li.im-row') : null;
    if (!item || !list.contains(item) || item.getAttribute('aria-disabled') === 'true') return;
    event.preventDefault(); openRowMenu(item);
  });
  const stopRoving = roving(list, {
    item: 'li.im-row[role="option"]', orientation: 'vertical',
    onSelect: item => act(item, item.classList.contains('thread-sidebar-group') ? 'fold' : 'toggle', null),
  });

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
    if (!options.create || busy || archivedView) return;
    void operate('created', async () => {
      const made = await options.create!({ projectId: project });
      if (!record(made) || !worksetId(made.id) || !integer(made.revision)) throw new Error('invalid');
      const row: WorksetSummary = { id: made.id, projectId: project, title: null, archived: false, revision: made.revision, usedAt: Date.now(), sessions: 0, open: 0, lead: null };
      await options.onOpen(Object.freeze(row));
    });
  }
  /** Opens the settings of a thread, or closes them; one thread at a time. */
  function toggleDetails(id: string) {
    editing = false; renamingSession = null;
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
  /** Closes the settings; the focus goes back to the thread's row. */
  function foldDetails() {
    const id = expanded; editing = false; expanded = null; render();
    if (id) anchors.get(id)?.focus();
  }
  renameInput.oninput = () => { editing = true; };
  function saveName() {
    const current = chosen(); const title = renameInput.value.trim();
    // While the list is being read the name waits in the field: Enter again once Save is enabled.
    if (busy || !current || !options.update || !title || !text(title, 256)) return;
    // Enter saves and folds the settings away, like Escape folds them without saving.
    editing = false; expanded = null;
    void operate('renamed', async () => { await options.update!({ worksetId: current.id, expectedRevision: current.revision, title }); });
  }
  renameSave.onclick = saveName;
  renameInput.onkeydown = event => {
    if (confirmsField(event)) { event.preventDefault(); claimKey(event); saveName(); }
    else if (event.key === 'Escape') { event.preventDefault(); claimKey(event); foldDetails(); }
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
  // Deleting a thread cannot be undone: a dialog names it and what goes with it, and the focus starts on Cancel.
  remove.onclick = () => {
    const current = chosen(); if (!current || !options.remove || busy) return;
    const sessions = `${current.sessions} session${current.sessions === 1 ? '' : 's'}`;
    void confirm(shell, {
      title: `Delete the thread ${shownTitle(current)}?`,
      body: `It is deleted from Shinatama with its ${sessions}, closed ones included. The agents' own files stay. This cannot be undone.`,
      confirm: 'Delete thread', cancel: 'Cancel', danger: true,
    }).then(yes => {
      const now = rows.find(row => row.id === current.id);
      if (!yes || !now || destroyed || busy) return;
      void operate('deleted', async () => { await options.remove!(Object.freeze({ ...now })); expanded = null; });
    });
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
  function setActive(next: readonly string[], save = true): void | Promise<void> {
    const unique = [...new Set(next)].filter(id => projectId(id)).slice(0, MAX_ACTIVE);
    if (unique.length === active.length && unique.every((id, index) => id === active[index])) { render(); return; }
    const leaving = active.filter(id => !unique.includes(id));
    generation++; active = unique; editing = false; failed = false; message = active.length ? '' : NO_PROJECTS;
    // Its threads go off with it: no tab is left without a row to turn it off or on again.
    const removed = leaving.length ? options.onProjectsRemoved?.(leaving) : undefined;
    for (const id of [...groups.keys()]) if (!active.includes(id)) groups.delete(id);
    for (const id of [...heads.keys()]) if (!active.includes(id)) { heads.delete(id); notes.delete(id); }
    rows = active.flatMap(id => groups.get(id)?.rows ?? []);
    if (save) changedDisplay(); else render();
    if (active.length) void read();
    return removed;
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
  /** Shows the sidebar full or compact, or closes it; saved with the rest of its display. */
  function setMode(next: SidebarMode) {
    if (destroyed || next === mode()) return;
    display = { ...display, hidden: next === 'hidden' ? true : undefined, collapsed: next === 'hidden' ? display.collapsed : next === 'compact' };
    if (!display.hidden) delete display.hidden;
    applyDisplay(); syncFold(); changedDisplay();
  }
  /** The fold the person chose with the sidebar's button (`sidebarCollapse`'s `onChange`): recorded and saved. */
  function setCollapsed(collapsed: boolean) {
    if (destroyed || typeof collapsed !== 'boolean' || collapsed === display.collapsed) return;
    display = { ...display, collapsed }; changedDisplay();
  }
  close.onclick = () => { setMode('hidden'); options.restoreFocus?.(); };
  // Under 620 the title bar's button opens the sidebar as a drawer, also while it is closed (hidden) above 620.
  const drawerWatch = new MutationObserver(() => {
    const wasHidden = element.hidden; applyDisplay();
    if (wasHidden && !element.hidden) (list.querySelector<HTMLElement>('[tabindex="0"]') ?? element.querySelector<HTMLElement>('button:not([disabled])'))?.focus();
  });
  drawerWatch.observe(shell, { attributes: true, attributeFilter: ['data-im-sidebar-open'] });
  /** The width: sidebarResize on the sidebar once it is in the shell, from the saved width. */
  let stopResize: (() => void) | null = null; let placed: MutationObserver | null = null;
  function mountResize() {
    if (destroyed || stopResize) return;
    if (shell.querySelector('.im-sidebar') !== element) {
      // The host has not put the sidebar in the shell's body yet.
      if (!placed) { placed = new MutationObserver(mountResize); placed.observe(shell, { childList: true, subtree: true }); }
      return;
    }
    placed?.disconnect(); placed = null;
    stopResize = sidebarResize(shell, {
      label: 'Thread sidebar width', width: shownWidth(display.width),
      onChange: width => { if (!destroyed) { display = { ...display, width: shownWidth(width) }; changedDisplay(); } },
    });
    element.querySelector('.im-sidebar-splitter')?.classList.add('thread-sidebar-resize');
  }

  render();
  /** Settles once the saved display is read, or its read failed: `collapsed()` is then the saved choice. */
  const ready: Promise<void> = (async () => {
    if (options.loadState) {
      try {
        const value = await options.loadState();
        const restored = destroyed || value === null ? null : stateValue(value);
        if (restored) {
          display = { ...restored, width: shownWidth(restored.width) };
          const saved = restored.activeProjects ?? [];
          setActive([...saved, ...active.filter(id => !saved.includes(id))], false);
        }
      } catch { if (!destroyed) options.reportError('Sidebar display could not be restored. Default dimensions are in use.'); }
    }
    loaded = true;
    if (destroyed) return;
    render(); syncFold(); mountResize();
    if (saveWanted) { saveWanted = false; changedDisplay(); }
  })();
  return {
    element, setProject, setProjects, currentProjectId: (): string | null => currentProject,
    activeIds: (): readonly string[] => [...active],
    /** Settles when the change is final: a removal waits for its confirmation, which may bring the project back. */
    async toggleActive(id: string): Promise<void> { if (!destroyed) await setActive(active.includes(id) ? active.filter(other => other !== id) : [...active, id]); },
    mode, setMode, ready,
    /** The saved fold into the rail, for `mountApp`'s `sidebar.collapsed` (read it once `ready` settled). */
    collapsed: (): boolean => display.collapsed,
    /** `mountApp`'s `sidebar.onChange`: the person folded or unfolded the sidebar. */
    setCollapsed,
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
    destroy() {
      destroyed = true; generation++; window.clearInterval(namesTimer); window.clearTimeout(saveTimer); window.clearTimeout(changeTimer);
      stopRoving(); stopResize?.(); placed?.disconnect(); drawerWatch.disconnect(); anchors.clear(); element.remove();
    },
  };
}
