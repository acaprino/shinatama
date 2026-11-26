// The native folder chooser of Windows, reached by F3 (the command "Open a folder as a project"), on the built exe, Windows only (PM6). A helper
// (native-folder.ps1, UI Automation) finds the real dialog of the app's window, checks that it is owned by it, and cancels it or chooses a folder in
// it. Cancel leaves the recent projects as they were; Choose makes the folder the most recent project, whatever characters its name has.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/picker.mjs
// It stands for legacy native-picker-smoke. window-state.ps1 gives the handle of the main window to the helper.
import { execFile } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { check, clearNotices, finish, fixtureFolder, openApp, pathKey, root, until } from './lib.mjs';

const fixture = await fixtureFolder('picker');
const chosen = resolve(fixture.folder, 'selected project à');
await mkdir(chosen);

/** A helper of this folder, in a single-threaded apartment, that answers one line of JSON: only ids, classes and types, never a window's text. */
const helper = (script, args, timeout = 28000) => new Promise((accept, reject) => {
  execFile('powershell.exe', ['-NoProfile', '-NonInteractive', '-STA', '-File', resolve(root, 'apps/desktop/tests/native', script), ...args],
    { encoding: 'utf8', windowsHide: true, timeout, maxBuffer: 128 * 1024 }, (error, stdout) => {
      let result;
      try { result = JSON.parse(stdout.replace(/^﻿/, '').trim()); } catch { reject(new Error(`${script} did not answer in JSON${error?.code ? ` (${error.code})` : ''}.`)); return; }
      if (error || result.status === 'failed') reject(new Error(`${script} failed: ${JSON.stringify(result)}`)); else accept(result);
    });
});

let handle;
try {
  handle = await openApp(fixture);
  const { page, ipc, app } = handle;
  const window = await helper('window-state.ps1', ['-AppProcessId', String(app.pid)], 10000);
  if (!window.handle) throw new Error('The app has no main window handle.');
  await ipc('project_select', { path: fixture.project });
  const before = await ipc('project_recent');
  const f3 = async (mode) => {
    const native = helper('native-folder.ps1', ['-AppProcessId', String(app.pid), '-ParentWindowHandle', String(window.handle), '-FixtureRoot', fixture.folder, '-Mode', mode, ...(mode === 'Choose' ? ['-Folder', chosen] : [])]);
    void native.catch(() => {});
    await clearNotices(page);
    await page.getByRole('button', { name: 'Workspace', exact: true }).first().focus();
    await page.keyboard.press('F3');
    return native;
  };
  const cancelled = await f3('Cancel');
  check('the native dialog is the system\'s and is owned by the window', cancelled.status === 'passed' && cancelled.dialogClass === '#32770' && cancelled.parentOwned === true, JSON.stringify(cancelled));
  await page.waitForTimeout(500);
  check('Cancel leaves the recent projects as they were', JSON.stringify(await ipc('project_recent')) === JSON.stringify(before));
  const picked = await f3('Choose');
  check('Choose answers from the same kind of dialog', picked.status === 'passed' && picked.parentOwned === true, JSON.stringify(picked));
  check('the folder chosen is the most recent project', !!await until(async () => pathKey((await ipc('project_recent'))[0]?.path) === pathKey(chosen), 15000, 'not chosen').catch(() => null));
  check('no terminal was started', (await ipc('terminal_list')).length === 0);
  check('the page said nothing wrong', handle.errors.length === 0, handle.errors.join(' | '));
} catch (error) {
  check('the probe ran to its end', false, String(error.message ?? error));
} finally {
  await handle?.stop();
  await fixture.remove();
}
finish();
