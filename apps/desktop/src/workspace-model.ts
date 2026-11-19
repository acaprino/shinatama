// Presentation and opaque resource references only. This module has no runtime,
// filesystem, PTY, process, browser or persistence effects.
export const WORKSPACE_LIMITS = Object.freeze({ views: 64, groups: 32, nodes: 63, depth: 12, jsonBytes: 262_144 });
export const MIN_PANE_SIZE = Object.freeze({ width: 160, height: 144 });
export type Axis = 'columns' | 'rows';
export type Direction = 'left' | 'right' | 'up' | 'down';
export type GatedPanelKind = 'acp' | 'companion' | 'sessions' | 'files' | 'markdown' | 'preview' | 'browser' | 'team' | 'mini-app';
export type ResourceReference =
  | { readonly kind: 'terminal'; readonly instanceId: string | null; readonly threadId: string | null; readonly profileId: string | null }
  | { readonly kind: GatedPanelKind; readonly resourceId: string };
export interface PanelView { readonly id: string; readonly resource: ResourceReference; readonly hidden: boolean }
export interface UsableSize { readonly width: number; readonly height: number }
export interface TabGroup {
  readonly kind: 'group'; readonly id: string; readonly tabs: readonly string[];
  readonly activeTabId: string | null; readonly collapsed: boolean; readonly lastUsableSize: UsableSize;
}
export interface SplitNode {
  readonly kind: 'split'; readonly id: string; readonly axis: Axis;
  readonly children: readonly LayoutNode[]; readonly weights: readonly number[];
}
export type LayoutNode = TabGroup | SplitNode;
export interface WorkspaceLayout {
  readonly version: 1; readonly root: LayoutNode; readonly views: readonly PanelView[];
  readonly focusedGroupId: string | null; readonly maximizedGroupId: string | null;
}
export type LayoutError = 'invalid-layout' | 'unsupported-version' | 'bounds' | 'duplicate' | 'membership' | 'unknown-target' | 'invalid-action' | 'conflict';
export type LayoutResult<T> = { readonly ok: true; readonly value: T } | { readonly ok: false; readonly error: LayoutError };
export type LayoutMutation = { readonly ok: true; readonly layout: WorkspaceLayout; readonly changed: boolean }
  | { readonly ok: false; readonly layout: WorkspaceLayout; readonly error: LayoutError };

// An available terminal container does not qualify an agent or authorize a launch.
export const NATIVE_CAPABILITIES = Object.freeze({ terminal: true, acp: false, companion: false, sessions: false,
  files: false, markdown: false, preview: false, browser: false, team: false, 'mini-app': false } as const);
export type NativeCapabilities = typeof NATIVE_CAPABILITIES;

type Edge = { readonly axis: Axis; readonly side: 'before' | 'after'; readonly newGroupId: string; readonly newSplitId: string };
export type LayoutAction =
  | { readonly type: 'remove-view'; readonly viewId: string }
  | { readonly type: 'add-view'; readonly groupId: string; readonly view: PanelView; readonly index?: number }
  | ({ readonly type: 'split'; readonly groupId: string; readonly view: PanelView } & Edge)
  | { readonly type: 'move'; readonly viewId: string; readonly groupId: string; readonly index?: number; readonly edge?: Edge }
  | { readonly type: 'resize'; readonly splitId: string; readonly weights: readonly number[] }
  | { readonly type: 'focus'; readonly groupId: string }
  | { readonly type: 'activate'; readonly groupId: string; readonly viewId: string }
  | { readonly type: 'hide'; readonly viewId: string; readonly hidden: boolean }
  | { readonly type: 'collapse'; readonly groupId: string; readonly collapsed: boolean }
  | { readonly type: 'maximize'; readonly groupId: string | null }
  | { readonly type: 'remember-size'; readonly groupId: string; readonly size: UsableSize }
  | { readonly type: 'cancel' };

