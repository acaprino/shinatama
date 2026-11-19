import { test } from 'node:test';
import assert from 'node:assert/strict';
import { forgetRefusal, openStep, threadsOff, restoredRunning, rowsToRecover, sessionState, sessionStateText, sessionTabLabel, sessionTabName, threadSessions, withoutInstance } from '../src/session-tabs.ts';
import { emptyWorkspace, reduceWorkspaceLayout, type LayoutAction, type WorkspaceLayout } from '../src/workspace-model.ts';

const named = (id: string, name: string) => ({ id, name });

// 2026-11-18: nine sessions of one thread were nine tabs reading "Silente e il pensato…".
test('sessions of one thread that share a name are numbered, oldest first, and keep their number', () => {
  const thread = [named('t_a', 'Claude Code'), named('t_b', 'Codex'), named('t_c', 'Claude Code'), named('t_d', 'Claude Code')];
  assert.equal(sessionTabName(thread, 't_a'), 'Claude Code 1');
  assert.equal(sessionTabName(thread, 't_c'), 'Claude Code 2');
  assert.equal(sessionTabName(thread, 't_d'), 'Claude Code 3');
  // The only one of its name carries no number; a name the user gave is used as it is.
  assert.equal(sessionTabName(thread, 't_b'), 'Codex');
  assert.equal(sessionTabName([...thread, named('t_e', 'Fix the proxy')], 't_e'), 'Fix the proxy');
  // Closing the second one (it stays among the thread's sessions) renumbers nothing.
  assert.equal(sessionTabName(thread, 't_d'), 'Claude Code 3');
  assert.equal(sessionTabName(thread, 't_missing'), null);
});
test('a tab names its session first, then the thread: what tells two tabs apart is not cut off', () => {
  assert.equal(sessionTabLabel({ name: 'Claude Code 2', where: 'Silente e il pensatorio', state: 'running' }), 'Claude Code 2 · Silente e il pensatorio');
  assert.equal(sessionTabLabel({ name: 'Claude Code 2', where: 'Silente e il pensatorio', state: 'saved' }), 'Claude Code 2 · Silente e il pensatorio · not running');
  assert.equal(sessionTabLabel({ name: 'Codex', where: 'ethel', state: 'exited' }), 'Codex · ethel · exited');
  assert.equal(sessionTabLabel({ name: 'PowerShell', where: null, state: 'running', mark: ' · chat' }), 'PowerShell · chat');
  // 2026-11-19: a tab said "not running" while its session was opening or waiting for its turn.
  assert.equal(sessionTabLabel({ name: 'Codex', where: 'ethel', state: 'opening' }), 'Codex · ethel · opening');
  assert.equal(sessionTabLabel({ name: 'Codex', where: 'ethel', state: 'waiting' }), 'Codex · ethel · waiting to open');
});

const row = (over: Record<string, unknown> = {}) => ({ status: 'inactive' as 'unbound' | 'inactive' | 'recorded_open' | 'archived', nativeAvailable: true as boolean | null, harness: 'claude_code' as string | null, ...over });
const resumes = (harness: string | null) => harness === 'claude_code' || harness === 'codex';
test('opening a saved session resumes it, recovers it first when it was left marked open, or has nothing to open', () => {
  assert.equal(openStep(row(), resumes), 'resume');
  // Marked open by a Shinatama that is gone: that is checked first, then its record is read again.
  assert.equal(openStep(row({ status: 'recorded_open' }), resumes), 'recover');
  assert.equal(openStep(row({ status: 'recorded_open', harness: 'opencode' }), resumes), 'recover');
  // Nothing to open, with the code of why: closed, nothing saved, a conversation out of reach, an agent that cannot resume.
  assert.deepEqual(openStep(row({ status: 'archived' }), resumes), { nothing: 'closed' });
  assert.deepEqual(openStep(row({ status: 'unbound', harness: null }), resumes), { nothing: 'unbound' });
  assert.deepEqual(openStep(row({ nativeAvailable: false }), resumes), { nothing: 'native_unreachable' });
  assert.deepEqual(openStep(row({ nativeAvailable: null }), resumes), { nothing: 'native_unreachable' });
  assert.deepEqual(openStep(row({ harness: 'opencode' }), resumes), { nothing: 'unsupported_harness' });
});

