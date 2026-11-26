// The settings of one thread and what the leftovers panel reads, on a window with no Rust.
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { fakeHost, row, tick, WORKSET } from './fake-host.ts';

afterEach(() => overrideInvoke(null));

const summary = (extra: Record<string, unknown> = {}) => ({ id: WORKSET.id, projectId: WORKSET.projectId, title: 'Billing rewrite', archived: false, revision: 3, usedAt: 1, sessions: 2, open: 0, lead: null, ...extra });
const answers = (extra: Record<string, unknown> = {}) => ({
  workset_list: { rows: [summary()], more: false },
  workset_threads: [row('t_one'), row('t_two', { status: 'archived' })],
  workset_update: null,
  thread_chat_list: [],
  workset_of: WORKSET,
  ...extra,
});

async function shown(extra: Record<string, unknown> = {}) {
  const host = fakeHost(answers(extra));
  await host.app.threads.refreshList(WORKSET.projectId);
  host.app.threadSettings.show(WORKSET.id);
  await tick(5);
  return host;
}

test('the settings of a thread list its sessions, the open ones apart from the closed ones', async () => {
  const { app } = await shown();
  const settings = app.threadSettings;
  assert.equal(settings.title, 'Billing rewrite');
  assert.deepEqual(settings.sessions?.map(session => [session.id, session.state]), [['t_one', 'saved'], ['t_two', 'closed']]);
  settings.forget();
  assert.equal(settings.sessions, null);
  assert.equal(settings.thread, undefined);
});

test('a thread is renamed and archived against the revision that was read, and a name that cannot be kept is not sent', async () => {
  const { app, calls } = await shown();
  assert.equal(await app.threadSettings.rename('   '), false);
  assert.equal(await app.threadSettings.rename('  New name '), true);
  assert.deepEqual(calls.find(call => call.command === 'workset_update')?.args?.request, { worksetId: WORKSET.id, expectedRevision: 3, title: 'New name' });
  await app.threadSettings.toggleArchived();
  assert.deepEqual(calls.filter(call => call.command === 'workset_update').at(-1)?.args?.request, { worksetId: WORKSET.id, expectedRevision: 3, archived: true });
});

test('a refused operation says why in words and in a notice, and the next one can be tried', async () => {
  let fail: unknown = { code: 'conflict' };
  const { app, notices } = await shown({ workset_update: () => { if (fail) throw fail; return null; } });
  assert.equal(await app.threadSettings.rename('x'), false);
  assert.equal(app.threadSettings.failed, true);
  assert.equal(app.threadSettings.message, 'This thread changed. Reload and try again.');
  assert.equal(notices.at(-1)?.error, true);
  assert.equal(app.threadSettings.busy, false);
  fail = null;
  assert.equal(await app.threadSettings.rename('x'), true);
  assert.equal(app.threadSettings.failed, false);
});

test('a thread is deleted only after a question that names it, and its panel goes with it', async () => {
  const { app, calls } = await shown({ workset_delete: null, thread_forget: null, workset_list: { rows: [summary()], more: false } });
  app.ui.openPanel({ kind: 'thread', threadId: WORKSET.id });
  const deleting = app.threadSettings.remove();
  await tick(2);
  assert.equal(app.ui.dialog?.kind === 'confirm' && app.ui.dialog.title, 'Delete the thread Billing rewrite?');
  assert.match(app.ui.dialog?.kind === 'confirm' ? app.ui.dialog.description : '', /with its 2 sessions, closed ones included/);
  app.ui.answer(false);
  assert.equal(await deleting, false);
  assert.equal(calls.some(call => call.command === 'workset_delete'), false);
  const again = app.threadSettings.remove();
  await tick(2);
  app.ui.answer(true);
  await again;
  assert.ok(calls.some(call => call.command === 'workset_delete'), 'the thread is deleted');
});

test('a closed session is deleted only after asking, and one with nothing saved is removed without', async () => {
  const { app, calls } = await shown({ thread_forget: null });
  const closed = app.threadSettings.sessions!.find(session => session.state === 'closed')!;
  const asking = app.threadSettings.forgetSession(closed);
  await tick(2);
  assert.equal(app.ui.dialog?.kind === 'confirm' && app.ui.dialog.confirm, `Delete session ${closed.name}`);
  app.ui.answer(false);
  await asking;
  assert.equal(calls.some(call => call.command === 'thread_forget'), false);
  const unbound = { ...closed, state: 'unbound' as const, id: 't_one' };
  await app.threadSettings.forgetSession(unbound);
  assert.equal(app.ui.dialog, null, 'nothing was saved in it: no question');
  assert.equal(calls.filter(call => call.command === 'thread_forget').length, 1);
});

// --- what agents left on disk --------------------------------------------------------------------------------------------------

const picture = { available: true, reason: null, projects: [{
  folders: ['C:\\w'], liveSessions: 1, suspects: {}, phase: 'watching', root: 'C:\\w', name: 'w', isRepo: true, attention: ['growth'],
  newItems: [
    { key: 'a', category: 'worktrees', path: 'C:\\w-wt', label: null, bytes: 10, bytesPartial: false, level: 'recommended', blocked: [], reason: 'r', firstSeenMs: 1, sessions: [] },
    { key: 'b', category: 'sessions', path: null, label: 'x', bytes: null, bytesPartial: false, level: 'review', blocked: [], reason: 'r', firstSeenMs: 2, sessions: [] },
  ],
  newItemsOmitted: 0, growthBytes: 0, stale: false, cleanCommand: null, cleanCommandTruncated: false, startedAtMs: 1, checkedAtMs: 1, message: null,
}] };

test('the leftovers are read, counted without the archived sessions every session leaves, and an action says what it did', async () => {
  let answer: unknown = picture;
  const { app, calls } = fakeHost({ project_leftovers: () => { if (answer instanceof Error) throw answer.message; return answer; }, project_leftovers_check: null, leftovers_reveal: () => { throw 'Explorer is not available.'; } });
  assert.equal(app.leftovers.loaded, false);
  await app.leftovers.refresh();
  assert.equal(app.leftovers.loaded, true);
  assert.equal(app.leftovers.count, 1);
  assert.equal(app.leftovers.attention, true);
  await app.leftovers.check('C:\\w', 'w');
  assert.deepEqual(calls.find(call => call.command === 'project_leftovers_check')?.args, { path: 'C:\\w' });
  assert.deepEqual(app.leftovers.note, { text: 'Xiox is checking w…', error: false });
  await app.leftovers.reveal('C:\\w-wt');
  assert.deepEqual(app.leftovers.note, { text: 'Explorer is not available.', error: true });
  answer = new Error('Xiox is not answering.');
  await app.leftovers.refresh();
  assert.equal(app.leftovers.readFailure, 'Xiox is not answering.');
  assert.equal(app.leftovers.count, 1, 'the last picture is kept');
  app.leftovers.dispose();
});
