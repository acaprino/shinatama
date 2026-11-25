// The controllers, on a window with no Rust (fake-host.ts): what a thread on, a terminal that ends and a layout that is saved do to one another.
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { fakeHost, row, tick, WORKSET } from './fake-host.ts';

afterEach(() => overrideInvoke(null));

const info = (id: string) => ({ id, cwd: 'C:\\work\\shinatama', shell: 'claude_code' });

test('a thread that is turned on brings its sessions into the layout as tabs that do not run, and off takes them away', async () => {
  const { app } = fakeHost({
    workset_threads: [row('t_one'), row('t_two', { status: 'archived' })],
    terminal_stop: null,
  });
  assert.equal(await app.threads.showWorkset(WORKSET), true);
  assert.deepEqual(app.threads.openWorksets, [WORKSET]);
  // The closed session has no tab; the open one has, and says it is not running.
  assert.deepEqual(app.layout.layout.views.map(view => view.id), ['saved-t_one']);
  assert.match(app.threads.tabLabel('saved-t_one'), /not running/);
  assert.equal(app.threads.toneOf(WORKSET.id)?.startsWith('#'), true, 'a thread that is on has one of Sigrid\'s colours');
  assert.equal(await app.threads.hideWorkset(WORKSET.id), true);
  assert.deepEqual(app.layout.layout.views, []);
  assert.equal(app.threads.toneOf(WORKSET.id), null);
});

test('a terminal is drawn by the screen that connects to it: what it printed before is written first, then the rest, and its end is saved in the layout', async () => {
  const batches = [
    { chunks: [[104, 105, 10]], eof: false, exit_code: null, error: null },
    { chunks: [[111, 107]], eof: true, exit_code: 3, error: null },
  ];
  const { app } = fakeHost({ workset_threads: [], terminal_read: () => batches.shift() });
  const written: string[] = [];
  const pipe = { write: (data: Uint8Array, done: () => void) => { written.push(new TextDecoder().decode(data)); done(); } };
  await app.terminals.attach(info('i_aaaaaaaaaaaa'), app.layout.focusedGroup(), null, null);
  await tick(5);
  // No screen yet: the first part was held; the screen connects and gets it, then what follows.
  const disconnect = app.terminals.connect('i_aaaaaaaaaaaa', pipe);
  await tick(80);
  assert.deepEqual(written.join(''), 'hiok'.replace('hi', 'hi\n'));
  const session = app.terminals.get('i_aaaaaaaaaaaa')!;
  assert.equal(session.ended, true);
  assert.equal(session.exitCode, 3);
  // A terminal that ended by itself is saved as not running: its tab no longer carries its instance.
  const view = app.layout.view('i_aaaaaaaaaaaa');
  assert.equal(view?.resource.kind === 'terminal' ? view.resource.instanceId : 'none', null);
  disconnect();
});

test('what the person types goes to the terminal, and a size is sent once, one resize at a time', async () => {
  const { app, calls } = fakeHost({ workset_threads: [], terminal_read: () => new Promise(() => {}), terminal_write: null, terminal_resize: null });
  await app.terminals.attach(info('i_bbbbbbbbbbbb'), app.layout.focusedGroup(), null, null);
  app.terminals.input('i_bbbbbbbbbbbb', 'ls\r');
  app.terminals.resize('i_bbbbbbbbbbbb', 100, 30);
  app.terminals.resize('i_bbbbbbbbbbbb', 100, 30);
  app.terminals.resize('i_bbbbbbbbbbbb', 1, 1);
  await tick(5);
  const write = calls.find(call => call.command === 'terminal_write');
  assert.deepEqual(write?.args, { id: 'i_bbbbbbbbbbbb', data: [108, 115, 13] });
  assert.equal(calls.filter(call => call.command === 'terminal_resize').length, 1);
  assert.deepEqual(calls.find(call => call.command === 'terminal_resize')?.args, { id: 'i_bbbbbbbbbbbb', rows: 30, cols: 100 });
});

test('a question that is asked while another is open is a no; a dialog that is closed unanswered is a no', async () => {
  const { app } = fakeHost();
  const first = app.ui.ask({ title: 'Stop?', description: 'It stops.', confirm: 'Stop' });
  assert.equal(await app.ui.ask({ title: 'Again?', description: 'Again.', confirm: 'Again' }), false);
  app.ui.closeDialog();
  assert.equal(await first, false);
  const second = app.ui.ask({ title: 'Stop?', description: 'It stops.', confirm: 'Stop' });
  app.ui.answer(true);
  assert.equal(await second, true);
});

test('what asks for the page while a dialog is open waits until the person has answered', () => {
  const { app } = fakeHost();
  const order: string[] = [];
  void app.ui.ask({ title: 'Stop?', description: 'It stops.', confirm: 'Stop' });
  app.ui.afterDialog(() => order.push('focus a tab'));
  order.push('asked');
  app.ui.answer(false);
  assert.deepEqual(order, ['asked', 'focus a tab']);
  app.ui.afterDialog(() => order.push('at once'));
  assert.equal(order.at(-1), 'at once');
});

test('the inspector is one stack: a panel of the same kind is replaced, another covers the one before and gives it back', () => {
  const { app } = fakeHost();
  app.ui.openPanel({ kind: 'files' });
  app.ui.openPanel({ kind: 'session', root: 'r', nativeId: 'one' });
  app.ui.openPanel({ kind: 'session', root: 'r', nativeId: 'two' });
  assert.deepEqual(app.ui.panels.map(panel => panel.kind), ['files', 'session']);
  assert.equal(app.ui.panel?.kind === 'session' && app.ui.panel.nativeId, 'two');
  app.ui.closePanel();
  assert.equal(app.ui.panel?.kind, 'files');
});

test('a screen that reads a controller is told when it changes, and only then', () => {
  const { app } = fakeHost();
  let told = 0;
  const off = app.ui.subscribe(() => { told += 1; });
  const before = app.ui.snapshot();
  app.ui.setView('sessions');
  app.ui.setView('sessions');
  assert.equal(told, 1);
  assert.notEqual(app.ui.snapshot(), before);
  off();
  app.ui.setView('workspace');
  assert.equal(told, 1);
});

test('the layout of the window follows the person: a tab brought forward takes its pane and the focus', async () => {
  const { app } = fakeHost({ workset_threads: [row('t_one'), row('t_two')] });
  await app.threads.showWorkset(WORKSET);
  assert.equal(app.layout.layout.views.length, 2);
  app.layout.reveal('saved-t_two');
  assert.equal(app.layout.activeView()?.id, 'saved-t_two');
  // A tab the tree says moved to a pane of its own is a split of the layout.
  const tree = app.layout.tree(view => ({ title: view.id }));
  assert.equal(tree.kind, 'tabs');
});
