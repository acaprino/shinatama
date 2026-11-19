import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { emptyWorkspace, parseWorkspaceLayout, reduceWorkspaceLayout, tabGroups, keyboardNeighbor, type WorkspaceLayout, type LayoutAction } from './workspace-model.ts';
import { createWorkspaceRenderer } from './workspace-renderer.ts';
import { createTauriWorkspaceTransport, createWorkspacePersistence, type WorkspacePersistence } from './workspace-persistence.ts';
import './workspace-renderer.css';
import { initializeWindowControls } from './window-controls.ts';
import { canZoom, changeZoom, initializeDesktopShell, interfaceZoom, onInterfaceZoom } from './desktop-shell.ts';
import { ZOOM_STEPS } from './desktop-shell-model.ts';
import { createProjectPicker } from './project-picker.ts';
import { createNewProjectDialog } from './new-project.ts';
import { initializeMaintenance } from './maintenance.ts';
import { openModal, openSheet } from './modal.ts';
import { createTerminalFind } from './terminal-find.ts';
import { createTerminalHistory, LIVE_SCROLLBACK, splitOutput, type TerminalHistory } from './terminal-history.ts';
import { createTerminalMinimap, type TerminalMinimap } from './terminal-minimap.ts';
import { openTerminalHistory } from './terminal-history-view.ts';
import { attachTerminalPaste, startTerminalDrop, type TerminalTarget } from './terminal-input.ts';
import { createPresentation, PROFILES, setTerminalGlass } from './presentation.ts';
import './presentation.css';
import { startTerminalResources, formatTerminalResources, type TerminalResourceStatus } from './terminal-resources.ts';
import { createAttentionControl } from './attention-control.ts';
import { createSidecarControl } from './sidecar-control.ts';
import { createNewSession } from './new-session.ts';
import { createSessionMenu } from './session-menu.ts';
import { beginMutation, browseFolder, createCatalogClient, createRequestId } from './project-catalog.ts';
import { createSessionLibrary } from './session-library.ts';
import { createSessionDetail } from './session-detail.ts';
import { createSessionExport } from './session-export.ts';
import type { ProgressBackend, ProgressEvent } from './session-progress.ts';
import { createHopPreview, type HopPreview } from './hop-preview.ts';
import { createFilesPanel } from './files-panel.ts';
import { createBrowserTabs, type BrowserTarget } from './browser-tab.ts';
import './browser-tab.css';
import { createTeamMap, teamViewId } from './team-map.ts';
import { classifyTarget, findLinksInRows, hoverText, type CellText } from './terminal-links.ts';
import { createProjectsOverview } from './projects-overview.ts';
import { createLeftoversPanel } from './leftovers-panel.ts';
import { createSessionsPanel } from './sessions-panel.ts';
import { createSettingsPanel } from './settings-panel.ts';
import { createSessionSearch } from './session-search.ts';
import { createProjectMenu, createTabFilter, folderName, organizeToolbar, samePath } from './toolbar-simple.ts';
import { createSessionRaw } from './session-raw.ts';
import { createSessionFind } from './session-find.ts';
import { resumeFailureText } from './resume-failure.ts';
import { layoutLoadFailureText, transientLayoutFailure } from './workspace-persistence.ts';
import { newSessionProject, type NewSessionScope } from './session-project.ts';
import type { ThreadSidebarState, ThreadSummary } from './thread-sidebar.ts';
import { createWorksetSidebar, type WorksetSummary } from './workset-sidebar.ts';
import { attemptKey, forgetRefusal, nextToReopen, threadsOff, reopenCard, reopenStep, restoredRunning, rowsToRecover, sessionTabLabel, sessionTabName, threadSessions, withoutInstance } from './session-tabs.ts';
import { createThreadChat, laterEvent } from './thread-chat.ts';
import { createToneKeeper } from './thread-tone.ts';
import './styles.css';
import './smooth.css';

type SessionInfo = { id: string; cwd: string; shell: string };
type HarnessCapabilities = { id: string; label: string; nativeResume: boolean };
type OutputBatch = { chunks: number[][]; eof: boolean; exit_code: number | null; error: string | null };
type View = {
  info: SessionInfo; terminal: Terminal; fit: FitAddon; element: HTMLDivElement;
  group: string; hidden: boolean; visible: boolean; ended: boolean; disposed: boolean; profile: string;
  lastSize: string; resizeTimer: number; resizeChain: Promise<void>;
  /** Lines that left the live buffer, its minimap, and the open history view, if any. */
  history: TerminalHistory; minimap: TerminalMinimap; historyView: { close(): void } | null;
  /** Removes the listener for files and images pasted on this terminal. */
  stopInput?: () => void;
  /** The session this terminal runs and the thread (ADR-0055) that session belongs to. */
  threadId: string | null; workset: WorksetScope | null;
};
type WorksetScope = { id: string; projectId: string };
type Tone = 'info' | 'error';
const $ = <T extends HTMLElement = HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const desktop = isTauri();
// The desktop window is transparent over Windows acrylic; only the terminal panes let it through.
if (desktop) { document.documentElement.classList.add('native-glass'); setTerminalGlass(true); }
const builtinHarness: HarnessCapabilities = { id: 'powershell', label: 'PowerShell', nativeResume: false };
let sessionHarnesses: HarnessCapabilities[] = [builtinHarness];
let harnessCatalogLoaded = false;
let harnessCatalogLoading: Promise<boolean> | null = null;
let openingSessionDialog = false;
const views = new Map<string, View>();
/** Each terminal's element, for the files and images dropped or pasted on it. */
const inputTargets = new WeakMap<Element, TerminalTarget>();
if (desktop) void startTerminalDrop(element => { const host = element?.closest('.terminal-view'); return host ? inputTargets.get(host) ?? null : null; })
  .catch(error => notify(`Dropping files on terminals is unavailable: ${error}`));
const profiles = PROFILES;
let workspaceLayout = emptyWorkspace();
let persistence: WorkspacePersistence | null = null;
let scopeRevision = 0;
/** The notice that another window holds the layout is said once, not at every retry. */
let takeoverNoticeShown = false;
let preferredAxis: 'columns' | 'rows' = 'columns';
const bindings = new Map<string, HTMLElement>();
const terminalMetrics = new Map<string, TerminalResourceStatus>();
let projectPath = '';
let selectedProjectPath = '';
let threadSidebar: ReturnType<typeof createWorksetSidebar> | null = null;
/** The threads that are on (ADR-0055 v2): the window shows their sessions together, in one layout,
 *  with the terminals that belong to no thread. A thread turned off keeps its sessions running. */
let openWorksets: WorksetScope[] = [];
/** The threads to turn on at this start, read before anything can overwrite them: reattaching a running terminal runs `rememberSessions` while none is on yet. */
const threadsToReopen = (() => { try { return localStorage.getItem('shinatama.threads') ?? `[${localStorage.getItem('shinatama.thread') ?? ''}]`; } catch { return null; } })();
/** Set once the window close is confirmed: the terminals the shutdown ends must stay saved in the layout as
 *  running, so the next start restores them with it (a terminal that ends by itself is saved as not running). */
let closingWindow = false;
/** The sessions of the threads that are on, as last read, and the thread each belongs to. */
let worksetMembers: ThreadSummary[] = [];
const memberOf = new Map<string, WorksetScope>();
let switching = false;
/** Whether a terminal of this thread is drawn: every terminal of no thread is. */
const isOn = (worksetId: string | null | undefined) => worksetId == null || openWorksets.some(workset => workset.id === worksetId);
/** The thread worked in last: the one last turned on, or the thread of the last tab used. */
let currentWorkset: string | null = null;
/** The thread a tab belongs to, when that thread is on. */
function tabWorkset(viewId: string | null | undefined): WorksetScope | null {
  if (!viewId) return null;
  const view = views.get(viewId); if (view?.workset && isOn(view.workset.id)) return view.workset;
  const panel = workspaceLayout.views.find(candidate => candidate.id === viewId);
  return panel?.resource.kind === 'terminal' && panel.resource.threadId ? memberOf.get(panel.resource.threadId) ?? null : null;
}
const groupWorkset = (groupId: string) => tabWorkset(tabGroups(workspaceLayout.root).find(group => group.id === groupId)?.activeTabId);
/** The thread New session and the sidebar's actions mean: the current one, else the focused tab's, else the last turned on. */
function activeWorkset(): WorksetScope | null {
  return openWorksets.find(workset => workset.id === currentWorkset) ?? groupWorkset(focusedGroup()) ?? openWorksets.at(-1) ?? null;
}
const threadTitle = (workset: WorksetScope) => threadSidebar?.title(workset.id) ?? null;
/** The colour of a thread that is on, on its row and on its tabs; threads that are on keep different ones. */
const tones = createToneKeeper();
const toneOf = (worksetId: string) => tones.tone(openWorksets.map(workset => workset.id), worksetId);
/* ADR-0057: threads whose sessions talk to each other through their Mukade room. */
let chatThreads = new Set<string>();
let threadChat: ReturnType<typeof createThreadChat> | null = null;
/** Per thread: whether its room has news the user has not seen, and the sessions that are members through their hooks. */
const chatUnreadIds = new Set<string>(); const chatConnected = new Map<string, Set<string>>(); const chatLaunched = new Map<string, Set<string>>();
type ChatSeen = { eventId: string; timestamp: string };
const CHAT_SEEN_KEY = 'shinatama.threadChat.seen.v2';
/** The last event the user saw per thread, with its time (the backend reads the room from there):
 *  a convenience of this window, kept in its own storage. */
const chatSeen: Map<string, ChatSeen> = (() => {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(CHAT_SEEN_KEY) ?? '{}');
    if (saved && typeof saved === 'object' && !Array.isArray(saved))
      return new Map(Object.entries(saved as Record<string, unknown>).filter((entry): entry is [string, ChatSeen] => {
        const value = entry[1] as Partial<ChatSeen> | null;
        return /^w_[a-z2-7]{12}$/.test(entry[0]) && !!value && typeof value.eventId === 'string' && /^[0-9]{1,19}$/.test(value.eventId)
          && typeof value.timestamp === 'string' && value.timestamp.length <= 64;
      }).slice(0, 200));
  } catch { /* no storage: nothing is marked seen */ }
  return new Map();
})();
function markChatSeen(id: string, eventId: string, timestamp: string) {
  const before = chatSeen.get(id); if (before && !laterEvent(eventId, before.eventId)) return;
  chatSeen.set(id, { eventId, timestamp });
  // Seen now: the mark goes at once, not at the next overview.
  chatUnreadIds.delete(id);
  try { localStorage.setItem(CHAT_SEEN_KEY, JSON.stringify(Object.fromEntries([...chatSeen].slice(-200)))); } catch { /* kept for this run */ }
}
const chatUnread = (id: string) => chatUnreadIds.has(id) && threadChat?.current() !== id;
const CHAT_AGENTS = ['Claude Code', 'claude_code', 'Codex', 'codex', 'OpenCode', 'opencode', 'Pi', 'pi'];
/** " · in chat" on the tab of a session that is a member of its talking thread's room; " · chat" on an
 *  OpenCode or Pi session launched with the room's kit, whose member Shinatama cannot tie to it. */
function chatMark(view: { info: SessionInfo; threadId: string | null; workset: WorksetScope | null }) {
  if (!view.workset || !chatThreads.has(view.workset.id) || !CHAT_AGENTS.includes(view.info.shell)) return '';
  if (view.threadId && chatConnected.get(view.workset.id)?.has(view.threadId)) return ' · in chat';
  return view.threadId && chatLaunched.get(view.workset.id)?.has(view.threadId) ? ' · chat' : ' · not in chat yet';
}
async function setThreadChat(thread: Readonly<WorksetSummary>, enabled: boolean) {
  const result = await invoke<{ enabled: string[]; warning: string | null }>('thread_chat_set', { request: { worksetId: thread.id, enabled } });
  chatThreads = new Set(result.enabled);
  if (result.warning) notify(`The thread will talk, but its room is not ready now: ${result.warning}`, 'error');
  else if (enabled) notify('Claude Code, Codex, OpenCode and Pi sessions of this thread opened (or, for Claude Code and Codex, resumed) from now on join its room.', 'info');
  if (!enabled && threadChat?.current() === thread.id) threadChat.hide();
  await readChatOverview(); render();
}
let chatReading = false;
/** One cheap read for every talking thread: the marks on rows and tabs. */
async function readChatOverview() {
  if (!desktop || chatReading) return;
  const ids = [...chatThreads].slice(0, 64); if (!ids.length) { chatUnreadIds.clear(); chatConnected.clear(); chatLaunched.clear(); return; }
  chatReading = true;
  try {
    const seen = Object.fromEntries(ids.filter(id => chatSeen.has(id)).map(id => [id, chatSeen.get(id)!]));
    const overview = await invoke<{ service: string; rooms: Record<string, { latest: string; lastActivity: string | null; unread: boolean }>; connected: Record<string, string[]>; launched: Record<string, string[]> }>(
      'thread_chat_overview', { request: { worksetIds: ids, seen } });
    const marks = () => [chatConnected, chatLaunched].map(map => [...map].map(([id, set]) => `${id}:${[...set].join(',')}`).join(';')).join('|');
    const before = marks();
    chatUnreadIds.clear(); chatConnected.clear(); chatLaunched.clear();
    for (const [id, room] of Object.entries(overview.rooms ?? {})) {
      // A thread seen for the first time starts as read: old history is not news.
      if (!chatSeen.has(id)) markChatSeen(id, room.latest, room.lastActivity ?? new Date(0).toISOString());
      else if (room.unread) chatUnreadIds.add(id);
    }
    for (const [id, sessions] of Object.entries(overview.connected ?? {})) chatConnected.set(id, new Set(sessions));
    for (const [id, sessions] of Object.entries(overview.launched ?? {})) chatLaunched.set(id, new Set(sessions));
    const after = marks();
    threadSidebar?.repaint();
    if (before !== after) render();
  } catch { /* the marks keep their last state */ }
  finally { chatReading = false; }
}
/** The colour of the thread a tab belongs to; none for terminals of no thread and browser tabs. */
function tabTone(panelId: string): string | null {
  const view = views.get(panelId); if (view) return view.workset ? toneOf(view.workset.id) : null;
  const saved = savedSession(panelId); const owner = saved ? memberOf.get(saved.id) : undefined;
  return owner ? toneOf(owner.id) : null;
}
/** The sidebar's last read of agent-given names; a launch waits for it so the two never share the worker. */
let namesInFlight: Promise<unknown> = Promise.resolve();
let sidebarRevision = 0;
let starting = false;
/** An Open terminal waiting for a project choice; a dismissed picker forgets it. */
let pendingStart: { group: string; projectChosen: boolean } | null = null;
let pendingAttentionFocus: string | null = null;
const workspace = $('#workspace');
const confirmDialog = $<HTMLDialogElement>('#confirm-dialog');

