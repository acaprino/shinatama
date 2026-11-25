// The picker of projects and the creation of a new one, on a window with no Rust (fake-host.ts): a folder operation is registered before it is
// sent and answered by its own request id, a close while it runs asks Rust to cancel it, roots are checked before they are saved, and a new
// project is created in the root chosen with the revision it was read at.
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { answers as mock, resetDialogsMock } from '../src/mock/dialogs.ts';
import { fakeHost, tick, type Answers } from './fake-host.ts';

afterEach(() => overrideInvoke(null));

function host(extra: Answers = {}) {
  resetDialogsMock();
  const table: Answers = { ...mock, project_recent: [], project_path: 'C:\\work\\shinatama', ...extra };
  return fakeHost(table);
}

test('a project chosen is registered first, sent under the same request id, and then becomes the project of the window', async () => {
  const { app, calls } = host();
  app.ui.open({ kind: 'project-picker' });
  await app.catalog.select('C:\\work\\xiox');
  const names = calls.map(call => call.command);
  assert.ok(names.indexOf('catalog_begin') < names.indexOf('project_select'), 'registered before it was sent');
  const begin = calls.find(call => call.command === 'catalog_begin')!.args!.requestId;
  assert.equal(calls.find(call => call.command === 'project_select')!.args!.requestId, begin);
  assert.equal(app.projects.path, 'C:\\work\\xiox');
  assert.equal(app.ui.dialog, null, 'the picker closes once the project is chosen');
  assert.equal(app.catalog.saving, false);
});

test('a path that is not a place is refused where it was typed, and nothing is sent', async () => {
  const { app, calls } = host();
  await app.catalog.select('work');
  assert.match(app.catalog.error, /absolute folder path/);
  assert.equal(calls.some(call => call.command === 'catalog_begin'), false);
});

test('closing while a folder operation runs asks Rust to cancel it; a cancel that was acknowledged closes, and the work that finishes later is not adopted', async () => {
  let release: (value: unknown) => void = () => {};
  const { app, calls } = host({
    project_select: () => new Promise(resolve => { release = resolve; }),
    catalog_cancel: { status: 'cancelled' },
  });
  app.ui.open({ kind: 'project-picker' });
  const choosing = app.catalog.select('C:\\work\\xiox');
  await tick(5);
  assert.equal(app.catalog.saving, true);
  await app.catalog.cancelOrClose();
  assert.ok(calls.some(call => call.command === 'catalog_cancel'));
  assert.equal(app.ui.dialog, null);
  release({ id: 'p_x', path: 'C:\\work\\xiox', name: 'xiox', last_opened: 1, available: true });
  await choosing;
  assert.notEqual(app.projects.selectedPath, 'C:\\work\\xiox', 'a cancelled choice is not adopted');
});

test('a root is added only as an absolute path that is not there already, becomes the default when none is, and removing one offers Undo', async () => {
  const { app, notices } = host();
  app.ui.open({ kind: 'project-picker' });
  app.catalog.openPicker();
  await tick(20);
  assert.equal(app.catalog.configFresh, true);
  assert.equal(await app.catalog.addRoot('code'), false);
  assert.match(app.catalog.error, /absolute path/);
  assert.equal(await app.catalog.addRoot('c:\\WORK\\'), false);
  assert.match(app.catalog.error, /already in the list/);
  assert.equal(await app.catalog.addRoot('D:\\code'), true);
  assert.deepEqual(app.catalog.config?.roots.map(root => root.path), ['C:\\work', 'D:\\code']);
  await app.catalog.removeRoot(1);
  assert.equal(app.catalog.config?.roots.length, 1);
  assert.match(notices.at(-1)!.text, /D:\\code is no longer a project root/);
  app.catalog.closePicker();
});

test('a new project is created in the root chosen with the revision it was read at, and only a name Windows accepts is sent', async () => {
  const { app, calls } = host();
  app.ui.open({ kind: 'new-project' });
  await app.catalog.openCreation();
  assert.equal(app.catalog.rootId, 'r1', 'the default root is chosen');
  assert.equal(app.catalog.canCreate, false, 'no name yet');
  await app.catalog.create();
  assert.equal(app.catalog.nameError, 'Enter a project folder name.');
  assert.equal(calls.some(call => call.command === 'new_project_create'), false);
  app.catalog.setName('CON');
  assert.match(app.catalog.nameError, /reserves for a device|does not reserve/);
  app.catalog.setName('imago-demo');
  app.catalog.setGit(true);
  assert.equal(app.catalog.destination, 'C:\\work\\imago-demo');
  await app.catalog.create();
  const sent = calls.find(call => call.command === 'new_project_create')!.args!;
  assert.deepEqual(sent.request, { expectedRevision: 1, rootId: 'r1', name: 'imago-demo', gitInit: true });
  assert.equal(app.ui.dialog, null);
  assert.equal(app.projects.path, 'C:\\work\\imago-demo');
});

test('a creation that fails says why in the dialog, with the folder that kept the work, and the dialog stays', async () => {
  const { app } = host({ new_project_create: () => { throw { message: 'The folder already exists.', recoveryPath: 'C:\\work\\x' }; } });
  app.ui.open({ kind: 'new-project' });
  await app.catalog.openCreation();
  app.catalog.setName('x');
  await app.catalog.create();
  assert.match(app.catalog.error, /already exists\. Preserved work: C:\\work\\x/);
  assert.equal(app.ui.dialog?.kind, 'new-project');
  assert.equal(app.catalog.saving, false);
});
