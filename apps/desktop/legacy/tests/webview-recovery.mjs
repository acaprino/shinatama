// Windows proof: the window comes back when WebView2 loses its browser process, where it was,
// with the running terminal reattached; and a quiet terminal does not flood the IPC.
// Run after `npm run build` and the desktop/helper build, with TEMP outside any git project.
import { chromium } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:net';

if (process.platform !== 'win32') throw new Error('This test requires Windows.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-recovery-'));
const projectPath = resolve(fixture, 'project');
await mkdir(projectPath);
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
const port = reservation.address().port;
await new Promise(accept => reservation.close(accept));
const app = spawn(resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: 'ignore',
  env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1',
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile') },
});
const nativeWindow = (place = false) => JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-File',
  resolve(root, 'apps/desktop/tests/webview-recovery.ps1'), '-AppProcessId', String(app.pid), ...(place ? ['-Place'] : [])],
{ encoding: 'utf8', windowsHide: true }));
async function connect() {
  let browser;
  for (let attempt = 0; attempt < 60 && !browser; attempt++) {
    if (app.exitCode !== null) throw new Error('Desktop exited: ' + app.exitCode);
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:' + port, { timeout: 1000 }); } catch { await delay(500); }
  }
  if (!browser) throw new Error('WebView2 unavailable.');
  const context = browser.contexts()[0];
  const mainPage = () => context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'));
  for (const deadline = Date.now() + 30000; !mainPage() && Date.now() < deadline;) await delay(250);
  const page = mainPage();
  if (!page) throw new Error('Main window did not appear.');
  await page.waitForFunction(() => document.querySelector('#workspace') && window.__TAURI_INTERNALS__);
  return { browser, page };
}
const ipc = (page, command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const check = (condition, message) => { if (!condition) throw new Error(message); };
try {
  let { browser, page } = await connect();
  const session = await ipc(page, 'terminal_start', { cwd: projectPath, rows: 24, cols: 80 });
  await page.reload();
  await page.waitForSelector('.terminal-view');
  await delay(3000); // the shell's first output has been drained
  let reads = 0;
  const count = request => { if (request.url().endsWith('/terminal_read')) reads++; };
  page.on('request', count); await delay(5000); page.off('request', count);
  check(reads >= 2 && reads <= 12, `A quiet terminal made ${reads} reads in 5 s; about 5 expected.`);

  const before = nativeWindow(true);
  check(before.found && before.visible && before.drawn > 0, 'The window was not drawn before the loss: ' + JSON.stringify(before));
  const lost = String(before.browser);
  check(before.browser > 0, 'Browser process not found.');
  await browser.close().catch(() => {});
  execFileSync('taskkill.exe', ['/F', '/PID', lost], { windowsHide: true });

  let after;
  for (const deadline = Date.now() + 20000; Date.now() < deadline;) {
    await delay(500);
    check(app.exitCode === null, 'The application ended with its browser process.');
    after = nativeWindow();
    if (after.found && after.visible && after.drawn > 0 && after.browser > 0) break;
  }
  check(after?.drawn > 0, 'The window stayed empty after the loss: ' + JSON.stringify(after));
  check(after.x === before.x && after.y === before.y && after.width === before.width && after.height === before.height,
    `The window moved: ${JSON.stringify(before)} -> ${JSON.stringify(after)}`);
  ({ browser, page } = await connect());
  await page.waitForSelector('.terminal-view');
  const sessions = await ipc(page, 'terminal_list');
  check(sessions.length === 1 && sessions[0].id === session.id, 'The running terminal was not kept: ' + JSON.stringify(sessions));
  check(await page.locator('.terminal-view').count() === 1, 'The terminal was not reattached to the page.');
  await ipc(page, 'terminal_shutdown');
  await browser.close().catch(() => {});
  check(after.browser !== before.browser, 'The browser process was not replaced.');
  console.log(JSON.stringify({ ok: true, quietReadsIn5s: reads, before, after }));
} finally {
  try { execFileSync('taskkill.exe', ['/F', '/T', '/PID', String(app.pid)], { windowsHide: true, stdio: 'ignore' }); } catch { /* already gone */ }
}