let noticeTimer = 0;
/** True while the notice shows what an open is waiting for (`showOpening`). */
let openingShown = false;
function notify(message: unknown, tone: Tone = 'error') {
  const notice = $('#notice');
  notice.dataset.tone = tone;
  notice.setAttribute('role', tone === 'error' ? 'alert' : 'status');
  $('#notice span').textContent = String(message);
  notice.hidden = false;
  clearTimeout(noticeTimer); openingShown = false;
  if (tone !== 'error') noticeTimer = window.setTimeout(() => { notice.hidden = true; }, 6000);
}
/** What the open in progress is waiting for, said until the open ends (a first open after a start
 * waits for the model proxy and the harness check; it no longer fails while they finish). */
const openingSteps: Record<string, (agent: string) => string> = {
  proxy: () => 'starting the model proxy', proxy_resources: () => 'waiting for free memory to start the model proxy',
  check: agent => `checking ${agent}`, reading: () => 'reading the saved conversation',
  prepare: () => 'preparing the session', start: agent => `starting ${agent}`,
};
function showOpening(harness: string, step: string) {
  const text = openingSteps[step]; if (!text) return;
  const agent = sessionHarnesses.find(item => item.id === harness)?.label ?? harness;
  notify(`Opening ${agent}: ${text(agent)}…`, 'info'); clearTimeout(noticeTimer); openingShown = true;
}
/** Ends the progress line of an open; a result notice shown since stays. */
function endOpening() { if (openingShown) $('#notice').hidden = true; openingShown = false; }
/** Writes one line in shinatama.log (data folder); a log that cannot be written changes nothing. */
function appLog(source: 'layout' | 'page', message: string) { if (desktop) void invoke('app_log', { source, message }).catch(() => {}); }
$('#notice button').onclick = () => { $('#notice').hidden = true; };
const attentionControl = createAttentionControl(desktop, notify);
// Restart is the panel's exit when the proxy supervisor stops answering; it closes like the window does.
createSidecarControl(desktop, notify, () => closeWorkspace(true));
// Only a real modal takes the page. Drawers and sheets (modal.ts) leave the terminals live.
function modalOpen() { return !!document.querySelector('dialog[open][aria-modal="true"]'); }
function focusAttentionSession(id: string) {
  if (modalOpen()) { pendingAttentionFocus = id; return; }
  pendingAttentionFocus = null;
  const view = views.get(id);
  if (view?.workset && !isOn(view.workset.id)) {
    void showWorkset(view.workset).then(opened => { if (opened) focusAttentionSession(id); });
    return;
  }
  const group = tabGroups(workspaceLayout.root).find(candidate => candidate.tabs.includes(id));
  if (!view || !group) { notify('This terminal is no longer in the workspace.', 'info'); return; }
  // A notification can target another project: reveal its tab before drawing its console.
  tabFilter?.reveal(view.info.cwd);
  currentWorkset = tabWorkset(id)?.id ?? currentWorkset;
  changeLayout({ type: 'hide', viewId: id, hidden: false });
  changeLayout({ type: 'collapse', groupId: group.id, collapsed: false });
  changeLayout({ type: 'maximize', groupId: null });
  changeLayout({ type: 'activate', groupId: group.id, viewId: id });
  setFocus(group.id);
}
document.addEventListener('close', () => {
  if (pendingAttentionFocus && !modalOpen()) focusAttentionSession(pendingAttentionFocus);
}, true);
function setProject(path: string) {
  if (path.trim() !== projectPath) threadSidebar?.setProject(null);
  projectPath = path.trim();
  // The sidebar lists several projects (ADR-0055 v2): the button says Projects, not the last one chosen.
  // The selected one stays on the button for the code and tests that need it.
  $('#project').dataset.project = projectPath.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? '';
}
const presentation = createPresentation({ desktop, notify, onChange() {
  for (const view of views.values()) {
    const applied = presentation.applyProfile(view.terminal, view.profile); view.profile = applied.id; view.element.style.background = applied.background;
    if (applied.metricsChanged) { view.lastSize = ''; scheduleFit(view); }
  }
} });

/** The session's name among all the sessions of its thread (closed ones included, oldest first, as
 *  `workset_threads` gives them), numbered when several share it: the same on its running tab, on its
 *  "not running" tab and in the thread's panel, and the same after a restart. */
function memberName(row: ThreadSummary): string {
  const owner = memberOf.get(row.id)?.id;
  const siblings = worksetMembers.filter(other => memberOf.get(other.id)?.id === owner).map(other => ({ id: other.id, name: sessionName(other) }));
  return sessionTabName(siblings, row.id) ?? sessionName(row);
}
/** "Claude Code 2 · Fix browser tabs": the session first, then its thread (or its project's folder when the thread has no name).
 *  A tab is cut at its end, so what tells two sessions of one thread apart comes first. A terminal of no thread, or one whose
 *  session is not read yet, is named by its folder and its agent, numbered when the same agent runs twice there. */
function tabLabel(id: string) {
  const view = views.get(id);
  if (!view) {
    const saved = savedSession(id); if (!saved) return 'Saved terminal · unavailable';
    const owner = memberOf.get(saved.id); const folder = recentThreadProjects.get(saved.projectId);
    return sessionTabLabel({ name: memberName(saved), where: (owner && threadTitle(owner)) || (folder ? folderName(folder) : null), state: 'saved' });
  }
  const where = (view.workset && threadTitle(view.workset)) || folderName(view.info.cwd);
  const row = view.threadId ? worksetMembers.find(member => member.id === view.threadId) : undefined;
  if (row) return sessionTabLabel({ name: memberName(row), where, state: view.ended ? 'exited' : 'running', mark: chatMark(view) });
  const harness = sessionHarnesses.find(item => item.id === view.info.shell)?.label ?? view.info.shell;
  const twins = [...views.values()].filter(other => other.info.shell === view.info.shell && (view.workset
    ? other.workset?.id === view.workset.id : !other.workset && samePath(other.info.cwd, view.info.cwd)));
  return `${where} · ${harness}${twins.length > 1 ? ` ${twins.indexOf(view) + 1}` : ''}${view.ended ? ' · exited' : chatMark(view)}`;
}
/** The project folder a terminal tab belongs to, whether it runs or not: a "not running" tab is its
 *  saved session's project. Null for a tab whose session or project is not known. */
function panelFolder(panelId: string): string | null {
  const view = views.get(panelId); if (view) return view.info.cwd;
  const saved = savedSession(panelId); return saved ? recentThreadProjects.get(saved.projectId) ?? null : null;
}
let tabFilter: ReturnType<typeof createTabFilter> | null = null;
/** Shows one project's tabs; each pane switches to its first tab of that project, if it has one. */
function filterTabs(project: string | null) {
  if (project) for (const group of tabGroups(workspaceLayout.root)) {
    const shown = group.tabs.filter(id => {
      const panel = workspaceLayout.views.find(candidate => candidate.id === id); const folder = panelFolder(id);
      return panel?.resource.kind === 'terminal' && !panel.hidden && !!folder && samePath(folder, project);
    });
    if (shown.length && !shown.includes(group.activeTabId ?? '')) changeLayout({ type: 'activate', groupId: group.id, viewId: shown[0] });
  }
  render();
}
const browserTabs = createBrowserTabs({
  invoke: (command, args) => desktop ? invoke(command, args) : Promise.reject('Open the desktop app to browse.'),
  channel: onFrame => { const channel = new Channel<ArrayBuffer>(); channel.onmessage = onFrame; return channel; },
  notify: message => notify(message),
});
/** ADR-0058: the team map of a thread, a pane view of its own (a resource of kind team) that lists beside the thread's tabs. */
const teamMap = createTeamMap({
  invoke: (command, args) => desktop ? invoke(command, args) : Promise.reject('Open the desktop app to use the team map.'),
  title: id => threadSidebar?.title(id) ?? 'Thread',
  harnessLabel: id => harnessLabel(id),
  focusSession: id => { void focusThreadSession(id); },
});
const isTeamView = (id: string) => workspaceLayout.views.find(panel => panel.id === id)?.resource.kind === 'team';
/** Opens the thread's team map: one per thread. The first one of the layout gets a column beside the focused pane; later ones join that pane. */
function openTeamTab(thread: { id: string; projectId?: string }) {
  try {
    const opened = teamMap.open(thread);
    bindings.set(opened.viewId, opened.element);
    const present = workspaceLayout.views.find(panel => panel.id === opened.viewId);
    if (present) {
      const group = tabGroups(workspaceLayout.root).find(candidate => candidate.tabs.includes(opened.viewId));
      if (present.hidden) changeLayout({ type: 'hide', viewId: opened.viewId, hidden: false });
      if (group) { changeLayout({ type: 'collapse', groupId: group.id, collapsed: false }); changeLayout({ type: 'activate', groupId: group.id, viewId: opened.viewId }); setFocus(group.id, false); teamMap.focus(opened.viewId); }
      render(); return;
    }
    const view = { id: opened.viewId, resource: { kind: 'team' as const, resourceId: opened.viewId }, hidden: false };
    const home = tabGroups(workspaceLayout.root).find(group => group.tabs.some(id => isTeamView(id)));
    const focused = tabGroups(workspaceLayout.root).find(group => group.id === focusedGroup());
    if (home) changeLayout({ type: 'add-view', groupId: home.id, view });
    else if (focused && focused.tabs.length) changeLayout({ type: 'split', groupId: focused.id, view, axis: 'columns', side: 'after', newGroupId: `g-${opened.viewId}`, newSplitId: `s-${opened.viewId}` });
    else changeLayout({ type: 'add-view', groupId: focusedGroup(), view });
    const group = tabGroups(workspaceLayout.root).find(candidate => candidate.tabs.includes(opened.viewId));
    if (group) { changeLayout({ type: 'activate', groupId: group.id, viewId: opened.viewId }); setFocus(group.id, false); }
  } catch (error) { notify(error); }
}
function closeTeamTab(viewId: string) { bindings.delete(viewId); teamMap.close(viewId); changeLayout({ type: 'remove-view', viewId }); }
/** Opens a page or a local HTML document in a browser tab of its own (ADR-0056). The first one
 *  of the layout gets a column beside the focused pane; later ones join that pane as tabs. */