const terminal = (id: string, threadId: string | null = null) => ({ id, resource: { kind: 'terminal' as const, instanceId: null, threadId, profileId: null }, hidden: false });
function layoutOf(...actions: LayoutAction[]): WorkspaceLayout {
  let layout = emptyWorkspace('g1');
  for (const action of actions) { const result = reduceWorkspaceLayout(layout, action); assert.equal(result.ok, true, JSON.stringify(action)); layout = result.layout; }
  return layout;
}
test('the sessions the saved layout holds as running come in the order of the panes and their tabs', () => {
  const split = layoutOf({ type: 'add-view', groupId: 'g1', view: running('i_bbbbbbbbbbbb', 't_b') }, { type: 'add-view', groupId: 'g1', view: running('i_aaaaaaaaaaaa', 't_a') },
    { type: 'split', groupId: 'g1', view: running('i_cccccccccccc', 't_c'), axis: 'columns', side: 'before', newGroupId: 'g0', newSplitId: 's1' });
  assert.deepEqual(restoredRunning(split, () => false).map(tab => tab.threadId), ['t_c', 't_b', 't_a']);
});

const summary = (id: string, over: Record<string, unknown> = {}) => ({ id, status: 'inactive' as 'unbound' | 'inactive' | 'recorded_open' | 'archived', nativeAvailable: true as boolean | null, updatedAt: 1, ...over });
test('a session of a thread is said in words: running, saved, marked open, nothing saved, closed', () => {
  assert.equal(sessionState(summary('t_a', { status: 'recorded_open' }), true), 'running');
  assert.equal(sessionState(summary('t_a'), false), 'saved');
  assert.equal(sessionState(summary('t_a', { nativeAvailable: false }), false), 'unreachable');
  assert.equal(sessionState(summary('t_a', { status: 'recorded_open' }), false), 'open');
  assert.equal(sessionState(summary('t_a', { status: 'unbound' }), false), 'unbound');
  assert.equal(sessionState(summary('t_a', { status: 'archived' }), false), 'closed');
  assert.equal(sessionStateText('running'), 'Running');
  assert.equal(sessionStateText('saved'), 'Not running · conversation saved');
  assert.equal(sessionStateText('open'), 'Not running · marked open by a session that is no longer here');
  assert.equal(sessionStateText('unbound'), 'Not running · nothing saved');
  assert.equal(sessionStateText('closed'), 'Closed');
});
test('the thread\'s panel lists every session, named as its tab is', () => {
  const rows = [summary('t_a'), summary('t_b', { status: 'archived' }), summary('t_c', { status: 'unbound' })];
  const listed = threadSessions(rows, () => 'Claude Code', new Set(['t_a']));
  assert.deepEqual(listed.map(item => [item.id, item.name, item.state]), [['t_a', 'Claude Code 1', 'running'], ['t_b', 'Claude Code 2', 'closed'], ['t_c', 'Claude Code 3', 'unbound']]);
});
test('forgetting a thread first recovers the sessions left marked open by a Shinatama that is gone', () => {
  const rows = [summary('t_a', { status: 'recorded_open' }), summary('t_b'), summary('t_c', { status: 'recorded_open' })];
  // One that runs in this window is not "left open": the forget is refused for it before anything.
  assert.deepEqual(rowsToRecover(rows, new Set(['t_c'])).map(row => row.id), ['t_a']);
  // "Running, stop it first" only when the session or its owner is really alive.
  assert.equal(forgetRefusal({ code: 'session_alive' }), 'owned_elsewhere');
  assert.equal(forgetRefusal({ code: 'owner_alive' }), 'owned_elsewhere');
  assert.equal(forgetRefusal({ code: 'cleanup_unproven' }), 'open_unverified');
  assert.equal(forgetRefusal('unavailable'), 'open_unverified');
  assert.equal(forgetRefusal(null), 'open_unverified');
});

