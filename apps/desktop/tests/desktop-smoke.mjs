// Windows integration check against the real Tauri WebView and Rust-owned ConPTY.
// Run after npm run build and cargo build -p shinatama-desktop --features custom-protocol.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This WebView2 smoke test is Windows-only.');
const root = process.cwd();
const output = resolve(root, '.prototype-evidence');
await mkdir(output, { recursive: true });
const port = 19226;
const app = spawn(resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: resolve(output, 'smoke-home'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`, WEBVIEW2_USER_DATA_FOLDER: resolve(output, 'webview-profile') },
});
let diagnostics = '';
app.stderr.on('data', chunk => { diagnostics += chunk; });
let browser;
let nativeHandle;
function nativeWindowState(restore = false) {
  return JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-File',
    resolve(root, 'apps/desktop/tests/window-state.ps1'), '-AppProcessId', String(app.pid),
    ...(nativeHandle ? ['-WindowHandle', String(nativeHandle)] : []),
    ...(restore ? ['-Restore'] : [])], { encoding: 'utf8', windowsHide: true }));
}
try {
  for (let attempt = 0; attempt < 60; attempt++) {
    if (app.exitCode !== null) throw new Error(`Desktop exited: ${app.exitCode}\n${diagnostics}`);
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
    catch { await delay(500); }
  }
  if (!browser) throw new Error(`WebView2 did not expose the test port. ${diagnostics}`);
  const context = browser.contexts()[0];
  const page = context.pages()[0] || await context.waitForEvent('page');
  const pageErrors = [];
  page.on('pageerror', error => pageErrors.push(error.message));
  await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 30000 });
  expect(new URL(page.url()).hostname).toBe('tauri.localhost');
  const ids = () => page.evaluate(() => window.__TAURI_INTERNALS__.invoke('terminal_list')).then(sessions => sessions.map(s => s.id).sort());
  expect(await ids()).toEqual([]);
  const initialWindow = nativeWindowState();
  nativeHandle = initialWindow.handle; // MainWindowHandle can select a helper window while minimized.
  expect(initialWindow.topInset).toBeLessThanOrEqual(8);
  await page.getByRole('button', { name: 'Minimize window', exact: true }).click();
  await expect.poll(() => nativeWindowState().minimized).toBe(true).catch(async error => {
    console.log('Window diagnostics:', nativeWindowState(), await page.locator('#notice').textContent());
    throw error;
  });
  nativeWindowState(true);
  await expect.poll(() => nativeWindowState().minimized).toBe(false);
  // Custom frame controls must operate on the actual native window, not just CSS.
  await page.getByRole('button', { name: 'Maximize window', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Restore window', exact: true })).toBeVisible();
  expect(await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('plugin:window|is_maximized'))).toBe(true);
  await page.getByRole('button', { name: 'Restore window', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Maximize window', exact: true })).toBeVisible();
  // Double-clicking the draggable title area also toggles the actual window.
  await page.locator('.titlebar-drag').dblclick();
  await expect(page.getByRole('button', { name: 'Restore window', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Restore window', exact: true }).click();
  await page.locator('#new-terminal').click();
  const left = page.locator('.pane').nth(0);
  const right = page.locator('.pane').nth(1);
  await expect(left.locator('.xterm-accessibility')).toContainText('PS ', { timeout: 20000 });
  await page.keyboard.type("Write-Output ('SHINATAMA_' + 'DESKTOP_OK')");
  await page.keyboard.press('Enter');
  await expect(left.locator('.xterm-accessibility')).toContainText('SHINATAMA_DESKTOP_OK');
  const firstIds = await ids();
  await left.getByRole('button', { name: 'Move terminal to other pane' }).click();
  expect(await ids()).toEqual(firstIds);
  await expect(right.locator('.xterm-accessibility')).toContainText('SHINATAMA_DESKTOP_OK');
  await right.getByRole('button', { name: 'Hide terminal view' }).click();
  expect(await ids()).toEqual(firstIds);
  await expect(right.locator('.terminal-view')).toBeHidden();
  await page.locator('#restore').click();
  await expect(right.locator('.terminal-view')).toBeVisible();
  await right.getByRole('button', { name: 'Maximize pane' }).click();
  await expect(left).toBeHidden();
  await right.getByRole('button', { name: 'Restore panes' }).click();
  await page.locator('#layout-toggle').click();
  await expect(page.locator('#workspace')).toHaveClass('rows');
  await page.locator('#layout-toggle').click();
  await page.getByRole('separator').focus();
  const beforeResize = Number(await page.getByRole('separator').getAttribute('aria-valuenow'));
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('separator')).toHaveAttribute('aria-valuenow', String(Math.min(80, beforeResize + 5)));
  expect(await ids()).toEqual(firstIds);
  await left.getByRole('button', { name: 'Open terminal' }).click();
  await expect(left.locator('.xterm-accessibility')).toContainText('PS ', { timeout: 20000 });
  expect((await ids()).length).toBe(2);
  await left.getByRole('combobox').selectOption('claude');
  await right.getByRole('combobox').selectOption('codex');
  await page.locator('#project').click();
  await page.locator('#folder').fill('not-an-absolute-folder');
  await page.getByRole('button', { name: 'Use folder' }).click();
  await expect(page.locator('#project-dialog')).toBeVisible();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.screenshot({ path: resolve(output, 'desktop-terminals.png') });
  // Stop requires confirmation; cancel preserves both processes.
  await left.getByRole('button', { name: 'Stop terminal' }).click();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect((await ids()).length).toBe(2);
  // Saturate a hidden terminal's bounded queue, while the other pane stays usable.
  await left.locator('.xterm-helper-textarea').focus();
  await page.keyboard.type("while ($true) { [Console]::Write(('x' * 8192)) }");
  await page.keyboard.press('Enter');
  await left.getByRole('button', { name: 'Hide terminal view' }).click();
  await right.locator('.xterm-helper-textarea').focus();
  await page.keyboard.type("Write-Output ('OTHER_' + 'PANE_ALIVE')");
  await page.keyboard.press('Enter');
  await expect(right.locator('.xterm-accessibility')).toContainText('OTHER_PANE_ALIVE', { timeout: 15000 });
  await page.locator('#restore').click();
  await left.getByRole('button', { name: 'Stop terminal' }).click();
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect.poll(ids).toHaveLength(1);
  // Remaining shell exits normally; output and status must finish without manual stop.
  await right.locator('.xterm-helper-textarea').focus();
  await page.keyboard.type('exit'); await page.keyboard.press('Enter');
  await expect(right.locator('.pane-state')).toHaveText('Exited', { timeout: 15000 });
  await right.getByRole('button', { name: 'Stop terminal' }).click();
  await expect.poll(ids).toHaveLength(0);
  await page.locator('#new-terminal').click();
  await expect.poll(ids).toHaveLength(1);
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  await expect(page.locator('#confirm-dialog')).toBeVisible();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect((await ids()).length).toBe(1);
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  const closed = page.waitForEvent('close');
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  await closed;
  expect(pageErrors).toEqual([]);
  console.log('PASS: borderless native frame, themed minimize/maximize/restore/close, titlebar double-click, real Tauri/ConPTY I/O, move/hide/restore identity, pane resize, profiles, noisy hidden output, natural exit, close confirmation; no frontend errors.');
  console.log(`Screenshot: ${resolve(output, 'desktop-terminals.png')}`);
} finally {
  if (browser) {
    for (const context of browser.contexts()) for (const page of context.pages()) {
      await page.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')).catch(() => {});
    }
    await browser.close().catch(() => {});
  }
  if (app.exitCode === null) app.kill();
}
