// F04 native proof: the instance overview reports observed states with their source.
// One PowerShell terminal in a synthetic Git repo; no agent is started.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-instances-smoke-'));
const data = resolve(fixture, 'data'); const repo = resolve(fixture, 'repo');
await mkdir(repo, { recursive: true });
execFileSync('git', ['init', '-q', '-b', 'main', repo]);
await writeFile(resolve(repo, 'a.txt'), 'x');
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = { check: 'F04-instances', status: 'running', checks: {} };
const reservation = createServer();
await new Promise((ok, no) => { reservation.once('error', no); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, ok); });
const port = reservation.address().port;
await new Promise((ok, no) => reservation.close(error => error ? no(error) : ok()));
const app = spawn(resolve(workspace, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: workspace, windowsHide: true, stdio: 'ignore', env: { ...process.env, SHINATAMA_HOME: data,
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let browser, page;
const reply = (command, args = {}) => page.evaluate(async ({ command, args }) => {
  try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; } catch (issue) { return { ok: false, issue }; }
}, { command, args });
const ipc = async (command, args, attempts = 1) => {
  for (let i = 1; ; i++) { const r = await reply(command, args); if (r.ok) return r.value; if (i >= attempts) throw new Error(`${command}: ${JSON.stringify(r.issue)}`); await delay(1000); }
};
const row = async id => (await ipc('instances_overview')).find(r => r.id === id);
async function waitFor(id, predicate, label) {
  for (const deadline = Date.now() + 30000; Date.now() < deadline; await delay(300)) {
    const batch = await ipc('terminal_read', { id });
    const current = await row(id);
    if (current && predicate(current)) return current;
    if (batch.eof && !predicate(current ?? {})) { /* keep polling the overview */ }
  }
  throw new Error(`timed out waiting for ${label}: ${JSON.stringify(await row(id))}`);
}
const write = (id, text) => ipc('terminal_write', { id, data: Array.from(Buffer.from(text, 'utf8')) });
try {
  for (const deadline = Date.now() + 30000; Date.now() < deadline;) {
    if (app.exitCode !== null) throw new Error('Owned app exited during startup.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; } catch { await delay(250); }
  }
  if (!browser) throw new Error('WebView startup exceeded 30 seconds.');
  for (const deadline = Date.now() + 15000; !page && Date.now() < deadline; await delay(100)) {
    page = browser.contexts()[0].pages().find(p => p.url().includes('tauri.localhost') && !p.url().includes('widget.html'));
  }
  if (!page) throw new Error('Main WebView not found.');
  try { await expect(page.locator('#resource-status')).toContainText('Resources: available', { timeout: 30000 }); }
  catch { throw new Error(`startup: ${JSON.stringify(await reply('storage_location'))}`); }
  const info = await ipc('terminal_start', { cwd: repo, rows: 24, cols: 100 }, 30);
  const first = await row(info.id);
  expect(first.git).toEqual({ state: 'known', branch: 'main', dirty: 0, untracked: 1 });
  expect(first.certain).toBe(false);
  evidence.checks.rowWithGitFacts = true;
  await waitFor(info.id, r => r.stateSource === 'terminal_output', 'output');
  await write(info.id, 'Write-Host ("ring" + [char]7)\r');
  const asked = await waitFor(info.id, r => r.state === 'attention_requested', 'bell');
  expect(asked.stateSource).toBe('terminal_signal'); expect(asked.certain).toBe(false);
  evidence.checks.bellIsUnsureAttention = true;
  await page.locator('#open-instances').click();
  await expect(page.locator(`#instances-dialog tr[data-instance-id="${info.id}"]`)).toContainText('Asked for attention (from a terminal signal, unsure)', { timeout: 10000 });
  await expect(page.locator('#instances-dialog tr button')).toHaveText(['Focus'], { timeout: 10000 });
  await page.locator('#instances-dialog .dialog-heading button').click();
  evidence.checks.uiShowsSourceAndOnlyFocus = true;
  const titles = await ipc('instance_titles', { requests: [{ instanceId: info.id, threadId: null, state: 'quiet' }] });
  expect(titles.enabled).toBe(true); expect(titles.rows[0].reason).toBe('no thread');
  await ipc('titles_set_enabled', { enabled: false });
  const off = await ipc('instance_titles', { requests: [{ instanceId: info.id, threadId: 't_aaaaaaaaaaaa', state: 'quiet' }] });
  expect(off.enabled).toBe(false); expect(off.rows[0].reason).toBe('titles off'); expect(off.rows[0].title).toBeNull();
  await ipc('titles_set_enabled', { enabled: true });
  await ipc('thread_summaries_pause', { threadId: 't_aaaaaaaaaaaa', paused: true });
  const paused = await ipc('instance_titles', { requests: [{ instanceId: info.id, threadId: 't_aaaaaaaaaaaa', state: 'quiet' }] });
  expect(paused.rows[0].reason).toBe('summaries paused'); expect(paused.excluded).toContain('t_aaaaaaaaaaaa');
  evidence.checks.titlesSwitchAndPausedThreadNeverRead = true;
  await write(info.id, 'exit 3\r');
  const ended = await waitFor(info.id, r => r.certain === true, 'exit');
  expect(ended.state).toBe('error'); expect(ended.stateSource).toBe('process');
  evidence.checks.exitIsCertain = true;
  evidence.status = 'pass';
} catch (error) {
  evidence.status = 'fail'; evidence.error = String(error?.message ?? error).slice(0, 2000); throw error;
} finally {
  await writeFile(resolve(output, 'instances.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  await browser?.close().catch(() => {});
  app.kill();
  for (let i = 0; i < 40 && app.exitCode === null; i++) await delay(100);
  await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }).catch(() => {});
}
console.log(`instances smoke: ${evidence.status}`);
