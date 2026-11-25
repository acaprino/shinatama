// The layout of the window: the panes and their tabs, the pane the person works in, and the saving of it (one layout for the window,
// whichever threads are on). The model is `workspace-model.ts` (the saved format, unchanged); `tree()` and `applyTree()` are how
// Sigrid's `Workspace` reads and changes it. Rust keeps the layout only: no process is restored from it.
import type { WorkspaceNode } from '@acaprino/sigrid/assemblies';
import { appLog, errorCode, hasHost, invoke, listen, sleep, type Unlisten } from '../api/core.ts';
import { workspaceApi } from '../api/workspace.ts';
import { layoutFromTree, layoutToTree, type DescribeView, type PaneTab } from '../layout-tree.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';
import {
  emptyWorkspace, keyboardNeighbor, parseWorkspaceLayout, reduceWorkspaceLayout, tabGroups,
  type Direction, type LayoutAction, type PanelView, type TabGroup, type WorkspaceLayout,
} from '../workspace-model.ts';
import {
  createTauriWorkspaceTransport, createWorkspacePersistence, layoutLoadFailureText, transientLayoutFailure,
  type WorkspacePersistence, type WorkspacePersistenceStatus,
} from '../workspace-persistence.ts';

/** How long a layout that is owned by another window waits before it asks again. */
const TAKEOVER_RETRY_MS = 30_000;
const SCOPE = { workspaceId: 'main' } as const;

/** A placeholder layout is still the placeholder when the only change is the pane sizes the renderer reports on its own. */
function sameApartFromSizes(a: WorkspaceLayout, b: WorkspaceLayout): boolean {
  if (a === b) return true;
  const bare = (layout: WorkspaceLayout) => JSON.stringify(layout, (key, value) => key === 'lastUsableSize' ? undefined : value);
  return bare(a) === bare(b);
}

export class LayoutController extends Store {
  layout: WorkspaceLayout = emptyWorkspace();
  /** How the saving stands (the title of the status strip says it); null before the layout is connected. */
  saving: WorkspacePersistenceStatus | null = null;
  /** The pane that fills the whole area, when the person maximized one. */
  get maximizedId(): string | null {
    return this.layout.maximizedGroupId;
  }

