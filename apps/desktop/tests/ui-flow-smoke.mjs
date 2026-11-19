// Essential frontend flows against scripted IPC. No shell, storage or native proof.
import { chromium, expect } from '@playwright/test';
import { resolve } from 'node:path';
import { createServer } from 'vite';
import { dragTab } from './session-flow.mjs';

function installBridge() {
  const state = window.__bridge = { calls: [], listeners: {}, callbacks: {}, nextTerminal: 1,
    live: [], exits: {}, stopped: [], selected: null, workspace: null, deferNextSelection: false,
    presentation: { revision: 0, settings: { version: 1, skin: 'shinatama', followSkin: false, fontFamily: 'profile', fontSize: 13, lineHeight: 115 } } };
  const project = path => ({ id: 'p_aaaaaaaaaaaa', path, name: path.split(/[\\/]/).pop(), last_opened: 1, available: true });
  const rootsConfig = { version: 1, revision: 1, roots: [], defaultRootId: null, nonStandardHistory: [] };
  let callback = 0;
  async function handle(command, args) {
    switch (command) {
      case 'project_path': return state.selected?.path ?? '';
      case 'project_recent': return state.selected ? [state.selected] : [];
      case 'terminal_list': return [...state.live];
      case 'presentation_load': return state.presentation;
      case 'presentation_save': state.presentation = { revision: state.presentation.revision + 1, settings: args.request.settings }; return state.presentation;
      case 'storage_location': return { directory: 'D:\\data', database_path: 'D:\\data\\db', read_only: false, message: null };
      case 'maintenance_status': return { phase: 'idle', completed_at: null, message: null };
      case 'catalog_read': return { scanId: 's1', revision: 1, rootsConfig, roots: [], projects: [], history: [], recent: [], scanning: false, configLoading: false, total: 0, offset: 0, limit: 100, error: null, truncated: false };
      case 'catalog_begin': case 'maintenance_activity': case 'terminal_write': case 'terminal_resize': return null;
      case 'catalog_cancel': return { status: 'cancelled' };
      case 'project_select':
        if (state.deferNextSelection) {
          state.deferNextSelection = false;
          await new Promise(done => { state.releaseSelection = () => { delete state.releaseSelection; done(); }; });
        }
        state.selected = project(args.path); return state.selected;
      case 'workspace_load': return state.workspace;
      case 'workspace_save':
        state.workspace = { projectId: args.request.projectId, workspaceId: args.request.workspaceId,
          revision: args.request.expectedRevision + 1, schemaVersion: 1, layoutJson: args.request.layoutJson, ownership: 'ours', readOnly: false };
        return state.workspace;
      case 'workspace_release': if (state.workspace) state.workspace.ownership = 'unclaimed'; return null;
      case 'new_project_options': return { revision: 1, roots: [], defaultRootId: null };
      case 'terminal_start': {
        const info = { id: 't' + state.nextTerminal++, cwd: args.cwd, shell: 'pwsh' };
        state.live.push(info); return info;
      }
      case 'terminal_read': {
        await new Promise(done => setTimeout(done, 20));
        const code = state.exits[args.id];
        return { chunks: [], eof: code !== undefined, exit_code: code ?? null, error: null };
      }
      case 'terminal_stop': state.stopped.push(args.id); state.live = state.live.filter(info => info.id !== args.id); return null;
      case 'terminal_shutdown': state.live = []; return null;
      case 'plugin:event|listen': state.listeners[args.event] = args.handler; return callback++;
      case 'plugin:window|is_maximized': return false;
      default: return null;
    }
  }
  window.isTauri = true;
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback(handler) { const id = ++callback; state.callbacks[id] = handler; window['_' + id] = handler; return id; },
    unregisterCallback() {},
    invoke(command, args) { state.calls.push({ command, args }); return handle(command, args); },
  };
  state.emit = name => state.callbacks[state.listeners[name]]?.({ event: name, id: 1, payload: null });
}
const root = process.cwd();
const server = await createServer({ configFile: resolve(root, 'apps/desktop/vite.config.ts'), server: { port: 1437, strictPort: true }, logLevel: 'error' });
await server.listen();
let browser;
const passed = [];
try {
  browser = await chromium.launch({ channel: 'msedge', headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 920 } });
  await context.addInitScript(installBridge);
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const calls = command => page.evaluate(name => window.__bridge.calls.filter(call => call.command === name).map(call => call.args), command);
  const runtimeIds = () => page.evaluate(() => window.__bridge.live.map(info => info.id).sort());
  const pane = id => page.locator('.wm-pane:has([data-view-id="' + id + '"])');
  const choose = async path => {
    await page.locator('#folder').fill(path);
    await page.getByRole('button', { name: 'Use folder', exact: true }).click();
    await expect(page.locator('#project-dialog')).toBeHidden();
  };
  await page.goto('http://127.0.0.1:1437/');
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await expect(page.locator('.wm-pane')).toHaveCount(1);
  await page.getByRole('button', { name: 'Open terminal', exact: true }).click();
  await choose('D:\\Projects\\alpha');
  await expect(page.locator('.terminal-view')).toBeVisible();
  expect((await calls('terminal_start')).map(call => call.cwd)).toEqual(['D:\\Projects\\alpha']);
  passed.push('first Open terminal waits for project selection and starts once');

  await page.locator('#project').click();
  await page.evaluate(() => { void window.__bridge.emit('confirm-close'); });
  await expect(page.locator('#confirm-dialog')).toBeVisible();
  const beforeBrowse = (await calls('catalog_browse')).length;
  const beforeCreate = (await calls('new_project_options')).length;
  await page.keyboard.press('F3'); await page.keyboard.press('n');
  await expect(page.locator('#project-dialog')).toBeVisible();
  await expect(page.locator('#new-project-dialog')).toBeHidden();
  expect((await calls('catalog_browse')).length).toBe(beforeBrowse);
  expect((await calls('new_project_options')).length).toBe(beforeCreate);
  for (const id of ['window-close', 'window-minimize']) {
    expect(await page.locator('#' + id).evaluate(element => {
      const rect = element.getBoundingClientRect();
      return document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2)?.closest('button')?.id;
    })).toBe(id);
  }
  await page.locator('#window-minimize').click();
  expect((await calls('plugin:window|minimize')).length).toBe(1);
  await page.locator('#confirm-dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.locator('#project-dialog').getByRole('button', { name: 'Close', exact: true }).click();
  passed.push('overlay blocks picker F3/n while custom frame stays reachable');

  await page.locator('#new-terminal').click();
  await expect(page.getByRole('tab')).toHaveCount(2);
  const ids = await runtimeIds();
  const destination = await pane('t1').getAttribute('data-group-id');
  const tab = id => page.locator('.wm-tab[data-view-id="' + id + '"]');
  // A tab pressed and released without moving is a click: nothing is rearranged.
  await tab('t2').click();
  await expect(page.locator('.wm-pane')).toHaveCount(1);
  await dragTab(page, tab('t2'), pane('t2'), 'right');
  await expect(page.locator('.wm-pane')).toHaveCount(2);
  await expect(page.locator('.wm-drop-preview')).toBeHidden();
  expect(await runtimeIds()).toEqual(ids);
  await dragTab(page, tab('t2'), page.locator('.wm-pane[data-group-id="' + destination + '"]'), 'center');
  await expect(page.locator('.wm-pane')).toHaveCount(1);
  // Escape drops nothing.
  const box = await tab('t2').boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await page.mouse.down();
  await page.mouse.move(box.x + 300, box.y + 200, { steps: 8 }); await expect(page.locator('.wm-drop-preview')).toBeVisible();
  await page.keyboard.press('Escape'); await page.mouse.up();
  await expect(page.locator('.wm-pane')).toHaveCount(1);
  await expect(page.locator('.wm-drop-preview')).toBeHidden();
  // The keyboard twin of a drag to the bottom.
  await tab('t2').focus(); await page.keyboard.press('Alt+Shift+ArrowDown');
  await expect(page.locator('.wm-pane')).toHaveCount(2);
  const secondId = await pane('t2').getAttribute('data-group-id');
  const second = page.locator('.wm-pane[data-group-id="' + secondId + '"]');
  await expect(second.getByRole('tab')).toHaveCount(1);
  expect(await runtimeIds()).toEqual(ids);
  expect(await calls('terminal_stop')).toEqual([]);
  expect((await calls('terminal_start')).length).toBe(2);
  passed.push('mouse drag split/join, Escape and Alt+Shift+arrow preserve the original runtime identities');

  await page.evaluate(() => { window.__bridge.exits.t2 = 3; });
  await expect(second.locator('.exit-banner')).toContainText('code 3');
  await second.getByRole('button', { name: 'Restart terminal', exact: true }).click();
  await expect(second.locator('.exit-banner')).toHaveCount(0);
  expect(await runtimeIds()).toEqual(['t1', 't3']);
  expect((await calls('terminal_start')).at(-1).cwd).toBe('D:\\Projects\\alpha');
  await page.evaluate(() => { window.__bridge.exits.t3 = 0; });
  await second.getByRole('button', { name: 'Close terminal', exact: true }).click();
  await expect(page.locator('#confirm-dialog')).toBeHidden();
  expect(await runtimeIds()).toEqual(['t1']);
  passed.push('exited terminals require explicit restart or close');

  // Fresh mock runtime, with one pending selection completing under an overlay.
  await page.reload();
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await page.evaluate(() => { window.__bridge.deferNextSelection = true; });
  await page.getByRole('button', { name: 'Open terminal', exact: true }).click();
  await page.locator('#folder').fill('D:\\Projects\\delayed');
  await page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect.poll(() => page.evaluate(() => !!window.__bridge.releaseSelection)).toBe(true);
  await page.evaluate(() => { void window.__bridge.emit('confirm-close'); });
  await expect(page.locator('#confirm-dialog')).toBeVisible();
  await page.evaluate(() => { window.__bridge.releaseSelection(); });
  await expect(page.locator('#project-dialog')).toBeHidden();
  expect(await calls('terminal_start')).toEqual([]);
  await page.locator('#confirm-dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.locator('.terminal-view')).toBeVisible();
  expect((await calls('terminal_start')).length).toBe(1);
  await page.locator('#project').click();
  await page.locator('#project-dialog').getByRole('button', { name: 'Close', exact: true }).click();
  expect((await calls('terminal_start')).length).toBe(1);
  passed.push('late selection consumes its pending Open once before a later picker cancel');
  expect(errors).toEqual([]);
  console.log('PASS ' + passed.length + ' essential mock UI flows:\n- ' + passed.join('\n- '));
} catch (error) {
  console.error('Passed before failure:\n- ' + passed.join('\n- ')); throw error;
} finally {
  await browser?.close(); await server.close();
}
