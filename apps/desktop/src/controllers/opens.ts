// How a session comes to run: a new session in a project, a shell, a saved session resumed, a hop's destination. Every open (a new
// session, a shell, a resume) takes its turn in one queue, one at a time; nothing is refused or disabled because another open is under
// way. The saved sessions asked to run stay `wanted` until they run, are given up or are cancelled, and a refusal that can pass is
// tried again by itself (session-opens.ts has the queue and the wants as pure functions).
import { errorText, listen, sleep, type Unlisten } from '../api/core.ts';
import { terminalsApi, type HarnessCapabilities, type SessionInfo } from '../api/terminals.ts';
import { threadsApi, type WorksetScope } from '../api/threads.ts';
import { projectsApi } from '../api/projects.ts';
import type { Shinatama } from '../app.ts';
import { folderName, samePath } from '../paths.ts';
import { resumeFailureCode, resumeFailureText, resumeRetryable } from '../resume-failure.ts';
import { createOpenQueue, createWants, type OpenOutcome } from '../session-opens.ts';
import { newSessionProject, type NewSessionScope } from '../session-project.ts';
import { openStep, restoredRunning, withoutInstance } from '../session-tabs.ts';
import { Store } from '../store.ts';
import type { ThreadSummary } from '../thread-sidebar.ts';

/** The shell is a harness of its own: always there, never resumable. */
export const BUILTIN_HARNESS: HarnessCapabilities = { id: 'powershell', label: 'PowerShell', nativeResume: false };

/** What the open under way is waiting for, said until the open ends (a first open after a start waits for the model proxy and the
 *  harness check; it no longer fails while they finish). */
const OPENING_STEPS: Record<string, (agent: string) => string> = {
  proxy: () => 'starting the model proxy', proxy_resources: () => 'waiting for free memory to start the model proxy',
  check: agent => `checking ${agent}`, reading: () => 'reading the saved conversation',
  prepare: () => 'preparing the session', start: agent => `starting ${agent}`,
};

/** Refusals that pass a moment later: right after launch the local service and the resource measurements are still warming up. */
const warmingUp = (failure: unknown) => /still finishing|not yet ready|waiting for healthy resource|Retry when Resources/i.test(
  typeof failure === 'string' ? failure : failure instanceof Error ? failure.message : JSON.stringify(failure ?? ''));

export interface SessionSelection { projectPath: string; harnessId: string; rows: number; cols: number; workset?: WorksetScope | null }

export class OpensController extends Store {
  /** The agents a session can run, and the shell. */
  harnesses: readonly HarnessCapabilities[] = [BUILTIN_HARNESS];
  harnessesLoaded = false;
  readonly queue = createOpenQueue(() => this.changed());
  /** The saved sessions asked to run, and the one loop that opens them. */
  readonly wants = createWants(this.queue, {
    attempt: (sessionId, restored) => this.openSaved(sessionId, restored),
    restoredOrder: () => this.restored().map(tab => tab.threadId),
    // Given up: the reason stays on the card and is said once. In every case the tab no longer says its session ran.
    settled: (sessionId, refused) => { if (refused) { this.openIssues.set(sessionId, refused); this.app.ui.error(refused); } this.settleRestored(sessionId); },
    changed: () => this.changed(),
  });
  /** What the last open of a session that was given up said; on its card until it is asked again. */
  readonly openIssues = new Map<string, string>();
  /** The step the open under way waits for, also said on the card of the session that is opening. */
  openingStep: string | null = null;
  /** The open in progress, said in the footer until it ends. */
  progress: string | null = null;
  private loading: Promise<boolean> | null = null;
  private sequence = 0;
  private asking = false;
  private readonly stops: Unlisten[] = [];

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  async start(): Promise<void> {
    // The steps an open the user waits on passes (proxy, check, prepare).
    this.stops.push(await listen<{ harness: string; step: string }>('agent-opening', payload => {
      if (this.queue.busy() && typeof payload?.harness === 'string' && typeof payload.step === 'string') this.showOpening(payload.harness, payload.step);
    }).catch(() => () => {}));
    // A session opened by something else (a collaboration of an agent): the backend has already moved it into the thread of the one
    // that asked for it (ADR-0055), so the thread comes from the saved record.
    this.stops.push(await listen<{ started: { session: SessionInfo; threadId: string }; harness: string; mode: string; joinedParentThread?: boolean }>(
      'collaboration-session-opened', payload => {
        const started = payload.started;
        if (!started?.session?.id || !started.threadId) return;
        void this.attachSession(started.session, started.threadId);
        for (const project of this.app.threads.activeProjectIds()) void this.app.threads.refreshList(project);
        const agent = ({ codex: 'Codex', opencode: 'OpenCode', pi: 'Pi' } as Record<string, string>)[payload.harness] ?? 'Claude';
        this.app.ui.info(payload.joinedParentThread ? `${agent} session opened in the same thread.` : `${agent} session opened.`);
      }));
  }