class InvalidLayout extends Error {
  readonly code: LayoutError;
  constructor(code: LayoutError) { super(code); this.code = code; }
}
function fail(code: LayoutError): never { throw new InvalidLayout(code); }
function record(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)
    || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) fail('invalid-layout');
  const result = value as Record<string, unknown>;
  if (Object.keys(result).some(key => !keys.includes(key)) || keys.some(key => !Object.hasOwn(result, key))) fail('invalid-layout');
  return result;
}
function id(value: unknown): string {
  if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$/.test(value)) fail('invalid-layout');
  return value;
}
function nullableId(value: unknown): string | null { return value === null ? null : id(value); }
function bool(value: unknown): boolean { if (typeof value !== 'boolean') fail('invalid-layout'); return value; }
function size(value: unknown): UsableSize {
  const data = record(value, ['width', 'height']);
  const width = data.width; const height = data.height;
  if (typeof width !== 'number' || typeof height !== 'number' || !Number.isFinite(width) || !Number.isFinite(height)
    || width < MIN_PANE_SIZE.width || height < MIN_PANE_SIZE.height || width > 32768 || height > 32768) fail('bounds');
  return Object.freeze({ width, height });
}
function view(value: unknown): PanelView {
  const data = record(value, ['id', 'resource', 'hidden']);
  const raw = data.resource;
  if (typeof raw !== 'object' || raw === null || !('kind' in raw)) fail('invalid-layout');
  let resource: ResourceReference;
  if (raw.kind === 'terminal') {
    const ref = record(raw, ['kind', 'instanceId', 'threadId', 'profileId']);
    resource = { kind: 'terminal', instanceId: nullableId(ref.instanceId), threadId: nullableId(ref.threadId), profileId: nullableId(ref.profileId) };
  } else {
    const ref = record(raw, ['kind', 'resourceId']);
    switch (ref.kind) {
      case 'acp': case 'companion': case 'sessions': case 'files': case 'markdown': case 'preview': case 'browser': case 'team': case 'mini-app':
        resource = { kind: ref.kind, resourceId: id(ref.resourceId) }; break;
      default: fail('invalid-layout');
    }
  }
  return Object.freeze({ id: id(data.id), resource: Object.freeze(resource), hidden: bool(data.hidden) });
}

export function parseWorkspaceLayout(value: unknown): LayoutResult<WorkspaceLayout> {
  try {
    if (typeof value === 'object' && value !== null && 'version' in value && value.version !== 1) fail('unsupported-version');
    const data = record(value, ['version', 'root', 'views', 'focusedGroupId', 'maximizedGroupId']);
    if (data.version !== 1) fail('unsupported-version');
    if (!Array.isArray(data.views) || data.views.length > WORKSPACE_LIMITS.views) fail('bounds');
    const views = data.views.map(view); const ids = new Set<string>(); const instances = new Set<string>();
    const viewsById = new Map<string, PanelView>();
    for (const panel of views) {
      if (ids.has(panel.id)) fail('duplicate'); ids.add(panel.id); viewsById.set(panel.id, panel);
      if (panel.resource.kind === 'terminal' && panel.resource.instanceId !== null) {
        if (instances.has(panel.resource.instanceId)) fail('duplicate'); instances.add(panel.resource.instanceId);
      }
    }
    const seen = new WeakSet<object>(); const membership = new Set<string>(); const groups = new Map<string, TabGroup>(); let nodes = 0;
    const readNode = (raw: unknown, depth: number): LayoutNode => {
      if (depth > WORKSPACE_LIMITS.depth || ++nodes > WORKSPACE_LIMITS.nodes) fail('bounds');
      if (typeof raw !== 'object' || raw === null || seen.has(raw) || !('kind' in raw)) fail('invalid-layout'); seen.add(raw);
      const node = record(raw, raw.kind === 'group' ? ['kind', 'id', 'tabs', 'activeTabId', 'collapsed', 'lastUsableSize'] : ['kind', 'id', 'axis', 'children', 'weights']);
      const nodeId = id(node.id); if (ids.has(nodeId)) fail('duplicate'); ids.add(nodeId);
      if (node.kind === 'group') {
        if (!Array.isArray(node.tabs) || node.tabs.length > WORKSPACE_LIMITS.views) fail('bounds');
        const tabs = node.tabs.map(id); const activeTabId = nullableId(node.activeTabId);
        for (const tab of tabs) { if (!viewsById.has(tab) || membership.has(tab)) fail('membership'); membership.add(tab); }
        const visible = tabs.filter(tab => !viewsById.get(tab)?.hidden);
        if (visible.length ? activeTabId === null || !visible.includes(activeTabId) : activeTabId !== null) fail('membership');
        if (tabs.length === 0 && depth !== 0) fail('membership');
        const group: TabGroup = Object.freeze({ kind: 'group', id: nodeId, tabs: Object.freeze(tabs), activeTabId, collapsed: bool(node.collapsed), lastUsableSize: size(node.lastUsableSize) });
        groups.set(nodeId, group); if (groups.size > WORKSPACE_LIMITS.groups) fail('bounds'); return group;
      }
      if (node.kind !== 'split' || (node.axis !== 'columns' && node.axis !== 'rows')
        || !Array.isArray(node.children) || !Array.isArray(node.weights) || node.children.length < 2
        || node.children.length > WORKSPACE_LIMITS.groups || node.children.length !== node.weights.length) fail('invalid-layout');
      const weights = node.weights.map(weight => {
        if (typeof weight !== 'number' || !Number.isFinite(weight) || weight <= 0 || weight > 1_000_000) fail('bounds'); return weight;
      });
      return Object.freeze({ kind: 'split', id: nodeId, axis: node.axis, children: Object.freeze(node.children.map(child => readNode(child, depth + 1))), weights: Object.freeze(weights) });
    };
    const root = readNode(data.root, 0);
    if (membership.size !== views.length) fail('membership');
    const focusedGroupId = nullableId(data.focusedGroupId); const maximizedGroupId = nullableId(data.maximizedGroupId);
    const usable = (groupId: string | null): boolean => groupId !== null && groups.has(groupId) && !groups.get(groupId)?.collapsed && groups.get(groupId)?.activeTabId !== null;
    if ((focusedGroupId !== null && !usable(focusedGroupId)) || (maximizedGroupId !== null && !usable(maximizedGroupId))) fail('membership');
    if (focusedGroupId === null && [...groups.values()].some(group => !group.collapsed && group.activeTabId !== null)) fail('membership');
    return { ok: true, value: Object.freeze({ version: 1, root, views: Object.freeze(views), focusedGroupId, maximizedGroupId }) };
  } catch (error) { return { ok: false, error: error instanceof InvalidLayout ? error.code : 'invalid-layout' }; }
}

