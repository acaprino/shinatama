// What agent sessions left on disk in their projects: worktrees, copies, bench and backup folders and session folders, as Xiox watches
// them (`project_leftovers`). Read-only: it copies paths and commands and selects a folder in Explorer; it never removes anything. The
// picture is read when the window starts, when Xiox says it changed (`project-leftovers`) and every 30 s while the window is shown,
// so that "5 min ago" moves on and an observer that started meanwhile shows up.
import { errorText, listen, type Unlisten } from '../api/core.ts';
import { projectsApi } from '../api/projects.ts';
import type { Shinatama } from '../app.ts';
import { visibleCount, type LeftoverStatus } from '../leftovers-model.ts';
import { Store } from '../store.ts';

const REFRESH_MS = 30_000;

export class LeftoversController extends Store {
  status: LeftoverStatus = { available: false, reason: null, projects: [] };
  /** The first read has finished (the panel's loading state ends). */
  loaded = false;
  /** Why the last read failed, when it did. */
  readFailure: string | null = null;
  /** What the last action said (a copy, a check), and whether it went wrong. */
  note: { text: string; error: boolean } | null = null;

  private stop: Unlisten | null = null;
  private timer: ReturnType<typeof setInterval> | undefined;
  private reading = false;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  async start(): Promise<void> {
    void this.refresh();
    this.timer = setInterval(() => { if (typeof document === 'undefined' || !document.hidden) void this.refresh(); }, REFRESH_MS);
    try { this.stop = await listen('project-leftovers', () => { void this.refresh(); }); } catch { /* the timer keeps it fresh */ }
  }

  dispose(): void {
    clearInterval(this.timer);
    this.stop?.();
    this.stop = null;
  }

  /** The new items across every observed project: what the toolbar counts. */
  get count(): number {
    return visibleCount(this.status.projects);
  }

  /** Whether some project has something that asks for attention. */
  get attention(): boolean {
    return this.status.projects.some(project => project.attention.length > 0);
  }

  open(): void {
    this.app.ui.openPanel({ kind: 'leftovers' });
  }

  async refresh(): Promise<void> {
    if (this.reading) return;
    this.reading = true;
    try { this.status = await projectsApi.leftovers() as LeftoverStatus; this.readFailure = null; }
    catch (error) { this.readFailure = errorText(error); }
    this.loaded = true; this.reading = false;
    this.changed();
  }

  say(text: string, error = false): void {
    this.note = { text, error };
    this.changed();
  }

  /** Asks Xiox to look again at a project now (or to start, when it stopped). */
  async check(folder: string, name: string): Promise<void> {
    try { await projectsApi.leftoversCheck(folder); this.say(`Xiox is checking ${name}…`); }
    catch (error) { this.say(errorText(error), true); }
  }

  /** Selects a folder in Explorer. Nothing is opened or removed. */
  async reveal(path: string): Promise<void> {
    try { await projectsApi.leftoversReveal(path); }
    catch (error) { this.say(errorText(error), true); }
  }

  /** Takes the person to the pane of an agent terminal that was open when an item appeared. */
  goTo(terminalId: string): void {
    this.app.ui.leaveToWorkspace();
    void this.app.threads.focusTab(terminalId);
  }
}
