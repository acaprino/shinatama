// Real embedded app: rejected storage cannot write or disable window controls.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdtemp, access } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('Storage UI checks require Windows.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-storage-ui-'));
const forbidden = resolve(root, '.prototype-evidence', `rejected-storage-${Date.now()}`);
const port = 19229;
const invoke = (page, command, args) => page.evaluate(
  ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });

async function check(home, valid, index) {
  const app = spawn(resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: root, windowsHide: true, stdio: 'ignore',
    env: { ...process.env, SHINATAMA_HOME: home,
      WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, `webview-${index}`),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
  });
  let browser;
  try {
    for (let attempt = 0; attempt < 60; attempt++) {
      if (app.exitCode !== null) throw new Error(`Desktop exited with ${app.exitCode}`);
      try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
      catch { await delay(300); }
    }
    if (!browser) throw new Error('Desktop did not expose its local test endpoint.');
    const page = browser.contexts()[0].pages()[0];
    await expect(page.locator('#runtime-status')).toContainText(valid ? 'Local runtime' : 'Storage unavailable', { timeout: 15000 });
    const error = () => invoke(page, 'storage_location').then(() => null, error => String(error));
    if (valid) {
      const location = await invoke(page, 'storage_location');
      expect(location.database_path.replace(/^\\\\\?\\/, '').toLowerCase()).toBe(resolve(home, 'shinatama.db').toLowerCase());
      const conflict = await invoke(page, 'project_select', { path: fixture }).then(() => null, error => String(error));
      expect(conflict).toContain('cartella dati');
      const launch = await invoke(page, 'terminal_start', { cwd: fixture, rows: 24, cols: 80 }).then(() => null, error => String(error));
      expect(launch).toContain('cartella dati');
    } else {
      await expect(page.locator('#runtime-status')).toHaveText('Storage unavailable', { timeout: 15000 });
      expect(await error()).toContain('SHINATAMA_HOME');
      await expect(page.locator('#notice')).toBeVisible();
    }
    // These commands remain responsive even when storage is rejected.
    expect(await invoke(page, 'terminal_list')).toEqual([]);
    const original = await invoke(page, 'plugin:window|is_maximized');
    await page.getByRole('button', { name: original ? 'Restore window' : 'Maximize window', exact: true }).click();
    await expect.poll(() => invoke(page, 'plugin:window|is_maximized')).toBe(!original);
    const exited = new Promise(resolve => app.once('exit', resolve));
    await page.getByRole('button', { name: 'Close window', exact: true }).click();
    await Promise.race([exited, delay(10000).then(() => { throw new Error('Rejected storage blocked closing the window.'); })]);
  } finally {
    if (browser) await browser.close().catch(() => {});
    if (app.exitCode === null) app.kill();
  }
}

await check(resolve(fixture, 'data'), true, 0);
await check(forbidden, false, 1);
expect(await access(forbidden).then(() => true, () => false)).toBe(false);
await check(String.raw`\\shinatama-invalid-network-host\missing\state`, false, 2);
console.log('PASS storage: exact data path, project conflict rejected, no writes to rejected project/UNC location, usable frame and close.');