async function openBrowserTab(target: BrowserTarget, viewId?: string) {
  try {
    const opened = await browserTabs.open(target, viewId);
    bindings.set(opened.viewId, opened.element);
    if (workspaceLayout.views.some(panel => panel.id === opened.viewId)) { render(); return; }
    const view = { id: opened.viewId, resource: { kind: 'browser' as const, resourceId: opened.viewId }, hidden: false };
    const home = tabGroups(workspaceLayout.root).find(group => group.tabs.some(id => workspaceLayout.views.find(panel => panel.id === id)?.resource.kind === 'browser'));
    const focused = tabGroups(workspaceLayout.root).find(group => group.id === focusedGroup());
    if (home) changeLayout({ type: 'add-view', groupId: home.id, view });
    else if (focused && focused.tabs.length) changeLayout({ type: 'split', groupId: focused.id, view, axis: 'columns', side: 'after', newGroupId: `g-${opened.viewId}`, newSplitId: `s-${opened.viewId}` });
    else changeLayout({ type: 'add-view', groupId: focusedGroup(), view });
  } catch (error) { notify(error); }
}
const renderer = createWorkspaceRenderer({
  host: workspace, bindings,
  dispatch(action, expected) { if (expected === workspaceLayout) changeLayout(action); },
  onFocusView(id) { currentWorkset = tabWorkset(id)?.id ?? currentWorkset; threadSidebar?.repaint(); if (modalOpen()) return; if (browserTabs.has(id)) browserTabs.focus(id); else if (teamMap.has(id)) teamMap.focus(id); else views.get(id)?.terminal.focus(); },
  onVisibilityChange(id, visible) { if (browserTabs.has(id)) { browserTabs.setVisible(id, visible); return; } if (teamMap.has(id)) { teamMap.setVisible(id, visible); return; } const view = views.get(id); if (view) { view.visible = visible; if (visible) { scheduleFit(view); view.minimap.redraw(); } } },
  labelForView: panel => panel.resource.kind === 'browser' ? browserTabs.label(panel.id) : panel.resource.kind === 'team' ? teamMap.label(panel.id) : tabLabel(panel.id),
  toneForView: panel => panel.resource.kind === 'terminal' ? tabTone(panel.id) : null,
  tabFilter() {
    const project = tabFilter?.current();
    // Browser tabs belong to the thread, not to a project: the filter never hides them.
    return project ? { label: folderName(project), shows: panel => { if (panel.resource.kind === 'browser' || panel.resource.kind === 'team') return true; const folder = panelFolder(panel.id); return !!folder && samePath(folder, project); } } : null;
  },
  isInputBlocked: modalOpen,
  onError: () => notify('That layout change is unavailable. The current workspace was preserved.'),
  onNewTab: groupId => { void openNewSession(groupId, undefined, undefined, 'thread'); },
  onCloseView(id) {
    if (workspaceLayout.views.find(panel => panel.id === id)?.resource.kind === 'browser') {
      bindings.delete(id); void browserTabs.close(id); changeLayout({ type: 'remove-view', viewId: id }); return;
    }
    if (isTeamView(id)) { closeTeamTab(id); return; }
    void closePanel(id);
  },
  placeholderFor: panel => panel.resource.kind === 'browser' ? browserTabs.placeholder(panel.id, target => { void openBrowserTab(target, panel.id); })
    : panel.resource.kind === 'team' ? teamMap.placeholder(panel.id, thread => openTeamTab(thread)) : savedSessionCard(panel.id),
});
const resizeObserver = new ResizeObserver(entries => {
  for (const entry of entries) { const view = [...views.values()].find(view => view.element === entry.target); if (view) scheduleFit(view); }
});
const terminalFind = createTerminalFind({
  current() { const id = tabGroups(workspaceLayout.root).find(group => group.id === focusedGroup())?.activeTabId; const view = id ? views.get(id) : null; return view ? { id: view.info.id, terminal: view.terminal } : null; },
  returnFocus(id) { const view = views.get(id); if (view && !view.disposed && !modalOpen()) view.terminal.focus(); },
  notify: message => notify(message, 'info'),
});
const findButton = document.createElement('button'); findButton.type = 'button'; findButton.id = 'find-terminal'; findButton.textContent = 'Find'; findButton.title = 'Find in terminal (Ctrl+Shift+F)';
findButton.onclick = () => terminalFind.open(); $('#restore').before(findButton);
function focusedGroup() { return workspaceLayout.focusedGroupId ?? tabGroups(workspaceLayout.root)[0].id; }
function changeLayout(action: LayoutAction, save = true) {
  const result = reduceWorkspaceLayout(workspaceLayout, action);
  if (!result.ok) { notify('That layout change is unavailable.'); return; }
  workspaceLayout = result.layout;
  if (save) persistence?.update(workspaceLayout);
  render();
}
function setFocus(groupId: string, terminal = true) {
  const group = tabGroups(workspaceLayout.root).find(group => group.id === groupId);
  if (!group) return;
  if (group.activeTabId && workspaceLayout.focusedGroupId !== groupId) changeLayout({ type: 'focus', groupId });
  if (terminal && !modalOpen()) {
    const view = group.activeTabId ? views.get(group.activeTabId) : null;
    if (view) view.terminal.focus();
    else workspace.querySelector<HTMLButtonElement>(`.wm-pane[data-group-id="${groupId}"] .wm-empty-start`)?.focus();
  }
}
function threadHasRunningTab(threadId: string) {
  return workspaceLayout.views.some(panel => {
    if (panel.resource.kind !== 'terminal' || panel.resource.threadId !== threadId) return false;
    const view = views.get(panel.id); return !!view && !view.disposed && !view.ended;
  });
}
function render() {
  threadSidebar?.repaint(); // rows count the sessions running now
  for (const panel of workspaceLayout.views) {
    const view = views.get(panel.id); if (!view) continue;
    view.hidden = panel.hidden;
    view.group = tabGroups(workspaceLayout.root).find(group => group.tabs.includes(panel.id))?.id ?? focusedGroup();
  }
  renderer.render(workspaceLayout);
  tabFilter?.paint();
  for (const pane of workspace.querySelectorAll<HTMLElement>('.wm-pane')) {
    const group = tabGroups(workspaceLayout.root).find(group => group.id === pane.dataset.groupId); if (!group) continue;
    pane.classList.add('pane'); pane.classList.toggle('focused', workspaceLayout.focusedGroupId === group.id);
    const active = group.activeTabId ? views.get(group.activeTabId) : null;
    let footer = pane.querySelector<HTMLElement>('.wm-footer');
    if (!footer) { footer = document.createElement('div'); footer.className = 'wm-footer pane-footer'; pane.append(footer); }
    footer.hidden = group.collapsed;
    const state = document.createElement('span'); state.className = 'pane-state'; state.textContent = active ? active.ended ? 'Exited' : 'Shell running' : 'Ready';
    const profile = document.createElement('select'); profile.setAttribute('aria-label', 'Terminal color profile'); profile.disabled = !active;
    for (const [id, data] of Object.entries(profiles)) { const option = document.createElement('option'); option.value = id; option.textContent = data.name; option.selected = id === active?.profile; profile.append(option); }
    profile.onchange = () => { if (active) { active.profile = profile.value; const applied = presentation.applyProfile(active.terminal, active.profile); active.element.style.background = applied.background; if (applied.metricsChanged) { active.lastSize = ''; scheduleFit(active); }
      const parsed = parseWorkspaceLayout({ ...workspaceLayout, views: workspaceLayout.views.map(view => view.id === active.info.id && view.resource.kind === 'terminal' ? { ...view, resource: { ...view.resource, profileId: active.profile } } : view) });
      if (parsed.ok) { workspaceLayout = parsed.value; persistence?.update(workspaceLayout); }
    } };
    const path = document.createElement('span'); path.className = 'path'; path.textContent = active?.info.cwd ?? projectPath; path.title = path.textContent;
    const size = document.createElement('span'); size.className = 'size'; size.textContent = active ? `${active.terminal.cols}×${active.terminal.rows}` : '';
    const resources = document.createElement('span'); resources.className = 'terminal-resources'; resources.setAttribute('aria-label', 'Terminal resources');
    const measured = active && terminalMetrics.get(active.info.id);
    if (measured) { const formatted = formatTerminalResources(measured); resources.textContent = formatted.summary; footer.title = formatted.detail; }
    else { resources.textContent = active ? 'Measuring…' : ''; footer.title = active ? 'Resource measurements unavailable; native limits remain applied.' : ''; }
    footer.replaceChildren(state, profile, resources, path, size);
  }
  const hidden = workspaceLayout.views.filter(view => view.hidden).length;
  const paneCount = tabGroups(workspaceLayout.root).length;
  $('#pane-count').textContent = `${paneCount} pane${paneCount === 1 ? '' : 's'}`;
  $('#restore').textContent = hidden ? `Restore views (${hidden})` : 'Restore views';
  $<HTMLButtonElement>('#restore').disabled = !hidden;
  $('#terminal-count').textContent = `${views.size} terminal${views.size === 1 ? '' : 's'}${hidden ? ` · ${hidden} hidden` : ''}`;
}
function scheduleFit(view: View) {
  clearTimeout(view.resizeTimer);
  view.resizeTimer = window.setTimeout(() => {
    if (view.disposed || !view.visible || view.element.hidden || view.element.clientWidth < 50 || view.element.clientHeight < 50) return;
    view.fit.fit();
    const { rows, cols } = view.terminal; const size = `${cols}×${rows}`;
    if (size !== view.lastSize) view.history.reflowed();
    if (rows < 2 || cols < 2 || size === view.lastSize) return;
    view.lastSize = size;
    const group = workspace.querySelector<HTMLElement>(`[data-group-id="${view.group}"]`);
    if (group) group.querySelector<HTMLElement>('.size')!.textContent = size;
    if (!view.ended) view.resizeChain = view.resizeChain.then(async () => {
      if (!view.disposed && !view.ended) await invoke('terminal_resize', { id: view.info.id, rows, cols });
    }).catch(async error => { view.lastSize = ''; if (!view.disposed && !await endedElsewhere(view)) notify(error); });
  }, 80);
}
async function refreshThreadProject() {
  if (!desktop) return;
  try {
    const recent = await invoke<{id:string;path:string}[]>('project_recent');
    recentThreadProjects.clear();
    for (const project of recent) recentThreadProjects.set(project.id, project.path);
    threadSidebar?.setProjects(recent.map(project => ({projectId:project.id,label:project.path.replace(/[\\/]+$/, '').split(/[\\/]/).pop()})));
    const key = (path:string) => path.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').toLowerCase();
    const project = recent.find(project => key(project.path) === key(projectPath));
    if (project) threadSidebar?.setProject({projectId:project.id,label:project.path.replace(/[\\/]+$/, '').split(/[\\/]/).pop()});
  } catch { notify('Saved project threads are unavailable.'); }
}
/** A placeholder layout is still the placeholder when the only change is the pane sizes the renderer reports on its own. */
function sameApartFromSizes(a: WorkspaceLayout, b: WorkspaceLayout): boolean {
  if (a === b) return true;
  const bare = (layout: WorkspaceLayout) => JSON.stringify(layout, (key, value) => key === 'lastUsableSize' ? undefined : value);
  return bare(a) === bare(b);
}
/** Connects saving for the layout on screen: the open thread's, or the loose terminals'. */
async function connectWorkspace(adoptSaved = true, recovered = false) {
  if (!desktop) return;
  // One layout for the window, whichever threads are on (ADR-0055 v2).
  const scope = { workspaceId: 'main' };
  const generation = ++scopeRevision;
  const previous = persistence; persistence = null;
  if (previous) { const released = await previous.release(); if (!released.ok || released.dirty) notify('The previous layout could not be fully saved.'); previous.dispose(); }
  try {
    const controller = createWorkspacePersistence({ scope, initialLayout: workspaceLayout,
      transport: createTauriWorkspaceTransport((command, args) => invoke(command, args)),
      onStatus(status) {
        if (generation !== scopeRevision) return;
        const label = $('.workspace-name');
        label.title = status.message ?? (status.inFlight ? 'Saving layout…' : status.dirty ? 'Unsaved layout changes' : 'Layout saved');
        label.dataset.layoutState = status.phase;
        label.dataset.layoutDirty = String(status.dirty);
        label.dataset.layoutPending = String(status.inFlight);
      }
    });
    const beforeLoad = workspaceLayout;
    // Not yet the window's saver: an edit reported while the saved layout loads would count as a local edit and keep it from being restored.
    // A refusal that can pass (storage busy with another operation, still starting) is retried a few times before it is reported.
    let loaded = await controller.load();
    for (let attempt = 1; !loaded.ok && transientLayoutFailure(loaded.error) && attempt <= 4 && generation === scopeRevision; attempt++) {
      appLog('layout', `Load refused (attempt ${attempt}): ${loaded.error}${loaded.detail ? ` (${loaded.detail})` : ''}. Retrying.`);
      await new Promise(resolve => setTimeout(resolve, 500 * attempt));
      if (generation === scopeRevision) loaded = await controller.load();
    }
    if (generation === scopeRevision) persistence = controller;
    if (generation !== scopeRevision) { controller.dispose(); return; }
    if (!loaded.ok) {
      appLog('layout', `Load refused: ${loaded.error}${loaded.detail ? ` (${loaded.detail})` : ''}. The window keeps its panes and does not save its layout.`);
      notify(layoutLoadFailureText(loaded.error, loaded.detail)); return;
    }
    if (loaded.restored && adoptSaved && sameApartFromSizes(workspaceLayout, beforeLoad)) {
      workspaceLayout = reconciled(loaded.layout);
      controller.update(workspaceLayout); render();
    } else controller.update(workspaceLayout);
    if (loaded.ownership === 'other') {
      // Marked by another Shinatama. The store hands the arrangement over only when that process is
      // proven gone (closed badly, crashed), and then it is taken over without asking. While the other
      // one lives, this window keeps its own panes and asks again every 30 s, so it starts saving them
      // as soon as that window closes.
      let code: unknown = 'still_owned';
      if (!recovered) {
        try {
          await invoke('global_workspace_recover', { request: { ...scope, expectedRevision: loaded.revision } });
          if (generation !== scopeRevision) return;
          controller.dispose(); persistence = null; await connectWorkspace(adoptSaved, true); return;
        } catch (error) { code = typeof error === 'object' && error && 'code' in error ? (error as { code: unknown }).code : null; }
      }
      if (generation !== scopeRevision) return;
      if (!takeoverNoticeShown) {
        takeoverNoticeShown = true;
        notify(code === 'owned_elsewhere'
          ? 'Another Shinatama window is open on the same data. This window\'s panes are remembered once that window closes.'
          : 'The window\'s arrangement could not be taken over yet. Shinatama tries again shortly; the current panes stay.', 'info');
      }
      window.setTimeout(() => {
        if (generation !== scopeRevision) return;
        controller.dispose(); persistence = null; void connectWorkspace(false);
      }, 30000);
      return;
    }
    const claimed = await controller.claim();
    if (!claimed.ok) notify('Layout saving is unavailable. Your current workspace is preserved.');
  } catch { notify('Layout storage is unavailable. Your current workspace is preserved.'); }
}
async function startTerminal(group: string, cwd?: string): Promise<boolean> {
  if (!desktop) { notify('This is the browser preview. Run npm run desktop to open real terminals.', 'info'); return false; }
  if (starting) return false;
  if (cwd === undefined && (!projectPath || !projectPicker.canStart())) { pendingStart = { group, projectChosen: false }; openProjectDialog(); return false; }
  starting = true;
  $<HTMLButtonElement>('#new-terminal').disabled = true;
  try {
    await namesInFlight;
    const info = await invoke<SessionInfo>('terminal_start', { cwd: cwd ?? projectPath, rows: 24, cols: 80 });
    attachTerminal(info, group, null, activeWorkset());
    return true;
  } catch (error) { notify(error); return false; }
  finally { starting = false; $<HTMLButtonElement>('#new-terminal').disabled = false; }
}
const instanceRef = (id: string) => /^i_[a-z2-7]{12}$/.test(id) ? id : `prototype-${id}`;
/** The thread a session belongs to. The store queues requests (ADR-0055 v3), so a refusal is final;
 *  a session saved a moment ago may not be listed yet, so "none" is asked again briefly. */
async function worksetOf(threadId: string): Promise<WorksetScope | null> {
  for (let attempt = 0; attempt < 5; attempt++) {
    try { const found = await invoke<WorksetScope | null>('workset_of', { request: { threadId } }); if (found) return found; } catch { return null; }
    await new Promise(resolve => setTimeout(resolve, 300));
  }
  return null;
}
/** The sessions of one thread. The store queues requests, so it is asked once. */
const worksetThreads = (worksetId: string) => invoke<ThreadSummary[]>('workset_threads', { request: { worksetId } });
/** A session started outside this window's own flow (a hop, a collaboration): its thread comes from the saved record. */
async function attachSession(info: SessionInfo, threadId: string) {
  let workset: WorksetScope | null = activeWorkset();
  workset = await worksetOf(threadId) ?? workset;
  attachTerminal(info, focusedGroup(), threadId, workset);
  if (isOn(workset?.id)) await refreshMembers(); else threadSidebar?.changed();
}
/** `workset`: the thread the session belongs to. A terminal opened into a thread that is off (a collaboration, a hop) runs
 *  without a tab and is drawn when that thread is turned on; turning a thread off stops its terminals (hideWorksets).
 *  `background`: reopened by the start, it takes its tab's place without bringing it forward or taking the focus. */
