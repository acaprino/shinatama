// Import workspace-renderer.css once from the desktop shell. Runtime ownership
// stays outside this renderer: bindings are existing DOM, dispatch is model-only.
import {
  beginLayoutGesture, commitLayoutGesture, keyboardNeighbor, MIN_PANE_SIZE, paneRects,
  parseWorkspaceLayout, previewLayoutGesture, tabGroups,
} from './workspace-model.ts';
import type {
  Direction, LayoutAction, LayoutError, LayoutGesture, LayoutNode, LayoutResult,
  PaneRect, PanelView, SplitNode, TabGroup, UsableSize, WorkspaceLayout,
} from './workspace-model.ts';

export interface WorkspaceRendererOptions {
  readonly host: HTMLElement;
  readonly bindings: ReadonlyMap<string, HTMLElement>;
  /** Synchronously reduce against expectedLayout; persistence may be debounced. */
  readonly dispatch: (action: LayoutAction, expectedLayout: WorkspaceLayout) => void;
  readonly onFocusView?: (viewId: string) => void;
  /** Update the runtime's visibility cache before any zero-size DOM observation. */
  readonly onVisibilityChange?: (viewId: string, visible: boolean, groupId: string | null) => void;
  readonly labelForView?: (view: PanelView) => string;
  /** The colour of the thread a tab belongs to, drawn as a square before its name; null for none. */
  readonly toneForView?: (view: PanelView) => string | null;
  /** Quick filter: only matching tabs are drawn; null shows every tab. Layout is not changed. */
  readonly tabFilter?: () => { readonly label: string; readonly shows: (view: PanelView) => boolean } | null;
  readonly isInputBlocked?: () => boolean;
  readonly onError?: (error: LayoutError) => void;
  /** The pane's "+" button or empty-pane button: open a new session in this pane. */
  readonly onNewTab?: (groupId: string) => void;
  /** The tab's "×": close (stop) the view. */
  readonly onCloseView?: (viewId: string) => void;
  /** What a terminal view without a running terminal shows; null keeps the plain notice. */
  readonly placeholderFor?: (view: PanelView) => HTMLElement | null;
}
export interface WorkspaceRenderer {
  render(layout: WorkspaceLayout): LayoutResult<void>;
  destroy(): void;
}
interface GroupDom { readonly element: HTMLElement; readonly tabs: HTMLElement; readonly body: HTMLElement; readonly focus: HTMLButtonElement; readonly collapse: HTMLButtonElement; readonly maximize: HTMLButtonElement; readonly add: HTMLButtonElement }
interface Bounds { readonly x: number; readonly y: number; readonly width: number; readonly height: number }
/** A tab held by the mouse. It becomes a drag (`gesture` set) only after it moves a few pixels, so a plain click still activates the tab. */
interface DragState { readonly viewId: string; readonly pointerId: number; readonly startX: number; readonly startY: number; readonly tab: HTMLElement; readonly groupId: string; readonly splitId: string; gesture: LayoutGesture | null; action: LayoutAction | null }
const DRAG_THRESHOLD = 6;
const EDGE_WORDS = { columns: { before: 'left', after: 'right' }, rows: { before: 'top', after: 'bottom' } } as const;
interface ResizeState { readonly gesture: LayoutGesture; readonly split: SplitNode; readonly boundary: number; readonly start: number; readonly free: number; readonly firstFree: number; readonly handle: HTMLElement; readonly pointerId: number; action: LayoutAction | null }

function element<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag); node.className = className; if (text !== undefined) node.textContent = text; return node;
}
function control(label: string, key: string, run: () => void): HTMLButtonElement {
  const button = element('button', 'wm-control', label); button.type = 'button'; button.dataset.wmKey = key; button.addEventListener('click', run); return button;
}
/** Reparent the original view element (terminal or browser tab); never clone, clear, dispose or reopen it. */
export function placeWorkspaceBinding(parent: HTMLElement, binding: HTMLElement, visible: boolean): void {
  binding.hidden = !visible;
  if (binding.parentNode !== parent) parent.appendChild(binding);
}
function minimum(node: LayoutNode): UsableSize {
  if (node.kind === 'group') return node.collapsed ? { width: 48, height: 32 } : MIN_PANE_SIZE;
  const sizes = node.children.map(minimum);
  return node.axis === 'columns' ? { width: sizes.reduce((sum, size) => sum + size.width, 0), height: Math.max(...sizes.map(size => size.height)) }
    : { width: Math.max(...sizes.map(size => size.width)), height: sizes.reduce((sum, size) => sum + size.height, 0) };
}
function bounds(node: LayoutNode, panes: ReadonlyMap<string, PaneRect>): Bounds {
  const rects = tabGroups(node).map(group => panes.get(group.id)).filter((rect): rect is PaneRect => rect !== undefined);
  const x = Math.min(...rects.map(rect => rect.x)); const y = Math.min(...rects.map(rect => rect.y));
  return { x, y, width: Math.max(...rects.map(rect => rect.x + rect.width)) - x, height: Math.max(...rects.map(rect => rect.y + rect.height)) - y };
}