  dispose(): void {
    for (const stop of this.stops.splice(0)) stop();
  }

  // --- the agents ---------------------------------------------------------------------------------------

  harnessLabel(id: string | null): string {
    return this.harnesses.find(item => item.id === id)?.label ?? id ?? 'Session';
  }

  canResume(harness: string | null): boolean {
    return this.harnesses.some(item => item.id === harness && item.nativeResume);
  }

  /** Reads which agents there are. The list is checked as it comes: a malformed one is refused whole. */
  loadHarnessCatalog(): Promise<boolean> {
    if (this.harnessesLoaded) return Promise.resolve(true);
    if (this.loading) return this.loading;
    const loading = (async () => {
      try {
        const value = await terminalsApi.harnessCatalog();
        if (!Array.isArray(value) || value.length > 31) throw new Error('Invalid harness catalog');
        const seen = new Set<string>();
        const received: HarnessCapabilities[] = [];
        for (const item of value) {
          if (!item || typeof item !== 'object' || Array.isArray(item)
            || typeof item.id !== 'string' || !/^[a-z0-9_]{1,64}$/.test(item.id) || item.id === BUILTIN_HARNESS.id || seen.has(item.id)
            || typeof item.label !== 'string' || !item.label.length || item.label.length > 128 || /[\u0000-\u001f\u007f-\u009f]/.test(item.label)
            || typeof item.nativeResume !== 'boolean') throw new Error('Invalid harness catalog');
          seen.add(item.id);
          received.push({ id: item.id, label: item.label, nativeResume: item.nativeResume });
        }
        this.harnesses = [...received, BUILTIN_HARNESS];
        this.harnessesLoaded = true;
        this.changed();
        this.app.threads.touch();
        return true;
      } catch {
        this.app.ui.error('Agent choices could not be loaded. Open New session to try again.');
        return false;
      }
    })();
    this.loading = loading;
    return loading.finally(() => { if (this.loading === loading) this.loading = null; });
  }

  // --- progress -----------------------------------------------------------------------------------------

  /** A new or queued open says it waits, so a click is never met with silence. */
  sayQueued(): void {
    if (this.queue.busy()) this.app.ui.info('Another session is opening. This one opens right after.');
  }

  private showOpening(harness: string, step: string): void {
    const text = OPENING_STEPS[step];
    if (!text) return;
    const agent = this.harnessLabel(harness);
    this.openingStep = text(agent);
    this.progress = `Opening ${agent}: ${this.openingStep}…`;
    this.changed();
  }

  /** Ends the progress line of an open; a result notice shown since stays. */
  private endOpening(): void {
    this.progress = null;
    this.openingStep = null;
    this.changed();
  }

  // --- a new session ---------------------------------------------------------------------------------------