function attachTerminal(info: SessionInfo, group: string, threadId: string | null = null, workset: WorksetScope | null = null, background = false) {
  // Native collaboration events and reload recovery may see the same owned
  // process. Attach once; neither delivery path is permission to relaunch it.
  if (views.has(info.id)) return;
  const element = document.createElement('div'); element.className = 'terminal-view';
  const initialProfile = ['Claude Code', 'claude_code'].includes(info.shell) ? 'claude' : 'neutral';
  const terminal = new Terminal({ ...presentation.optionsForProfile(initialProfile),
    // The live buffer is small; older lines move to the history archive (terminal-history.ts).
    cursorBlink: false, scrollback: LIVE_SCROLLBACK,
    allowProposedApi: false, allowTransparency: desktop, screenReaderMode: true });
  const fit = new FitAddon(); terminal.loadAddon(fit);
  // A new terminal paints its profile background at once. On glass the pane behind it is clear,
  // so without this a fresh session has no tint and its text sits on the bare backdrop.
  element.style.background = String(terminal.options.theme?.background ?? '');
  workspace.append(element); terminal.open(element);
  // No clipboard or image addon. Escape text never reaches HTML; bounded native attention
  // controls are observed by Rust before rendering. Links (plain URLs, local .html paths and
  // OSC 8 hyperlinks) open in a browser tab on a click (ADR-0056 v2); a drag still selects text,
  // and the hover shows the real target.
  for (const osc of [0, 1, 2, 7, 9, 52, 777]) terminal.parser.registerOscHandler(osc, () => true);
  const openLink = (event: MouseEvent, text: string) => {
    if (event.button !== 0) return;
    const target = classifyTarget(text, info.cwd);
    if (target.kind === 'web') void openBrowserTab({ url: target.url });
    else if (target.kind === 'file') void openBrowserTab({ file: target.path });
    else notify(target.reason, 'info');
  };
  const hover = (text: string) => { element.title = hoverText(classifyTarget(text, info.cwd)); };
  const leave = () => { element.title = ''; };
  terminal.options.linkHandler = { activate: openLink, hover: (_event, text) => hover(text), leave, allowNonHttpProtocols: true };
  terminal.registerLinkProvider({ provideLinks(y, callback) {
    // The whole line the terminal wrapped over several rows, so a path cut by the right edge is one link.
    const buffer = terminal.buffer.active; const blank = buffer.getNullCell();
    let first = y - 1; while (first > 0 && y - 1 - first < 16 && buffer.getLine(first)?.isWrapped) first--;
    let last = y - 1; while (last - (y - 1) < 16 && buffer.getLine(last + 1)?.isWrapped) last++;
    const rows: CellText[][] = [];
    for (let row = first; row <= last; row++) {
      const line = buffer.getLine(row); const cells: CellText[] = [];
      if (line) for (let x = 0; x < line.length; x++) {
        const cell = line.getCell(x, blank); if (!cell || cell.getWidth() === 0) continue;
        cells.push({ chars: cell.getChars() || ' ', x: x + 1 });
      }
      rows.push(cells);
    }
    const links = findLinksInRows(rows, first + 1).filter(link => link.start.y <= y && link.end.y >= y).map(link => ({ text: link.target, range: { start: link.start, end: link.end },
      activate: openLink, hover: () => hover(link.target), leave, decorations: { pointerCursor: true, underline: true } }));
    callback(links.length ? links : undefined);
  } });
  terminal.parser.registerCsiHandler({ final: 't' }, () => true);
  const history = createTerminalHistory(terminal);
  const view: View = { info, terminal, fit, element, group, hidden: false, visible: false, ended: false, disposed: false,
    profile: initialProfile, lastSize: '', resizeTimer: 0, resizeChain: Promise.resolve(), threadId, workset,
    history, historyView: null, minimap: createTerminalMinimap({ host: element, terminal, history,
      shown: () => view.visible && !view.hidden && !view.disposed,
      openHistory: line => {
        view.historyView = openTerminalHistory({ host: element, history, line,
          terminalOptions: presentation.optionsForProfile(view.profile), background: element.style.background,
          closed: () => { view.historyView = null; if (!view.disposed && !modalOpen()) terminal.focus(); } });
      } }) };
  views.set(info.id, view); bindings.set(info.id, element); resizeObserver.observe(element);
  const input: TerminalTarget = { terminal, element, reader: info.shell === 'PowerShell' ? 'powershell' : 'agent',
    accepts: () => !view.ended && !view.disposed && !view.historyView && !modalOpen(),
    focus: () => { setFocus(view.group); terminal.focus(); } };
  inputTargets.set(element, input); view.stopInput = attachTerminalPaste(input, notify);
  // A session reopened by the start keeps its tab where it was: the user's project filter stays.
  if (!background) tabFilter?.reveal(info.cwd);
  const restored = workspaceLayout.views.find(panel => panel.id === info.id);
  if (restored?.resource.kind === 'terminal') { const applied = presentation.applyProfile(terminal, restored.resource.profileId); view.profile = applied.id; element.style.background = applied.background; }
  const shownHere = isOn(workset?.id);
  if (!restored && shownHere) {
    // The running terminal takes the place of the tab that said "not running".
    const stale = threadId ? workspaceLayout.views.find(panel => panel.resource.kind === 'terminal' && panel.resource.threadId === threadId && !views.has(panel.id)) : undefined;
    const staleGroup = stale && tabGroups(workspaceLayout.root).find(candidate => candidate.tabs.includes(stale.id));
    const target = staleGroup?.id ?? (tabGroups(workspaceLayout.root).some(candidate => candidate.id === group) ? group : focusedGroup());
    changeLayout({ type: 'add-view', groupId: target, index: staleGroup ? staleGroup.tabs.indexOf(stale!.id) : undefined,
      view: { id: info.id, resource: { kind: 'terminal', instanceId: instanceRef(info.id), threadId, profileId: view.profile }, hidden: false } });
    // In the background the pane keeps showing what it showed: the new terminal is brought forward only where its tab was the one shown.
    const wasShown = !!staleGroup && staleGroup.activeTabId === stale?.id;
    if (stale) { changeLayout({ type: 'remove-view', viewId: stale.id }); if (!background || wasShown) changeLayout({ type: 'activate', groupId: target, viewId: info.id }); }
  }
  rememberSessions();
  terminal.onData(data => {
    // xterm also emits protocol replies here (e.g. ConPTY's cursor query). They must
    // reach hidden terminals too. DOM focus and the modal own user-input isolation.
    if (view.ended || view.disposed) return;
    void invoke('terminal_write', { id: info.id, data: Array.from(new TextEncoder().encode(data)) }).catch(notify);
  });
  terminal.attachCustomKeyEventHandler(event => {
    if (modalOpen()) return false;
    // Ctrl+V / Ctrl+Shift+V / Shift+Insert paste: xterm must not consume the key (it would send ^V
    // to the harness and cancel the browser's paste event), so the textarea's own paste reaches it.
    if (event.type === 'keydown' && !event.altKey && ((event.ctrlKey && (event.key === 'v' || event.key === 'V')) || (event.shiftKey && event.key === 'Insert'))) return false;
    return !(event.ctrlKey && event.shiftKey && ['T', 't', 'F', 'f', 'ArrowLeft', 'ArrowRight'].includes(event.key));
  });
  render(); if (shownHere && !background) setFocus(view.group);
  void drain(view);
}
async function drain(view: View) {
  try {
    while (!view.disposed) {
      const batch = await invoke<OutputBatch>('terminal_read', { id: view.info.id });
      if (view.disposed) return;
      for (const chunk of batch.chunks) {
        if (view.disposed) return;
        for (const part of splitOutput(new Uint8Array(chunk))) {
          await new Promise<void>(resolve => view.terminal.write(part, resolve));
          view.history.capture();
        }
      }
      if (batch.error) notify(`Terminal output: ${batch.error}`);
      if (batch.eof) { view.ended = true; showExit(view, batch.exit_code); savedAsEnded(view); render(); return; }
      await new Promise(resolve => setTimeout(resolve, 25));
    }
  } catch (error) { if (!view.disposed && !await endedElsewhere(view)) notify(`Terminal connection interrupted: ${error}`); }
}
/** A terminal stopped by something other than this window (another window, an agent's tool, a hop):
 *  its tab says it exited, as when the process ends, instead of reporting an error. */
async function endedElsewhere(view: View): Promise<boolean> {
  if (view.ended) return true;
  try {
    const alive = await invoke<SessionInfo[]>('terminal_list');
    if (alive.some(info => info.id === view.info.id) || view.disposed || view.ended) return view.ended;
  } catch { return false; }
  view.ended = true; showExit(view, null); savedAsEnded(view); render(); return true;
}
/** A terminal that ended by itself, or was stopped from elsewhere, is saved in the layout as not running:
 *  the next start restores its tab without reopening it. Not the ends of a confirmed window close. */
function savedAsEnded(view: View) {
  if (closingWindow) return;
  const next = withoutInstance(workspaceLayout, view.info.id);
  if (next !== workspaceLayout) { workspaceLayout = next; persistence?.update(workspaceLayout); }
}
function showExit(view: View, code: number | null) {
  const banner = document.createElement('div'); banner.className = 'exit-banner'; banner.setAttribute('role', 'status');
  const text = document.createElement('span'); text.textContent = code === null ? 'Process exited' : `Process exited · code ${code}`;
  const restart = document.createElement('button'); restart.type = 'button'; restart.textContent = 'New session';
  restart.onclick = () => openNewSession(view.group, view.info.cwd,
    view.info.shell === 'PowerShell' ? 'powershell' : view.info.shell);
  const close = document.createElement('button'); close.type = 'button'; close.textContent = 'Close terminal';
  close.onclick = () => void stopTerminal(view);
  banner.append(text, restart, close); view.element.append(banner);
}
/** Closes the workspace as the window's close does: preferences saved, the layout released with its running
 * sessions (the next start restores them), terminals ended, window closed. A relaunch starts the new Shinatama
 * at the point of no return, after any question; it waits for this process to end, and if that start is
 * refused the terminals keep running and the window stays. */
async function closeWorkspace(relaunch: boolean): Promise<void> {
  const appearanceSaved = await presentation.flush();
  const saved = persistence?.status().ownership === 'ours' ? await persistence.release() : undefined;
  // Asked only when something would be lost: a release refused while nothing is unsaved (storage
  // read-only, newer schema) loses nothing, and the next start takes the layout over.
  if (!appearanceSaved || (saved && (saved.ok ? saved.dirty : persistence?.status().dirty))) {
    if (!await confirmStop('Close without saving the latest preferences?', 'Previously saved data is preserved. Recent layout or appearance changes could not be saved.')) return;
    await invoke('workspace_allow_close');
  }
  // The point of no return: a restart starts its successor here, never before a question the user may decline.
  if (relaunch) await invoke('app_relaunch');
  // From here the terminals end because the window closes: their tabs stay saved as running (the layout was
  // just released with them so), and the next start restores those sessions with the layout.
  closingWindow = true;
  try { await invoke('terminal_shutdown'); } catch (error) { closingWindow = false; throw error; }
  for (const view of views.values()) view.disposed = true;
  await getCurrentWindow().close();
}
async function confirmStop(title: string, message: string, group?: string, action = 'Stop'): Promise<boolean> {
  // A window close request can arrive over another dialog; the confirmation stacks above it.
  if (confirmDialog.open) return false;
  $('#confirm-title').textContent = title; $('#confirm-message').textContent = message;
  confirmDialog.querySelector<HTMLButtonElement>('button[value="stop"]')!.textContent = action;
  confirmDialog.returnValue = 'cancel';
  // One terminal is confirmed in its own pane, with the rest of the workspace usable.
  // Whatever concerns the whole workspace still takes the page.
  if (!group || !openSheet(confirmDialog, { anchor: () => paneBody(group), placement: 'pane-bottom' })) openModal(confirmDialog);
  confirmDialog.querySelector<HTMLButtonElement>('button[value="cancel"]')?.focus();
  return new Promise(resolve => confirmDialog.addEventListener('close', () => {
    resolve(confirmDialog.returnValue === 'stop'); setFocus(focusedGroup());
  }, { once: true }));
}
async function stopTerminal(view: View) {
  const kept = view.threadId !== null && view.workset !== null;
  if (!view.ended && !await confirmStop(kept ? 'Close session?' : 'Stop terminal?', kept
    ? 'This stops the agent and the programs it started. The session stays in this thread under Closed sessions, where it can be reopened or deleted.'
    : 'This stops the shell and the programs it started.', view.group, kept ? 'Close' : 'Stop')) return;
  try {
    await invoke('terminal_stop', { id: view.info.id });
    discardView(view);
    if (view.threadId && isOn(view.workset?.id)) { await refreshMembers(); await closeSession(view.threadId); sayClosed(view.threadId); }
    setFocus(focusedGroup()); threadSidebar?.changed();
  } catch (error) { notify(error); }
}
/** Closing keeps the session: it leaves the tabs and waits under Closed sessions of its thread.
 * The record of a just-stopped process may still be marked open for a moment, hence the retries. */
async function closeSession(threadId: string) {
  for (let attempt = 0; attempt < 6; attempt++) {
    const row = worksetMembers.find(member => member.id === threadId); if (!row || row.status === 'archived') return;
    try {
      await invoke('thread_archive', { request: { projectId: row.projectId, threadId, expectedRevision: row.revision, archived: true } });
      await refreshMembers(); return;
    } catch { await new Promise(resolve => setTimeout(resolve, 400)); await refreshMembers(); }
  }
  // It stays as a tab that says "not running"; nothing is lost.
}
/** Closing a tab closes its session: it leaves the tabs and waits under Closed sessions of its thread. That
 *  was not visible (2026-11-18: "2 sessions" over one tab, a session the user did not know was closed), so a
 *  close says where the session went and how it comes back. Nothing is said when the close did not happen. */
function sayClosed(threadId: string) {
  const row = worksetMembers.find(member => member.id === threadId);
  if (row?.status !== 'archived') return;
  const owner = memberOf.get(threadId); const thread = owner ? threadTitle(owner) : null;
  const back = row.harness !== null && row.nativeAvailable === true && canResume(row.harness) ? 'Resume' : 'Reopen';
  notify(`${memberName(row)} is closed. It is kept in ${thread ? `the thread "${thread}"` : 'its thread'}, under Closed sessions of its ⋯ menu, where ${back} brings it back.`, 'info');
}
/** A saved session and its thread, also when that thread is off. */
async function sessionRow(threadId: string): Promise<{ row: ThreadSummary; workset: WorksetScope } | null> {
  const known = worksetMembers.find(member => member.id === threadId); const knownSet = memberOf.get(threadId);
  if (known && knownSet) return { row: known, workset: knownSet };
  const workset = await worksetOf(threadId); if (!workset) return null;
  const row = (await worksetThreads(workset.id)).find(member => member.id === threadId);
  return row ? { row, workset } : null;
}
/** Brings a closed session back as a tab of its thread, and resumes it when its agent can (the panel's
 *  Resume): a closed session comes back in one step, the way it is used. One no agent can resume comes
 *  back as a tab that does not run (Reopen). */
