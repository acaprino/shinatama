// Connected C04 UI proof. Native Windows chooser interaction is a separate PM6 proof.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, rename, rm, utimes, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { setTimeout as delay } from 'node:timers/promises';
import { startSession } from './session-flow.mjs';

if (process.platform !== 'win32') throw new Error('This embedded catalog test qualifies Windows only.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-catalog-smoke-'));
const data = resolve(fixture, 'data');
const rootA = resolve(fixture, 'root-a');
const rootB = resolve(fixture, 'root-b');
const renamedRoot = resolve(fixture, 'root-b-unavailable');
await mkdir(rootA); await mkdir(rootB);
const used = resolve(rootA, 'z-last-opened');
await mkdir(used); await utimes(used, 1000, 1000);
for (let index = 0; index < 180; index++) {
  const path = resolve(rootA, `alpha-${String(index).padStart(3, '0')}`);
  await mkdir(path); await utimes(path, 1700000000 + index, 1700000000 + index);
}
for (let index = 0; index < 5; index++) {
  const path = resolve(rootB, `beta-${index}`);
  await mkdir(path); await utimes(path, 1700001000 + index, 1700001000 + index);
}
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = { check: 'C04-connected-project-catalog', checks: {},
  gaps: ['Native folder chooser interaction and cancellation are not automated by this CDP test (PM6).', 'Real Google Drive/shared-drive latency is not qualified by the synthetic local roots.'], status: 'running' };
