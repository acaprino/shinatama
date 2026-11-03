import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  beginLayoutGesture, cancelLayoutGesture, commitLayoutGesture, emptyWorkspace,
  keyboardNeighbor, paneRects, panelAvailability, parseWorkspaceLayout,
  previewLayoutGesture, reduceWorkspaceLayout, restoreWorkspaceLayout,
  serializeWorkspaceLayout, tabGroups,
} from '../src/workspace-model.ts';
import type { LayoutAction, PanelView, WorkspaceLayout } from '../src/workspace-model.ts';

function terminal(id: string): PanelView {
  return { id: `view-${id}`, hidden: false, resource: { kind: 'terminal', instanceId: `instance-${id}`, threadId: `thread-${id}`, profileId: 'neutral' } };
}
function apply(layout: WorkspaceLayout, action: LayoutAction): WorkspaceLayout {
  const result = reduceWorkspaceLayout(layout, action);
  assert.equal(result.ok, true, result.ok ? '' : result.error);
  if (!result.ok) throw new Error(result.error);
  return result.layout;
}
function populated(): WorkspaceLayout {
  let layout = emptyWorkspace();
  layout = apply(layout, { type: 'add-view', groupId: 'group-main', view: terminal('a') });
  layout = apply(layout, { type: 'add-view', groupId: 'group-main', view: terminal('b') });
  layout = apply(layout, { type: 'split', groupId: 'group-main', view: terminal('c'), axis: 'columns', side: 'after', newGroupId: 'group-right', newSplitId: 'split-root' });
  return apply(layout, { type: 'split', groupId: 'group-right', view: terminal('d'), axis: 'rows', side: 'after', newGroupId: 'group-bottom', newSplitId: 'split-right' });
}

test('nested panes, keyboard focus and atomic moves preserve resource identity and usable hidden sizes', () => {
  let layout = populated();
  const originalReferences = layout.views.map(view => view.resource);
  assert.equal(keyboardNeighbor(layout, 'group-bottom', 'up', { width: 1200, height: 800 }), 'group-right');
  layout = apply(layout, { type: 'resize', splitId: 'split-root', weights: [3, 1] });
  layout = apply(layout, { type: 'move', viewId: 'view-b', groupId: 'group-bottom', index: 0 });
  assert.deepEqual(tabGroups(layout.root).find(group => group.id === 'group-bottom')?.tabs, ['view-b', 'view-d']);
  assert.deepEqual(layout.views.map(view => view.resource), originalReferences);
  layout = apply(layout, { type: 'remember-size', groupId: 'group-bottom', size: { width: 240, height: 180 } });
  layout = apply(layout, { type: 'hide', viewId: 'view-b', hidden: true });
  assert.equal(tabGroups(layout.root).find(group => group.id === 'group-bottom')?.activeTabId, 'view-d');
  layout = apply(layout, { type: 'collapse', groupId: 'group-bottom', collapsed: true });
  assert.deepEqual(tabGroups(layout.root).find(group => group.id === 'group-bottom')?.lastUsableSize, { width: 240, height: 180 });
  const tree = layout.root;
  layout = apply(layout, { type: 'maximize', groupId: 'group-main' });
  layout = apply(layout, { type: 'maximize', groupId: null });
  assert.deepEqual(layout.root, tree);
  const narrow = paneRects(layout, { width: 80, height: 60 });
  assert.equal(narrow.panes.length, 3);
  assert.ok(narrow.width >= 320 && narrow.height >= 128);
  assert.ok(narrow.panes.every(pane => pane.width > 0 && pane.height > 0));
  // Moving the sole tab away removes an empty leaf and its redundant split.
  layout = apply(layout, { type: 'move', viewId: 'view-c', groupId: 'group-main' });
  assert.equal(tabGroups(layout.root).length, 2);
  assert.equal(parseWorkspaceLayout(layout).ok, true);
});

test('invalid changes and cancelled previews return the original object without partial mutation', () => {
  const original = populated();
  const json = JSON.stringify(original);
  const duplicate = { ...terminal('extra'), resource: terminal('a').resource };
  for (const action of [
    { type: 'resize', splitId: 'split-root', weights: [Number.NaN, 1] },
    { type: 'move', viewId: 'view-a', groupId: 'missing' },
    { type: 'activate', groupId: 'group-main', viewId: 'view-c' },
    { type: 'add-view', groupId: 'group-main', view: duplicate },
  ] satisfies LayoutAction[]) {
    const result = reduceWorkspaceLayout(original, action);
    assert.equal(result.ok, false);
    assert.equal(result.layout, original);
    assert.equal(JSON.stringify(original), json);
  }
  const preview = previewLayoutGesture(beginLayoutGesture(original), { type: 'resize', splitId: 'split-root', weights: [1, 4] });
  assert.equal(preview.ok, true);
  if (!preview.ok) throw new Error(preview.error);
  assert.equal(cancelLayoutGesture(preview.value), original);
  assert.equal(reduceWorkspaceLayout(original, { type: 'cancel' }).layout, original);
  const conflicting = apply(original, { type: 'focus', groupId: 'group-main' });
  const commit = commitLayoutGesture(preview.value, conflicting);
  assert.equal(commit.ok, false);
  assert.equal(commit.layout, conflicting);
  const cycle: Record<string, unknown> = { kind: 'split', id: 'cycle', axis: 'rows', weights: [1, 1] };
  cycle.children = [cycle, original.root];
  assert.equal(parseWorkspaceLayout({ ...original, root: cycle }).ok, false);
});

test('saved layouts contain only references; restore has no launch path and gated views remain unavailable', () => {
  const original = populated();
  const serialized = serializeWorkspaceLayout(original);
  assert.equal(serialized.ok, true);
  if (!serialized.ok) throw new Error(serialized.error);
  const restored = restoreWorkspaceLayout(serialized.value, emptyWorkspace());
  assert.equal(restored.restored, true);
  assert.deepEqual(restored.layout, original);
  assert.deepEqual(panelAvailability(restored.layout.views[0], new Set()), { available: false, reason: 'resource-unavailable' });
  assert.deepEqual(panelAvailability(restored.layout.views[0], new Set(['instance-a'])), { available: true });
  const browser: PanelView = { id: 'view-browser', hidden: false, resource: { kind: 'browser', resourceId: 'browser-reference' } };
  const gated = apply(original, { type: 'add-view', groupId: 'group-main', view: browser });
  assert.deepEqual(panelAvailability(browser, new Set()), { available: false, reason: 'gated-panel' });
  assert.equal(serializeWorkspaceLayout(gated).ok, true);
  for (const corrupt of ['{', JSON.stringify({ ...original, version: 99 }), JSON.stringify({ ...original, pid: 12 }), JSON.stringify({ ...original, dirty_buffer: 'unsaved' })]) {
    const recovery = restoreWorkspaceLayout(corrupt, original);
    assert.equal(recovery.layout, original);
    assert.equal(recovery.restored, false);
  }
  assert.doesNotMatch(serialized.value, /"(?:pid|pty|command|credentials|dirty_buffer|rendered_content)"\s*:/);
  assert.equal(Object.isFrozen(restored.layout.root), true);
});
