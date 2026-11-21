import { componentElement, iconButtonHtml, renderShellFrame, shellFrameHtml, titlebarHtml, uiButton, uiPart, uiTraits } from '@acaprino/imago/ui';
import { projectControls, stopControls } from './project-controls.ts';
import './shell-content.css';

// The canon's window (Imago v0.2.0): the title bar with its sidebar toggle, the thread sidebar (main.ts puts it in the
// body), the content region with the toolbar as its tools row and the tiling workspace as its body, the side panel
// (the inspector, written empty and hidden) and the footer. The frame is written once and filled in place.
const shell = document.querySelector<HTMLElement>('#app')!;
renderShellFrame(shell, shellFrameHtml({
  titlebarHtml: titlebarHtml({ brand: { name: 'Shinatama' }, sidebarToggle: { label: 'Threads', controls: 'thread-sidebar' }, windowControls: true }),
  contentId: 'desktop-content', contentLabel: 'Terminal workspace', skipLabel: 'Skip to terminal workspace',
  inspectorLabel: 'Side panel', contentToolsHtml: '', contentHtml: '', footerHtml: '',
}));
const main = shell.querySelector<HTMLElement>('#desktop-content')!;
const toolbar = uiTraits(uiPart('nav', undefined, 'toolbar'), 'toolbar');
toolbar.setAttribute('aria-label', 'Workspace controls');
const threads = componentElement(iconButtonHtml('Thread sidebar', 'threads-toggle', 'layers', {
  attrs: { id: 'threads-toggle', 'aria-pressed': 'true' },
}));
const project = uiButton('Projects', { attrs: { id: 'project', title: 'Choose projects for the thread sidebar' } });
project.querySelector('span')!.id = 'project-name';
toolbar.append(threads, project,
  uiButton('New session', { primary: true, icon: 'plus', attrs: { id: 'new-terminal' } }),
  uiTraits(uiPart('spacer', undefined, 'spring'), 'spacer'),
  uiButton('Layout', { attrs: { id: 'layout-toggle', title: 'Switch between columns and rows' } }),
  uiButton('Restore views', { attrs: { id: 'restore', title: 'Show all hidden terminals' } }));
main.querySelector('.im-panel-tools')!.append(toolbar);
// The tiling workspace fills the content panel's body.
const workspace = uiPart('stack', undefined, 'workspace-host');
workspace.id = 'workspace';
workspace.setAttribute('aria-label', 'Terminal workspace');
main.querySelector('.im-panel-body')!.append(workspace);
// The footer: the runtime's state, the counts, the keys.
const footer = shell.querySelector<HTMLElement>('.im-footer')!;
const runtime = uiPart('text', 'Connecting…', 'im-footer-text'); runtime.id = 'runtime-status';
const terminals = uiPart('text', '0 terminals'); terminals.id = 'terminal-count';
const count = uiPart('hint', '2 panes');
count.id = 'pane-count';
const workspaceStatus = uiPart('text', 'Workspace ', 'workspace-name');
workspaceStatus.append(count);
const keys = uiPart('text', undefined, 'im-footer-keys');
keys.append(uiPart('hint', 'Ctrl+Shift+T  new session'), uiPart('hint', 'Ctrl+Shift+Left / Right  focus pane'));
footer.append(runtime, terminals, workspaceStatus, uiPart('spacer'), keys);
for (const button of shell.querySelectorAll<HTMLButtonElement>('[data-im-window]')) {
  button.id = 'window-' + button.dataset.imWindow;
  button.disabled = true;
}
const host = uiTraits(uiPart('body'), 'noticeHost');
host.dataset.imNotices = '';
const notice = uiTraits(uiPart('row'), 'notice');
notice.id = 'notice';
notice.hidden = true;
notice.setAttribute('role', 'status');
notice.append(uiPart('text'), componentElement(iconButtonHtml('Dismiss notification', 'dismiss-notice', 'close')));
host.append(notice);
shell.append(host, projectControls(), stopControls());
