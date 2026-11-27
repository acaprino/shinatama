// The projects the window knows: the one new sessions start in, and the recent ones (the sidebar lists their threads).
import { errorText } from '../api/core.ts';
import { projectsApi, type ProjectEntry } from '../api/projects.ts';
import { folderName, samePath } from '../paths.ts';
import { Store } from '../store.ts';
import type { Shinatama } from '../app.ts';

export class ProjectsController extends Store {
  /** The folder new sessions start in: the project chosen last. Empty before one is chosen. */
  path = '';
  /** The project chosen by the person (not the one restored at the start). */
  selectedPath = '';
  /** Whether the folder of `path` is still there: a session cannot start in one that is not. */
  available = true;
  /** The recent projects, last used first. */
  recent: ProjectEntry[] = [];
  private restoring = 0;

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  /** The path of a project by its id (a thread knows its project by id). */
  pathOf(projectId: string): string | undefined {
    return this.recent.find(project => project.id === projectId)?.path;
  }

  /** The project folder of `path`, as the sidebar labels it. */
  label(path: string): string {
    return folderName(path);
  }

  /** Makes `path` the folder new sessions start in. Moving to another project leaves the one the thread sidebar had chosen. */
  set(path: string): void {
    const next = path.trim();
    if (next !== this.path) this.app.threads.setCurrentProject(null);
    this.path = next;
    this.changed();
  }

  /** A project that Rust already chose (the picker's, a new one's): it becomes the folder new sessions start in, as the last used. */
  adopt(project: ProjectEntry): void {
    this.selectedPath = project.path;
    this.available = project.available;
    this.set(project.path);
    this.recent = [project, ...this.recent.filter(entry => entry.id !== project.id)].slice(0, 10);
    void this.refreshRecent();
  }

  /** The project the window was started in, then the last one used: where new sessions start until the person chooses. */
  async restore(): Promise<void> {
    const revision = ++this.restoring;
    try {
      this.set(await projectsApi.path());
      const recent = await projectsApi.recent(this.app.threads.activeProjectIds());
      if (revision !== this.restoring) return;
      this.recent = recent;
      const latest = recent[0];
      this.available = !latest || latest.available;
      if (latest) {
        this.set(latest.path);
        if (!latest.available) this.app.ui.error(`Last project unavailable: ${latest.path}. Choose a folder before opening a new terminal.`);
      }
    } catch (failure) {
      if (revision !== this.restoring) return;
      this.available = false;
      this.app.ui.error(`Project history unavailable: ${errorText(failure)}`);
    }
    this.changed();
  }

  /** Reads the recent projects again and tells the thread sidebar which one is the current. */
  async refreshRecent(): Promise<void> {
    try {
      const recent = await projectsApi.recent(this.app.threads.activeProjectIds());
      this.recent = recent;
      const current = recent.find(project => samePath(project.path, this.path));
      this.app.threads.setProjects(
        recent.map(project => ({ projectId: project.id, label: folderName(project.path) })),
        current ? { projectId: current.id, label: folderName(current.path) } : undefined,
      );
      this.changed();
    } catch {
      this.app.ui.error('Saved project threads are unavailable.');
    }
  }

  /** The person chose a folder: it becomes the project (saved as the last used) and the sessions start in it. */
  async choose(path: string): Promise<ProjectEntry> {
    const selected = await projectsApi.select(path);
    this.selectedPath = selected.path;
    this.available = selected.available;
    const changed = selected.path !== this.path;
    this.set(selected.path);
    if (changed) await this.refreshRecent();
    return selected;
  }
}
