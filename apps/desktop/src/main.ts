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
import { openModal, openSheet } from './modal.ts';
import { createTerminalFind } from './terminal-find.ts';
import { createPresentation, PROFILES, setTerminalGlass } from './presentation.ts';
import './presentation.css';
import { startTerminalResources, formatTerminalResources, type TerminalResourceStatus } from './terminal-resources.ts';
import { createAttentionControl } from './attention-control.ts';
import { createNewSession } from './new-session.ts';
import { createSessionMenu } from './session-menu.ts';
import { beginMutation, browseFolder, createCatalogClient, createRequestId } from './project-catalog.ts';
import { createSessionLibrary } from './session-library.ts';
import { createSessionDetail } from './session-detail.ts';
import { createSessionExport } from './session-export.ts';
import { createHopPreview, type HopPreview } from './hop-preview.ts';
import { createFilesPanel } from './files-panel.ts';
import { createProjectsOverview } from './projects-overview.ts';
import { createInstancesOverview } from './instances-overview.ts';
import { createSessionSearch } from './session-search.ts';
import { createProjectMenu, organizeToolbar } from './toolbar-simple.ts';
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
// The desktop window is transparent over Windows acrylic; only the terminal panes let it through.
if (desktop) { document.documentElement.classList.add('native-glass'); setTerminalGlass(true); }
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

let noticeTimer = 0;
function notify(message: unknown, tone: Tone = 'error') {
  const notice = $('#notice');
  notice.dataset.tone = tone;
  notice.setAttribute('role', tone === 'error' ? 'alert' : 'status');
  $('#notice span').textContent = String(message);
  notice.hidden = false;
  clearTimeout(noticeTimer);
  if (tone !== 'error') noticeTimer = window.setTimeout(() => { notice.hidden = true; }, 6000);
}
$('#notice button').onclick = () => { $('#notice').hidden = true; };
const attentionControl = createAttentionControl(desktop, notify);
// Only a real modal takes the page. Drawers and sheets (modal.ts) leave the terminals live.
function modalOpen() { return !!document.querySelector('dialog[open][aria-modal="true"]'); }
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
  onNewTab: groupId => { void openNewSession(groupId); },
  onCloseView(id) {
    const view = views.get(id);
    if (view) void stopTerminal(view); // confirms before stopping a running session
    else changeLayout({ type: 'remove-view', viewId: id });
  },
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
    allowProposedApi: false, allowTransparency: desktop, screenReaderMode: true });
  const fit = new FitAddon(); terminal.loadAddon(fit);
  // A new terminal paints its profile background at once. On glass the pane behind it is clear,
  // so without this a fresh session has no tint and its text sits on the bare backdrop.
  element.style.background = String(terminal.options.theme?.background ?? '');
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
async function confirmStop(title: string, message: string, group?: string): Promise<boolean> {
  // A window close request can arrive over another dialog; the confirmation stacks above it.
  if (confirmDialog.open) return false;
  $('#confirm-title').textContent = title; $('#confirm-message').textContent = message;
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
  if (!view.ended && !await confirmStop('Stop terminal?', 'This stops the shell and the programs it started. To keep them running, cancel and use ⋯ then Hide instead.', view.group)) return;
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
/** Starts one session in a project. Shared by the harness menu and the folder dialog. */
async function startSession(selection: { projectPath: string; harnessId: string; rows: number; cols: number }) {
  if (!desktop) throw new Error('Open the desktop app to start a session.');
  if (starting) throw new Error('A session is already opening.');
  const button = $<HTMLButtonElement>('#new-terminal');
  starting = true; button.disabled = true; button.setAttribute('aria-busy', 'true');
  try {
    const selected = await invoke<{id:string;path:string}>('project_select', {path:selection.projectPath});
    // Right after launch the local service and resource measurements are still warming up.
    // Those refusals are temporary: wait and retry instead of making the user press Open again.
    const warmingUp = (failure: unknown) => /still finishing|not yet ready|waiting for healthy resource|Retry when Resources/i.test(
      typeof failure === 'string' ? failure : failure instanceof Error ? failure.message : JSON.stringify(failure ?? ''));
    let started: {session:SessionInfo;threadId:string} | undefined;
    for (let attempt = 0; !started; attempt++) {
      try {
        started = await invoke<{session:SessionInfo;threadId:string}>(selection.harnessId === 'powershell' ? 'shell_open' : 'agent_open',
          {agentId:selection.harnessId,cwd:selected.path,rows:selection.rows,cols:selection.cols});
      } catch (failure) {
        if (!warmingUp(failure) || attempt >= 30) throw failure;
        await new Promise(resolve => setTimeout(resolve, 2000));
      }
    }
    setProject(selected.path);
    selectedProjectPath = selected.path;
    threadSidebar?.setProject({projectId:selected.id,label:selected.path.replace(/[\\/]+$/, '').split(/[\\/]/).pop()});
    const group = tabGroups(workspaceLayout.root).some(group => group.id === newSessionGroup) ? newSessionGroup! : focusedGroup();
    attachTerminal(started.session, group, started.threadId);
    await refreshThreadProject();
    void threadSidebar?.refresh();
  } finally { starting = false; button.disabled = false; button.removeAttribute('aria-busy'); }
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
  start: harnessId => { void launchSession(sessionMenuTarget.path, harnessId, sessionMenuTarget.group); },
  otherFolder: () => { newSession.setHarnesses(sessionHarnesses); newSessionGroup = sessionMenuTarget.group; newSession.show(sessionMenuTarget.path); },
});
let sessionMenuTarget = { path: '', group: '' };
/** No dialog: the session starts where it was asked for, and a failure is reported as a notice. */
async function launchSession(path: string, harnessId: string, group: string) {
  newSessionGroup = group; sessionMenu.remember(harnessId);
  try { await startSession({ projectPath: path, harnessId, rows: 24, cols: 80 }); }
  catch (failure) { notify(typeof failure === 'string' ? failure : failure instanceof Error ? failure.message : 'The session could not be opened. Check the project folder and try again.'); }
}
/** New session: a menu of harnesses under the control that was pressed, for the selected
 * project. With both the folder and the harness already known (a restart) it starts at once. */
