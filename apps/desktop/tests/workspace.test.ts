// The controllers of the workspace on a window with no Rust (fake-host.ts): the thread sidebar's operations, the conversation of a thread,
// the search and the links of a terminal, and the window's report of its own measures.
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { fakeHost, row, tick, WORKSET } from './fake-host.ts';

afterEach(() => overrideInvoke(null));

const summary = (id: string, extra: Record<string, unknown> = {}) => ({
  id, projectId: WORKSET.projectId, title: null, archived: false, revision: 1, usedAt: 1, sessions: 1, open: 0, lead: null, ...extra,
});
const info = (id: string) => ({ id, cwd: 'C:\\work\\shinatama', shell: 'claude_code' });

test('a new thread is turned on at once, so its first session has a place', async () => {
  const { app, calls } = fakeHost({
    workset_create: { id: 'w_bbbbbbbbbbbb', revision: 1 },
    workset_list: { rows: [summary('w_bbbbbbbbbbbb')], more: false },
    workset_threads: [],
  });
  await app.threads.create(WORKSET.projectId);
  assert.deepEqual(app.threads.openWorksets.map(workset => workset.id), ['w_bbbbbbbbbbbb']);
  assert.equal(app.threads.lists.get(WORKSET.projectId)?.length, 1);
  assert.ok(calls.some(call => call.command === 'workset_create'));
});

test('a click on a thread turns it on, and off when it is on; one operation at a time', async () => {
  const { app } = fakeHost({ workset_threads: [row('t_one', { status: 'archived' })] });
  const first = app.threads.toggleOn(WORKSET);
  assert.equal(app.threads.busy, true);
  // The sidebar's controls wait: a second click while the first is under way does nothing.
  await app.threads.toggleOn(WORKSET);
  await first;
  assert.equal(app.threads.busy, false);
  assert.equal(app.threads.isOn(WORKSET.id), true);
  await app.threads.toggleOn(WORKSET);
  assert.equal(app.threads.isOn(WORKSET.id), false);
});

test('a thread is deleted only after the person says so, and a refusal is said in the words of the store', async () => {
  const answers = { workset_threads: [], workset_delete: null, workset_list: { rows: [], more: false } };
  const { app, calls, notices } = fakeHost(answers);
  const thread = summary(WORKSET.id, { sessions: 3, title: 'Billing' });
  const asked = app.threads.deleteThread(thread);
  assert.equal(app.ui.dialog?.kind, 'confirm');
  if (app.ui.dialog?.kind === 'confirm') {
    assert.equal(app.ui.dialog.title, 'Delete the thread Billing?');
    assert.equal(app.ui.dialog.confirm, 'Delete thread', 'the confirming button carries the verb and the object');
    assert.match(app.ui.dialog.description, /3 sessions/);
  }
  app.ui.answer(false);
  await asked;
  assert.equal(calls.some(call => call.command === 'workset_delete'), false, 'a no deletes nothing');
  const again = app.threads.deleteThread(thread);
  app.ui.answer(true);
  await again;
  assert.ok(calls.some(call => call.command === 'workset_delete'));
  assert.deepEqual(notices, []);
  // The store refuses: a session of the thread runs.
  overrideInvoke(async command => { if (command === 'workset_threads') return []; throw { code: 'conflict' }; });
  const refused = app.threads.deleteThread(thread);
  app.ui.answer(true);
  await refused;
  assert.match(notices.at(-1)?.text ?? '', /changed\. Reload and try again/);
  assert.equal(notices.at(-1)?.error, true);
});

test('archived threads are listed on request, for every project the sidebar lists', async () => {
  const asked: unknown[] = [];
  const { app } = fakeHost({
    thread_sidebar_load: { revision: 1, settings: { version: 1, width: 236, collapsed: false, activeProjects: ['p_aaaaaaaaaaaa', 'p_bbbbbbbbbbbb'] } },
    workset_list: (args) => { asked.push(args); return { rows: [], more: false }; },
  });
  await app.threads.loadSidebar();
  await app.threads.setShowArchived(true);
  assert.deepEqual(asked.map(args => (args as { request: unknown }).request), [
    { projectId: 'p_aaaaaaaaaaaa', archived: true }, { projectId: 'p_bbbbbbbbbbbb', archived: true },
  ]);
  assert.equal(app.threads.showArchived, true);
  await app.threads.refreshList('p_aaaaaaaaaaaa');
  assert.deepEqual((asked.at(-1) as { request: unknown }).request, { projectId: 'p_aaaaaaaaaaaa', archived: true }, 'a read of the list follows the choice');
});