async function restoreSession(threadId: string) {
  const found = await sessionRow(threadId); if (!found) return; const { row, workset } = found;
  await invoke('thread_archive', { request: { projectId: row.projectId, threadId, expectedRevision: row.revision, archived: false } });
  // Reopened from a thread that is off: the thread comes on, so the reopened tab is seen.
  if (isOn(workset.id)) await refreshMembers();
  else if (!await showWorkset(workset)) { notify('The session is back in its thread. Turn the thread on to see its tab.', 'info'); return; }
  // Shown as Show shows a session: its project selected above the tabs, never another project's (2026-11-18).
  await focusThreadSession(threadId);
  const resumable = (member: ThreadSummary | undefined): member is ThreadSummary =>
    member?.status === 'inactive' && member.nativeAvailable === true && canResume(member.harness);
  if (!resumable(worksetMembers.find(member => member.id === threadId))) return;
  // One open at a time: resumeSaved returns without opening while another session opens, so this waits for it.
  for (const deadline = Date.now() + 120_000; starting && Date.now() < deadline;) await new Promise(resolve => setTimeout(resolve, 200));
  if (starting) { notify('Another session is still opening. Press Resume on the tab of this one when it is done.', 'info'); return; }
  const latest = worksetMembers.find(member => member.id === threadId);
  if (resumable(latest) && ![...views.values()].some(view => view.threadId === threadId && !view.disposed)) await resumeSaved(latest);
}
/** Deletes one session from Shinatama: a closed one (the sidebar's Delete) or one with nothing saved (Remove). */
async function forgetSession(threadId: string) {
  const row = (await sessionRow(threadId))?.row; if (!row) return;
  await invoke('thread_forget', { request: { projectId: row.projectId, threadId, expectedRevision: row.revision } });
  await refreshMembers();
}
/** Renames one session of a thread; its tabs and its thread's panel take the new name. */
async function renameSession(threadId: string, title: string) {
  const row = (await sessionRow(threadId))?.row; if (!row) return;
  await invoke('thread_rename', { request: { projectId: row.projectId, threadId, expectedRevision: row.revision, title } });
  await refreshMembers();
}
/** Removes a stopped terminal from the window; its saved session is not touched. */
function discardView(view: View) {
  view.disposed = true; clearTimeout(view.resizeTimer);
  view.historyView?.close(); view.minimap.dispose(); view.history.dispose();
  view.stopInput?.(); inputTargets.delete(view.element);
  view.terminal.dispose(); view.element.remove();
  resizeObserver.unobserve(view.element); bindings.delete(view.info.id); views.delete(view.info.id); rememberSessions();
  if (workspaceLayout.views.some(panel => panel.id === view.info.id)) {
    const group = tabGroups(workspaceLayout.root).find(candidate => candidate.tabs.includes(view.info.id));
    const member = view.threadId && isOn(view.workset?.id) ? worksetMembers.find(row => row.id === view.threadId) : undefined;
    if (group && member) changeLayout({ type: 'add-view', groupId: group.id, index: group.tabs.indexOf(view.info.id),
      view: { id: `saved-${member.id}`, resource: { kind: 'terminal', instanceId: null, threadId: member.id, profileId: null }, hidden: false } });
    changeLayout({ type: 'remove-view', viewId: view.info.id });
  }
}
const harnessLabel = (id: string | null) => sessionHarnesses.find(item => item.id === id)?.label ?? id ?? 'Session';
/** The name the user gave the session, or its agent. */
const sessionName = (row: ThreadSummary) => row.title && row.title !== harnessLabel(row.harness) ? row.title : harnessLabel(row.harness);
/** The session a tab without a running terminal stands for. */
function savedSession(panelId: string): ThreadSummary | null {
  if (views.has(panelId)) return null;
  const panel = workspaceLayout.views.find(candidate => candidate.id === panelId);
  const thread = panel?.resource.kind === 'terminal' ? panel.resource.threadId : null;
  return thread ? worksetMembers.find(row => row.id === thread) ?? null : null;
}
const canResume = (harness: string | null) => sessionHarnesses.some(item => item.id === harness && item.nativeResume);
/** The pane of a session that is not running: what it is, why, and Resume when the agent can. While the
 *  start reopens the sessions the saved layout restores as running (reopenRestored), the card of each
 *  says whether its own reopens now or waits its turn, and a waiting one can be left closed. */
function savedSessionCard(panelId: string): HTMLElement | null {
  const saved = savedSession(panelId); if (!saved) return null;
  const card = document.createElement('div'); card.className = 'wm-placeholder session-idle';
  const name = document.createElement('strong'); name.textContent = memberName(saved);
  const resumable = saved.status === 'inactive' && saved.nativeAvailable === true && canResume(saved.harness);
  const said = document.createElement('span');
  const plan = reopenCard(reopenStep(saved, ranAtClose(), canResume), saved.id, { attempted: reopenAttempts, current: reopening, active: reopenActive });
  if (plan !== 'offer') {
    said.textContent = plan === 'reopening' ? 'Reopening…' : 'Waiting to reopen: the sessions of the last close reopen one at a time.';
    said.setAttribute('role', 'status'); card.append(name, said);
    if (plan === 'queued') {
      const skip = document.createElement('button'); skip.type = 'button'; skip.className = 'wm-empty-start'; skip.textContent = 'Don\'t reopen';
      skip.title = 'Leave this session not running; Resume stays available.';
      skip.onclick = () => { settleRestored(panelId); render(); };
      card.append(skip);
    }
    return card;
  }
  said.textContent = resumable ? 'Not running. Its conversation is saved.'
    : saved.status === 'recorded_open' ? 'Marked open by a session that is no longer here.'
    : saved.status === 'unbound' ? 'Nothing was saved in this session.'
    : saved.nativeAvailable !== true ? 'Its saved conversation cannot be reached now.' : `${harnessLabel(saved.harness)} cannot reopen its sessions.`;
  card.append(name, said);
  if (resumable || saved.status === 'recorded_open') {
    const act = document.createElement('button'); act.type = 'button'; act.className = 'wm-empty-start'; act.textContent = resumable ? 'Resume' : 'Recover session';
    act.disabled = starting;
    act.onclick = () => { act.disabled = true; void (resumable ? resumeSaved(saved) : recoverSaved(saved)); };
    card.append(act);
  } else if (saved.status === 'unbound') {
    // Nothing to reopen and nothing to lose: the session can only be removed from its thread.
    const remove = document.createElement('button'); remove.type = 'button'; remove.className = 'wm-empty-start'; remove.textContent = 'Remove';
    remove.title = 'Remove this session from its thread. Nothing else is deleted.';
    remove.onclick = () => { remove.disabled = true; void forgetSession(saved.id).then(() => threadSidebar?.changed(), () => { remove.disabled = false; notify('The session could not be removed. Nothing was changed.'); }); };
    card.append(remove);
  }
  return card;
}
/** The reopen under way (its session), whether the start is still reopening, and the attempts made in this run. */
let reopening: string | null = null; let reopenActive = false;
const reopenAttempts = new Set<string>();
/** The layout and its sessions are restored together (ADR-0005 v4): once the saved layout is back, the
 *  sessions it holds as running are reopened one after the other, in the order of the panes and their
 *  tabs, in the background (each keeps its tab where it was; the tab each pane shows and the focus stay).
 *  A session still marked open by the Shinatama that is gone is recovered first (`thread_recover` verifies
 *  that owner is gone and refuses otherwise), then resumed. Each step is tried once per run; a refusal
 *  keeps its notice and leaves the tab not running with its button. Never beside another open. */
async function reopenRestored() {
  if (reopenActive) return;
  reopenActive = true; render();
  try {
    for (;;) {
      while (starting) await new Promise(resolve => setTimeout(resolve, 200));
      const tabs = restoredRunning(workspaceLayout, panelId => views.has(panelId)); const wanted = new Set(tabs.map(tab => tab.threadId));
      const rowOf = (tab: { panelId: string; threadId: string }) => { const row = savedSession(tab.panelId); return row?.id === tab.threadId ? row : undefined; };
      // A session that cannot reopen by itself (a shell, an agent without native resume) is restored not running, and so saved.
      if (harnessCatalogLoaded) for (const tab of tabs) { const row = rowOf(tab); if (row && !reopenStep(row, wanted, canResume)) settleRestored(tab.panelId); }
      const next = nextToReopen(tabs, tab => { const row = rowOf(tab); return row ? reopenStep(row, wanted, canResume) : undefined; }, reopenAttempts);
      if (!next) return;
      const row = rowOf(next.tab)!;
      reopening = row.id; reopenAttempts.add(attemptKey(row.id, next.step)); render();
      const done = next.step === 'resume' ? await resumeSaved(row, true) : await recoverSaved(row);
      reopening = null;
      // Resumed (its terminal took the tab's place) or refused: the tab no longer says it ran. Recovered: it is resumed next.
      if (!done || next.step === 'resume') settleRestored(next.tab.panelId);
    }
  } finally { reopening = null; reopenActive = false; render(); }
}
/** The sessions the restored layout says were running when Shinatama last closed or crashed, not reopened yet:
 *  a tab saved with its terminal's instance that no terminal of this window holds (session-tabs.ts restoredRunning). */
const ranAtClose = () => new Set(restoredRunning(workspaceLayout, panelId => views.has(panelId)).map(tab => tab.threadId));
/** The tab no longer says its session ran: it is not reopened by itself again, in this run or at a later start. */
function settleRestored(panelId: string) {
  const next = withoutInstance(workspaceLayout, panelId);
  if (next !== workspaceLayout) { workspaceLayout = next; persistence?.update(workspaceLayout); }
}
/** Opens a saved session; false when it was refused (said in a notice) or another open is in flight.
 *  `background`: reopened by the start, it leaves the tab each pane shows and the focus as they are. */
async function resumeSaved(saved: ThreadSummary, background = false): Promise<boolean> {
  const workset = memberOf.get(saved.id) ?? null;
  if (starting || !workset) return false;
  starting = true; $<HTMLButtonElement>('#new-terminal').disabled = true;
  try {
    await namesInFlight;
    const started = await invoke<{ session: SessionInfo; threadId: string }>('thread_resume', {
      request: { threadId: saved.id, projectId: saved.projectId, expectedRevision: saved.revision, rows: 24, cols: 80 },
    });
    attachTerminal(started.session, focusedGroup(), started.threadId, workset, background);
    return true;
  } catch (error) {
    notify(resumeFailureText(error)); return false;
  } finally { endOpening(); starting = false; $<HTMLButtonElement>('#new-terminal').disabled = false; await refreshMembers(); render(); }
}
/** Closes the record a Shinatama that is gone left open; false when that could not be verified (said in a notice). */
async function recoverSaved(saved: ThreadSummary): Promise<boolean> {
  let done = true;
  try { await invoke('thread_recover', { request: { threadId: saved.id, projectId: saved.projectId, expectedRevision: saved.revision } }); }
  catch { done = false; notify('The previous session could not be verified. Its saved conversation is preserved.'); }
  await refreshMembers(); render(); return done;
}
/** The tab's close: the session stops (after asking, if it runs) and moves to Closed sessions. It is never deleted here. */
async function closePanel(id: string) {
  const view = views.get(id);
  if (view) { await stopTerminal(view); return; }
  const saved = savedSession(id);
  if (!saved) { changeLayout({ type: 'remove-view', viewId: id }); return; }
  // Nothing runs here, so nothing is asked: the session moves to Closed sessions and can come back
  // (as a new tab that does not say it ran, so it stays not running). The notice says so.
  await closeSession(saved.id); threadSidebar?.changed(); sayClosed(saved.id);
}
/** Makes a layout hold all and only the sessions of the threads that are on (ADR-0055 v2). Starts nothing. */
function reconciled(layout: WorkspaceLayout): WorkspaceLayout {
  let next = layout;
  const apply = (action: LayoutAction) => { const result = reduceWorkspaceLayout(next, action); if (result.ok) next = result.layout; };
  const members = new Map(worksetMembers.filter(row => row.status !== 'archived').map(row => [row.id, row]));
  const mine = [...views.values()].filter(view => !view.disposed && isOn(view.workset?.id));
  const running = new Set(mine.map(view => view.threadId).filter((id): id is string => id !== null));
  const shown = new Set<string>();
  for (const panel of layout.views) {
    if (panel.resource.kind !== 'terminal') continue;
    const view = views.get(panel.id); const thread = panel.resource.threadId;
    const keep = view ? mine.includes(view) : thread !== null && members.has(thread) && !running.has(thread) && !shown.has(thread);
    if (!keep) { apply({ type: 'remove-view', viewId: panel.id }); continue; }
    if (thread) shown.add(thread);
  }
  const group = () => tabGroups(next.root).find(candidate => candidate.id === next.focusedGroupId)?.id ?? tabGroups(next.root)[0].id;
  for (const row of members.values()) if (!running.has(row.id) && !shown.has(row.id))
    apply({ type: 'add-view', groupId: group(), view: { id: `saved-${row.id}`, resource: { kind: 'terminal', instanceId: null, threadId: row.id, profileId: null }, hidden: false } });
  for (const view of mine) if (!next.views.some(panel => panel.id === view.info.id))
    apply({ type: 'add-view', groupId: group(), view: { id: view.info.id, resource: { kind: 'terminal', instanceId: instanceRef(view.info.id), threadId: view.threadId, profileId: view.profile }, hidden: false } });
  return next;
}
/** Reads the sessions of the threads that are on again and brings the layout in line. */
async function refreshMembers() {
  if (!desktop) return;
  const open = openWorksets;
  try {
    // One read at a time: the store refuses a second one while busy.
    const lists: ThreadSummary[][] = [];
    for (const workset of open) lists.push(await worksetThreads(workset.id));
    if (openWorksets !== open) return;
    worksetMembers = lists.flat(); memberOf.clear();
    lists.forEach((rows, index) => { for (const row of rows) memberOf.set(row.id, open[index]); });
    const next = reconciled(workspaceLayout);
    if (next !== workspaceLayout) { workspaceLayout = next; persistence?.update(workspaceLayout); }
    render();
  } catch { /* the last read stays on screen */ }
  threadSidebar?.repaint();
}
/** Which thread each running terminal belongs to, kept for a frontend reload; and the threads to turn on at the next start.
 *  Which sessions were running is the layout's to say (a tab saved with its instance), so the two are restored together. */
