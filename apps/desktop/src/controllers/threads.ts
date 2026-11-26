// Threads (ADR-0055): a thread is a set of sessions with its own place in the window. A thread that is ON has its sessions in the
// window's one layout, beside the others, with a colour; a thread that is OFF keeps its sessions saved, not running. This controller
// holds which threads are on, the sessions of those threads (the members), the names of the tabs, and what the thread sidebar lists
// (the projects, their threads). Turning a thread on starts nothing; turning it off closes its tabs and stops its sessions, after asking.
import { errorText, sleep } from '../api/core.ts';
import { projectsApi } from '../api/projects.ts';
import { terminalsApi } from '../api/terminals.ts';
import { threadsApi, type WorksetScope, type WorksetSummary } from '../api/threads.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';
import { folderName, samePath } from '../paths.ts';
import { forgetRefusal, rowsToRecover, sessionTabLabel, sessionTabName, threadSessions, threadsOff, type ThreadSession } from '../session-tabs.ts';
import { openCard } from '../session-opens.ts';
import { threadFailureText, threadTitle, type CurrentThreadProject, type ThreadSidebarState, type ThreadSummary } from '../thread-sidebar.ts';
import { createToneKeeper } from '../thread-tone.ts';
import { instanceRef } from './terminals.ts';
import { teamViewId, threadOfChatView, threadOfTeamView } from '../view-ids.ts';
import { reduceWorkspaceLayout, tabGroups, type LayoutAction, type PanelView, type WorkspaceLayout } from '../workspace-model.ts';

const THREADS_KEY = 'shinatama.threads';
const SESSIONS_KEY = 'shinatama.sessions';

/** The threads to turn on at this start, read before anything can overwrite them: reattaching a running terminal runs
 *  `rememberSessions` while none is on yet, which is why it leaves the saved list alone until `reopenLast` has read it. */
function threadsToReopen(): unknown {
  try {
    return JSON.parse(localStorage.getItem(THREADS_KEY) ?? `[${localStorage.getItem('shinatama.thread') ?? ''}]`);
  } catch { return []; }
}

export class ThreadsController extends Store {
  /** The threads that are on, in the order they were turned on: the window shows their sessions together, in one layout, with the
   *  terminals that belong to no thread. A thread turned off keeps its sessions running only until it is closed (after asking). */
  openWorksets: readonly WorksetScope[] = [];
  /** The thread worked in last: the one last turned on, or the thread of the last tab used. */
  currentWorkset: string | null = null;
  /** The sessions of the threads that are on, as last read (closed ones included, oldest first), and the thread each belongs to. */
  members: readonly ThreadSummary[] = [];
  readonly memberOf = new Map<string, WorksetScope>();

  // What the thread sidebar lists.
  /** The projects of the sidebar (a project is listed when it was added to it), and the one new threads go to. */
  projects: readonly CurrentThreadProject[] = [];
  currentProject: CurrentThreadProject | null = null;
  /** The state the sidebar keeps between runs: which projects are listed, and whether it is hidden. */
  sidebarState: ThreadSidebarState | null = null;
  /** The threads of each project that is listed, as the store gave them. */
  readonly lists = new Map<string, readonly WorksetSummary[]>();
  /** The name an agent gave a session, by session. */
  readonly names = new Map<string, string>();
  /** The sidebar lists the archived threads, not the others. */
  showArchived = false;
  /** An operation on a thread or a session is under way: the sidebar's controls wait for it. */
  busy = false;
  /** A press of "Choose projects" is being saved: the rows wait for it, since two at once would save the sidebar from the same old state. */
  toggling = false;
  private readonly namedAt = new Map<string, number>();

  private readonly tones = createToneKeeper();
  /** The threads that were on are not yet read (and so not yet turned on again): what is saved of them is not overwritten. */
  private restoring = true;
  private switching = false;
  private sidebarRevision = 0;
  private namesInFlight: Promise<unknown> = Promise.resolve();

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  // --- which threads are on --------------------------------------------------------------------------

  /** Whether a terminal of this thread is drawn: every terminal of no thread is. */
  isOn(worksetId: string | null | undefined): boolean {
    return worksetId == null || this.openWorksets.some(workset => workset.id === worksetId);
  }

  /** The colour of a thread that is on (a colour of Sigrid's palette), null while it is off. */
  toneOf(worksetId: string): string | null {
    return this.tones.tone(this.openWorksets.map(workset => workset.id), worksetId);
  }

