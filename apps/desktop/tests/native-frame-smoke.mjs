// Desktop behaviour of the built exe (native_frame.rs, desktop-shell.ts, webview_recovery.rs):
// Snap Layouts hit test on the maximize button, window menu reload, browser keys and
// navigation refused, zoom, engine menu, renderer crash recovery with its retry limit.
// Run after `npm run build` and `cargo build -p shinatama-desktop --features custom-protocol`,
// with TEMP/TMP outside any git project.
import { chromium } from '@playwright/test';
import { execFileSync, spawn } from 'node:child_process';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This test requires Windows.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-native-'));
const profile = resolve(fixture, 'webview-profile');
await mkdir(resolve(fixture, 'data'));
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
const port = reservation.address().port;
await new Promise(accept => reservation.close(accept));
const app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'), WEBVIEW2_USER_DATA_FOLDER: profile,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let diagnostics = '';
app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-16384); });
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok, detail }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`); };
const native = (action, extra = []) => execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-File',
  resolve(root, 'apps/desktop/tests/native-frame.ps1'), '-AppProcessId', String(app.pid), '-Action', action, ...extra], { encoding: 'utf8', windowsHide: true }).trim();
const probe = () => JSON.parse(native('probe'));
async function until(test, ms = 10000) {
  for (const deadline = Date.now() + ms; Date.now() < deadline; await delay(150)) { try { const value = await test(); if (value) return value; } catch { /* page reloading */ } }
  return null;
}
function renderers() {
  const out = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
    `Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" | Where-Object { $_.CommandLine -like '*${profile.replaceAll("'", "''")}*' -and $_.CommandLine -like '*--type=renderer*' } | ForEach-Object { $_.ProcessId }`], { encoding: 'utf8', windowsHide: true });
  return out.split(/\s+/).filter(Boolean).map(Number);
}

let browser, context;
const mainPage = () => context.pages().find(page => page.url().includes('tauri.localhost') && !page.url().includes('widget.html'));
// After a renderer exit Playwright keeps the crashed page: a new connection sees the reloaded one.
async function connect(ms = 30000) {
  await browser?.close().catch(() => undefined); browser = undefined;
  for (const deadline = Date.now() + ms; !browser && Date.now() < deadline;) {
    if (app.exitCode !== null) throw new Error(`Desktop exited: ${app.exitCode}\n${diagnostics}`);
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); } catch { await delay(500); }
  }
  if (!browser) throw new Error(`WebView2 unavailable.\n${diagnostics}`);
  context = browser.contexts()[0];
  return until(mainPage, ms);
}
try {
  const page = await connect();
  if (!page) throw new Error('Main window did not appear.');
  await page.waitForSelector('#window-maximize:not(:disabled)', { timeout: 30000 });
  const t0 = Date.now(); if (process.env.NATIVE_DIAGNOSTICS) page.on('framenavigated', frame => { if (frame === page.mainFrame()) console.log(`  navigated +${Date.now() - t0} ms ${frame.url()}`); });
  const step = name => { if (process.env.NATIVE_DIAGNOSTICS) console.log(`  step +${Date.now() - t0} ms ${name}`); };
  check('release build loads the embedded frontend', /^https?:\/\/tauri\.localhost\//.test(page.url()), page.url());

  // Snap Layouts: the native overlay answers HTMAXBUTTON over the HTML button.
  const box = await page.evaluate(() => { const r = document.querySelector('#window-maximize').getBoundingClientRect(); const k = devicePixelRatio; return { x: Math.round(r.left * k), y: Math.round(r.top * k), width: Math.round(r.width * k), height: Math.round(r.height * k) }; });
  let state = await until(() => { const s = probe(); return s.overlay && s.overlayVisible ? s : null; });
  check('maximize overlay exists and is visible', !!state);
  if (state) {
    const rect = state.overlayRect;
    check('overlay covers the HTML maximize button', Math.abs(rect.x - box.x) <= 1 && Math.abs(rect.width - box.width) <= 1 && rect.y + rect.height === box.y + box.height, `overlay ${JSON.stringify(rect)} button ${JSON.stringify(box)}`);
    check('overlay is the top child over the button', state.topChildAtCenter === 'ShinatamaMaximizeOverlay', state.topChildAtCenter);
    check('overlay answers HTMAXBUTTON (9)', state.hitTest === 9, String(state.hitTest));
  }
  await page.evaluate(() => { window.__caption = []; new MutationObserver(() => window.__caption.push(document.querySelector('#window-maximize').className)).observe(document.querySelector('#window-maximize'), { attributes: true, attributeFilter: ['class'] }); });
  native('hover-maximize');
  check('hover reaches the HTML button', !!await until(() => page.evaluate(() => window.__caption.some(name => name.includes('os-hover'))), 3000));
  native('press-maximize');
  check('click on the overlay maximizes', !!await until(() => probe().zoomed, 5000));
  check('maximized overlay follows the button', !!await until(async () => { const s = probe(); const b = await page.evaluate(() => Math.round(document.querySelector('#window-maximize').getBoundingClientRect().left * devicePixelRatio)); return s.overlayRect && Math.abs(s.overlayRect.x - b) <= 1 && s.hitTest === 9; }, 5000));
  native('press-maximize');
  check('second click restores', !!await until(() => !probe().zoomed, 5000));

  // Window menu: Reload interface, after the page answered.
  state = probe();
  check('window menu offers Reload interface', state.lastMenuItem.text === 'Reload interface' && state.lastMenuItem.id === 0x5350, JSON.stringify(state.lastMenuItem));
  await page.evaluate(() => { window.__marker = 1; });
  const started = Date.now();
  step('reload-menu'); native('reload-menu'); step('posted');
  const reloaded = await until(() => page.evaluate(() => window.__marker === undefined && !!document.querySelector('#window-maximize:not(:disabled)')), 10000);
  check('Reload interface reloads the page after it saved', !!reloaded && Date.now() - started < 2900, `${Date.now() - started} ms`);

  // Browser keys: a posted F5 reaches the page and does not reload it. Posted keys skip WebView2's
  // accelerator stage and carry no reliable Ctrl state, so this proves the page's guard for F5;
  // Ctrl+R and the others are covered by desktop-native.test.ts.
  await page.waitForSelector('#window-maximize:not(:disabled)');
  const before = await page.evaluate(() => { window.__marker = 2; window.__keys = []; document.addEventListener('keydown', event => window.__keys.push((event.ctrlKey ? 'Ctrl+' : '') + event.key), true); return performance.timeOrigin; });
  step('marker 2'); native('key', ['-VirtualKey', '0x74']); step('F5 posted');
  await delay(1500);
  const keys = await page.evaluate(() => ({ marker: window.__marker, keys: window.__keys, origin: performance.timeOrigin }));
  check('posted keys reach the page (instrument works)', keys.keys?.includes('F5'), JSON.stringify(keys.keys));
  check('F5 does not reload', keys.marker === 2, `document ${before} → ${keys.origin}`);

  // Navigation and new windows are refused, with no recovery question.
  const pages = context.pages().length;
  await page.evaluate(() => { window.open('https://example.com/'); location.href = 'https://example.com/'; }).catch(() => undefined);
  await delay(2000);
  const stayed = await page.evaluate(() => window.__marker === 2 && location.hostname === 'tauri.localhost').catch(() => false);
  check('navigation to another origin is refused', stayed, page.url());
  check('window.open opens nothing', context.pages().length === pages, String(context.pages().length));
  check('a refused navigation raises no question', probe().dialogs.length === 0);

  // Zoom up to 200% from the keyboard, back to 100%.
  const ratio = () => page.evaluate(() => devicePixelRatio);
  const base = await ratio();
  await page.locator('#workspace').click({ position: { x: 5, y: 5 } }).catch(() => undefined);
  await page.keyboard.press('Control+Equal');
  check('Ctrl+= zooms in', !!await until(async () => Math.abs(await ratio() / base - 1.1) < 0.02, 3000), String(await ratio() / base));
  for (let i = 0; i < 12; i++) await page.keyboard.press('Control+Equal');
  check('zoom reaches 200%', !!await until(async () => Math.abs(await ratio() / base - 2) < 0.02, 3000), String(await ratio() / base));
  await page.keyboard.press('Control+0');
  check('Ctrl+0 returns to 100%', !!await until(async () => Math.abs(await ratio() / base - 1) < 0.02, 3000));

  // Engine menu and selection.
  const menu = await page.evaluate(() => {
    const fire = target => { const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true }); target.dispatchEvent(event); return event.defaultPrevented; };
    const field = document.createElement('input'); document.body.append(field);
    const result = { toolbar: fire(document.querySelector('.toolbar')), field: fire(field) };
    field.remove(); return result;
  });
  check('no page menu on the chrome', menu.toolbar === true);
  check('editing menu kept in fields', menu.field === false);
  const select = await page.evaluate(() => ({ titlebar: getComputedStyle(document.querySelector('.toolbar')).userSelect, cursor: getComputedStyle(document.querySelector('#new-terminal')).cursor }));
  check('chrome is not selectable and uses the arrow', select.titlebar === 'none' && select.cursor === 'default', JSON.stringify(select));

  // A renderer exit reloads by itself three times a minute; the fourth asks.
  for (let crash = 1; crash <= 4; crash++) {
    await mainPage()?.evaluate(value => { window.__marker = value; }, 100 + crash).catch(() => undefined);
    const pids = renderers();
    if (!pids.length) { check(`renderer ${crash} found`, false); break; }
    for (const pid of pids) { try { process.kill(pid); } catch { /* already gone */ } }
    if (crash < 4) {
      await delay(1500);
      const current = await connect(15000);
      const back = current && await until(() => current.evaluate(() => window.__marker === undefined && !!document.querySelector('#window-maximize:not(:disabled)')), 15000);
      check(`renderer exit ${crash} reloads without asking`, !!back && probe().dialogs.length === 0);
    } else {
      const asked = await until(() => { const s = probe(); return s.dialogs.length ? s : null; }, 10000);
      check('the fourth exit in a minute asks, owned by the main window', !!asked && asked.enabled === false, JSON.stringify(asked?.dialogs));
      native('close-dialog');
      check('answering gives the main window back', !!await until(() => { const s = probe(); return s.enabled && !s.dialogs.length; }, 5000));
    }
  }
} finally {
  await browser?.close().catch(() => undefined);
  if (app.exitCode === null) app.kill();
  await delay(1500);
  await rm(fixture, { recursive: true, force: true }).catch(() => undefined);
}
const failed = results.filter(result => !result.ok);
console.log(`${results.length - failed.length}/${results.length} passed`);
if (failed.length) { process.exitCode = 1; if (process.env.NATIVE_DIAGNOSTICS) console.log(diagnostics); }