// 2026-11-18, the owner: the layout and its sessions are restored together, never one without the other.
const running = (id: string, threadId: string | null) => ({ id, resource: { kind: 'terminal' as const, instanceId: id, threadId, profileId: null }, hidden: false });
test('the saved layout says which sessions were running: a tab saved with its instance and no terminal now', () => {
  const layout = layoutOf({ type: 'add-view', groupId: 'g1', view: running('i_aaaaaaaaaaaa', 't_a') }, { type: 'add-view', groupId: 'g1', view: terminal('saved-t_b', 't_b') },
    { type: 'add-view', groupId: 'g1', view: running('i_cccccccccccc', null) }, { type: 'add-view', groupId: 'g1', view: running('i_dddddddddddd', 't_d') });
  // t_b was stopped (its tab has no instance), i_ccc… belongs to no thread, i_ddd… runs in this window.
  assert.deepEqual(restoredRunning(layout, id => id === 'i_dddddddddddd'), [{ panelId: 'i_aaaaaaaaaaaa', threadId: 't_a' }]);
});
test('a tab that has reopened, was refused or ended by itself no longer says it was running', () => {
  const layout = layoutOf({ type: 'add-view', groupId: 'g1', view: running('i_aaaaaaaaaaaa', 't_a') }, { type: 'add-view', groupId: 'g1', view: terminal('saved-t_b', 't_b') });
  const settled = withoutInstance(layout, 'i_aaaaaaaaaaaa');
  assert.deepEqual(restoredRunning(settled, () => false), []);
  // The tab keeps its place, its session and its profile; only its instance is gone.
  assert.deepEqual(settled.views.find(view => view.id === 'i_aaaaaaaaaaaa')?.resource, { kind: 'terminal', instanceId: null, threadId: 't_a', profileId: null });
  assert.equal(settled.root, layout.root);
  // Nothing to change: the same layout comes back.
  assert.equal(withoutInstance(layout, 'saved-t_b'), layout);
  assert.equal(withoutInstance(layout, 'i_missing'), layout);
});

// 2026-11-18, the owner: turning a thread off closes it ("se le ho chiuse restano chiuse"), it does not delete it.
test('turning threads off stops their running sessions, after asking only when one still runs', () => {
  const view = (workset: string | null, ended = false, disposed = false) => ({ workset: workset ? { id: workset } : null, ended, disposed });
  const all = [view('w_a'), view('w_a', true), view('w_b'), view(null), view('w_a', false, true)];
  const plan = threadsOff(all, ['w_a']);
  // The running one stops; the exited one only leaves; the other thread, the loose terminal and a removed view are untouched.
  assert.deepEqual(plan.stop, [all[0]]);
  assert.deepEqual(plan.leave, [all[0], all[1]]);
  assert.equal(plan.ask, true);
  // Nothing runs any more: the thread goes off without asking.
  assert.deepEqual(threadsOff([all[1]], ['w_a']), { stop: [], leave: [all[1]], ask: false });
  assert.deepEqual(threadsOff(all, ['w_c']), { stop: [], leave: [], ask: false });
});

// 2026-11-18: a closed session offered "Reopen", which only put back a tab that does not run; Resume was a second step.
import { closedSessionAction } from '../src/session-tabs.ts';
test('a closed session that its agent can resume offers Resume; another one offers Reopen', () => {
  const rows = [summary('t_a', { status: 'archived', harness: 'claude_code', nativeAvailable: true }),
    summary('t_b', { status: 'archived', harness: 'opencode', nativeAvailable: true }),
    summary('t_c', { status: 'archived', harness: null, nativeAvailable: null }),
    summary('t_d', { status: 'archived', harness: 'claude_code', nativeAvailable: false })];
  const canResume = (harness: string | null) => harness === 'claude_code' || harness === 'codex';
  const listed = threadSessions(rows, () => 'Claude Code', new Set(), row => row.harness !== null && row.nativeAvailable === true && canResume(row.harness));
  assert.deepEqual(listed.map(item => item.resumable), [true, false, false, false]);
  assert.equal(closedSessionAction(listed[0]).label, 'Resume');
  assert.match(closedSessionAction(listed[0]).title, /resumes/);
  assert.equal(closedSessionAction(listed[1]).label, 'Reopen');
  assert.match(closedSessionAction(listed[1]).title, /nothing starts/);
  // A host that does not say whether a session can resume keeps the old Reopen.
  assert.equal(closedSessionAction(threadSessions(rows, () => 'Codex', new Set())[0]).label, 'Reopen');
});
