// The team map of a thread (ADR-0058): who works with whom. A tab of a pane, one per thread, that takes the whole sheet when it is
// shown. This opens and closes it and keeps one `TeamMap` (controllers/team-map.ts) for each thread whose map was drawn; the map that is
// on screen is `shown`, which is what the window's inspector and actions read.
import { teamApi } from '../api/team.ts';
import type { WorksetScope } from '../api/threads.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';
import { teamViewId } from '../view-ids.ts';
import { TeamMap, type TeamHost } from './team-map.ts';

export class TeamController extends Store {
  private readonly app: Shinatama;
  private readonly maps = new Map<string, TeamMap>();
  /** The thread whose map is on screen, or none. */
  shownThread: string | null = null;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  private host(): TeamHost {
    const { ui, threads } = this.app;
    return {
      title: id => threads.title(id) ?? 'Thread',
      harnessLabel: harness => threads.harnessLabel(harness),
      focusSession: id => { void threads.focusThreadSession(id); },
      say: (text, say) => ui.report(text, say),
      ask: question => ui.ask(question),
    };
  }

  /** The map of a thread, made when it is first asked for. It tells the screens when it changes (its own store is subscribed to). */
  map(threadId: string): TeamMap {
    let map = this.maps.get(threadId);
    if (!map) {
      map = new TeamMap({ id: threadId }, this.host());
      map.subscribe(() => { if (this.shownThread === threadId) this.changed(); });
      this.maps.set(threadId, map);
    }
    return map;
  }

  /** The map on screen: what the window's inspector and actions are about. */
  get shown(): TeamMap | null {
    return this.shownThread ? this.maps.get(this.shownThread) ?? null : null;
  }

  /** Whether the window's inspector has something of the map to show: an agent or an arrow is chosen. */
  get inspecting(): boolean {
    const map = this.shown;
    return !!map && (map.selectedAgent() !== null || map.selectedLink() >= 0);
  }

  /** Whether the window's actions row is the map's (Apply, Discard). */
  get acting(): boolean {
    return !!this.shown && this.shown.enabled;
  }

  /** The screen of a thread's map is drawn (or goes): while it is, its map reads the stored team again every few seconds. */
  show(threadId: string, on: boolean): void {
    const map = this.map(threadId);
    if (on) {
      this.shownThread = threadId;
      map.setVisible(true);
    } else {
      map.setVisible(false);
      if (this.shownThread === threadId) this.shownThread = null;
    }
    this.changed();
  }

  /** Opens the team map of a thread, or brings it forward: a column beside the pane the person works in, then as a tab of the pane that
   *  already holds one. */
  open(thread: Readonly<Pick<WorksetScope, 'id'>>): void {
    const id = teamViewId(thread.id);
    this.app.ui.leaveToWorkspace();
    this.app.layout.open({ id, resource: { kind: 'team', resourceId: id }, hidden: false }, other => other.resource.kind === 'team');
  }

  close(threadId: string): void {
    this.app.layout.close(teamViewId(threadId));
    this.maps.get(threadId)?.dispose();
    this.maps.delete(threadId);
    if (this.shownThread === threadId) this.shownThread = null;
    this.changed();
  }

  templates(): Promise<unknown> {
    return teamApi.templates();
  }

  dispose(): void {
    for (const map of this.maps.values()) map.dispose();
    this.maps.clear();
  }
}
