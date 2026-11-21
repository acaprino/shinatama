// The terminal minimap through the real app: drawn as soon as the first session is on screen, at
// first start and after a restart, with no drawer opened; it holds the output in miniature and marks
// the visible part; a click or a drag on it scrolls the terminal there.
// Run after the desktop build (SHINATAMA_EXE names the executable, the terminal helper beside it),
// with TEMP/TMP outside any git project. SHOTS names a folder for captures.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp } from 'node:fs/promises';
import { tmpdir as systemTmpdir } from 'node:os';
import { realpathSync } from 'node:fs';
// The temporary folder by its long name: a runner's is C:\Users\RUNNER~1\..., and the app answers with the long one.
const tmpdir = () => realpathSync.native(systemTmpdir());
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:net';
import { startSession } from './session-flow.mjs';

if (process.platform !== 'win32') throw new Error('This test requires Windows.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-minimap-'));
const projectPath = resolve(fixture, 'project'); await mkdir(projectPath);
const shots = process.env.SHOTS ? resolve(process.env.SHOTS) : null; if (shots) await mkdir(shots, { recursive: true });
const shot = async name => { if (shots) await page.screenshot({ path: resolve(shots, name + '.png') }); };
const freePort = async () => {
  const reservation = createServer();
  await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
  const found = reservation.address().port;
  await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
  return found;
};
const until = async (read, milliseconds, message) => {
  let last;
  for (const deadline = Date.now() + milliseconds; ;) {
    last = await read().catch(error => ({ error: String(error) })); if (last?.ok) return last;
    if (Date.now() >= deadline) throw new Error(`${message} Last state: ${JSON.stringify(last)}`);
    await delay(100);
  }
};
let diagnostics = '';
let app, browser, page;
const errors = [];
/** Starts the app on the fixture's data and webview profile, so a second launch is a restart. */
const launch = async () => {
  const port = await freePort();
  app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1',
      WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile') },
  });
  const mine = app;
  mine.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-32768); });
  browser = undefined;
  for (let attempt = 0; attempt < 60; attempt++) {
    if (mine.exitCode !== null) throw new Error('Desktop exited: ' + mine.exitCode + ' ' + diagnostics);
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:' + port, { timeout: 1000 }); break; } catch { await delay(500); }
  }
  if (!browser) throw new Error('WebView2 unavailable. ' + diagnostics);
  const context = browser.contexts()[0];
  const mainPage = () => context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'));
  for (const deadline = Date.now() + 30000; !mainPage() && Date.now() < deadline;) await delay(250);
  page = mainPage(); if (!page) throw new Error('Main window did not appear.');
  page.on('pageerror', error => errors.push(error.message));
};

/** The visible terminal's minimap: where it is, how much of its canvas is drawn, the mark of the
 * visible part, and whether a drawer or sheet is open. */
const minimap = () => page.evaluate(() => {
  const view = document.querySelector('.wm-body > .terminal-view:not([hidden])');
  const strip = view?.querySelector('.terminal-minimap'); const canvas = strip?.querySelector('canvas');
  const box = element => { const rect = element?.getBoundingClientRect(); return rect ? { x: rect.x, y: rect.y, width: rect.width, height: rect.height } : null; };
  // Painted pixels, and how wide the drawn rows are on average, as a share of the strip.
  let painted = 0; let rows = 0; let ink = 0;
  if (canvas?.width && canvas.height) {
    const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
    for (let y = 0; y < canvas.height; y++) {
      let count = 0; for (let x = 0; x < canvas.width; x++) if (data[(y * canvas.width + x) * 4 + 3] > 0) count++;
      if (count) { painted += count; rows++; ink += count / canvas.width; }
    }
  }
  const screen = view?.querySelector('.xterm-screen');
  const mark = strip?.querySelector('.im-overview-frame');
  const open = [...document.querySelectorAll('dialog[open]')].map(element => element.id || element.className);
  return { strip: box(strip), screen: box(screen), mark: mark && !mark.hidden ? box(mark) : null, painted,
    ink: rows ? Math.round(ink / rows * 100) / 100 : 0, canvas: canvas ? [canvas.width, canvas.height] : null, open };
});
/** The numbers of the "line N" rows on screen, from the terminal's accessibility rows. */
const shownLines = () => page.evaluate(() => [...document.querySelectorAll('.wm-body > .terminal-view:not([hidden]) .xterm-accessibility-tree > div')]
  .map(row => /line (\d+)\b/.exec(row.textContent ?? '')?.[1]).filter(Boolean).map(Number));
/** Drawn within a second or so, with no drawer or sheet open: the prompt's line, and a frame around
 * the screen's lines that is a mark, not the whole strip. */
const drawnSoon = message => until(async () => {
  const state = await minimap();
  const ok = !!state.strip && state.strip.width >= 16 && state.strip.height >= 100 && state.painted > 0 && state.open.length === 0
    && !!state.mark && state.mark.height >= 6 && state.mark.height < state.strip.height / 2;
  return { ...state, ok };
}, 3000, message);