async function openNewSession(group: string = focusedGroup(), path?: string, harness?: string) {
  if (starting || openingSessionDialog) return;
  const pressed = document.activeElement instanceof HTMLElement && document.activeElement.matches('#new-terminal,[data-new-terminal],.wm-add,.wm-empty-start,.thread-sidebar-create')
    ? document.activeElement : null;
  const selectedProject = threadSidebar?.currentProjectId();
  const selectedPath = path ?? (selectedProject ? recentThreadProjects.get(selectedProject) : undefined) ?? selectedProjectPath ?? projectPath;
  openingSessionDialog = true;
  try {
    await loadHarnessCatalog();
    if (!selectedPath) { notify('Choose a project first: sessions start in the selected project.', 'info'); $('#project').click(); return; }
    $('#notice').hidden = true;
    if (harness && sessionHarnesses.some(candidate => candidate.id === harness)) { await launchSession(selectedPath, harness, group); return; }
    const anchor = pressed?.isConnected ? pressed : $<HTMLElement>('#new-terminal');
    sessionMenuTarget = { path: selectedPath, group };
    sessionMenu.show(anchor, sessionHarnesses, selectedPath.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || selectedPath);
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
const sessionExport = createSessionExport({
  save: request => desktop ? invoke('session_export', { request }) : Promise.reject({ issue: 'read_unavailable' }),
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
  attachTerminal(launched.started.session, focusedGroup(), launched.started.threadId);
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
});
{
  const button = document.createElement('button'); button.type = 'button'; button.id = 'open-files'; button.textContent = 'Files';
  button.title = 'Browse and edit the project files (nothing is run)';
  button.onclick = () => { void filesPanel.show(); };
  document.querySelector('#new-terminal')?.before(button);
}
const projectsOverview = createProjectsOverview({
  invoke: (command, args) => desktop ? invoke(command, args) : Promise.reject('Open the desktop app to manage projects.'),
  newSession: path => { void openNewSession(focusedGroup(), path); },
  resume: async (row, thread) => {
    if (starting) throw 'A session is already opening.';
    starting = true;
    try {
      const started = await invoke<{ session: SessionInfo; threadId: string }>('thread_resume', {
        request: { threadId: thread.id, projectId: row.id, expectedRevision: thread.revision, rows: 24, cols: 80 },
      });
      attachTerminal(started.session, focusedGroup(), started.threadId);
    } finally { starting = false; }
  },
  decide: (title, message, accept) => decide(title, message, accept),
  reportError: notify,
  restoreFocus: () => setFocus(focusedGroup()),
});
{
  const button = document.createElement('button'); button.type = 'button'; button.id = 'open-projects'; button.textContent = 'Projects';
  button.title = 'Pinned projects, Git status, last thread, new session or worktree';
  button.onclick = () => { void projectsOverview.show(); };
  document.querySelector('#open-files')?.before(button);
}
const instancesOverview = createInstancesOverview({
  invoke: (command, args) => desktop ? invoke(command, args) : Promise.reject('Open the desktop app to see instances.'),
  focus: id => {
    const view = views.get(id); if (!view || view.disposed) return false;
    changeLayout({ type: 'activate', groupId: view.group, viewId: id }); setFocus(view.group); return true;
  },
  threadOf: id => { const panel = workspaceLayout.views.find(p => p.id === id); return panel?.resource.kind === 'terminal' ? panel.resource.threadId ?? null : null; },
  restoreFocus: () => setFocus(focusedGroup()),
});
{
  const button = document.createElement('button'); button.type = 'button'; button.id = 'open-instances'; button.textContent = 'Instances';
  button.title = 'Every running instance with its observed state';
  button.onclick = () => { void instancesOverview.show(); };
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
  openRaw: selection => {sessionRaw.show(selection);},
  openFind: request => {sessionFind.show(request);},
  openExport: selection => { sessionExport.show(selection); },
  openHop: selection => { hopPreview.show(selection); },
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
const unavailable = () => Promise.reject({ issue: 'read_unavailable' });
createSessionSearch({
  search: request => desktop ? invoke('session_search', { request }) : unavailable(),
  indexStatus: () => desktop ? invoke('session_index_status') : unavailable(),
  indexStart: () => desktop ? invoke('session_index_start') : unavailable(),
  indexCancel: () => desktop ? invoke('session_index_cancel') : unavailable(),
  indexEnable: enabled => desktop ? invoke('session_index_enable', { enabled }) : unavailable(),
  indexForget: () => desktop ? invoke('session_index_forget', { storeRootId: null }) : unavailable(),
  openDetail: (root, nativeId) => { sessionDetail.show(root, nativeId); },
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
createProjectMenu({
  button: $('#project'),
  load: () => desktop ? invoke('project_recent') : Promise.resolve([]),
  search: desktop ? onResults => createCatalogClient(snapshot => onResults(snapshot.projects), () => onResults([])) : undefined,
  current: () => projectPath,
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