function rememberSessions() {
  try {
    sessionStorage.setItem('shinatama.sessions', JSON.stringify(Object.fromEntries([...views.values()].map(view => [view.info.id, { threadId: view.threadId, workset: view.workset }]))));
    localStorage.setItem('shinatama.threads', JSON.stringify(openWorksets)); localStorage.removeItem('shinatama.thread');
  } catch { /* remembering is a convenience */ }
}
/** Turns a thread on: its sessions join the window's layout beside the others. It starts and stops
 *  nothing itself; a tab the saved layout restores as running reopens its session when shown (savedSessionCard). */
async function showWorkset(target: WorksetScope): Promise<boolean> {
  // Another thread is being turned on or off: this one waits its turn instead of failing.
  for (const deadline = Date.now() + 10000; switching && Date.now() < deadline;) await new Promise(resolve => setTimeout(resolve, 50));
  if (openWorksets.some(workset => workset.id === target.id)) return true;
  if (switching) return false;
  switching = true;
  try {
    try { await worksetThreads(target.id); }
    catch { notify('That thread is unavailable. The window stays as it is.'); return false; }
    openWorksets = [...openWorksets, target];
    await refreshMembers(); rememberSessions(); render();
    setFocus(focusedGroup()); currentWorkset = target.id; threadSidebar?.repaint();
    return true;
  } finally { switching = false; }
}
/** Turns threads off, which closes them without deleting anything (2026-11-18, the owner; ADR-0055 v4):
 *  their tabs leave the window, all at once, and their running sessions stop, after asking. The sessions
 *  stay in the thread, not running, and come back as such (with Resume) when it is turned on again; at a
 *  later start nothing of them reopens by itself. False when a stop failed; a cancel leaves the threads on
 *  and returns true, as nothing went wrong (the caller reads `isOn` when it needs to know). */
async function hideWorksets(ids: readonly string[]): Promise<boolean> {
  if (!openWorksets.some(workset => ids.includes(workset.id))) return true;
  const off = threadsOff([...views.values()], ids);
  if (off.ask) {
    const count = off.stop.length; const several = ids.filter(id => isOn(id)).length > 1;
    if (!await confirmStop(several ? 'Turn the threads off?' : 'Turn the thread off?',
      `This closes ${several ? 'them' : 'it'}: ${count} running session${count === 1 ? '' : 's'} and the programs ${count === 1 ? 'it' : 'they'} started stop. `
      + `The sessions stay in the thread, not running, and can be resumed when it is turned on again. Nothing is deleted.`, undefined, 'Turn off')) return true;
  }
  for (const view of off.stop) {
    try { await invoke('terminal_stop', { id: view.info.id }); }
    catch (error) { if (!await endedElsewhere(view)) { notify(error); return false; } }
  }
  for (const view of off.leave) discardView(view);
  openWorksets = openWorksets.filter(workset => !ids.includes(workset.id)); if (currentWorkset && ids.includes(currentWorkset)) currentWorkset = null;
  for (const id of ids) if (workspaceLayout.views.some(panel => panel.id === teamViewId(id))) closeTeamTab(teamViewId(id));
  await refreshMembers(); rememberSessions(); render(); threadSidebar?.repaint();
  return true;
}
const hideWorkset = (id: string) => hideWorksets([id]);
/** Forgets every session of a thread, then the thread. Refused while one of its sessions runs. A session
 *  still marked open by a Shinatama that is gone (a crash) is recovered first, which verifies that owner is
 *  gone: the forget refuses an open one. Everything is checked before the thread is turned off, so a
 *  refused forget leaves it as it was. */
async function forgetWorkset(thread: Readonly<WorksetSummary>) {
  const mine = [...views.values()].filter(view => view.workset?.id === thread.id && !view.disposed);
  if (mine.some(view => !view.ended)) throw { code: 'owned_elsewhere' };
  let rows = await worksetThreads(thread.id);
  const live = new Set([...views.values()].filter(view => view.threadId && !view.ended && !view.disposed).map(view => view.threadId!));
  if (rows.some(row => live.has(row.id))) throw { code: 'owned_elsewhere' };
  const stale = rowsToRecover(rows, live);
  for (const row of stale) {
    try { await invoke('thread_recover', { request: { threadId: row.id, projectId: row.projectId, expectedRevision: row.revision } }); }
    catch (error) { throw { code: forgetRefusal(error) }; }
  }
  // Recovery raises the revisions the forget checks.
  if (stale.length) rows = await worksetThreads(thread.id);
  await hideWorkset(thread.id);
  for (const view of mine) discardView(view);
  for (const row of rows) await invoke('thread_forget', { request: { projectId: row.projectId, threadId: row.id, expectedRevision: row.revision } });
  await invoke('workset_delete', { request: { worksetId: thread.id, expectedRevision: thread.revision } });
}

const projectPicker = createProjectPicker({ desktop, getPath: () => projectPath,
  onSelected: (path, chosen) => {
    selectedProjectPath = path;
    const changed = path !== projectPath; setProject(path); render();
    if (changed && chosen) void refreshThreadProject();
    if (chosen && pendingStart) pendingStart.projectChosen = true;
  }, onCreate: () => { newProject.open(); }, reportError: notify });
const newProject = createNewProjectDialog({ desktop, onCreated: project => projectPicker.acceptCreated(project),
  prepareOpen: () => projectPicker.prepareForCreation(), notify });
let newSessionGroup: string | null = null;
const recentThreadProjects = new Map<string,string>();
/** Starts one session in a project. Shared by the harness menu and the folder dialog. */
async function startSession(selection: { projectPath: string; harnessId: string; rows: number; cols: number; workset?: WorksetScope | null }) {
  if (!desktop) throw new Error('Open the desktop app to start a session.');
  if (starting) throw new Error('A session is already opening.');
  const button = $<HTMLButtonElement>('#new-terminal');
  starting = true; button.disabled = true; button.setAttribute('aria-busy', 'true');
  try {
    await namesInFlight;
    const selected = await invoke<{id:string;path:string}>('project_select', {path:selection.projectPath});
    // Right after launch the local service and resource measurements are still warming up.
    // Those refusals are temporary: wait and retry instead of making the user press Open again.
    const warmingUp = (failure: unknown) => /still finishing|not yet ready|waiting for healthy resource|Retry when Resources/i.test(
      typeof failure === 'string' ? failure : failure instanceof Error ? failure.message : JSON.stringify(failure ?? ''));
    // The session joins the thread it was asked for, else a thread of its project that is on; otherwise a new thread of its own is turned on.
    // An agent is created in that thread, so a thread whose sessions talk (ADR-0057) gives it its room at launch.
    const wanted = selection.workset ?? activeWorkset();
    const target = wanted && wanted.projectId === selected.id ? wanted : openWorksets.find(candidate => candidate.projectId === selected.id) ?? null;
    let started: {session:SessionInfo;threadId:string} | undefined;
    for (let attempt = 0; !started; attempt++) {
      try {
        started = await invoke<{session:SessionInfo;threadId:string}>(selection.harnessId === 'powershell' ? 'shell_open' : 'agent_open',
          selection.harnessId === 'powershell' ? {agentId:selection.harnessId,cwd:selected.path,rows:selection.rows,cols:selection.cols}
            : {agentId:selection.harnessId,cwd:selected.path,rows:selection.rows,cols:selection.cols,worksetId:target?.id ?? null});
      } catch (failure) {
        if (!warmingUp(failure) || attempt >= 30) throw failure;
        await new Promise(resolve => setTimeout(resolve, 2000));
      }
    }
    setProject(selected.path);
    selectedProjectPath = selected.path;
    threadSidebar?.setProject({projectId:selected.id,label:selected.path.replace(/[\\/]+$/, '').split(/[\\/]/).pop()});
    let workset = target;
    if (workset) {
      try { await invoke('workset_adopt', { request: { worksetId: workset.id, threadId: started.threadId } }); }
      catch { workset = null; }
    }
    if (!workset) {
      workset = await worksetOf(started.threadId);
      if (workset) await showWorkset(workset);
    }
    const group = tabGroups(workspaceLayout.root).some(group => group.id === newSessionGroup) ? newSessionGroup! : focusedGroup();
    attachTerminal(started.session, group, started.threadId, workset); currentWorkset = workset?.id ?? currentWorkset;
    await refreshMembers();
    await refreshThreadProject();
    void threadSidebar?.refresh();
  } finally { endOpening(); starting = false; button.disabled = false; button.removeAttribute('aria-busy'); }
}
const newSession = createNewSession({
  projects: () => desktop ? invoke('project_recent') : Promise.resolve([]),
  harnesses: sessionHarnesses,
  browse: desktop ? async () => {
    const requestId = createRequestId(); await beginMutation(requestId);
    const selected = await browseFolder('project', undefined, requestId);
    return selected.kind === 'project' ? selected.project.path : null;
  } : undefined,
  open: startSession,
  restoreFocus: () => setFocus(focusedGroup()),
  anchor: () => paneBody(newSessionGroup),
});
/** The body of a pane: where a sheet that belongs to that pane is docked. */
function paneBody(groupId: string | null): Element | null {
  const group = tabGroups(workspaceLayout.root).some(group => group.id === groupId) ? groupId! : focusedGroup();
  return [...workspace.querySelectorAll<HTMLElement>('.wm-pane')].find(pane => pane.dataset.groupId === group)?.querySelector('.wm-body') ?? null;
}
const sessionMenu = createSessionMenu({
  start: (harnessId, path) => { void launchSession(path, harnessId, sessionMenuTarget.group, sessionMenuTarget.workset); },
  otherFolder: path => { newSession.setHarnesses(sessionHarnesses); newSessionGroup = sessionMenuTarget.group; newSession.show(path); },
});
let sessionMenuTarget: { path: string; group: string; workset: WorksetScope | null } = { path: '', group: '', workset: null };
/** No dialog: the session starts where it was asked for, and a failure is reported as a notice. */
async function launchSession(path: string, harnessId: string, group: string, workset: WorksetScope | null = null) {
  newSessionGroup = group; sessionMenu.remember(harnessId);
  try { await startSession({ projectPath: path, harnessId, rows: 24, cols: 80, workset }); }
  catch (failure) { notify(typeof failure === 'string' ? failure : failure instanceof Error ? failure.message : 'The session could not be opened. Check the project folder and try again.'); }
}
/** New session: a menu of harnesses under the control that was pressed. A pane's + (`thread`)
 * starts in its thread's project (`workset`, else the focused tab's thread); New session beside the
 * projects (`project`) starts in the project shown there. With both the folder and the harness already
 * known (a restart) it starts at once. */
async function openNewSession(group: string = focusedGroup(), path?: string, harness?: string, scope: NewSessionScope = 'project', workset: WorksetScope | null = null) {
  if (starting || openingSessionDialog) return;
  const pressed = document.activeElement instanceof HTMLElement && document.activeElement.matches('#new-terminal,[data-new-terminal],.wm-add,.wm-empty-start,.thread-sidebar-create,.thread-sidebar-add-session')
    ? document.activeElement : null;
  const selectedProject = threadSidebar?.currentProjectId();
  // The thread's folder: from the project list, else from one of its open sessions.
  const thread = scope === 'thread' ? workset ?? groupWorkset(group) ?? activeWorkset() : null;
  const threadFolder = thread ? recentThreadProjects.get(thread.projectId)
    ?? [...views.values()].find(view => view.workset?.id === thread.id && !view.disposed)?.info.cwd : undefined;
  const selectedPath = newSessionProject(scope, { explicit: path, threadOpen: !!thread,
    openThread: threadFolder,
    sidebar: selectedProject ? recentThreadProjects.get(selectedProject) : undefined,
    selected: selectedProjectPath, current: projectPath });
  openingSessionDialog = true;
  try {
    await loadHarnessCatalog();
    if (!selectedPath && scope === 'thread' && thread) { notify('The folder of this thread\'s project is not available. Use New session beside the projects.', 'info'); return; }
    if (!selectedPath) { notify('Choose a project first: sessions start in the selected project.', 'info'); $('#project').click(); return; }
    $('#notice').hidden = true;
    if (harness && sessionHarnesses.some(candidate => candidate.id === harness)) { await launchSession(selectedPath, harness, group, thread); return; }
    const anchor = pressed?.isConnected ? pressed : $<HTMLElement>('#new-terminal');
    sessionMenuTarget = { path: selectedPath, group, workset: thread };
    // New session beside the projects offers every project listed in the sidebar; a pane's + stays in its thread's project.
    const choices = scope === 'project' ? (threadSidebar?.activeIds() ?? []).map(id => recentThreadProjects.get(id))
      .filter((path): path is string => !!path).map(path => ({ label: folderName(path), path })) : [];
    if (choices.length && !choices.some(choice => samePath(choice.path, selectedPath))) choices.unshift({ label: folderName(selectedPath), path: selectedPath });
    sessionMenu.show(anchor, sessionHarnesses, selectedPath.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || selectedPath, selectedPath, choices);
  } finally { openingSessionDialog = false; }
}
threadSidebar = createWorksetSidebar({
  host: $('#work-area'),
  list: request => desktop ? invoke('workset_list', {request}) : Promise.resolve({rows:[],more:false}),
  create: desktop ? request => invoke('workset_create', {request}) : undefined,
  update: desktop ? request => invoke('workset_update', {request}) : undefined,
  remove: desktop ? forgetWorkset : undefined,
  // Every session of the thread, named as its tab is; whether one runs is asked again at each paint.
  sessions: desktop ? async worksetId => threadSessions(await worksetThreads(worksetId), sessionName, new Set(),
    row => row.harness !== null && row.nativeAvailable === true && canResume(row.harness)) : undefined,
  sessionRunning: threadId => [...views.values()].some(view => view.threadId === threadId && !view.ended && !view.disposed),
  onShowSession: focusThreadSession, onRenameSession: renameSession,
  onRestore: restoreSession, onForgetSession: forgetSession,
  // A project leaves the sidebar: its threads go off with it, so no tab is left without a row to bring it back.
  // Turning them off closes their running sessions, after asking: kept on (cancelled, or a stop failed), the project comes back to the sidebar.
  onProjectsRemoved: projectIds => { void (async () => {
    await hideWorksets(openWorksets.filter(workset => projectIds.includes(workset.projectId)).map(workset => workset.id));
    for (const id of projectIds) if (openWorksets.some(workset => workset.projectId === id) && !threadSidebar?.activeIds().includes(id)) threadSidebar?.toggleActive(id);
  })(); },
  harnessLabel,
  onOpen: thread => showWorkset({ id: thread.id, projectId: thread.projectId }),
  onClose: thread => hideWorkset(thread.id),
  isOn: id => openWorksets.some(workset => workset.id === id),
  tone: toneOf,
  // + on a thread: that thread is turned on, then the agent menu hangs from the + that was pressed.
  onAddSession: (thread, anchor) => { void (async () => {
    const scope = { id: thread.id, projectId: thread.projectId };
    if (!await showWorkset(scope)) return;
    if (anchor.isConnected) anchor.focus();
    await openNewSession(focusedGroup(), undefined, undefined, 'thread', scope);
  })(); },
  chat: desktop ? {
    enabled: id => chatThreads.has(id),
    unread: id => chatUnread(id),
    set: (thread, enabled) => setThreadChat(thread, enabled),
    open: thread => { threadChat?.open({ id: thread.id, projectId: thread.projectId }); threadSidebar?.repaint(); },
  } : undefined,
  team: desktop ? { open: thread => openTeamTab({ id: thread.id, projectId: thread.projectId }) } : undefined,
  openId: () => activeWorkset()?.id ?? null,
  running: id => [...views.values()].filter(view => view.workset?.id === id && !view.ended && !view.disposed).length,
  // Never while a session starts: a name read and a launch share the same worker capacity.
  names: desktop ? threadIds => {
    if (starting) return Promise.reject(new Error('busy'));
    const call = invoke('thread_names', { threadIds }); namesInFlight = call.catch(() => undefined); return call;
  } : undefined,
  loadState: desktop ? async () => {
    const snapshot = await invoke<{revision:number;settings:ThreadSidebarState}>('thread_sidebar_load');
    sidebarRevision = snapshot.revision; return snapshot.settings;
  } : undefined,
  saveState: desktop ? async settings => {
    const snapshot = await invoke<{revision:number;settings:ThreadSidebarState}>('thread_sidebar_save', {request:{expectedRevision:sidebarRevision,settings}});
    sidebarRevision = snapshot.revision;
  } : undefined,
  reportError: notify,
  onModeChange: mode => { $('#threads-toggle').setAttribute('aria-pressed', String(mode !== 'hidden')); },
});
$('#work-area').prepend(threadSidebar.element);
// ADR-0057: the conversation of a thread whose sessions talk to each other, beside the terminals.
threadChat = createThreadChat({
  invoke: (command, args) => invoke(command, args),
  notify,
  title: thread => threadTitle(thread) ?? 'Thread',
  tone: thread => toneOf(thread.id),
  harnessLabel: id => harnessLabel(id),
  focusSession: id => { void focusThreadSession(id); },
  seen: (id, eventId, timestamp) => { markChatSeen(id, eventId, timestamp); threadSidebar?.repaint(); },
  restoreFocus: () => { setFocus(focusedGroup()); },
});
/** Shows a session of a thread: its thread comes on if it is off, then its tab is brought forward. It starts
 *  nothing itself; a "not running" tab shown this way reopens only if the saved layout restored it as running. */