  /** Starts one session in a project: a shell, or an agent in a thread of its own or the one asked for. Shared by the harness menu and
   *  the folder dialog. Refusals that pass (the service is warming up) are waited out. */
  async startSession(selection: SessionSelection): Promise<void> {
    const { terminals, threads, layout, projects } = this.app;
    this.sayQueued();
    await this.queue.run(`new:${++this.sequence}`, async () => {
      try {
        await threads.idleNames();
        const selected = await projectsApi.select(selection.projectPath);
        // The session joins the thread it was asked for, else a thread of its project that is on; otherwise a new thread of its own is
        // turned on. An agent is created in that thread, so a thread whose sessions talk (ADR-0057) gives it its room at launch.
        const wanted = selection.workset ?? threads.activeWorkset();
        const target = wanted && wanted.projectId === selected.id ? wanted : threads.openWorksets.find(candidate => candidate.projectId === selected.id) ?? null;
        let started: Awaited<ReturnType<typeof terminalsApi.shellOpen>> | undefined;
        for (let attempt = 0; !started; attempt++) {
          try {
            started = selection.harnessId === 'powershell'
              ? await terminalsApi.shellOpen(selected.path, selection.rows, selection.cols)
              : await terminalsApi.agentOpen(selection.harnessId, selected.path, selection.rows, selection.cols, target?.id ?? null);
          } catch (failure) {
            if (!warmingUp(failure) || attempt >= 30) throw failure;
            await sleep(2000);
          }
        }
        projects.set(selected.path);
        projects.selectedPath = selected.path;
        threads.setCurrentProject({ projectId: selected.id, label: folderName(selected.path) });
        let workset = target;
        if (workset) {
          try { await threadsApi.worksetAdopt(workset.id, started.threadId); } catch { workset = null; }
        }
        if (!workset) {
          workset = await threads.worksetOf(started.threadId);
          if (workset) await threads.showWorkset(workset);
        }
        const group = layout.groups.some(candidate => candidate.id === this.group) ? this.group! : layout.focusedGroup();
        await terminals.attach(started.session, group, started.threadId, workset);
        threads.focusWorkset(workset?.id ?? threads.currentWorkset);
        await threads.refreshMembers();
        await projects.refreshRecent();
        void threads.refreshList(selected.id);
      } finally { this.endOpening(); }
    });
  }

  /** The pane the session being started opens in. */
  private group: string | null = null;

  /** No dialog: the session starts where it was asked for, and a failure is reported as a notice. */
  async launchSession(path: string, harnessId: string, group: string, workset: WorksetScope | null = null): Promise<void> {
    this.group = group;
    try { await this.startSession({ projectPath: path, harnessId, rows: 24, cols: 80, workset }); }
    catch (failure) {
      this.app.ui.error(typeof failure === 'string' ? failure : failure instanceof Error ? failure.message : 'The session could not be opened. Check the project folder and try again.');
    }
  }

  /** New session. A pane's + (`thread`) starts in its thread's project (`workset`, else the focused tab's thread); New session
   *  beside the projects (`project`) starts in the project shown there. With both the folder and the harness already known (a restart)
   *  it starts at once; otherwise the dialog of the new session opens on the folder, to choose the agent (and another folder). */
  async newSession(scope: NewSessionScope = 'project', group: string = this.app.layout.focusedGroup(), path?: string, harness?: string, workset: WorksetScope | null = null): Promise<void> {
    if (this.asking) return;
    this.asking = true;
    const { ui, threads, projects, layout } = this.app;
    try {
      ui.leaveToWorkspace();
      // The thread's folder: from the project list, else from one of its open sessions.
      const thread = scope === 'thread' ? workset ?? threads.groupWorkset(group) ?? threads.activeWorkset() : null;
      const threadFolder = thread ? projects.pathOf(thread.projectId)
        ?? [...this.app.terminals.sessions.values()].find(view => view.workset?.id === thread.id && !view.disposed)?.info.cwd : undefined;
      const selectedPath = newSessionProject(scope, {
        explicit: path, threadOpen: !!thread, openThread: threadFolder,
        sidebar: threads.currentProject ? projects.pathOf(threads.currentProject.projectId) : undefined,
        selected: projects.selectedPath, current: projects.path,
      });
      await this.loadHarnessCatalog();
      if (!selectedPath && scope === 'thread' && thread) { ui.info('The folder of this thread\'s project is not available. Use New session beside the projects.'); return; }
      if (!selectedPath) { ui.info('Choose a project first: sessions start in the selected project.'); ui.open({ kind: 'project-picker' }); return; }
      if (harness && this.harnesses.some(candidate => candidate.id === harness)) { await this.launchSession(selectedPath, harness, group, thread); return; }
      this.group = layout.groups.some(candidate => candidate.id === group) ? group : null;
      ui.open({ kind: 'new-session', scope, groupId: group, path: selectedPath, workset: thread });
    } finally { this.asking = false; }
  }

