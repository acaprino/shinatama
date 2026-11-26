// The files of the project and the files panel's controller, on a window with no Rust: the tree that reads a folder when it opens, the
// search by name, a file read, edited and saved against the version that was read, and what leaving the panel asks.
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { fakeHost, tick } from './fake-host.ts';

afterEach(() => overrideInvoke(null));

const project = { id: 'p_aaaaaaaaaaaa', path: 'C:\\work\\shinatama', name: 'shinatama', last_opened: 1, available: true };
const other = { id: 'p_bbbbbbbbbbbb', path: 'C:\\work\\xiox', name: 'xiox', last_opened: 0, available: true };
const gone = { id: 'p_cccccccccccc', path: 'C:\\work\\gone', name: 'gone', last_opened: 0, available: false };
const entry = (name: string, kind: 'file' | 'directory' = 'file', path = name) => ({ name, path, kind, size: 10, linkInside: null });
const listing = (path: string, entries: unknown[]) => ({ path, entries, truncated: false });
const file = (path: string, text: string, extra: Record<string, unknown> = {}) => ({ path, text, binary: false, truncated: false, bytes: text.length, sha256: `sha-${text.length}`, ...extra });

const base = () => ({
  project_recent: [project, other, gone],
  files_list: (args: Record<string, unknown> | undefined) => (args?.path === '' ? listing('', [entry('src', 'directory'), entry('README.md')]) : listing('src', [entry('retry.ts', 'file', 'src/retry.ts')])),
});

test('the panel reads the projects that are there, starts on the one the window works in, and reads its first folder', async () => {
  const { app, calls } = fakeHost(base());
  app.projects.set('C:\\WORK\\xiox\\');
  await app.files.start();
  assert.deepEqual(app.files.projects.map(one => one.name), ['shinatama', 'xiox'], 'a project that is gone cannot be browsed');
  assert.equal(app.files.project, 'C:\\work\\xiox');
  assert.deepEqual(app.files.nodes.map(node => [node.name, node.hasChildren === true]), [['src', true], ['README.md', false]]);
  assert.deepEqual(calls.find(call => call.command === 'files_list')?.args && { ...calls.find(call => call.command === 'files_list')!.args, requestId: undefined }, { project: 'C:\\work\\xiox', path: '', requestId: undefined });
  await app.files.start();
  assert.equal(calls.filter(call => call.command === 'files_list').length, 1, 'a panel that is open is not read again');
});

test('with no project to browse the panel says so', async () => {
  const { app } = fakeHost({ project_recent: [gone] });
  await app.files.start();
  assert.equal(app.files.project, null);
  assert.match(app.files.problem ?? '', /Open a project first/);
});

test('a folder is read when the tree opens it, and a refusal is said and leaves the folder closed', async () => {
  let refuse = false;
  const { app, calls } = fakeHost({ ...base(), files_list: (args: Record<string, unknown> | undefined) => { if (refuse) throw 'Access is denied.'; return (args?.path === '' ? listing('', [entry('src', 'directory')]) : listing('src', [])); } });
  await app.files.start();
  const [src] = app.files.nodes;
  const children = await app.files.children(src);
  assert.deepEqual(children.map(node => node.name), ['Empty folder']);
  assert.equal(calls.filter(call => call.command === 'files_list').at(-1)?.args?.path, 'src');
  refuse = true;
  await assert.rejects(app.files.children(src));
  assert.equal(app.files.failed, true);
  assert.equal(app.files.status, 'Access is denied.');
});

test('a file opens as text without running it, and a file that is not text is said and not opened', async () => {
  const { app, calls } = fakeHost({
    ...base(), files_read: (args: Record<string, unknown> | undefined) => (args?.path === 'data.bin' ? file('data.bin', '', { text: null, binary: true, bytes: 2048 }) : file(String(args?.path), 'one\ntwo')),
  });
  await app.files.start();
  await app.files.openFile('src/retry.ts');
  assert.equal(app.files.current?.path, 'src/retry.ts');
  assert.equal(app.files.selected, 'src/retry.ts');
  assert.match(app.files.status, /^2 lines · edit and press Save/);
  await app.files.openFile('data.bin');
  assert.equal(app.files.current?.path, 'src/retry.ts', 'the file that was open stays');
  assert.match(app.files.status, /not text \(2,048 bytes\); it is not opened or run/);
  await app.files.openFile('src/retry.ts');
  assert.equal(calls.filter(call => call.command === 'files_read').length, 2, 'a file that is held is not read again (its edits are kept)');
});

