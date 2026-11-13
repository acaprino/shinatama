// ADR-0055 through the real app: a thread is a set of sessions with its own layout; the window
// shows all and only the sessions of the open thread; a clicked tab leaves the focus in its terminal.
// Run after the desktop build. SHINATAMA_EXE names another executable; SHOTS a folder for captures.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:net';
import { startSession } from './session-flow.mjs';

if (process.platform !== 'win32') throw new Error('This test requires Windows.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-threads-'));
const projectPath = resolve(fixture, 'project'); await mkdir(projectPath);
const shots = process.env.SHOTS ? resolve(process.env.SHOTS) : null; if (shots) await mkdir(shots, { recursive: true });
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
const app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1',
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile') },
});
let diagnostics = ''; app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-32768); });
let browser, page;
const shot = async name => { if (shots) await page.screenshot({ path: resolve(shots, name + '.png') }); };
try {
  for (let attempt = 0; attempt < 60; attempt++) {
    if (app.exitCode !== null) throw new Error('Desktop exited: ' + app.exitCode + '\n' + diagnostics);
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:' + port, { timeout: 1000 }); break; } catch { await delay(500); }
  }
  if (!browser) throw new Error('WebView2 unavailable.\n' + diagnostics);
  const context = browser.contexts()[0];
  const mainPage = () => context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'));
  for (const deadline = Date.now() + 30000; !mainPage() && Date.now() < deadline;) await delay(250);
  page = mainPage(); if (!page) throw new Error('Main window did not appear.');
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  const rows = page.locator('.thread-sidebar-list .thread-sidebar-anchor');
  const tabs = page.locator('.wm-tab');
  const open = async () => { const flow = await startSession(page, { project: projectPath, harness: 'powershell' }); await flow.done(); };
  const terminals = () => page.evaluate(() => window.__TAURI_INTERNALS__.invoke('terminal_list').then(list => list.length));

  // 1. The first session makes a thread and opens it.
  await open();
  await expect(rows).toHaveCount(1, { timeout: 15000 });
  await expect(tabs).toHaveCount(1);
  await expect(rows.first()).toHaveAttribute('aria-pressed', 'true');
  await expect(rows.first().locator('.thread-sidebar-state')).toHaveText('1 session · 1 running');

  // 2. A second session joins the open thread: one row, two tabs.
  await open();
  await expect(tabs).toHaveCount(2, { timeout: 15000 });
  await expect(rows).toHaveCount(1);
  await expect(rows.first().locator('.thread-sidebar-state')).toHaveText('2 sessions · 2 running');
  await shot('1-thread-with-two-sessions');

  // 3. A clicked tab hands the focus to its terminal and it stays there.
  for (const index of [0, 1, 0]) {
    await tabs.nth(index).click();
    await delay(400);
    const focus = await page.evaluate(() => ({ inTerminal: !!document.activeElement?.closest('.xterm'), visible: !document.activeElement?.closest('.terminal-view')?.hidden }));
    expect(focus, `focus after clicking tab ${index}`).toEqual({ inTerminal: true, visible: true });
    await expect(tabs.nth(index)).toHaveAttribute('aria-selected', 'true');
  }

  // 4. A new thread shows none of the first thread's sessions; they keep running.
  await page.locator('.thread-sidebar-new').click();
  await expect(rows).toHaveCount(2, { timeout: 15000 });
  await expect(tabs).toHaveCount(0);
  expect(await terminals()).toBe(2);
  await open();
  await expect(tabs).toHaveCount(1, { timeout: 15000 });
  expect(await terminals()).toBe(3);
  await shot('2-second-thread-one-session');
  const second = page.locator('.thread-sidebar-anchor[aria-pressed="true"]');
  await expect(second.locator('.thread-sidebar-state')).toHaveText('1 session · 1 running');

  // 5. Going back shows all and only the first thread's sessions; no process started or stopped.
  const first = page.locator('.thread-sidebar-anchor[aria-pressed="false"]');
  await first.click();
  await expect(tabs).toHaveCount(2, { timeout: 15000 });
  expect(await terminals()).toBe(3);
  await expect(page.locator('.terminal-view:not([hidden])')).toHaveCount(1);
  await shot('3-back-to-first-thread');

  // 6. A stopped session stays in its thread as a tab that says so.
  await page.locator('.wm-tab-close').first().click({ force: true });
  await page.locator('#confirm-dialog button[value="stop"]').click();
  await expect(page.locator('.wm-tab', { hasText: 'not running' })).toHaveCount(1, { timeout: 15000 });
  await expect(tabs).toHaveCount(2);
  expect(await terminals()).toBe(2);
  await page.locator('.wm-tab', { hasText: 'not running' }).click();
  await expect(page.locator('.session-idle')).toBeVisible();
  await shot('4-stopped-session-stays');
  expect(errors, 'page errors').toEqual([]);
  console.log('thread-sets smoke: ok');
} catch (error) {
  if (page && shots) await page.screenshot({ path: resolve(shots, 'failure.png') }).catch(() => {});
  console.error(diagnostics.slice(-2000));
  throw error;
} finally {
  if (browser) {
    for (const context of browser.contexts()) for (const page of context.pages()) await page.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')).catch(() => {});
    await browser.close().catch(() => {});
  }
  if (app.exitCode === null) app.kill();
}
