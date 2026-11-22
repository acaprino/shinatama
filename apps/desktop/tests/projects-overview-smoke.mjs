// C36 native proof: pins, git facts and an explicitly confirmed worktree through the real app.
// Synthetic projects only; no agent is started.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { openPanel } from './panel-open.mjs';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-overview-smoke-'));
const data = resolve(fixture, 'data');
const repo = resolve(fixture, 'repo'); const plain = resolve(fixture, 'plain');
await mkdir(repo, { recursive: true }); await mkdir(plain, { recursive: true });
const git = (...args) => execFileSync('git', ['-C', repo, ...args], { stdio: 'ignore' });
git('init', '-q', '-b', 'main');
git('-c', 'user.email=t@t', '-c', 'user.name=t', 'commit', '-q', '--allow-empty', '-m', 'init');
await writeFile(resolve(repo, 'draft.txt'), 'x');
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = { check: 'C36-projects-overview', status: 'running', checks: {} };
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
  try { await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 30000 }); }
  catch { throw new Error(`startup: ${JSON.stringify(await reply('storage_location'))}`); }
  const repoRecord = await ipc('project_select', { path: repo });
  const plainRecord = await ipc('project_select', { path: plain });
  let rows = await ipc('project_overview');
  const repoRow = rows.find(r => r.id === repoRecord.id); const plainRow = rows.find(r => r.id === plainRecord.id);
  expect(repoRow.git).toEqual({ state: 'known', branch: 'main', dirty: 0, untracked: 1 });
  expect(['not_repository', 'known']).toContain(plainRow.git.state);
  expect(repoRow.lastThread).toBeNull();
  evidence.checks.gitFacts = true;
  await ipc('project_pin', { projectId: repoRecord.id, pinned: true });
  rows = await ipc('project_overview');
  expect(rows[0].id).toBe(repoRecord.id); expect(rows[0].pinned).toBe(true);
  expect((await reply('project_pin', { projectId: 'p_doesnotexist0', pinned: true })).ok).toBe(false);
  evidence.checks.pinnedFirstAndUnknownRefused = true;
  expect((await reply('project_worktree_plan', { project: fixture, name: 'x' })).ok).toBe(false);
  expect((await reply('project_worktree_plan', { project: repo, name: 'Bad Name' })).ok).toBe(false);
  const plan = await ipc('project_worktree_plan', { project: repo, name: 'task-1' });
  expect(plan.branch).toBe('shinatama/task-1');
  expect(existsSync(plan.path)).toBe(false);
  evidence.checks.planCreatesNothing = true;
  const wrong = await reply('project_worktree_create', { project: repo, name: 'task-1', confirmedPath: resolve(fixture, 'elsewhere') });
  expect(wrong.ok).toBe(false); expect(existsSync(plan.path)).toBe(false);
  const created = await ipc('project_worktree_create', { project: repo, name: 'task-1', confirmedPath: plan.path });
  expect(existsSync(resolve(created, '.git'))).toBe(true);
  const branches = execFileSync('git', ['-C', repo, 'branch', '--list', 'shinatama/*'], { encoding: 'utf8' });
  expect(branches).toContain('shinatama/task-1');
  expect((await reply('project_worktree_create', { project: repo, name: 'task-1', confirmedPath: plan.path })).ok).toBe(false);
  evidence.checks.confirmedWorktreeOnly = true;
  await openPanel(page, 'open-projects', 'Overview');
  await expect(page.locator('#projects-overview-dialog .projects-overview-row').first()).toHaveAttribute('data-project-id', repoRecord.id, { timeout: 15000 });
  await expect(page.locator('#projects-overview-dialog .projects-git').first()).toContainText('main · 0 changed, 1 untracked');
  await page.locator('#projects-overview-dialog .projects-pin').first().click();
  await expect(page.locator('#projects-overview-dialog .projects-pin[aria-pressed="true"]')).toHaveCount(0, { timeout: 10000 });
  evidence.checks.uiListsAndUnpins = true;
  evidence.status = 'pass';
} catch (error) {
  evidence.status = 'fail'; evidence.error = String(error?.message ?? error).slice(0, 2000); throw error;
} finally {
  await writeFile(resolve(output, 'projects-overview.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  await browser?.close().catch(() => {});
  app.kill();
  for (let i = 0; i < 40 && app.exitCode === null; i++) await delay(100);
  try { execFileSync('git', ['-C', repo, 'worktree', 'remove', '--force', resolve(fixture, 'repo-worktrees', 'task-1')], { stdio: 'ignore' }); } catch {}
  await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }).catch(() => {});
}
console.log(`projects-overview smoke: ${evidence.status}`);