test('a picture opens as a picture and is never editable, and a refused one says why', async () => {
  let outcome: unknown = { state: 'ready', image: { path: 'logo.png', width: 4, height: 2, bytes: 9, dataUrl: 'data:image/png;base64,AA==' } };
  const { app } = fakeHost({ ...base(), files_image: () => outcome });
  await app.files.start();
  await app.files.openFile('logo.png');
  assert.equal(app.files.current, null);
  assert.equal(app.files.image?.width, 4);
  assert.match(app.files.status, /4 × 2 pixels, 9 bytes\. Read-only\./);
  outcome = { state: 'refused', message: 'Too large to show.' };
  await app.files.openFile('big.png');
  assert.equal(app.files.image, null);
  assert.equal(app.files.status, 'Too large to show.');
  assert.equal(app.files.failed, true);
});

test('a file that is large is read-only, and a markdown file has a Source and a Preview', async () => {
  const { app } = fakeHost({ ...base(), files_read: (args: Record<string, unknown> | undefined) => file(String(args?.path), '# T', { truncated: args?.path === 'big.ts' }) });
  await app.files.start();
  await app.files.openFile('big.ts');
  assert.equal(app.files.current?.editable, false);
  app.files.edit('changed');
  assert.equal(app.files.current?.text, '# T', 'what cannot be written cannot be edited');
  assert.match(app.files.status, /larger than 4 MiB/);
  await app.files.openFile('README.md');
  app.files.setMode('preview');
  assert.equal(app.files.mode, 'preview');
  await app.files.openFile('src/retry.ts');
  assert.equal(app.files.mode, 'source', 'only Markdown has a Preview');
});

test('an edit is kept until Save, and Save writes it against the version that was read', async () => {
  const { app, calls } = fakeHost({ ...base(), files_read: file('src/retry.ts', 'one'), files_save: { state: 'saved', sha256: 'sha-new' } });
  await app.files.start();
  await app.files.openFile('src/retry.ts');
  assert.equal(app.files.canSave, false);
  app.files.edit('one two');
  assert.equal(app.files.dirty, true);
  assert.equal(app.files.canSave, true);
  await app.files.save();
  const request = calls.find(call => call.command === 'files_save')?.args?.request;
  assert.deepEqual(request, { project: 'C:\\work\\shinatama', path: 'src/retry.ts', text: 'one two', baseSha256: 'sha-3', overwrite: false });
  assert.equal(app.files.status, 'Saved.');
  assert.equal(app.files.dirty, false);
  assert.equal(app.files.current?.baseSha256, 'sha-new');
});

test('a file that changed on disk is replaced only when the person says so, and only the version they saw', async () => {
  const answers: unknown[] = [{ state: 'conflict', disk_sha256: 'sha-disk' }, { state: 'saved', sha256: 'sha-won' }];
  const { app, calls } = fakeHost({ ...base(), files_read: file('src/retry.ts', 'one'), files_save: () => answers.shift() });
  await app.files.start();
  await app.files.openFile('src/retry.ts');
  app.files.edit('mine');
  const saving = app.files.save();
  await tick(2);
  assert.equal(app.ui.dialog?.kind, 'confirm');
  app.ui.answer(false);
  await saving;
  assert.equal(app.files.status, 'Not saved: the file on disk is newer. Your text is still here.');
  assert.equal(app.files.failed, true);
  assert.equal(app.files.dirty, true);
  answers.unshift({ state: 'conflict', disk_sha256: 'sha-disk' });
  const again = app.files.save();
  await tick(2);
  app.ui.answer(true);
  await again;
  const saves = calls.filter(call => call.command === 'files_save').map(call => call.args?.request as { baseSha256: string });
  assert.deepEqual(saves.map(request => request.baseSha256), ['sha-3', 'sha-3', 'sha-disk']);
  assert.equal(saves.at(-1)?.baseSha256, 'sha-disk', 'the replacement is of the disk version that was reviewed');
  assert.equal(app.files.status, 'Saved.');
});

