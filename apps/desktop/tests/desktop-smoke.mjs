// Essential Windows proof: real frame, ConPTY, nested layout and layout-only reload.
// Run only after the coordinated desktop/helper build.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:net';

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
const app = spawn(resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + port + ' --remote-debugging-address=127.0.0.1',
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile') },
});
let diagnostics = '';
app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-32768); });
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
try {
  for (let attempt = 0; attempt < 60; attempt++) {
    if (app.exitCode !== null) throw new Error('Desktop exited: ' + app.exitCode + '\n' + diagnostics);
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:' + port, { timeout: 1000 }); break; }
    catch { await delay(500); }
  }
  if (!browser) throw new Error('WebView2 unavailable.\n' + diagnostics);
  const context = browser.contexts()[0];
  page = context.pages()[0] || await context.waitForEvent('page');
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
  const pane = id => page.locator('.wm-pane:has([data-view-id="' + id + '"])');
  const shell = async id => {
    await expect(pane(id).locator('.xterm-accessibility')).toContainText('PS ', { timeout: 30000 });
  };
  const type = async (id, command) => {
    await pane(id).locator('.xterm-helper-textarea').focus();
    await page.keyboard.type(command); await page.keyboard.press('Enter');
  };
  await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 30000 });
  expect(new URL(page.url()).hostname).toBe('tauri.localhost');
  expect(await ids()).toEqual([]);
  // Register a real canonical project ID before loading its workspace scope.
  const project = await ipc('project_select', { path: projectPath });
  expect(project.id).toMatch(/^p_[a-z2-7]{12}$/);
  await page.reload();
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  scope = { projectId: project.id, workspaceId: 'main' };
  const saved = async () => {
    // Observe the app's actual acknowledgement before the independent DB read.
    await expect.poll(() => page.locator('.workspace-name').evaluate(label => ({
      state: label.dataset.layoutState, dirty: label.dataset.layoutDirty, pending: label.dataset.layoutPending,
    })), { timeout: 10000 }).toEqual({ state: 'owned', dirty: 'false', pending: 'false' });
    return ipc('workspace_load', { request: scope });
  };
  await expect.poll(async () => (await saved())?.ownership, { timeout: 15000 }).toBe('ours');
  await expect(page.locator('.wm-pane')).toHaveCount(1);
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
  await page.locator('#project-dialog').getByRole('button', { name: 'Cancel', exact: true }).click();

  await expect(page.locator('#resource-status')).toContainText('Resources: available', { timeout: 20000 });
  await page.locator('#new-terminal').click();
  await expect.poll(ids, { timeout: 30000 }).toHaveLength(1);
  const first = (await ids())[0]; await shell(first);
  await expect.poll(async () => (await ipc('terminal_resources')).find(row => row.id === first)?.applied?.jobCommitBytes).toBe(512 * 1024 * 1024);
  await expect(pane(first).locator('.terminal-resources')).toContainText('/ 512 MiB');
  await type(first, "Write-Output ('SHINATAMA_' + 'DESKTOP_OK')");
  await expect(pane(first).locator('.xterm-accessibility')).toContainText('SHINATAMA_DESKTOP_OK');
  const beforeAppearance = await ids();
  await page.getByLabel('Interface theme', { exact: true }).selectOption('section9');
  await expect.poll(async () => (await appearance())?.settings.skin).toBe('section9');
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--accent').trim())).toBe('#78e2dc');
  expect(await ids()).toEqual(beforeAppearance);
  await page.getByRole('button', { name: 'Appearance', exact: true }).click();
  await page.locator('#presentation-follow').check();
  await page.locator('#presentation-size').fill('14');
  await page.locator('#presentation-dialog').getByRole('button', { name: 'Apply', exact: true }).click();
  await expect.poll(async () => (await appearance())?.settings.fontSize).toBe(14);
  await page.locator('#presentation-dialog').getByRole('button', { name: 'Close', exact: true }).click();
  expect(await ids()).toEqual(beforeAppearance);
  const beforeFind = await ids();
  await page.locator('#find-terminal').click();
  await page.locator('#terminal-find-query').fill('SHINATAMA_DESKTOP_OK');
  await expect(page.locator('#terminal-find-dialog [role="status"]')).toHaveText(/\d+ of \d+\+? matching lines/);
  await page.keyboard.press('Escape');
  await expect(page.locator('#terminal-find-dialog')).toBeHidden();
  await expect(pane(first).locator('.xterm-helper-textarea')).toBeFocused();
  expect(await ids()).toEqual(beforeFind);
  await page.locator('#new-terminal').click();
  await expect.poll(ids, { timeout: 30000 }).toHaveLength(2);
  const second = (await ids()).find(id => id !== first); await shell(second);
  await expect(page.locator('.wm-pane')).toHaveCount(1);
  await pane(second).getByLabel('Split edge', { exact: true }).selectOption('right');
  await pane(second).getByRole('button', { name: 'Split', exact: true }).click();
  await expect(page.locator('.wm-pane')).toHaveCount(2);
  const pair = await ids();

  await pane(second).getByRole('button', { name: 'Open terminal', exact: true }).click();
  await expect.poll(ids, { timeout: 30000 }).toHaveLength(3);
  const third = (await ids()).find(id => !pair.includes(id)); await shell(third);
  const rightGroup = await pane(third).getAttribute('data-group-id');
  await pane(third).getByLabel('Destination pane', { exact: true }).selectOption(rightGroup);
  await pane(third).getByLabel('Split edge', { exact: true }).selectOption('down');
  await pane(third).getByRole('button', { name: 'Split', exact: true }).click();
  await expect(page.locator('.wm-pane')).toHaveCount(3);
  const runtimeIds = await ids();
  const separator = page.getByRole('separator').first();
  const beforeResize = await separator.getAttribute('aria-valuenow');
  await separator.focus(); await page.keyboard.press('ArrowRight');
  await expect(separator).not.toHaveAttribute('aria-valuenow', beforeResize);
  expect(await ids()).toEqual(runtimeIds);

  const thirdGroup = await pane(third).getAttribute('data-group-id');
  const thirdPane = page.locator('.wm-pane[data-group-id="' + thirdGroup + '"]');
  await thirdPane.getByRole('button', { name: 'Hide', exact: true }).click();
  await type(first, "Write-Output ('HEALTHY_' + 'WHILE_HIDDEN')");
  await expect(pane(first).locator('.xterm-accessibility')).toContainText('HEALTHY_WHILE_HIDDEN');
  expect(await ids()).toEqual(runtimeIds);
  await expect.poll(async () => {
    const record = await saved();
    return record && JSON.parse(record.layoutJson).views.find(view => view.id === third)?.hidden;
  }).toBe(true);
  const beforeShowRevision = (await saved()).revision;
  await thirdPane.getByRole('button', { name: 'Show', exact: true }).click();
  await expect(thirdPane.getByRole('tab')).toHaveCount(1);
  const expectedActive = await page.locator('.wm-pane').evaluateAll(panes => Object.fromEntries(
    panes.map(pane => [pane.dataset.groupId, pane.querySelector('[role="tab"][aria-selected="true"]')?.dataset.viewId ?? null])));
  await expect.poll(async () => {
    const record = await saved(); if (!record || record.revision <= beforeShowRevision) return false;
    const layout = JSON.parse(record.layoutJson);
    return groupCount(layout.root) === 3 && layout.views.every(view => !view.hidden)
      && groups(layout.root).every(group => group.activeTabId === expectedActive[group.id]);
  }).toBe(true);
  const persisted = JSON.parse((await saved()).layoutJson);
  expect(persisted.views.map(view => view.id).sort()).toEqual(runtimeIds);
  const beforeReload = shape(persisted.root);
  await page.reload();
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await expect(page.locator('.wm-pane')).toHaveCount(3);
  await expect(page.getByLabel('Interface theme', { exact: true })).toHaveValue('section9');
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

  // One complete explicit first-agent flow. No answers, sign-in or model request.
  await expect(page.locator('#resource-status')).toContainText('Resources: available', { timeout: 20000 });
  await page.locator('#session-index').click();
  await page.locator('#session-index-dialog').getByRole('button', { name: 'Start', exact: true }).click();
  await expect.poll(async () => {
    const status = await ipc('sidecar_status');
    if (['failed', 'retired'].includes(status.phase)) throw new Error('Session index refused: ' + JSON.stringify(status));
    return status.phase;
  }, { timeout: 30000 }).toBe('ready');
  await page.getByRole('button', { name: 'Close session index', exact: true }).click();
  await page.locator('#agents').click();
  await page.locator('#agent-setup-dialog').getByRole('button', { name: 'Check', exact: true }).click();
  await expect(page.locator('#agent-setup-status')).toContainText('Check complete', { timeout: 60000 });
  const checkup = await page.locator('.agent-checkup').innerText();
  expect(checkup).toContain('Found'); expect(checkup).toContain('Unknown');
  const openAgent = page.locator('#agent-setup-dialog').getByRole('button', { name: 'Open agent', exact: true });
  if (await openAgent.isDisabled()) throw new Error('Claude check refused: ' + (await page.locator('.agent-issues').innerText()).slice(0, 3000));
  await openAgent.click();
  await expect.poll(async () => {
    if ((await page.locator('#agent-setup-status').textContent()).includes('action did not complete')) {
      throw new Error('Agent open refused: ' + JSON.stringify(await ipc('agent_status')));
    }
    return page.locator('#agent-setup-dialog').isVisible();
  }, { timeout: 30000 }).toBe(false);
  await expect.poll(ids, { timeout: 15000 }).toHaveLength(healthyCount + 1);
  const native = (await ids()).find(id => /^i_[a-z2-7]{12}$/.test(id));
  expect(native).toBeTruthy();
  await expect(pane(native).locator('.xterm-accessibility')).toContainText(/Claude|Welcome|theme|text style/i, { timeout: 20000 });
  await expect.poll(async () => (await ipc('terminal_resources')).find(row => row.id === native)?.applied?.jobCommitBytes).toBe(1024 * 1024 * 1024);
  await expect(pane(native).getByLabel('Terminal color profile', { exact: true })).toHaveValue('claude');
  const agentLayout = JSON.parse((await saved()).layoutJson);
  expect(agentLayout.views.find(view => view.id === native)?.resource.instanceId).toBe(native);
  expect(agentLayout.views.find(view => view.id === native)?.resource.threadId).toMatch(/^t_[a-z2-7]{12}$/);
  await ipc('terminal_stop', { id: native });
  await expect(pane(native).locator('.exit-banner')).toBeVisible({ timeout: 15000 });
  await expect(page.locator('#notice[data-tone="error"]')).toBeHidden();
  await expect(pane(native).getByRole('button', { name: 'Check agent again', exact: true })).toBeVisible();
  await pane(native).getByRole('button', { name: 'Close terminal', exact: true }).click();
  await expect.poll(ids).toHaveLength(healthyCount);

  if (healthyCount > 0) {
    await page.getByRole('button', { name: 'Close window', exact: true }).click();
    await page.locator('#confirm-dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
    expect((await ids()).length).toBe(healthyCount);
  }
  const closed = page.waitForEvent('close', { timeout: 30000 });
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  if (healthyCount > 0) await page.locator('#confirm-dialog').getByRole('button', { name: 'Stop', exact: true }).click();
  await closed;
  expect(errors).toEqual([]);
  console.log('PASS: native borderless controls; real contained terminals and visible limits; nested layout/themes/find/reload; sidecar ready; isolated Claude check and original TUI startup/stop; canonical instance/thread references; confirmed save/close.');
} catch (error) {
  if (page && !page.isClosed()) {
    try {
      const snapshot = await Promise.race([
        page.evaluate(async scope => ({
          resourceControl: await window.__TAURI_INTERNALS__.invoke('resources_telemetry', { telemetry: {} }),
          notice: document.querySelector('#notice')?.textContent?.slice(0, 2048),
          sessionIndex: document.querySelector('#session-index-dialog')?.textContent?.slice(0, 2048),
          agentCheckup: document.querySelector('#agent-setup-dialog')?.textContent?.slice(0, 4096),
          sidecarStatus: await window.__TAURI_INTERNALS__.invoke('sidecar_status'),
          agentStatus: await window.__TAURI_INTERNALS__.invoke('agent_status'),
          panes: [...document.querySelectorAll('.wm-pane')].slice(0, 16).map(pane => ({
            groupId: pane.dataset.groupId, hidden: pane.hidden,
            tabs: [...pane.querySelectorAll('[data-view-id]')].map(tab => ({ id: tab.dataset.viewId, active: tab.getAttribute('aria-selected') })),
            bodyChildren: [...pane.querySelector('.wm-body').children].map(child => ({ tag: child.tagName, class: child.className, hidden: child.hidden, inputs: child.querySelectorAll('.xterm-helper-textarea').length })),
          })),
          bindingsInDocument: [...document.querySelectorAll('.terminal-view')].slice(0, 16).map(view => ({
            groupId: view.closest('.wm-pane')?.dataset.groupId ?? null, parent: view.parentElement?.className,
            hidden: view.hidden, inputs: view.querySelectorAll('.xterm-helper-textarea').length,
          })),
          nativeSessions: await window.__TAURI_INTERNALS__.invoke('terminal_list'),
          saved: scope ? await window.__TAURI_INTERNALS__.invoke('workspace_load', { request: scope }) : null,
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