test('the names agents gave to unnamed threads are asked for once a minute, and never for one that has a name', async () => {
  const lead = (threadId: string, title: string | null) => ({ threadId, title, harness: 'claude_code' });
  const named: string[][] = [];
  const { app } = fakeHost({
    workset_list: { rows: [
      summary('w_aaaaaaaaaaaa', { lead: lead('t_aaaaaaaaaaaa', 'Claude Code') }),
      summary('w_bbbbbbbbbbbb', { title: 'Mine', lead: lead('t_bbbbbbbbbbbb', 'Claude Code') }),
      summary('w_cccccccccccc', { lead: lead('t_cccccccccccc', 'Fix the retry') }),
    ], more: false },
    harness_catalog: [{ id: 'claude_code', label: 'Claude Code', nativeResume: true }],
    thread_names: (args) => { named.push((args as { threadIds: string[] }).threadIds); return [{ threadId: 't_aaaaaaaaaaaa', name: 'Refactor billing' }]; },
  });
  await app.opens.loadHarnessCatalog();
  await app.threads.refreshList(WORKSET.projectId);
  await app.threads.requestNames(1_000_000);
  assert.deepEqual(named, [['t_aaaaaaaaaaaa']]);
  assert.equal(app.threads.title('w_aaaaaaaaaaaa'), 'Refactor billing');
  assert.equal(app.threads.title('w_bbbbbbbbbbbb'), 'Mine');
  assert.equal(app.threads.title('w_cccccccccccc'), 'Fix the retry');
  await app.threads.requestNames(1_000_100);
  assert.equal(named.length, 1, 'the name it has is not asked for again');
});

test('every kind of tab is called by what it holds', async () => {
  const { app } = fakeHost({
    workset_list: { rows: [summary(WORKSET.id, { title: 'Billing' })], more: false },
    workset_threads: [row('t_one')],
  });
  await app.threads.refreshList(WORKSET.projectId);
  await app.threads.showWorkset(WORKSET);
  const view = (id: string, kind: 'chat' | 'team' | 'files') => ({ id, resource: { kind, resourceId: id }, hidden: false } as const);
  assert.equal(app.threads.tabTitle(view(`chat-${WORKSET.id}`, 'chat')), 'Conversation · Billing');
  assert.equal(app.threads.tabTitle(view(`team-${WORKSET.id}`, 'team')), 'Team map · Billing');
  assert.equal(app.threads.tabTitle(view('chat-w_zzzzzzzzzzzz', 'chat')), 'Conversation · Thread');
  assert.equal(app.threads.tabTitle(view('files-1', 'files')), 'Files');
  const saved = app.layout.layout.views[0];
  assert.match(app.threads.tabTitle(saved), /not running/);
});

test('the conversation reads its room while it is on screen, merges what it reads, and says when a read failed', async () => {
  let reads = 0;
  const events = [
    { eventId: '1', timestamp: '2026-11-25T10:00:00.000Z', kind: 'message', actor: 'claude-1', actorHost: false, recipients: [], text: 'one', signal: null, reason: null },
    { eventId: '2', timestamp: '2026-11-25T10:01:00.000Z', kind: 'message', actor: 'codex-1', actorHost: false, recipients: [], text: 'two', signal: null, reason: null },
  ];
  const { app, calls } = fakeHost({
    thread_chat_snapshot: () => {
      reads += 1;
      if (reads === 3) throw 'The room is unreachable.';
      return { enabled: true, members: [], sessions: [], events: reads === 1 ? [events[0]] : events };
    },
  });
  const stop = app.chat.watch(WORKSET.id);
  assert.equal(app.chat.reading, WORKSET.id, 'its news is read as it comes, so it is never unread');
  assert.equal(app.chat.room(WORKSET.id).loading, true);
  await tick(5);
  assert.deepEqual(app.chat.room(WORKSET.id).events.map(event => event.eventId), ['1']);
  assert.equal(app.chat.room(WORKSET.id).loading, false);
  await app.chat.readRoom(WORKSET.id);
  assert.deepEqual(app.chat.room(WORKSET.id).events.map(event => event.eventId), ['1', '2']);
  // The second read asked only for what came after the first line.
  assert.equal((calls.filter(call => call.command === 'thread_chat_snapshot')[1].args as { request: { since: string } }).request.since, events[0].timestamp);
  // A read that fails with a room on screen stays a line of status.
  await app.chat.readRoom(WORKSET.id);
  assert.deepEqual(app.chat.room(WORKSET.id).status, { text: 'The room is unreachable.', error: true });
  assert.equal(app.chat.room(WORKSET.id).failure, null);
  assert.equal(app.chat.room(WORKSET.id).events.length, 2);
  stop();
  assert.equal(app.chat.reading, null);
  const before = reads;
  await app.chat.readRoom(WORKSET.id);
  assert.equal(reads, before, 'a room that is not on screen is not read');
  app.chat.dispose();
});