test('a save that fails says why and keeps the text', async () => {
  const { app } = fakeHost({ ...base(), files_read: file('a.ts', 'x'), files_save: { state: 'failed', message: 'The disk is full.' } });
  await app.files.start();
  await app.files.openFile('a.ts');
  app.files.edit('y');
  await app.files.save();
  assert.equal(app.files.status, 'The disk is full.');
  assert.equal(app.files.failed, true);
  assert.equal(app.files.dirty, true);
  assert.equal(app.files.saving, false);
});

test('the search by name waits for the typing to stop, and an answer that is no longer wanted is dropped', async () => {
  const { app, calls } = fakeHost({
    ...base(), files_search: (args: Record<string, unknown> | undefined) => ({ query: args?.query, hits: [entry('retry.ts', 'file', 'src/retry.ts')], truncated: false, cancelled: false, timedOut: false }),
    files_cancel: null,
  });
  await app.files.start();
  app.files.setQuery('re');
  app.files.setQuery('ret');
  await tick(320);
  assert.equal(calls.filter(call => call.command === 'files_search').length, 1, 'one search for the text that was left');
  assert.deepEqual(app.files.results?.map(node => node.name), ['retry.ts']);
  assert.equal(app.files.status, '1 match for “ret”.');
  app.files.clearSearch();
  assert.equal(app.files.results, null);
  assert.equal(app.files.query, '');
  app.files.setQuery('   ');
  await tick(320);
  assert.equal(app.files.results, null, 'a blank search shows the tree');
});

test('another project asks first when something is unsaved, and starts from its own tree', async () => {
  const { app } = fakeHost({ ...base(), files_read: file('a.ts', 'x') });
  await app.files.start();
  await app.files.chooseProject('C:\\work\\xiox');
  assert.equal(app.files.project, 'C:\\work\\xiox');
  await app.files.openFile('a.ts');
  app.files.edit('y');
  const switching = app.files.chooseProject('C:\\work\\shinatama');
  await tick(2);
  assert.equal(app.ui.dialog?.kind === 'confirm' && app.ui.dialog.title, 'Discard unsaved changes?');
  app.ui.answer(false);
  await switching;
  assert.equal(app.files.project, 'C:\\work\\xiox', 'cancel keeps the files open');
  assert.equal(app.files.dirty, true);
  const again = app.files.chooseProject('C:\\work\\shinatama');
  await tick(2);
  app.ui.answer(true);
  await again;
  assert.equal(app.files.project, 'C:\\work\\shinatama');
  assert.equal(app.files.dirty, false);
  assert.equal(app.files.current, null);
});

test('closing the panel with unsaved changes asks, names the files, and goes only when they are given up', async () => {
  const { app } = fakeHost({ ...base(), files_read: file('a.ts', 'x') });
  await app.files.start();
  await app.files.openFile('a.ts');
  app.files.edit('y');
  const closing = app.files.release();
  await tick(2);
  assert.match(app.ui.dialog?.kind === 'confirm' ? app.ui.dialog.description : '', /^a\.ts: Closing discards them/);
  app.ui.answer(false);
  assert.equal(await closing, false);
  assert.equal(app.files.current?.path, 'a.ts');
  const again = app.files.release();
  await tick(2);
  app.ui.answer(true);
  assert.equal(await again, true);
  assert.equal(app.files.project, null, 'nothing of it is kept');
  assert.deepEqual(app.files.nodes, []);
  // A panel with nothing unsaved closes without a question.
  await app.files.start();
  assert.equal(await app.files.release(), true);
  assert.equal(app.ui.dialog, null);
});

test('a page of the project opens in a browser tab with its own folder only', async () => {
  const { app, calls } = fakeHost({ ...base(), files_read: file('web/index.html', '<p>'), browser_open: { id: 'pg1', url: 'file:///x', title: 'x', controller: 'person', live: true, openedBy: null } });
  await app.files.start();
  await app.files.openFile('web/index.html');
  app.files.openInBrowser();
  await tick(5);
  assert.deepEqual(calls.find(call => call.command === 'browser_open')?.args, { file: 'C:\\work\\shinatama\\web\\index.html' });
});
