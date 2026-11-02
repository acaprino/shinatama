import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal, type ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { boundedRatio, parseLayout } from './layout.ts';
import { initializeWindowControls } from './window-controls.ts';
import { createProjectPicker } from './project-picker.ts';
import './styles.css';

type SessionInfo = { id: string; cwd: string; shell: string };
type OutputBatch = { chunks: number[][]; eof: boolean; exit_code: number | null; error: string | null };
type Profile = { name: string; foreground: string; background: string; accent: string };
type View = {
  info: SessionInfo; terminal: Terminal; fit: FitAddon; element: HTMLDivElement;
  group: number; hidden: boolean; ended: boolean; disposed: boolean; profile: string;
  lastSize: string; resizeTimer: number; resizeChain: Promise<void>;
};
type Pane = { element: HTMLElement; tabs: HTMLElement; body: HTMLElement; empty: HTMLElement;
  state: HTMLElement; path: HTMLElement; size: HTMLElement; profile: HTMLSelectElement; active: string | null };

const $ = <T extends HTMLElement = HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const desktop = isTauri();
const views = new Map<string, View>();
const profiles: Record<string, Profile> = {
  neutral: { name: 'Neutral', foreground: '#d9e5f2', background: '#070d17', accent: '#f05e97' },
  claude: { name: 'Claude', foreground: '#f3dfce', background: '#100d0b', accent: '#f072a2' },
  codex: { name: 'Codex', foreground: '#d4e9ff', background: '#061018', accent: '#8bb8ff' },
  opencode: { name: 'OpenCode', foreground: '#d1f4eb', background: '#071512', accent: '#58d6c2' },
  pi: { name: 'Pi', foreground: '#e6dcff', background: '#100d1b', accent: '#b7a4ff' },
};
let layout = parseLayout(readPreference());
let focused = 0;
let maximized: number | null = null;
let projectPath = '';
let starting = false;
const workspace = $('#workspace');
const confirmDialog = $<HTMLDialogElement>('#confirm-dialog');

function readPreference() { try { return localStorage.getItem('shinatama.prototype.layout.v1'); } catch { return null; } }
function savePreference() {
  try { localStorage.setItem('shinatama.prototype.layout.v1', JSON.stringify(layout)); }
  catch { notify('Layout preferences could not be saved. Terminals are still running.'); }
}
function notify(message: unknown) {
  $('#notice span').textContent = String(message);
  $('#notice').hidden = false;
}
$('#notice button').onclick = () => { $('#notice').hidden = true; };
function modalOpen() { return !!document.querySelector('dialog[open]'); }
function setProject(path: string) {
  projectPath = path.trim();
  $('#project-name').textContent = projectPath.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || 'Choose project';
  $('#project').title = projectPath || 'Choose project folder';
}
function theme(profile: Profile): ITheme {
  return {
    foreground: profile.foreground, background: profile.background, cursor: profile.accent,
    selectionBackground: '#41567599', black: '#172538', red: '#ff8495', green: '#75d9a7',
    yellow: '#e8bb72', blue: '#8bb8ff', magenta: '#d5a4ff', cyan: '#58d6c2', white: '#d9e5f2',
    brightBlack: '#91a7ba', brightRed: '#ffb4bf', brightGreen: '#a8f0ca', brightYellow: '#f5d8a5',
    brightBlue: '#b7d5ff', brightMagenta: '#e8ccff', brightCyan: '#a2f3e5', brightWhite: '#ffffff',
  };
}

