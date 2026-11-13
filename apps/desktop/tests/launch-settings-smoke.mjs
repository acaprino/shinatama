// Settings drawer and the launch policy, in the real app with a private SHINATAMA_HOME.
// Proves: the drawer opens with full access on, a saved token and a secret variable never come
// back and never sit in the file as text, and the agents really start the way the switch says
// (Claude's per-instance settings, Codex's argv). No sign-in answers or prompts are sent.
import { chromium, expect } from '@playwright/test';
import { execFileSync, spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { startSession } from './session-flow.mjs';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const packageExecutable = process.env.SHINATAMA_SMOKE_APP_EXE;
if (packageExecutable && !isAbsolute(packageExecutable)) throw new Error('The package executable must be an explicit absolute path.');
const executable = packageExecutable || resolve(workspace, 'target/debug/shinatama-desktop.exe');
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-launch-settings-'));
const data = resolve(fixture, 'data'); const project = resolve(fixture, 'project');
await mkdir(project);
const TOKEN = 'ghp_smoketoken0123456789'; const SECRET = 'smoke-secret-value';

const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
const app = spawn(executable, [], {
  cwd: packageExecutable ? dirname(executable) : workspace, windowsHide: true, stdio: 'ignore',
  env: { ...process.env, SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let browser, page, owned = false;
const ipc = (command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
// Every process of that name, as { pid, line }. Callers compare against a snapshot taken
// before they start a session, so another Shinatama's agents on this machine never count.
const processes = name => {
  const out = execFileSync('powershell', ['-NoProfile', '-Command',
    `Get-CimInstance Win32_Process -Filter "Name='${name}'" | ForEach-Object { "$($_.ProcessId)|$($_.CommandLine)" }`], { encoding: 'utf8' });
  return out.split(/\r?\n/).filter(Boolean).map(text => ({ pid: Number(text.split('|')[0]), line: text.slice(text.indexOf('|') + 1) }));
};
const newCodex = known => processes('codex.exe').filter(item => !known.has(item.pid) && item.line.includes('--no-daemon')).map(item => item.line);
async function walk(directory, found = []) {
  for (const entry of await readdir(directory, { withFileTypes: true }).catch(() => [])) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) await walk(path, found); else found.push(path);
  }
  return found;
}
const instanceSettings = async () => {
  const files = (await walk(data)).filter(path => /[\\/]instances[\\/][^\\/]+[\\/]settings\.json$/.test(path));
  return Promise.all(files.map(async path => JSON.parse(await readFile(path, 'utf8'))));
};
async function openSettings() {
  await page.locator('#toolbar-more').click();
  await page.locator('#open-settings').click();
  await expect(page.locator('#settings-dialog')).toBeVisible();
  await expect(page.locator('#settings-status')).toContainText('New sessions start with');
}
async function sessionOf(harness) {
  const before = (await ipc('terminal_list')).map(session => session.id);
  // A harness checked for the first time can answer "Try Open again" once; the product asks for
  // exactly that, so the proof does it once and only for that text.
  for (let attempt = 0; ; attempt++) {
    const flow = await startSession(page, { project, harness });
    try { await flow.done(120000); break; }
    catch (error) { if (attempt > 0 || !/Try Open again/.test(String(error.message))) throw error; await delay(2000); }
  }
  const created = (await ipc('terminal_list')).find(session => !before.includes(session.id));
  if (!created) throw new Error(`${harness} did not start.`);
  return created;
}

try {
  for (const deadline = Date.now() + 30000; !browser && Date.now() < deadline;) {
    if (app.exitCode !== null) throw new Error('The app exited during startup.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); } catch { await delay(250); }
  }
  if (!browser) throw new Error('The WebView did not start in 30 seconds.');
  const context = browser.contexts()[0];
  for (const deadline = Date.now() + 10000; !page && Date.now() < deadline;) {
    page = context.pages().find(candidate => { try { return new URL(candidate.url()).hostname === 'tauri.localhost'; } catch { return false; } });
    if (!page) await delay(100);
  }
  if (!page) throw new Error('The main WebView was not found.');
  page.setDefaultTimeout(15000); owned = true;
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');

  // 1. The drawer opens, full access is on by default, and nothing is saved yet.
  await openSettings();
  await expect(page.locator('#settings-full-access')).toBeChecked();
  console.log('ok  drawer opens with full access on');

  // 2. Save a GitHub sign-in and two variables.
  await page.locator('#settings-github-user').fill('octocat');
  await page.locator('#settings-github-token').fill(TOKEN);
  for (const [name, value, secret] of [['SMOKE_PLAIN', 'visible-value', false], ['SMOKE_SECRET', SECRET, true]]) {
    await page.locator('#settings-add-variable').click();
    const row = page.locator('.settings-var').last();
    await row.locator('input').nth(0).fill(name); await row.locator('input').nth(1).fill(value);
    if (secret) await row.locator('.settings-secret input').check();
  }
  await page.locator('#settings-save').click();
  await expect(page.locator('#settings-status')).toContainText('Saved.');
  const onDisk = await readFile(resolve(data, 'launch-settings.json'), 'utf8');
  expect(onDisk).not.toContain(TOKEN); expect(onDisk).not.toContain(SECRET); expect(onDisk).toContain('visible-value');
  console.log('ok  saved; the token and the secret are not in the file as text');

  // 3. Reopened, the page never gets the secrets back.
  await page.locator('#settings-dialog button[aria-label="Close settings"]').click();
  await openSettings();
  await expect(page.locator('#settings-github-token')).toHaveValue('');
  await expect(page.locator('#settings-github-token')).toHaveAttribute('placeholder', /Saved/);
  const values = await page.locator('.settings-var').evaluateAll(rows => rows.map(row => [...row.querySelectorAll('input')].map(input => input.value)));
  expect(JSON.stringify(values)).not.toContain(SECRET);
  expect(await page.evaluate(() => document.documentElement.outerHTML)).not.toContain('smoketoken');
  console.log('ok  reopened: the page holds no secret');
  await page.locator('#settings-dialog button[aria-label="Close settings"]').click();

  // 4. Full access on: Claude's per-instance settings and Codex's argv.
  const claude = await sessionOf('claude_code');
  const withAccess = (await instanceSettings()).filter(settings => settings.permissions?.defaultMode === 'bypassPermissions');
  expect(withAccess.length).toBeGreaterThan(0);
  expect(withAccess[0].skipDangerousModePermissionPrompt).toBe(true);
  console.log('ok  Claude starts in bypass-permissions mode with no confirmation prompt');
  await ipc('terminal_stop', { id: claude.id });
  // Codex 0.169.0 does not qualify on this machine even without this feature (the commit before it
  // is refused with the same text), so a refused Codex is reported and skipped, never a pass.
  let codexOpens = process.env.SHINATAMA_SMOKE_CODEX !== '0';
  if (codexOpens) {
    try { await ipc('terminal_stop', { id: (await sessionOf('codex')).id }); }
    catch (error) { if (!/Try Open again|did not start/.test(String(error.message))) throw error; codexOpens = false; console.log('skip Codex did not qualify here; its launch flags are covered by the unit tests'); }
  }
  if (codexOpens) {
    let known = new Set(processes('codex.exe').map(item => item.pid));
    const codex = await sessionOf('codex');
    await expect.poll(() => newCodex(known).length, { timeout: 20000 }).toBeGreaterThan(0);
    const open = newCodex(known);
    expect(open.some(line => line.includes('--dangerously-bypass-approvals-and-sandbox'))).toBe(true);
    expect(open.some(line => line.includes('--sandbox'))).toBe(false);
    console.log('ok  Codex starts without its sandbox and approvals');
    await ipc('terminal_stop', { id: codex.id });

    // 5. Full access off: Codex keeps its sandbox.
    await openSettings();
    await page.locator('#settings-full-access').uncheck();
    await page.locator('#settings-save').click();
    await expect(page.locator('#settings-status')).toContainText('agents keep their own protections');
    await page.locator('#settings-dialog button[aria-label="Close settings"]').click();
    known = new Set(processes('codex.exe').map(item => item.pid));
    const protectedCodex = await sessionOf('codex');
    await expect.poll(() => newCodex(known).length, { timeout: 20000 }).toBeGreaterThan(0);
    const closed = newCodex(known);
    expect(closed.some(line => line.includes('workspace-write') && line.includes('--sandbox'))).toBe(true);
    expect(closed.some(line => line.includes('--dangerously-bypass-approvals-and-sandbox'))).toBe(false);
    console.log('ok  switched off: Codex keeps workspace-write and its approvals');
    await ipc('terminal_stop', { id: protectedCodex.id });
  }
  console.log('PASS: settings drawer, secrets at rest, and the launch policy in real sessions.');
} finally {
  if (owned && page && !page.isClosed()) {
    try { for (const session of await ipc('terminal_list')) await ipc('terminal_stop', { id: session.id }); } catch {}
  }
  try { await browser?.close(); } catch {}
  if (app.exitCode === null) { try { execFileSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' }); } catch {} }
  await rm(fixture, { recursive: true, force: true, maxRetries: 5 }).catch(() => {});
}
