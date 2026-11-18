// Pure decisions about the tabs of a thread's sessions: how a tab is named so that two sessions
// of one thread can be told apart, and which sessions are reopened after a restart.
import type { ThreadSummary } from './thread-sidebar.ts';
import { tabGroups, type WorkspaceLayout } from './workspace-model.ts';

/** "Claude Code 2": the session's name, numbered when other sessions of its thread share it.
 *  `siblings` are all the sessions of the thread, oldest first, closed ones included, so a
 *  session keeps its number when another one is closed. Null when the session is not among them. */
export function sessionTabName(siblings: readonly { id: string; name: string }[], id: string): string | null {
  const mine = siblings.find(session => session.id === id);
  if (!mine) return null;
  const same = siblings.filter(session => session.name === mine.name);
  return same.length > 1 ? `${mine.name} ${same.indexOf(mine) + 1}` : mine.name;
}

/** The session first, then its thread: a tab is cut at its end, and nine tabs that all began with
 *  the thread's name read as one tab nine times. The thread is also the tab's colour. */
export function sessionTabLabel(tab: { name: string; where: string | null; state: 'running' | 'saved' | 'exited'; mark?: string }): string {
  const state = tab.state === 'saved' ? ' · not running' : tab.state === 'exited' ? ' · exited' : tab.mark ?? '';
  return `${tab.name}${tab.where ? ` · ${tab.where}` : ''}${state}`;
}

export type RestoredTab = { panelId: string; threadId: string };
/** The tabs whose sessions the saved layout says were running, in the order of the panes and of their
 *  tabs (2026-11-18, the owner: the layout and its sessions are restored together, never one without
 *  the other). A running terminal's tab is saved with its instance; a tab whose session was stopped,
 *  ended by itself or never ran has none. So a tab that still carries an instance while no terminal of
 *  this window holds it (`live`) is a session that ran when Shinatama last closed or crashed, not reopened yet. */
export function restoredRunning(layout: Pick<WorkspaceLayout, 'root' | 'views'>, live: (panelId: string) => boolean): RestoredTab[] {
  const order = tabGroups(layout.root).flatMap(group => group.tabs);
  const rank = (id: string) => { const at = order.indexOf(id); return at < 0 ? order.length : at; };
  return layout.views
    .filter(panel => panel.resource.kind === 'terminal' && panel.resource.instanceId !== null && panel.resource.threadId !== null && !live(panel.id))
    .sort((a, b) => rank(a.id) - rank(b.id))
    .map(panel => ({ panelId: panel.id, threadId: (panel.resource as { threadId: string }).threadId }));
}
/** The layout with this tab no longer saying its session runs: it reopened, its reopen was refused,
 *  or its terminal ended by itself. The same layout when there is nothing to change. */
export function withoutInstance<T extends Pick<WorkspaceLayout, 'views'>>(layout: T, panelId: string): T {
  const panel = layout.views.find(view => view.id === panelId);
  if (panel?.resource.kind !== 'terminal' || panel.resource.instanceId === null) return layout;
  const resource = { ...panel.resource, instanceId: null };
  return { ...layout, views: layout.views.map(view => view === panel ? { ...view, resource } : view) };
}

export type ReopenStep = 'resume' | 'recover' | null;
/** What a restored tab does by itself. Only a session that was running when Shinatama last closed
 *  (`wanted`: restoredRunning) is reopened: one the user stopped stays "not running" until asked.
 *  A session still marked open by the Shinatama that is gone is checked first (`recover`). */
export function reopenStep(
  session: Pick<ThreadSummary, 'id' | 'status' | 'nativeAvailable' | 'harness'>,
  wanted: ReadonlySet<string>, canResume: (harness: string | null) => boolean,
): ReopenStep {
  if (!wanted.has(session.id)) return null;
  if (session.status === 'recorded_open') return 'recover';
  return session.status === 'inactive' && session.nativeAvailable === true && canResume(session.harness) ? 'resume' : null;
}

export const attemptKey = (sessionId: string, step: Exclude<ReopenStep, null>) => `${sessionId}:${step}`;
/** The restored tab to reopen next: the first, in the order of the panes and tabs, whose session has
 *  a step not tried yet in this run. `stepOf` is undefined while the session's row is not read: that
 *  tab is passed over, not given up. Each (session, step) is tried once, so a refusal never loops. */