  /** The thread a tab belongs to, when that thread is on. */
  tabWorkset(viewId: string | null | undefined): WorksetScope | null {
    if (!viewId) return null;
    const running = this.app.terminals.get(viewId);
    if (running?.workset && this.isOn(running.workset.id)) return running.workset;
    const panel = this.app.layout.view(viewId);
    return panel?.resource.kind === 'terminal' && panel.resource.threadId ? this.memberOf.get(panel.resource.threadId) ?? null : null;
  }

  groupWorkset(groupId: string): WorksetScope | null {
    return this.tabWorkset(this.app.layout.groups.find(group => group.id === groupId)?.activeTabId);
  }

  /** The thread New session and the sidebar's actions mean: the current one, else the focused tab's, else the last turned on. */
  activeWorkset(): WorksetScope | null {
    return this.openWorksets.find(workset => workset.id === this.currentWorkset)
      ?? this.groupWorkset(this.app.layout.focusedGroup()) ?? this.openWorksets.at(-1) ?? null;
  }

  /** The title of a thread, as the sidebar lists it: its own, else its first session's, else the one its agent gave it; null when it has none. */
  title(worksetId: string): string | null {
    const row = this.summary(worksetId);
    return row ? threadTitle(row, row.lead ? this.names.get(row.lead.threadId) : undefined, id => this.harnessLabel(id)) : null;
  }

  /** A thread's record, as the sidebar lists it. */
  summary(worksetId: string): WorksetSummary | undefined {
    for (const rows of this.lists.values()) {
      const row = rows.find(candidate => candidate.id === worksetId);
      if (row) return row;
    }
    return undefined;
  }

  /** Remembers the thread the sidebar's actions are for. */
  focusWorkset(worksetId: string | null): void {
    if (this.currentWorkset === worksetId) return;
    this.currentWorkset = worksetId;
    this.changed();
  }

  // --- the sessions of the threads that are on ---------------------------------------------------------------

  /** The thread a session belongs to. The store queues requests (ADR-0055 v3), so a refusal is final; a session saved a moment
   *  ago may not be listed yet, so "none" is asked again briefly. */
  async worksetOf(threadId: string): Promise<WorksetScope | null> {
    for (let attempt = 0; attempt < 5; attempt++) {
      try { const found = await threadsApi.worksetOf(threadId); if (found) return found; } catch { return null; }
      await sleep(300);
    }
    return null;
  }

  /** The label of an agent. */
  harnessLabel(id: string | null): string {
    return this.app.opens.harnessLabel(id);
  }

  /** The name the user gave the session, or the one its agent gave it, or its agent. */
  sessionName(row: ThreadSummary): string {
    return row.title && row.title !== this.harnessLabel(row.harness) ? row.title : this.harnessLabel(row.harness);
  }

  /** The session's name among all the sessions of its thread (closed ones included, oldest first), numbered when several share it: the
   *  same on its running tab, on its "not running" tab and in the thread's panel, and the same after a restart. */
  memberName(row: ThreadSummary): string {
    const owner = this.memberOf.get(row.id)?.id;
    const siblings = this.members.filter(other => this.memberOf.get(other.id)?.id === owner).map(other => ({ id: other.id, name: this.sessionName(other) }));
    return sessionTabName(siblings, row.id) ?? this.sessionName(row);
  }

  /** The session a tab without a running terminal stands for. */
  savedSession(viewId: string): ThreadSummary | null {
    if (this.app.terminals.get(viewId)) return null;
    const panel = this.app.layout.view(viewId);
    const thread = panel?.resource.kind === 'terminal' ? panel.resource.threadId : null;
    return thread ? this.members.find(row => row.id === thread) ?? null : null;
  }

  /** The project folder a terminal tab belongs to, whether it runs or not: a "not running" tab is its saved session's project. */
  panelFolder(viewId: string): string | null {
    const running = this.app.terminals.get(viewId);
    if (running) return running.info.cwd;
    const saved = this.savedSession(viewId);
    return saved ? this.app.projects.pathOf(saved.projectId) ?? null : null;
  }

