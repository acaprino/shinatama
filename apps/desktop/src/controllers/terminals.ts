// The terminals of the window: the sessions that run in this process (the pseudo-terminals live in Rust), what they print on its way
// to the screen that draws it, what the person types, and how a terminal ends. The screen of a terminal is Sigrid's `Terminal`:
// it connects to its session with `connect(id, pipe)` and sends what the person typed with `input(id, data)` and its size with
// `resize(id, cols, rows)`. Which tab a terminal is in, and which session of which thread it is, is the layout's and the threads'.
import { closeWindow, errorText, sleep } from '../api/core.ts';
import { settingsApi } from '../api/settings.ts';
import { terminalsApi, type SessionInfo } from '../api/terminals.ts';
import { workspaceApi } from '../api/workspace.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';
import { holdOutput, splitOutput } from '../terminal-output.ts';
import { classifyTarget } from '../terminal-links.ts';
import { parseResourceHealth, startTerminalResources, type ResourceHealth, type TerminalResourceStatus, type WindowTelemetry } from '../terminal-resources.ts';
import { withoutInstance } from '../session-tabs.ts';
import type { WorksetScope } from '../api/threads.ts';

/** A terminal that runs here, or ran until it ended. */
export interface TerminalSession {
  readonly info: SessionInfo;
  /** The session of its thread this terminal runs (null for a terminal of no thread). */
  readonly threadId: string | null;
  /** The thread it belongs to; a terminal of a thread that is off runs with no tab. */
  readonly workset: WorksetScope | null;
  ended: boolean;
  exitCode: number | null;
  /** Taken away from the window: nothing is read from it any more. */
  disposed: boolean;
  /** The size the pseudo-terminal was last given. */
  size: { cols: number; rows: number } | null;
}

/** Where the output of a terminal is written: the screen's `Terminal`. `done` is called when it has been parsed. */
export interface TerminalPipe { write(data: Uint8Array, done: () => void): void }

/** The layout's id of a terminal's instance: a session id, or a made-up one for a terminal of no session. */
export const instanceRef = (id: string): string => /^i_[a-z2-7]{12}$/.test(id) ? id : `prototype-${id}`;

export class TerminalController extends Store {
  readonly sessions = new Map<string, TerminalSession>();
  /** The measures of each terminal (memory, processes), read every second while the window is shown. */
  readonly metrics = new Map<string, TerminalResourceStatus>();
  /** The close of the window is confirmed: the terminals the shutdown ends stay saved in the layout as running, so the next start
   *  restores them with it (a terminal that ends by itself is saved as not running). */
  closing = false;
  /** The terminal whose search (Find in terminal) is open, or none. */
  finding: string | null = null;
  /** The lines of the terminal being searched that hold the text, for the strip of its lines to mark (empty when none, or no search). */
  findHits: readonly number[] = [];
  /** What the backend last said of the machine's resources, after the window's last report; null before the first. */
  health: ResourceHealth | null = null;
  private reportedAt: number | null = null;
  private readonly pipes = new Map<string, TerminalPipe>();
  private readonly held = new Map<string, Uint8Array[]>();
  private readonly readers = new Map<string, () => string>();
  private readonly resizing = new Map<string, Promise<void>>();
  private stopMetrics: (() => void) | null = null;

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  start(): void {
    this.stopMetrics = startTerminalResources(statuses => {
      this.metrics.clear();
      for (const status of statuses ?? []) this.metrics.set(status.id, status);
      this.changed();
    });
  }

  dispose(): void {
    this.stopMetrics?.();
    for (const session of this.sessions.values()) session.disposed = true;
  }

  // --- reading ---------------------------------------------------------------------------------------

  get(id: string): TerminalSession | undefined {
    return this.sessions.get(id);
  }

  /** A terminal of this session that is on screen and running. */
  runningOf(threadId: string): TerminalSession | undefined {
    return [...this.sessions.values()].find(session => session.threadId === threadId && !session.ended && !session.disposed);
  }

  /** The terminal of a session, running or not (an ended terminal keeps its tab). */
  ofThread(threadId: string): TerminalSession | undefined {
    return [...this.sessions.values()].find(session => session.threadId === threadId && !session.disposed);
  }

  /** How many terminals of a thread run. */
  runningIn(worksetId: string): number {
    return [...this.sessions.values()].filter(session => session.workset?.id === worksetId && !session.ended && !session.disposed).length;
  }

