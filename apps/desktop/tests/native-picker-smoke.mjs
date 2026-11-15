// PM6: real Windows native folder selection/cancellation, reached through the app's F3 handler.
// Run only after the integrated normal desktop binary has been rebuilt.
import { chromium, expect } from '@playwright/test';
import { execFile, spawn } from 'node:child_process';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This native folder picker proof qualifies Windows only.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-native-picker-'));
const data = resolve(fixture, 'data');
const original = resolve(fixture, 'original-project');
const chosen = resolve(fixture, 'selected project à');
await mkdir(original); await mkdir(chosen);
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const resultPath = resolve(output, 'native-picker.json');
const evidence = { check: 'C04.3-PM6-real-Windows-folder-picker', checks: {}, native: [],
  gaps: ['Real Google Drive/shared-drive behavior and the global U3 keyboard matrix remain separate qualifications.'], status: 'running' };
await writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`);
const port = 19232;
const live = new Set(); const helpers = new Set(); let launches = 0;
const key = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();
function within(promise, milliseconds, label) {
  let timer;
  const deadline = new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} exceeded its deadline.`)), milliseconds); });
  return Promise.race([promise, deadline]).finally(() => clearTimeout(timer));
}
const invoke = (instance, command, args) => within(instance.page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args }),
  command === 'catalog_browse' ? 25000 : 15000, command);

