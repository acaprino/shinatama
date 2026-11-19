// A session's tools may start many processes at once: a Claude Code session was stopped mid-turn
// when `npm ci` took its job past 16 processes. This proof starts 24 processes from a session in
// the real app and requires the session to stay open and keep answering.
// Run after the coordinated desktop/helper build, with TEMP outside any Git work tree.
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
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-limits-'));
const projectPath = resolve(fixture, 'project');
await mkdir(projectPath);
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
// SHINATAMA_EXE checks a packaged Shinatama.exe (helpers beside it) instead of the dev build.
const app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1',
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile') },
});
let diagnostics = '';
app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-32768); });
let browser;
try {
  for (let attempt = 0; attempt < 60; attempt++) {
    if (app.exitCode !== null) throw new Error('Desktop exited: ' + app.exitCode + '\n' + diagnostics);
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:' + port, { timeout: 1000 }); break; }
    catch { await delay(500); }
  }
  if (!browser) throw new Error('WebView2 unavailable.\n' + diagnostics);
  const context = browser.contexts()[0];
  const mainPage = () => context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'));
  for (const deadline = Date.now() + 30000; !mainPage() && Date.now() < deadline;) await delay(250);
  const page = mainPage();
  if (!page) throw new Error('Main window did not appear.');
  const ipc = command => page.evaluate(name => window.__TAURI_INTERNALS__.invoke(name), command);
  await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 30000 });

  await (await startSession(page, { project: projectPath, harness: 'powershell' })).done(30000);
  const [session] = await ipc('terminal_list');
  if (!session) throw new Error('The PowerShell session did not open.');
  const pane = page.locator('.wm-pane:has([data-view-id="' + session.id + '"])');
  const screen = pane.locator('.xterm-accessibility');
  await expect(screen).toContainText('PS ', { timeout: 30000 });
  const type = async command => {
    await pane.locator('.xterm-helper-textarea').focus();
    await page.keyboard.type(command); await page.keyboard.press('Enter');
  };
  // Markers are split in the command so its echo never matches the output.
  await type("$p = 1..24 | % { Start-Process -PassThru -WindowStyle Hidden ping.exe -ArgumentList '-n','6','127.0.0.1' }; "
    + "'STARTED'+'='+$p.Count; $p | Wait-Process; 'ALL'+'-DONE'");
  await expect(screen).toContainText('STARTED=24', { timeout: 30000 });
  await expect(screen).toContainText('ALL-DONE', { timeout: 60000 });
  await type("'STILL'+'-OPEN'");
  await expect(screen).toContainText('STILL-OPEN', { timeout: 15000 });
  expect((await ipc('terminal_list')).map(item => item.id)).toContain(session.id);
  await expect(pane.locator('.exit-banner')).toHaveCount(0);
  console.log(JSON.stringify({ ok: true, session: session.id, started: 24 }));
} finally {
  if (browser) {
    for (const context of browser.contexts()) for (const page of context.pages()) {
      await page.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')).catch(() => {});
    }
    await browser.close().catch(() => {});
  }
  if (app.exitCode === null) app.kill();
}
