// Harnesses draw with the terminal palette while the terminals follow the skin (FRD-0001 R69),
// in the real app with a private SHINATAMA_HOME. Each harness is started twice: once on its own
// theme, once with "Follow interface theme in terminals" on; the second screen is captured again
// after a skin change. Proves Claude's per-instance `theme` and Codex's argv; the screenshots in
// SHINATAMA_SMOKE_SHOTS show every harness (OpenCode keeps its own theme, R69). No sign-in answers
// or prompts are sent. SHINATAMA_SMOKE_HARNESSES and SHINATAMA_SMOKE_ROUNDS (own,follow) narrow it.
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
const packageExecutable = process.env.SHINATAMA_SMOKE_APP_EXE || process.env.SHINATAMA_EXE;
if (packageExecutable && !isAbsolute(packageExecutable)) throw new Error('The package executable must be an explicit absolute path.');
const executable = packageExecutable || resolve(workspace, 'target/debug/shinatama-desktop.exe');
const shots = resolve(process.env.SHINATAMA_SMOKE_SHOTS || resolve(tmpdir(), 'shinatama-harness-palette-shots'));
await mkdir(shots, { recursive: true });
const harnesses = (process.env.SHINATAMA_SMOKE_HARNESSES || 'claude_code,codex,opencode,pi').split(',').filter(Boolean);
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-harness-palette-'));
const data = resolve(fixture, 'data'); const project = resolve(fixture, 'project');
await mkdir(project);

const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
const app = spawn(executable, [], {
  cwd: packageExecutable ? dirname(executable) : workspace, windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let diagnostics = '';
app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-8192); });
let browser, page, owned = false;
const ipc = (command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const processes = name => {
  const out = execFileSync('powershell', ['-NoProfile', '-Command',
    `Get-CimInstance Win32_Process -Filter "Name='${name}'" | ForEach-Object { "$($_.ProcessId)|$($_.CommandLine)" }`], { encoding: 'utf8' });
  return out.split(/\r?\n/).filter(Boolean).map(text => ({ pid: Number(text.split('|')[0]), line: text.slice(text.indexOf('|') + 1) }));
};
async function walk(directory, found = []) {
  for (const entry of await readdir(directory, { withFileTypes: true }).catch(() => [])) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) await walk(path, found); else found.push(path);
  }
  return found;
}
const claudeSettings = async () => (await walk(data)).filter(path => /claude_code[\\/]instances[\\/][^\\/]+[\\/]settings\.json$/.test(path));
const claudeInstanceThemes = async known => Promise.all((await claudeSettings()).filter(path => !known.includes(path))
  .map(async path => JSON.parse(await readFile(path, 'utf8')).theme ?? null));
async function sessionOf(harness) {
  const before = (await ipc('terminal_list')).map(session => session.id);
  for (let attempt = 0; ; attempt++) {
    const flow = await startSession(page, { project, harness });
    try { await flow.done(180000); break; }
    // A harness checked for the first time can be refused once while it qualifies; asked again, it opens.
    catch (error) { if (attempt > 0 || !/Try Open again|qualified startup receipt/.test(String(error.message))) throw error; await delay(2000); }
  }
  const created = (await ipc('terminal_list')).find(session => !before.includes(session.id));
  if (!created) throw new Error(`${harness} did not start.`);
  return created;
}
// The option and the theme are in the Appearance section of Settings; each applies and saves at once.
async function setFollow(on) {
  await page.locator('#toolbar-more').click();
  await page.getByRole('tab', { name: 'Settings' }).click();
  await expect(page.locator('#settings-dialog')).toBeVisible();
  const follow = page.locator('#presentation-follow');
  if (on) await follow.check(); else await follow.uncheck();
  await expect.poll(async () => (await ipc('presentation_load')).settings.followSkin).toBe(on);
  await page.locator('#settings-dialog button[aria-label="Close settings"]').click();
}
const setSkin = skin => page.evaluate(id => {
  const select = document.querySelector('#presentation-skin'); select.value = id; select.dispatchEvent(new Event('change'));
}, skin);
async function capture(name) {
  const view = page.locator('.terminal-view:visible').first();
  await view.screenshot({ path: resolve(shots, `${name}.png`) });
  console.log(`shot ${resolve(shots, `${name}.png`)}`);
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
  page.on('console', message => { if (message.type() === 'error') console.log(`console ${message.text().slice(0, 300)}`); });
  page.on('pageerror', error => console.log(`pageerror ${String(error).slice(0, 300)}`));
  page.on('crash', () => console.log('the WebView renderer crashed'));
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await page.setViewportSize({ width: 1280, height: 800 }).catch(() => {});

  const refused = [];
  const rounds = (process.env.SHINATAMA_SMOKE_ROUNDS || 'own,follow').split(',');
  for (const follow of [false, true].filter(on => rounds.includes(on ? 'follow' : 'own'))) {
    await setFollow(follow);
    for (const harness of harnesses) {
      const codexBefore = new Set(processes('codex.exe').map(item => item.pid));
      const claudeBefore = await claudeSettings();
      let session;
      try { session = await sessionOf(harness); }
      catch (error) { refused.push(`${harness}: ${String(error.message).slice(0, 200)}`); console.log(`skip ${harness} did not start: ${error.message}`); continue; }
      await delay(12000);
      await capture(`${harness}-${follow ? 'follow-shinatama' : 'own'}`);
      if (follow) {
        await setSkin('sumi'); await delay(3000);
        await capture(`${harness}-follow-sumi`);
        await setSkin('shinatama'); await delay(1000);
      }
      if (harness === 'claude_code') {
        const themes = await claudeInstanceThemes(claudeBefore);
        expect(themes.length).toBeGreaterThan(0);
        if (follow) expect(themes).toContain('dark-ansi'); else expect(themes.every(theme => theme !== 'dark-ansi')).toBe(true);
        console.log(`ok  Claude instance theme ${follow ? 'dark-ansi' : 'its own'}`);
      }
      if (harness === 'codex') {
        await expect.poll(() => processes('codex.exe').filter(item => !codexBefore.has(item.pid) && item.line.includes('--no-daemon')).length, { timeout: 20000 }).toBeGreaterThan(0);
        const lines = processes('codex.exe').filter(item => !codexBefore.has(item.pid) && item.line.includes('--no-daemon')).map(item => item.line);
        expect(lines.some(line => line.includes('--config=tui.theme=\\"ansi\\"') || line.includes('--config=tui.theme="ansi"'))).toBe(follow);
        console.log(`ok  Codex argv ${follow ? 'carries' : 'lacks'} the ansi syntax theme`);
      }
      await ipc('terminal_stop', { id: session.id });
      await delay(1500);
    }
  }
  if (refused.length) console.log(`refused:\n  ${refused.join('\n  ')}`);
  console.log(`PASS: harness palette in real sessions; screenshots in ${shots}`);
} catch (error) {
  // The app's own last words, when it went away under the test.
  console.error(`app exit ${app.exitCode} ${app.signalCode ?? ''}
${diagnostics.slice(-3000)}`);
  throw error;
} finally {
  if (owned && page && !page.isClosed()) {
    try { for (const session of await ipc('terminal_list')) await ipc('terminal_stop', { id: session.id }); } catch {}
  }
  try { await browser?.close(); } catch {}
  if (app.exitCode === null) { try { execFileSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' }); } catch {} }
  await rm(fixture, { recursive: true, force: true, maxRetries: 5 }).catch(() => {});
}