export function nextToReopen(tabs: readonly RestoredTab[], stepOf: (tab: RestoredTab) => ReopenStep | undefined, attempted: ReadonlySet<string>):
  { tab: RestoredTab; step: Exclude<ReopenStep, null> } | null {
  for (const tab of tabs) {
    const step = stepOf(tab);
    if (step && !attempted.has(attemptKey(tab.threadId, step))) return { tab, step };
  }
  return null;
}
/** What the card of a restored tab says: its session is `reopening` now, is `queued` behind the one
 *  that is (it then offers "Don't reopen"), or the card `offer`s its button as before. */
export function reopenCard(step: ReopenStep, sessionId: string, state: { attempted: ReadonlySet<string>; current: string | null; active: boolean }): 'reopening' | 'queued' | 'offer' {
  if (state.current === sessionId) return 'reopening';
  return state.active && step && !state.attempted.has(attemptKey(sessionId, step)) ? 'queued' : 'offer';
}

export type SessionState = 'running' | 'saved' | 'unreachable' | 'open' | 'unbound' | 'closed';
type Row = Pick<ThreadSummary, 'id' | 'status' | 'nativeAvailable' | 'updatedAt'>;
/** A session of a thread as the thread's panel says it. `running`: a terminal of this window runs it
 *  (its record then says open: that open is this window's own). */
export function sessionState(row: Pick<Row, 'status' | 'nativeAvailable'>, running: boolean): SessionState {
  if (running) return 'running';
  if (row.status === 'archived') return 'closed';
  if (row.status === 'recorded_open') return 'open';
  if (row.status === 'unbound') return 'unbound';
  return row.nativeAvailable === true ? 'saved' : 'unreachable';
}
const STATE_TEXT: Record<SessionState, string> = {
  running: 'Running', saved: 'Not running · conversation saved', unreachable: 'Not running · its conversation cannot be reached now',
  open: 'Not running · marked open by a session that is no longer here', unbound: 'Not running · nothing saved', closed: 'Closed',
};
export const sessionStateText = (state: SessionState) => STATE_TEXT[state];
export type ThreadSession = { id: string; name: string; state: SessionState; updatedAt: number };
/** Every session of a thread (`rows` oldest first, closed ones included), named as its tab is. */
export function threadSessions<T extends Row>(rows: readonly T[], nameOf: (row: T) => string, running: ReadonlySet<string>): ThreadSession[] {
  const named = rows.map(row => ({ id: row.id, name: nameOf(row) }));
  return rows.map(row => ({ id: row.id, name: sessionTabName(named, row.id) ?? nameOf(row), state: sessionState(row, running.has(row.id)), updatedAt: row.updatedAt }));
}

/** The sessions a forget of their thread recovers first: still marked open by a Shinatama that is
 *  gone (a crash), which the forget refuses. One a terminal of this window runs is not among them. */
export const rowsToRecover = <T extends Pick<Row, 'id' | 'status'>>(rows: readonly T[], live: ReadonlySet<string>): T[] =>
  rows.filter(row => row.status === 'recorded_open' && !live.has(row.id));
/** Why a forget is refused when the recovery of such a session is: "running, stop it first" only when
 *  the session or the Shinatama that opened it is really alive; otherwise its owner is gone but could
 *  not be verified. */
export function forgetRefusal(error: unknown): 'owned_elsewhere' | 'open_unverified' {
  const code = error && typeof error === 'object' && 'code' in error ? (error as { code: unknown }).code : error;
  return code === 'session_alive' || code === 'owner_alive' ? 'owned_elsewhere' : 'open_unverified';
}

/** What turning threads off does (2026-11-18, the owner: turning a thread off closes it, it does not
 *  delete it). The terminals of those threads leave the window (`leave`); the ones still running stop
 *  (`stop`) and their sessions stay in the thread, not running, resumable when it is turned on again.
 *  Asked first only when one still runs. Nothing is archived or forgotten. */
export function threadsOff<T extends { workset: { id: string } | null; ended: boolean; disposed: boolean }>(views: readonly T[], ids: readonly string[]):
  { stop: T[]; leave: T[]; ask: boolean } {
  const leave = views.filter(view => !view.disposed && !!view.workset && ids.includes(view.workset.id));
  const stop = leave.filter(view => !view.ended);
  return { stop, leave, ask: stop.length > 0 };
}
