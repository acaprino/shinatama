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
import { dragTab, startSession } from './session-flow.mjs';

if (process.platform !== 'win32') throw new Error('This test requires Windows.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-threads-'));
const projectPath = resolve(fixture, 'project'); await mkdir(projectPath);
const shots = process.env.SHOTS ? resolve(process.env.SHOTS) : null; if (shots) await mkdir(shots, { recursive: true });
const shot = async name => { if (shots) await page.screenshot({ path: resolve(shots, name + '.png') }); };
const freePort = async () => {
  const reservation = createServer();
  await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
  const found = reservation.address().port;
  await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
  return found;
};
let diagnostics = '';
let app, browser, page;
/** Starts the app on the fixture's data and webview profile, so a second launch is a restart. */
const launch = async () => {
  const port = await freePort();
  app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1',
      WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile') },
  });
  const mine = app;
  mine.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-32768); });
  browser = undefined;
  for (let attempt = 0; attempt < 60; attempt++) {
    if (mine.exitCode !== null) throw new Error('Desktop exited: ' + mine.exitCode + ' ' + diagnostics);
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:' + port, { timeout: 1000 }); break; } catch { await delay(500); }
  }
  if (!browser) throw new Error('WebView2 unavailable. ' + diagnostics);
  const context = browser.contexts()[0];
  const mainPage = () => context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'));
  for (const deadline = Date.now() + 30000; !mainPage() && Date.now() < deadline;) await delay(250);
  page = mainPage(); if (!page) throw new Error('Main window did not appear.');
  page.on('pageerror', error => errors.push(error.message));
};
const errors = [];
try {
  await launch();
  let rows, tabs;
  const bind = () => { rows = page.locator('.thread-sidebar-list .thread-sidebar-anchor'); tabs = page.locator('.wm-tab'); };
  bind();
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

  // 4. A new thread is turned on beside the first: the next session joins it; the first thread's tabs stay and keep running.
  const threadA = await rows.first().getAttribute('data-workset-id');
  const rowOf = id => page.locator(`.thread-sidebar-anchor[data-workset-id="${id}"]`);
  await page.locator('.thread-sidebar-group-add').first().click();
  await expect(rows).toHaveCount(2, { timeout: 15000 });
  await expect(tabs).toHaveCount(2);
  expect(await terminals()).toBe(2);
  await open();
  await expect(tabs).toHaveCount(3, { timeout: 15000 });
  expect(await terminals()).toBe(3);
  await shot('2-two-threads-on');
  const threadB = await rows.evaluateAll((list, a) => list.map(row => row.dataset.worksetId).find(id => id !== a), threadA);
  await expect(rowOf(threadB).locator('.thread-sidebar-state')).toHaveText('1 session · 1 running');
  await expect(rowOf(threadA).locator('.thread-sidebar-state')).toHaveText('2 sessions · 2 running');

  // 5. Turning a thread off takes its tabs away and stops nothing; turning it on brings them back.
  await rowOf(threadA).click();
  await expect(tabs).toHaveCount(1, { timeout: 15000 });
  expect(await terminals()).toBe(3);
  await rowOf(threadA).click();
  await expect(tabs).toHaveCount(3, { timeout: 15000 });
  await rowOf(threadB).click();
  await expect(tabs).toHaveCount(2, { timeout: 15000 });
  await expect(rowOf(threadB)).toHaveAttribute('aria-pressed', 'false');
  expect(await terminals()).toBe(3);
  await shot('3-first-thread-only');

  // 6. Closing a session stops it and keeps it: no tab, listed under Closed sessions, never deleted.
  await page.locator('.wm-tab-close').first().click({ force: true });
  await expect(page.locator('#confirm-title')).toHaveText('Close session?');
  await page.locator('#confirm-dialog button[value="stop"]').click();
  await expect(tabs).toHaveCount(1, { timeout: 15000 });
  expect(await terminals()).toBe(2);
  const closed = page.locator('.thread-sidebar-details');
  await page.locator(`.thread-sidebar-anchor[data-workset-id="${threadA}"] ~ .thread-sidebar-more`).click();
  await expect(closed.locator('.thread-sidebar-closed-head')).toHaveText('Closed sessions (1)', { timeout: 15000 });
  await expect(page.locator('.thread-sidebar-anchor[aria-pressed="true"] .thread-sidebar-state')).toHaveText('2 sessions · 1 running');
  await shot('4-closed-session-kept');

  // 7. Reopen puts it back as a tab that says it is not running; nothing starts.
  await closed.locator('button[data-restore]').click();
  await expect(page.locator('.wm-tab', { hasText: 'not running' })).toHaveCount(1, { timeout: 15000 });
  await expect(tabs).toHaveCount(2);
  await expect(closed.locator('.thread-sidebar-closed')).toBeHidden();
  expect(await terminals()).toBe(2);
  await expect(page.locator('.session-idle')).toBeVisible();
  await shot('5-reopened-session');

  // 8. The x on a tab that is not running closes it again without asking; Forget deletes it after a second click.
  await page.locator('.wm-tabwrap', { hasText: 'not running' }).locator('.wm-tab-close').click({ force: true });
  await expect(closed.locator('.thread-sidebar-closed-head')).toHaveText('Closed sessions (1)', { timeout: 15000 });
  await expect(tabs).toHaveCount(1);
  const forget = closed.locator('.thread-sidebar-closed li button').nth(1);
  await forget.click(); await expect(forget).toHaveText('Sure?'); await forget.click();
  await expect(closed.locator('.thread-sidebar-closed')).toBeHidden({ timeout: 15000 });
  await expect(page.locator('.thread-sidebar-anchor[aria-pressed="true"] .thread-sidebar-state')).toHaveText('1 session · 1 running', { timeout: 15000 });

  // 9. One layout for the window: split the first thread's tabs in two columns, turn the second thread on and off; the columns stay.
  const panes = () => page.locator('.wm-pane').evaluateAll(list => list.map(pane => [...pane.querySelectorAll('[role="tab"]')].map(tab => tab.dataset.viewId)));
  await open();
  await expect(tabs).toHaveCount(2, { timeout: 15000 });
  await expect(page.locator('.wm-pane')).toHaveCount(1);
  const selected = page.locator('.wm-pane [role="tab"][aria-selected="true"]');
  const split = page.locator('.wm-pane').first();
  await dragTab(page, split.locator('.wm-tab').last(), split, 'right');
  await expect(page.locator('.wm-pane')).toHaveCount(2, { timeout: 15000 });
  const columns = await panes();
  expect(columns.map(group => group.length), 'tabs per column of the first thread').toEqual([1, 1]);
  expect(new Set(columns.flat()).size).toBe(2);
  expect(await selected.count()).toBe(2);
  await shot('6-first-thread-two-columns');
  await rowOf(threadB).click();
  await expect(tabs).toHaveCount(3, { timeout: 15000 });
  await expect(page.locator('.wm-pane')).toHaveCount(2);
  expect(await terminals()).toBe(3);
  await shot('7-second-thread-on-beside');
  await rowOf(threadB).click();
  await expect(tabs).toHaveCount(2, { timeout: 15000 });
  expect(await panes(), 'the first thread columns after the round trip').toEqual(columns);
  expect(await terminals()).toBe(3);
  await shot('8-columns-kept');
  const identify = () => rows.evaluateAll(list => list.map(row => row.dataset.worksetId + ' ' + row.querySelector('.thread-sidebar-title').textContent));
  const listBefore = await identify();
  expect(listBefore).toHaveLength(2);

  // 10. A clean restart turns on the threads that were on, with the window's two columns, their sessions not running, nothing started.
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  await expect(page.locator('#confirm-title')).toHaveText('Close workspace?');
  const gone = new Promise(accept => app.once('exit', accept));
  await page.locator('#confirm-dialog button[value="stop"]').click();
  expect(await Promise.race([gone, delay(20000).then(() => 'timeout')])).toBe(0);
  await browser.close().catch(() => {});
  await launch(); bind();
  await expect(rows).toHaveCount(2, { timeout: 30000 });
  expect(await identify()).toEqual(listBefore);
  await expect(page.locator('.thread-sidebar-anchor[aria-pressed="true"]')).toHaveCount(1);
  await expect(page.locator('.wm-pane')).toHaveCount(2, { timeout: 15000 });
  await expect(tabs).toHaveCount(2);
  await expect(page.locator('.wm-tab', { hasText: 'not running' })).toHaveCount(2);
  expect((await panes()).map(group => group.length)).toEqual([1, 1]);
  expect(await terminals()).toBe(0);
  await expect(page.locator('.thread-sidebar-anchor[aria-pressed="true"] .thread-sidebar-state')).toHaveText('2 sessions');
  await shot('9-after-restart');
  await delay(1500);
  expect(await terminals(), 'nothing starts by itself after a restart').toBe(0);
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
