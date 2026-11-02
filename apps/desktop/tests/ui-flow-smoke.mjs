// Frontend flows against a scripted IPC bridge in Edge: no shell, no database and no Rust.
// Real window, ConPTY and storage behavior stays with the desktop smoke tests.
import { chromium, expect } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createServer } from 'vite';

const root = process.cwd();
const output = resolve(root, process.env.SHINATAMA_UI_EVIDENCE ?? '.prototype-evidence');
await mkdir(output, { recursive: true });
const port = 1437;

function installBridge() {
  const state = window.__bridge = { calls: [], listeners: {}, nextTerminal: 1, exits: {}, failNextStart: false, deferNextSelection: false, stopped: [] };
  const project = path => ({ id: 'p_test', path, name: path.split(/[\\/]/).pop(), last_opened: 1, available: true });
  const rootsConfig = { version: 1, revision: 1, roots: [], defaultRootId: null, nonStandardHistory: [] };
  let callback = 0;
  async function handle(command, args) {
    switch (command) {
      case 'project_path': return '';
      case 'project_recent': case 'terminal_list': return [];
      case 'storage_location': return { directory: 'D:\\data', database_path: 'D:\\data\\db', read_only: false, message: null };
      case 'maintenance_status': return { phase: 'idle', completed_at: null, message: null };
      case 'catalog_read': return { scanId: 's1', revision: 1, rootsConfig, roots: [], projects: [], history: [], recent: [], scanning: false, configLoading: false, total: 0, offset: 0, limit: 100, error: null, truncated: false };
      case 'catalog_begin': case 'maintenance_activity': case 'terminal_write': case 'terminal_resize': return null;
      case 'catalog_cancel': return { status: 'cancelled' };
      case 'project_select': {
        if (state.deferNextSelection) {
          state.deferNextSelection = false;
          await new Promise(done => { state.releaseSelection = () => { delete state.releaseSelection; done(); }; });
        }
        return project(args.path);
      }
      case 'new_project_options': return { revision: 1, roots: [], defaultRootId: null };
      case 'terminal_start': {
        if (state.failNextStart) { state.failNextStart = false; throw 'Cannot start this shell.'; }
        return { id: `t${state.nextTerminal++}`, cwd: args.cwd, shell: 'pwsh' };
      }
      case 'terminal_read': {
        await new Promise(done => setTimeout(done, 20));
        const code = state.exits[args.id];
        return code === undefined ? { chunks: [], eof: false, exit_code: null, error: null } : { chunks: [], eof: true, exit_code: code, error: null };
      }
      case 'terminal_stop': state.stopped.push(args.id); return null;
      case 'plugin:event|listen': state.listeners[args.event] = args.handler; return callback++;
      case 'plugin:window|is_maximized': return false;
      default: return null;
    }
  }
  window.isTauri = true;
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback(handler) { const id = ++callback; window[`_${id}`] = handler; state.callbacks = { ...state.callbacks, [id]: handler }; return id; },
    unregisterCallback() {},
    invoke(command, args) { state.calls.push({ command, args }); return handle(command, args); },
  };
  state.emit = name => state.callbacks[state.listeners[name]]?.({ event: name, id: 1, payload: null });
}

const server = await createServer({ configFile: resolve(root, 'apps/desktop/vite.config.ts'), server: { port, strictPort: true }, logLevel: 'error' });
await server.listen();
const browser = await chromium.launch({ channel: 'msedge', headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 920 } });
await context.addInitScript(installBridge);
const page = await context.newPage();
const errors = [];
page.on('pageerror', error => errors.push(error.message));
const calls = name => page.evaluate(command => window.__bridge.calls.filter(call => call.command === command).map(call => call.args), name);
const pickProject = async path => {
  await page.locator('#folder').fill(path);
  await page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect(page.locator('#project-dialog')).toBeHidden();
};
const hitAt = locator => locator.evaluate(element => {
  const box = element.getBoundingClientRect();
  return document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2)?.closest('button,dialog')?.id ?? null;
});
const passed = [];

