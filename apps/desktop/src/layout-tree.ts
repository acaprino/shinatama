// The saved layout of the window (`workspace-model.ts`, kept as it was so that the layouts people saved still load) as the tree that
// Sigrid's `Workspace` draws and changes (`WorkspaceNode`), and back. Pure: no window, no React.
//
// What the tree does not say stays with the layout: the views (what each tab is: a terminal of a session, a team map, the files...),
// the pane the person works in and the pane that fills the area. What the tree cannot hold is dropped by `layoutFromTree`:
//  - `hidden`: a view is either a tab or closed, so every view of a layout that was saved hidden is a tab;
//  - `collapsed`: a pane is either there or closed;
//  - `lastUsableSize`: kept for a pane that stays, a default for one that is new.
import type { WorkspaceNode, WorkspaceTab, WorkspaceTabs } from '@acaprino/sigrid/assemblies';
import type { IconName } from '@acaprino/sigrid/primitives';
import {
  parseWorkspaceLayout, tabGroups,
  type LayoutNode, type LayoutResult, type PanelView, type TabGroup, type WorkspaceLayout,
} from './workspace-model.ts';

/** A tab of the tree: what a view is, and how it is named. `kind` is the kind of the view's resource (a terminal, a team map...). */
export interface PaneTab extends WorkspaceTab {
  /** The colour of the thread the tab belongs to (one of Sigrid's 18), when that thread is on; Sigrid draws it as a dot before the title
   *  (`WorkspaceTab.color`, in the release after 6.11.0: until the pin moves the field is carried and not drawn). */
  color?: string | undefined;
  kind: PanelView['resource']['kind'];
  /** The session a terminal tab stands for, when it stands for one. */
  threadId: string | null;
}

/** How a tab is named and drawn, given the view: the controllers know (a session's name, a thread's colour). */
export type DescribeView = (view: PanelView) => Pick<PaneTab, 'title' | 'icon' | 'closable' | 'color'>;

/** The layout as a tree of panes. A pane with no tab is kept (the only pane of an empty window). */
export function layoutToTree(layout: WorkspaceLayout, describe: DescribeView): WorkspaceNode<PaneTab> {
  const views = new Map(layout.views.map(view => [view.id, view]));
  const leaf = (group: TabGroup): WorkspaceTabs<PaneTab> => {
    const tabs: PaneTab[] = [];
    for (const id of group.tabs) {
      const view = views.get(id);
      if (!view) continue;
      tabs.push({ id, kind: view.resource.kind, threadId: view.resource.kind === 'terminal' ? view.resource.threadId : null, ...describe(view) });
    }
    const active = tabs.some(tab => tab.id === group.activeTabId) ? group.activeTabId : tabs[0]?.id ?? null;
    return { kind: 'tabs', id: group.id, tabs, activeId: active };
  };
  const node = (current: LayoutNode): WorkspaceNode<PaneTab> => current.kind === 'group'
    ? leaf(current)
    : { kind: 'split', direction: current.axis === 'columns' ? 'horizontal' : 'vertical', weights: current.weights, children: current.children.map(node) };
  return node(layout.root);
}

/** What the person did to the tree, as a layout: the views are the previous layout's, in the panes the tree says. */
export function layoutFromTree(
  previous: WorkspaceLayout,
  tree: WorkspaceNode<PaneTab>,
  keep: { focusedGroupId?: string | null; maximizedGroupId?: string | null } = {},
): LayoutResult<WorkspaceLayout> {
  const views = new Map(previous.views.map(view => [view.id, view]));
  const before = new Map(tabGroups(previous.root).map(group => [group.id, group]));
  const inTree: PanelView[] = [];
  const used = new Set<string>();
  const claim = (id: string) => { used.add(id); return id; };
  const fresh = (base: string) => {
    let id = base;
    for (let n = 2; used.has(id); n++) id = `${base}-${n}`;
    return claim(id);
  };
  const failed: { error: 'unknown-target' } = { error: 'unknown-target' };
  let missing = false;
  // Every id of the tree is taken first, so that an id made up for a split never meets one of a pane or of a tab.
  const collect = (node: WorkspaceNode<PaneTab>) => {
    if (node.kind === 'tabs') { claim(node.id); for (const tab of node.tabs) claim(tab.id); } else node.children.forEach(collect);
  };
  collect(tree);
  const build = (node: WorkspaceNode<PaneTab>, path: readonly number[]): LayoutNode => {
    if (node.kind === 'tabs') {
      const old = before.get(node.id);
      const tabs = node.tabs.map(tab => {
        const view = views.get(tab.id);
        if (!view) { missing = true; return tab.id; }
        inTree.push(view.hidden ? { ...view, hidden: false } : view);
        return tab.id;
      });
      return {
        kind: 'group', id: node.id, tabs, activeTabId: tabs.includes(node.activeId ?? '') ? node.activeId : tabs[0] ?? null,
        collapsed: false, lastUsableSize: old?.lastUsableSize ?? { width: 640, height: 480 },
      };
    }
    return {
      kind: 'split', id: fresh(`split-${path.join('-') || 'root'}`), axis: node.direction === 'horizontal' ? 'columns' : 'rows',
      children: node.children.map((child, index) => build(child, [...path, index])), weights: node.weights,
    };
  };
  const root = build(tree, []);
  if (missing) return { ok: false, ...failed };
  const groups = tabGroups(root);
  const usable = (id: string | null | undefined) => id != null && groups.some(group => group.id === id && group.activeTabId !== null);
  const wanted = keep.focusedGroupId ?? previous.focusedGroupId;
  const focusedGroupId = usable(wanted) ? wanted ?? null : groups.find(group => group.activeTabId !== null)?.id ?? null;
  const maximizedGroupId = usable(keep.maximizedGroupId) ? keep.maximizedGroupId ?? null : null;
  return parseWorkspaceLayout({ version: 1, root, views: inTree, focusedGroupId, maximizedGroupId });
}

/** The icon of a tab, by what it holds. */
export const TAB_ICONS: Record<PanelView['resource']['kind'], IconName> = {
  terminal: 'terminal', team: 'graph', chat: 'mail', browser: 'globe', files: 'folder', markdown: 'file', preview: 'file',
  sessions: 'history', acp: 'terminal', companion: 'terminal', 'mini-app': 'layout',
};
