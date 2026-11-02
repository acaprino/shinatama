// A real app restart proves picker persistence, without touching personal settings.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, rmdir, mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This WebView2 test is Windows-only.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-projects-'));
const first = resolve(fixture, 'first-project');
const second = resolve(fixture, 'second-project');
await mkdir(first, { recursive: true }); await mkdir(second);
const port = 19227;

async function openApp() {
  const app = spawn(resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: root, windowsHide: true, stdio: 'ignore',
    env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'settings'),
      WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
  });
  let browser;
  try {
    for (let attempt = 0; attempt < 60; attempt++) {
      if (app.exitCode !== null) throw new Error(`App exited: ${app.exitCode}`);
      try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
      catch { await delay(300); }
    }
    if (!browser) throw new Error('App did not expose its test port');
    const page = browser.contexts()[0].pages()[0];
    await expect(page.locator('#runtime-status')).toContainText('Local runtime');
    return { app, browser, page };
  } catch (error) { app.kill(); throw error; }
}
async function closeApp(instance) {
  const hasTerminals = await instance.page.evaluate(async () => (await window.__TAURI_INTERNALS__.invoke('terminal_list')).length > 0);
  const exited = new Promise(resolve => instance.app.once('exit', resolve));
  const closed = instance.page.waitForEvent('close');
  await instance.page.getByRole('button', { name: 'Close window', exact: true }).click();
  if (hasTerminals) {
    await expect(instance.page.locator('#confirm-dialog')).toBeVisible();
    await instance.page.getByRole('button', { name: 'Stop', exact: true }).click();
  }
  await closed; await exited;
  await instance.browser.close().catch(() => {});
}
async function selectFolder(page, path) {
  await page.locator('#project').click(); await page.locator('#folder').fill(path);
  await page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect(page.locator('#project-dialog')).not.toBeVisible();
}
let running;
try {
  running = await openApp();
  await selectFolder(running.page, first);
  await running.page.locator('#new-terminal').click();
  const sessions = () => running.page.evaluate(() => window.__TAURI_INTERNALS__.invoke('terminal_list'));
  await expect.poll(async () => (await sessions()).length).toBe(1);
  const original = (await sessions())[0];
  await selectFolder(running.page, second);
  expect((await sessions())[0]).toEqual(original);
  await running.page.locator('#project').click();
  await running.page.locator('#folder').fill(resolve(fixture, 'missing-project'));
  await running.page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect(running.page.locator('#project-error')).toBeVisible();
  await expect(running.page.locator('#project-name')).toHaveText('second-project');
  await running.page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await closeApp(running); running = null;

  running = await openApp();
  await expect(running.page.locator('#project-name')).toHaveText('second-project');
  expect(await running.page.evaluate(() => window.__TAURI_INTERNALS__.invoke('terminal_list'))).toEqual([]);
  await running.page.locator('#project').click();
  await expect(running.page.locator('.recent-project strong')).toHaveText(['second-project', 'first-project']);
  await running.page.locator('#project-filter').fill('first-project');
  await expect(running.page.locator('.recent-project')).toHaveCount(1);
  await running.page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await rmdir(first); // Exact, empty synthetic fixture only; never recurse into a project.
  await running.page.locator('#project').click();
  await expect(running.page.locator('.recent-project').filter({ hasText: 'first-project' })).toBeDisabled();
  await expect(running.page.getByText('Folder unavailable', { exact: true })).toBeVisible();
  await running.page.screenshot({ path: resolve(root, '.prototype-evidence/projects-picker.png') });
  await running.page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await mkdir(first); await rmdir(second);
  await closeApp(running); running = null;
  running = await openApp();
  await expect(running.page.locator('#project-name')).toHaveText('second-project');
  await expect(running.page.locator('#notice')).toContainText('Last project unavailable');
  await running.page.locator('#new-terminal').click();
  await expect(running.page.locator('#project-dialog')).toBeVisible();
  expect(await running.page.evaluate(() => window.__TAURI_INTERNALS__.invoke('terminal_list'))).toEqual([]);
  await running.page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await closeApp(running); running = null;
  console.log('PASS: real restart preserves MRU and selection; no automatic terminals; existing terminal cwd/identity unchanged; invalid/missing paths rejected or marked; incremental recent filter works.');
} finally {
  if (running) {
    await running.page.evaluate(() => window.__TAURI_INTERNALS__.invoke('terminal_shutdown')).catch(() => {});
    await running.browser.close().catch(() => {}); running.app.kill();
  }
}
