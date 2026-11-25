// Helpers of the README screenshot scripts shoot-*.mjs: start the built app on a disposable data home and a fixture, size its window to 1920x1080.
import { chromium } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

export const env0 = { exe: process.env.SHINATAMA_EXE, out: process.env.SHOTS_OUT, temp: process.env.SHOTS_TEMP };
for (const key of Object.keys(process.env)) if (/^(SHINATAMA_|CLAUDE|WEBVIEW2_|https?_proxy|all_proxy)/i.test(key)) delete process.env[key];
process.env.NO_PROXY = '127.0.0.1,localhost';
Object.assign(process.env, { USERPROFILE: 'C:/Users/alfio', APPDATA: 'C:/Users/alfio/AppData/Roaming', LOCALAPPDATA: 'C:/Users/alfio/AppData/Local' });

export const git = (cwd, ...args) => execFileSync('git', ['-c', 'user.name=Demo', '-c', 'user.email=demo@example.com', '-c', 'core.autocrlf=false', ...args], { cwd, stdio: 'ignore' });

/** A fixture project: a git repository on `branch` with `changed` modified files. */
export async function makeProject(root, name, branch, changed) {
  const dir = resolve(root, name);
  await mkdir(resolve(dir, 'src'), { recursive: true });
  git(dir, 'init', '-q', '-b', 'main');
  for (let i = 0; i < Math.max(changed, 1); i += 1) await writeFile(resolve(dir, 'src', `module${i + 1}.ts`), `export const v${i} = ${i};\n`);
  git(dir, 'add', '.'); git(dir, 'commit', '-q', '-m', 'init');
  if (branch !== 'main') git(dir, 'checkout', '-q', '-b', branch);
  for (let i = 0; i < changed; i += 1) await writeFile(resolve(dir, 'src', `module${i + 1}.ts`), `export const v${i} = ${i + 100};\n`);
  return dir;
}

export async function launch() {
  const fixture = resolve(env0.temp, 'acme');
  await rm(fixture, { recursive: true, force: true, maxRetries: 5 }).catch(() => {});
  await mkdir(fixture, { recursive: true });
  const data = resolve(fixture, 'data');
  const reservation = createServer();
  await new Promise((yes, no) => { reservation.once('error', no); reservation.listen({ host: '127.0.0.1', port: 0 }, yes); });
  const port = reservation.address().port;
  await new Promise(yes => reservation.close(yes));
  const app = spawn(env0.exe, [], { windowsHide: false, stdio: 'ignore', env: { ...process.env, SHINATAMA_HOME: data,
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` } });
  let browser, page;
  for (const until = Date.now() + 40000; !browser && Date.now() < until;) {
    if (app.exitCode !== null) throw new Error('App exited before WebView startup');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); } catch { await delay(200); }
  }
  if (!browser) throw new Error('WebView startup timed out');
  for (const until = Date.now() + 15000; !page && Date.now() < until;) {
    page = browser.contexts()[0].pages().find(p => new URL(p.url()).hostname === 'tauri.localhost');
    if (!page) await delay(100);
  }
  if (!page) throw new Error('Main WebView missing');
  page.setDefaultTimeout(15000);
  const ipc = (command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
  const stop = async () => {
    try { await browser?.close(); } catch {}
    if (app.exitCode === null) { try { execFileSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' }); } catch {} }
    await rm(fixture, { recursive: true, force: true, maxRetries: 5 }).catch(() => {});
  };
  return { fixture, app, browser, page, ipc, stop };
}

/** Sizes the real window so the page has a 1920x1080 viewport. */
export async function desktopSize(browser, page, width = 1920, height = 1080) {
  const session = await browser.newBrowserCDPSession();
  const { targetInfos } = await session.send('Target.getTargets');
  const target = targetInfos.find(t => t.type === 'page' && t.url.includes('tauri.localhost'));
  const { windowId } = await session.send('Browser.getWindowForTarget', { targetId: target.targetId });
  await session.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'normal', left: 0, top: 0, width, height: height + 80 } }).catch(e => console.log('bounds', String(e).slice(0, 100)));
  await delay(500);
  try { await page.setViewportSize({ width, height }); } catch (e) { console.log('viewport', String(e).slice(0, 100)); }
  await delay(300);
  console.log('inner', await page.evaluate(() => `${window.innerWidth}x${window.innerHeight} dpr ${window.devicePixelRatio}`));
}