  hasLive(): boolean {
    return [...this.sessions.values()].some(session => !session.ended && !session.disposed);
  }

  // --- links, search and the machine -----------------------------------------------------------------------

  /** A link in a terminal was pressed: a web address or a local HTML page opens in a browser tab; anything else says why it does not. */
  openLink(id: string, target: string): void {
    const session = this.sessions.get(id);
    const link = classifyTarget(target, session?.info.cwd ?? '');
    if (link.kind === 'web') void this.app.browser.open({ url: link.url });
    else if (link.kind === 'file') void this.app.browser.open({ file: link.path });
    else this.app.ui.info(link.reason);
  }

  /** Opens the search of a terminal's retained output (the one in the pane the person works in when none is named). */
  openFind(id?: string): void {
    const target = id ?? this.app.layout.activeView()?.id;
    if (!target || !this.sessions.has(target)) { this.app.ui.info('Open or select a terminal to search its retained output.'); return; }
    this.app.ui.leaveToWorkspace();
    this.finding = target;
    this.changed();
  }

  closeFind(): void {
    if (this.finding === null) return;
    this.finding = null;
    this.findHits = [];
    this.changed();
  }

  /** The lines that hold what is searched in the terminal `id`: the strip marks them while the search is open. Ignored for another terminal. */
  setFindHits(id: string, rows: readonly number[]): void {
    if (this.finding !== id) return;
    if (rows.length === this.findHits.length && rows.every((row, at) => row === this.findHits[at])) return;
    this.findHits = rows;
    this.changed();
  }

  /** The window reports what it measured of itself, and learns what the backend makes of it (a refusal leaves the last answer). Resolves
   *  to how long the round trip took, in microseconds, or null when it failed. */
  async reportTelemetry(sample: Omit<WindowTelemetry, 'ipc_roundtrip_us'>): Promise<number | null> {
    const started = Date.now();
    try {
      const telemetry: WindowTelemetry = { ...sample, ipc_roundtrip_us: this.reportedAt };
      const health = parseResourceHealth(await settingsApi.resourcesTelemetry(telemetry));
      this.reportedAt = (Date.now() - started) * 1000;
      if (health) { this.health = health; this.changed(); }
      return this.reportedAt;
    } catch { this.reportedAt = null; return null; }
  }

  // --- attaching ---------------------------------------------------------------------------------------

  /** Takes a terminal Rust holds into the window: its tab (when its thread is on), and the reading of what it prints.
   *  `workset`: the thread it belongs to; a terminal opened into a thread that is off runs without a tab, and is drawn when that
   *  thread is turned on. `background`: reopened by the start, it takes its tab's place without bringing it forward. */
  async attach(info: SessionInfo, group: string, threadId: string | null = null, workset: WorksetScope | null = null, background = false): Promise<void> {
    // Native collaboration events and reload recovery may see the same owned process. Attach once; neither delivery path is
    // permission to relaunch it.
    if (this.sessions.has(info.id)) return;
    const session: TerminalSession = { info, threadId, workset, ended: false, exitCode: null, disposed: false, size: null };
    this.sessions.set(info.id, session);
    const { layout, threads } = this.app;
    const restored = layout.view(info.id);
    const shownHere = threads.isOn(workset?.id);
    if (!restored && shownHere) {
      // The running terminal takes the place of the tab that said "not running".
      const stale = threadId ? layout.layout.views.find(panel => panel.resource.kind === 'terminal' && panel.resource.threadId === threadId && !this.sessions.has(panel.id)) : undefined;
      const staleGroup = stale && layout.groupOf(stale.id);
      const target = staleGroup?.id ?? (layout.groups.some(candidate => candidate.id === group) ? group : layout.focusedGroup());
      layout.change({
        type: 'add-view', groupId: target, index: staleGroup ? staleGroup.tabs.indexOf(stale!.id) : undefined,
        view: { id: info.id, resource: { kind: 'terminal', instanceId: instanceRef(info.id), threadId, profileId: null }, hidden: false },
      });
      // In the background the pane keeps showing what it showed: the new terminal is brought forward only where its tab was the one shown.
      const wasShown = !!staleGroup && staleGroup.activeTabId === stale?.id;
      if (stale) {
        layout.change({ type: 'remove-view', viewId: stale.id });
        if (!background || wasShown) layout.change({ type: 'activate', groupId: target, viewId: info.id });
      }
    }
    threads.rememberSessions();
    this.changed();
    if (shownHere && !background) { const pane = layout.groupOf(info.id); if (pane) layout.focus(pane.id); }
    void this.drain(session);
  }

