// WORKSPACE. The pane area of the Workspace view: Sigrid's `Workspace` over the layout the controller keeps. A terminal in each tab of a
// session (`terminal-tab.tsx`: Sigrid's `Terminal`, fed by `app.terminals`), and beside the terminals the tabs of a thread's team map and
// conversation. Every tab stays mounted while it is moved or hidden, so a terminal keeps running and keeps what it printed. In a narrow
// window one pane is shown at a time (the one the person works in), and a row of buttons above it goes to another.
import { useEffect } from 'react';
import { useMedia } from '@acaprino/sigrid';
import { Workspace as Panes, type WorkspaceTabState } from '@acaprino/sigrid/assemblies';
import { Empty, type ContextMenuEntry } from '@acaprino/sigrid/components';
import { Button, Cell, Cluster, Column } from '@acaprino/sigrid/primitives';
import { TAB_ICONS, type PaneTab } from '../../layout-tree.ts';
import { useApp, useController } from '../../hooks.ts';
import { threadOfChatView, threadOfTeamView } from '../../view-ids.ts';
import { BrowserTab } from '../browser/index.tsx';
import { Team } from '../team/index.tsx';
import { TerminalFind } from './terminal-find.tsx';
import { startTerminalDrops } from './terminal-io.ts';
import { TerminalTab } from './terminal-tab.tsx';
import { ThreadChat } from './thread-chat.tsx';

/** Under this width (the shell's own step) the panes are shown one at a time. */
const NARROW = '(width < 620px)';

export function Workspace() {
  const app = useApp();
  const layout = useController(app.layout);
  useController(app.threads);
  useController(app.terminals);
  useController(app.chat);
  useController(app.opens);
  useController(app.browser);
  const narrow = useMedia(NARROW);

  // Files and folders dropped on the window paste their paths into the terminal under the pointer.
  useEffect(() => {
    let stop: (() => void) | undefined;
    let gone = false;
    void startTerminalDrops().then(off => { if (gone) off(); else stop = off; }, () => {});
    return () => { gone = true; stop?.(); };
  }, []);

  const tree = layout.tree(view => ({ title: app.threads.tabTitle(view), icon: TAB_ICONS[view.resource.kind], closable: true, color: app.threads.tabTone(view.id) ?? undefined }));
  const groups = layout.groups.filter(group => group.activeTabId !== null);

  const renderTab = (tab: PaneTab, state: WorkspaceTabState) => {
    switch (tab.kind) {
      case 'terminal': return <TerminalTab id={tab.id} state={state} />;
      case 'chat': return <ThreadChat threadId={threadOfChatView(tab.id) ?? ''} state={state} />;
      // The map keeps drawing and reading its team only while its tab is shown.
      case 'team': return state.visible ? <Team threadId={threadOfTeamView(tab.id) ?? ''} /> : null;
      case 'browser': return <BrowserTab viewId={tab.id} visible={state.visible} />;
      default: return <Empty fill title={tab.title} description="This kind of tab has no screen in this window." />;
    }
  };

  /** What the tab adds to the toolkit's own menu (Close, Move, Divide). */
  const tabMenu = (tab: PaneTab): readonly ContextMenuEntry[] => {
    const entries: ContextMenuEntry[] = [];
    const thread = tab.kind === 'chat' ? threadOfChatView(tab.id) : tab.kind === 'team' ? threadOfTeamView(tab.id) : app.threads.tabWorkset(tab.id)?.id ?? null;
    if (tab.kind === 'terminal' && app.terminals.get(tab.id)) entries.push({ label: 'Find in terminal', icon: 'search', shortcut: 'Ctrl+Shift+F', onSelect: () => app.terminals.openFind(tab.id) });
    if (thread) entries.push({ label: 'Thread settings', icon: 'sliders', onSelect: () => app.ui.openPanel({ kind: 'thread', threadId: thread }) });
    return entries;
  };

  /** The close of a tab: a running session stops after asking and moves to Closed sessions; a tab that does not run moves there too (and
   *  nothing is asked: it comes back with Resume); the other tabs just go. */
  const closeTab = (tab: PaneTab) => {
    if (tab.kind === 'terminal') {
      if (app.terminals.get(tab.id)) { void app.terminals.stop(tab.id); return; }
      const saved = app.threads.savedSession(tab.id);
      if (!saved) { layout.close(tab.id); return; }
      app.opens.cancelWant(saved.id);
      void app.threads.closeSession(saved.id).then(() => app.threads.sayClosed(saved.id));
    } else if (tab.kind === 'team') app.team.close(threadOfTeamView(tab.id) ?? '');
    else if (tab.kind === 'browser') void app.browser.close(tab.id);
    else layout.close(tab.id);
  };

  const panes = (
    <Panes<PaneTab>
      label="Terminals"
      tree={tree}
      onChange={next => layout.applyTree(next)}
      renderTab={renderTab}
      tabMenu={tabMenu}
      focusedId={layout.layout.focusedGroupId}
      onFocusedChange={groupId => layout.focus(groupId)}
      // One pane at a time in a narrow window: the one the person works in fills the area.
      maximizedId={narrow ? layout.focusedGroup() : layout.maximizedId}
      onMaximizedChange={narrow ? undefined : groupId => layout.maximize(groupId)}
      onAddTab={groupId => { void app.opens.newSession('thread', groupId); }}
      onCloseTab={closeTab}
    />
  );
  return (
    <>
      <Column fill gap="xs">
        {narrow && groups.length > 1 ? (
          <Cluster gap="xs" pad="xs" role="group" aria-label="Panes">
            {groups.map((group, index) => (
              <Button key={group.id} size="sm" variant={group.id === layout.focusedGroup() ? 'default' : 'outline'} aria-pressed={group.id === layout.focusedGroup()} onClick={() => layout.focus(group.id)}>
                {`Pane ${index + 1}`}
              </Button>
            ))}
          </Cluster>
        ) : null}
        <Cell grow>{panes}</Cell>
      </Column>
      <TerminalFind />
    </>
  );
}
