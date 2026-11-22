import { componentElement, iconButtonHtml, renderShellFrame, shellFrameHtml, tabNavigationHtml, titlebarHtml, uiButton, uiPart, uiTraits } from '@acaprino/imago/ui';
import { projectControls, stopControls } from './project-controls.ts';
import { VIEW_TABS } from './views.ts';
import './shell-content.css';

// The canon's window (Imago v0.2.0): the title bar with its sidebar toggle, the thread sidebar (main.ts puts it in the
// body), the content region with the toolbar as its tools row and the tiling workspace as its body, the side panel
// (the inspector, written empty and hidden) and the footer. The frame is written once and filled in place.
const shell = document.querySelector<HTMLElement>('#app')!;
renderShellFrame(shell, shellFrameHtml({
  // The views are the tabs of the title bar, Settings the last (main.ts createViews); under 620 the Views button stands for them.
  titlebarHtml: titlebarHtml({
    brand: { name: 'Shinatama' }, sidebarToggle: { label: 'Threads', controls: 'thread-sidebar' }, windowControls: true,
    navigationHtml: tabNavigationHtml(VIEW_TABS, 'workspace', { label: 'Views', action: 'view' }), views: { label: 'Views', action: 'views-menu' },
  }),
  contentId: 'desktop-content', contentLabel: 'Terminal workspace', skipLabel: 'Skip to terminal workspace',
  inspectorLabel: 'Side panel', contentHtml: '', footerHtml: '',
}));
const main = shell.querySelector<HTMLElement>('#desktop-content')!;
const titlebar = shell.querySelector<HTMLElement>('.im-titlebar')!;
// No row above the terminals: the window's controls sit in the title bar, so the terminals keep the whole height.
const toolbar = uiTraits(uiPart('nav', undefined, 'toolbar titlebar-tools'), 'toolbar');
toolbar.setAttribute('aria-label', 'Workspace controls');
const threads = componentElement(iconButtonHtml('Thread sidebar', 'threads-toggle', 'layers', {
  attrs: { id: 'threads-toggle', 'aria-pressed': 'true' },
}));
threads.classList.add('titlebar-threads');
// Choosing the projects the thread sidebar lists is the sidebar's own action: workset-sidebar.ts moves this button into its header.
const project = componentElement(iconButtonHtml('Choose projects for the thread sidebar', 'choose-projects', 'folder', { attrs: { id: 'project' } }));
const newSession = uiButton('New session', { primary: true, icon: 'plus', attrs: { id: 'new-terminal' } });
newSession.classList.add('titlebar-new');
toolbar.append(project, newSession,
  uiTraits(uiPart('spacer', undefined, 'spring'), 'spacer'),
  uiButton('Layout', { attrs: { id: 'layout-toggle', title: 'Switch between columns and rows' } }),
  uiButton('Restore views', { attrs: { id: 'restore', title: 'Show all hidden terminals' } }));
titlebar.querySelector('.im-spacer')!.after(toolbar);
titlebar.querySelector('.im-sidebar-toggle')!.after(threads);
// The tiling workspace fills the content panel's body.
const workspace = uiPart('stack', undefined, 'workspace-host');
workspace.id = 'workspace';
workspace.setAttribute('aria-label', 'Terminal workspace');
main.querySelector('.im-panel-body')!.append(workspace);
// The footer: the runtime's state, the counts, the keys.
const footer = shell.querySelector<HTMLElement>('.im-footer')!;
const runtime = uiPart('text', 'Connecting…', 'im-footer-text'); runtime.id = 'runtime-status';
// What an open in progress waits for, said until it ends (main.ts showProgress).
const progress = uiPart('text'); progress.id = 'open-progress'; progress.setAttribute('role', 'status'); progress.hidden = true;
const terminals = uiPart('text', '0 terminals'); terminals.id = 'terminal-count';
const count = uiPart('hint', '2 panes');
count.id = 'pane-count';
const workspaceStatus = uiPart('text', 'Workspace ', 'workspace-name');
workspaceStatus.append(count);
// The keys that work now: main.ts writes them from the registry (footerKeys) once mountApp has registered its own.
const keys = uiPart('text', undefined, 'im-footer-keys');
footer.append(runtime, progress, terminals, workspaceStatus, uiPart('spacer'), keys);
for (const button of shell.querySelectorAll<HTMLButtonElement>('[data-im-window]')) {
  button.id = 'window-' + button.dataset.imWindow;
  button.disabled = true;
}
// The notices are Imago's (main.ts notify): their host is made in the shell at the first one.
shell.append(projectControls(), stopControls());
