// Real WebView + Rust IPC, disposable data home. A synthetic project and an empty thread whose sessions talk (no terminal, no agent is started);
// the team commands and the room's snapshot are stubbed in the page, so the map is checked on its own: see team-map-drive.mjs for what it drives.
import { chromium, expect as plainExpect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { driveBoxesFit, driveNotices, driveTeamMap, driveTemplateListed, driveTemplateSave, driveTemplateUse, installStub } from './team-map-drive.mjs';

// A cold start (the first run after a fresh build: the program is scanned, the WebView profile made) takes longer than the 5 s an assertion waits by default.
const expect = plainExpect.configure({ timeout: 15000 });
if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const fixture = await mkdtemp(resolve(process.env.SHINATAMA_SMOKE_TEMP || tmpdir(), 'shinatama-team-map-'));
const data = resolve(fixture, 'data'); const projectPath = resolve(fixture, 'synthetic-project');
await mkdir(projectPath, { recursive: true });
const executable = process.env.SHINATAMA_EXE || resolve('target/debug/shinatama-desktop.exe');
const reservation = createServer();
await new Promise((yes, no) => { reservation.once('error', no); reservation.listen({ host: '127.0.0.1', port: 0 }, yes); });
const port = reservation.address().port;
await new Promise(yes => reservation.close(yes));
const app = spawn(executable, [], { windowsHide: true, stdio: 'ignore', env: { ...process.env, SHINATAMA_HOME: data,
  WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` } });
let browser, page;
const errors = [];
const ipc = (command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
try {
  for (const until = Date.now() + 40000; !browser && Date.now() < until;) {
    if (app.exitCode !== null) throw new Error('App exited before WebView startup');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); } catch { await delay(200); }
  }
  if (!browser) throw new Error('WebView startup timed out');
  for (const until = Date.now() + 15000; !page && Date.now() < until;) {
    page = browser.contexts()[0].pages().find(p => new URL(p.url()).hostname === 'tauri.localhost');
    if (!page) await delay(100);
  }
  if (!page) throw new Error('Main WebView missing');
  page.setDefaultTimeout(15000);
  page.on('pageerror', error => { errors.push(error.message.slice(0, 1024)); });
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const project = await ipc('project_select', { path: projectPath });
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');

  // A thread of the project whose sessions talk: only then does its row offer Team beside Chat.
  const sidebar = page.locator('.thread-sidebar');
  await sidebar.locator(`.thread-sidebar-group[data-project-id="${project.id}"] .thread-sidebar-group-add`).click();
  const anchor = sidebar.locator('button.thread-sidebar-anchor');
  await expect(anchor).toHaveCount(1);
  const worksetId = await anchor.getAttribute('data-workset-id');
  await expect(sidebar.locator('.thread-sidebar-team')).toBeHidden();
  await ipc('thread_chat_set', { request: { worksetId, enabled: true } });
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await expect(sidebar.locator('.thread-sidebar-team')).toBeVisible();
  await expect(sidebar.locator('.thread-sidebar-chat')).toBeVisible();
  console.log('ok a Team button beside Chat on a talking thread');

  await installStub(page);
  await sidebar.locator('.thread-sidebar-team').click();
  await expect(page.locator('.wm-tab[data-view-id^="team-"]')).toBeVisible();
  await driveTeamMap(page, expect, message => console.log(message));
  await driveNotices(page, expect, message => console.log(message));

  // Templates run on the real backend: save in this thread, restart the window, see it listed, use it in another thread.
  const log = message => console.log(message);
  const runtime = async () => { await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime'); await installStub(page); };
  const teamOf = id => sidebar.locator('li.thread-sidebar-row', { has: page.locator(`button.thread-sidebar-anchor[data-workset-id="${id}"]`) }).locator('.thread-sidebar-team');
  await teamOf(worksetId).click();
  await driveTemplateSave(page, expect, log);
  await runtime();
  await teamOf(worksetId).click();
  await expect(page.locator('.team-map[data-view-id]').first()).toBeVisible();
  await driveTemplateListed(page, expect, log);
  await sidebar.locator(`.thread-sidebar-group[data-project-id="${project.id}"] .thread-sidebar-group-add`).click();
  await expect(sidebar.locator('button.thread-sidebar-anchor')).toHaveCount(2);
  const otherId = (await sidebar.locator('button.thread-sidebar-anchor').evaluateAll(nodes => nodes.map(n => n.dataset.worksetId))).find(id => id !== worksetId);
  await ipc('thread_chat_set', { request: { worksetId: otherId, enabled: true } });
  await runtime();
  await teamOf(otherId).click();
  await expect(page.locator('.team-map[data-view-id="team-' + otherId + '"]')).toBeVisible();
  await driveTemplateUse(page, expect, log);
  await driveBoxesFit(page, expect, log);
  expect(errors).toEqual([]);
  console.log('PASS team map native smoke');
} finally {
  try { await browser?.close(); } catch {}
  if (app.exitCode === null) { try { execFileSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' }); } catch {} }
  await rm(fixture, { recursive: true, force: true, maxRetries: 5 }).catch(() => {});
}
