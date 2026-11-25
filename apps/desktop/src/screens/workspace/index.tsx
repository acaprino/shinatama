// WORKSPACE (placeholder, Phase A). The pane area of the Workspace view: Sigrid's `Workspace` over the layout the controller keeps, a
// terminal in each tab of a session (Sigrid's `Terminal`, fed by `app.terminals.connect`), and beside the terminals the tabs of a thread's
// team map and conversation. Owned by the agent that fills the workspace in: it also owns the thread sidebar (`sidebar.tsx`), the tab
// labels and menus, the session cards of a tab that does not run, and the thread chat tab.
import { Workspace as Panes } from '@acaprino/sigrid/assemblies';
import { Empty } from '@acaprino/sigrid/components';
import type { PaneTab } from '../../layout-tree.ts';
import { useApp, useController } from '../../hooks.ts';

export function Workspace() {
  const app = useApp();
  const layout = useController(app.layout);
  useController(app.threads);
  useController(app.terminals);
  const tree = layout.tree(view => ({ title: app.threads.tabLabel(view.id), closable: true }));
  return (
    <Panes<PaneTab>
      label="Terminals"
      tree={tree}
      onChange={next => layout.applyTree(next)}
      renderTab={tab => <Empty fill title={tab.title} description={`A ${tab.kind} tab: its screen is written in the next phase.`} />}
      focusedId={layout.layout.focusedGroupId}
      onFocusedChange={groupId => layout.focus(groupId)}
      maximizedId={layout.maximizedId}
      onMaximizedChange={groupId => layout.maximize(groupId)}
      onAddTab={groupId => { void app.opens.newSession('thread', groupId); }}
      onCloseTab={tab => { void (app.terminals.get(tab.id) ? app.terminals.stop(tab.id) : app.threads.closeSession(tab.threadId ?? '')); }}
    />
  );
}
