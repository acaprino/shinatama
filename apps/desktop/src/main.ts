import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { emptyWorkspace, parseWorkspaceLayout, reduceWorkspaceLayout, tabGroups, keyboardNeighbor, type WorkspaceLayout, type LayoutAction } from './workspace-model.ts';
import { createWorkspaceRenderer } from './workspace-renderer.ts';
import { createTauriWorkspaceTransport, createWorkspacePersistence, type WorkspacePersistence } from './workspace-persistence.ts';
import './workspace-renderer.css';
import { initializeWindowControls } from './window-controls.ts';
import { createProjectPicker } from './project-picker.ts';
import { createNewProjectDialog } from './new-project.ts';
import { initializeMaintenance } from './maintenance.ts';
import { openModal } from './modal.ts';
import { createTerminalFind } from './terminal-find.ts';
import { createPresentation, PROFILES } from './presentation.ts';
import './presentation.css';
import { startTerminalResources, formatTerminalResources, type TerminalResourceStatus } from './terminal-resources.ts';
import { createAttentionControl } from './attention-control.ts';
import { createNewSession } from './new-session.ts';
import { beginMutation, browseFolder, createRequestId } from './project-catalog.ts';
import { createSessionLibrary } from './session-library.ts';
import { createSessionDetail } from './session-detail.ts';
import { createSessionRaw } from './session-raw.ts';
import { createSessionFind } from './session-find.ts';
import { createThreadSidebar, type ThreadSidebarState } from './thread-sidebar.ts';
import './styles.css';

type SessionInfo = { id: string; cwd: string; shell: string };
type HarnessCapabilities = { id: string; label: string; nativeResume: boolean };
type OutputBatch = { chunks: number[][]; eof: boolean; exit_code: number | null; error: string | null };
type View = {
  info: SessionInfo; terminal: Terminal; fit: FitAddon; element: HTMLDivElement;
  group: string; hidden: boolean; visible: boolean; ended: boolean; disposed: boolean; profile: string;
  lastSize: string; resizeTimer: number; resizeChain: Promise<void>;
};
type Tone = 'info' | 'error';
const $ = <T extends HTMLElement = HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const desktop = isTauri();
const builtinHarness: HarnessCapabilities = { id: 'powershell', label: 'PowerShell', nativeResume: false };
let sessionHarnesses: HarnessCapabilities[] = [builtinHarness];
let harnessCatalogLoaded = false;
let harnessCatalogLoading: Promise<boolean> | null = null;
let openingSessionDialog = false;
const views = new Map<string, View>();
const profiles = PROFILES;
let workspaceLayout = emptyWorkspace();
let persistence: WorkspacePersistence | null = null;
let scopeRevision = 0;
let preferredAxis: 'columns' | 'rows' = 'columns';
const bindings = new Map<string, HTMLElement>();
const terminalMetrics = new Map<string, TerminalResourceStatus>();
let projectPath = '';
let selectedProjectPath = '';
let threadSidebar: ReturnType<typeof createThreadSidebar> | null = null;
let sidebarRevision = 0;
let starting = false;
/** An Open terminal waiting for a project choice; a dismissed picker forgets it. */
let pendingStart: { group: string; projectChosen: boolean } | null = null;
let pendingAttentionFocus: string | null = null;
const workspace = $('#workspace');
const confirmDialog = $<HTMLDialogElement>('#confirm-dialog');

function notify(message: unknown, tone: Tone = 'error') {
  const notice = $('#notice');
  notice.dataset.tone = tone;
  notice.setAttribute('role', tone === 'error' ? 'alert' : 'status');
  $('#notice span').textContent = String(message);
  notice.hidden = false;
}
$('#notice button').onclick = () => { $('#notice').hidden = true; };
const attentionControl = createAttentionControl(desktop, notify);
function modalOpen() { return !!document.querySelector('dialog[open]'); }
function focusAttentionSession(id: string) {
  if (modalOpen()) { pendingAttentionFocus = id; return; }
  pendingAttentionFocus = null;
  const view = views.get(id);
  const group = tabGroups(workspaceLayout.root).find(candidate => candidate.tabs.includes(id));
  if (!view || !group) { notify('This terminal is no longer in the workspace.', 'info'); return; }
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
  $('#project-name').textContent = projectPath.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || 'Choose project';
  $('#project').title = projectPath || 'Choose project folder';
}
const presentation = createPresentation({ desktop, notify, onChange() {
  for (const view of views.values()) {
    const applied = presentation.applyProfile(view.terminal, view.profile); view.profile = applied.id; view.element.style.background = applied.background;
    if (applied.metricsChanged) { view.lastSize = ''; scheduleFit(view); }
  }
} });

