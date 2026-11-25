// The Workspace on the built exe, Windows only: what the browser cannot prove. A real shell runs in a terminal of the window (ConPTY), its output
// and what is typed go through Sigrid's Terminal, the strip of its lines is drawn at the first start, a path of a local .html page printed in it is a
// link that opens a browser tab (ADR-0056 v2), the resources badge gets its first answer from the backend, and the layout of a window that was killed
// (a crash, Task Manager) is taken over at the next start without a notice (ADR-0055 v3). Driven over the debugging port, by roles and names only;
// the session is started with the page's own `invoke` and the page is loaded again, which reconnects the terminals Rust holds.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/workspace.mjs
// It has not been run in a cloud session (no Windows here), and it stands for the legacy minimap, terminal-link and layout-takeover smokes, and for the layout part of desktop-smoke.
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
if (process.platform !== 'win32') throw new Error('Windows required.');

const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-workspace-'));
const project = resolve(fixture, 'project');
await mkdir(resolve(project, 'docs'), { recursive: true });
await writeFile(resolve(project, 'docs', 'a.html'), '<!doctype html><title>Local page</title><h1>Local page</h1>');

const freePort = async () => {
  const reservation = createServer();
  await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
  const found = reservation.address().port;
  await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
  return found;
};
const until = async (read, milliseconds, message) => {
  for (const deadline = Date.now() + milliseconds; ;) {
    const value = await Promise.resolve().then(read).catch(() => null);
    if (value) return value;
    if (Date.now() >= deadline) throw new Error(message);
    await delay(250);
  }
};
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`); };

let app, browser, page, diagnostics = '';
const launch = async () => {
  const port = await freePort();
  app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
    env: {
      ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'), WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
    },
  });
  app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-16384); });
  browser = await until(() => chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }), 30000, `WebView2 unavailable. ${diagnostics}`);
  page = await until(() => browser.contexts()[0].pages().find(one => one.url().includes('tauri.localhost') && !one.url().includes('widget.html')), 30000, 'The main window did not appear.');
};
const ipc = (command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const ownership = () => ipc('global_workspace_load', { request: { workspaceId: 'main' } }).then(snapshot => snapshot?.ownership ?? null);
const stop = async () => {
  await browser?.close().catch(() => undefined);
  if (app && app.exitCode === null) { const gone = new Promise(accept => app.once('exit', accept)); app.kill(); await gone; }
};

try {
  await launch();
  await page.getByRole('navigation', { name: 'Threads' }).waitFor({ timeout: 30000 });

  // A shell runs: the page is loaded again, which reconnects the terminals Rust holds.
  await ipc('shell_open', { cwd: project, rows: 24, cols: 80 });
  await page.reload();
  const shell = page.getByRole('textbox', { name: 'Terminal of PowerShell' });
  await shell.waitFor({ state: 'attached', timeout: 30000 });
  await shell.focus();
  await page.keyboard.type('echo workspace-native-ok');
  await page.keyboard.press('Enter');
  check('what is typed reaches the shell and its answer is drawn', !!await until(() => page.getByRole('tabpanel').first().textContent().then(text => (text.match(/workspace-native-ok/g) ?? []).length >= 2), 20000, 'no echo').catch(() => null));

  // The strip of the lines is there as soon as the session is on screen, a scroll bar for the keyboard and for screen readers.
  const strip = page.getByRole('scrollbar', { name: 'Lines of the terminal' }).first();
  check('the strip of the lines is drawn at the first start', await strip.isVisible());

  // The resources badge gets its first answer from the backend (the window reports its own measures once a second).
  check('the resources badge says what the backend sees', !!await until(() => page.getByText(/^Resources: (available|reduced activity)$/).first().isVisible(), 20000, 'still checking').catch(() => null));

  // A local page printed in the terminal is a link: a press opens it in a browser tab, also when the path is longer than the terminal is wide.
  await page.keyboard.type('echo docs\\a.html');
  await page.keyboard.press('Enter');
  const link = page.getByRole('tabpanel').first().getByText('docs\\a.html').last();
  await link.hover();
  await link.click();
  check('a press on a path of a local .html page opens a browser tab', !!await until(() => page.getByRole('tab', { name: /a\.html|Local page/ }).first().isVisible(), 20000, 'no browser tab').catch(() => null));

  // Killed, as a crash or Task Manager would: nothing releases the layout. The next start takes it over, says nothing, and brings the tabs back.
  await until(async () => await ownership() === 'ours', 15000, 'The first window never owned its layout.');
  await ipc('terminal_shutdown').catch(() => undefined);
  await browser.close().catch(() => undefined);
  const gone = new Promise(accept => app.once('exit', accept));
  app.kill();
  await gone;
  await launch();
  check('the layout of the killed window is taken over', !!await until(async () => await ownership() === 'ours', 20000, 'not taken over').catch(() => null));
  await page.getByRole('tab', { name: /PowerShell/ }).first().waitFor({ timeout: 20000 });
  await delay(1500);
  const notices = await page.getByRole('status', { name: 'Notices' }).allTextContents();
  check('it said nothing of another window or of the arrangement', !notices.some(text => /another|arrangement|layout/i.test(text)), notices.join(' | '));
} finally {
  await page?.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')).catch(() => undefined);
  await stop();
  await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 }).catch(() => undefined);
}
const failed = results.filter(ok => !ok).length;
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exitCode = failed ? 1 : 0;
