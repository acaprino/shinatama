import { attachLayers, componentElement, iconButtonHtml, renderShellFrame, shellFrameHtml, titlebarHtml, uiButton, uiPart } from '@acaprino/imago/ui';
import { projectControls, stopControls } from './project-controls.ts';

const shell = document.querySelector<HTMLElement>('#app')!;
renderShellFrame(shell, shellFrameHtml({
  titlebarHtml: titlebarHtml({ brand: { name: 'Shinatama' }, windowControls: true }),
  mainId: 'desktop-content', skipLabel: 'Skip to terminal workspace', mainHtml: '',
}));
const main = shell.querySelector<HTMLElement>('#desktop-content')!;
main.classList.add('im-desktop-content');
const toolbar = uiPart('nav', undefined, 'toolbar im-toolbar');
toolbar.setAttribute('aria-label', 'Workspace controls');
const threads = componentElement(iconButtonHtml('Thread sidebar', 'threads-toggle', 'layers', {
  attrs: { id: 'threads-toggle', 'aria-pressed': 'true' },
}));
const project = uiButton('Projects', { attrs: { id: 'project', title: 'Choose projects for the thread sidebar' } });
project.querySelector('span')!.id = 'project-name';
const count = uiPart('hint', '2 panes');
count.id = 'pane-count';
const workspaceStatus = uiPart('text', 'Workspace ', 'workspace-name');
workspaceStatus.append(count);
toolbar.append(threads, project,
  uiButton('New session', { primary: true, icon: 'plus', attrs: { id: 'new-terminal' } }),
  uiPart('spacer', undefined, 'spring im-spacer'),
  uiButton('Layout', { attrs: { id: 'layout-toggle', title: 'Switch between columns and rows' } }),
  uiButton('Restore views', { attrs: { id: 'restore', title: 'Show all hidden terminals' } }));
const area = uiPart('body', undefined, 'im-work-area');
area.id = 'work-area';
const workspace = uiPart('region', undefined, 'im-workspace-area');
workspace.id = 'workspace';
workspace.setAttribute('aria-label', 'Terminal workspace');
area.append(workspace);
const status = uiPart('footer', undefined, 'statusbar');
const runtime = uiPart('text', 'Connecting.'); runtime.id = 'runtime-status';
const terminals = uiPart('text', '0 terminals'); terminals.id = 'terminal-count';
status.append(runtime, terminals, workspaceStatus, uiPart('spacer'), uiPart('hint', 'Ctrl+Shift+T  new session'),
  uiPart('hint', 'Ctrl+Shift+Left / Right  focus pane'));
main.append(toolbar, area, status);
for (const button of shell.querySelectorAll<HTMLButtonElement>('[data-im-window]')) {
  button.id = 'window-' + button.dataset.imWindow;
  button.disabled = true;
}
const host = uiPart('body', undefined, 'im-notices');
host.dataset.imNotices = '';
const notice = uiPart('row', undefined, 'im-float im-notice');
notice.id = 'notice';
notice.hidden = true;
notice.setAttribute('role', 'status');
notice.append(uiPart('text'), componentElement(iconButtonHtml('Dismiss notification', 'dismiss-notice', 'close')));
host.append(notice);
shell.append(host, projectControls(), stopControls());
const detach = attachLayers(shell);
window.addEventListener('pagehide', detach, { once: true });