const renderer = createWorkspaceRenderer({
  host: workspace, bindings,
  dispatch(action, expected) { if (expected === workspaceLayout) changeLayout(action); },
  onFocusView(id) { if (!modalOpen()) views.get(id)?.terminal.focus(); },
  onVisibilityChange(id, visible) { const view = views.get(id); if (view) { view.visible = visible; if (visible) scheduleFit(view); } },
  labelForView(panel) { const view = views.get(panel.id); return view ? `${sessionHarnesses.find(item => item.id === view.info.shell)?.label ?? view.info.shell} ${[...views.keys()].indexOf(panel.id) + 1}${view.ended ? ' · exited' : ''}` : 'Saved terminal · unavailable'; },
  isInputBlocked: modalOpen,
  onError: () => notify('That layout change is unavailable. The current workspace was preserved.'),
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
    else workspace.querySelector<HTMLButtonElement>('[data-new-terminal]')?.focus();
  }
}
function render() {
  for (const panel of workspaceLayout.views) {
    const view = views.get(panel.id); if (!view) continue;
    view.hidden = panel.hidden;
    view.group = tabGroups(workspaceLayout.root).find(group => group.tabs.includes(panel.id))?.id ?? focusedGroup();
  }
  renderer.render(workspaceLayout);
  for (const pane of workspace.querySelectorAll<HTMLElement>('.wm-pane')) {
    const group = tabGroups(workspaceLayout.root).find(group => group.id === pane.dataset.groupId); if (!group) continue;
    pane.classList.add('pane'); pane.classList.toggle('focused', workspaceLayout.focusedGroupId === group.id);
    const controls = pane.querySelector<HTMLElement>('.wm-controls')!;
    const open = document.createElement('button'); open.type = 'button'; open.dataset.newTerminal = ''; open.textContent = '+ Session'; open.setAttribute('aria-label', 'Open session');
    open.onclick = () => openNewSession(group.id); controls.prepend(open);
    const active = group.activeTabId ? views.get(group.activeTabId) : null;
    const stop = document.createElement('button'); stop.type = 'button';
    stop.textContent = active ? 'Stop' : 'Remove saved view';
    stop.setAttribute('aria-label', active ? 'Stop terminal' : 'Remove saved terminal view'); stop.disabled = !group.activeTabId;
    stop.onclick = () => {
      if (active) void stopTerminal(active);
      else if (group.activeTabId) changeLayout({ type: 'remove-view', viewId: group.activeTabId });
    }; controls.append(stop);
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
    if (rows < 2 || cols < 2 || size === view.lastSize) return;
    view.lastSize = size;
    const group = workspace.querySelector<HTMLElement>(`[data-group-id="${view.group}"]`);
    if (group) group.querySelector<HTMLElement>('.size')!.textContent = size;
    if (!view.ended) view.resizeChain = view.resizeChain.then(async () => {
      if (!view.disposed && !view.ended) await invoke('terminal_resize', { id: view.info.id, rows, cols });
    }).catch(error => { view.lastSize = ''; if (!view.disposed) notify(error); });
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
async function connectWorkspace() {
  if (!desktop) return;
  const generation = ++scopeRevision;
  const previous = persistence; persistence = null;
  if (previous) { const released = await previous.release(); if (!released.ok || released.dirty) notify('The previous layout could not be fully saved.'); previous.dispose(); }
  try {
    const controller = createWorkspacePersistence({ scope: { workspaceId: 'main' }, initialLayout: workspaceLayout,
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
    persistence = controller;
    const beforeLoad = workspaceLayout;
    const loaded = await controller.load();
    if (generation !== scopeRevision) { controller.dispose(); return; }
    if (!loaded.ok) { notify('The saved layout is unavailable. The current workspace is preserved.'); return; }
    if (loaded.restored && workspaceLayout === beforeLoad) {
      workspaceLayout = loaded.layout;
      for (const view of views.values()) if (!workspaceLayout.views.some(panel => panel.id === view.info.id)) {
        const next = reduceWorkspaceLayout(workspaceLayout, { type: 'add-view', groupId: focusedGroup(),
          view: { id: view.info.id, resource: { kind: 'terminal', instanceId: `prototype-${view.info.id}`, threadId: null, profileId: view.profile }, hidden: false } });
        if (next.ok) workspaceLayout = next.layout;
      }
      controller.update(workspaceLayout); render();
    } else controller.update(workspaceLayout);
    $('#recover-layout')?.remove();
    if (loaded.ownership === 'other') {
      const recover = document.createElement('button'); recover.id = 'recover-layout'; recover.type = 'button'; recover.textContent = 'Recover saved layout';
      recover.onclick = async () => {
        recover.disabled = true;
        try {
          await invoke('global_workspace_recover', { request: { workspaceId: 'main', expectedRevision: loaded.revision } });
          controller.dispose(); persistence = null; recover.remove(); await connectWorkspace();
        } catch (error) { notify(typeof error === 'object' && error && 'message' in error ? error.message : 'Layout recovery was refused.'); recover.disabled = false; }
      };
      $('.toolbar').append(recover);
      notify('This layout belongs to another window. Recover it explicitly after that window has closed.', 'info'); return;
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
    const info = await invoke<SessionInfo>('terminal_start', { cwd: cwd ?? projectPath, rows: 24, cols: 80 });
    attachTerminal(info, group);
    return true;
  } catch (error) { notify(error); return false; }
  finally { starting = false; $<HTMLButtonElement>('#new-terminal').disabled = false; }
}
function attachTerminal(info: SessionInfo, group: string, threadId: string | null = null) {
  const element = document.createElement('div'); element.className = 'terminal-view';
  const initialProfile = ['Claude Code', 'claude_code'].includes(info.shell) ? 'claude' : 'neutral';
  const terminal = new Terminal({ ...presentation.optionsForProfile(initialProfile),
    cursorBlink: false, scrollback: 5000,
    allowProposedApi: false, allowTransparency: false, screenReaderMode: true });
  const fit = new FitAddon(); terminal.loadAddon(fit);
  workspace.append(element); terminal.open(element);
  // No clipboard, link opener or image addon. Escape text never reaches HTML;
  // bounded native attention controls are observed by Rust before rendering.
  for (const osc of [0, 1, 2, 7, 8, 9, 52, 777]) terminal.parser.registerOscHandler(osc, () => true);
  terminal.parser.registerCsiHandler({ final: 't' }, () => true);
  const view: View = { info, terminal, fit, element, group, hidden: false, visible: false, ended: false, disposed: false,
    profile: initialProfile, lastSize: '', resizeTimer: 0, resizeChain: Promise.resolve() };
  views.set(info.id, view); bindings.set(info.id, element); resizeObserver.observe(element);
  const restored = workspaceLayout.views.find(panel => panel.id === info.id);
  if (restored?.resource.kind === 'terminal') { const applied = presentation.applyProfile(terminal, restored.resource.profileId); view.profile = applied.id; element.style.background = applied.background; }
  if (!restored) changeLayout({ type: 'add-view', groupId: tabGroups(workspaceLayout.root).some(candidate => candidate.id === group) ? group : focusedGroup(),
    view: { id: info.id, resource: { kind: 'terminal', instanceId: /^i_[a-z2-7]{12}$/.test(info.id) ? info.id : `prototype-${info.id}`, threadId, profileId: view.profile }, hidden: false } });
  terminal.onData(data => {
    // xterm also emits protocol replies here (e.g. ConPTY's cursor query). They must
    // reach hidden terminals too. DOM focus and the modal own user-input isolation.
    if (view.ended || view.disposed) return;
    void invoke('terminal_write', { id: info.id, data: Array.from(new TextEncoder().encode(data)) }).catch(notify);
  });
  terminal.attachCustomKeyEventHandler(event => {
    if (modalOpen()) return false;
    return !(event.ctrlKey && event.shiftKey && ['T', 't', 'F', 'f', 'ArrowLeft', 'ArrowRight'].includes(event.key));
  });
  render(); setFocus(view.group);
  void drain(view);
}
async function drain(view: View) {
  try {
    while (!view.disposed) {
      const batch = await invoke<OutputBatch>('terminal_read', { id: view.info.id });
      if (view.disposed) return;
      for (const chunk of batch.chunks) {
        if (view.disposed) return;
        await new Promise<void>(resolve => view.terminal.write(new Uint8Array(chunk), resolve));
      }
      if (batch.error) notify(`Terminal output: ${batch.error}`);
      if (batch.eof) { view.ended = true; showExit(view, batch.exit_code); render(); return; }
      await new Promise(resolve => setTimeout(resolve, 25));
    }
  } catch (error) { if (!view.disposed) notify(`Terminal connection interrupted: ${error}`); }
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
async function confirmStop(title: string, message: string): Promise<boolean> {
  // A window close request can arrive over another dialog; the confirmation stacks above it.
  if (confirmDialog.open) return false;
  $('#confirm-title').textContent = title; $('#confirm-message').textContent = message;
  confirmDialog.returnValue = 'cancel'; openModal(confirmDialog);
  return new Promise(resolve => confirmDialog.addEventListener('close', () => {
    resolve(confirmDialog.returnValue === 'stop'); setFocus(focusedGroup());
  }, { once: true }));
}
async function stopTerminal(view: View) {
  if (!view.ended && !await confirmStop('Stop terminal?', 'This stops the shell and the programs it started. To keep them running, cancel and use Hide view instead.')) return;
  try {
    await invoke('terminal_stop', { id: view.info.id });
    view.disposed = true; clearTimeout(view.resizeTimer); view.terminal.dispose(); view.element.remove();
    resizeObserver.unobserve(view.element); bindings.delete(view.info.id); views.delete(view.info.id); changeLayout({ type: 'remove-view', viewId: view.info.id }); setFocus(focusedGroup());
  } catch (error) { notify(error); }
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
const newSession = createNewSession({
  projects: () => desktop ? invoke('project_recent') : Promise.resolve([]),
  harnesses: sessionHarnesses,
  browse: desktop ? async () => {
    const requestId = createRequestId(); await beginMutation(requestId);
    const selected = await browseFolder('project', undefined, requestId);
    return selected.kind === 'project' ? selected.project.path : null;
  } : undefined,
  async open(selection) {
    if (!desktop) throw new Error('Open the desktop app to start a session.');
    if (starting) throw new Error('A session is already opening.');
    starting = true; $<HTMLButtonElement>('#new-terminal').disabled = true;
    try {
    const selected = await invoke<{id:string;path:string}>('project_select', {path:selection.projectPath});
    const started = await invoke<{session:SessionInfo;threadId:string}>(selection.harnessId === 'powershell' ? 'shell_open' : 'agent_open',
      {agentId:selection.harnessId,cwd:selected.path,rows:selection.rows,cols:selection.cols});
    setProject(selected.path);
    selectedProjectPath = selected.path;
    threadSidebar?.setProject({projectId:selected.id,label:selected.path.replace(/[\\/]+$/, '').split(/[\\/]/).pop()});
    const group = tabGroups(workspaceLayout.root).some(group => group.id === newSessionGroup) ? newSessionGroup! : focusedGroup();
    attachTerminal(started.session, group, started.threadId);
    await refreshThreadProject();
    void threadSidebar?.refresh();
    } finally { starting = false; $<HTMLButtonElement>('#new-terminal').disabled = false; }
  },
  restoreFocus: () => setFocus(focusedGroup()),
});
async function openNewSession(group: string = focusedGroup(), path?: string, harness?: string) {
  if (starting || openingSessionDialog) return;
  const selectedProject = threadSidebar?.currentProjectId();
  const selectedPath = selectedProject ? recentThreadProjects.get(selectedProject) : selectedProjectPath;
  openingSessionDialog = true;
  try {
    await loadHarnessCatalog();
    newSession.setHarnesses(sessionHarnesses);
    newSessionGroup = group; newSession.show(path ?? selectedPath, harness);
  } finally { openingSessionDialog = false; }
}
threadSidebar = createThreadSidebar({
  host: $('#work-area'),
  list: request => desktop ? invoke('thread_list', {request}) : Promise.resolve({rows:[],next:null}),
  rename: desktop ? request => invoke('thread_rename', {request}) : undefined,
  onNewSession: projectId => {
    const path = recentThreadProjects.get(projectId);
    if (!path) { notify('Choose the project folder for this session.'); openNewSession(); return; }
    openNewSession(focusedGroup(), path);
  },
  onSelect: thread => {
    const panels = workspaceLayout.views.filter(panel => panel.resource.kind === 'terminal' && panel.resource.threadId === thread.id);
    const panel = panels.find(panel => { const view = views.get(panel.id); return view && !view.disposed && !view.ended; })
      ?? panels.find(panel => { const view = views.get(panel.id); return view && !view.disposed; });
    if (!panel) return;
    const group = tabGroups(workspaceLayout.root).find(group => group.tabs.includes(panel.id));
    if (!group || !views.has(panel.id)) return;
    changeLayout({type:'hide',viewId:panel.id,hidden:false});
    changeLayout({type:'collapse',groupId:group.id,collapsed:false});
    changeLayout({type:'activate',groupId:group.id,viewId:panel.id});
    setFocus(group.id);
  },
  canResume: thread => sessionHarnesses.some(harness => harness.id === thread.harness && harness.nativeResume),
  canRecover: thread => sessionHarnesses.some(harness => harness.id === thread.harness && harness.nativeResume)
    && !workspaceLayout.views.some(panel => {
      if (panel.resource.kind !== 'terminal' || panel.resource.threadId !== thread.id) return false;
      const view = views.get(panel.id); return view && !view.disposed && !view.ended;
    }),
  onRecover: desktop ? async thread => {
    if (starting) throw { code: 'busy' };
    if (thread.projectId !== threadSidebar?.currentProjectId()) throw { code: 'source_changed' };
    await invoke('thread_recover', {
      request: { threadId: thread.id, projectId: thread.projectId, expectedRevision: thread.revision },
    });
  } : undefined,
  onResume: desktop ? async thread => {
    if (starting) throw { code: 'busy' };
    if (thread.projectId !== threadSidebar?.currentProjectId()) throw { code: 'source_changed' };
    const requestedGroup = focusedGroup();
    starting = true; $<HTMLButtonElement>('#new-terminal').disabled = true;
    try {
      const started = await invoke<{ session: SessionInfo; threadId: string }>('thread_resume', {
        request: { threadId: thread.id, projectId: thread.projectId, expectedRevision: thread.revision, rows: 24, cols: 80 },
      });
      const group = tabGroups(workspaceLayout.root).some(candidate => candidate.id === requestedGroup) ? requestedGroup : focusedGroup();
      attachTerminal(started.session, group, started.threadId);
    } finally { starting = false; $<HTMLButtonElement>('#new-terminal').disabled = false; }
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
});
$('#work-area').prepend(threadSidebar.element);
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
      newSession.setHarnesses(sessionHarnesses); threadSidebar?.refreshCapabilities(); render(); return true;
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
const sessionFind = createSessionFind({
  read: request => desktop ? invoke('session_find', {request}) : Promise.reject({issue:'read_unavailable'}),
  onSelectHit: hit => { sessionDetail.show(hit.root, hit.nativeId, {offset:hit.eventOrdinal,fingerprint:hit.fingerprint}); },
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
const sessionDetail = createSessionDetail({
  read: request => desktop ? invoke('session_detail', { request }) : Promise.reject({ issue: 'read_unavailable' }),
  openRaw: selection => {sessionRaw.show(selection);},
  openFind: request => {sessionFind.show(request);},
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
createSessionLibrary({
  roots: () => desktop ? invoke('session_roots') : Promise.reject({ issue: 'read_unavailable' }),
  list: request => desktop ? invoke('session_list', { request }) : Promise.reject({ issue: 'worker_unavailable' }),
  addArchive: desktop ? request => invoke('session_archive_add', { request }) : undefined,
  openDetail: (root, nativeId) => { sessionDetail.show(root, nativeId); },
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
function openProjectDialog() { projectPicker.open(); }
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
$('#project').onclick = openProjectDialog;
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
    const stopThreadUpdates = await listen<unknown>('thread-changed', ({ payload }) => {
      threadSidebar?.notifyThreadChanged(payload);
    }).catch(() => null);
    window.addEventListener('pagehide', () => { stopThreadUpdates?.(); }, { once: true });
    await attentionControl.initialize();
    await listen<{sessionId: string | null}>('attention-focus', ({ payload }) => {
      if (!payload.sessionId) return;
      focusAttentionSession(payload.sessionId);
    });
    setProject(await invoke<string>('project_path'));
    // A development frontend reload reattaches existing processes; it never starts a new one.
    const existing = await invoke<SessionInfo[]>('terminal_list');
    for (const info of existing) attachTerminal(info, focusedGroup());
    if (existing.length) notify('Reconnected to running terminals. Earlier screen contents are not retained across a frontend reload.', 'info');
    await listen('confirm-close', async () => {
      if ((views.size || pendingStart || modalOpen()) && !await confirmStop('Close workspace?', 'All running terminals and their child programs will stop.')) return;
      try {
        const appearanceSaved = await presentation.flush();
        const saved = persistence?.status().ownership === 'ours' ? await persistence.release() : undefined;
        if (!appearanceSaved || (saved && (!saved.ok || saved.dirty))) {
          if (!await confirmStop('Close without saving the latest preferences?', 'Previously saved data is preserved. Recent layout or appearance changes could not be saved.')) return;
          await invoke('workspace_allow_close');
        }
        await invoke('terminal_shutdown');
        for (const view of views.values()) view.disposed = true;
        await getCurrentWindow().close();
      } catch (error) { notify(error); }
    });
    await initializeWindowControls(notify);
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
    await connectWorkspace();
  } catch (error) { $('#runtime-status').textContent = 'Runtime unavailable'; notify(error); }
}
void initialize();
