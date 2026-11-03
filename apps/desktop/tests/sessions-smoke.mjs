// One coordinated native Windows/CDP proof. All archive contents are synthetic.
// Run after rebuilding the normal desktop and shinatama-session-helper.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-sessions-smoke-'));
const data = resolve(fixture, 'data');
const archive = resolve(fixture, 'synthetic-claude');
const project = resolve(archive, 'projects/fixture-project');
await mkdir(project, { recursive: true });
const originals = new Map();
for (let index = 0; index < 53; index++) {
  const id = `idsession-${String(index).padStart(2, '0')}`;
  const name = `${id}.jsonl`;
  const bytes = Buffer.from(`${JSON.stringify({ sessionId: id, cwd: 'C:\\synthetic-session-project' })}\n`);
  originals.set(name, bytes); await writeFile(resolve(project, name), bytes);
}
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const resultPath = resolve(output, 'sessions-library.json');
const evidence = { check: 'C22-native-session-metadata-library', status: 'running', checks: {},
  scope: '53 tiny synthetic Claude JSONL sessions; metadata only; no native agent start or transcript detail' };
const save = () => writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`);
await save();
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
const app = spawn(resolve(workspace, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: workspace, windowsHide: true, stdio: 'ignore', env: { ...process.env,
    SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let browser, page, owned = false, spawnError;
const errors = [];
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
  }, { command, args }), 16000, command);
}
async function ipc(command, args = {}) {
  const result = await reply(command, args);
  if (!result.ok) throw new Error(`${command} refused (${typeof result.issue === 'string' ? result.issue : 'typed error'}).`);
  return result.value;
}
function exitWithin(milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return new Promise((accept, reject) => {
    const done = () => { clearTimeout(timer); accept(); };
    const timer = setTimeout(() => { app.removeListener('exit', done); reject(new Error('Owned sessions-test app exit deadline exceeded.')); }, milliseconds);
    app.once('exit', done);
  });
}
async function assertSourceUnchanged() {
  expect((await readdir(archive)).sort()).toEqual(['projects']);
  expect((await readdir(resolve(archive, 'projects'))).sort()).toEqual(['fixture-project']);
  expect((await readdir(project)).sort()).toEqual([...originals.keys()].sort());
  const hash = createHash('sha256');
  for (const [name, bytes] of originals) {
    const after = await readFile(resolve(project, name)); expect(after.equals(bytes)).toBe(true);
    hash.update(name); hash.update(after);
  }
  return hash.digest('hex');
}
try {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (spawnError) throw new Error(`Cannot start owned sessions-test app (${spawnError.code}).`);
    if (app.exitCode !== null) throw new Error('Owned sessions-test app exited during startup.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
    catch { await delay(250); }
  }
  if (!browser) throw new Error('Owned sessions-test WebView startup exceeded 30 seconds.');
  const context = browser.contexts()[0]; page = context.pages()[0] || await context.waitForEvent('page', { timeout: 5000 });
  await page.waitForURL(url => url.hostname === 'tauri.localhost', { timeout: 10000 });
  await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 30000 });
  const storage = await ipc('storage_location');
  if (pathKey(storage.directory) !== pathKey(data)) throw new Error('Synthetic storage identity mismatch; no UI was touched.');
  owned = true; page.on('pageerror', error => { errors.push(error.message.slice(0, 1024)); });
  expect(await ipc('terminal_list')).toEqual([]);
  const sourceBefore = await assertSourceUnchanged();
  await page.locator('#sessions').click();
  const dialog = page.locator('#session-library-dialog');
  await dialog.getByRole('button', { name: 'Load archives', exact: true }).click();
  await expect(dialog.locator('#session-library-status')).toContainText('No registered archives');
  expect(await ipc('session_roots')).toEqual([]);
  await expect(dialog.locator('#session-library-root option')).toHaveCount(1);
  evidence.checks.noAmbientArchiveDiscovery = true;

  await dialog.getByText('Add a read-only archive', { exact: true }).click();
  await dialog.locator('#session-archive-agent').selectOption('claude_code');
  await dialog.locator('#session-archive-path').fill(archive);
  await dialog.getByRole('button', { name: 'Add archive', exact: true }).click();
  await expect(dialog.locator('#session-library-status')).toContainText('Archive registered', { timeout: 15000 });
  const roots = await ipc('session_roots'); expect(roots).toHaveLength(1);
  const root = roots[0]; expect(root.harness).toBe('claude_code'); expect(root.kind).toBe('system'); expect(root.access).toBe('read_only');
  const selector = { storeRootId: root.storeRootId, configurationRevision: root.configurationRevision };
  expect(root.available).toBe(true); expect(root.label).toContain('synthetic-claude');
  evidence.checks.explicitReadOnlyArchive = true;

  await expect(page.locator('#resource-status')).toContainText('Resources: available', { timeout: 20000 });
  await dialog.getByRole('button', { name: 'Reload sessions', exact: true }).click();
  const rows = dialog.locator('tbody tr');
  await expect(rows).toHaveCount(50, { timeout: 15000 });
  await expect(dialog.locator('.session-library-pagination')).toContainText('1–50 of 53');
  const firstId = await rows.first().getByRole('button').textContent(); expect(firstId).toMatch(/^idsession-\d{2}$/);
  await dialog.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(rows).toHaveCount(3, { timeout: 15000 });
  await expect(dialog.locator('.session-library-pagination')).toContainText('51–53 of 53');
  await dialog.getByRole('button', { name: 'Previous', exact: true }).click();
  await expect(rows).toHaveCount(50, { timeout: 15000 });
  await dialog.locator('#session-library-filter').fill(firstId);
  await expect(rows).toHaveCount(1);
  await rows.getByRole('button', { name: firstId, exact: true }).click();
  await expect(dialog.locator('.session-library-metadata')).toContainText('Unknown');
  await expect(dialog.getByRole('button', { name: 'Detail unavailable', exact: true })).toBeDisabled();
  await expect(dialog.getByRole('button', { name: 'Resume unavailable', exact: true })).toBeDisabled();
  evidence.checks.pages = [50, 3, 50]; evidence.checks.localFilterAndUnknownMetadata = true;

  const duplicate = await ipc('session_archive_add', { request: { harness: 'claude_code', path: archive } });
  expect(duplicate.storeRootId).toBe(root.storeRootId); expect(duplicate.configurationRevision).toBe(root.configurationRevision);
  expect(await ipc('session_roots')).toHaveLength(1);
  const request = { root: selector, cursor: null, limit: 50, includeChildren: true };
  const missing = await reply('session_list', { request: { ...request, root: { storeRootId: 'missing-explicit-root', configurationRevision: 1 } } });
  expect(missing.ok).toBe(false); expect(missing.issue).toBe('root_unavailable');
  const stale = await reply('session_list', { request: { ...request, root: { ...selector, configurationRevision: selector.configurationRevision + 1 } } });
  expect(stale.ok).toBe(false); expect(stale.issue).toBe('root_changed');
  evidence.checks.duplicateIdentityStable = true; evidence.checks.missingAndStaleRefused = true;
  expect(await assertSourceUnchanged()).toBe(sourceBefore); evidence.checks.all53SourceFilesUnchanged = true;
  expect(await ipc('terminal_list')).toEqual([]); evidence.checks.noAgentOrTerminalStarted = true;
  expect(errors).toEqual([]);
  await page.screenshot({ path: resolve(output, 'sessions-library.png'), timeout: 3000 });
  await dialog.getByRole('button', { name: 'Close sessions', exact: true }).click();
  await expect(dialog).toBeHidden();
  const closing = Promise.all([page.waitForEvent('close', { timeout: 15000 }), exitWithin(15000)]);
  void closing.catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  await closing; expect(app.exitCode).toBe(0);
  expect(await assertSourceUnchanged()).toBe(sourceBefore);
  evidence.status = 'passed'; await save();
  console.log('PASS: explicit read-only archive, 53 synthetic sessions in 50/3 pages, filter/Unknown metadata, duplicate identity, missing/stale refusal, unchanged source bytes, no agent startup, owned frame close.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = String(error.message).slice(0, 4096);
  if (owned && page && !page.isClosed()) {
    try {
      evidence.diagnostic = await within(page.evaluate(() => ({
        status: document.querySelector('#session-library-status')?.textContent?.slice(0, 2048),
        rowCount: document.querySelectorAll('#session-library-dialog tbody tr').length,
        pagination: document.querySelector('.session-library-pagination')?.textContent?.slice(0, 256),
        resourceStatus: document.querySelector('#resource-status')?.textContent?.slice(0, 256),
      })), 2000, 'Synthetic UI diagnostics');
      await page.screenshot({ path: resolve(output, 'sessions-library-failure.png'), timeout: 2000 });
    } catch { evidence.diagnostic = 'unavailable'; }
  }
  await save(); throw error;
} finally {
  if (app.exitCode === null && app.signalCode === null) { app.kill(); await exitWithin(3000).catch(() => {}); }
  await browser?.close().catch(() => {});
}
