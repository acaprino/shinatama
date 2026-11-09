// C41/C42 native proof: browse, preview and save project files through the real app.
// Synthetic project only; nothing is executed and no agent is started.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-files-smoke-'));
const data = resolve(fixture, 'data');
const project = resolve(fixture, 'project');
await mkdir(resolve(project, 'docs'), { recursive: true });
await mkdir(resolve(fixture, 'outside'), { recursive: true });
await writeFile(resolve(fixture, 'outside', 'secret.txt'), 'outside-secret');
await writeFile(resolve(project, 'README.md'), '# Hello\n\nSee [guide](docs/guide.md).\n\n<script>alert(1)</script>\n');
await writeFile(resolve(project, 'docs', 'guide.md'), '## Guide\n');
const marker = resolve(fixture, 'ran.txt');
await writeFile(resolve(project, 'run.bat'), `@echo ran > "${marker}"\n`);
execFileSync('powershell', ['-NoProfile', '-Command', `New-Item -ItemType Junction -Path '${resolve(project, 'escape')}' -Target '${resolve(fixture, 'outside')}' | Out-Null`]);
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = { check: 'C41-C42-files', status: 'running', checks: {} };
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
const ipc = async (command, args) => { const r = await reply(command, args); if (!r.ok) throw new Error(`${command}: ${JSON.stringify(r.issue)}`); return r.value; };
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
  await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 30000 });
  const unknown = await reply('files_list', { project, path: '', requestId: 'r0' });
  expect(unknown.ok).toBe(false);
  expect(JSON.stringify(unknown.issue)).toContain('not one of your projects');
  evidence.checks.unrecordedFolderRefused = true;
  const selected = await ipc('project_select', { path: project });
  const root = selected.path;
  const listing = await ipc('files_list', { project: root, path: '', requestId: 'r1' });
  expect(listing.entries.map(e => e.name)).toEqual(['docs', 'escape', 'README.md', 'run.bat']);
  const escape = listing.entries.find(e => e.name === 'escape');
  expect(escape.kind).toBe('link'); expect(escape.linkInside).toBe(false);
  for (const bad of ['escape/secret.txt', '../outside/secret.txt', 'C:/Windows/win.ini', 'README.md:x']) expect((await reply('files_read', { project: root, path: bad })).ok).toBe(false);
  evidence.checks.listingAndRootContainment = true;
  const script = await ipc('files_read', { project: root, path: 'run.bat' });
  expect(script.text).toContain('@echo ran');
  expect(existsSync(marker)).toBe(false);
  evidence.checks.openingNeverExecutes = true;
  const search = await ipc('files_search', { project: root, query: 'guide', requestId: 'r2' });
  expect(search.hits.map(h => h.path)).toEqual(['docs/guide.md']);
  const md = await ipc('files_markdown', { text: '# T\n<script>x</script> [a](https://evil.example)' });
  expect(md.html).not.toContain('<script'); expect(md.html).not.toContain('href=');
  evidence.checks.searchAndInertMarkdown = true;
  const readme = await ipc('files_read', { project: root, path: 'README.md' });
  await writeFile(resolve(project, 'README.md'), '# Changed by an agent\n');
  const conflict = await ipc('files_save', { request: { project: root, path: 'README.md', text: '# Mine\n', baseSha256: readme.sha256, overwrite: false } });
  expect(conflict.state).toBe('conflict');
  expect(await readFile(resolve(project, 'README.md'), 'utf8')).toBe('# Changed by an agent\n');
  const fresh = await ipc('files_read', { project: root, path: 'README.md' });
  const saved = await ipc('files_save', { request: { project: root, path: 'README.md', text: '# Mine\n', baseSha256: fresh.sha256, overwrite: false } });
  expect(saved.state).toBe('saved');
  expect(await readFile(resolve(project, 'README.md'), 'utf8')).toBe('# Mine\n');
  evidence.checks.versionCheckedSave = true;
  // UI: the Files button opens the panel, a Markdown file renders in split view.
  await page.locator('#open-files').click();
  await expect(page.locator('#files-dialog')).toBeVisible();
  await page.locator('#files-dialog .files-list button', { hasText: 'README.md' }).click();
  await expect(page.locator('#files-dialog .files-preview h1')).toHaveText('Mine', { timeout: 10000 });
  await page.locator('#files-dialog .files-editor').fill('# Edited\n\nbody');
  await expect(page.locator('#files-dialog .files-preview h1')).toHaveText('Edited', { timeout: 10000 });
  await page.locator('#files-dialog button.primary', { hasText: 'Save' }).click();
  await expect(page.locator('#files-dialog .files-status')).toHaveText('Saved.', { timeout: 10000 });
  expect(await readFile(resolve(project, 'README.md'), 'utf8')).toBe('# Edited\n\nbody');
  evidence.checks.panelPreviewEditSave = true;
  evidence.status = 'pass';
} catch (error) {
  evidence.status = 'fail'; evidence.error = String(error?.message ?? error).slice(0, 2000); throw error;
} finally {
  await writeFile(resolve(output, 'files.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  await browser?.close().catch(() => {});
  app.kill();
  for (let i = 0; i < 40 && app.exitCode === null; i++) await delay(100);
  await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }).catch(() => {});
}
console.log(`files smoke: ${evidence.status}`);