export function emptyWorkspace(groupId = 'group-main'): WorkspaceLayout {
  const group: TabGroup = Object.freeze({ kind: 'group', id: id(groupId), tabs: Object.freeze([]), activeTabId: null, collapsed: false, lastUsableSize: MIN_PANE_SIZE });
  return Object.freeze({ version: 1, root: group, views: Object.freeze([]), focusedGroupId: null, maximizedGroupId: null });
}
export function serializeWorkspaceLayout(layout: WorkspaceLayout): LayoutResult<string> {
  const parsed = parseWorkspaceLayout(layout); if (!parsed.ok) return parsed;
  const json = JSON.stringify(parsed.value);
  return new TextEncoder().encode(json).byteLength <= WORKSPACE_LIMITS.jsonBytes ? { ok: true, value: json } : { ok: false, error: 'bounds' };
}
export function restoreWorkspaceLayout(raw: string | null, lastValid: WorkspaceLayout): { readonly layout: WorkspaceLayout; readonly restored: boolean; readonly error?: LayoutError } {
  try {
    if (raw === null) return { layout: lastValid, restored: false };
    if (raw.length > WORKSPACE_LIMITS.jsonBytes || new TextEncoder().encode(raw).byteLength > WORKSPACE_LIMITS.jsonBytes) fail('bounds');
    const parsed = parseWorkspaceLayout(JSON.parse(raw) as unknown);
    if (parsed.ok) return { layout: parsed.value, restored: true };
    return { layout: lastValid, restored: false, error: parsed.error };
  } catch (error) { return { layout: lastValid, restored: false, error: error instanceof InvalidLayout ? error.code : 'invalid-layout' }; }
}