test('a room that cannot be read at first offers Retry, and a thread that does not talk says so', async () => {
  let enabled = false;
  const { app } = fakeHost({
    thread_chat_snapshot: () => { if (!enabled) { enabled = true; throw 'No room.'; } return { enabled: false }; },
  });
  const stop = app.chat.watch(WORKSET.id);
  await tick(5);
  assert.equal(app.chat.room(WORKSET.id).failure, 'No room.');
  app.chat.retryRoom(WORKSET.id);
  await tick(5);
  assert.equal(app.chat.room(WORKSET.id).failure, null);
  assert.match(app.chat.room(WORKSET.id).status.text, /do not talk to each other/);
  stop();
  app.chat.dispose();
});

test('a message goes to the recipient chosen, the room is read again, and one that is too long or refused stays unsent', async () => {
  const sent: unknown[] = [];
  const { app } = fakeHost({
    thread_chat_snapshot: { enabled: true, members: [], sessions: [], events: [] },
    thread_chat_send: (args) => { sent.push((args as { request: unknown }).request); return null; },
  });
  const stop = app.chat.watch(WORKSET.id);
  await tick(5);
  assert.equal(await app.chat.send(WORKSET.id, 'codex-1', 'review it'), true);
  assert.deepEqual(sent[0], { worksetId: WORKSET.id, recipients: ['codex-1'], text: 'review it' });
  assert.equal(app.chat.room(WORKSET.id).status.text, 'Sent to codex-1.');
  assert.equal(await app.chat.send(WORKSET.id, '', 'to all'), true);
  assert.deepEqual((sent[1] as { recipients: string[] }).recipients, [], 'the whole room wakes no one');
  assert.equal(await app.chat.send(WORKSET.id, '', '   '), false);
  assert.equal(await app.chat.send(WORKSET.id, '', 'x'.repeat(17 * 1024)), false);
  assert.match(app.chat.room(WORKSET.id).status.text, /at most 16 KB/);
  assert.equal(sent.length, 2);
  overrideInvoke(async () => { throw 'The room is closed.'; });
  assert.equal(await app.chat.send(WORKSET.id, '', 'again'), false);
  assert.deepEqual(app.chat.room(WORKSET.id).status, { text: 'The room is closed.', error: true });
  assert.equal(app.chat.room(WORKSET.id).sending, false);
  stop();
  app.chat.dispose();
});

test('a link in a terminal opens a page; one that is not a page says why', async () => {
  const { app, calls, notices } = fakeHost({ browser_open: { id: 'page1', url: 'https://example.com/', title: 'Example', controller: 'user', live: true, openedBy: null }, workset_threads: [] });
  await app.terminals.attach(info('i_aaaaaaaaaaaa'), app.layout.focusedGroup(), null, null);
  app.terminals.openLink('i_aaaaaaaaaaaa', 'ftp://host/file');
  assert.match(notices.at(-1)?.text ?? '', /ftp: links do not open in Shinatama/);
  app.terminals.openLink('i_aaaaaaaaaaaa', 'notes.txt');
  assert.match(notices.at(-1)?.text ?? '', /Not a link Shinatama opens/);
  assert.equal(calls.some(call => call.command === 'browser_open'), false);
  app.terminals.dispose();
});

