// ADR-0056 proof on the built exe: a local HTML file opens from the Files panel in a browser
// tab drawn by shinatama-moli.exe; the address bar loads a page whose form is filled and sent
// through the canvas; the host has no TCP socket and ends with the app. Prints the host's memory
// and the click-to-frame latency (M2, M3 first figures).
// Run after `npm run build`, `cargo build -p shinatama-desktop --features custom-protocol` and
// copying shinatama-moli.exe beside the desktop exe, with TEMP/TMP outside any git project.
import { chromium } from '@playwright/test';
import { execFileSync, spawn } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createServer as createNet } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-browser-'));
const project = resolve(fixture, 'project');
await mkdir(resolve(project, 'site', 'css'), { recursive: true });
await writeFile(resolve(project, 'site', 'index.html'), '<!doctype html><link rel="stylesheet" href="css/a.css"><h1>Local page</h1><div id="box"></div>');
await writeFile(resolve(project, 'site', 'css', 'a.css'), 'html,body{margin:0;background:#c00000} #box{width:200px;height:200px;background:#00a000}');
await writeFile(resolve(project, 'outside.txt'), 'secret');
await mkdir(resolve(fixture, 'data'));

const sent = [];
const web = createServer((request, response) => {
  const url = new URL(request.url, 'http://x');
  if (url.pathname === '/submit') { sent.push(url.searchParams.get('q')); response.end('<body style="background:#0000c0">sent</body>'); return; }
  response.setHeader('content-type', 'text/html');
  response.end('<!doctype html><body style="margin:0;background:#fff"><form action="/submit"><input id="q" name="q" autofocus style="position:absolute;left:20px;top:20px;width:300px;height:40px;font-size:24px"></form></body>');
});
await new Promise(accept => web.listen(0, '127.0.0.1', accept));
const site = `http://127.0.0.1:${web.address().port}/`;

const reservation = createNet();
await new Promise(accept => reservation.listen(0, '127.0.0.1', accept));
const port = reservation.address().port;
await new Promise(accept => reservation.close(accept));
const app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'), WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let diagnostics = '';
app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-8192); if (process.env.SMOKE_TRACE) process.stderr.write(chunk); });
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`); };
async function until(test, ms = 15000) {
  for (const deadline = Date.now() + ms; Date.now() < deadline; await delay(100)) { try { const v = await test(); if (v) return v; } catch { /* not yet */ } }
  return null;
}
const ps = command => execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', command], { encoding: 'utf8', windowsHide: true }).trim();
const hostPids = () => ps(`Get-CimInstance Win32_Process -Filter "Name='shinatama-moli.exe'" | Where-Object { $_.ParentProcessId -eq ${app.pid} } | ForEach-Object { $_.ProcessId }`).split(/\s+/).filter(Boolean).map(Number);
/** Share of canvas pixels close to an RGB colour. */
const share = (page, [r, g, b]) => page.evaluate(([r, g, b]) => {
  const canvas = document.querySelector('.browser-tab:not([hidden]) .browser-canvas'); if (!canvas || !canvas.width) return 0;
  const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data; let hit = 0;
  for (let i = 0; i < data.length; i += 16) if (Math.abs(data[i] - r) < 40 && Math.abs(data[i + 1] - g) < 40 && Math.abs(data[i + 2] - b) < 40) hit++;
  return hit / (data.length / 16);
}, [r, g, b]);

let browser;
try {
  for (const deadline = Date.now() + 30000; !browser && Date.now() < deadline;) {
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); } catch { await delay(500); }
  }
  if (!browser) throw new Error(`WebView2 unavailable.\n${diagnostics}`);
  const page = await until(() => browser.contexts()[0].pages().find(p => p.url().includes('tauri.localhost') && !p.url().includes('widget.html')));
  await page.waitForSelector('#open-files', { timeout: 30000 });
  check('no browser host before a page is asked', hostPids().length === 0);
  await page.evaluate(path => window.__TAURI_INTERNALS__.invoke('project_select', { path }), project);

  // Files panel → Browser button on a local document.
  await page.locator('#open-files').click();
  await page.locator('#files-dialog .files-row', { hasText: 'site' }).first().click();
  await page.locator('#files-dialog .files-row', { hasText: 'index.html' }).first().click();
  const button = page.locator('#files-dialog button', { hasText: 'Browser' });
  await button.waitFor({ state: 'visible', timeout: 10000 });
  const opened = Date.now();
  await button.click();
  const tabShown = await until(() => page.locator('.browser-tab').count(), 30000);
  if (!tabShown) throw new Error(`No browser tab. Notice: ${await page.locator('#notice span').textContent()}