const LINES = 400;
async function printLines() {
  await page.locator('.wm-body > .terminal-view:not([hidden]) .xterm-screen').click({ position: { x: 40, y: 20 } });
  await page.keyboard.type(`1..${LINES} | ForEach-Object { "line $_ " + ('#' * ($_ % 70)) }`); await page.keyboard.press('Enter');
  await until(async () => ({ ok: (await shownLines()).includes(LINES) }), 20000, `The ${LINES} lines were not printed.`);
}
/** A click on the minimap's top part shows the first lines; a drag back down shows the last ones. */
async function scrollThroughMinimap(label) {
  // The output shows: its lines fill a good part of the strip's width whatever the terminal's, and
  // the frame sits at the bottom, around the last lines.
  const before = await until(async () => {
    const state = await minimap();
    return { ...state, ok: state.ink >= 0.35 && !!state.mark && state.mark.y + state.mark.height >= state.strip.y + state.strip.height * 0.8 };
  }, 3000, `${label}: the output is not drawn legibly in the minimap, or its frame is not at the end.`);
  const linesBefore = await shownLines();
  expect(Math.min(...linesBefore), `${label}: the last lines are on screen before the click`).toBeGreaterThan(LINES - 80);
  await shot(`${label}-2-output`);
  const { x, y, width, height } = before.strip;
  await page.mouse.click(x + width / 2, y + 4);
  const top = await until(async () => { const lines = await shownLines(); return { lines, ok: lines.length > 0 && Math.min(...lines) <= 5 }; }, 3000,
    `${label}: a click at the minimap's top did not scroll the terminal to its first lines.`);
  const afterClick = await minimap();
  expect(afterClick.mark.y - afterClick.strip.y, `${label}: the frame follows the click to the top`).toBeLessThan(2);
  await page.mouse.move(x - 200, y + height / 2); // off the strip, so the screenshot shows the frame at rest
  await shot(`${label}-3-after-click`);
  // A drag from the top down past the strip's end follows the pointer to the last lines.
  await page.mouse.move(x + width / 2, y + 4); await page.mouse.down();
  await page.mouse.move(x + width / 2, y + height * 0.5, { steps: 8 }); await page.mouse.move(x + width / 2, y + height + 40, { steps: 8 }); await page.mouse.up();
  await until(async () => { const lines = await shownLines(); return { lines, ok: lines.includes(LINES) }; }, 3000,
    `${label}: a drag down the minimap did not bring the last lines back.`);
  console.log(`${label}: minimap ${Math.round(before.strip.width)}×${Math.round(before.strip.height)}, ${before.painted} px drawn, rows ${Math.round(before.ink * 100)}% wide on average; `
    + `click → lines ${Math.min(...top.lines)}–${Math.max(...top.lines)}`);
}

try {
  await launch();
  // 1. First start: the first session opens, and its minimap is drawn with nothing else done.
  await (await startSession(page, { project: projectPath, harness: 'powershell' })).done();
  const first = await drawnSoon('First start: the minimap was not drawn once the first session was on screen.');
  expect(first.strip.x, 'the minimap sits right of the terminal text').toBeGreaterThanOrEqual(first.screen.x + first.screen.width - 1);
  await shot('first-start-1-session-open');
  await printLines();
  await scrollThroughMinimap('first-start');

  // 2. Restart: the window closes and opens again; the next session's minimap is drawn at once too.
  const gone = new Promise(accept => app.once('exit', accept));
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  // A running terminal makes the window ask first.
  await Promise.race([gone, page.getByRole('alertdialog', { name: 'Close workspace?', exact: true }).getByRole('button', { name: 'Stop', exact: true }).click({ timeout: 10000 }).catch(() => {})]);
  expect(await Promise.race([gone, delay(20000).then(() => 'timeout')])).toBe(0);
  await browser.close().catch(() => {});
  await launch();
  await expect(page.locator('.wm-tab').first()).toBeVisible({ timeout: 30000 });
  await (await startSession(page, { project: projectPath, harness: 'powershell' })).done();
  await drawnSoon('After a restart: the minimap was not drawn once the session was on screen.');
  await shot('restart-1-session-open');
  await printLines();
  await scrollThroughMinimap('restart');

  // 3. A drawer narrows the terminal: the minimap follows it and reads the same.
  const wide = await minimap();
  await page.getByRole('button', { name: 'Sessions', exact: true }).click();
  await until(async () => { const state = await minimap(); return { ...state, ok: !!state.strip && state.strip.x < wide.strip.x - 100 && Math.abs(state.ink - wide.ink) < 0.15 && !!state.mark }; },
    3000, 'With a drawer open, the minimap did not follow the narrower terminal.');
  await shot('restart-4-drawer');
  expect(errors, 'page errors').toEqual([]);
  console.log('minimap smoke: ok');
} catch (error) {
  if (page && shots) await page.screenshot({ path: resolve(shots, 'failure.png') }).catch(() => {});
  console.error(diagnostics.slice(-2000));
  throw error;
} finally {
  if (browser) {
    for (const context of browser.contexts()) for (const each of context.pages()) await each.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')).catch(() => {});
    await browser.close().catch(() => {});
  }
  if (app && app.exitCode === null) app.kill();
}