test('the search of a terminal opens on the one in the pane the person works in, and says when there is none', async () => {
  const { app, notices } = fakeHost({ workset_threads: [], terminal_read: () => new Promise(() => {}) });
  app.terminals.openFind();
  assert.match(notices.at(-1)?.text ?? '', /Open or select a terminal/);
  assert.equal(app.terminals.finding, null);
  await app.terminals.attach(info('i_aaaaaaaaaaaa'), app.layout.focusedGroup(), null, null);
  app.terminals.openFind();
  assert.equal(app.terminals.finding, 'i_aaaaaaaaaaaa');
  app.terminals.closeFind();
  assert.equal(app.terminals.finding, null);
  // A terminal taken away takes its search with it.
  app.terminals.openFind('i_aaaaaaaaaaaa');
  app.terminals.discard('i_aaaaaaaaaaaa');
  assert.equal(app.terminals.finding, null);
});

test('the window reports its measures, learns what the backend makes of them, and keeps the last answer when a report fails', async () => {
  let fail = false;
  const { app, calls } = fakeHost({
    resources_telemetry: () => {
      if (fail) throw 'busy';
      return { accepted: true, policy: { stage: 'normal', reason: 'observed', reduce_optional: false, defer_new_starts: false }, resources: { owned_private_commit_bytes: 1, system_remaining_commit_bytes: 2, observed_processes: 3 } };
    },
  });
  const sample = { input_latency_us: 1200, frame_time_us: 16_000, pending_input_events: null, pending_output_bytes: null, queued_launches: null } as const;
  assert.equal(app.terminals.health, null);
  assert.notEqual(await app.terminals.reportTelemetry(sample), null);
  assert.equal(app.terminals.health?.policy?.stage, 'normal');
  const sent = (calls.at(-1)!.args as { telemetry: { ipc_roundtrip_us: number | null; input_latency_us: number } }).telemetry;
  assert.equal(sent.ipc_roundtrip_us, null, 'the round trip of the report before is what is sent: none yet');
  assert.equal(sent.input_latency_us, 1200);
  fail = true;
  assert.equal(await app.terminals.reportTelemetry(sample), null);
  assert.equal(app.terminals.health?.policy?.stage, 'normal');
});

test('the threads that were on are read when the start asks, not before: a terminal reattached first does not overwrite them', async () => {
  const saved = new Map<string, string>([['shinatama.threads', JSON.stringify([WORKSET])]]);
  const store = {
    getItem: (key: string) => saved.get(key) ?? null, setItem: (key: string, value: string) => { saved.set(key, value); }, removeItem: (key: string) => { saved.delete(key); },
  };
  Object.assign(globalThis, { localStorage: store, sessionStorage: { ...store, getItem: () => null } });
  try {
    const { app } = fakeHost({ workset_threads: [row('t_one', { status: 'archived' })], terminal_read: () => new Promise(() => {}) });
    // A running terminal is attached while no thread is on yet: it remembers its sessions, and must leave the saved threads alone.
    await app.terminals.attach(info('i_aaaaaaaaaaaa'), app.layout.focusedGroup(), null, null);
    assert.deepEqual(JSON.parse(saved.get('shinatama.threads')!), [WORKSET]);
    await app.threads.reopenLast();
    assert.deepEqual(app.threads.openWorksets, [WORKSET]);
    assert.deepEqual(JSON.parse(saved.get('shinatama.threads')!), [WORKSET], 'and they are kept once they are on');
  } finally {
    Reflect.deleteProperty(globalThis, 'localStorage');
    Reflect.deleteProperty(globalThis, 'sessionStorage');
  }
});

test('a tab has the colour of its thread while the thread is on: a session\'s, its conversation\'s and its team map\'s', async () => {
  const { app } = fakeHost({ workset_threads: [row('t_one')] });
  const colour = (id: string) => app.threads.tabTone(id);
  assert.equal(colour(`chat-${WORKSET.id}`), null, 'a thread that is off has none');
  await app.threads.showWorkset(WORKSET);
  assert.equal(colour(`chat-${WORKSET.id}`)?.startsWith('#'), true);
  assert.equal(colour(`team-${WORKSET.id}`), colour(`chat-${WORKSET.id}`));
  assert.equal(colour('saved-t_one'), colour(`chat-${WORKSET.id}`));
  assert.equal(colour('files-1'), null);
});