  private persistence: WorkspacePersistence | null = null;
  private generation = 0;
  /** The notice that another window holds the layout is said once, not at every retry. */
  private takeoverNoticeShown = false;
  private readonly stops: Unlisten[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  async start(): Promise<void> {
    // The shell reloads the page when the person asks (the item of the window menu): what is not saved yet is saved first.
    this.stops.push(await listen<number>('interface-reload-requested', token => {
      void this.flush().finally(() => { void workspaceApi.reloadReady(token).catch(() => {}); });
    }));
  }

  dispose(): void {
    clearTimeout(this.timer);
    for (const stop of this.stops.splice(0)) stop();
    this.persistence?.dispose();
  }

  // --- reading ---------------------------------------------------------------------------------------

  get groups(): readonly TabGroup[] {
    return tabGroups(this.layout.root);
  }

  /** The pane the person works in (the first one when none was chosen). */
  focusedGroup(): string {
    return this.layout.focusedGroupId ?? this.groups[0].id;
  }

  view(id: string): PanelView | undefined {
    return this.layout.views.find(panel => panel.id === id);
  }

  groupOf(viewId: string): TabGroup | undefined {
    return this.groups.find(group => group.tabs.includes(viewId));
  }

  /** The tab shown in the pane the person works in. */
  activeView(): PanelView | undefined {
    const id = this.groups.find(group => group.id === this.focusedGroup())?.activeTabId;
    return id ? this.view(id) : undefined;
  }

  /** The pane next to `groupId` in a direction, for the keys that move between panes. */
  neighbor(groupId: string, direction: Direction, viewport: { width: number; height: number }): string | null {
    return keyboardNeighbor(this.layout, groupId, direction, viewport);
  }

  // --- changing --------------------------------------------------------------------------------------

  /** Applies one action to the layout and saves it. False (with a notice) when the layout refuses it. */
  change(action: LayoutAction, save = true): boolean {
    const result = reduceWorkspaceLayout(this.layout, action);
    if (!result.ok) { this.app.ui.error('That layout change is unavailable.'); return false; }
    this.replace(result.layout, save);
    return true;
  }

  /** Takes a layout made elsewhere (`withoutInstance`, `reconciled`) as the layout of the window. */
  replace(layout: WorkspaceLayout, save = true): void {
    if (layout === this.layout) return;
    this.layout = layout;
    if (save) this.persistence?.update(layout);
    this.changed();
  }

  /** The person works in this pane: the keyboard goes to it. */
  focus(groupId: string): void {
    const group = this.groups.find(candidate => candidate.id === groupId);
    if (group?.activeTabId && this.layout.focusedGroupId !== groupId) this.change({ type: 'focus', groupId });
  }

  /** Brings a tab forward: its pane shows it, the area is not maximized on another pane, and the pane takes the focus. */
  reveal(viewId: string): boolean {
    const group = this.groupOf(viewId);
    if (!group) return false;
    if (this.view(viewId)?.hidden) this.change({ type: 'hide', viewId, hidden: false });
    if (group.collapsed) this.change({ type: 'collapse', groupId: group.id, collapsed: false });
    if (this.layout.maximizedGroupId !== null && this.layout.maximizedGroupId !== group.id) this.change({ type: 'maximize', groupId: null });
    this.change({ type: 'activate', groupId: group.id, viewId });
    this.focus(group.id);
    return true;
  }

  maximize(groupId: string | null): void {
    this.change({ type: 'maximize', groupId });
  }

  /** Adds a view of a kind that is one per window or per thread (the team map, the files, a browser page): a column beside the
   *  pane the person works in at first, then as a tab of the pane that already holds one of its kind. */
  open(view: PanelView, joins: (other: PanelView) => boolean = () => false): void {
    if (this.view(view.id)) { this.reveal(view.id); return; }
    const home = this.groups.find(group => group.tabs.some(id => { const other = this.view(id); return !!other && joins(other); }));
    const focused = this.groups.find(group => group.id === this.focusedGroup());
    if (home) this.change({ type: 'add-view', groupId: home.id, view });
    else if (focused && focused.tabs.length) {
      this.change({ type: 'split', groupId: focused.id, view, axis: 'columns', side: 'after', newGroupId: `g-${view.id}`, newSplitId: `s-${view.id}` });
    } else this.change({ type: 'add-view', groupId: this.focusedGroup(), view });
    const group = this.groupOf(view.id);
    if (group) { this.change({ type: 'activate', groupId: group.id, viewId: view.id }); this.focus(group.id); }
  }

  close(viewId: string): void {
    if (this.view(viewId)) this.change({ type: 'remove-view', viewId });
  }

  /** The preferred axis of the split of the window: columns or rows (the command of the palette). */
  swapAxis(): void {
    const root = this.layout.root;
    if (root.kind !== 'split') return;
    const result = parseWorkspaceLayout({ ...this.layout, root: { ...root, axis: root.axis === 'columns' ? 'rows' : 'columns' } });
    if (result.ok) this.replace(result.value);
  }

  // --- Sigrid's Workspace ------------------------------------------------------------------------------

  /** The layout as the tree `Workspace` draws; `describe` names each tab. */
  tree(describe: DescribeView): WorkspaceNode<PaneTab> {
    return layoutToTree(this.layout, describe);
  }

  /** The person changed the tree (a tab chosen, moved, closed, a pane divided, a divider moved). */
  applyTree(tree: WorkspaceNode<PaneTab>): void {
    const result = layoutFromTree(this.layout, tree);
    if (!result.ok) { this.app.ui.error('That layout change is unavailable. The current workspace was preserved.'); return; }
    this.replace(result.value);
  }

  // --- saving ----------------------------------------------------------------------------------------

  /** Connects saving for the layout on screen. `adoptSaved`: the saved layout replaces the one on screen when this one was not
   *  touched meanwhile. `recovered`: the layout was taken over from a window that is gone. `unchangedSince`: the layout the
   *  caller found on screen, for a retry. */
  async connect(adoptSaved = true, recovered = false, unchangedSince?: WorkspaceLayout): Promise<void> {
    if (!hasHost) return;
    const generation = ++this.generation;
    const previous = this.persistence;
    this.persistence = null;
    if (previous) {
      const released = await previous.release();
      if (!released.ok || released.dirty) this.app.ui.error('The previous layout could not be fully saved.');
      previous.dispose();
    }
    const notify = this.app.ui;
    try {
      const controller = createWorkspacePersistence({
        scope: SCOPE, initialLayout: this.layout,
        transport: createTauriWorkspaceTransport((command, args) => invoke(command, args)),
        onStatus: status => {
          if (generation !== this.generation) return;
          this.saving = status;
          this.changed();
        },
      });
      const beforeLoad = unchangedSince ?? this.layout;
      // Not yet the window's saver: an edit reported while the saved layout loads would count as a local edit and keep it from being
      // restored. A refusal that can pass (storage busy with another operation, still starting) is retried a few times first.
      let loaded = await controller.load();
      for (let attempt = 1; !loaded.ok && transientLayoutFailure(loaded.error) && attempt <= 4 && generation === this.generation; attempt++) {
        appLog('layout', `Load refused (attempt ${attempt}): ${loaded.error}${loaded.detail ? ` (${loaded.detail})` : ''}. Retrying.`);
        await sleep(500 * attempt);
        if (generation === this.generation) loaded = await controller.load();
      }
      if (generation === this.generation) this.persistence = controller;
      if (generation !== this.generation) { controller.dispose(); return; }
      if (!loaded.ok) {
        appLog('layout', `Load refused: ${loaded.error}${loaded.detail ? ` (${loaded.detail})` : ''}. The window keeps its panes and does not save its layout.`);
        notify.error(layoutLoadFailureText(loaded.error, loaded.detail));
        return;
      }
      let adopted = false;
      if (loaded.restored && adoptSaved && sameApartFromSizes(this.layout, beforeLoad)) {
        this.layout = this.app.threads.reconciled(loaded.layout);
        controller.update(this.layout);
        this.changed();
        adopted = true;
      } else controller.update(this.layout);
      if (loaded.ownership === 'other') {
        // Marked by another Shinatama. The store hands the arrangement over only when that process is proven gone (closed badly,
        // crashed), and then it is taken over without asking. While the other one lives, this window keeps its own panes and asks
        // again every 30 s, so it starts saving them as soon as that window closes.
        let code: unknown = 'still_owned';
        if (!recovered) {
          // The takeover loads the saved arrangement again. It replaces the panes only where this load could (it adopted it, or
          // there was none) and only if they are still as this load left them: a tab opened meanwhile, or an edit this load kept, stays.
          const settled = this.layout;
          try {
            await invoke('global_workspace_recover', { request: { ...SCOPE, expectedRevision: loaded.revision } });
            if (generation !== this.generation) return;
            controller.dispose();
            this.persistence = null;
            await this.connect(adoptSaved && (adopted || !loaded.restored), true, settled);
            return;
          } catch (error) { code = errorCode(error); }
        }
        if (generation !== this.generation) return;
        if (!this.takeoverNoticeShown) {
          this.takeoverNoticeShown = true;
          notify.info(code === 'owned_elsewhere'
            ? 'Another Shinatama window is open on the same data. This window\'s panes are remembered once that window closes.'
            : 'The window\'s arrangement could not be taken over yet. Shinatama tries again shortly; the current panes stay.');
        }
        this.timer = setTimeout(() => {
          if (generation !== this.generation) return;
          controller.dispose();
          this.persistence = null;
          void this.connect(false);
        }, TAKEOVER_RETRY_MS);
        return;
      }
      const claimed = await controller.claim();
      if (!claimed.ok && !this.app.ui.runtime.readOnly) notify.error('Layout saving is unavailable. Your current workspace is preserved.');
    } catch {
      if (!this.app.ui.runtime.readOnly) notify.error('Layout storage is unavailable. Your current workspace is preserved.');
    }
  }

  /** Saves what is pending, waiting at most `timeoutMs`. */
  async flush(timeoutMs = 1500): Promise<void> {
    await this.persistence?.flush(timeoutMs).catch(() => undefined);
  }

  /** The layout is released with its running sessions, as the window's close does: the next start takes it over. Resolves to whether
   *  everything was saved. Nothing to release when this window does not own the layout. */
  async release(): Promise<{ released: boolean; unsaved: boolean }> {
    const persistence = this.persistence;
    if (!persistence || persistence.status().ownership !== 'ours') return { released: false, unsaved: false };
    const saved = await persistence.release();
    return { released: true, unsaved: saved.ok ? saved.dirty : persistence.status().dirty };
  }
}
