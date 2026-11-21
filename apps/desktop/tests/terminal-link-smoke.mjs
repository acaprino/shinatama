// ADR-0056 v2 through the real app: a plain click on a local .html path printed in a terminal opens it
// in a Moli browser tab, also when the path is longer than the terminal is wide and wraps.
// Run after build-release.bat with SHINATAMA_EXE=release\Shinatama.exe (Moli sits beside it),
// TEMP/TMP outside any git project.
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:net';
import { startSession } from './session-flow.mjs';

if (process.platform !== 'win32') throw new Error('This test requires Windows.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-link-'));
const projectPath = resolve(fixture, 'project');
// Deep enough that the path is wider than the terminal and wraps.
const folder = resolve(projectPath, '.marionette', 'prototypes', 'a-folder-name-long-enough-to-wrap-the-terminal-line', 'and-another-one-for-good-measure-xxxxxxxxxxxxxxxx');
await mkdir(folder, { recursive: true });
const page = resolve(folder, 'chart-panel.html');
await writeFile(page, '<!doctype html><body style="margin:0;background:#c00000"><h1>chart</h1></body>');
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
    await delay(200);
  }
};
const port = await freePort();
let diagnostics = '';
const app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'), WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-8192); });
let browser, window;
try {
  browser = await until(() => chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }), 30000, 'WebView2 unavailable. ' + diagnostics);
  window = await until(async () => browser.contexts()[0].pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html')), 30000, 'Main window did not appear.');
  const flow = await startSession(window, { project: projectPath, harness: 'powershell' }); await flow.done();
  const terminal = window.locator('.terminal-view:not([hidden]) .xterm').first();
  await terminal.click();
  await window.keyboard.type(`Write-Output '${page}'`); await window.keyboard.press('Enter');

  // The rows of the accessibility tree hold the text, one element per terminal row.
  const rows = window.locator('.terminal-view:not([hidden]) .xterm-accessibility-tree > div');
  const tail = 'chart-panel.html';
  const rowIndex = await until(async () => {
    const texts = await rows.allTextContents();
    const index = texts.findIndex(text => text.includes(tail) && !text.includes('Write-Output'));
    return index >= 0 ? index + 1 : null;
  }, 15000, 'The path was not printed.') - 1;
  const texts = await rows.allTextContents();
  if (texts[rowIndex].includes(page.slice(0, 3))) throw new Error('The path fits on one row; the test needs it to wrap.');
  // Click the wrapped part, on the second row of the path.
  const screen = await terminal.locator('.xterm-screen').boundingBox();
  // The pane's footer says the terminal's size, e.g. 150×43.
  const cols = Number((await window.locator('.pane-footer').first().textContent()).match(/(\d+)×\d+/)?.[1]);
  if (!cols) throw new Error('The terminal size is not shown.');
  const width = screen.width / cols;
  const box = await rows.nth(rowIndex).boundingBox();
  const column = texts[rowIndex].indexOf(tail) + 3;
  await window.mouse.move(screen.x + (column + 0.5) * width, box.y + box.height / 2);
  await delay(300);
  await window.mouse.down(); await window.mouse.up();
  const tab = await until(async () => (await window.locator('.browser-tab').count()) > 0, 30000, 'No browser tab opened.')
    .catch(async error => { throw new Error(`${error.message} Notice: ${(await window.locator('.im-notice-text').allTextContents().catch(() => [])).join(' | ')}`); });
  const red = await until(() => window.evaluate(() => {
    const canvas = document.querySelector('.browser-tab:not([hidden]) .browser-canvas'); if (!canvas || !canvas.width) return false;
    const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data; let hit = 0;
    for (let i = 0; i < data.length; i += 16) if (Math.abs(data[i] - 192) < 40 && data[i + 1] < 40 && data[i + 2] < 40) hit++;
    return hit / (data.length / 16) > 0.3;
  }), 30000, 'The page did not render in the browser tab.');
  if (process.env.SHOTS) await window.screenshot({ path: resolve(process.env.SHOTS, 'terminal-link.png') });
  console.log(`terminal-link smoke: ok (${tab && red ? 'tab opened, page drawn' : ''})`);
} finally {
  // Every step of the teardown is bounded: a browser host still attached must not keep the test alive.
  const bounded = (work, ms = 5000) => Promise.race([work.catch(() => {}), delay(ms)]);
  if (browser) {
    await bounded(window?.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')) ?? Promise.resolve());
    await bounded(browser.close());
  }
  if (app.exitCode === null) { const gone = new Promise(accept => app.once('exit', accept)); app.kill(); await bounded(gone, 10000); }
  await bounded(rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 }), 15000);
}
process.exit(process.exitCode ?? 0);
