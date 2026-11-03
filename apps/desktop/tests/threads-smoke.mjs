// DB-only native proof: synthetic project/draft, private WebView, no terminal launch.
// Root schedules this against the rebuilt desktop; global resource deferral is respected.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const packageExecutable = process.env.SHINATAMA_SMOKE_APP_EXE;
if (packageExecutable && !isAbsolute(packageExecutable)) throw new Error('The package executable must be an explicit absolute path.');
const executable = packageExecutable || resolve(workspace, 'target/debug/shinatama-desktop.exe');
const receiptName = packageExecutable ? 'package-threads-sidebar' : 'threads-sidebar';
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-threads-smoke-'));
const data = resolve(fixture, 'data'); const projectPath = resolve(fixture, 'synthetic-project');
await mkdir(projectPath, { recursive: true });
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = { check: 'C25-native-manual-thread-sidebar', status: 'running', checks: {},
  scope: 'Synthetic project and saved draft only; no terminal, agent, resource-policy override or native resume' };
const save = () => writeFile(resolve(output, `${receiptName}.json`), `${JSON.stringify(evidence, null, 2)}\n`);
await save();
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
const app = spawn(executable, [], {
  cwd: packageExecutable ? dirname(executable) : workspace, windowsHide: true, stdio: 'ignore', env: { ...process.env,
    SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let browser, page, owned = false, spawnError; const errors = [];
app.once('error', error => { spawnError = error; });
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();
function within(promise, milliseconds, label) {
  let timer;
  return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} exceeded its deadline.`)), milliseconds); })])
    .finally(() => clearTimeout(timer));
}
async function reply(command, args = {}) {
  return within(page.evaluate(async ({ command, args }) => {
    try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
    catch (issue) { return { ok: false, issue }; }
  }, { command, args }), 15000, command);
}
async function idleReply(command, args = {}) {
  const deadline = Date.now() + 15000;
  do {
    const result = await reply(command, args);
    if (result.ok || result.issue?.code !== 'busy') return result;
    await delay(100);
  } while (Date.now() < deadline);
  throw new Error(`${command} remained busy for 15 seconds.`);
}
async function ipc(command, args = {}) {
  const result = await idleReply(command, args);
  if (!result.ok) throw new Error(`${command} refused (${result.issue?.code ?? 'typed error'}).`);
  return result.value;
}
function exitWithin(milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return new Promise((accept, reject) => {
    const done = () => { clearTimeout(timer); accept(); };
    const timer = setTimeout(() => { app.removeListener('exit', done); reject(new Error('Owned threads-test app exit deadline exceeded.')); }, milliseconds);
    app.once('exit', done);
  });
}
try {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (spawnError) throw new Error(`Cannot start owned threads-test app (${spawnError.code}).`);
    if (app.exitCode !== null) throw new Error('Owned threads-test app exited during startup.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
    catch { await delay(250); }
  }
  if (!browser) throw new Error('Owned threads-test WebView startup exceeded 30 seconds.');
  const context = browser.contexts()[0]; page = context.pages()[0] || await context.waitForEvent('page', { timeout: 5000 });
  page.setDefaultTimeout(15000);
  await page.waitForURL(url => url.hostname === 'tauri.localhost');
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const storage = await ipc('storage_location');
  if (pathKey(storage.directory) !== pathKey(data)) throw new Error('Synthetic storage identity mismatch; no UI was touched.');
  owned = true; page.on('pageerror', error => { errors.push(error.message.slice(0, 1024)); });
  expect(await ipc('terminal_list')).toEqual([]);
  const project = await ipc('project_select', { path: projectPath });
  expect(project.id).toMatch(/^p_[a-z2-7]{12}$/);
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const sidebar = page.locator('.thread-sidebar'); const selector = sidebar.locator('#thread-sidebar-project');
  await expect(selector).toHaveValue(project.id);
  await expect(sidebar.locator('.thread-sidebar-body')).toHaveAttribute('aria-busy', 'false');
  expect(await ipc('thread_sidebar_load')).toEqual({ revision: 0, settings: { version: 1, width: 264, collapsed: false } });
  await sidebar.getByRole('button', { name: 'New thread', exact: true }).click();
  const anchor = sidebar.locator('button.thread-sidebar-anchor');
  await expect(anchor).toHaveCount(1); const threadId = await anchor.getAttribute('data-thread-id');
  expect(threadId).toMatch(/^t_[a-z2-7]{12}$/);
  const listRequest = { projectId: project.id, cursor: null, limit: 50 };
  const listed = await ipc('thread_list', { request: listRequest });
  expect(listed.next).toBeNull(); expect(listed.rows).toHaveLength(1);
  expect(listed.rows[0]).toMatchObject({ id: threadId, projectId: project.id, status: 'unbound', harness: null, nativeAvailable: null });
  await anchor.click(); await expect(sidebar.locator('.thread-sidebar-detail')).toContainText('No native session linked');
  expect(await ipc('terminal_list')).toEqual([]);
  evidence.checks.savedDraftAndCanonicalProjectScope = true;

  const separator = sidebar.getByRole('separator', { name: 'Thread sidebar width', exact: true });
  await expect(separator).toHaveAttribute('aria-valuenow', '264');
  await separator.focus(); await separator.press('ArrowRight');
  await expect(separator).toHaveAttribute('aria-valuenow', '280');
  await sidebar.getByRole('button', { name: 'Collapse threads', exact: true }).click();
  const collapsed = { version: 1, width: 280, collapsed: true };
  await expect.poll(async () => {
    const value = await ipc('thread_sidebar_load'); return value.revision > 0 && JSON.stringify(value.settings) === JSON.stringify(collapsed);
  }, { timeout: 15000 }).toBe(true);
  const acknowledged = await ipc('thread_sidebar_load');
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await expect(selector).toHaveValue(project.id);
  await expect(sidebar.getByRole('button', { name: 'Expand threads', exact: true })).toBeVisible();
  await expect(sidebar).toHaveClass(/is-collapsed/);
  expect(await sidebar.evaluate(element => element.style.getPropertyValue('--thread-sidebar-width'))).toBe('280px');
  expect(await ipc('thread_sidebar_load')).toEqual(acknowledged);
  const missing = await idleReply('thread_list', { request: { ...listRequest, projectId: 'p_aaaaaaaaaaaa' } });
  expect(missing).toEqual({ ok: false, issue: { code: 'project_missing' } });
  const stale = await idleReply('thread_sidebar_save', { request: { expectedRevision: 0, settings: { version: 1, width: 180, collapsed: false } } });
  expect(stale).toEqual({ ok: false, issue: { code: 'conflict' } });
  expect(await ipc('thread_sidebar_load')).toEqual(acknowledged);
  evidence.checks.keyboardWidthAndCollapsedPersisted = true;
  evidence.checks.missingProjectAndStaleSaveRefusedWithoutMutation = true;

  await sidebar.getByRole('button', { name: 'Expand threads', exact: true }).click();
  await expect(sidebar.getByRole('button', { name: 'Reload threads', exact: true })).toBeEnabled();
  await sidebar.getByRole('button', { name: 'Reload threads', exact: true }).click();
  await expect(anchor).toHaveCount(1); await expect(anchor).toHaveAttribute('data-thread-id', threadId);
  await anchor.click(); await expect(sidebar.locator('.thread-sidebar-detail')).toContainText('No native session linked');
  expect((await ipc('thread_list', { request: listRequest })).rows.map(row => row.id)).toEqual([threadId]);
  expect(await ipc('terminal_list')).toEqual([]); expect(errors).toEqual([]);
  // Observe the last display acknowledgement before frame close, rather than racing its debounce.
  await expect.poll(async () => (await ipc('thread_sidebar_load')).settings, { timeout: 15000 })
    .toEqual({ version: 1, width: 280, collapsed: false });
  evidence.checks.sameDraftAfterReloadAndNoAutoResume = true;
  await page.screenshot({ path: resolve(output, `${receiptName}.png`), timeout: 3000 });
  const closing = Promise.all([page.waitForEvent('close', { timeout: 15000 }), exitWithin(15000)]); void closing.catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  await closing; expect(app.exitCode).toBe(0); evidence.checks.ownedFrameClose = true;
  evidence.status = 'passed'; await save();
  console.log('PASS: manual canonical thread, unchanged project scope, keyboard width and collapse persisted, stale save refused, same draft after reload, no terminal startup, owned frame close.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = String(error.message).slice(0, 4096);
  if (owned && page && !page.isClosed()) {
    try {
      evidence.diagnostic = await within(page.evaluate(() => ({
        status: document.querySelector('.thread-sidebar-status')?.textContent?.slice(0, 512),
        rows: document.querySelectorAll('button.thread-sidebar-anchor').length,
        collapsed: document.querySelector('.thread-sidebar')?.classList.contains('is-collapsed'),
        width: document.querySelector('[aria-label="Thread sidebar width"]')?.getAttribute('aria-valuenow'),
      })), 2000, 'Synthetic thread diagnostics');
      await page.screenshot({ path: resolve(output, 'threads-sidebar-failure.png'), timeout: 2000 });
    } catch { evidence.diagnostic = 'unavailable'; }
  }
  await save(); throw error;
} finally {
  if (app.exitCode === null && app.signalCode === null) { app.kill(); await exitWithin(3000).catch(() => {}); }
  await browser?.close().catch(() => {});
}
