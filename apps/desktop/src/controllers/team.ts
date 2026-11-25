// The team map of a thread (ADR-0058): who works with whom. A tab of a pane, one per thread, that takes the whole sheet when it is
// shown. The map's own state (the formation being drawn, the templates, the evaluation) is the team screen's; this opens and closes it.
import { teamApi } from '../api/team.ts';
import type { WorksetScope } from '../api/threads.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';
import { teamViewId } from '../view-ids.ts';

export class TeamController extends Store {
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
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
  }

  templates(): Promise<unknown> {
    return teamApi.templates();
  }
}
