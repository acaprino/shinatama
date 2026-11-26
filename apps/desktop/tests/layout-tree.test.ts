import test from 'node:test';
import assert from 'node:assert/strict';
import { closeWorkspaceTab, moveWorkspaceTab } from '@acaprino/sigrid/assemblies';
import { layoutFromTree, layoutToTree, type PaneTab } from '../src/layout-tree.ts';
import { emptyWorkspace, reduceWorkspaceLayout, tabGroups, type LayoutAction, type WorkspaceLayout } from '../src/workspace-model.ts';

const terminal = (id: string, threadId: string | null = null) => ({ id, resource: { kind: 'terminal' as const, instanceId: null, threadId, profileId: null }, hidden: false });
const apply = (layout: WorkspaceLayout, action: LayoutAction) => { const result = reduceWorkspaceLayout(layout, action); assert.ok(result.ok, JSON.stringify(result)); return result.layout; };
const describe = (view: { id: string }) => ({ title: `Tab ${view.id}` });

/** Two panes side by side: a and b in the first, c in the second. */
function sample(): WorkspaceLayout {
  let layout = emptyWorkspace();
  layout = apply(layout, { type: 'add-view', groupId: 'group-main', view: terminal('a') });
  layout = apply(layout, { type: 'add-view', groupId: 'group-main', view: terminal('b') });
  layout = apply(layout, { type: 'split', groupId: 'group-main', view: terminal('c'), axis: 'columns', side: 'after', newGroupId: 'g-c', newSplitId: 's-c' });
  return apply(layout, { type: 'focus', groupId: 'g-c' });
}

test('the layout is a tree of panes: columns are a horizontal split, every tab is named by the app', () => {
  const tree = layoutToTree(sample(), describe);
  assert.equal(tree.kind, 'split');
  if (tree.kind !== 'split') return;
  assert.equal(tree.direction, 'horizontal');
  assert.deepEqual(tree.children.map(child => child.kind === 'tabs' ? child.tabs.map(tab => tab.title) : null), [['Tab a', 'Tab b'], ['Tab c']]);
});

test('a tree that nobody changed is the same layout apart from what the tree cannot hold', () => {
  const layout = sample();
  const result = layoutFromTree(layout, layoutToTree(layout, describe));
  assert.ok(result.ok);
  if (!result.ok) return;
  assert.deepEqual(tabGroups(result.value.root).map(group => [group.id, group.tabs, group.activeTabId]), tabGroups(layout.root).map(group => [group.id, group.tabs, group.activeTabId]));
  assert.equal(result.value.focusedGroupId, 'g-c');
  assert.deepEqual(result.value.views, layout.views);
});

test('a tab moved by Sigrid to the other pane is moved in the layout, and the views are the layout\'s own', () => {
  const layout = sample();
  const moved = moveWorkspaceTab<PaneTab>(layoutToTree(layout, describe), 'a', { groupId: 'g-c' });
  const result = layoutFromTree(layout, moved);
  assert.ok(result.ok);
  if (!result.ok) return;
  const groups = tabGroups(result.value.root);
  assert.deepEqual(groups.map(group => group.tabs), [['b'], ['c', 'a']]);
  assert.deepEqual(result.value.views.map(view => view.id).sort(), ['a', 'b', 'c']);
});

test('a tab closed in the tree leaves the layout, and a pane left empty is folded away', () => {
  const layout = sample();
  const result = layoutFromTree(layout, closeWorkspaceTab<PaneTab>(layoutToTree(layout, describe), 'c'));
  assert.ok(result.ok, JSON.stringify(result));
  if (!result.ok) return;
  assert.deepEqual(result.value.views.map(view => view.id).sort(), ['a', 'b']);
  assert.equal(result.value.root.kind, 'group', 'one pane is left: no split');
});

test('a view that was saved hidden comes back as a tab, and a tab the layout does not know is refused', () => {
  let layout = sample();
  layout = apply(layout, { type: 'hide', viewId: 'b', hidden: true });
  const result = layoutFromTree(layout, layoutToTree(layout, describe));
  assert.ok(result.ok);
  if (result.ok) assert.equal(result.value.views.find(view => view.id === 'b')?.hidden, false);
  const tree = layoutToTree(layout, describe);
  const foreign = tree.kind === 'split' ? { ...tree, children: [{ kind: 'tabs' as const, id: 'x', tabs: [{ id: 'nope', title: 'x', kind: 'terminal' as const, threadId: null }], activeId: 'nope' }, tree.children[1]] } : tree;
  assert.equal(layoutFromTree(layout, foreign as typeof tree).ok, false);
});

test('a pane divided by Sigrid gets a split of its own with an id the layout accepts', () => {
  const layout = sample();
  const moved = moveWorkspaceTab<PaneTab>(layoutToTree(layout, describe), 'b', { groupId: 'g-c', edge: 'bottom', newId: 'pane-9' });
  const result = layoutFromTree(layout, moved);
  assert.ok(result.ok, JSON.stringify(result));
  if (!result.ok) return;
  assert.deepEqual(tabGroups(result.value.root).map(group => group.id), ['group-main', 'g-c', 'pane-9']);
});
