// ADR-0056 on the built exe, Windows only: the browser host (shinatama-moli.exe) is not started until a page is asked for, is one process
// that is a child of the app and owns no TCP socket, serves a local document from its folder (and refuses what is not an .html file), frames come over the
// channel as JPEG, and the host ends with the app. These are the invariants of the Rust side, driven over the debugging port with the page's
// own `invoke`; no selector is used.
// Not here yet, because the screen waits for Sigrid's FrameSurface (src/screens/browser/surface.tsx) and the Files inspector's "Browser"
// button is the inspector's to port: the pixels of the canvas (the CSS of the local page drawn, a typed form sent, the click-to-frame latency
// of the legacy smoke). They are added to this file when those two exist; the browser tab's screen is proved by `npm run test:browser`.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/browser-native.mjs
// It has not been run in a cloud session (no Windows here).
import { chromium } from '@playwright/test';
import { execFileSync, spawn } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createServer as createNet } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
if (process.platform !== 'win32') throw new Error('Windows required.');

const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-browser-'));
const site = resolve(fixture, 'project', 'site');
await mkdir(resolve(site, 'css'), { recursive: true });
await writeFile(resolve(site, 'index.html'), '<!doctype html><link rel="stylesheet" href="css/a.css"><h1>Local page</h1>');
await writeFile(resolve(site, 'css', 'a.css'), 'html,body{margin:0;background:#c00000}');
await writeFile(resolve(fixture, 'project', 'outside.txt'), 'secret');
await mkdir(resolve(fixture, 'data'));

const reservation = createNet();
await new Promise(accept => reservation.listen(0, '127.0.0.1', accept));
const port = reservation.address().port;
await new Promise(accept => reservation.close(accept));
const app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: {
    ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'), WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  },
});
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`); };
const ps = command => execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', command], { encoding: 'utf8', windowsHide: true }).trim();
const hostPids = () => ps(`Get-CimInstance Win32_Process -Filter "Name='shinatama-moli.exe'" | Where-Object { $_.ParentProcessId -eq ${app.pid} } | ForEach-Object { $_.ProcessId }`).split(/\s+/).filter(Boolean).map(Number);
const until = async (test, ms = 15000) => { for (const deadline = Date.now() + ms; Date.now() < deadline; await delay(100)) { try { const value = await test(); if (value) return value; } catch { /* not yet */ } } return null; };

let browser;
try {
  for (const deadline = Date.now() + 30000; !browser && Date.now() < deadline;) {
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); } catch { await delay(500); }
  }
  if (!browser) throw new Error('The window did not start in time.');
  const page = await until(() => browser.contexts()[0].pages().find(one => one.url().includes('tauri.localhost') && !one.url().includes('widget.html')));
  const ipc = (command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
  await until(() => ipc('terminal_list'), 30000);
  check('no browser host before a page is asked', hostPids().length === 0);

  const opened = await ipc('browser_open', { file: resolve(site, 'index.html') });
  check('a local document opens on the host\'s own address', /^http:\/\/127\.0\.0\.1:\d+\/[0-9a-f]{32}\/index\.html$/.test(opened.url), opened.url);
  const hosts = hostPids();
  check('one host process, a child of the app', hosts.length === 1, hosts.join(','));
  const tcp = hosts.length ? ps(`(Get-NetTCPConnection -OwningProcess ${hosts[0]} -ErrorAction SilentlyContinue | Measure-Object).Count`) : '?';
  check('the host owns no TCP socket', tcp === '0', tcp);

  // A file that is not an .html document is refused, and a closed page is no longer known.
  let refused = false;
  try { await ipc('browser_open', { file: resolve(fixture, 'project', 'outside.txt') }); } catch { refused = true; }
  check('a file that is not a document is refused', refused);
  await ipc('browser_close', { page: opened.id });
  check('a closed page is no longer known', await ipc('browser_info', { page: opened.id }).then(() => false, () => true));

  // The host ends with the app.
  await browser.close().catch(() => undefined);
  browser = undefined;
  app.kill();
  const gone = await until(() => hosts.every(pid => ps(`(Get-Process -Id ${pid} -ErrorAction SilentlyContinue | Measure-Object).Count`) === '0'), 10000);
  check('killing the app ends the host', !!gone);
} finally {
  await browser?.close().catch(() => undefined);
  if (app.exitCode === null) app.kill();
}
const failed = results.filter(ok => !ok).length;
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exitCode = failed ? 1 : 0;