function exitWithin(app, milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const done = () => { clearTimeout(timer); resolve(); };
    const timer = setTimeout(() => { app.removeListener('exit', done); reject(new Error('Owned native-picker app exit deadline exceeded.')); }, milliseconds);
    app.once('exit', done);
  });
}
function powershell(script, args, timeout = 28000) {
  return new Promise((resolvePromise, reject) => {
    const helper = execFile('powershell.exe', ['-NoProfile', '-NonInteractive', '-STA', '-File', resolve(workspace, 'apps/desktop/tests', script), ...args],
      { encoding: 'utf8', windowsHide: true, timeout, maxBuffer: 128 * 1024 }, (error, stdout) => {
        helpers.delete(helper);
        let result;
        try { result = JSON.parse(stdout.replace(/^\uFEFF/, '').trim()); }
        catch { reject(new Error(`Native helper ${script} did not return bounded JSON${error?.code ? ` (${error.code})` : ''}.`)); return; }
        if (error || result.status === 'failed') {
          // The helper outputs only IDs/classes/types, never window labels or folder values.
          reject(new Error(`Native helper failed: ${JSON.stringify(result)}`)); return;
        }
        resolvePromise(result);
      });
    helpers.add(helper);
  });
}
async function forceClose(instance) {
  if (instance.app.exitCode === null && instance.app.signalCode === null) { instance.app.kill(); await exitWithin(instance.app, 3000).catch(() => {}); }
  await instance.browser?.close().catch(() => {}); live.delete(instance);
}
async function openApp() {
  // Refuse an existing CDP endpoint before touching any renderer, even another Shinatama app.
  await new Promise((resolvePromise, reject) => {
    const probe = createServer();
    probe.once('error', () => reject(new Error('The native-picker test CDP port is already in use; no existing app was attached.')));
    probe.listen({ host: '127.0.0.1', port, exclusive: true }, () => probe.close(error => error ? reject(error) : resolvePromise()));
  });
  const app = spawn(resolve(workspace, 'target/debug/shinatama-desktop.exe'), [], { cwd: workspace, windowsHide: true, stdio: 'ignore',
    env: { ...process.env, SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, `webview-${++launches}`),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` } });
  const instance = { app, browser: null, page: null, errors: [] }; live.add(instance);
  let spawnError; app.once('error', error => { spawnError = error; });
  try {
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
      if (spawnError) throw new Error(`Cannot start the owned native-picker app: ${spawnError.code}`);
      if (app.exitCode !== null) throw new Error(`Native-picker app exited with code ${app.exitCode}.`);
      try { instance.browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
      catch { await delay(250); }
    }
    if (!instance.browser) throw new Error('Native-picker app startup exceeded 30 seconds.');
    const context = instance.browser.contexts()[0];
    // The attention widget is a page too; the workspace is the one that is not widget.html.
    for (const deadline = Date.now() + 10000; !(instance.page = context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'))) && Date.now() < deadline;) await delay(100);
    if (!instance.page) throw new Error('The workspace page did not load.');
    instance.page.on('pageerror', error => instance.errors.push(error.message));
    await instance.page.waitForURL(url => url.hostname === 'tauri.localhost', { timeout: 10000 });
    await expect(instance.page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 10000 });
    const storage = await invoke(instance, 'storage_location');
    expect(key(storage.directory)).toBe(key(data));
    instance.window = await powershell('window-state.ps1', ['-AppProcessId', String(app.pid)], 10000);
    if (!instance.window.handle) throw new Error('The owned app has no main window handle.');
    evidence.runtime = await instance.browser.version(); return instance;
  } catch (error) { await forceClose(instance); throw error; }
}
async function closeApp(instance) {
  expect(await invoke(instance, 'terminal_list')).toEqual([]);
  const closing = Promise.all([instance.page.waitForEvent('close', { timeout: 10000 }), exitWithin(instance.app, 10000)]);
  void closing.catch(() => {});
  await instance.page.getByRole('button', { name: 'Close window', exact: true }).click();
  await closing; expect(instance.app.exitCode).toBe(0); expect(instance.errors).toEqual([]);
  await instance.browser.close().catch(() => {}); live.delete(instance);
}
function nativeAction(instance, mode) {
  const helper = powershell('native-folder.ps1', ['-AppProcessId', String(instance.app.pid), '-ParentWindowHandle', String(instance.window.handle),
    '-FixtureRoot', fixture, '-Mode', mode, ...(mode === 'Choose' ? ['-Folder', chosen] : [])]);
  void helper.catch(() => {});
  return helper;
}
function assertNativeResult(result) {
  expect(result.status).toBe('passed'); expect(result.dialogClass).toBe('#32770'); expect(result.parentOwned).toBe(true);
  evidence.native.push(result);
}
async function f3WithNativeAction(instance, mode) {
  const helper = nativeAction(instance, mode);
  await instance.page.locator('#project').focus(); await instance.page.keyboard.press('F3');
  assertNativeResult(await helper);
}

try {
  let instance = await openApp(); const page = instance.page;
  await page.locator('#project').click(); await page.getByRole('button', { name: 'Project roots…', exact: true }).click(); await page.locator('#folder').fill(original);
  await page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect(page.locator('#project-dialog')).not.toBeVisible();
  await expect(page.locator('#project')).toHaveAttribute('data-project', 'original-project');
  const beforeCancel = await invoke(instance, 'project_recent');
  expect(beforeCancel).toHaveLength(1); expect(key(beforeCancel[0].path)).toBe(key(original));
  await f3WithNativeAction(instance, 'Cancel');
  await expect(page.locator('#project-dialog')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Use folder', exact: true })).toBeEnabled();
  await expect(page.locator('#project-error')).not.toBeVisible();
  await expect(page.locator('#project')).toHaveAttribute('data-project', 'original-project');
  expect(await invoke(instance, 'project_recent')).toEqual(beforeCancel);
  expect(await invoke(instance, 'terminal_list')).toEqual([]);
  evidence.checks.realF3NativeCancelReturnsCancelledWithoutSelectionOrHistoryWrite = true;
  await page.getByRole('button', { name: 'Close', exact: true }).click();

  // Tauri's invoke entry is immutable. Observe a second real chooser's return value
  // directly through the existing scoped app command, with no fake production bridge.
  const requestId = 'native-picker-cancel-result';
  await invoke(instance, 'catalog_begin', { requestId });
  const cancellingHelper = nativeAction(instance, 'Cancel');
  const actualCancel = invoke(instance, 'catalog_browse', { request: { purpose: 'project' }, requestId });
  void actualCancel.catch(() => {});
  const [cancelled, nativeCancelled] = await Promise.all([actualCancel, cancellingHelper]);
  expect(cancelled).toEqual({ kind: 'cancelled' }); assertNativeResult(nativeCancelled);
  expect(await invoke(instance, 'project_recent')).toEqual(beforeCancel);
  evidence.checks.realNativeCancelCommandReturnsCancelled = true;

  await f3WithNativeAction(instance, 'Choose');
  await expect(page.locator('#project-dialog')).not.toBeVisible();
  await expect(page.locator('#project')).toHaveAttribute('data-project', 'selected project à');
  const saved = await invoke(instance, 'project_recent');
  expect(saved).toHaveLength(2); expect(key(saved[0].path)).toBe(key(chosen));
  expect(saved[0].id).toMatch(/^p_[a-z2-7]{12}$/); expect(saved[1]).toEqual(beforeCancel[0]);
  evidence.checks.realF3NativeSelectionSavesExactSyntheticUnicodePath = true;
  await closeApp(instance);
  instance = await openApp();
  await expect(instance.page.locator('#project')).toHaveAttribute('data-project', 'selected project à');
  const restored = await invoke(instance, 'project_recent');
  expect(restored).toEqual(saved); expect(await invoke(instance, 'terminal_list')).toEqual([]);
  await closeApp(instance);
  evidence.checks.nativeSelectionPersistsWithStableIdAcrossRestart = true;
  evidence.checks.noAutomaticTerminalLaunchAndCustomCloseWorks = true;
  evidence.status = 'passed'; await writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`);
  console.log('PASS PM6 Windows: real F3 native dialog cancellation leaves selection/history unchanged; scoped native control selects the exact synthetic Unicode folder; selection persists across restart.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.message;
  await writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`); throw error;
} finally {
  for (const helper of helpers) helper.kill();
  for (const instance of [...live]) await forceClose(instance);
  if (dirname(fixture) === resolve(tmpdir()) && basename(fixture).startsWith('shinatama-native-picker-')) {
    await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  }
}