  /** "Claude Code 2 · Fix browser tabs": the session first, then its thread (or its project's folder when the thread has no name). A
   *  tab is cut at its end, so what tells two sessions of one thread apart comes first. A terminal of no thread, or one whose session
   *  is not read yet, is named by its folder and its agent, numbered when the same agent runs twice there. */
  tabLabel(viewId: string): string {
    const { terminals, opens, chat } = this.app;
    const view = terminals.get(viewId);
    if (!view) {
      const saved = this.savedSession(viewId);
      if (!saved) return 'Saved terminal · unavailable';
      const owner = this.memberOf.get(saved.id);
      const folder = this.app.projects.pathOf(saved.projectId);
      const plan = openCard(saved.id, { current: opens.queue.current(), wants: opens.wants.all(), now: Date.now() });
      return sessionTabLabel({
        name: this.memberName(saved), where: (owner && this.title(owner.id)) || (folder ? folderName(folder) : null),
        state: plan === 'offer' ? 'saved' : plan === 'opening' ? 'opening' : 'waiting',
      });
    }
    const where = (view.workset && this.title(view.workset.id)) || folderName(view.info.cwd);
    const row = view.threadId ? this.members.find(member => member.id === view.threadId) : undefined;
    const mark = chat.mark(view);
    if (row) return sessionTabLabel({ name: this.memberName(row), where, state: view.ended ? 'exited' : 'running', mark });
    const harness = this.harnessLabel(view.info.shell);
    const twins = [...terminals.sessions.values()].filter(other => other.info.shell === view.info.shell && (view.workset
      ? other.workset?.id === view.workset.id : !other.workset && samePath(other.info.cwd, view.info.cwd)));
    return `${where} · ${harness}${twins.length > 1 ? ` ${twins.indexOf(view) + 1}` : ''}${view.ended ? ' · exited' : mark}`;
  }

  /** What a tab is called, whatever it holds: a session by `tabLabel`, a thread's conversation and team map by their thread, a page by its
   *  title. */
  tabTitle(view: PanelView): string {
    const kind = view.resource.kind;
    if (kind === 'terminal') return this.tabLabel(view.id);
    const thread = (id: string | null) => (id && this.title(id)) || 'Thread';
    if (kind === 'chat') return `Conversation · ${thread(threadOfChatView(view.id))}`;
    if (kind === 'team') return `Team map · ${thread(threadOfTeamView(view.id))}`;
    if (kind === 'browser') return this.app.browser.label(view.id);
    if (kind === 'files') return this.app.fileTabs.label(view.id);
    return kind.charAt(0).toUpperCase() + kind.slice(1);
  }

  /** The close of a tab (its ×, Delete, a middle press, the menu). A running session stops after asking and moves to Closed sessions, where
   *  it can come back; a tab whose session does not run moves there too and asks nothing (Resume brings it back); a conversation, a team map
   *  or a page just goes. Nothing is ever deleted here. */
  closeTab(viewId: string): void {
    const { layout, terminals, opens, team, browser } = this.app;
    const kind = layout.view(viewId)?.resource.kind;
    if (kind === 'terminal') {
      if (terminals.get(viewId)) { void terminals.stop(viewId); return; }
      const saved = this.savedSession(viewId);
      if (!saved) { layout.close(viewId); return; }
      opens.cancelWant(saved.id);
      void this.closeSession(saved.id).then(() => this.sayClosed(saved.id));
    } else if (kind === 'team') team.close(threadOfTeamView(viewId) ?? '');
    else if (kind === 'browser') void browser.close(viewId);
    else if (kind === 'files') void this.app.fileTabs.close(viewId);
    else layout.close(viewId);
  }

  /** The colour of the thread a tab belongs to (its conversation and its team map included); none for terminals of no thread and for the
   *  other kinds of tab. */
  tabTone(viewId: string): string | null {
    const own = threadOfChatView(viewId) ?? threadOfTeamView(viewId);
    if (own) return this.toneOf(own);
    const running = this.app.terminals.get(viewId);
    if (running) return running.workset ? this.toneOf(running.workset.id) : null;
    const saved = this.savedSession(viewId);
    const owner = saved ? this.memberOf.get(saved.id) : undefined;
    return owner ? this.toneOf(owner.id) : null;
  }

  /** The sessions of one thread as the thread's panel lists them (a read, not kept). */
  async sessionsOf(worksetId: string): Promise<ThreadSession[]> {
    const rows = await threadsApi.worksetThreads(worksetId);
    return threadSessions(rows, row => this.sessionName(row), new Set(this.runningThreadIds()),
      row => row.harness !== null && row.nativeAvailable === true && this.app.opens.canResume(row.harness));
  }

  private runningThreadIds(): string[] {
    return [...this.app.terminals.sessions.values()].filter(view => view.threadId && !view.ended && !view.disposed).map(view => view.threadId!);
  }

  /** Reads the sessions of the threads that are on again and brings the layout in line. */
  async refreshMembers(): Promise<void> {
    const open = this.openWorksets;
    try {
      // One read at a time: the store refuses a second one while busy.
      const lists: ThreadSummary[][] = [];
      for (const workset of open) lists.push(await threadsApi.worksetThreads(workset.id));
      if (this.openWorksets !== open) return;
      this.members = lists.flat();
      this.memberOf.clear();
      lists.forEach((rows, index) => { for (const row of rows) this.memberOf.set(row.id, open[index]); });
      this.app.layout.replace(this.reconciled(this.app.layout.layout));
    } catch { /* the last read stays on screen */ }
    this.changed();
  }