const resultPath = resolve(output, 'project-catalog.json');
await writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`);
const port = 19231;
const live = new Set();
let launches = 0;
const invoke = (instance, command, args) => instance.page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const key = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();

function exitWithin(app, milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const done = () => { clearTimeout(timer); resolve(); };
    const timer = setTimeout(() => { app.removeListener('exit', done); reject(new Error('Owned catalog-test app exit deadline exceeded.')); }, milliseconds);
    app.once('exit', done);
  });
}
async function forceClose(instance) {
  if (instance.page && !instance.page.isClosed()) await invoke(instance, 'terminal_shutdown').catch(() => {});
  if (instance.app.exitCode === null && instance.app.signalCode === null) { instance.app.kill(); await exitWithin(instance.app, 3000).catch(() => {}); }
  await instance.browser?.close().catch(() => {}); live.delete(instance);
}
async function openApp() {
  await new Promise((resolvePromise, reject) => {
    const probe = createServer();
    probe.once('error', () => reject(new Error('The catalog test CDP port is already in use; no existing app was attached.')));
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
      if (spawnError) throw new Error(`Cannot start catalog-test app: ${spawnError.code}`);
      if (app.exitCode !== null) throw new Error(`Catalog-test app exited with code ${app.exitCode}.`);
      try { instance.browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
      catch { await delay(250); }
    }
    if (!instance.browser) throw new Error('Catalog-test app startup exceeded 30 seconds.');
    const context = instance.browser.contexts()[0];
    // The attention widget is a page too; the workspace is the one that is not widget.html.
    for (const deadline = Date.now() + 10000; !(instance.page = context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'))) && Date.now() < deadline;) await delay(100);
    if (!instance.page) throw new Error('The workspace page did not load.');
    instance.page.on('pageerror', error => instance.errors.push(error.message));
    await instance.page.waitForURL(url => url.hostname === 'tauri.localhost', { timeout: 10000 });
    await expect(instance.page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 10000 });
    const storage = await invoke(instance, 'storage_location');
    expect(key(storage.directory)).toBe(key(data));
    return instance;
  } catch (error) { await forceClose(instance); throw error; }
}
async function closeApp(instance) {
  const hasTerminals = (await invoke(instance, 'terminal_list')).length > 0;
  const closing = Promise.all([instance.page.waitForEvent('close', { timeout: 10000 }), exitWithin(instance.app, 10000)]);
  void closing.catch(() => {});
  await instance.page.getByRole('button', { name: 'Close window', exact: true }).click();
  if (hasTerminals) await instance.page.getByRole('alertdialog', { name: 'Close workspace?', exact: true }).getByRole('button', { name: 'Stop', exact: true }).click();
  await closing; expect(instance.app.exitCode).toBe(0); expect(instance.errors).toEqual([]);
  await instance.browser.close().catch(() => {}); live.delete(instance);
}
function managerConfig() {
  const db = new DatabaseSync(resolve(data, 'shinatama.db'), { readOnly: true });
  try { return JSON.parse(db.prepare("SELECT value FROM setting WHERE key='projects.manager.v1'").get().value); }
  finally { db.close(); }
}

try {
  let instance = await openApp(); const page = instance.page;
  await page.locator('#project').click(); await page.getByRole('button', { name: 'Project roots…', exact: true }).click(); await page.locator('#folder').fill(used);
  await page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect(page.locator('#project-dialog')).not.toBeVisible();
  await (await startSession(page, { project: used, harness: 'powershell' })).done(30000);
  await expect(page.locator('.xterm-accessibility')).toContainText('PS ', { timeout: 20000 });
  const originalSession = (await invoke(instance, 'terminal_list'))[0];
  // Roots are saved the moment they are added; the first one becomes the default.
  await page.locator('#project').click(); await page.getByRole('button', { name: 'Project roots…', exact: true }).click();
  const rootsSaved = count => expect.poll(async () => {
    if (await page.locator('#project-error').isVisible()) throw new Error(`Saving synthetic project roots failed: ${await page.locator('#project-error').textContent()}`);
    return page.locator('.catalog-root-status').count();
  }, { timeout: 10000 }).toBe(count);
  await page.locator('#root-path').fill(rootA); await page.locator('#add-root').click(); await rootsSaved(1);
  await page.locator('#root-path').fill(rootB); await page.locator('#root-path').press('Enter'); await rootsSaved(2);
  await page.locator('.catalog-root-status').nth(1).getByRole('button', { name: /^Make default/ }).click();
  await expect(page.locator('.catalog-root-status').nth(1).locator('.root-default')).toHaveAttribute('aria-pressed', 'true', { timeout: 10000 });
  await expect(page.locator('#catalog-state')).toHaveText('186 matching projects', { timeout: 15000 });
  await expect(page.locator('.catalog-root-status')).toHaveCount(2);
  await expect(page.locator('#default-root-summary')).toContainText(rootB);
  await expect(page.locator('.catalog-project')).toHaveCount(100);
  await expect(page.locator('.catalog-project strong').first()).toHaveText('z-last-opened');
  await expect(page.locator('#recent-project-heading')).toHaveText('Single folders outside your roots');
  await expect(page.locator('.recent-project')).toHaveCount(0);
  await page.locator('#catalog-next').click();
  await expect(page.locator('.catalog-project')).toHaveCount(86);
  await expect(page.locator('#catalog-page')).toHaveText('101–186 of 186');
  await page.locator('#project-filter').fill('alpha-17');
  await page.locator('#catalog-next').evaluate(button => button.click()); // A pending filter must not reuse the old offset.
  await expect(page.locator('.catalog-project')).toHaveCount(10);
  await expect(page.locator('#catalog-state')).toHaveText('10 matching projects');
  await page.locator('#project-filter').fill('no-project-has-this-name');
  await expect(page.locator('.catalog-project')).toHaveCount(0);
  await expect(page.locator('#catalog-state')).toHaveText('0 matching projects');
  await page.locator('#refresh-projects').focus(); await page.keyboard.press('/');
  await expect(page.locator('#project-filter')).toBeFocused();
  await page.locator('#project-filter').fill('z-last-opened');
  await expect(page.locator('.catalog-project')).toHaveCount(1);
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.locator('#project')).toHaveAttribute('data-project', 'z-last-opened');
  expect(await invoke(instance, 'terminal_list')).toEqual([originalSession]);
  evidence.checks.multipleRootsExplicitDefaultAnd100RowPagination = true;
  evidence.checks.usageOrderingAndIncrementalFilter = true;
  evidence.checks.catalogCancelPreservesSelectionAndTerminalIdentity = true;

  // A path typed but never added is not saved.
  await page.locator('#project').click(); await page.getByRole('button', { name: 'Project roots…', exact: true }).click();
  await page.locator('#root-path').fill(resolve(fixture, 'discarded-root'));
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await page.locator('.xterm-helper-textarea').focus(); await page.keyboard.type('/');
  await expect(page.locator('#project-dialog')).not.toBeVisible();
  await page.keyboard.press('Backspace');
  await page.keyboard.type("Write-Output ('CATALOG_' + 'HEALTHY')"); await page.keyboard.press('Enter');
  await expect(page.locator('.xterm-accessibility')).toContainText('CATALOG_HEALTHY', { timeout: 10000 });
  evidence.checks.filterShortcutNeverStealsTerminalInput = true;
  evidence.checks.discardedRootDraftDoesNotChangeConfiguration = true;
  await closeApp(instance);
  const saved = managerConfig(); expect(saved.roots).toHaveLength(2);
  expect(key(saved.roots.find(root => root.id === saved.defaultRootId).path)).toBe(key(rootB));
  // Both endpoints are named children of this test's unique temporary directory.
  expect(dirname(rootB)).toBe(fixture); expect(dirname(renamedRoot)).toBe(fixture);
  await rename(rootB, renamedRoot);
  instance = await openApp();
  await expect(instance.page.locator('#project')).toHaveAttribute('data-project', 'z-last-opened');
  expect(await invoke(instance, 'terminal_list')).toEqual([]);
  await instance.page.locator('#project').click(); await instance.page.getByRole('button', { name: 'Project roots…', exact: true }).click();
  await expect(instance.page.locator('.catalog-root-status.missing')).toHaveCount(1, { timeout: 15000 });
  await expect(instance.page.locator('.catalog-root-status.missing')).toContainText(rootB);
  await expect(instance.page.locator('.catalog-root-status.missing')).toContainText('Default');
  await expect(instance.page.locator('#default-root-summary')).toContainText(rootB);
  await expect(instance.page.locator('.catalog-root-status').nth(1).locator('.root-default')).toHaveAttribute('aria-pressed', 'true');
  await instance.page.screenshot({ path: resolve(output, 'project-roots.png') });
  await instance.page.getByRole('button', { name: 'Close', exact: true }).click();
  await closeApp(instance);
  expect(managerConfig()).toEqual(saved);
  evidence.checks.restartRetainsRootsAndMissingDefaultWithoutFallback = true;
  evidence.checks.noAutomaticTerminalLaunchOnRestart = true;
  evidence.status = 'passed'; await writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`);
  console.log('PASS: configured roots and explicit default persist, 186 projects paginate at100 rows, usage ordering/filter work, cancellation preserves terminal, missing default is reported without fallback, terminal slash is preserved. PM6 native chooser and real remote-root qualification remain separate.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.message;
  await writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`); throw error;
} finally {
  for (const instance of [...live]) await forceClose(instance);
  if (dirname(fixture) === resolve(tmpdir()) && basename(fixture).startsWith('shinatama-catalog-smoke-')) {
    await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  }
}