export function createWorkspaceRenderer(options: WorkspaceRendererOptions): WorkspaceRenderer {
  const canvas = element('div', 'wm-canvas'); const parking = element('div', 'wm-parking'); parking.hidden = true; parking.setAttribute('aria-hidden', 'true');
  const preview = element('div', 'wm-drop-preview'); preview.hidden = true; preview.setAttribute('aria-hidden', 'true');
  const previewLabel = element('span', 'wm-drop-label'); preview.appendChild(previewLabel);
  const marker = element('div', 'wm-drop-marker'); marker.hidden = true; marker.setAttribute('aria-hidden', 'true');
  const ghost = element('div', 'wm-drag-ghost'); ghost.hidden = true; ghost.setAttribute('aria-hidden', 'true');
  const status = element('div', 'wm-status'); status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
  options.host.classList.add('wm-host'); options.host.append(canvas, parking, status, ghost); canvas.append(preview, marker);
  const groups = new Map<string, GroupDom>(); const splitters = new Map<string, HTMLElement>(); const visibility = new Map<string, { visible: boolean; groupId: string | null }>();
  const namespace = `wm-${crypto.randomUUID()}`;
  let current: WorkspaceLayout | null = null; let drag: DragState | null = null; let resize: ResizeState | null = null;
  let disposed = false; let rendering = false; let pending: WorkspaceLayout | null = null; let sizeFrame = 0;
  const blocked = (): boolean => options.isInputBlocked?.() ?? false;
  const viewport = (): UsableSize => ({ width: Math.max(1, options.host.clientWidth), height: Math.max(1, options.host.clientHeight) });
  const report = (error: LayoutError): void => { status.textContent = 'That layout change is unavailable.'; options.onError?.(error); };
  const label = (view: PanelView): string => (options.labelForView?.(view) ?? (view.resource.kind === 'terminal' ? 'Terminal' : view.resource.kind)).slice(0, 128);
  const emit = (action: LayoutAction, expected = current): void => { if (disposed || blocked() || !expected) return; status.textContent = ''; options.dispatch(action, expected); };
  const focusView = (viewId: string | null): void => { if (viewId !== null && !blocked()) options.onFocusView?.(viewId); };
  const focusChrome = (groupId: string, key: string): void => { queueMicrotask(() => {
    if (disposed || blocked()) return;
    groups.get(groupId)?.element.querySelectorAll<HTMLElement>('[data-wm-key]').forEach(node => { if (node.dataset.wmKey === key) node.focus({ preventScroll: true }); });
  }); };
  const markVisibility = (viewId: string, visible: boolean, groupId: string | null): void => {
    const previous = visibility.get(viewId);
    if (!previous || previous.visible !== visible || previous.groupId !== groupId) { visibility.set(viewId, { visible, groupId }); options.onVisibilityChange?.(viewId, visible, groupId); }
  };
  const freshId = (kind: string): string => `wm-${kind}-${crypto.randomUUID()}`;

  function cancelGesture(): void {
    if (drag) {
      const held = drag; drag = null;
      window.removeEventListener('pointermove', moveTab, true); window.removeEventListener('pointerup', dropTab, true);
      window.removeEventListener('pointercancel', cancelGesture, true); window.removeEventListener('keydown', escapeDrag, true);
      held.tab.classList.remove('wm-tab-dragging'); if (held.tab.hasPointerCapture(held.pointerId)) held.tab.releasePointerCapture(held.pointerId);
    }
    if (resize) { const previous = resize; resize = null; if (previous.handle.hasPointerCapture(previous.pointerId)) previous.handle.releasePointerCapture(previous.pointerId); }
    preview.hidden = true; marker.hidden = true; ghost.hidden = true; options.host.classList.remove('wm-changing', 'wm-dragging');
    if (current && !disposed) drawGeometry(current);
  }
  function commitGesture(gesture: LayoutGesture, action: LayoutAction | null): void {
    if (!current || !action || blocked()) { cancelGesture(); return; }
    const checked = previewLayoutGesture(gesture, action);
    if (!checked.ok) { report(checked.error); cancelGesture(); return; }
    const committed = commitLayoutGesture(checked.value, current);
    const expected = current; cancelGesture();
    if (!committed.ok) { report(committed.error); return; }
    emit(action, expected);
  }

  /** Where the held tab would land: the tab strip reorders, a pane's centre joins its tabs, a side splits the pane there. */
  function dropAction(x: number, y: number, hit: Element, target: TabGroup): { action: LayoutAction; before: HTMLElement | null } | null {
    if (!drag || !current) return null;
    const dom = groups.get(target.id); if (!dom) return null;
    const source = tabGroups(current.root).find(group => group.tabs.includes(drag?.viewId ?? ''));
    if (hit.closest('.wm-header')) {
      const before = [...dom.tabs.querySelectorAll<HTMLElement>('.wm-tab')].find(tab => { const box = tab.getBoundingClientRect(); return x < box.left + box.width / 2; }) ?? null;
      let index = before?.dataset.viewId ? target.tabs.indexOf(before.dataset.viewId) : target.tabs.length;
      if (source?.id === target.id && source.tabs.indexOf(drag.viewId) < index) index -= 1;
      if (source?.id === target.id && source.tabs.indexOf(drag.viewId) === index) return null;
      return { action: { type: 'move', viewId: drag.viewId, groupId: target.id, index }, before };
    }
    const box = dom.body.getBoundingClientRect(); if (!box.width || !box.height) return null;
    const dx = (x - box.left) / box.width - 0.5; const dy = (y - box.top) / box.height - 0.5;
    if (Math.abs(dx) < 0.25 && Math.abs(dy) < 0.25) return source?.id === target.id ? null : { action: { type: 'move', viewId: drag.viewId, groupId: target.id }, before: null };
    const edge = Math.abs(dx) >= Math.abs(dy) ? { axis: 'columns' as const, side: dx < 0 ? 'before' as const : 'after' as const }
      : { axis: 'rows' as const, side: dy < 0 ? 'before' as const : 'after' as const };
    return { action: { type: 'move', viewId: drag.viewId, groupId: target.id, edge: { ...edge, newGroupId: drag.groupId, newSplitId: drag.splitId } }, before: null };
  }
  function showDrop(target: TabGroup, action: LayoutAction, before: HTMLElement | null): void {
    if (!current || action.type !== 'move') return;
    const dom = groups.get(target.id); if (!dom) return;
    const origin = canvas.getBoundingClientRect();
    if (action.index !== undefined) {
      const strip = dom.tabs.getBoundingClientRect(); const last = dom.tabs.querySelector<HTMLElement>('.wm-tabwrap:last-of-type');
      const at = before ? before.getBoundingClientRect().left : last ? last.getBoundingClientRect().right : strip.left;
      Object.assign(marker.style, { left: `${at - origin.left - 1}px`, top: `${strip.top - origin.top}px`, height: `${strip.height}px` }); marker.hidden = false; return;
    }
    const pane = paneRects(current, viewport()).panes.find(pane => pane.groupId === target.id); if (!pane) return;
    let { x, y, width, height } = pane;
    if (action.edge) {
      if (action.edge.axis === 'columns') { width /= 2; if (action.edge.side === 'after') x += width; }
      else { height /= 2; if (action.edge.side === 'after') y += height; }
    }
    previewLabel.textContent = action.edge ? `New pane on the ${EDGE_WORDS[action.edge.axis][action.edge.side]}` : 'Move to this pane';
    Object.assign(preview.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` }); preview.hidden = false;
  }
  function holdTab(event: PointerEvent, viewId: string, tab: HTMLElement): void {
    if (event.button !== 0 || !current || blocked() || drag || resize) return;
    drag = { viewId, pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, tab, groupId: freshId('group'), splitId: freshId('split'), gesture: null, action: null };
    window.addEventListener('pointermove', moveTab, true); window.addEventListener('pointerup', dropTab, true);
    window.addEventListener('pointercancel', cancelGesture, true); window.addEventListener('keydown', escapeDrag, true);
  }
  function moveTab(event: PointerEvent): void {
    if (!drag || !current || event.pointerId !== drag.pointerId) return;
    if (blocked() || !drag.tab.isConnected) { cancelGesture(); return; }
    if (!drag.gesture) {
      if (Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < DRAG_THRESHOLD) return;
      drag.gesture = beginLayoutGesture(current); drag.tab.setPointerCapture(event.pointerId); drag.tab.classList.add('wm-tab-dragging');
      ghost.textContent = drag.tab.textContent ?? ''; ghost.hidden = false; options.host.classList.add('wm-changing', 'wm-dragging');
    }
    event.preventDefault();
    ghost.style.transform = `translate(${event.clientX + 14}px, ${event.clientY + 12}px)`;
    drag.action = null; preview.hidden = true; marker.hidden = true;
    const hit = document.elementFromPoint(event.clientX, event.clientY); const paneNode = hit?.closest<HTMLElement>('.wm-pane');
    const group = hit && paneNode && canvas.contains(paneNode) ? tabGroups(current.root).find(group => group.id === paneNode.dataset.groupId) : undefined;
    if (!hit || !group || group.collapsed) return;
    const drop = dropAction(event.clientX, event.clientY, hit, group);
    if (drop && previewLayoutGesture(drag.gesture, drop.action).ok) { drag.action = drop.action; showDrop(group, drop.action, drop.before); }
  }
  function dropTab(event: PointerEvent): void {
    if (!drag || event.pointerId !== drag.pointerId) return;
    const held = drag;
    if (!held.gesture) { cancelGesture(); return; }
    // The release after a drag is not a click on whatever sits under the pointer.
    const swallow = (click: MouseEvent): void => { click.stopPropagation(); click.preventDefault(); };
    window.addEventListener('click', swallow, { capture: true, once: true }); setTimeout(() => window.removeEventListener('click', swallow, true), 0);
    event.preventDefault(); const viewId = held.viewId; const action = held.action;
    commitGesture(held.gesture, action); if (action) focusView(viewId);
  }
  function escapeDrag(event: KeyboardEvent): void { if (event.key === 'Escape' && drag) { event.preventDefault(); event.stopPropagation(); cancelGesture(); } }

  function makeGroup(groupId: string): GroupDom {
    const pane = element('section', 'wm-pane'); pane.dataset.groupId = groupId; pane.setAttribute('aria-label', 'Workspace pane');
    const header = element('div', 'wm-header wm-chrome');
    const focus = control('Pane', 'pane-focus', () => { const group = current && tabGroups(current.root).find(group => group.id === groupId); if (group) { emit({ type: 'focus', groupId }); focusView(group.activeTabId); } });
    focus.classList.add('wm-pane-focus'); focus.title = 'Arrow keys focus adjacent panes. Enter focuses the active view.';
    const tabs = element('div', 'wm-tabs wm-chrome'); tabs.setAttribute('role', 'tablist'); tabs.setAttribute('aria-label', 'Pane tabs');
    const collapse = control('−', 'collapse', () => { const group = current && tabGroups(current.root).find(group => group.id === groupId); if (group) emit({ type: 'collapse', groupId, collapsed: !group.collapsed }); });
    const maximize = control('□', 'maximize', () => emit({ type: 'maximize', groupId: current?.maximizedGroupId === groupId ? null : groupId }));
    const add = control('+', 'new-tab', () => options.onNewTab?.(groupId)); add.classList.add('wm-add'); add.title = 'New session in this pane'; add.setAttribute('aria-label', 'New session in this pane');
    collapse.classList.add('wm-pane-tools');
    header.append(focus, tabs, add, collapse, maximize);
    const body = element('div', 'wm-body'); body.id = `${namespace}-panel-${groupId}`; body.setAttribute('role', 'tabpanel');
    pane.append(header, body); canvas.appendChild(pane);
    pane.addEventListener('pointerdown', event => {
      if (blocked() || (event.target instanceof Element && event.target.closest('.wm-chrome'))) return;
      const group = current && tabGroups(current.root).find(group => group.id === groupId);
      if (group?.activeTabId && !group.collapsed && current?.focusedGroupId !== groupId) emit({ type: 'focus', groupId });
    });
    pane.addEventListener('focusin', event => {
      if (event.target instanceof Element && event.target.closest('.wm-chrome')) return;
      const group = current && tabGroups(current.root).find(group => group.id === groupId);
      if (!blocked() && group?.activeTabId && !group.collapsed && current?.focusedGroupId !== groupId) emit({ type: 'focus', groupId });
    });
    return { element: pane, tabs, body, focus, collapse, maximize, add };
  }

  function buildChrome(group: TabGroup, index: number, dom: GroupDom): void {
    if (!current) return;
    dom.element.classList.toggle('wm-collapsed', group.collapsed); dom.element.classList.toggle('wm-focused', current.focusedGroupId === group.id);
    dom.focus.textContent = `Pane ${index + 1}`; dom.focus.hidden = tabGroups(current.root).length < 2; dom.collapse.hidden = dom.maximize.hidden = tabGroups(current.root).length < 2; dom.collapse.setAttribute('aria-label', group.collapsed ? 'Restore pane' : 'Collapse pane');
    dom.collapse.title = group.collapsed ? 'Restore pane' : 'Collapse pane'; dom.collapse.textContent = group.collapsed ? '+' : '−';
    dom.maximize.setAttribute('aria-label', current.maximizedGroupId === group.id ? 'Restore layout' : 'Maximize pane'); dom.maximize.title = dom.maximize.getAttribute('aria-label') ?? '';
    dom.maximize.disabled = group.collapsed || group.activeTabId === null;
    const tabs: HTMLElement[] = []; const filter = options.tabFilter?.() ?? null; let filteredOut = false;
    for (const viewId of group.tabs) {
      const view = current.views.find(view => view.id === viewId); if (!view || view.hidden) continue;
      if (filter && !filter.shows(view)) { filteredOut = true; continue; }
      // A clicked tab hands the focus to its terminal and keeps it there: the tab button lets go first,
      // so the render it triggers does not queue the focus back onto the tab chrome.
      const tab = control(label(view), `tab:${view.id}`, () => {
        tab.blur();
        if (current?.focusedGroupId !== group.id) emit({ type: 'focus', groupId: group.id });
        emit({ type: 'activate', groupId: group.id, viewId: view.id }); focusView(view.id);
      });
      tab.classList.add('wm-tab'); tab.dataset.viewId = view.id; tab.setAttribute('role', 'tab');
      tab.id = `${namespace}-tab-${view.id}`; tab.setAttribute('aria-controls', dom.body.id);
      tab.setAttribute('aria-selected', String(group.activeTabId === view.id)); tab.tabIndex = group.activeTabId === view.id ? 0 : -1;
      tab.title = `${label(view)} · drag to another pane or to a side to split; Alt+Shift+arrow does the same from the keyboard, Delete closes it`;
      // Mouse drag, not HTML drag and drop: on Windows the window's native file drop takes those events.
      tab.addEventListener('pointerdown', event => holdTab(event, view.id, tab));
      const wrap = element('span', 'wm-tabwrap'); wrap.classList.toggle('wm-tabwrap-active', group.activeTabId === view.id);
      const close = control('×', `close:${view.id}`, () => options.onCloseView?.(view.id)); close.classList.add('wm-tab-close');
      close.title = `Close ${label(view)}`; close.setAttribute('aria-label', `Close ${label(view)}`); close.tabIndex = -1;
      const tone = options.toneForView?.(view) ?? null;
      wrap.classList.toggle('wm-tab-toned', tone !== null); if (tone) wrap.style.setProperty('--tab-tone', tone);
      wrap.append(tab, close); tabs.push(wrap);
    }
    // Every tab of this pane belongs to other projects: say so instead of an empty strip.
    const empty = filter !== null && filteredOut && tabs.length === 0;
    if (empty) tabs.push(element('span', 'wm-tabs-filtered', `No ${filter.label} tabs here`));
    dom.element.classList.toggle('wm-filtered-out', empty);
    dom.tabs.replaceChildren(...tabs);
    if (group.activeTabId !== null) dom.body.setAttribute('aria-labelledby', `${namespace}-tab-${group.activeTabId}`); else dom.body.removeAttribute('aria-labelledby');
    dom.body.hidden = group.collapsed;
  }

  function position(node: HTMLElement, rect: Bounds): void { Object.assign(node.style, { left: `${rect.x}px`, top: `${rect.y}px`, width: `${rect.width}px`, height: `${rect.height}px` }); }
  function drawGeometry(layout: WorkspaceLayout): void {
    const geometry = paneRects(layout, viewport()); canvas.style.width = `${geometry.width}px`; canvas.style.height = `${geometry.height}px`;
    const panes = new Map(geometry.panes.map(pane => [pane.groupId, pane]));
    for (const [groupId, dom] of groups) { const rect = panes.get(groupId); dom.element.hidden = !rect?.visible; if (rect) position(dom.element, rect); }
    const used = new Set<string>();
    const visit = (node: LayoutNode): void => {
      if (node.kind === 'group') return;
      const box = bounds(node, panes);
      for (let index = 0; index < node.children.length - 1; index += 1) {
        const key = `${node.id}:${index}`; used.add(key); let handle = splitters.get(key);
        if (!handle) {
          handle = element('div', 'wm-splitter wm-chrome'); handle.tabIndex = 0; handle.setAttribute('role', 'separator'); handle.dataset.splitId = node.id; handle.dataset.boundary = String(index); handle.dataset.wmKey = `split:${key}`;
          handle.addEventListener('pointerdown', event => startResize(event, key));
          handle.addEventListener('pointermove', updateResize);
          handle.addEventListener('pointerup', finishResize);
          handle.addEventListener('pointercancel', cancelGesture);
          handle.addEventListener('lostpointercapture', () => { if (resize?.handle === handle) cancelGesture(); });
          splitters.set(key, handle); canvas.appendChild(handle);
        }
        const child = bounds(node.children[index], panes); const columns = node.axis === 'columns';
        position(handle, columns ? { x: child.x + child.width - 3, y: box.y, width: 6, height: box.height } : { x: box.x, y: child.y + child.height - 3, width: box.width, height: 6 });
        handle.dataset.axis = node.axis; handle.setAttribute('aria-orientation', columns ? 'vertical' : 'horizontal'); handle.setAttribute('aria-label', 'Resize adjacent panes'); handle.setAttribute('aria-valuemin', '1'); handle.setAttribute('aria-valuemax', '99');
        handle.setAttribute('aria-valuenow', String(Math.round(100 * node.weights[index] / (node.weights[index] + node.weights[index + 1])))); handle.hidden = layout.maximizedGroupId !== null;
      }
      node.children.forEach(visit);
    };
    visit(layout.root); for (const [key, handle] of splitters) if (!used.has(key)) { handle.remove(); splitters.delete(key); }
  }
  function findSplit(node: LayoutNode, splitId: string): SplitNode | null {
    if (node.kind === 'group') return null; if (node.id === splitId) return node;
    for (const child of node.children) { const found = findSplit(child, splitId); if (found) return found; } return null;
  }
  function startResize(event: PointerEvent, key: string): void {
    if (!current || blocked() || event.button !== 0) return;
    const handle = splitters.get(key); if (!handle) return;
    const split = findSplit(current.root, handle.dataset.splitId ?? ''); const boundary = Number(handle.dataset.boundary); if (!split || !Number.isInteger(boundary)) return;
    event.preventDefault(); cancelGesture(); handle.focus();
    const panes = new Map(paneRects(current, viewport()).panes.map(pane => [pane.groupId, pane])); const first = bounds(split.children[boundary], panes); const second = bounds(split.children[boundary + 1], panes);
    const firstMinimum = minimum(split.children[boundary]); const secondMinimum = minimum(split.children[boundary + 1]); const columns = split.axis === 'columns';
    const firstFree = (columns ? first.width - firstMinimum.width : first.height - firstMinimum.height);
    const free = firstFree + (columns ? second.width - secondMinimum.width : second.height - secondMinimum.height);
    resize = { gesture: beginLayoutGesture(current), split, boundary, start: columns ? event.clientX : event.clientY, free, firstFree, handle, pointerId: event.pointerId, action: null };
    handle.setPointerCapture(event.pointerId); options.host.classList.add('wm-changing');
  }
  function updateResize(event: PointerEvent): void {
    if (!resize || !current || event.pointerId !== resize.pointerId || resize.free <= 0) return;
    if (blocked()) { cancelGesture(); return; }
    const delta = (resize.split.axis === 'columns' ? event.clientX : event.clientY) - resize.start;
    const fraction = Math.max(0.01, Math.min(0.99, (resize.firstFree + delta) / resize.free)); const weights = [...resize.split.weights]; const total = weights[resize.boundary] + weights[resize.boundary + 1];
    weights[resize.boundary] = total * fraction; weights[resize.boundary + 1] = total * (1 - fraction);
    const action: LayoutAction = { type: 'resize', splitId: resize.split.id, weights }; const checked = previewLayoutGesture(resize.gesture, action);
    if (checked.ok) { resize.action = action; drawGeometry(checked.value.preview); }
  }
  function finishResize(event: PointerEvent): void { if (resize && event.pointerId === resize.pointerId) { const state = resize; commitGesture(state.gesture, state.action); } }

  function onKey(event: KeyboardEvent): void {
    if (blocked() || !current || event.isComposing || !(event.target instanceof HTMLElement)) return;
    const target = event.target; if (!target.closest('.wm-chrome') || !canvas.contains(target)) return;
    if (event.key === 'Escape' && (drag || resize)) { event.preventDefault(); cancelGesture(); return; }
    const pane = target.closest<HTMLElement>('.wm-pane'); const group = pane && tabGroups(current.root).find(group => group.id === pane.dataset.groupId);
    if (target.classList.contains('wm-pane-focus') && group && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) {
      event.preventDefault(); const direction: Direction = event.key === 'ArrowLeft' ? 'left' : event.key === 'ArrowRight' ? 'right' : event.key === 'ArrowUp' ? 'up' : 'down';
      const neighbor = keyboardNeighbor(current, group.id, direction, viewport()); if (neighbor) { emit({ type: 'focus', groupId: neighbor }); focusChrome(neighbor, 'pane-focus'); } return;
    }
    // The keyboard twin of a tab's "×", which stays out of the Tab order.
    if (target.classList.contains('wm-tab') && event.key === 'Delete' && target.dataset.viewId) { event.preventDefault(); options.onCloseView?.(target.dataset.viewId); return; }
    if (target.classList.contains('wm-tab') && group && event.altKey && event.shiftKey && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) {
      event.preventDefault(); const viewId = target.dataset.viewId; if (!viewId) return;
      const direction: Direction = event.key === 'ArrowLeft' ? 'left' : event.key === 'ArrowRight' ? 'right' : event.key === 'ArrowUp' ? 'up' : 'down';
      const axis = direction === 'left' || direction === 'right' ? 'columns' as const : 'rows' as const;
      const side = direction === 'left' || direction === 'up' ? 'before' as const : 'after' as const;
      // The keyboard twin of dragging a tab to a side. A pane's only tab cannot split off itself: it joins the pane on that side.
      if (group.tabs.length > 1) emit({ type: 'move', viewId, groupId: group.id, edge: { axis, side, newGroupId: freshId('group'), newSplitId: freshId('split') } });
      else { const neighbor = keyboardNeighbor(current, group.id, direction, viewport()); if (!neighbor) return; emit({ type: 'move', viewId, groupId: neighbor }); }
      focusView(viewId); return;
    }
    if (target.classList.contains('wm-tab') && group && ['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) {
      event.preventDefault(); const filter = options.tabFilter?.() ?? null;
      const visible = group.tabs.filter(viewId => { const view = current?.views.find(view => view.id === viewId); return !!view && !view.hidden && (!filter || filter.shows(view)); }); const viewId = target.dataset.viewId; const index = visible.indexOf(viewId ?? '');
      if (event.shiftKey && viewId && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) emit({ type: 'move', viewId, groupId: group.id, index: Math.max(0, Math.min(group.tabs.length - 1, group.tabs.indexOf(viewId) + (event.key === 'ArrowLeft' ? -1 : 1))) });
      else { const next = event.key === 'Home' ? visible[0] : event.key === 'End' ? visible.at(-1) : visible[(index + (event.key === 'ArrowLeft' ? -1 : 1) + visible.length) % visible.length]; if (next) { emit({ type: 'activate', groupId: group.id, viewId: next }); focusChrome(group.id, `tab:${next}`); } }
      return;
    }
    if (target.classList.contains('wm-splitter')) {
      const split = findSplit(current.root, target.dataset.splitId ?? ''); const boundary = Number(target.dataset.boundary); if (!split) return;
      const negative = split.axis === 'columns' ? 'ArrowLeft' : 'ArrowUp'; const positive = split.axis === 'columns' ? 'ArrowRight' : 'ArrowDown';
      if (![negative, positive, 'Home'].includes(event.key)) return; event.preventDefault(); const weights = [...split.weights]; const total = weights[boundary] + weights[boundary + 1];
      const fraction = event.key === 'Home' ? 0.5 : Math.max(0.05, Math.min(0.95, weights[boundary] / total + (event.key === negative ? -0.05 : 0.05)));
      weights[boundary] = total * fraction; weights[boundary + 1] = total * (1 - fraction); emit({ type: 'resize', splitId: split.id, weights });
    }
  }
  options.host.addEventListener('keydown', onKey);

  function rememberSizes(): void {
    sizeFrame = 0; if (!current || drag || resize || blocked() || disposed) return;
    const geometry = paneRects(current, viewport()); const byId = new Map(tabGroups(current.root).map(group => [group.id, group]));
    for (const rect of geometry.panes) {
      const group = byId.get(rect.groupId); if (!group || group.collapsed || !rect.visible || group.activeTabId === null || current.maximizedGroupId !== null) continue;
      if (rect.width >= MIN_PANE_SIZE.width && rect.height >= MIN_PANE_SIZE.height && (Math.abs(group.lastUsableSize.width - rect.width) > 1 || Math.abs(group.lastUsableSize.height - rect.height) > 1)) {
        emit({ type: 'remember-size', groupId: group.id, size: { width: rect.width, height: rect.height } }); return;
      }
    }
  }
  function scheduleSizes(): void { if (!sizeFrame) sizeFrame = requestAnimationFrame(rememberSizes); }
  const observer = new ResizeObserver(() => { if (current && !drag && !resize && !disposed) { drawGeometry(current); scheduleSizes(); } }); observer.observe(options.host);

  function render(layout: WorkspaceLayout): LayoutResult<void> {
    if (disposed) return { ok: false, error: 'invalid-action' };
    const parsed = parseWorkspaceLayout(layout); if (!parsed.ok) return parsed;
    if (rendering) { pending = layout; return { ok: true, value: undefined }; }
    rendering = true;
    try {
      const previous = current; current = layout; if ((drag || resize) && previous !== layout) cancelGesture();
      const oldFocus = document.activeElement instanceof HTMLElement && canvas.contains(document.activeElement) ? document.activeElement : null;
      const focusKey = oldFocus?.dataset.wmKey; const focusGroup = oldFocus?.closest<HTMLElement>('.wm-pane')?.dataset.groupId;
      const leaves = tabGroups(parsed.value.root); const liveGroups = new Set(leaves.map(group => group.id)); const liveViews = new Map(parsed.value.views.map(view => [view.id, view]));
      // Hide/park first, so a new parent never exposes an inactive terminal.
      for (const [viewId, binding] of options.bindings) {
        const group = leaves.find(group => group.activeTabId === viewId && liveViews.has(viewId) && !group.collapsed && !liveViews.get(viewId)?.hidden
          && (layout.maximizedGroupId === null || layout.maximizedGroupId === group.id));
        markVisibility(viewId, group !== undefined, group?.id ?? null);
        if (!group) placeWorkspaceBinding(parking, binding, false);
      }
      leaves.forEach((group, index) => {
        let dom = groups.get(group.id); if (!dom) { dom = makeGroup(group.id); groups.set(group.id, dom); }
        buildChrome(group, index, dom);
        const selected = group.activeTabId ? liveViews.get(group.activeTabId) : undefined;
        const binding = selected ? options.bindings.get(selected.id) : undefined;
        if (binding && !group.collapsed && (layout.maximizedGroupId === null || layout.maximizedGroupId === group.id)) {
          for (const child of [...dom.body.children]) if (child !== binding) child.remove(); placeWorkspaceBinding(dom.body, binding, true);
        } else if (selected && options.placeholderFor?.(selected)) {
          dom.body.replaceChildren(options.placeholderFor(selected)!);
        } else {
          const placeholder = element('div', 'wm-placeholder', selected ? selected.resource.kind === 'terminal' ? 'This terminal is no longer running.' : 'This view is not available yet.' : 'Nothing open in this pane.');
          if (!selected) { const start = element('button', 'wm-empty-start', '+ New session'); start.type = 'button'; start.addEventListener('click', () => options.onNewTab?.(group.id)); placeholder.append(start); }
          dom.body.replaceChildren(placeholder);
        }
      });
      for (const [groupId, dom] of groups) if (!liveGroups.has(groupId)) { dom.element.remove(); groups.delete(groupId); }
      drawGeometry(layout); if (focusKey && focusGroup) focusChrome(focusGroup, focusKey); scheduleSizes();
    } finally { rendering = false; }
    const next = pending; pending = null; if (next && next !== current) return render(next);
    return { ok: true, value: undefined };
  }
  return { render, destroy(): void {
    if (disposed) return; cancelGesture(); disposed = true; observer.disconnect(); options.host.removeEventListener('keydown', onKey); if (sizeFrame) cancelAnimationFrame(sizeFrame);
    for (const [viewId, binding] of options.bindings) { markVisibility(viewId, false, null); placeWorkspaceBinding(options.host, binding, false); }
    canvas.remove(); parking.remove(); status.remove(); options.host.classList.remove('wm-host', 'wm-changing'); groups.clear(); splitters.clear();
  } };
}