  /** Reconnects the terminals Rust still holds after the page was reloaded: it never starts a process. */
  async reattach(known: Record<string, { threadId: string | null; workset: WorksetScope | null }>): Promise<number> {
    const existing = await terminalsApi.list();
    for (const info of existing) await this.attach(info, this.app.layout.focusedGroup(), known[info.id]?.threadId ?? null, known[info.id]?.workset ?? null);
    return existing.length;
  }

  // --- output and input --------------------------------------------------------------------------------

  /** Connects the screen of a terminal: what it printed while there was no screen is written first, then the rest as it comes.
   *  Gives back how to disconnect. */
  connect(id: string, pipe: TerminalPipe): () => void {
    this.pipes.set(id, pipe);
    const held = this.held.get(id) ?? [];
    this.held.delete(id);
    for (const part of held) pipe.write(part, () => {});
    return () => { if (this.pipes.get(id) === pipe) this.pipes.delete(id); };
  }

  /** What the person typed (or a reply of the terminal that must reach the program, hidden terminals included). */
  input(id: string, data: string): void {
    const session = this.sessions.get(id);
    if (!session || session.ended || session.disposed) return;
    void terminalsApi.write(id, data).catch(error => this.app.ui.error(error));
  }

  /** The screen has this size (its first fit and each change): the pseudo-terminal is given it, one resize at a time. */
  resize(id: string, cols: number, rows: number): void {
    const session = this.sessions.get(id);
    if (!session || cols < 2 || rows < 2) return;
    if (session.size?.cols === cols && session.size.rows === rows) return;
    session.size = { cols, rows };
    if (session.ended) return;
    const chain = (this.resizing.get(id) ?? Promise.resolve()).then(async () => {
      if (!session.disposed && !session.ended) await terminalsApi.resize(id, rows, cols);
    }).catch(async error => {
      session.size = null;
      if (!session.disposed && !await this.endedElsewhere(id)) this.app.ui.error(error);
    });
    this.resizing.set(id, chain);
    this.changed();
  }

  /** The screen tells how to read its text (the hop waits for what the destination prints). */
  setReader(id: string, reader: (() => string) | null): void {
    if (reader) this.readers.set(id, reader); else this.readers.delete(id);
  }

  screenText(id: string): string {
    return this.readers.get(id)?.() ?? '';
  }

  private async deliver(id: string, part: Uint8Array): Promise<void> {
    const pipe = this.pipes.get(id);
    if (!pipe) { this.held.set(id, holdOutput(this.held.get(id) ?? [], part)); return; }
    await new Promise<void>(resolve => pipe.write(part, resolve));
  }

  private async drain(session: TerminalSession): Promise<void> {
    const id = session.info.id;
    try {
      while (!session.disposed) {
        const batch = await terminalsApi.read(id);
        if (session.disposed) return;
        for (const chunk of batch.chunks) {
          for (const part of splitOutput(new Uint8Array(chunk))) {
            if (session.disposed) return;
            await this.deliver(id, part);
          }
        }
        if (batch.error) this.app.ui.error(`Terminal output: ${batch.error}`);
        if (batch.eof) { this.ended(session, batch.exit_code); return; }
        await sleep(25);
      }
    } catch (error) {
      if (!session.disposed && !await this.endedElsewhere(id)) this.app.ui.error(`Terminal connection interrupted: ${errorText(error)}`);
    }
  }

  // --- ending ------------------------------------------------------------------------------------------

  private ended(session: TerminalSession, code: number | null): void {
    session.ended = true;
    session.exitCode = code;
    this.savedAsEnded(session);
    this.app.threads.touch();
    this.changed();
  }

  /** A terminal stopped by something other than this window (another window, an agent's tool, a hop): its tab says it exited, as
   *  when the process ends, instead of reporting an error. */
  async endedElsewhere(id: string): Promise<boolean> {
    const session = this.sessions.get(id);
    if (!session) return true;
    if (session.ended) return true;
    try {
      const alive = await terminalsApi.list();
      if (alive.some(info => info.id === id) || session.disposed || session.ended) return session.ended;
    } catch { return false; }
    this.ended(session, null);
    return true;
  }