function makePane(index: number): Pane {
  const element = document.createElement('section');
  element.className = 'pane'; element.dataset.index = String(index);
  element.setAttribute('aria-label', `Terminal pane ${index + 1}`);
  element.innerHTML = `<div class="pane-header"><div class="tabs" role="tablist" aria-label="Pane ${index + 1} terminals"></div>
    <div class="pane-actions"><span class="pane-label">${index ? 'B' : 'A'}</span>
    <button data-action="move" title="Move terminal to other pane" aria-label="Move terminal to other pane">⇄</button>
    <button data-action="hide" title="Hide view; terminal keeps running" aria-label="Hide terminal view">−</button>
    <button data-action="maximize" title="Maximize pane" aria-label="Maximize pane">⛶</button>
    <button data-action="stop" title="Stop terminal" aria-label="Stop terminal">×</button></div></div>
    <div class="pane-body"><div class="empty"><span class="empty-symbol" aria-hidden="true">${index ? '⌗' : '>_'}</span>
    <h2>${index ? 'A second perspective' : 'Your workspace starts here'}</h2>
    <p>${index ? 'Keep another terminal beside your work. Moving a tab keeps its process running.' : 'Open a terminal in your project. The shell runs locally, with its own input and scrollback.'}</p>
    <button data-action="new">＋ Open terminal</button></div></div>
    <div class="pane-footer"><span class="pane-state">Ready</span><select aria-label="Terminal color profile"><option value="neutral">Neutral</option><option value="claude">Claude</option><option value="codex">Codex</option><option value="opencode">OpenCode</option><option value="pi">Pi</option></select><span class="path"></span><span class="size"></span></div>`;
  const pane: Pane = { element, tabs: element.querySelector('.tabs')!, body: element.querySelector('.pane-body')!,
    empty: element.querySelector('.empty')!, state: element.querySelector('.pane-state')!, path: element.querySelector('.path')!,
    size: element.querySelector('.size')!, profile: element.querySelector('select')!, active: null };
  element.addEventListener('pointerdown', () => setFocus(index, false));
  element.querySelector<HTMLButtonElement>('[data-action="new"]')!.onclick = () => void startTerminal(index);
  element.querySelector<HTMLButtonElement>('[data-action="maximize"]')!.onclick = () => {
    maximized = maximized === index ? null : index; applyLayout();
    element.querySelector('[data-action="maximize"]')!.setAttribute('aria-label', maximized === index ? 'Restore panes' : 'Maximize pane');
  };
  element.querySelector<HTMLButtonElement>('[data-action="move"]')!.onclick = () => { if (pane.active) moveView(pane.active, 1 - index); };
  element.querySelector<HTMLButtonElement>('[data-action="hide"]')!.onclick = () => {
    const view = pane.active && views.get(pane.active);
    if (view) { view.hidden = true; pane.active = null; render(); }
  };
  element.querySelector<HTMLButtonElement>('[data-action="stop"]')!.onclick = () => {
    const view = pane.active && views.get(pane.active); if (view) void stopTerminal(view);
  };
  pane.profile.onchange = () => {
    const view = pane.active && views.get(pane.active);
    if (view) { view.profile = pane.profile.value; view.terminal.options.theme = theme(profiles[view.profile]); view.element.style.background = profiles[view.profile].background; }
  };
  pane.tabs.addEventListener('dragover', event => { event.preventDefault(); });
  pane.tabs.addEventListener('drop', event => {
    event.preventDefault(); const id = event.dataTransfer?.getData('application/x-shinatama-terminal');
    if (id && views.has(id)) moveView(id, index);
  });
  new ResizeObserver(() => { for (const view of views.values()) if (view.group === index) scheduleFit(view); }).observe(pane.body);
  return pane;
}

const panes = [makePane(0), makePane(1)];
const splitter = document.createElement('div');
splitter.className = 'splitter'; splitter.tabIndex = 0; splitter.setAttribute('role', 'separator');
splitter.setAttribute('aria-label', 'Resize terminal panes');
splitter.setAttribute('aria-valuemin', '20'); splitter.setAttribute('aria-valuemax', '80');
workspace.append(panes[0].element, splitter, panes[1].element);
function applyLayout() {
  workspace.classList.toggle('rows', layout.axis === 'rows');
  panes.forEach((pane, index) => {
    pane.element.hidden = maximized !== null && maximized !== index;
    pane.element.classList.toggle('maximized', maximized === index);
    pane.element.style.flex = `${index === 0 ? layout.ratio : 1 - layout.ratio} 1 0`;
  });
  splitter.hidden = maximized !== null;
  splitter.setAttribute('aria-orientation', layout.axis === 'columns' ? 'vertical' : 'horizontal');
  splitter.setAttribute('aria-valuenow', String(Math.round(layout.ratio * 100)));
}
splitter.addEventListener('pointerdown', event => {
  if (event.button !== 0) return;
  splitter.setPointerCapture(event.pointerId);
  const original = layout.ratio;
  const move = (event: PointerEvent) => {
    const bounds = workspace.getBoundingClientRect();
    layout.ratio = boundedRatio(layout.axis === 'columns' ? (event.clientX - bounds.left) / bounds.width : (event.clientY - bounds.top) / bounds.height);
    applyLayout();
  };
  const finish = (event: PointerEvent) => {
    if (event.type === 'pointercancel') { layout.ratio = original; applyLayout(); }
    else savePreference();
    splitter.removeEventListener('pointermove', move);
    splitter.removeEventListener('pointerup', finish); splitter.removeEventListener('pointercancel', finish);
  };
  splitter.addEventListener('pointermove', move);
  splitter.addEventListener('pointerup', finish); splitter.addEventListener('pointercancel', finish);
});
splitter.addEventListener('keydown', event => {
  const negative = layout.axis === 'columns' ? 'ArrowLeft' : 'ArrowUp';
  const positive = layout.axis === 'columns' ? 'ArrowRight' : 'ArrowDown';
  if (event.key === negative || event.key === positive) {
    event.preventDefault(); layout.ratio = boundedRatio(layout.ratio + (event.key === negative ? -0.05 : 0.05)); applyLayout(); savePreference();
  }
});