  /** Makes a layout hold all and only the sessions of the threads that are on (ADR-0055 v2). Starts nothing. */
  reconciled(layout: WorkspaceLayout): WorkspaceLayout {
    let next = layout;
    const apply = (action: LayoutAction) => { const result = reduceWorkspaceLayout(next, action); if (result.ok) next = result.layout; };
    const members = new Map(this.members.filter(row => row.status !== 'archived').map(row => [row.id, row]));
    const mine = [...this.app.terminals.sessions.values()].filter(view => !view.disposed && this.isOn(view.workset?.id));
    const running = new Set(mine.map(view => view.threadId).filter((id): id is string => id !== null));
    const shown = new Set<string>();
    for (const panel of layout.views) {
      if (panel.resource.kind !== 'terminal') continue;
      const view = this.app.terminals.get(panel.id);
      const thread = panel.resource.threadId;
      const keep = view ? mine.includes(view) : thread !== null && members.has(thread) && !running.has(thread) && !shown.has(thread);
      if (!keep) { apply({ type: 'remove-view', viewId: panel.id }); continue; }
      if (thread) shown.add(thread);
    }
    const group = () => tabGroups(next.root).find(candidate => candidate.id === next.focusedGroupId)?.id ?? tabGroups(next.root)[0].id;
    const saved = (row: ThreadSummary): PanelView => ({ id: `saved-${row.id}`, resource: { kind: 'terminal', instanceId: null, threadId: row.id, profileId: null }, hidden: false });
    for (const row of members.values()) if (!running.has(row.id) && !shown.has(row.id)) apply({ type: 'add-view', groupId: group(), view: saved(row) });
    for (const view of mine) {
      if (!next.views.some(panel => panel.id === view.info.id)) {
        apply({ type: 'add-view', groupId: group(), view: { id: view.info.id, resource: { kind: 'terminal', instanceId: instanceRef(view.info.id), threadId: view.threadId, profileId: null }, hidden: false } });
      }
    }
    return next;
  }

  /** Which thread each running terminal belongs to, kept for a frontend reload; and the threads to turn on at the next start. Which
   *  sessions were running is the layout's to say (a tab saved with its instance), so the two are restored together. */
  rememberSessions(): void {
    try {
      sessionStorage.setItem(SESSIONS_KEY, JSON.stringify(Object.fromEntries([...this.app.terminals.sessions.values()].map(view => [view.info.id, { threadId: view.threadId, workset: view.workset }]))));
      if (!this.restoring) {
        localStorage.setItem(THREADS_KEY, JSON.stringify(this.openWorksets));
        localStorage.removeItem('shinatama.thread');
      }
    } catch { /* remembering is a convenience */ }
  }

  /** The terminals that were running before the page was loaded again, by id, with their thread. */
  knownSessions(): Record<string, { threadId: string | null; workset: WorksetScope | null }> {
    try { return JSON.parse(sessionStorage.getItem(SESSIONS_KEY) ?? '{}'); } catch { return {}; }
  }

  /** Turns on the threads that were on at the last close, before the saved layout loads, so their tabs keep their panes. One that no
   *  longer exists is left out. */
  async reopenLast(): Promise<void> {
    const last = threadsToReopen();
    this.restoring = false;
    for (const item of Array.isArray(last) ? last.slice(0, 32) : []) {
      if (item && /^w_[a-z2-7]{12}$/.test(item.id) && /^p_[a-z2-7]{12}$/.test(item.projectId)) await this.showWorkset({ id: item.id, projectId: item.projectId });
    }
  }

  // --- turning threads on and off ------------------------------------------------------------------------------

  /** Turns a thread on: its sessions join the window's layout beside the others. It starts and stops nothing itself; a tab the saved
   *  layout restores as running reopens its session when shown. */
  async showWorkset(target: WorksetScope): Promise<boolean> {
    // Another thread is being turned on or off: this one waits its turn instead of failing.
    for (const deadline = Date.now() + 10000; this.switching && Date.now() < deadline;) await sleep(50);
    if (this.openWorksets.some(workset => workset.id === target.id)) return true;
    if (this.switching) return false;
    this.switching = true;
    try {
      try { await threadsApi.worksetThreads(target.id); }
      catch { this.app.ui.error('That thread is unavailable. The window stays as it is.'); return false; }
      this.openWorksets = [...this.openWorksets, target];
      await this.refreshMembers();
      this.rememberSessions();
      this.app.layout.focus(this.app.layout.focusedGroup());
      this.currentWorkset = target.id;
      this.changed();
      return true;
    } finally { this.switching = false; }
  }

