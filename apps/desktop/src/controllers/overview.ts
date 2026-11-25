// The projects of the window as a list, with what each one holds: its Git state, its last thread, new sessions (also in a worktree) and what
// agents left on disk (`app.leftovers`). It is read when the dialog opens. A worktree is a Git change on disk, so it is planned first and
// the person confirms the exact path and branch before it is made.
import { errorText } from '../api/core.ts';
import { projectsApi } from '../api/projects.ts';
import type { Shinatama } from '../app.ts';
import { ordered, parsePlan, parseRows, worktreeNameProblem, type LastThread, type ProjectRow } from '../projects-overview-model.ts';
import { Store } from '../store.ts';

export class OverviewController extends Store {
  rows: ProjectRow[] | null = null;
  loading = false;
  /** Why the read failed, and what the last action said. */
  failure = '';
  note: { text: string; error: boolean } | null = null;
  private generation = 0;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  /** The dialog is on screen: the projects and their Git status are read again. */
  async load(): Promise<void> {
    const mine = ++this.generation;
    this.loading = true;
    this.failure = '';
    this.changed();
    try {
      const rows = parseRows(await projectsApi.overview());
      if (mine !== this.generation) return;
      if (!rows) throw new Error('The projects could not be read.');
      this.rows = ordered(rows);
      void this.app.leftovers.refresh();
    } catch (error) { if (mine === this.generation) this.failure = errorText(error); }
    finally { if (mine === this.generation) { this.loading = false; this.changed(); } }
  }

  close(): void {
    this.generation += 1;
  }

  say(text: string, error = false): void {
    this.note = { text, error };
    this.changed();
  }

  async pin(row: ProjectRow): Promise<void> {
    try { await projectsApi.pin(row.id, !row.pinned); await this.load(); }
    catch (error) { this.say(errorText(error), true); }
  }

  newSession(row: ProjectRow): void {
    this.app.ui.closeDialog();
    void this.app.opens.newSession('project', undefined, row.path);
  }

  async resume(row: ProjectRow, thread: LastThread): Promise<void> {
    this.app.ui.closeDialog();
    await this.app.opens.resumeThread({ id: row.id }, { id: thread.id, revision: thread.revision });
  }

  /** Plans a worktree, asks with the exact path and branch, makes it and opens a session there. */
  async worktree(row: ProjectRow, name: string): Promise<void> {
    const problem = worktreeNameProblem(name);
    if (problem) { this.say(problem, true); return; }
    try {
      const plan = parsePlan(await projectsApi.worktreePlan(row.path, name));
      if (!plan) throw new Error('The worktree could not be planned.');
      if (!await this.app.ui.ask({
        title: 'Create a worktree?', description: `Git will create ${plan.path} on a new branch ${plan.branch}, then a new session opens there. Nothing existing is changed or deleted.`, confirm: 'Create the worktree', danger: false,
      })) return;
      const path = await projectsApi.worktreeCreate(row.path, name, plan.path);
      void this.app.opens.newSession('project', undefined, path);
    } catch (error) { this.say(errorText(error), true); }
  }
}