  /** A session like one that ended: the same agent (or shell) in the same folder, in the same pane and thread. */
  async newSessionLike(session: { info: SessionInfo; workset: WorksetScope | null }, group: string): Promise<void> {
    const harness = session.info.shell === BUILTIN_HARNESS.label ? BUILTIN_HARNESS.id : session.info.shell;
    await this.newSession('project', group, session.info.cwd, harness, session.workset);
  }

  /** The projects a new session can start in, beside the one chosen: the ones the sidebar lists (a pane's + stays in its thread's). */
  newSessionChoices(scope: NewSessionScope, selectedPath: string): { label: string; path: string }[] {
    if (scope !== 'project') return [];
    const choices = this.app.threads.activeProjectIds().map(id => this.app.projects.pathOf(id))
      .filter((path): path is string => !!path).map(path => ({ label: folderName(path), path }));
    if (choices.length && !choices.some(choice => samePath(choice.path, selectedPath))) choices.unshift({ label: folderName(selectedPath), path: selectedPath });
    return choices;
  }

  /** A shell in a folder, in no thread: the browser preview cannot open one. */
  async startLoose(group: string, cwd?: string): Promise<boolean> {
    const { ui, projects, terminals, threads } = this.app;
    const folder = cwd ?? projects.path;
    if (!folder || (cwd === undefined && !projects.available)) { ui.open({ kind: 'project-picker' }); return false; }
    this.sayQueued();
    return this.queue.run(`new:${++this.sequence}`, async () => {
      try {
        await threads.idleNames();
        const info = await terminalsApi.start(folder, 24, 80);
        await terminals.attach(info, group, null, threads.activeWorkset());
        return true;
      } catch (error) { ui.error(error); return false; }
    });
  }

  // --- a session that ran ------------------------------------------------------------------------------------------

  /** A session started outside this window's own flow (a hop, a collaboration): its thread comes from the saved record. */
  async attachSession(info: SessionInfo, threadId: string): Promise<void> {
    const { threads, terminals, layout } = this.app;
    let workset: WorksetScope | null = threads.activeWorkset();
    workset = await threads.worksetOf(threadId) ?? workset;
    await terminals.attach(info, layout.focusedGroup(), threadId, workset);
    if (threads.isOn(workset?.id)) await threads.refreshMembers(); else threads.touch();
  }

  /** The saved sessions the layout holds as running that do not run yet, in the order of the panes and their tabs. */
  private restored() {
    return restoredRunning(this.app.layout.layout, panelId => this.app.terminals.sessions.has(panelId));
  }

  /** Asks for a saved session to run: it opens at its turn, and a refusal that can pass is tried again by itself. `restored`: asked
   *  by the start. What its last try said leaves its card. */
  wantOpen(sessionId: string, restored = false): void {
    this.openIssues.delete(sessionId);
    this.wants.ask(sessionId, restored);
  }

  /** Leaves a waiting session not running. */
  cancelWant(sessionId: string): void {
    this.wants.cancel(sessionId);
  }

  /** A closed session that comes back and can be resumed is asked to run at once. */
  resumeIfSaved(threadId: string): void {
    const back = this.app.threads.members.find(member => member.id === threadId);
    if (back && openStep(back, harness => this.canResume(harness)) === 'resume') this.wantOpen(threadId);
  }

  /** The layout and its sessions are restored together (ADR-0005 v4): once the saved layout is back, the sessions it holds as
   *  running are asked to open, in the background (each keeps its tab where it was; the tab each pane shows and the focus stay). A
   *  refusal that can pass a moment later is tried again by itself: at a start the model proxy is not ready for up to a minute. */
  reopenRestored(): void {
    for (const tab of this.restored()) this.wantOpen(tab.threadId, true);
  }