  /** Turns threads off, which closes them without deleting anything (2026-11-18, the owner; ADR-0055 v4): their tabs leave the window,
   *  all at once, and their running sessions stop, after asking. The sessions stay in the thread, not running, and come back as such
   *  (with Resume) when it is turned on again; at a later start nothing of them reopens by itself. False when a stop failed; a cancel
   *  leaves the threads on and returns true, as nothing went wrong (the caller reads `isOn` when it needs to know). */
  async hideWorksets(ids: readonly string[]): Promise<boolean> {
    if (!this.openWorksets.some(workset => ids.includes(workset.id))) return true;
    const { terminals, layout } = this.app;
    const off = threadsOff([...terminals.sessions.values()], ids);
    if (off.ask) {
      const count = off.stop.length;
      const turning = ids.filter(id => this.isOn(id)).length;
      const several = turning > 1;
      if (!await this.app.ui.ask({
        title: several ? 'Turn the threads off?' : 'Turn the thread off?',
        description: `This closes ${several ? 'them' : 'it'}: ${count} running session${count === 1 ? '' : 's'} and the programs ${count === 1 ? 'it' : 'they'} started stop. `
          + 'The sessions stay in the thread, not running, and can be resumed when it is turned on again. Nothing is deleted.',
        confirm: several ? `Turn ${turning} threads off` : 'Turn the thread off',
      })) return true;
    }
    for (const view of off.stop) {
      try { await terminalsApi.stop(view.info.id); }
      catch (error) { if (!await terminals.endedElsewhere(view.info.id)) { this.app.ui.error(error); return false; } }
    }
    for (const view of off.leave) terminals.discard(view.info.id);
    this.openWorksets = this.openWorksets.filter(workset => !ids.includes(workset.id));
    if (this.currentWorkset && ids.includes(this.currentWorkset)) this.currentWorkset = null;
    for (const id of ids) layout.close(teamViewId(id));
    await this.refreshMembers();
    this.rememberSessions();
    this.changed();
    return true;
  }

  hideWorkset(id: string): Promise<boolean> {
    return this.hideWorksets([id]);
  }

  /** Forgets every session of a thread, then the thread. Refused (with `{ code: 'owned_elsewhere' }`) while one of its sessions runs.
   *  A session still marked open by a Shinatama that is gone (a crash) is recovered first, which verifies that owner is gone: the
   *  forget refuses an open one. Everything is checked before the thread is turned off, so a refused forget leaves it as it was. */
  async forgetWorkset(thread: Readonly<WorksetSummary>): Promise<void> {
    const { terminals } = this.app;
    const mine = [...terminals.sessions.values()].filter(view => view.workset?.id === thread.id && !view.disposed);
    if (mine.some(view => !view.ended)) throw { code: 'owned_elsewhere' };
    let rows = await threadsApi.worksetThreads(thread.id);
    const live = new Set(this.runningThreadIds());
    if (rows.some(row => live.has(row.id))) throw { code: 'owned_elsewhere' };
    const stale = rowsToRecover(rows, live);
    for (const row of stale) {
      try { await threadsApi.threadRecover({ threadId: row.id, projectId: row.projectId, expectedRevision: row.revision }); }
      catch (error) { throw { code: forgetRefusal(error) }; }
    }
    // Recovery raises the revisions the forget checks.
    if (stale.length) rows = await threadsApi.worksetThreads(thread.id);
    await this.hideWorkset(thread.id);
    for (const view of mine) terminals.discard(view.info.id);
    for (const row of rows) await threadsApi.threadForget({ projectId: row.projectId, threadId: row.id, expectedRevision: row.revision });
    await threadsApi.worksetDelete({ worksetId: thread.id, expectedRevision: thread.revision });
    await this.refreshList(thread.projectId);
  }

  // --- one session ---------------------------------------------------------------------------------------------

  /** A saved session and its thread, also when that thread is off. */
  async sessionRow(threadId: string): Promise<{ row: ThreadSummary; workset: WorksetScope } | null> {
    const known = this.members.find(member => member.id === threadId);
    const knownSet = this.memberOf.get(threadId);
    if (known && knownSet) return { row: known, workset: knownSet };
    const workset = await this.worksetOf(threadId);
    if (!workset) return null;
    const row = (await threadsApi.worksetThreads(workset.id)).find(member => member.id === threadId);
    return row ? { row, workset } : null;
  }