function setFocus(index: number, terminal = true) {
  focused = index;
  panes.forEach((pane, i) => pane.element.classList.toggle('focused', i === index));
  if (terminal && !modalOpen()) { const id = panes[index].active; if (id) views.get(id)?.terminal.focus(); }
}
function moveView(id: string, group: number) {
  const view = views.get(id); if (!view || view.group === group) return;
  if (panes[view.group].active === id) panes[view.group].active = null;
  view.group = group; view.hidden = false; panes[group].body.append(view.element);
  panes[group].active = id; maximized = null; render(); setFocus(group);
}
function render() {
  for (const [index, pane] of panes.entries()) {
    const members = [...views.values()].filter(view => view.group === index && !view.hidden);
    if (!members.some(view => view.info.id === pane.active)) pane.active = members[0]?.info.id ?? null;
    pane.tabs.replaceChildren();
    for (const view of members) {
      const tab = document.createElement('button'); tab.className = `tab${view.ended ? ' exited' : ''}`;
      tab.setAttribute('role', 'tab'); tab.setAttribute('aria-selected', String(pane.active === view.info.id));
      tab.textContent = `${view.info.shell} ${[...views.keys()].indexOf(view.info.id) + 1}`;
      const dot = document.createElement('i'); dot.setAttribute('aria-hidden', 'true'); tab.append(dot);
      tab.draggable = true;
      tab.ondragstart = event => { event.dataTransfer?.setData('application/x-shinatama-terminal', view.info.id); };
      tab.onclick = () => { pane.active = view.info.id; render(); setFocus(index); };
      pane.tabs.append(tab);
    }
    for (const view of views.values()) if (view.group === index) {
      view.element.hidden = view.hidden || pane.active !== view.info.id;
      if (!view.element.hidden) scheduleFit(view);
    }
    pane.empty.hidden = members.length > 0;
    const current = pane.active ? views.get(pane.active) : undefined;
    pane.state.textContent = current ? current.ended ? 'Exited' : 'Shell running' : 'Ready';
    pane.path.textContent = current?.info.cwd ?? projectPath;
    pane.path.title = current?.info.cwd ?? projectPath;
    pane.size.textContent = current ? `${current.terminal.cols}×${current.terminal.rows}` : '';
    pane.profile.value = current?.profile ?? 'neutral'; pane.profile.disabled = !current;
    for (const action of ['move', 'hide', 'stop']) pane.element.querySelector<HTMLButtonElement>(`[data-action="${action}"]`)!.disabled = !current;
  }
  const hidden = [...views.values()].filter(view => view.hidden).length;
  $('#restore').textContent = hidden ? `Restore views (${hidden})` : 'Restore views';
  $<HTMLButtonElement>('#restore').disabled = !hidden;
  $('#terminal-count').textContent = `${views.size} terminal${views.size === 1 ? '' : 's'}${hidden ? ` · ${hidden} hidden` : ''}`;
  applyLayout();
}

