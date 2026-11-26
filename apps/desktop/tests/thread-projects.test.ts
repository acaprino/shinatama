// "Choose projects" of the thread sidebar: a folder is recorded as a project (the window's own project does not change) and listed in the sidebar,
// or taken away; and a project whose session was just started is listed, so that the thread that session is in has its row.
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { keepOrder } from '../src/project-catalog.ts';
import { fakeHost } from './fake-host.ts';

afterEach(() => overrideInvoke(null));

const entry = { id: 'p_aaaaaaaaaaaa', path: 'C:\\work\\shinatama', name: 'shinatama', last_opened: 1, available: true };
const answers = () => {
  let state: Record<string, unknown> = { version: 1, width: 236, collapsed: false };
  let revision = 0;
  return {
    project_select: entry,
    project_recent: [entry],
    thread_sidebar_load: () => ({ revision, settings: state }),
    thread_sidebar_save: (args: Record<string, unknown> | undefined) => { state = (args?.request as { settings: Record<string, unknown> }).settings; revision += 1; return { revision, settings: state }; },
    workset_list: { rows: [], more: false },
  };
};

test('a press records the folder and lists it in the sidebar, and a second press takes it away', async () => {
  const { app, calls } = fakeHost(answers());
  await app.threads.loadSidebar();
  assert.deepEqual(app.threads.activeProjectIds(), []);
  await app.threads.toggleProjectAt(entry.path);
  assert.deepEqual(app.threads.activeProjectIds(), [entry.id]);
  assert.equal(calls.filter(call => call.command === 'project_select').length, 1, 'it is recorded as a project first');
  assert.equal(app.projects.path, '', 'and the window\'s own project is not changed');
  await app.threads.toggleProjectAt(entry.path);
  assert.deepEqual(app.threads.activeProjectIds(), []);
});

test('a press while another is saved is not taken, so the two do not save the sidebar from the same old state', async () => {
  const { app, calls } = fakeHost(answers());
  await app.threads.loadSidebar();
  const first = app.threads.toggleProjectAt(entry.path);
  assert.equal(app.threads.toggling, true);
  await app.threads.toggleProjectAt(entry.path);
  await first;
  assert.deepEqual(app.threads.activeProjectIds(), [entry.id], 'listed once, not taken away by the second press');
  assert.equal(calls.filter(call => call.command === 'project_select').length, 1);
  assert.equal(app.threads.toggling, false);
});

test('the rows keep the place they were first shown in when Rust puts the one just pressed first, and new rows come after them', () => {
  const seen = new Map<string, number>();
  const row = (path: string) => ({ path });
  assert.deepEqual(keepOrder([row('a'), row('b'), row('c')], seen).map(r => r.path), ['a', 'b', 'c']);
  assert.deepEqual(keepOrder([row('c'), row('a'), row('b')], seen).map(r => r.path), ['a', 'b', 'c'], 'c was pressed: it stays third');
  assert.deepEqual(keepOrder([row('d'), row('c'), row('a'), row('b')], seen).map(r => r.path), ['a', 'b', 'c', 'd'], 'a new folder goes last');
  assert.deepEqual(keepOrder([row('b'), row('d')], seen).map(r => r.path), ['b', 'd'], 'a filter keeps the order of what is left');
});

test('a project whose session was started is listed once, and a refusal of Rust is said', async () => {
  const { app, notices } = fakeHost(answers());
  await app.threads.loadSidebar();
  await app.threads.listProject(entry.id);
  await app.threads.listProject(entry.id);
  assert.deepEqual(app.threads.activeProjectIds(), [entry.id], 'listed, not toggled away by the second call');
  const refused = fakeHost({ ...answers(), project_select: () => { throw 'The folder is gone.'; } });
  await refused.app.threads.loadSidebar();
  await refused.app.threads.toggleProjectAt('C:\\gone');
  assert.deepEqual(refused.notices, [{ text: 'The folder is gone.', error: true }]);
  void notices;
});
