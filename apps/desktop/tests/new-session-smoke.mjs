// Native user-path proof. Root schedules execution after the coordinated build.
// No Check/Prepare IPC, login answers, Claude input, clipboard or user-home reads.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const harness = process.env.SHINATAMA_SMOKE_SESSION_HARNESS || 'claude_code';
if (!['claude_code', 'powershell'].includes(harness)) throw new Error('Choose claude_code or powershell for this proof.');
const workspace = process.cwd();
const packageExecutable = process.env.SHINATAMA_SMOKE_APP_EXE;
if (packageExecutable && !isAbsolute(packageExecutable)) throw new Error('The package executable must be an explicit absolute path.');
const executable = packageExecutable || resolve(workspace, 'target/debug/shinatama-desktop.exe');
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-new-session-'));
const data = resolve(fixture, 'data');
const projectPaths = [resolve(fixture, 'project-a'), resolve(fixture, 'project-b')];
for (const folder of projectPaths) await mkdir(folder);
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = {
  check: 'native-new-session-user-path', harness, status: 'running', checks: {},
  scope: 'Fresh private app state and synthetic projects; original terminal startup only. No sign-in answers or Claude prompt sent; provider traffic not qualified.',
};
const save = () => writeFile(resolve(output, `new-session-${harness}.json`), `${JSON.stringify(evidence, null, 2)}\n`);
await save();
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
const app = spawn(executable, [], {
  cwd: packageExecutable ? dirname(executable) : workspace, windowsHide: true, stdio: 'ignore',
  env: { ...process.env, SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let browser, page, owned = false, spawnError, pageErrors = 0, frameClosed = false;
const opened = [];
app.once('error', error => { spawnError = error; });
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
function within(promise, milliseconds, label) {
  let timer;
  return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} exceeded its deadline.`)), milliseconds); })])
    .finally(() => clearTimeout(timer));
}
async function reply(command, args = {}) {
  return within(page.evaluate(async ({ command, args }) => {
    try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
    catch (issue) { return { ok: false, code: issue?.code ?? 'unavailable' }; }
  }, { command, args }), 15000, command);
}
async function ipc(command, args = {}) {
  const deadline = Date.now() + 15000;
  do {
    const result = await reply(command, args);
    if (result.ok) return result.value;
    if (!['busy', 'unavailable'].includes(result.code)) throw new Error(`${command} refused (${result.code}).`);
    await delay(100);
  } while (Date.now() < deadline);
  throw new Error(`${command} remained unavailable for 15 seconds.`);
}
const sessions = () => ipc('terminal_list');
const ids = async () => (await sessions()).map(session => session.id).sort();
const savedLayout = async () => {
  const value = await ipc('global_workspace_load', { request: { workspaceId: 'main' } });
  return value ? JSON.parse(value.layoutJson) : null;
};
const pane = id => page.locator(`.wm-pane:has([data-view-id="${id}"])`);
function exitWithin(milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return new Promise((accept, reject) => {
    const done = () => { clearTimeout(timer); accept(); };
    const timer = setTimeout(() => { app.removeListener('exit', done); reject(new Error('Owned proof app exit deadline exceeded.')); }, milliseconds);
    app.once('exit', done);
  });
}
async function redactDiagnostics() {
  const result = {};
  try {
    const status = await ipc('resources_status');
    result.resources = {complete:status.complete,phase:status.phase,issue:status.monitor_issue,
      remainingCommit:status.system_remaining_commit_bytes,availablePhysical:status.system_available_physical_bytes,
      observedCpu:status.observed_group_cpu_busy_basis_points,callerAge:status.caller_age_ms,cycleAge:status.cycle_age_ms};
    result.policy = await page.evaluate(() => globalThis.__newSessionObservedPolicy ?? null);
  } catch { result.resources = 'unavailable'; }
  try {
    const status = await ipc('agent_status');
    result.agent = { checking: status.checking, blocked: status.blocked,
      canaryCount: Array.isArray(status.canary) ? status.canary.length : 0,
      startupFailure: typeof status.startupFailure === 'string' ? status.startupFailure.replace(/[\r\n]/g, ' ').slice(0, 240) : null,
      versionProbe: status.versionProbe ? {
        issue: status.versionProbe.issue, elapsedMs: status.versionProbe.elapsed_ms,
        preparationMs: status.versionProbe.preparation_elapsed_ms,
        admissionMs: status.versionProbe.admission_elapsed_ms,
        executionMs: status.versionProbe.execution_elapsed_ms,
      } : null };
  } catch { result.agent = 'unavailable'; }
  try {
    const status = await ipc('sidecar_status');
    result.service = { phase: status.phase, healthy: status.healthy, reason: status.observation?.reason ?? null };
  } catch { result.service = 'unavailable'; }
  return result;
}
async function openFromDialog(projectPath, expectedCount) {
  const before = await ids();
  await page.locator('#new-terminal').click();
  const dialog = page.locator('#new-session-dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Open', exact: true })).toBeEnabled();
  await dialog.locator('#new-session-project').fill(projectPath);
  await dialog.locator('#new-session-harness').selectOption(harness);
  expect(await ids()).toEqual(before);
  await dialog.getByRole('button', { name: 'Open', exact: true }).click();
  await expect.poll(async () => {
    if (!await dialog.isVisible()) return 'opened';
    if (await dialog.locator('#new-session-error').isVisible()) return 'failed';
    return 'opening';
  }, { timeout: 90000, intervals: [100, 250, 500] }).not.toBe('opening');
  if (await dialog.isVisible()) {
    evidence.inlineError = (await dialog.locator('#new-session-error').textContent()).replace(/[\r\n]/g, ' ').slice(0, 512);
    throw new Error('New session displayed an inline startup error.');
  }
  await expect.poll(ids, { timeout: 15000 }).toHaveLength(expectedCount);
  const now = await sessions();
  const created = now.find(session => !before.includes(session.id));
  if (!created || now.filter(session => !before.includes(session.id)).length !== 1) throw new Error('One Open did not create exactly one session.');
  expect(pathKey(created.cwd)).toBe(pathKey(projectPath));
  if (harness === 'claude_code') {
    expect(created.id).toMatch(/^i_[a-z2-7]{12}$/); expect(created.shell).toBe('claude_code');
  } else expect(created.shell).toBe('PowerShell');
  await expect.poll(async () => (await savedLayout())?.views.find(view => view.id === created.id)?.resource.threadId ?? null,
    { timeout: 15000 }).toMatch(/^t_[a-z2-7]{12}$/);
  const layout = await savedLayout();
  const panel = layout.views.find(view => view.id === created.id);
  if (harness === 'claude_code') expect(panel.resource.instanceId).toBe(created.id);
  const recent = await ipc('project_recent');
  const project = recent.find(item => pathKey(item.path) === pathKey(projectPath));
  expect(project?.id).toMatch(/^p_[a-z2-7]{12}$/);
  const stored = await ipc('thread_list', { request: { projectId: project.id, cursor: null, limit: 50 } });
  expect(stored.rows).toHaveLength(1); expect(stored.rows[0].id).toBe(panel.resource.threadId);
  const sidebar = page.locator('.thread-sidebar');
  await expect(sidebar.locator('#thread-sidebar-project')).toHaveValue(project.id);
  await expect(sidebar.locator(`button[data-thread-id="${panel.resource.threadId}"]`)).toBeVisible();
  const selectedIds = await ids();
  await sidebar.locator(`button[data-thread-id="${panel.resource.threadId}"]`).click();
  expect(await ids()).toEqual(selectedIds);
  const result = { id: created.id, threadId: panel.resource.threadId, projectId: project.id };
  opened.push(result); return result;
}
async function closeFrame() {
  const closing = Promise.all([page.waitForEvent('close', { timeout: 15000 }), exitWithin(15000)]); void closing.catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  await closing; frameClosed = true;
}
try {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (spawnError) throw new Error(`Cannot start owned proof app (${spawnError.code}).`);
    if (app.exitCode !== null) throw new Error('Owned proof app exited during startup.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
    catch { await delay(250); }
  }
  if (!browser) throw new Error('Owned proof WebView startup exceeded 30 seconds.');
  const context = browser.contexts()[0];
  const mainDeadline = Date.now() + 10000;
  while (Date.now() < mainDeadline && !page) {
    page = context.pages().find(candidate => {
      try { const url = new URL(candidate.url()); return url.hostname === 'tauri.localhost' && ['/', '/index.html'].includes(url.pathname); }
      catch { return false; }
    });
    if (!page) await delay(100);
  }
  if (!page) throw new Error('Owned main workspace WebView was not found.');
  page.setDefaultTimeout(15000);
  await page.waitForURL(url => url.hostname === 'tauri.localhost');
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const storage = await ipc('storage_location');
  if (pathKey(storage.directory) !== pathKey(data)) throw new Error('Synthetic storage identity mismatch; no UI was touched.');
  owned = true; page.on('pageerror', () => { pageErrors++; });
  await page.evaluate(() => {
    const bridge=window.__TAURI_INTERNALS__; const original=bridge.invoke.bind(bridge);
    bridge.invoke=(command,...args)=>original(command,...args).then(value=>{
      if(command==='resources_telemetry'&&value?.policy){
        const policy=value.policy;
        globalThis.__newSessionObservedPolicy={stage:policy.stage,reason:policy.reason,defer:policy.defer_new_starts,
          hostResponsive:policy.host_responsive,callerAge:policy.caller_age_ms,monitorAge:policy.monitor_age_ms};
      }
      return value;
    });
  });
  expect(await sessions()).toEqual([]);
  await expect(page.locator('#agents')).toHaveCount(0); await expect(page.locator('#session-index')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Check', exact: true })).toHaveCount(0);
  evidence.checks.noPreparatoryControlsOrAutoStartup = true;
  const first = await openFromDialog(projectPaths[0], 1);
  evidence.checks.oneOpenSavedProjectThreadAndView = true;
  if (harness === 'claude_code') {
    await expect.poll(async () => {
      const status = await ipc('sidecar_status'); return status.healthy && ['ready','busy'].includes(status.phase);
    }, {timeout:10000}).toBe(true);
    evidence.checks.localServicePreparedAutomatically = true;
  } else {
    await openFromDialog(projectPaths[1], 2);
    const stableIds = await ids(); const threadIds = opened.map(item => item.threadId).sort();
    const selector = page.locator('#thread-sidebar-project');
    for (const item of opened) {
      await expect(selector).toBeEnabled(); await selector.selectOption(item.projectId);
      await expect(page.locator(`.thread-sidebar button[data-thread-id="${item.threadId}"]`)).toBeVisible();
      expect(await ids()).toEqual(stableIds);
      expect((await savedLayout()).views.map(view => view.resource.threadId).filter(Boolean).sort()).toEqual(threadIds);
    }
    await expect(selector).toBeEnabled(); await selector.selectOption(first.projectId);
    await page.locator(`.thread-sidebar button[data-thread-id="${first.threadId}"]`).click();
    await ipc('terminal_write', { id: first.id, data: Array.from(Buffer.from("Write-Output ('SHINATAMA_' + 'HEALTHY')\r")) });
    await expect(pane(first.id).locator('.xterm-accessibility')).toContainText('SHINATAMA_HEALTHY');
    evidence.checks.twoProjectThreadsRemainInOneLayout = true;
    evidence.checks.projectSelectionDoesNotRestartAndHealthyShellResponds = true;
  }
  expect(pageErrors).toBe(0);
  // Stop only sessions in this verified private app. No terminal contents or
  // screenshots are recorded, including the original Claude sign-in screen.
  for (const item of opened) await ipc('terminal_stop', { id: item.id });
  for (const item of opened) {
    await expect(pane(item.id).locator('.exit-banner')).toBeVisible();
    await pane(item.id).getByRole('button', { name: 'Close terminal', exact: true }).click();
  }
  await expect.poll(ids).toEqual([]);
  await closeFrame(); expect(app.exitCode).toBe(0);
  evidence.checks.ownedSessionStopAndFrameClose = true;
  evidence.status = 'passed'; await save();
  console.log(`PASS: New session -> synthetic project -> ${harness} -> Open; durable thread/view and owned cleanup.`);
} catch (error) {
  evidence.status = 'failed'; evidence.failure = String(error.message).split('\n')[0].slice(0, 512);
  if (owned && page && !page.isClosed()) evidence.diagnostic = await within(redactDiagnostics(), 5000, 'Redacted startup diagnostics').catch(() => 'unavailable');
  await save(); throw error;
} finally {
  if (owned && page && !page.isClosed() && !frameClosed) {
    try {
      for (const session of await sessions()) await ipc('terminal_stop', { id: session.id });
      const dialog = page.locator('#new-session-dialog');
      if (await dialog.isVisible() && await dialog.getByRole('button', { name: 'Cancel', exact: true }).isEnabled()) {
        await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
      }
      await closeFrame();
    } catch { /* Only this proof's owned app is eligible for fallback shutdown. */ }
  }
  if (app.exitCode === null && app.signalCode === null) { app.kill(); await exitWithin(3000).catch(() => {}); }
  await browser?.close().catch(() => {});
}