  /** Closing keeps the session: it leaves the tabs and waits under Closed sessions of its thread. The record of a just-stopped process
   *  may still be marked open for a moment, hence the retries. */
  async closeSession(threadId: string): Promise<void> {
    for (let attempt = 0; attempt < 6; attempt++) {
      const row = this.members.find(member => member.id === threadId);
      if (!row || row.status === 'archived') return;
      try {
        await threadsApi.threadArchive({ projectId: row.projectId, threadId, expectedRevision: row.revision, archived: true });
        await this.refreshMembers();
        return;
      } catch { await sleep(400); await this.refreshMembers(); }
    }
    // It stays as a tab that says "not running"; nothing is lost.
  }

  /** A close says where the session went and how it comes back. Nothing is said when the close did not happen. */
  sayClosed(threadId: string): void {
    const row = this.members.find(member => member.id === threadId);
    if (row?.status !== 'archived') return;
    const owner = this.memberOf.get(threadId);
    const thread = owner ? this.title(owner.id) : null;
    const back = row.harness !== null && row.nativeAvailable === true && this.app.opens.canResume(row.harness) ? 'Resume' : 'Reopen';
    this.app.ui.info(`${this.memberName(row)} is closed. It is kept in ${thread ? `the thread "${thread}"` : 'its thread'}, under Closed sessions of its ⋯ menu, where ${back} brings it back.`);
  }

  /** Brings a closed session back as a tab of its thread, and resumes it when its agent can (the panel's Resume): a closed session
   *  comes back in one step, the way it is used. One no agent can resume comes back as a tab that does not run (Reopen). */
  async restoreSession(threadId: string): Promise<void> {
    const found = await this.sessionRow(threadId);
    if (!found) return;
    const { row, workset } = found;
    await threadsApi.threadArchive({ projectId: row.projectId, threadId, expectedRevision: row.revision, archived: false });
    // Reopened from a thread that is off: the thread comes on, so the reopened tab is seen.
    if (this.isOn(workset.id)) await this.refreshMembers();
    else if (!await this.showWorkset(workset)) { this.app.ui.info('The session is back in its thread. Turn the thread on to see its tab.'); return; }
    // Shown as Show shows a session: its project selected above the tabs, never another project's (2026-11-18).
    await this.focusThreadSession(threadId);
    this.app.opens.resumeIfSaved(threadId);
  }

  /** Deletes one session from Shinatama: a closed one (the sidebar's Delete) or one with nothing saved (Remove). */
  async forgetSession(threadId: string): Promise<void> {
    const row = (await this.sessionRow(threadId))?.row;
    if (!row) return;
    await threadsApi.threadForget({ projectId: row.projectId, threadId, expectedRevision: row.revision });
    await this.refreshMembers();
  }

  /** Renames one session of a thread; its tabs and its thread's panel take the new name. */
  async renameSession(threadId: string, title: string): Promise<void> {
    const row = (await this.sessionRow(threadId))?.row;
    if (!row) return;
    await threadsApi.threadRename({ projectId: row.projectId, threadId, expectedRevision: row.revision, title });
    await this.refreshMembers();
  }

  /** Shows a session of a thread: its thread comes on if it is off, then its tab is brought forward. It starts nothing itself; a "not
   *  running" tab shown this way reopens only if the saved layout restored it as running. */
  async focusThreadSession(threadId: string): Promise<void> {
    const running = this.app.terminals.ofThread(threadId);
    if (running) { await this.focusTab(running.info.id); return; }
    const workset = await this.worksetOf(threadId);
    if (workset && !this.isOn(workset.id) && !await this.showWorkset(workset)) return;
    const panel = this.app.layout.layout.views.find(candidate => candidate.resource.kind === 'terminal' && candidate.resource.threadId === threadId);
    if (panel && this.app.layout.groupOf(panel.id)) {
      this.app.ui.leaveToWorkspace();
      this.app.layout.reveal(panel.id);
    } else this.app.ui.info('That session has no tab in the window.');
  }

  /** Brings a terminal's tab forward: from an alert of the attention widget, a leftover, the Sessions view. A modal dialog that is open
   *  keeps its place: the person is asked first, and this runs when the dialog is gone (`ui.afterDialog`). */
  async focusTab(viewId: string): Promise<void> {
    if (this.app.ui.modalOpen()) { this.app.ui.afterDialog(() => { void this.focusTab(viewId); }); return; }
    this.app.ui.leaveToWorkspace();
    const view = this.app.terminals.get(viewId);
    if (view?.workset && !this.isOn(view.workset.id)) {
      if (await this.showWorkset(view.workset)) await this.focusTab(viewId);
      return;
    }
    if (!view || !this.app.layout.groupOf(viewId)) { this.app.ui.info('This terminal is no longer in the workspace.'); return; }
    this.currentWorkset = this.tabWorkset(viewId)?.id ?? this.currentWorkset;
    this.app.layout.reveal(viewId);
    this.changed();
  }