export function tabGroups(node: LayoutNode): readonly TabGroup[] {
  const pending: LayoutNode[] = [node]; const seen = new Set<LayoutNode>(); const groups: TabGroup[] = [];
  while (pending.length) {
    const current = pending.pop(); if (!current) continue;
    if (seen.has(current) || seen.size >= WORKSPACE_LIMITS.nodes) fail('invalid-layout'); seen.add(current);
    if (current.kind === 'group') groups.push(current);
    else { if (current.children.length > WORKSPACE_LIMITS.groups) fail('bounds'); pending.push(...[...current.children].reverse()); }
  }
  return groups;
}
function mapNode(node: LayoutNode, transform: (node: LayoutNode) => LayoutNode): LayoutNode {
  return transform(node.kind === 'group' ? node : { ...node, children: node.children.map(child => mapNode(child, transform)) });
}
function normalize(node: LayoutNode): LayoutNode | null {
  if (node.kind === 'group') return node.tabs.length ? node : null;
  const kept = node.children.map((child, index) => ({ node: normalize(child), weight: node.weights[index] })).filter(child => child.node !== null);
  if (!kept.length) return null; if (kept.length === 1) return kept[0].node;
  return { ...node, children: kept.map(child => child.node as LayoutNode), weights: kept.map(child => child.weight) };
}
function repair(layout: WorkspaceLayout, preferredGroupId: string | null): WorkspaceLayout {
  const byId = new Map(layout.views.map(panel => [panel.id, panel]));
  const root = mapNode(layout.root, node => {
    if (node.kind === 'split') return node;
    const visible = node.tabs.filter(tab => !byId.get(tab)?.hidden);
    return { ...node, activeTabId: node.activeTabId !== null && visible.includes(node.activeTabId) ? node.activeTabId : visible[0] ?? null };
  });
  const groups = tabGroups(root); const available = groups.filter(group => !group.collapsed && group.activeTabId !== null);
  const focusedGroupId = available.find(group => group.id === preferredGroupId)?.id ?? available[0]?.id ?? null;
  const maximizedGroupId = available.some(group => group.id === layout.maximizedGroupId) ? layout.maximizedGroupId : null;
  return { ...layout, root, focusedGroupId, maximizedGroupId };
}
function insertTab(group: TabGroup, viewId: string, index: number | undefined): TabGroup {
  const at = index ?? group.tabs.length;
  if (!Number.isInteger(at) || at < 0 || at > group.tabs.length) fail('invalid-action');
  const tabs = [...group.tabs]; tabs.splice(at, 0, viewId); return { ...group, tabs, activeTabId: viewId, collapsed: false };
}
function splitGroup(target: TabGroup, viewId: string, edge: Edge): SplitNode {
  const group: TabGroup = { kind: 'group', id: id(edge.newGroupId), tabs: [viewId], activeTabId: viewId, collapsed: false, lastUsableSize: target.lastUsableSize };
  if ((edge.axis !== 'columns' && edge.axis !== 'rows') || (edge.side !== 'before' && edge.side !== 'after')) fail('invalid-action');
  return { kind: 'split', id: id(edge.newSplitId), axis: edge.axis, children: edge.side === 'before' ? [group, target] : [target, group], weights: [1, 1] };
}

