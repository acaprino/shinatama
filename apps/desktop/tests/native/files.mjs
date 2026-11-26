// The project files on the built exe, Windows only (C41, C42): the Files panel lists the real folder, a Markdown file is edited and written to
// disk only by Save, checked against the version that was read (a file an agent changed meanwhile asks before it is replaced), opening a script
// never runs it, a picture shows its size, and "Open in a pane" moves a file to a tab of the workspace that saves the same way.
// Containment (a junction out of the project, `..`, drive paths), the inert Markdown and the bounds of a picture are Rust's, proved by the tests of
// crates/files (navigator, markdown, preview, buffers); this probe proves the page reaches them. Roles and names only.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/files.mjs
// It stands for legacy files-smoke.
import { existsSync } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { check, clearNotices, command, finish, fixtureFolder, openApp, until } from './lib.mjs';

const fixture = await fixtureFolder('files');
const { project } = fixture;
await mkdir(resolve(project, 'docs'), { recursive: true });
await writeFile(resolve(project, 'README.md'), '# Hello\n\nSee the guide.\n');
await writeFile(resolve(project, 'docs', 'guide.md'), '## Guide\n');
await writeFile(resolve(project, 'docs', 'dot.png'), Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==', 'base64'));
const marker = resolve(fixture.folder, 'ran.txt');
await writeFile(resolve(project, 'run.bat'), `@echo ran > "${marker}"\n`);
const onDisk = (name) => readFile(resolve(project, name), 'utf8');

let handle;
try {
  handle = await openApp(fixture);
  const { page, ipc } = handle;
  await ipc('project_select', { path: project });
  await clearNotices(page);
  await command(page, 'Project files');
  const panel = page.getByRole('complementary', { name: 'Project files' });
  const tree = panel.getByRole('tree', { name: 'Folder tree' });
  await tree.waitFor();
  check('the real folder is listed', await tree.getByRole('treeitem', { name: /^README\.md/ }).isVisible() && await tree.getByRole('treeitem', { name: /^docs/ }).isVisible());

  // A script is opened as text and never run.
  await tree.getByRole('treeitem', { name: /^run\.bat/ }).click();
  await panel.getByRole('textbox', { name: 'Source of run.bat' }).waitFor();
  check('opening a script never runs it', !existsSync(marker));

  // Edited in the panel, written only by Save.
  await tree.getByRole('treeitem', { name: /^README\.md/ }).click();
  const editor = panel.getByRole('textbox', { name: 'Source of README.md' });
  await editor.click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('edited in the panel');
  check('nothing is written before Save', !(await onDisk('README.md')).includes('edited in the panel'));
  await panel.getByRole('button', { name: 'Save', exact: true }).click();
  await panel.getByText('Saved.', { exact: true }).waitFor();
  check('Save writes the file', (await onDisk('README.md')).includes('edited in the panel'));

  // A file that an agent changed meanwhile is replaced only when the person says so.
  await writeFile(resolve(project, 'README.md'), '# Changed by an agent\n');
  await editor.click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type(' mine');
  await page.keyboard.press('Control+s');
  const ask = page.getByRole('alertdialog').or(page.getByRole('dialog')).filter({ hasText: 'The file changed on disk' });
  await ask.waitFor();
  await ask.getByRole('button', { name: 'Cancel' }).click();
  check('a no keeps the agent\'s version on disk', (await onDisk('README.md')) === '# Changed by an agent\n');
  await panel.getByRole('button', { name: 'Save', exact: true }).click();
  await ask.getByRole('button', { name: 'Overwrite README.md' }).click();
  await panel.getByText('Saved.', { exact: true }).waitFor();
  check('a yes replaces it', (await onDisk('README.md')).includes('mine'));

  // A picture shows what it is, not its pixels, until asked.
  await tree.getByRole('treeitem', { name: /^docs/ }).click();
  await tree.getByRole('treeitem', { name: /^dot\.png/ }).click();
  await panel.getByText(/1 × 1 pixels/).waitFor();
  check('a picture says its size and offers to show it', await panel.getByRole('button', { name: 'Show the picture' }).isVisible());

  // Open in a pane: the file moves to a tab of the workspace, with what it holds, and saves the same way.
  await tree.getByRole('treeitem', { name: /^guide\.md/ }).click();
  await panel.getByRole('textbox', { name: 'Source of docs/guide.md' }).click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('unsaved');
  await panel.getByRole('button', { name: 'Open in a pane' }).click();
  const tab = page.getByRole('tab', { name: /^guide\.md •$/ });
  check('the file is a tab of the workspace, marked while it has changes', !!await until(() => tab.isVisible(), 10000, 'no tab').catch(() => null));
  check('and the panel no longer holds it', await panel.getByRole('textbox', { name: 'Source of docs/guide.md' }).count() === 0);
  await page.getByRole('button', { name: 'Save', exact: true }).last().click();
  await page.getByText('Saved.', { exact: true }).last().waitFor();
  check('the tab writes the file', (await onDisk('docs/guide.md')).includes('unsaved'));
  check('the page said nothing wrong', handle.errors.length === 0, handle.errors.join(' | '));
} catch (error) {
  check('the probe ran to its end', false, String(error.message ?? error));
} finally {
  await handle?.stop();
  await fixture.remove();
}
finish();
