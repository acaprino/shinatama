// The attention widget in the real window, on Windows only: the program of the app is started with a debugging port and the two pages are
// driven over it. It proves what the mock cannot: the widget window is real, is shown without taking the focus, stays on top, is hidden when
// the alerts are off, is built again when it is gone, and goes with the main window. No agent or provider is called.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/attention-native.mjs
// It has not been run in a cloud session (no Windows here): the Verify run on Windows is where it is proved (docs/desktop-ui.md, "Tests").
import { chromium, expect } from '@playwright/test';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtemp } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
if (process.platform !== 'win32') throw new Error('Windows required.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-attention-'));
const reservation = createServer();
await new Promise((ok, no) => { reservation.once('error', no); reservation.listen(0, '127.0.0.1', ok); });
const port = reservation.address().port;
await new Promise(ok => reservation.close(ok));
const app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'], env: {
    ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'), WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  },
});
const native = () => JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-File', resolve(root, 'apps/desktop/tests/attention-window.ps1'), '-AppProcessId', String(app.pid)], { encoding: 'utf8', windowsHide: true }));
const ipc = (page, command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const rejected = async (page, command, args) => { try { await ipc(page, command, args); return false; } catch { return true; } };
const widgetWindow = () => native().windows.find(win => win.title === 'Shinatama — avvisi');
let browser;
const errors = [];
try {
  for (const deadline = Date.now() + 30000; Date.now() < deadline;) {
    if (app.exitCode !== null) throw new Error('The app exited during startup.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; } catch { await delay(250); }
  }
  if (!browser) throw new Error('The window did not start in time.');
  const context = browser.contexts()[0];
  await expect.poll(() => context.pages().filter(page => page.url().includes('tauri.localhost')).length).toBe(2);
  const widget = context.pages().find(page => page.url().includes('widget.html'));
  const main = context.pages().find(page => !page.url().includes('widget.html'));
  for (const page of [main, widget]) page.on('pageerror', error => errors.push(error.message));
  const notice = widget.getByRole('region', { name: 'Avviso di Shinatama' });

  // Hidden until there is a notice; nothing is drawn.
  expect((await ipc(main, 'attention_current')).current).toBeNull();
  expect(widgetWindow().visible).toBe(false);
  await expect(notice).toHaveCount(0);

  // A test alert: the widget is shown, on top, and does not take the focus.
  await ipc(main, 'attention_configure', { enabled: true, muted: true });
  await ipc(main, 'attention_preview', { kind: 'input' });
  await expect(notice.getByRole('heading', { name: /prova avviso/ })).toBeVisible();
  const shown = native();
  const visible = shown.windows.find(win => win.title === 'Shinatama — avvisi');
  expect(visible.visible).toBe(true);
  expect(visible.noActivate).toBe(true);
  expect(visible.topmost).toBe(true);
  expect(shown.foreground).not.toBe(visible.handle);

  // The widget page may answer its own notice and nothing else.
  expect(await rejected(widget, 'attention_configure', { enabled: false, muted: true })).toBe(true);
  expect(await rejected(widget, 'attention_dismiss', { id: 'stale' })).toBe(true);

  // Close hides the window; the alerts off hide it too and the next preview is refused.
  await notice.getByRole('button', { name: 'Chiudi avviso' }).click();
  await expect.poll(() => widgetWindow().visible).toBe(false);
  await ipc(main, 'attention_configure', { enabled: false, muted: true });
  expect(await rejected(main, 'attention_preview', { kind: 'input' })).toBe(true);
  await ipc(main, 'attention_configure', { enabled: true, muted: true });

  // The widget goes with the main window.
  expect(errors).toEqual([]);
  await ipc(main, 'terminal_shutdown');
  await ipc(main, 'workspace_allow_close');
  const exited = new Promise(ok => app.once('exit', ok));
  await ipc(main, 'plugin:window|close', { label: 'main' }).catch(() => {});
  await Promise.race([exited, delay(10000).then(() => { throw new Error('The app did not close with its main window.'); })]);
  console.log('attention native: ok');
} finally {
  if (browser) await browser.close().catch(() => {});
  if (app.exitCode === null) app.kill();
}