export function reduceWorkspaceLayout(original: WorkspaceLayout, action: LayoutAction): LayoutMutation {
  if (action.type === 'cancel') return { ok: true, layout: original, changed: false };
  const parsed = parseWorkspaceLayout(original); if (!parsed.ok) return { ok: false, layout: original, error: parsed.error };
  try {
    const base = parsed.value; let root = base.root; let views = [...base.views];
    let focusedGroupId = base.focusedGroupId; let maximizedGroupId = base.maximizedGroupId;
    const group = (groupId: string): TabGroup => { const found = tabGroups(root).find(group => group.id === groupId); if (!found) fail('unknown-target'); return found; };
    const panel = (viewId: string): PanelView => { const found = views.find(view => view.id === viewId); if (!found) fail('unknown-target'); return found; };
    switch (action.type) {
      case 'remove-view': {
        panel(action.viewId);
        views = views.filter(view => view.id !== action.viewId);
        root = mapNode(root, node => node.kind === 'group' ? { ...node, tabs: node.tabs.filter(tab => tab !== action.viewId) } : node);
        break;
      }
      case 'add-view': case 'split': {
        const target = group(action.groupId); const added = view(action.view); views.push(added);
        root = mapNode(root, node => node.id !== target.id ? node : action.type === 'add-view'
          ? insertTab(target, added.id, action.index) : splitGroup(target, added.id, action));
        focusedGroupId = action.type === 'split' ? action.newGroupId : target.id; break;
      }
      case 'move': {
        const moved = panel(action.viewId); const target = group(action.groupId);
        const source = tabGroups(root).find(group => group.tabs.includes(moved.id)); if (!source) fail('membership');
        if (action.edge && source.id === target.id && source.tabs.length === 1) fail('invalid-action');
        root = mapNode(root, node => node.kind === 'group' ? { ...node, tabs: node.tabs.filter(tab => tab !== moved.id) } : node);
        root = mapNode(root, node => node.id !== target.id || node.kind !== 'group' ? node
          : action.edge ? splitGroup(node, moved.id, action.edge) : insertTab(node, moved.id, action.index));
        focusedGroupId = action.edge?.newGroupId ?? target.id; break;
      }
      case 'resize': {
        let found = false;
        root = mapNode(root, node => { if (node.id !== action.splitId || node.kind !== 'split') return node; found = true; return { ...node, weights: [...action.weights] }; });
        if (!found) fail('unknown-target'); break;
      }
      case 'focus': case 'activate': {
        const target = group(action.groupId);
        if (target.collapsed) fail('invalid-action');
        if (action.type === 'activate') {
          if (!target.tabs.includes(action.viewId) || panel(action.viewId).hidden) fail('membership');
          root = mapNode(root, node => node.id === target.id ? { ...target, activeTabId: action.viewId } : node);
        } else if (target.activeTabId === null) fail('invalid-action');
        focusedGroupId = target.id; if (maximizedGroupId !== null && maximizedGroupId !== target.id) maximizedGroupId = null; break;
      }
      case 'hide': panel(action.viewId); views = views.map(view => view.id === action.viewId ? { ...view, hidden: action.hidden } : view); break;
      case 'collapse': { const target = group(action.groupId); root = mapNode(root, node => node.id === target.id ? { ...target, collapsed: action.collapsed } : node); break; }
      case 'maximize': if (action.groupId !== null) { const target = group(action.groupId); if (target.collapsed || target.activeTabId === null) fail('invalid-action'); focusedGroupId = target.id; } maximizedGroupId = action.groupId; break;
      case 'remember-size': { const target = group(action.groupId); const usable = size(action.size); root = mapNode(root, node => node.id === target.id ? { ...target, lastUsableSize: usable } : node); break; }
      default: fail('invalid-action');
    }
    root = normalize(root) ?? emptyWorkspace(tabGroups(base.root)[0].id).root;
    const next = parseWorkspaceLayout(repair({ version: 1, root, views, focusedGroupId, maximizedGroupId }, focusedGroupId));
    if (!next.ok) return { ok: false, layout: original, error: next.error };
    return { ok: true, layout: next.value, changed: true };
  } catch (error) { return { ok: false, layout: original, error: error instanceof InvalidLayout ? error.code : 'invalid-action' }; }
}

export interface LayoutGesture { readonly base: WorkspaceLayout; readonly preview: WorkspaceLayout }
export function beginLayoutGesture(base: WorkspaceLayout): LayoutGesture { return { base, preview: base }; }
export function previewLayoutGesture(gesture: LayoutGesture, action: LayoutAction): LayoutResult<LayoutGesture> {
  const result = reduceWorkspaceLayout(gesture.base, action); return result.ok ? { ok: true, value: { base: gesture.base, preview: result.layout } } : result;
}
export function cancelLayoutGesture(gesture: LayoutGesture): WorkspaceLayout { return gesture.base; }
export function commitLayoutGesture(gesture: LayoutGesture, current: WorkspaceLayout): LayoutMutation {
  if (current !== gesture.base) return { ok: false, layout: current, error: 'conflict' };
  const parsed = parseWorkspaceLayout(gesture.preview); return parsed.ok ? { ok: true, layout: parsed.value, changed: gesture.preview !== gesture.base } : { ok: false, layout: current, error: parsed.error };
}

