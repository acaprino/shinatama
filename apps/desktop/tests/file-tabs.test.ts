// A file open in a pane ("Open in a pane" of the Files panel) on a window with no Rust: the tab takes the file from the panel with its
// unsaved text, the same file is one tab, it writes against the version that was read, closing asks first, and a tab that was saved reads
// its file again after a restart. The view id of a file holds no path.
import test, { afterEach, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { fileTabLabel, fileViewId, parseSavedFiles } from '../src/files-model.ts';
import { fakeHost } from './fake-host.ts';

const store = new Map<string, string>();
beforeEach(() => {
  store.clear();
  (globalThis as { localStorage?: unknown }).localStorage = { getItem: (key: string) => store.get(key) ?? null, setItem: (key: string, value: string) => { store.set(key, value); }, removeItem: (key: string) => { store.delete(key); } };
});
afterEach(() => overrideInvoke(null));

const project = { id: 'p_aaaaaaaaaaaa', path: 'C:\\work\\shinatama', name: 'shinatama', last_opened: 1, available: true };
const read = (path: string, text: string) => ({ path, text, binary: false, truncated: false, bytes: text.length, sha256: `sha-${text.length}` });
const base = () => ({
  project_recent: [project],
  files_list: { path: '', entries: [{ name: 'retry.ts', path: 'src/retry.ts', kind: 'file', size: 10, linkInside: null }], truncated: false },
  files_read: (args: Record<string, unknown> | undefined) => read(String(args?.path), 'const a = 1;'),
});

test('the view id of a file is a view id: letters, digits and a few signs, the same for the same file and another for another', () => {
  const id = fileViewId('C:\\work\\shinatama', 'src/retry.ts');
  assert.match(id, /^file-[0-9a-f]{8}$/);
  assert.equal(fileViewId('c:\\WORK\\shinatama', 'src/retry.ts'), id, 'a Windows path is not case sensitive');
  assert.notEqual(fileViewId('C:\\work\\shinatama', 'src/charge.ts'), id);
  assert.notEqual(fileViewId('C:\\work\\xiox', 'src/retry.ts'), id);
});

test('the saved files are read as far as they can be trusted, and a tab is named after its file', () => {
  assert.deepEqual(parseSavedFiles('{"file-1":{"project":"C:\\\\p","path":"a/b.ts"},"file-2":{"project":1},"file-3":null}'), { 'file-1': { project: 'C:\\p', path: 'a/b.ts' } });
  assert.deepEqual(parseSavedFiles('[1]'), {});
  assert.deepEqual(parseSavedFiles('not json'), {});
  assert.equal(fileTabLabel('src/retry.ts', false), 'retry.ts');
  assert.equal(fileTabLabel('src/retry.ts', true), 'retry.ts •');
});

test('Open in a pane takes the file, with its unsaved text, out of the panel and into a tab of the workspace', async () => {
  const { app } = fakeHost(base());
  await app.files.start();
  await app.files.openFile('src/retry.ts');
  app.files.edit('const a = 2;');
  app.files.openInPane();
  assert.equal(app.files.current, null, 'the panel no longer holds it');
  assert.equal(app.files.dirty, false, 'so closing the panel asks nothing about it');
  const id = fileViewId(project.path, 'src/retry.ts');
  const view = app.layout.view(id);
  assert.equal(view?.resource.kind, 'files');
  assert.equal(app.fileTabs.tabs.get(id)?.buffer?.text, 'const a = 2;');
  assert.equal(app.fileTabs.label(id), 'retry.ts •');
  assert.equal(app.layout.groupOf(id)?.activeTabId, id, 'it is the tab in sight');
  // The same file again is the same tab.
  app.files.openFile('src/retry.ts').then(() => app.files.openInPane());
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.equal([...app.fileTabs.tabs.keys()].length, 1);
});

test('a tab writes the file against the version that was read, and says how it went', async () => {
  const saves: Record<string, unknown>[] = [];
  const { app } = fakeHost({ ...base(), files_save: (args: Record<string, unknown> | undefined) => { saves.push(args?.request as Record<string, unknown>); return { state: 'saved', sha256: 'sha-new' }; } });
  await app.files.start();
  await app.files.openFile('src/retry.ts');
  app.files.openInPane();
  const id = fileViewId(project.path, 'src/retry.ts');
  app.fileTabs.edit(id, 'const a = 3;');
  await app.fileTabs.save(id);
  assert.equal(saves.length, 1);
  assert.deepEqual({ ...saves[0], text: undefined }, { project: project.path, path: 'src/retry.ts', text: undefined, baseSha256: 'sha-12', overwrite: false });
  assert.equal(app.fileTabs.tabs.get(id)?.line, 'Saved.');
  assert.equal(app.fileTabs.label(id), 'retry.ts');
  await app.fileTabs.save(id);
  assert.equal(saves.length, 1, 'nothing changed: nothing is written');
});

test('closing a tab with unsaved changes asks first, and a no keeps the tab', async () => {
  const { app } = fakeHost(base());
  await app.files.start();
  await app.files.openFile('src/retry.ts');
  app.files.openInPane();
  const id = fileViewId(project.path, 'src/retry.ts');
  app.fileTabs.edit(id, 'changed');
  const closing = app.fileTabs.close(id);
  assert.equal(app.ui.dialog?.kind, 'confirm');
  app.ui.answer(false);
  assert.equal(await closing, false);
  assert.ok(app.layout.view(id), 'the tab stays');
  const again = app.fileTabs.close(id);
  app.ui.answer(true);
  assert.equal(await again, true);
  assert.equal(app.layout.view(id), undefined);
  assert.equal(app.fileTabs.tabs.size, 0);
  assert.deepEqual(parseSavedFiles(store.get('shinatama.files.tabs') ?? null), {}, 'a closed tab is not offered again');
});

test('a tab that was saved reads its file again after a restart, and says why when it cannot', async () => {
  const first = fakeHost(base());
  await first.app.files.start();
  await first.app.files.openFile('src/retry.ts');
  first.app.files.openInPane();
  const id = fileViewId(project.path, 'src/retry.ts');
  // A new window: the store of the page is what is left.
  const second = fakeHost(base());
  assert.equal(second.app.fileTabs.label(id), 'retry.ts');
  await second.app.fileTabs.ensure(id);
  assert.equal(second.app.fileTabs.tabs.get(id)?.state, 'ready');
  assert.equal(second.app.fileTabs.tabs.get(id)?.buffer?.text, 'const a = 1;');
  const third = fakeHost({ ...base(), files_read: () => { throw 'The file is gone.'; } });
  await third.app.fileTabs.ensure(id);
  assert.equal(third.app.fileTabs.tabs.get(id)?.state, 'failed');
  assert.equal(third.app.fileTabs.tabs.get(id)?.line, 'The file is gone.');
  // A view nobody knows stays unknown.
  await third.app.fileTabs.ensure('file-00000000');
  assert.equal(third.app.fileTabs.tabs.has('file-00000000'), false);
});