async function focusThreadSession(threadId: string) {
  const running = [...views.values()].find(view => view.threadId === threadId && !view.disposed);
  if (running) { focusAttentionSession(running.info.id); return; }
  const workset = await worksetOf(threadId);
  if (workset && !isOn(workset.id) && !await showWorkset(workset)) return;
  const panel = workspaceLayout.views.find(candidate => candidate.resource.kind === 'terminal' && candidate.resource.threadId === threadId);
  const group = panel && tabGroups(workspaceLayout.root).find(candidate => candidate.tabs.includes(panel.id));
  if (panel && group) {
    // A tab of another project than the filtered one must not stay hidden, nor another project stay
    // selected above it: a folder that is not known clears the filter (it shows every project).
    tabFilter?.reveal(panelFolder(panel.id) ?? '');
    if (panel.hidden) changeLayout({ type: 'hide', viewId: panel.id, hidden: false });
    if (group.collapsed) changeLayout({ type: 'collapse', groupId: group.id, collapsed: false });
    if (workspaceLayout.maximizedGroupId !== null && workspaceLayout.maximizedGroupId !== group.id) changeLayout({ type: 'maximize', groupId: null });
    changeLayout({ type: 'activate', groupId: group.id, viewId: panel.id }); setFocus(group.id, false);
  }
  else notify('That session has no tab in the window.', 'info');
}
// The thread sidebar can be closed altogether; this button brings it back as it was (full or compact).
$('#threads-toggle').onclick = () => {
  if (!threadSidebar) return;
  threadSidebar.setMode(threadSidebar.mode() === 'hidden' ? (threadSidebar.compactWhenShown() ? 'compact' : 'open') : 'hidden');
};
async function loadHarnessCatalog(): Promise<boolean> {
  if (!desktop || harnessCatalogLoaded) return true;
  if (harnessCatalogLoading) return harnessCatalogLoading;
  const loading = (async () => {
    try {
      const value = await invoke<unknown>('harness_catalog');
      if (!Array.isArray(value) || value.length > 31) throw new Error('Invalid harness catalog');
      const seen = new Set<string>(); const received: HarnessCapabilities[] = [];
      for (const item of value) {
        if (!item || typeof item !== 'object' || Array.isArray(item)
          || typeof item.id !== 'string' || !/^[a-z0-9_]{1,64}$/.test(item.id) || item.id === builtinHarness.id || seen.has(item.id)
          || typeof item.label !== 'string' || !item.label.length || item.label.length > 128 || /[\u0000-\u001f\u007f-\u009f]/.test(item.label)
          || typeof item.nativeResume !== 'boolean') throw new Error('Invalid harness catalog');
        seen.add(item.id); received.push({ id: item.id, label: item.label, nativeResume: item.nativeResume });
      }
      sessionHarnesses = [...received, builtinHarness]; harnessCatalogLoaded = true;
      newSession.setHarnesses(sessionHarnesses); threadSidebar?.repaint(); render(); return true;
    } catch {
      notify('Agent choices could not be loaded. Open New session to try again.'); return false;
    }
  })();
  harnessCatalogLoading = loading;
  try { return await loading; }
  finally { if (harnessCatalogLoading === loading) harnessCatalogLoading = null; }
}
const sessionRaw = createSessionRaw({
  read: request => desktop ? invoke('session_raw', {request}) : Promise.reject({issue:'read_unavailable'}),
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
// Progress of a long search or export (the worker reports at most four times a second) and its cancel.
const sessionProgress: ProgressBackend = {
  subscribe: handler => {
    if (!desktop) return () => {};
    let stopped = false; let off: (() => void) | null = null;
    void listen<ProgressEvent>('session-progress', event => { if (!stopped) handler(event.payload); })
      .then(unlisten => { if (stopped) unlisten(); else off = unlisten; }).catch(() => {});
    return () => { stopped = true; off?.(); };
  },
  cancel: requestId => desktop ? invoke('session_cancel', { requestId }) : Promise.resolve(false),
};
const sessionFind = createSessionFind({
  progress: sessionProgress,
  read: (request, requestId) => desktop ? invoke('session_find', {request, requestId}) : Promise.reject({issue:'read_unavailable'}),
  onSelectHit: hit => { sessionDetail.show(hit.root, hit.nativeId, {offset:hit.eventOrdinal,fingerprint:hit.fingerprint}); },
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
const sessionExport = createSessionExport({
  progress: sessionProgress,
  save: (request, requestId) => desktop ? invoke('session_export', { request, requestId }) : Promise.reject({ issue: 'read_unavailable' }),
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
function screenText(view: View): string {
  const buffer = view.terminal.buffer.active; const lines: string[] = [];
  for (let index = 0; index < buffer.length; index++) lines.push(buffer.getLine(index)?.translateToString(true) ?? '');
  return lines.join('\n');
}
async function decide(title: string, message: string, accept: string): Promise<boolean> {
  const button = confirmDialog.querySelector<HTMLButtonElement>('button[value="stop"]')!; const label = button.textContent;
  button.textContent = accept; button.classList.remove('danger');
  try { return await confirmStop(title, message); } finally { button.textContent = label; button.classList.add('danger'); }
}
/** C28: open the destination, paste the exact previewed bootstrap, wait for `ready <nonce>`. */
async function runHop(preview: HopPreview, progress: (message: string) => void, release: () => void): Promise<string> {
  progress(`Opening ${preview.destination}…`);
  let launched: { hopId: string; nonce: string; started: { session: SessionInfo; threadId: string } } | undefined;
  for (let attempt = 0; !launched; attempt++) {
    try { launched = await invoke('hop_launch', { request: { hopId: preview.hop_id, rows: 30, cols: 110 } }); }
    catch (failure) {
      const text = typeof failure === 'string' ? failure : JSON.stringify(failure ?? '');
      if (!/still finishing|not yet ready|waiting for healthy resource|Retry when Resources/i.test(text) || attempt >= 30) throw text;
      await new Promise(resolve => setTimeout(resolve, 2000));
    }
  }
  await attachSession(launched.started.session, launched.started.threadId);
  const view = views.get(launched.started.session.id);
  if (!view) throw 'The destination pane could not be attached.';
  const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
  const trust = /trust (this|the files in this) folder|do you trust/i;
  progress('Waiting for the destination to be ready for input…');
  let last = ''; let stableSince = Date.now(); let released = false;
  for (const deadline = Date.now() + 120000; ; await sleep(250)) {
    if (view.ended) throw 'The destination exited before the seed was sent; the source is untouched.';
    const now = screenText(view);
    if (trust.test(now)) {
      if (!released) { released = true; release(); notify(`Answer ${preview.destination}'s folder question in the new pane; the hop continues after it.`, 'info'); }
      stableSince = Date.now(); last = now; continue;
    }
    if (now !== last) { last = now; stableSince = Date.now(); }
    if (now.trim() && Date.now() - stableSince > 2000) break;
    if (Date.now() > deadline) { await invoke('hop_finish', { hopId: launched.hopId, outcome: 'fail' }); throw 'The destination never became ready for input; the hop failed and its pane stays open.'; }
  }
  await invoke('hop_deliver', { hopId: launched.hopId });
  progress('Seed sent. Waiting for the destination to confirm…');
  const receipt = new RegExp(`(^|[^\`])ready ${launched.nonce}(?!\`)`);
  const seen = () => screenText(view).split('\n').some(line => !/reply exactly/i.test(line) && receipt.test(line));
  for (const deadline = Date.now() + 90000; Date.now() < deadline && !view.ended; await sleep(500)) {
    if (seen()) {
      await invoke('hop_finish', { hopId: launched.hopId, outcome: 'receipt' });
      setFocus(view.group);
      return `Hop committed: ${preview.destination} confirmed the hand-off. The source session is untouched.`;
    }
  }
  const keep = !view.ended && await decide('No confirmation from the destination', `${preview.destination} did not reply "ready ${launched.nonce}" within 90 seconds. Keep it as the new session anyway, or mark the hop failed? Its pane stays open either way.`, 'Keep');
  await invoke('hop_finish', { hopId: launched.hopId, outcome: keep ? 'accept_without_receipt' : 'fail' });
  return keep ? 'Hop committed without confirmation.' : 'Hop marked failed; the destination pane stays open.';
}
const filesPanel = createFilesPanel({
  invoke: (command, args) => desktop ? invoke(command, args) : Promise.reject('Open the desktop app to browse files.'),
  project: () => projectPath || null,
  reportError: notify,
  decide: (title, message, accept) => decide(title, message, accept),
  restoreFocus: () => setFocus(focusedGroup()),
  openInBrowser: desktop ? path => { void openBrowserTab({ file: path }); } : undefined,
});
{
  const button = document.createElement('button'); button.type = 'button'; button.id = 'open-files'; button.textContent = 'Files';
  button.title = 'Browse and edit the project files (nothing is run)';
  button.onclick = () => { void filesPanel.show(); };
  document.querySelector('#new-terminal')?.before(button);
}
// What agent sessions create on disk (Xiox), docked right of the workspace and always in view.
if (desktop) {
  const leftovers = createLeftoversPanel({
    invoke: (command, args) => invoke(command, args),
    focusSession: id => focusAttentionSession(id),
    reportError: notify,
  });
  $('#work-area').append(leftovers.element);
  document.querySelector('#new-terminal')?.before(leftovers.toggle);
}
const projectsOverview = createProjectsOverview({
  invoke: (command, args) => desktop ? invoke(command, args) : Promise.reject('Open the desktop app to manage projects.'),
  newSession: path => { void openNewSession(focusedGroup(), path); },
  resume: async (row, thread) => {
    if (starting) throw 'A session is already opening.';
    starting = true;
    try {
      await namesInFlight;
      const started = await invoke<{ session: SessionInfo; threadId: string }>('thread_resume', {
        request: { threadId: thread.id, projectId: row.id, expectedRevision: thread.revision, rows: 24, cols: 80 },
      }).catch(error => { throw new Error(resumeFailureText(error)); });
      await attachSession(started.session, started.threadId);
    } finally { endOpening(); starting = false; }
  },
  decide: (title, message, accept) => decide(title, message, accept),
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
{
  // "Projects" is the menu on the left; this one is the overview of pinned projects.
  const button = document.createElement('button'); button.type = 'button'; button.id = 'open-projects'; button.textContent = 'Overview';
  button.title = 'Pinned projects, Git status, last thread, new session or worktree';
  button.onclick = () => { void projectsOverview.show(); };
  document.querySelector('#open-files')?.before(button);
}
const unavailable = () => Promise.reject({ issue: 'read_unavailable' });
// One panel for every session, running or saved. The archive browser and the search index stay
// behind it for what it does not show (explicit paging of one archive, index management).
const sessionsPanel = createSessionsPanel({
  invoke: (command, args) => desktop ? invoke(command, args) : Promise.reject('Open the desktop app to see sessions.'),
  focus: id => {
    const view = views.get(id); if (!view || view.disposed) return false;
    changeLayout({ type: 'activate', groupId: view.group, viewId: id }); setFocus(view.group); return true;
  },
  threadOf: id => { const panel = workspaceLayout.views.find(p => p.id === id); return panel?.resource.kind === 'terminal' ? panel.resource.threadId ?? null : null; },
  roots: () => desktop ? invoke('session_roots') : unavailable(),
  list: request => desktop ? invoke('session_list', { request }) : Promise.reject({ issue: 'worker_unavailable' }),
  search: request => desktop ? invoke('session_search', { request }) : unavailable(),
  openDetail: (root, nativeId) => { sessionDetail.show(root, nativeId); },
  openArchives: () => { sessionLibrary.show(); },
  openIndex: () => { sessionSearch.show(); },
  openSettings: () => { settingsPanel.show('sessions'); },
  project: () => projectPath || null,
  restoreFocus: () => setFocus(focusedGroup()),
});
{
  const button = document.createElement('button'); button.type = 'button'; button.id = 'open-sessions'; button.textContent = 'Sessions';
  button.title = 'Every session: the ones running now and the ones saved';
  button.onclick = () => { void sessionsPanel.show(); };
  document.querySelector('#open-projects')?.before(button);
}
// Every preference in one drawer. Each section keeps its owner's saving: presentation.ts for the
// theme and terminals, attention-control.ts for alerts, desktop-shell.ts for the zoom.
const settingsPanel = createSettingsPanel({
  desktop, invoke: (command, args) => invoke(command, args), notify, restoreFocus: () => setFocus(focusedGroup()),
  appearance: presentation, alerts: attentionControl,
  zoom: { levels: ZOOM_STEPS, current: interfaceZoom, available: canZoom, set: changeZoom, subscribe: onInterfaceZoom },
});
{
  const button = document.createElement('button'); button.type = 'button'; button.id = 'open-settings'; button.textContent = 'Settings';
  button.title = 'Appearance, agents, memory, alerts, GitHub, environment variables and sessions';
  button.onclick = () => { settingsPanel.show(); };
  document.querySelector('#open-projects')?.before(button);
}
const hopPreview = createHopPreview({
  prepare: request => desktop ? invoke('hop_preview', { request }) : Promise.reject({ issue: 'read_unavailable' }),
  send: desktop ? runHop : undefined,
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
const sessionDetail = createSessionDetail({
  read: request => desktop ? invoke('session_detail', { request }) : Promise.reject({ issue: 'read_unavailable' }),
  capabilities: desktop ? nativeId => invoke('sidecar_capabilities', { nativeId }) : undefined,
  openRaw: selection => {sessionRaw.show(selection);},
  openFind: request => {sessionFind.show(request);},
  openExport: selection => { sessionExport.show(selection); },
  openHop: selection => { hopPreview.show(selection); },
  back: { label: 'Sessions', open: () => { void sessionsPanel.show(); } },
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
const sessionLibrary = createSessionLibrary({
  roots: () => desktop ? invoke('session_roots') : Promise.reject({ issue: 'read_unavailable' }),
  list: request => desktop ? invoke('session_list', { request }) : Promise.reject({ issue: 'worker_unavailable' }),
  addArchive: desktop ? request => invoke('session_archive_add', { request }) : undefined,
  openDetail: (root, nativeId) => { sessionDetail.show(root, nativeId); },
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
const sessionSearch = createSessionSearch({
  search: request => desktop ? invoke('session_search', { request }) : unavailable(),
  indexStatus: () => desktop ? invoke('session_index_status') : unavailable(),
  indexStart: () => desktop ? invoke('session_index_start') : unavailable(),
  indexCancel: () => desktop ? invoke('session_index_cancel') : unavailable(),
  indexForget: () => desktop ? invoke('session_index_forget', { storeRootId: null }) : unavailable(),
  openDetail: (root, nativeId) => { sessionDetail.show(root, nativeId); },
  openSettings: () => { settingsPanel.show('sessions'); },
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
function openProjectDialog() { projectPicker.open(); }
async function chooseProject(path: string) {
  const selected = await invoke<{ id: string; path: string }>('project_select', { path });
  selectedProjectPath = selected.path;
  const changed = selected.path !== projectPath; setProject(selected.path); render();
  if (changed) await refreshThreadProject();
}
organizeToolbar();
// Counts tabs, not running sessions: an exited tab, and a "not running" one (after a restart, all of
// them), still sits in the strip under its project. The tabs on screen: the sessions of the threads that are on.
tabFilter = createTabFilter({ tabs: () => workspaceLayout.views.filter(panel => panel.resource.kind === 'terminal')
  .map(panel => panelFolder(panel.id)).filter((folder): folder is string => folder !== null), onChange: filterTabs });
tabFilter.paint();
createProjectMenu({
  button: $('#project'),
  load: () => desktop ? invoke('project_recent') : Promise.resolve([]),
  search: desktop ? onResults => createCatalogClient(snapshot => onResults(snapshot.projects), () => onResults([])) : undefined,
  current: () => projectPath,
  active: () => new Set(threadSidebar?.activeIds() ?? []),
  toggleActive: id => { threadSidebar?.toggleActive(id); },
  running: () => [...views.values()].filter(view => !view.ended && !view.disposed).map(view => view.info.cwd),
  choose: chooseProject,
  browse: () => { projectPicker.open(); void projectPicker.browse(); },
  create: () => { newProject.open(); },
  manage: openProjectDialog,
  reportError: notify,
});
function settlePendingStart() {
  if (modalOpen()) return;
  const request = pendingStart; pendingStart = null;
  if (request?.projectChosen) void startTerminal(request.group);
}
for (const dialog of ['#project-dialog', '#new-project-dialog']) {
  $(dialog).addEventListener('close', settlePendingStart);
}
confirmDialog.addEventListener('close', () => {
  // A selection may finish under this overlay. Cancel resumes the original
  // request once; confirming shutdown must never create another terminal.
  if (confirmDialog.returnValue === 'stop') pendingStart = null;
  else settlePendingStart();
});
$('#new-terminal').onclick = () => openNewSession();
$('#restore').onclick = () => { for (const view of workspaceLayout.views.filter(view => view.hidden)) changeLayout({ type: 'hide', viewId: view.id, hidden: false }); setFocus(focusedGroup()); };
$('#layout-toggle').onclick = () => {
  preferredAxis = preferredAxis === 'columns' ? 'rows' : 'columns';
  if (workspaceLayout.root.kind === 'split') {
    const result = parseWorkspaceLayout({ ...workspaceLayout, root: { ...workspaceLayout.root, axis: preferredAxis } });
    if (result.ok) { workspaceLayout = result.value; persistence?.update(workspaceLayout); render(); }
  }
};
document.addEventListener('keydown', event => {
  if (projectPicker.handleKey(event)) return;
  if (newProject.handleKey(event)) return;
  const target = event.target instanceof HTMLElement ? event.target : null;
  const terminalInput = !!target?.closest('.xterm');
  const textInput = !!target?.closest('input,textarea,select,[contenteditable="true"]');
  if (!modalOpen() && !terminalInput && !textInput && !event.ctrlKey && !event.altKey && !event.metaKey) {
    // A character key acts only while its control has focus, never on stray focus.
    if (event.key === '/' && target?.closest('#project')) { event.preventDefault(); projectPicker.open(true); return; }
    if (event.key === 'F3') { event.preventDefault(); projectPicker.open(); void projectPicker.browse(); return; }
  }
  if (modalOpen() || !event.ctrlKey || !event.shiftKey) return;
  if (event.key.toLowerCase() === 'f') { event.preventDefault(); terminalFind.open(); }
  if (event.key.toLowerCase() === 't') { event.preventDefault(); openNewSession(); }
  if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
    event.preventDefault(); const neighbor = keyboardNeighbor(workspaceLayout, focusedGroup(), event.key === 'ArrowLeft' ? 'left' : 'right', { width: workspace.clientWidth, height: workspace.clientHeight }); if (neighbor) setFocus(neighbor);
  }
});
async function initialize() {
  render(); setFocus(focusedGroup(), false);
  if (!desktop) {
    $('#runtime-status').textContent = 'Browser preview · terminals require the desktop app';
    return;
  }
  try {
    await loadHarnessCatalog();
    // ADR-0057: which threads talk, and the marks of their rooms; read again while the window is shown.
    try { chatThreads = new Set(await invoke<string[]>('thread_chat_list')); } catch { chatThreads = new Set(); }
    void readChatOverview().then(() => threadSidebar?.repaint());
    window.setInterval(() => { if (!document.hidden) void readChatOverview(); }, 10000);
    const stopCollaborationUpdates = await listen<{started:{session:SessionInfo;threadId:string};harness:string;mode:string;joinedParentThread?:boolean}>(
      'collaboration-session-opened', ({ payload }) => {
        const started = payload.started;
        if (!started?.session?.id || !started.threadId) return;
        // The backend has already moved the session into the thread of the one that asked
        // for it (ADR-0055), so attachSession finds that thread from the saved record.
        void attachSession(started.session, started.threadId);
        void threadSidebar?.refresh();
        const agent = ({ codex: 'Codex', opencode: 'OpenCode', pi: 'Pi' } as Record<string, string>)[payload.harness] ?? 'Claude';
        notify(payload.joinedParentThread ? `${agent} session opened in the same thread.` : `${agent} session opened.`, 'info');
      });
    window.addEventListener('pagehide', () => { stopCollaborationUpdates(); }, { once: true });
    const stopOpeningSteps = await listen<{ harness: string; step: string }>('agent-opening', ({ payload }) => {
      if (starting && typeof payload?.harness === 'string' && typeof payload.step === 'string') showOpening(payload.harness, payload.step);
    }).catch(() => null);
    window.addEventListener('pagehide', () => { stopOpeningSteps?.(); }, { once: true });
    const stopThreadUpdates = await listen<unknown>('thread-changed', ({ payload }) => {
      void payload; threadSidebar?.changed(); void refreshMembers();
    }).catch(() => null);
    window.addEventListener('pagehide', () => { stopThreadUpdates?.(); }, { once: true });
    await attentionControl.initialize();
    await listen<{sessionId: string | null}>('attention-focus', ({ payload }) => {
      if (!payload.sessionId) return;
      focusAttentionSession(payload.sessionId);
    });
    // A page an agent opened through its browser tools becomes a tab the user can watch and take over.
    await listen<{ page: string; instance: string }>('browser-agent-page', ({ payload }) => {
      if (!browserTabs.viewOf(payload.page)) void openBrowserTab({ page: payload.page });
    });
    await listen<{ page: string }>('browser-page-closed', ({ payload }) => {
      const id = browserTabs.viewOf(payload.page); if (!id) return;
      bindings.delete(id); void browserTabs.close(id); changeLayout({ type: 'remove-view', viewId: id });
    });
    setProject(await invoke<string>('project_path'));
    // A development frontend reload reattaches existing processes; it never starts a new one.
    const existing = await invoke<SessionInfo[]>('terminal_list');
    let known: Record<string, { threadId: string | null; workset: WorksetScope | null }> = {};
    try { known = JSON.parse(sessionStorage.getItem('shinatama.sessions') ?? '{}'); } catch { /* reattached as loose terminals */ }
    for (const info of existing) attachTerminal(info, focusedGroup(), known[info.id]?.threadId ?? null, known[info.id]?.workset ?? null);
    if (existing.length) notify('Reconnected to running terminals. Earlier screen contents are not retained across a frontend reload.', 'info');
    await listen('confirm-close', async () => {
      if ((views.size || pendingStart || modalOpen()) && !await confirmStop('Close workspace?', 'All running terminals and their child programs will stop.')) return;
      try { await closeWorkspace(false); } catch (error) { notify(error); }
    });
    await initializeWindowControls(notify);
    await initializeDesktopShell({ notify, beforeReload: () => Promise.all([presentation.flush(), persistence?.flush(1500)]) });
    const stopResources = startTerminalResources(statuses => {
      terminalMetrics.clear();
      for (const status of statuses ?? []) terminalMetrics.set(status.id, status);
      for (const pane of workspace.querySelectorAll<HTMLElement>('.wm-pane')) {
        const group = tabGroups(workspaceLayout.root).find(group => group.id === pane.dataset.groupId);
        const measured = group?.activeTabId && terminalMetrics.get(group.activeTabId);
        const footer = pane.querySelector<HTMLElement>('.wm-footer');
        const label = footer?.querySelector<HTMLElement>('.terminal-resources');
        if (!footer || !label) continue;
        if (measured) { const formatted = formatTerminalResources(measured); label.textContent = formatted.summary; footer.title = formatted.detail; }
        else if (group?.activeTabId) { label.textContent = 'Measurements unavailable'; footer.title = 'Resource measurements unavailable; native limits remain applied.'; }
      }
    });
    window.addEventListener('pagehide', stopResources, { once: true });
    initializeMaintenance(notify);
    await presentation.initialize();
    $('#runtime-status').innerHTML = '<i></i>Local runtime · experimental';
    appLog('page', 'The main window loaded.');
    render();
    try {
      const storage = await invoke<{ directory: string; database_path: string; read_only: boolean; message: string | null }>('storage_location');
      $('#runtime-status').title = `Private data: ${storage.directory}\nDatabase: ${storage.database_path}`;
      if (storage.read_only) {
        $('#runtime-status').textContent = 'Read-only storage · update required';
        notify(storage.message || 'Update Shinatama to modify this database.');
      }
    } catch (error) {
      $('#runtime-status').textContent = 'Storage unavailable';
      notify(error);
    }
    // Window controls stay usable while missing/removable paths are checked.
    await projectPicker.restore();
    await refreshThreadProject();
    // Turn on the threads that were on, before the saved layout loads, so their tabs keep their panes.
    // One that no longer exists is left out.
    try {
      const last: unknown = JSON.parse(threadsToReopen ?? '[]');
      for (const item of Array.isArray(last) ? last.slice(0, 32) : []) {
        if (item && /^w_[a-z2-7]{12}$/.test(item.id) && /^p_[a-z2-7]{12}$/.test(item.projectId)) await showWorkset({ id: item.id, projectId: item.projectId });
      }
    } catch { /* nothing to reopen */ }
    await connectWorkspace();
    // The saved panes are back, and with them the sessions they held as running (reopenRestored).
    void reopenRestored();
  } catch (error) { $('#runtime-status').textContent = 'Runtime unavailable'; notify(error); }
}
void initialize();