${diagnostics}`);
  const red = await until(async () => (await share(page, [192, 0, 0])) > 0.3, 30000);
  await page.locator('.browser-tab').first().screenshot({ path: resolve(process.env.SMOKE_SHOTS ?? fixture, 'browser-local.png') }).catch(() => undefined);
  check('local document renders with its own CSS', !!red, `first frame after ${Date.now() - opened} ms (host start included)`);
  check('its folder CSS loaded (green box drawn)', (await share(page, [0, 160, 0])) > 0.005);
  const address = await page.locator('.browser-address').inputValue();
  check('address bar shows the served local address', /^http:\/\/127\.0\.0\.1:\d+\/[0-9a-f]{32}\/index\.html$/.test(address), address);
  const hosts = hostPids();
  check('one host process, a child of the app', hosts.length === 1, hosts.join(','));
  const tcp = hosts.length ? ps(`(Get-NetTCPConnection -OwningProcess ${hosts[0]} -ErrorAction SilentlyContinue | Measure-Object).Count`) : '?';
  check('host owns no TCP socket', tcp === '0', tcp);

  // Address bar → a web page; type into its field through the canvas and send the form.
  await page.locator('.browser-address').fill(site);
  await page.locator('.browser-address').press('Enter');
  check('web page drawn', !!await until(async () => (await share(page, [255, 255, 255])) > 0.5, 20000));
  const canvas = page.locator('.browser-tab .browser-canvas');
  const box = await canvas.boundingBox();
  const snapshot = () => page.evaluate(() => { const c = document.querySelector('.browser-tab .browser-canvas'); const d = c.getContext('2d').getImageData(0, 0, c.width, Math.min(c.height, 200)).data; let h = 0; for (let i = 0; i < d.length; i += 4) h = (h * 31 + d[i] + d[i + 1] + d[i + 2]) | 0; return h; });
  const before = await snapshot();
  const clicked = Date.now();
  await page.mouse.click(box.x + 120, box.y + 40);
  await page.keyboard.type('hello moli');
  const typed = await until(async () => (await snapshot()) !== before, 10000);
  await page.locator('.browser-tab').first().screenshot({ path: resolve(process.env.SMOKE_SHOTS ?? fixture, 'browser-typed.png') }).catch(() => undefined);
  check('typed text appears on the canvas', !!typed, `click+type to frame ${Date.now() - clicked} ms`);
  await page.keyboard.press('Enter');
  check('form sent with the typed text', !!await until(() => sent.includes('hello moli'), 10000), JSON.stringify(sent));
  check('next page drawn', !!await until(async () => (await share(page, [0, 0, 192])) > 0.3, 10000));

  // Latency of one click to a fresh frame, 10 samples.
  const samples = [];
  for (let i = 0; i < 10; i++) {
    const marker = await page.evaluate(() => { const c = document.querySelector('.browser-tab .browser-canvas'); return c.getContext('2d').getImageData(0, 0, 1, 1).data[3]; });
    await page.evaluate(() => { const c = document.querySelector('.browser-tab .browser-canvas'); c.getContext('2d').clearRect(0, 0, 1, 1); });
    const t = Date.now(); await page.mouse.click(box.x + 400, box.y + 300);
    if (await until(() => page.evaluate(() => document.querySelector('.browser-tab .browser-canvas').getContext('2d').getImageData(0, 0, 1, 1).data[3] > 0), 5000)) samples.push(Date.now() - t);
    void marker;
  }
  samples.sort((a, b) => a - b);
  check('click → fresh frame', samples.length === 10, `median ${samples[5]} ms, max ${samples[9]} ms`);
  const memory = hosts.length ? ps(`$p = Get-Process -Id ${hosts[0]}; '{0:N0} MB working set, {1:N0} MB private' -f ($p.WorkingSet64/1MB), ($p.PrivateMemorySize64/1MB)`) : '?';
  console.log(`INFO host memory with one page: ${memory}`);

  // The host ends with the app.
  await browser.close().catch(() => undefined); browser = undefined;
  app.kill();
  const gone = await until(() => hosts.every(pid => ps(`(Get-Process -Id ${pid} -ErrorAction SilentlyContinue | Measure-Object).Count`) === '0'), 10000);
  check('killing the app ends the host', !!gone);
} finally {
  await browser?.close().catch(() => undefined);
  if (app.exitCode === null) app.kill();
  web.close();
}
const failed = results.filter(ok => !ok).length;
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exitCode = failed ? 1 : 0;