  // --- the sidebar ---------------------------------------------------------------------------------------------

  /** The projects the sidebar lists, and the one new threads go to (the current project of the window, when it is among them). */
  setProjects(projects: readonly CurrentThreadProject[], current?: CurrentThreadProject): void {
    this.projects = projects;
    if (current) this.currentProject = current;
    this.changed();
  }

  setCurrentProject(project: CurrentThreadProject | null): void {
    this.currentProject = project;
    this.changed();
  }

  /** The state the sidebar kept, read at the start. */
  async loadSidebar(): Promise<void> {
    try {
      const snapshot = await threadsApi.sidebarLoad();
      this.sidebarRevision = snapshot.revision;
      this.sidebarState = snapshot.settings;
      this.changed();
    } catch (error) { this.app.ui.error(`The thread sidebar could not be read: ${errorText(error)}`); }
  }

  /** Keeps the state of the sidebar for the next run. */
  async saveSidebar(state: ThreadSidebarState): Promise<void> {
    const snapshot = await threadsApi.sidebarSave(this.sidebarRevision, state);
    this.sidebarRevision = snapshot.revision;
    this.sidebarState = snapshot.settings;
    this.changed();
  }

  /** The projects whose threads the sidebar lists. */
  activeProjectIds(): string[] {
    return this.sidebarState?.activeProjects ?? [];
  }

  /** Adds a project to the sidebar, or takes it away (its threads go off with it, after asking). Resolves to whether it is listed now. */
  async toggleActive(projectId: string): Promise<boolean> {
    const state: ThreadSidebarState = this.sidebarState ?? { version: 1, width: 236, collapsed: false };
    const active = state.activeProjects ?? [];
    const listed = active.includes(projectId);
    if (listed) {
      await this.hideWorksets(this.openWorksets.filter(workset => workset.projectId === projectId).map(workset => workset.id));
      // Kept on (cancelled, or a stop failed): the project stays in the sidebar, so no tab is left without a row to bring it back.
      if (this.openWorksets.some(workset => workset.projectId === projectId)) return true;
    }
    await this.saveSidebar({ ...state, activeProjects: listed ? active.filter(id => id !== projectId) : [...active, projectId] });
    if (!listed) await this.refreshList(projectId);
    return !listed;
  }

  /** "Choose projects": a folder is put in the sidebar, or taken away. A folder that is not a project yet is recorded as one first (the window's
   *  own project does not change). Said in a notice when Rust refuses it. */
  async toggleProjectAt(path: string): Promise<void> {
    if (this.toggling) return;
    this.toggling = true;
    this.changed();
    try {
      const project = await projectsApi.select(path);
      await this.toggleActive(project.id);
      await this.app.projects.refreshRecent();
    } catch (error) { this.app.ui.error(error); }
    finally { this.toggling = false; this.changed(); }
  }

  /** A project whose session was just started is listed, so that the thread that session is in has its row. */
  async listProject(projectId: string): Promise<void> {
    if (this.activeProjectIds().includes(projectId)) return;
    try { await this.toggleActive(projectId); } catch { /* the thread is on and its tab is shown; the sidebar lists it at the next choice */ }
  }

  /** Does one operation of the sidebar (a rename, an archive, a delete): the controls wait for it, and a refusal is said in a notice that
   *  names what the store refused. Resolves to whether it was done. */
  async operate(doing: string, work: () => Promise<unknown>, what: 'thread' | 'session' = 'thread'): Promise<boolean> {
    if (this.busy) return false;
    this.busy = true;
    this.changed();
    try { await work(); return true; }
    catch (error) { this.app.ui.error(threadFailureText(error, doing, what)); return false; }
    finally { this.busy = false; this.changed(); }
  }

  /** The sidebar lists the archived threads (or the others again): every listed project is read again. */
  async setShowArchived(on: boolean): Promise<void> {
    if (this.showArchived === on) return;
    this.showArchived = on;
    this.changed();
    await Promise.all(this.activeProjectIds().map(project => this.refreshList(project, on)));
  }

  /** How many sessions of a thread run in this window. */
  runningIn(worksetId: string): number {
    return this.app.terminals.runningIn(worksetId);
  }

  /** Reads the threads of a project again. */
  async refreshList(projectId: string, archived: boolean = this.showArchived): Promise<void> {
    try {
      const page = await threadsApi.worksetList({ projectId, archived });
      this.lists.set(projectId, page.rows);
      this.changed();
    } catch (error) { this.app.ui.error(`Threads could not be read: ${errorText(error)}`); }
  }