  /** A terminal that ended by itself, or was stopped from elsewhere, is saved in the layout as not running: the next start restores
   *  its tab without reopening it. Not the ends of a confirmed window close. */
  private savedAsEnded(session: TerminalSession): void {
    if (this.closing) return;
    const { layout } = this.app;
    layout.replace(withoutInstance(layout.layout, session.info.id));
  }

  /** Stops the process of a terminal and takes it away; its session stays in its thread under Closed sessions. Asks first when it
   *  runs: the confirming button carries the verb and the object. */
  async stop(id: string): Promise<void> {
    const session = this.sessions.get(id);
    if (!session) return;
    const kept = session.threadId !== null && session.workset !== null;
    if (!session.ended && !await this.app.ui.ask({
      title: kept ? 'Close session?' : 'Stop terminal?',
      description: kept
        ? 'This stops the agent and the programs it started. The session stays in this thread under Closed sessions, where it can be reopened or deleted.'
        : 'This stops the shell and the programs it started.',
      confirm: kept ? 'Close session' : 'Stop terminal',
    })) return;
    try {
      await terminalsApi.stop(id);
      this.discard(id);
      const { threads } = this.app;
      if (session.threadId && threads.isOn(session.workset?.id)) {
        await threads.refreshMembers();
        await threads.closeSession(session.threadId);
        threads.sayClosed(session.threadId);
      }
      const pane = this.app.layout.focusedGroup();
      this.app.layout.focus(pane);
      threads.touch();
    } catch (error) { this.app.ui.error(error); }
  }

  /** Removes a terminal that is stopped from the window; its saved session is not touched. A tab that was its session's stays as a
   *  tab that says "not running". */
  discard(id: string): void {
    const session = this.sessions.get(id);
    if (!session) return;
    session.disposed = true;
    this.pipes.delete(id);
    this.held.delete(id);
    this.readers.delete(id);
    this.resizing.delete(id);
    this.metrics.delete(id);
    this.sessions.delete(id);
    if (this.finding === id) { this.finding = null; this.findHits = []; }
    const { layout, threads } = this.app;
    threads.rememberSessions();
    if (layout.view(id)) {
      const group = layout.groupOf(id);
      const member = session.threadId && threads.isOn(session.workset?.id) ? threads.members.find(row => row.id === session.threadId) : undefined;
      if (group && member) {
        layout.change({
          type: 'add-view', groupId: group.id, index: group.tabs.indexOf(id),
          view: { id: `saved-${member.id}`, resource: { kind: 'terminal', instanceId: null, threadId: member.id, profileId: null }, hidden: false },
        });
      }
      layout.change({ type: 'remove-view', viewId: id });
    }
    this.changed();
  }

  // --- the window's close ---------------------------------------------------------------------------------

  /** Closes the workspace as the window's close does: preferences saved, the layout released with its running sessions (the next
   *  start restores them), terminals ended, window closed. A relaunch starts the new Shinatama at the point of no return, after any
   *  question; it waits for this process to end, and if that start is refused the terminals keep running and the window stays. */
  async closeWorkspace(relaunch: boolean): Promise<void> {
    const appearanceSaved = await this.app.settings.flush();
    const { released, unsaved } = await this.app.layout.release();
    // Asked only when something would be lost: a release refused while nothing is unsaved (storage read-only, newer schema) loses
    // nothing, and the next start takes the layout over.
    if (!appearanceSaved || (released && unsaved)) {
      if (!await this.app.ui.ask({
        title: 'Close without saving the latest preferences?',
        description: 'Previously saved data is preserved. Recent layout or appearance changes could not be saved.',
        confirm: 'Close without saving',
      })) return;
      await workspaceApi.allowClose();
    }
    // The point of no return: a restart starts its successor here, never before a question the user may decline.
    if (relaunch) await workspaceApi.relaunch();
    // From here the terminals end because the window closes: their tabs stay saved as running (the layout was just released with
    // them so), and the next start restores those sessions with the layout.
    this.closing = true;
    try { await terminalsApi.shutdown(); } catch (error) { this.closing = false; throw error; }
    for (const session of this.sessions.values()) session.disposed = true;
    await closeWindow();
  }
}
