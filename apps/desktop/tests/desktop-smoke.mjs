// Essential Windows proof: real frame, ConPTY, nested layout and layout-only reload.
// Run only after the coordinated desktop/helper build.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, opendir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir as systemTmpdir } from 'node:os';
import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:net';
import { dragTab, startSession } from './session-flow.mjs';
// The temporary folder by its long name: a runner's is C:\Users\RUNNER~1\..., and the app answers with the long one.
const tmpdir = () => realpathSync.native(systemTmpdir());

if (process.platform !== 'win32') throw new Error('This test requires Windows.');
const root = process.cwd();
const output = resolve(root, '.prototype-evidence');
await mkdir(output, { recursive: true });
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-desktop-'));
const projectPath = resolve(fixture, 'project');
await mkdir(projectPath);
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
const app = spawn(process.env.SHINATAMA_EXE ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1',
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile') },
});
let diagnostics = '';
app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-32768); });
let started = '';
app.stdout.on('data', chunk => { started = (started + chunk).slice(-32768); });
let browser, nativeHandle, page, scope;
function nativeWindowState(restore = false) {
  return JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-File',
    resolve(root, 'apps/desktop/tests/window-state.ps1'), '-AppProcessId', String(app.pid),
    ...(nativeHandle ? ['-WindowHandle', String(nativeHandle)] : []), ...(restore ? ['-Restore'] : [])],
  { encoding: 'utf8', windowsHide: true }));
}
function shape(node) {
  return node.kind === 'group' ? { kind: node.kind, id: node.id, tabs: node.tabs, collapsed: node.collapsed }
    : { kind: node.kind, id: node.id, axis: node.axis, weights: node.weights, children: node.children.map(shape) };
}
function groupCount(node) { return node.kind === 'group' ? 1 : node.children.reduce((sum, child) => sum + groupCount(child), 0); }
function groups(node) { return node.kind === 'group' ? [node] : node.children.flatMap(groups); }
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();
async function checkScratchNames(directory) {
  const counts = { probe: 0, trial: 0 };
  let inspected = 0;
  // Inspect only child names in this run's verified synthetic C01 root. No file
  // contents, prior app directories, persistent home or archive are read.
  for await (const entry of await opendir(directory)) {
    if (++inspected > 512) throw new Error('Synthetic Check scratch name scan exceeded its bound.');
    const name = entry.name.toLowerCase();
    if (name.startsWith('claude-probe-')) counts.probe++;
    if (name.startsWith('claude-tui-trial-')) counts.trial++;
  }
  return counts;
}
let lastConnectError = 'none';
try {
  for (let attempt = 0; attempt < 60; attempt++) {
    if (app.exitCode !== null) throw new Error('Desktop exited: ' + app.exitCode + '\n' + diagnostics);
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:' + port, { timeout: 1000 }); break; }
    catch (error) { lastConnectError = String(error?.message ?? error).slice(0, 2048); await delay(500); }
  }
  if (!browser) throw new Error('WebView2 unavailable.\n' + diagnostics);
  const context = browser.contexts()[0];
  // The attention overlay (widget.html) is a second WebView; the proof drives the main window.
  const mainPage = () => context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'));
  for (const deadline = Date.now() + 30000; !mainPage() && Date.now() < deadline;) await delay(250);
  page = mainPage();
  if (!page) throw new Error('Main window did not appear.');
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const ipc = async (command, args = {}) => {
    const result = await page.evaluate(async ({ command, args }) => {
      try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
      catch (error) { return { ok: false, code: error?.code ?? 'unavailable', message: error?.message ?? String(error) }; }
    }, { command, args });
    if (!result.ok) throw new Error(command + ': ' + result.code + ': ' + result.message);
    return result.value;
  };
  const appearance = async () => {
    try { return await ipc('presentation_load'); }
    catch (error) { if (error.message.startsWith('presentation_load: busy:')) return null; throw error; }
  };
  const ids = () => ipc('terminal_list').then(sessions => sessions.map(session => session.id).sort());
  const pane = id => page.locator('.im-pane:has([data-view-id="' + id + '"])');
  const shell = async id => {
    await expect(pane(id).locator('.xterm-accessibility')).toContainText('PS ', { timeout: 30000 });
  };
  // "New session" and a pane's own control open the harness menu for the selected project;
  // one choice starts exactly one PowerShell (ADR-0039 v2).
  const openShell = async trigger => { await (await startSession(page, { project: projectPath, harness: 'powershell', trigger })).done(30000); };
  // Panes are arranged by dragging a tab with the mouse onto a side of a pane.
  const tab = id => page.locator('.im-tile-tab[data-view-id="' + id + '"]');
  const type = async (id, command) => {
    await pane(id).locator('.xterm-helper-textarea').focus();
    await page.keyboard.type(command); await page.keyboard.press('Enter');
  };
  await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 30000 });
  expect(new URL(page.url()).hostname).toBe('tauri.localhost');
  const storage = await ipc('storage_location');
  expect(pathKey(storage.directory)).toBe(pathKey(resolve(fixture, 'data')));
  const noScratch = { probe: 0, trial: 0 };
  expect(await checkScratchNames(storage.directory)).toEqual(noScratch);
  expect(await ids()).toEqual([]);
  // Register a real canonical project ID; since dffb269 the layout persists in
  // the global workspace scope, so ownership is read from the global record.
  const project = await ipc('project_select', { path: projectPath });
  expect(project.id).toMatch(/^p_[a-z2-7]{12}$/);
  await page.reload();
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  scope = { workspaceId: 'main' };
  const saved = async () => {
    // Observe the app's actual acknowledgement before the independent DB read.
    await expect.poll(() => page.locator('.workspace-name').evaluate(label => ({
      state: label.dataset.layoutState, dirty: label.dataset.layoutDirty, pending: label.dataset.layoutPending,
    })), { timeout: 10000 }).toEqual({ state: 'owned', dirty: 'false', pending: 'false' });
    return ipc(scope.projectId ? 'workspace_load' : 'global_workspace_load', { request: scope });
  };
  await expect.poll(async () => (await saved())?.ownership, { timeout: 15000 }).toBe('ours');
  await expect(page.locator('.im-pane')).toHaveCount(1);
  expect(await ids()).toEqual([]);

  const agentOnly = process.env.SHINATAMA_SMOKE_FIRST_AGENT_ONLY === '1';
  const healthyCount = agentOnly ? 0 : 2;
  if (!agentOnly) {
  const initial = nativeWindowState(); nativeHandle = initial.handle;
  expect(initial.topInset).toBeLessThanOrEqual(8);
  await page.locator('#project').click();
  await page.getByRole('button', { name: 'Minimize window', exact: true }).click();
  await expect.poll(() => nativeWindowState().minimized).toBe(true);
  nativeWindowState(true);
  await expect.poll(() => nativeWindowState().minimized).toBe(false);
  await page.getByRole('button', { name: 'Maximize window', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Restore window', exact: true })).toBeVisible();
  expect(await ipc('plugin:window|is_maximized')).toBe(true);
  await page.getByRole('button', { name: 'Restore window', exact: true }).click();
  // The project button opens a finder menu, not a dialog: the frame stayed usable over it.
  if (await page.locator('#project-menu').isVisible()) await page.locator('#project').click();
  await expect(page.locator('#project-menu')).toBeHidden();

  await expect(page.locator('#resource-status')).toContainText('Resources: available', { timeout: 20000 });
  await openShell(page.locator('#new-terminal'));
  await expect.poll(ids, { timeout: 30000 }).toHaveLength(1);
  const first = (await ids())[0]; await shell(first);
  // ADR-0055 v2: the first session made a thread and turned it on; the window keeps one layout, `main`.
  expect(await page.locator('.thread-sidebar-anchor[aria-pressed="true"]').getAttribute('data-workset-id')).toMatch(/^w_[a-z2-7]{12}$/);
  await expect.poll(async () => (await ipc('terminal_resources')).find(row => row.id === first)?.applied?.jobCommitBytes).toBe(512 * 1024 * 1024);
  // An interactive session's limits are advisory (545cd63): the job keeps its 512 MiB reservation, but the
  // label shows measured use only, never "used / limit".
  expect((await ipc('terminal_resources')).find(row => row.id === first)?.applied?.limitsEnforced).toBe(false);
  await expect(pane(first).locator('.terminal-resources')).toHaveText(/^\d+ MiB · running$/);
  await type(first, "Write-Output ('SHINATAMA_' + 'DESKTOP_OK')");
  await expect(pane(first).locator('.xterm-accessibility')).toContainText('SHINATAMA_DESKTOP_OK');
  const beforeAppearance = await ids();
  // The look and the theme are the window's Appearance commands (Ctrl+K); ONI dark's skin and the terminal appearance
  // live in the Appearance section of Settings (the title bar's last tab). Every change applies at once
  // and the view stays open.
  await page.locator('.im-titlebar').getByRole('tab', { name: 'Settings', exact: true }).click();
  await expect(page.locator('#settings-dialog')).toBeVisible();
  await expect(page.locator('#toolbar-more-menu')).toBeHidden();
  await page.locator('#project').focus(); await page.keyboard.press('Control+k');
  const commands = page.getByRole('dialog', { name: 'Type a command' });
  await commands.getByRole('combobox').fill('ONI look');
  await commands.getByRole('option').filter({ hasText: 'ONI look' }).first().click();
  await expect(commands).toBeHidden();
  // ONI with the Windows theme is dark, so the skin applies.
  await expect(page.locator('html')).toHaveAttribute('data-im-look', 'oni');
  await page.getByLabel('ONI skin', { exact: true }).selectOption('section9');
  await expect.poll(async () => (await appearance())?.settings.skin).toBe('section9');
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--im-accent').trim())).toBe('#78e2dc');
  expect(await ids()).toEqual(beforeAppearance);
  await page.locator('#presentation-follow').check();
  await expect.poll(async () => (await appearance())?.settings.followSkin).toBe(true);
  await page.locator('#presentation-size').fill('14'); await page.locator('#presentation-size').press('Enter');
  await expect.poll(async () => (await appearance())?.settings.fontSize).toBe(14);
  await page.locator('#settings-dialog button[aria-label="Close settings"]').click();
  await expect(page.locator('#settings-dialog')).toBeHidden();
  expect(await ids()).toEqual(beforeAppearance);
  const beforeFind = await ids();
  await page.locator('#toolbar-more').click(); await page.locator('#find-terminal').click();
  await page.locator('#terminal-find-query').fill('SHINATAMA_DESKTOP_OK');
  await expect(page.locator('#terminal-find-dialog [role="status"]')).toHaveText(/\d+ of \d+\+? matching lines/);
  await page.keyboard.press('Escape');
  await expect(page.locator('#terminal-find-dialog')).toBeHidden();
  await expect(pane(first).locator('.xterm-helper-textarea')).toBeFocused();
  expect(await ids()).toEqual(beforeFind);
  await openShell(page.locator('#new-terminal'));
  await expect.poll(ids, { timeout: 30000 }).toHaveLength(2);
  const second = (await ids()).find(id => id !== first); await shell(second);
  await expect(page.locator('.im-pane')).toHaveCount(1);
  await dragTab(page, tab(second), pane(second), 'right');
  await expect(page.locator('.im-pane')).toHaveCount(2);
  const pair = await ids();

  await openShell(pane(second).locator('.im-add'));
  await expect.poll(ids, { timeout: 30000 }).toHaveLength(3);
  const third = (await ids()).find(id => !pair.includes(id)); await shell(third);
  await dragTab(page, tab(third), pane(third), 'bottom');
  await expect(page.locator('.im-pane')).toHaveCount(3);
  const runtimeIds = await ids();
  const separator = page.getByRole('separator').first();
  const beforeResize = await separator.getAttribute('aria-valuenow');
  await separator.focus(); await page.keyboard.press('ArrowRight');
  await expect(separator).not.toHaveAttribute('aria-valuenow', beforeResize);
  expect(await ids()).toEqual(runtimeIds);

  // Every terminal keeps running through the drags; the arranged panes are saved as they look.
  await type(first, "Write-Output ('HEALTHY_' + 'AFTER_DRAG')");
  await expect(pane(first).locator('.xterm-accessibility')).toContainText('HEALTHY_AFTER_DRAG');
  expect(await ids()).toEqual(runtimeIds);
  const expectedActive = await page.locator('.im-pane').evaluateAll(panes => Object.fromEntries(
    panes.map(pane => [pane.dataset.groupId, pane.querySelector('[role="tab"][aria-selected="true"]')?.dataset.viewId ?? null])));
  await expect.poll(async () => {
    const record = await saved(); if (!record) return false;
    const layout = JSON.parse(record.layoutJson);
    return groupCount(layout.root) === 3 && layout.views.every(view => !view.hidden)
      && groups(layout.root).every(group => group.activeTabId === expectedActive[group.id]);
  }).toBe(true);
  const persisted = JSON.parse((await saved()).layoutJson);
  expect(persisted.views.map(view => view.id).sort()).toEqual(runtimeIds);
  const beforeReload = shape(persisted.root);
  await page.reload();
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await expect(page.locator('.im-pane')).toHaveCount(3);
  await expect(page.locator('#presentation-skin')).toHaveValue('section9');
  expect((await ipc('presentation_load')).settings.followSkin).toBe(true);
  expect(await ids()).toEqual(runtimeIds);
  await expect.poll(async () => shape(JSON.parse((await saved()).layoutJson).root)).toEqual(beforeReload);
  await type(first, "Write-Output ('RELOADED_' + 'ALIVE')");
  await expect(pane(first).locator('.xterm-accessibility')).toContainText('RELOADED_ALIVE');
  expect(await ids()).toEqual(runtimeIds);
  await page.screenshot({ path: resolve(output, 'desktop-terminals.png') });

  // A natural shell exit remains a view until the user explicitly closes it.
  await type(third, 'exit');
  await expect(pane(third).locator('.exit-banner')).toContainText('code 0', { timeout: 15000 });
  await pane(third).getByRole('button', { name: 'Close terminal', exact: true }).click();
  await expect.poll(ids).toHaveLength(2);
  }

  // One complete first-agent flow through the real New session menu (project +
  // Claude Code + Open); the model proxy and the qualification check start
  // implicitly with Open. No answers, sign-in or model request.
  await expect(page.locator('#resource-status')).toContainText('Resources: available', { timeout: 20000 });
  if ((await ipc('sidecar_status')).phase !== 'ready') await ipc('sidecar_start').catch(() => {});
  await expect.poll(async () => {
    const status = await ipc('sidecar_status');
    if (['failed', 'retired'].includes(status.phase)) throw new Error('Model proxy refused: ' + JSON.stringify(status));
    return status.phase;
  }, { timeout: 45000 }).toBe('ready');
  const agentOpen = await startSession(page, { project: projectPath, harness: 'claude_code' });
  // A machine without Claude Code (a GitHub runner) skips the agent part and says so; any other
  // refusal still fails, and so does a missing Claude Code in the first-agent-only run.
  let claudeMissing = false;
  await expect.poll(async () => {
    const refused = await agentOpen.refusal();
    if (refused && !agentOnly && /harness is not installed/i.test(refused)) { claudeMissing = true; return false; }
    if (refused) throw new Error('Agent open refused: ' + refused.slice(0, 300) + ' ' + JSON.stringify(await ipc('agent_status')));
    return agentOpen.pending();
  }, { timeout: 120000 }).toBe(false);
  if (claudeMissing) {
    console.log('SKIP: Claude Code is not installed here; the isolated Claude check and the TUI startup/stop did not run.');
    await page.keyboard.press('Escape');
    await expect(page.locator('#new-session-dialog')).toBeHidden();
    await expect(page.locator('#session-menu')).toBeHidden();
  } else {
  await expect.poll(() => checkScratchNames(storage.directory), {
    timeout: 5000, intervals: [50, 100, 200],
    message: 'A successful open must leave no released version-probe or TUI-canary scratch.',
  }).toEqual(noScratch);
  console.log('Check scratch names: fresh baseline zero; after the implicit check zero.');
  await expect.poll(ids, { timeout: 15000 }).toHaveLength(healthyCount + 1);
  const native = (await ids()).find(id => /^i_[a-z2-7]{12}$/.test(id));
  expect(native).toBeTruthy();
  await expect(pane(native).locator('.xterm-accessibility')).toContainText(/Claude|Welcome|theme|text style/i, { timeout: 20000 });
  await expect.poll(async () => (await ipc('terminal_resources')).find(row => row.id === native)?.applied?.jobCommitBytes).toBe(1024 * 1024 * 1024);
  await expect(pane(native).getByLabel('Terminal color profile', { exact: true })).toHaveValue('claude');
  const agentLayout = JSON.parse((await saved()).layoutJson);
  expect(agentLayout.views.find(view => view.id === native)?.resource.instanceId).toBe(native);
  expect(agentLayout.views.find(view => view.id === native)?.resource.threadId).toMatch(/^t_[a-z2-7]{12}$/);
  const threadId = agentLayout.views.find(view => view.id === native).resource.threadId;
  const sidebar = page.locator('.thread-sidebar');
  // ADR-0055 v2: the sidebar lists the thread that holds the session; turning it off and on again
  // hides and brings back its tab and stops nothing.
  const openThread = sidebar.locator('button.thread-sidebar-anchor[aria-pressed="true"]');
  await expect(openThread.locator('.thread-sidebar-state')).toContainText('running', {timeout:10000});
  const nativeBeforeSelection = await ids();
  const threadRow = sidebar.locator(`button.thread-sidebar-anchor[data-workset-id="${await openThread.getAttribute('data-workset-id')}"]`);
  const runningBefore = (await ipc('terminal_list')).length;
  await threadRow.click();
  await expect(threadRow).toHaveAttribute('aria-pressed', 'false');
  expect((await ipc('terminal_list')).length).toBe(runningBefore);
  await threadRow.click();
  await expect(threadRow).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(ids).toEqual(nativeBeforeSelection);
  const durableThreads = await ipc('thread_list', {request:{projectId:project.id,cursor:null,limit:50}});
  expect(durableThreads.rows.map(row => row.id)).toContain(threadId);
  expect(durableThreads.rows.find(row => row.id === threadId).status).toBe('recorded_open');
  await ipc('terminal_stop', { id: native });
  await expect(pane(native).locator('.exit-banner')).toBeVisible({ timeout: 15000 });
  await expect(page.locator('.im-notice[data-im-tone="err"]')).toHaveCount(0);
  await pane(native).getByRole('button', { name: 'Close terminal', exact: true }).click();
  await expect.poll(ids).toHaveLength(healthyCount);
  if (agentOnly) {
    const separator = sidebar.getByRole('separator', {name:'Thread sidebar width'});
    await separator.focus(); await separator.press('ArrowRight');
    await expect(separator).toHaveAttribute('aria-valuenow','272');
    await sidebar.getByRole('button',{name:'Collapse threads',exact:true}).click();
    await expect.poll(async()=> (await ipc('thread_sidebar_load')).settings,{timeout:10000})
      .toEqual({version:1,width:272,collapsed:true});
    await page.reload();
    await expect(page.locator('#runtime-status')).toContainText('Local runtime');
    await expect(sidebar).toHaveClass(/is-collapsed/);
    await sidebar.getByRole('button',{name:'Expand threads',exact:true}).click();
    await expect(sidebar.getByRole('separator',{name:'Thread sidebar width'})).toHaveAttribute('aria-valuenow','272');
    await expect(sidebar.locator('.thread-sidebar-group[data-project-id="'+project.id+'"]')).toBeVisible({timeout:10000});
    await expect(sidebar.locator('button.thread-sidebar-anchor')).toHaveCount(1);
    expect(await ids()).toEqual([]);
    await expect.poll(async()=> (await ipc('thread_sidebar_load')).settings.collapsed).toBe(false);
    await page.screenshot({path:resolve(output,'c25-thread-sidebar.png'),timeout:3000});
    console.log('PASS: durable real-start thread, read-only selection, sidebar width/collapse saved and restored; no auto-resume.');
  }
  } // the agent part, when Claude Code is installed

  if (healthyCount > 0) {
    await page.getByRole('button', { name: 'Close window', exact: true }).click();
    await page.getByRole('alertdialog', { name: 'Close workspace?', exact: true }).getByRole('button', { name: 'Cancel', exact: true }).click();
    expect((await ids()).length).toBe(healthyCount);
  }
  const closed = page.waitForEvent('close', { timeout: 30000 });
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  if (healthyCount > 0) await page.getByRole('alertdialog', { name: 'Close workspace?', exact: true }).getByRole('button', { name: 'Close workspace', exact: true }).click();
  await closed;
  expect(errors).toEqual([]);
  console.log('PASS: native borderless controls; real contained terminals and visible limits; nested layout/themes/find/reload; sidecar ready; '
    + (claudeMissing ? 'Claude Code not installed, its check skipped; ' : 'isolated Claude check and original TUI startup/stop; canonical instance/thread references; ') + 'confirmed save/close.');
} catch (error) {
  if (!browser) {
    // The app ran but its WebView never answered: what the app logged in this run's synthetic home,
    // and which of its processes exist, say where the start stopped.
    try {
      const log = await readFile(resolve(fixture, 'data', 'shinatama.log'), 'utf8').catch(reason => 'no shinatama.log: ' + reason.message);
      console.error('Start diagnostic, app log:\n' + log.slice(-8192));
      console.error('Start diagnostic, app exit code: ' + app.exitCode + ', stdout: ' + started.slice(-4096));
      console.error('Start diagnostic, processes:\n' + execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
        "Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'shinatama|msedgewebview2' } | ForEach-Object { '{0} {1} parent={2}' -f $_.ProcessId, $_.Name, $_.ParentProcessId }"],
      { encoding: 'utf8', windowsHide: true, timeout: 20000 }));
      // Whether the debugging port reached WebView2, is open, and what Playwright refused.
      console.error('Start diagnostic, last CDP connect error (port ' + port + '): ' + lastConnectError);
      console.error('Start diagnostic, /json/version: ' + await fetch('http://127.0.0.1:' + port + '/json/version', { signal: AbortSignal.timeout(3000) })
        .then(async response => response.status + ' ' + (await response.text()).slice(0, 1024)).catch(reason => 'unreachable: ' + reason.message));
      console.error('Start diagnostic, WebView2 browser arguments:\n' + execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
        "Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedgewebview2.exe' -and $_.CommandLine -notmatch '--type=' } | ForEach-Object { $_.CommandLine }"],
      { encoding: 'utf8', windowsHide: true, timeout: 20000 }).slice(0, 4096));
    } catch (diagnosticError) { console.error('Start diagnostic failed:', diagnosticError.message); }
  }
  if (page && !page.isClosed()) {
    try {
      const snapshot = await Promise.race([
        page.evaluate(async scope => ({
          resourceControl: await window.__TAURI_INTERNALS__.invoke('resources_telemetry', { telemetry: {} }),
          notice: [...document.querySelectorAll('.im-notice')].map(node => node.textContent).join(' | ').slice(0, 2048),
          modelProxy: document.querySelector('#model-proxy-panel')?.textContent?.slice(0, 2048),
          agentCheckup: document.querySelector('#agent-setup-dialog')?.textContent?.slice(0, 4096),
          sidecarStatus: await window.__TAURI_INTERNALS__.invoke('sidecar_status'),
          agentStatus: await window.__TAURI_INTERNALS__.invoke('agent_status'),
          panes: [...document.querySelectorAll('.im-pane')].slice(0, 16).map(pane => ({
            groupId: pane.dataset.groupId, hidden: pane.hidden,
            tabs: [...pane.querySelectorAll('[data-view-id]')].map(tab => ({ id: tab.dataset.viewId, active: tab.getAttribute('aria-selected') })),
            bodyChildren: [...pane.querySelector('.im-tile-body').children].map(child => ({ tag: child.tagName, class: child.className, hidden: child.hidden, inputs: child.querySelectorAll('.xterm-helper-textarea').length })),
          })),
          bindingsInDocument: [...document.querySelectorAll('.terminal-view')].slice(0, 16).map(view => ({
            groupId: view.closest('.im-pane')?.dataset.groupId ?? null, parent: view.parentElement?.className,
            hidden: view.hidden, inputs: view.querySelectorAll('.xterm-helper-textarea').length,
          })),
          nativeSessions: await window.__TAURI_INTERNALS__.invoke('terminal_list'),
          saved: scope ? await window.__TAURI_INTERNALS__.invoke(scope.projectId ? 'workspace_load' : 'global_workspace_load', { request: scope }) : null,
        }), scope),
        delay(2000).then(() => { throw new Error('UI diagnostic deadline exceeded'); }),
      ]);
      const json = JSON.stringify(snapshot, null, 2);
      if (json.length <= 65536) await writeFile(resolve(output, 'c07-desktop-failure.json'), json);
      console.error('Bounded C07 failure snapshot:', json.slice(0, 65536));
      await page.screenshot({ path: resolve(output, 'c07-desktop-failure.png'), timeout: 2000 });
    } catch (diagnosticError) { console.error('Diagnostic capture:', diagnosticError.message); }
  }
  throw error;
} finally {
  if (browser) {
    for (const context of browser.contexts()) for (const page of context.pages()) {
      await page.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')).catch(() => {});
    }
    await browser.close().catch(() => {});
  }
  if (app.exitCode === null) app.kill();
}
