// ADR-0055 v3 through the real app: the window's layout `main`, left marked by a Shinatama that was
// killed, is taken over at the next start without a button or a notice, and is saved again.
// Run after the desktop build. SHINATAMA_EXE names another executable.
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:net';
import { startSession } from './session-flow.mjs';

if (process.platform !== 'win32') throw new Error('This test requires Windows.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-takeover-'));
const projectPath = resolve(fixture, 'project'); await mkdir(projectPath);
const freePort = async () => {
  const reservation = createServer();
  await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
  const found = reservation.address().port;
  await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
  return found;
};
const until = async (read, milliseconds, message) => {
  for (const deadline = Date.now() + milliseconds; ;) {
    const value = await read().catch(() => null); if (value) return value;
    if (Date.now() >= deadline) throw new Error(message);
    await delay(250);
  }
};
let app, browser, page, diagnostics = '';
const launch = async () => {
  const port = await freePort();
  app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1',
      WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile') },
  });
  app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-16384); });
  browser = await until(() => chromium.connectOverCDP('http://127.0.0.1:' + port, { timeout: 1000 }), 30000, 'WebView2 unavailable. ' + diagnostics);
  const context = browser.contexts()[0];
  page = await until(async () => context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html')), 30000, 'Main window did not appear.');
};
const ownership = () => page.evaluate(() => window.__TAURI_INTERNALS__.invoke('global_workspace_load', { request: { workspaceId: 'main' } }).then(snapshot => snapshot?.ownership ?? null));
try {
  await launch();
  const flow = await startSession(page, { project: projectPath, harness: 'powershell' }); await flow.done();
  await until(async () => await ownership() === 'ours', 15000, 'The first window never owned its layout.');

  // Killed, as a crash or Task Manager would: nothing releases the layout.
  await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('terminal_shutdown')).catch(() => {});
  await browser.close().catch(() => {});
  const gone = new Promise(accept => app.once('exit', accept)); app.kill(); await gone;

  await launch();
  await until(async () => await ownership() === 'ours', 20000, 'The layout of the killed window was not taken over.');
  await delay(1500);
  const notice = await page.locator('.im-notice-text').allTextContents();
  if (notice.some(text => /another|arrangement|layout/i.test(text))) throw new Error('A notice was shown: ' + notice.join(' | '));
  if (await page.locator('#recover-layout').count()) throw new Error('A Recover saved layout button was shown.');
  console.log('layout-takeover smoke: ok');
} finally {
  if (browser) {
    await page?.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')).catch(() => {});
    await browser.close().catch(() => {});
  }
  if (app?.exitCode === null) { const gone = new Promise(accept => app.once('exit', accept)); app.kill(); await gone; }
  await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 }).catch(() => {});
}