  /** The tab of this session no longer says it ran: it is not reopened by itself again, in this run or at a later start. */
  private settleRestored(sessionId: string): void {
    const { layout } = this.app;
    for (const tab of this.restored()) {
      if (tab.threadId === sessionId) layout.replace(withoutInstance(layout.layout, tab.panelId));
    }
  }

  /** One try to make a saved session run, inside its turn of the open queue. One still marked open by a Shinatama that is gone is
   *  recovered first (`thread_recover` verifies that owner is gone and refuses otherwise), then the session is resumed through its
   *  agent's native resume. `background`: asked by the start, it leaves the tab each pane shows and the focus as they are. */
  private async openSaved(sessionId: string, background: boolean): Promise<OpenOutcome> {
    const { terminals, threads, layout } = this.app;
    if (terminals.runningOf(sessionId)) return { opened: true };
    if (!this.harnessesLoaded && !await this.loadHarnessCatalog()) return { refused: 'The agents of this Shinatama are not read yet.', retry: true };
    try {
      for (let recovered = false; ;) {
        const row = threads.members.find(member => member.id === sessionId);
        const workset = threads.memberOf.get(sessionId);
        // Its thread was turned off, or the session was closed or deleted meanwhile.
        if (!row || !workset) return { nothing: true };
        const step = openStep(row, harness => this.canResume(harness));
        if (typeof step === 'object') return { nothing: true };
        if (step === 'recover') {
          if (recovered) return { refused: 'This session is still marked open after its recovery.', retry: true };
          const refused = await this.recoverRecord(row);
          if (refused) return refused;
          recovered = true;
          await threads.refreshMembers();
          continue;
        }
        await threads.idleNames();
        try {
          const started = await terminalsApi.threadResume({ threadId: row.id, projectId: row.projectId, expectedRevision: row.revision, rows: 24, cols: 80 });
          await terminals.attach(started.session, layout.focusedGroup(), started.threadId, workset, background);
          return { opened: true };
        } catch (error) { return { refused: resumeFailureText(error), retry: resumeRetryable(error) }; }
      }
    } finally { this.endOpening(); await threads.refreshMembers(); }
  }

  /** Closes the record a Shinatama that is gone left open (`thread_recover` verifies that owner is gone and refuses otherwise; it
   *  starts nothing). Null when done; else what to say, and whether a later try can pass: not while the session or the Shinatama that
   *  opened it is really alive. */
  private async recoverRecord(row: ThreadSummary): Promise<{ refused: string; retry: boolean } | null> {
    try { await threadsApi.threadRecover({ threadId: row.id, projectId: row.projectId, expectedRevision: row.revision }); return null; }
    catch (error) {
      const alive = ['session_alive', 'owner_alive'].includes(resumeFailureCode(error) ?? '');
      return { refused: `The previous run of this session could not be verified as ended${alive ? ': it is still running, here or in another Shinatama window' : ''}. Its saved conversation is kept.`, retry: !alive };
    }
  }

  /** The card's Recover session: the record left open is closed and nothing is started; a refusal is said in a notice. */
  async recoverSaved(saved: ThreadSummary): Promise<void> {
    const refused = await this.recoverRecord(saved);
    if (refused) this.app.ui.error(refused.refused);
    await this.app.threads.refreshMembers();
  }

  /** The overview of the projects resumes a thread's last session: the person is told why when it cannot. */
  async resumeThread(project: { id: string }, thread: { id: string; revision: number }): Promise<void> {
    this.sayQueued();
    await this.queue.run(`overview:${thread.id}`, async () => {
      try {
        await this.app.threads.idleNames();
        const started = await terminalsApi.threadResume({ threadId: thread.id, projectId: project.id, expectedRevision: thread.revision, rows: 24, cols: 80 })
          .catch(error => { throw new Error(resumeFailureText(error)); });
        await this.attachSession(started.session, started.threadId);
      } catch (error) { this.app.ui.error(errorText(error)); }
      finally { this.endOpening(); }
    });
  }
}