try {
  await page.goto(`http://127.0.0.1:${port}/`);
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await page.screenshot({ path: resolve(output, 'ui-empty.png') });

  // Text has a floor, whatever the screen; the 9-11px chrome was the review's main reading problem.
  const smallText = await page.evaluate(() => [...document.querySelectorAll('body *')].filter(element =>
    element.getClientRects().length && [...element.childNodes].some(node => node.nodeType === 3 && node.textContent.trim())
    && parseFloat(getComputedStyle(element).fontSize) < 12).map(element => `${element.tagName}.${element.className}: ${element.textContent.trim().slice(0, 24)}`));
  expect(smallText).toEqual([]);
  passed.push('no visible text under 12px');

  // First-run intent survives the picker, in the pane that asked.
  await page.locator('.pane').nth(1).getByRole('button', { name: 'Open terminal' }).click();
  await expect(page.locator('#project-dialog')).toBeVisible();

  // The window stays operable while a dialog is open, and notices stay reachable.
  expect(await hitAt(page.locator('#window-close'))).toBe('window-close');
  expect(await hitAt(page.locator('#window-minimize'))).toBe('window-minimize');
  await expect(page.locator('.toolbar')).toHaveAttribute('inert', '');
  await page.locator('#window-minimize').click();
  await expect.poll(async () => (await calls('plugin:window|minimize')).length).toBe(1);
  await expect(page.locator('#project-dialog')).toBeVisible();
  await page.evaluate(() => { const notice = document.querySelector('#notice'); notice.querySelector('span').textContent = 'probe'; notice.hidden = false; });
  await page.locator('#notice button').click();
  await expect(page.locator('#notice')).toBeHidden();
  passed.push('titlebar and notices stay usable above a dialog');

  // Focus is contained and Escape dismisses, as a native modal would.
  await page.locator('#folder').focus();
  for (let press = 0; press < 30; press++) {
    await page.keyboard.press('Tab');
    expect(await page.evaluate(() => !!document.activeElement.closest('#project-dialog'))).toBe(true);
  }
  await page.keyboard.press('Escape');
  await expect(page.locator('#project-dialog')).toBeHidden();
  await expect(page.locator('.toolbar')).not.toHaveAttribute('inert', '');
  passed.push('focus trap and Escape');

  // Cancelling forgets the request; a later unrelated selection starts nothing.
  await page.locator('#project').click();
  await pickProject('D:\\Projects\\first');
  expect(await calls('terminal_start')).toEqual([]);
  passed.push('cancelled first-run request starts nothing');

  // With a request pending, selecting a project completes it in the requesting pane.
  await page.evaluate(() => window.__bridge.calls.length = 0);
  await page.evaluate(() => { document.querySelector('#project-name').textContent = 'Choose project'; });
  // Reload for a clean no-project state.
  await page.reload();
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await page.locator('.pane').nth(1).getByRole('button', { name: 'Open terminal' }).click();
  await pickProject('D:\\Projects\\alpha');
  await expect(page.locator('.pane').nth(1).locator('.terminal-view')).toBeVisible();
  await expect(page.locator('.pane').nth(0).locator('.terminal-view')).toHaveCount(0);
  expect((await calls('terminal_start'))[0].cwd).toBe('D:\\Projects\\alpha');
  await expect(page.locator('.pane').nth(1)).toHaveClass(/focused/);
  await expect(page.locator('.pane').nth(1).locator('.xterm-helper-textarea')).toBeFocused();
  passed.push('first-run Open terminal completes after choosing a project');

  // A close request while a dialog is open is answered, never silently dropped.
  await page.locator('#project').click();
  await page.evaluate(() => { void window.__bridge.emit('confirm-close'); });
  await expect(page.locator('#confirm-dialog')).toBeVisible();
  await expect(page.locator('#project-dialog')).toBeVisible();
  await page.screenshot({ path: resolve(output, 'ui-confirm-over-picker.png') });
  const browseBeforeOverlayKeys = (await calls('catalog_browse')).length;
  const createBeforeOverlayKeys = (await calls('new_project_options')).length;
  await page.keyboard.press('F3');
  await page.keyboard.press('n');
  await expect(page.locator('#confirm-dialog')).toBeVisible();
  await expect(page.locator('#project-dialog')).toBeVisible();
  await expect(page.locator('#new-project-dialog')).toBeHidden();
  expect((await calls('catalog_browse')).length).toBe(browseBeforeOverlayKeys);
  expect((await calls('new_project_options')).length).toBe(createBeforeOverlayKeys);
  passed.push('F3 and n do not act on the picker below a confirmation');
  await page.locator('#confirm-dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.locator('#confirm-dialog')).toBeHidden();
  await expect(page.locator('#project-dialog')).toBeVisible();
  await expect(page.locator('#folder')).toBeFocused();
  passed.push('window close request with a dialog open reaches the confirmation');
  await page.screenshot({ path: resolve(output, 'ui-picker.png') });
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();

  // Character shortcuts belong to the project button, not to stray focus.
  await page.locator('#layout-toggle').focus();
  await page.keyboard.press('/');
  await page.keyboard.press('n');
  await expect(page.locator('dialog[open]')).toHaveCount(0);
  await page.locator('#layout-toggle').click(); await page.locator('#layout-toggle').click();
  await page.locator('#project').focus();
  await page.keyboard.press('n');
  await expect(page.locator('#new-project-dialog')).toBeVisible();
  await page.screenshot({ path: resolve(output, 'ui-new-project.png') });
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.locator('#project').focus();
  await page.keyboard.press('/');
  await expect(page.locator('#project-filter')).toBeFocused();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  passed.push('n and / fire only on the project button');

  // Hide and Stop leave keyboard focus somewhere that can take it.
  const paneB = page.locator('.pane').nth(1);
  await paneB.getByRole('button', { name: 'Hide terminal view' }).click();
  await expect(paneB.getByRole('button', { name: 'Open terminal' })).toBeFocused();
  await page.keyboard.press('n');
  await expect(page.locator('dialog[open]')).toHaveCount(0);
  await page.locator('#restore').click();
  await expect(paneB.locator('.xterm-helper-textarea')).toBeFocused();
  await paneB.getByRole('button', { name: 'Stop terminal' }).click();
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect(paneB.getByRole('button', { name: 'Open terminal' })).toBeFocused();
  passed.push('focus lands on a usable target after Hide, Restore and Stop');

  // Tabbing into a pane makes it the target of the next new terminal.
  await page.locator('.pane').nth(0).getByRole('button', { name: 'Open terminal' }).focus();
  await expect(page.locator('.pane').nth(0)).toHaveClass(/focused/);
  await paneB.getByRole('button', { name: 'Open terminal' }).focus();
  await expect(paneB).toHaveClass(/focused/);
  passed.push('keyboard focus moves the active pane');

  // A failed start reports as an error, distinct from an informational notice.
  await page.evaluate(() => { window.__bridge.failNextStart = true; });
  await page.locator('#new-terminal').click();
  await expect(page.locator('#notice')).toHaveAttribute('data-tone', 'error');
  await expect(page.locator('#notice')).toHaveAttribute('role', 'alert');
  await expect(page.locator('#notice span')).toHaveText('Cannot start this shell.');
  await page.locator('#notice button').click();
  passed.push('errors are marked as errors');

  // An exited terminal says so, with its code and both ways forward.
  await page.locator('#new-terminal').click();
  await expect(paneB.locator('.terminal-view')).toBeVisible();
  const exitedId = await page.evaluate(() => `t${window.__bridge.nextTerminal - 1}`);
  await page.evaluate(id => { window.__bridge.exits[id] = 3; }, exitedId);
  await expect(paneB.locator('.exit-banner')).toContainText('code 3');
  await expect(paneB.locator('.pane-state')).toHaveText('Exited');
  await expect(paneB.getByRole('tab')).toHaveAttribute('aria-label', /exited/);
  await page.screenshot({ path: resolve(output, 'ui-exited.png') });
  await paneB.getByRole('button', { name: 'Restart terminal' }).click();
  await expect(paneB.locator('.exit-banner')).toHaveCount(0);
  await expect(paneB.getByRole('tab')).toHaveCount(1);
  expect(await page.evaluate(() => window.__bridge.stopped)).toContain(exitedId);
  expect((await calls('terminal_start')).at(-1).cwd).toBe('D:\\Projects\\alpha');
  await expect(paneB.locator('.xterm-helper-textarea')).toBeFocused();
  const restartedId = await page.evaluate(() => `t${window.__bridge.nextTerminal - 1}`);
  await page.evaluate(id => { window.__bridge.exits[id] = 0; }, restartedId);
  await paneB.getByRole('button', { name: 'Close terminal' }).click();
  await expect(paneB.locator('.terminal-view')).toHaveCount(0);
  await expect(page.locator('#confirm-dialog')).toBeHidden();
  passed.push('exit banner shows the code; Restart and Close work');

  // The minimum window keeps its primary dialog actions on screen.
  await page.setViewportSize({ width: 760, height: 500 });
  await page.screenshot({ path: resolve(output, 'ui-min.png') });
  await page.locator('#project').click();
  await expect(page.getByRole('button', { name: 'Use folder', exact: true })).toBeInViewport({ ratio: 1 });
  await page.screenshot({ path: resolve(output, 'ui-picker-min.png') });
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  passed.push('dialog actions stay visible at the minimum window size');

  // A delayed selection can close its picker underneath a close confirmation.
  // Consume the original Open request after Cancel, never on an unrelated later close.
  await page.setViewportSize({ width: 1440, height: 920 });
  await page.reload();
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await page.evaluate(() => { window.__bridge.deferNextSelection = true; });
  await page.locator('.pane').nth(1).getByRole('button', { name: 'Open terminal' }).click();
  await page.locator('#folder').fill('D:\\Projects\\delayed');
  await page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect.poll(() => page.evaluate(() => !!window.__bridge.releaseSelection)).toBe(true);
  await page.evaluate(() => { void window.__bridge.emit('confirm-close'); });
  await expect(page.locator('#confirm-dialog')).toBeVisible();
  await page.evaluate(() => { window.__bridge.releaseSelection(); });
  await expect(page.locator('#project-dialog')).toBeHidden();
  await expect(page.locator('#confirm-dialog')).toBeVisible();
  expect(await calls('terminal_start')).toEqual([]);
  await page.locator('#confirm-dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.locator('.pane').nth(1).locator('.terminal-view')).toBeVisible();
  expect((await calls('terminal_start')).length).toBe(1);
  expect((await calls('terminal_start'))[0].cwd).toBe('D:\\Projects\\delayed');
  await page.locator('#project').click();
  await page.locator('#project-dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.locator('#project-dialog')).toBeHidden();
  expect((await calls('terminal_start')).length).toBe(1);
  passed.push('delayed selection under confirmation consumes the pending start exactly once');

  expect(errors).toEqual([]);
  console.log(`PASS ${passed.length} UI flows:\n- ${passed.join('\n- ')}`);
} catch (error) {
  console.error(`Passed before the failure:\n- ${passed.join('\n- ')}`);
  throw error;
} finally {
  await browser.close();
  await server.close();
}