export interface PaneRect extends UsableSize { readonly groupId: string; readonly x: number; readonly y: number; readonly visible: boolean }
function minimum(node: LayoutNode): UsableSize {
  if (node.kind === 'group') return node.collapsed ? { width: 48, height: 32 } : MIN_PANE_SIZE;
  const sizes = node.children.map(minimum);
  return node.axis === 'columns' ? { width: sizes.reduce((sum, size) => sum + size.width, 0), height: Math.max(...sizes.map(size => size.height)) }
    : { width: Math.max(...sizes.map(size => size.width)), height: sizes.reduce((sum, size) => sum + size.height, 0) };
}
export function paneRects(layout: WorkspaceLayout, viewport: UsableSize): { readonly width: number; readonly height: number; readonly panes: readonly PaneRect[] } {
  const parsed = parseWorkspaceLayout(layout); if (!parsed.ok) throw new Error('Invalid workspace layout.');
  if (![viewport.width, viewport.height].every(value => Number.isFinite(value) && value > 0 && value <= 32768)) throw new Error('Invalid workspace viewport.');
  const min = minimum(parsed.value.root); const width = Math.max(viewport.width, min.width); const height = Math.max(viewport.height, min.height); const panes: PaneRect[] = [];
  const visit = (node: LayoutNode, x: number, y: number, w: number, h: number): void => {
    if (node.kind === 'group') { panes.push({ groupId: node.id, x, y, width: w, height: h, visible: layout.maximizedGroupId === null || layout.maximizedGroupId === node.id }); return; }
    const mins = node.children.map(child => minimum(child)); const weights = node.weights.reduce((sum, weight) => sum + weight, 0);
    const reserved = mins.reduce((sum, min) => sum + (node.axis === 'columns' ? min.width : min.height), 0);
    const extra = Math.max(0, (node.axis === 'columns' ? w : h) - reserved); let offset = 0;
    node.children.forEach((child, index) => { const extent = (node.axis === 'columns' ? mins[index].width : mins[index].height) + extra * (node.weights[index] / weights);
      visit(child, x + (node.axis === 'columns' ? offset : 0), y + (node.axis === 'rows' ? offset : 0), node.axis === 'columns' ? extent : w, node.axis === 'rows' ? extent : h); offset += extent; });
  };
  visit(parsed.value.root, 0, 0, width, height);
  return { width, height, panes: panes.map(pane => pane.groupId === layout.maximizedGroupId ? { ...pane, x: 0, y: 0, width, height } : pane) };
}
export function keyboardNeighbor(layout: WorkspaceLayout, groupId: string, direction: Direction, viewport: UsableSize): string | null {
  const parsed = parseWorkspaceLayout(layout); if (!parsed.ok) throw new Error('Invalid workspace layout.');
  const groups = new Map(tabGroups(parsed.value.root).map(group => [group.id, group]));
  const panes = paneRects({ ...parsed.value, maximizedGroupId: null }, viewport).panes.filter(pane => !groups.get(pane.groupId)?.collapsed && groups.get(pane.groupId)?.activeTabId !== null);
  const current = panes.find(pane => pane.groupId === groupId); if (!current) return null;
  const cx = current.x + current.width / 2; const cy = current.y + current.height / 2;
  return panes.filter(pane => pane.groupId !== groupId).map(pane => {
    const dx = pane.x + pane.width / 2 - cx; const dy = pane.y + pane.height / 2 - cy;
    const along = direction === 'left' ? -dx : direction === 'right' ? dx : direction === 'up' ? -dy : dy;
    return { pane, along, across: direction === 'left' || direction === 'right' ? Math.abs(dy) : Math.abs(dx) };
  }).filter(candidate => candidate.along > 0).sort((a, b) => (a.along + 2 * a.across) - (b.along + 2 * b.across) || a.pane.groupId.localeCompare(b.pane.groupId))[0]?.pane.groupId ?? null;
}
export function panelAvailability(panel: PanelView, liveInstances: ReadonlySet<string>): { readonly available: boolean; readonly reason?: 'gated-panel' | 'resource-unavailable' } {
  if (panel.resource.kind !== 'terminal') return { available: false, reason: 'gated-panel' };
  return panel.resource.instanceId !== null && liveInstances.has(panel.resource.instanceId) ? { available: true } : { available: false, reason: 'resource-unavailable' };
}