  /** A new, empty thread in a project, turned on at once so that its first session has a place. */
  async create(projectId: string): Promise<void> {
    const made = await threadsApi.worksetCreate({ projectId }) as { id?: unknown } | null;
    await this.refreshList(projectId);
    if (typeof made?.id === 'string' && /^w_[a-z2-7]{12}$/.test(made.id)) await this.showWorkset({ id: made.id, projectId });
  }

  /** A click on a thread's row: it is turned on (its tabs join the window), or off when it is on (after asking, when sessions run). */
  async toggleOn(thread: { id: string; projectId: string }): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    this.changed();
    try {
      const done = this.isOn(thread.id) ? await this.hideWorkset(thread.id) : await this.showWorkset({ id: thread.id, projectId: thread.projectId });
      if (!done) this.app.ui.error(this.isOn(thread.id) ? 'The thread could not be turned off.' : 'The thread could not be opened. The window stays as it is.');
    } finally { this.busy = false; this.changed(); }
  }

  /** The + of a thread: it is turned on, then the new session asks for its agent (and its folder) in that thread. */
  async addSession(thread: { id: string; projectId: string }): Promise<void> {
    const scope = { id: thread.id, projectId: thread.projectId };
    if (!await this.showWorkset(scope)) return;
    await this.app.opens.newSession('thread', this.app.layout.focusedGroup(), undefined, undefined, scope);
  }

  /** Deletes a thread with its sessions, after asking (the confirming button carries the verb and the object). */
  async deleteThread(thread: Readonly<WorksetSummary>): Promise<void> {
    const sessions = `${thread.sessions} session${thread.sessions === 1 ? '' : 's'}`;
    if (!await this.app.ui.ask({
      title: `Delete the thread ${this.title(thread.id) ?? thread.title ?? 'New thread'}?`,
      description: `It is deleted from Shinatama with its ${sessions}, closed ones included. The agents' own files stay. This cannot be undone.`,
      confirm: 'Delete thread',
    })) return;
    await this.operate('deleted', () => this.forgetWorkset(thread));
  }

  /** Archives a thread (it leaves the list; nothing is deleted), or puts it back. */
  async setArchived(thread: Readonly<WorksetSummary>, archived: boolean): Promise<void> {
    await this.operate(archived ? 'archived' : 'unarchived', () => this.update(thread, { archived }));
  }

  /** Takes a project out of the sidebar: its threads turn off with it, after asking. */
  async removeProject(projectId: string): Promise<void> {
    await this.operate('removed', async () => { if (this.activeProjectIds().includes(projectId)) await this.toggleActive(projectId); });
  }

  /** Renames a thread, or archives it. */
  async update(thread: Readonly<WorksetSummary>, change: { title?: string; archived?: boolean }): Promise<void> {
    await threadsApi.worksetUpdate({ worksetId: thread.id, expectedRevision: thread.revision, ...change });
    await this.refreshList(thread.projectId);
  }

  /** Reads the names agents gave the sessions in `threadIds`. Not while a session starts: a name read and a launch share the same worker
   *  capacity, so a launch waits for the read that is under way, and a read that finds a launch under way is refused. */
  async readNames(threadIds: string[]): Promise<void> {
    if (this.app.opens.queue.busy()) return;
    const call = threadsApi.names(threadIds);
    this.namesInFlight = call.catch(() => undefined);
    try {
      for (const entry of await call) if (entry.name) this.names.set(entry.threadId, entry.name);
      this.changed();
    } catch { /* the names keep their last state */ }
  }

  /** The names of the sessions that lead threads with no name, read again at most once a minute for each (the sidebar asks every few
   *  seconds while it is shown). At most 16 at a time. */
  async requestNames(now: number = Date.now()): Promise<void> {
    const ids: string[] = [];
    for (const rows of this.lists.values()) {
      for (const row of rows) {
        const lead = row.lead;
        if (row.title || !lead?.harness || this.names.has(lead.threadId)) continue;
        if (lead.title && lead.title !== this.harnessLabel(lead.harness)) continue;
        const at = this.namedAt.get(lead.threadId);
        if (at === undefined || now - at >= 60_000) ids.push(lead.threadId);
      }
    }
    const asked = ids.slice(0, 16);
    if (!asked.length || this.app.opens.queue.busy()) return;
    for (const id of asked) this.namedAt.set(id, now);
    await this.readNames(asked);
  }

  /** Resolves when the read of names that is under way has ended: a launch waits for it. */
  idleNames(): Promise<unknown> {
    return this.namesInFlight;
  }
}