function scheduleFit(view: View) {
  clearTimeout(view.resizeTimer);
  view.resizeTimer = window.setTimeout(() => {
    if (view.disposed || view.element.hidden || panes[view.group].element.hidden || view.element.clientWidth < 50 || view.element.clientHeight < 50) return;
    view.fit.fit();
    const { rows, cols } = view.terminal; const size = `${cols}×${rows}`;
    if (rows < 2 || cols < 2 || size === view.lastSize) return;
    view.lastSize = size;
    if (panes[view.group].active === view.info.id) panes[view.group].size.textContent = size;
    if (!view.ended) view.resizeChain = view.resizeChain.then(async () => {
      if (!view.disposed && !view.ended) await invoke('terminal_resize', { id: view.info.id, rows, cols });
    }).catch(error => { view.lastSize = ''; if (!view.disposed) notify(error); });
  }, 80);
}
async function startTerminal(group: number) {
  if (!desktop) { notify('This is the browser preview. Run npm run desktop to open real terminals.'); return; }
  if (starting) return;
  if (!projectPath || !projectPicker.canStart()) { openProjectDialog(); return; }
  starting = true;
  $<HTMLButtonElement>('#new-terminal').disabled = true;
  try {
    const info = await invoke<SessionInfo>('terminal_start', { cwd: projectPath, rows: 24, cols: 80 });
    attachTerminal(info, group);
  } catch (error) { notify(error); }
  finally { starting = false; $<HTMLButtonElement>('#new-terminal').disabled = false; }
}
function attachTerminal(info: SessionInfo, group: number) {
  const element = document.createElement('div'); element.className = 'terminal-view';
  const terminal = new Terminal({ fontFamily: '"Cascadia Mono", "Cascadia Code", Consolas, monospace', fontSize: 13,
    lineHeight: 1.15, cursorBlink: false, cursorStyle: 'bar', scrollback: 5000, theme: theme(profiles.neutral),
    allowProposedApi: false, allowTransparency: false, screenReaderMode: true });
  const fit = new FitAddon(); terminal.loadAddon(fit);
  panes[group].body.append(element); terminal.open(element);
  // No clipboard, link opener or image addon. Terminal escape data never reaches HTML or IPC.
  for (const osc of [0, 1, 2, 7, 8, 9, 52, 777]) terminal.parser.registerOscHandler(osc, () => true);
  terminal.parser.registerCsiHandler({ final: 't' }, () => true);
  const view: View = { info, terminal, fit, element, group, hidden: false, ended: false, disposed: false,
    profile: 'neutral', lastSize: '', resizeTimer: 0, resizeChain: Promise.resolve() };
  views.set(info.id, view);
  terminal.onData(data => {
    // xterm also emits protocol replies here (e.g. ConPTY's cursor query). They must
    // reach hidden terminals too. DOM focus and the modal own user-input isolation.
    if (view.ended || view.disposed) return;
    void invoke('terminal_write', { id: info.id, data: Array.from(new TextEncoder().encode(data)) }).catch(notify);
  });
  terminal.attachCustomKeyEventHandler(event => {
    if (modalOpen()) return false;
    return !(event.ctrlKey && event.shiftKey && ['T', 't', 'ArrowLeft', 'ArrowRight'].includes(event.key));
  });
  panes[group].active = info.id; maximized = null; render(); setFocus(group);
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
      if (batch.eof) { view.ended = true; render(); return; }
      await new Promise(resolve => setTimeout(resolve, 25));
    }
  } catch (error) { if (!view.disposed) notify(`Terminal connection interrupted: ${error}`); }
}
async function confirmStop(title: string, message: string): Promise<boolean> {
  if (modalOpen()) return false;
  $('#confirm-title').textContent = title; $('#confirm-message').textContent = message;
  confirmDialog.returnValue = 'cancel'; confirmDialog.showModal();
  return new Promise(resolve => confirmDialog.addEventListener('close', () => {
    resolve(confirmDialog.returnValue === 'stop'); setFocus(focused);
  }, { once: true }));
}
async function stopTerminal(view: View) {
  if (!view.ended && !await confirmStop('Stop terminal?', 'This stops the shell and the programs it started. To keep them running, cancel and use Hide view instead.')) return;
  try {
    await invoke('terminal_stop', { id: view.info.id });
    view.disposed = true; clearTimeout(view.resizeTimer); view.terminal.dispose(); view.element.remove();
    views.delete(view.info.id); render();
  } catch (error) { notify(error); }
}

const projectPicker = createProjectPicker({ desktop, getPath: () => projectPath,
  onSelected: path => { setProject(path); render(); }, reportError: notify });
function openProjectDialog() { projectPicker.open(); }
$('#project').onclick = openProjectDialog;
$('#new-terminal').onclick = () => void startTerminal(focused);
$('#restore').onclick = () => { for (const view of views.values()) view.hidden = false; maximized = null; render(); };
$('#layout-toggle').onclick = () => { layout.axis = layout.axis === 'columns' ? 'rows' : 'columns'; maximized = null; applyLayout(); savePreference(); };
document.addEventListener('keydown', event => {
  if (modalOpen() || !event.ctrlKey || !event.shiftKey) return;
  if (event.key.toLowerCase() === 't') { event.preventDefault(); void startTerminal(focused); }
  if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
    event.preventDefault(); maximized = null; applyLayout(); setFocus(1 - focused);
  }
});
async function initialize() {
  render(); setFocus(0, false);
  if (!desktop) {
    $('#runtime-status').textContent = 'Browser preview · terminals require the desktop app';
    return;
  }
  try {
    setProject(await invoke<string>('project_path'));
    // A development frontend reload reattaches existing processes; it never starts a new one.
    const existing = await invoke<SessionInfo[]>('terminal_list');
    for (const [index, info] of existing.entries()) attachTerminal(info, index % 2);
    if (existing.length) notify('Reconnected to running terminals. Earlier screen contents are not retained across a frontend reload.');
    await listen('confirm-close', async () => {
      if (!await confirmStop('Close workspace?', 'All running terminals and their child programs will stop.')) return;
      try {
        await invoke('terminal_shutdown');
        for (const view of views.values()) view.disposed = true;
        await getCurrentWindow().close();
      } catch (error) { notify(error); }
    });
    await initializeWindowControls(notify);
    $('#runtime-status').innerHTML = '<i></i>Local runtime · experimental';
    render();
    // Window controls stay usable while missing/removable paths are checked.
    await projectPicker.restore();
  } catch (error) { $('#runtime-status').textContent = 'Runtime unavailable'; notify(error); }
}
void initialize();
