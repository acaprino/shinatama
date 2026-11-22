// Real WebView + Rust IPC, disposable data home, synthetic keys; never sends a provider request.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const fixture = await mkdtemp(resolve(process.env.SHINATAMA_SMOKE_TEMP || tmpdir(), 'shinatama-model-settings-'));
const data = resolve(fixture, 'data');
const executable = resolve('target/debug/shinatama-desktop.exe');
const reservation = createServer();
await new Promise((yes, no) => { reservation.once('error', no); reservation.listen({ host: '127.0.0.1', port: 0 }, yes); });
const port = reservation.address().port;
await new Promise(yes => reservation.close(yes));
const app = spawn(executable, [], { windowsHide: true, stdio: 'ignore', env: { ...process.env, SHINATAMA_HOME: data,
  WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` } });
let browser, page;
const key = '!synthetic$KEY-not-a-command';
const ipc = (command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
try {
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
  await ipc('storage_location').catch(error => { throw new Error(`Scratch storage: ${String(error)}`); });
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await page.locator('.im-titlebar').getByRole('tab', { name: 'Settings', exact: true }).click();
  await expect(page.locator('#settings-status')).toContainText('New sessions start with');
  await page.getByRole('button', { name: 'Models', exact: true }).click();
  await expect(page.locator('#settings-section-models-title')).toBeInViewport();
  await expect(page.locator('.model-empty')).toBeVisible();
  await page.locator('#models-preset').selectOption({ label: 'LM Studio' });
  await page.locator('#models-add-connection').click();
  const card = page.locator('.model-connection');
  await card.getByLabel('Model ID', { exact: true }).fill('org/test-model');
  await card.getByLabel('Context tokens', { exact: true }).fill('32000');
  await card.getByLabel('Output tokens', { exact: true }).fill('4000');
  await card.locator('.model-api-key').fill(key);
  // The new connection says no agent uses it, and one click assigns it.
  await expect(card.locator('.model-status')).toHaveText('Not used');
  await card.getByRole('button', { name: 'Use for OpenCode' }).click();
  await expect(card.locator('.model-status')).toHaveText('Used by OpenCode');
  const choice = await page.getByLabel('OpenCode model', { exact: true }).inputValue();
  const id = JSON.parse(choice)[0];
  for (const name of ['Pi', 'Hermes']) await page.getByLabel(`${name} model`, { exact: true }).selectOption(choice);
  await expect(card.locator('.model-status')).toHaveText('Used by OpenCode, Pi and Hermes');
  await expect(page.locator('.model-settings')).toContainText('Saved only');
  await page.locator('#settings-save').click();
  await expect(page.locator('#settings-status')).toContainText('Saved.');
  await expect(card.locator('.model-api-key')).toHaveValue('');
  const saved = await ipc('model_settings_load');
  expect(saved.connections).toHaveLength(1);
  expect(saved.connections[0].hasApiKey).toBe(true);
  for (const harness of ['opencode', 'pi', 'hermes']) expect(saved.assignments[harness]).toEqual({ connectionId: id, modelId: 'org/test-model' });
  expect(JSON.stringify(saved)).not.toContain(key);
  const file = resolve(data, 'model-settings.json');
  expect(await readFile(file, 'utf8')).not.toContain(key);
  console.log('ok shared connection, three assignments, encrypted key and redacted IPC');

  await page.locator('#settings-dialog button[aria-label="Close settings"]').click();
  await page.locator('.im-titlebar').getByRole('tab', { name: 'Settings', exact: true }).click();
  await expect(page.locator('#settings-status')).toContainText('New sessions start with');
  await page.getByRole('button', { name: 'Models', exact: true }).click();
  await expect(page.getByLabel('Pi model', { exact: true })).toHaveValue(choice);
  // Saved connections start closed; Edit opens the form.
  await expect(card.getByLabel('Base URL', { exact: true })).toBeHidden();
  await card.getByRole('button', { name: /^Edit / }).click();
  await expect(card.locator('.model-api-key')).toHaveAttribute('placeholder', 'Saved · type to replace');
  // A rejected endpoint must leave the file unchanged.
  const beforeInvalid = await readFile(file, 'utf8');
  await card.getByLabel('Base URL', { exact: true }).fill('https://user:secret@example.com');
  await page.locator('#settings-save').click();
  await expect(page.locator('#settings-status')).toContainText('without credentials');
  expect(await readFile(file, 'utf8')).toBe(beforeInvalid);
  await card.getByLabel('Base URL', { exact: true }).fill('http://localhost:1234/v1');
  await page.locator('#settings-section-models').evaluate(section => section.parentElement.scrollTo({ top: section.offsetTop, behavior: 'instant' }));
  await page.screenshot({ path: resolve('target/model-settings-panel.png') });
  // Saving another section can fail after Models succeeds; the status must make the partial save explicit.
  await page.locator('#settings-add-variable').click();
  const variable = page.locator('.settings-var').last();
  await variable.locator('input').nth(0).fill('ANTHROPIC_API_KEY');
  await variable.locator('input').nth(1).fill('synthetic-value');
  await page.locator('#settings-save').click();
  await expect(page.locator('#settings-status')).toContainText('Models were saved; the remaining settings were not all saved.');
  expect((await ipc('model_settings_load')).connections[0].hasApiKey).toBe(true);
  await variable.getByRole('button').click();
  await page.getByRole('button', { name: 'Models', exact: true }).click();
  // A removed connection stays visibly unresolved until the user explicitly chooses the agent's own setup.
  await card.getByRole('button', { name: 'Remove connection' }).click();
  await expect(page.getByLabel('Pi model', { exact: true })).toHaveValue(choice);
  await expect(page.getByLabel('Pi model', { exact: true })).toHaveAttribute('aria-invalid', 'true');
  await page.locator('#settings-save').click();
  await expect(page.locator('#settings-status')).toContainText('Choose an existing model');
  for (const name of ['OpenCode', 'Pi', 'Hermes']) await page.getByLabel(`${name} model`, { exact: true }).selectOption('');
  await page.locator('#settings-save').click(); await expect(page.locator('#settings-status')).toContainText('Saved.');
  const cleared = await ipc('model_settings_load'); expect(cleared.connections).toEqual([]);
  expect(JSON.parse(await readFile(file, 'utf8')).keys).toEqual({});
  console.log('ok reload, invalid URL preservation, explicit own-setup fallback and orphan key removal');
  console.log('PASS model settings native smoke');
} finally {
  try { await browser?.close(); } catch {}
  if (app.exitCode === null) { try { execFileSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' }); } catch {} }
  await rm(fixture, { recursive: true, force: true, maxRetries: 5 }).catch(() => {});
}
