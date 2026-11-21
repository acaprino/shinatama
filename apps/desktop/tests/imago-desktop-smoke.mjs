// Run after npm run build. The two real Vite entry points share only a simulated native boundary.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, expect } from '@playwright/test';

const dist = resolve(process.env.SHINATAMA_SMOKE_DIST ?? fileURLToPath(new URL('../dist/', import.meta.url)));
const origin = 'https://tauri.localhost';
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.svg': 'image/svg+xml', '.woff2': 'font/woff2' };

function mockDesktop() {
  const callbacks = new Map(), listeners = new Map();
  let callbackId = 0;
  const label = location.pathname === '/widget.html' ? 'attention' : 'main';
  const views = [
    { id: 'fixture-live', hidden: false, resource: { kind: 'terminal', instanceId: 'prototype-fixture-live', threadId: null, profileId: 'neutral' } },
    { id: 'markdown', hidden: false, resource: { kind: 'markdown', resourceId: 'saved-document' } },
  ];
  const children = views.map(view => ({ kind: 'group', id: 'pane-' + view.id, tabs: [view.id], activeTabId: view.id, collapsed: false, lastUsableSize: { width: 460, height: 540 } }));
  const layout = { version: 1, root: { kind: 'split', id: 'split', axis: 'columns', weights: [1, 1], children }, views, focusedGroupId: children[0].id, maximizedGroupId: null };
  let saved = { workspaceId: 'main', revision: 1, schemaVersion: 1, layoutJson: JSON.stringify(layout), ownership: 'ours', readOnly: false };
  const settings = { version: 1, skin: 'shinatama', fontFamily: 'profile', fontSize: 13, lineHeight: 115, followSkin: false };
  const state = window.smoke = {
    calls: [], failure: null, notice: null,
    emit(event, payload) { for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload }); },
  };
  window.isTauri = true;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener(event, id) { listeners.get(event)?.delete(id); } };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label }, currentWebview: { label, windowLabel: label } },
    transformCallback(fn) { callbacks.set(++callbackId, fn); return callbackId; },
    unregisterCallback(id) { callbacks.delete(id); },
    async invoke(command, args = {}) {
      state.calls.push({ command, args });
      if (command === state.failure) throw new Error('Fixture refused ' + command);
      switch (command) {
        case 'plugin:event|listen':
          if (!listeners.has(args.event)) listeners.set(args.event, new Set());
          listeners.get(args.event).add(args.handler);
          return args.handler;
        case 'plugin:event|unlisten': listeners.get(args.event)?.delete(args.eventId); return;
        case 'plugin:window|is_maximized': return false;
        case 'harness_catalog': case 'thread_chat_list': case 'project_recent': case 'terminal_resources': return [];
        case 'terminal_list': return [{ id: 'fixture-live', shell: 'PowerShell', cwd: 'C:/Fixture' }];
        case 'terminal_read': return { chunks: [], eof: false, exit_code: null, error: null };
        case 'resources_telemetry': return { accepted: true, policy: null, resources: { owned_private_commit_bytes: null, system_remaining_commit_bytes: null, observed_processes: null } };
        case 'memory_settings_load': return { enabled: true, marginMib: 128 };
        case 'titles_enabled': return false;
        case 'session_index_status': return { index: { enabled: false } };
        case 'project_leftovers': return { projects: [] };
        case 'project_path': return 'C:/Fixture';
        case 'workset_list': return { rows: [], more: false };
        case 'thread_sidebar_load': return { revision: 1, settings: { version: 1, width: 230, collapsed: false, hidden: false, activeProjects: [] } };
        case 'global_workspace_load': return saved;
        case 'global_workspace_save':
          saved = { ...saved, revision: saved.revision + 1, layoutJson: args.request.layoutJson };
          return saved;
        case 'presentation_load': return { revision: 1, settings };
        case 'storage_location': return { directory: 'C:/Fixture', database_path: 'C:/Fixture/state.sqlite', read_only: false, message: null };
        case 'attention_current': return { enabled: true, muted: false, current: state.notice };
        case 'attention_open': case 'attention_dismiss': state.notice = null; return;
        case 'model_settings_load': throw new Error('Model settings are unavailable in the fixture.');
        default: return null;
      }
    },
  };
}

const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;
const browser = await chromium.launch({
  executablePath,
  channel: !executablePath && process.platform === 'win32' ? 'msedge' : undefined,
});
try {
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, reducedMotion: 'reduce', colorScheme: 'dark' });
  await context.addInitScript(mockDesktop);
  await context.route(origin + '/**', async route => {
    const path = resolve(dist, '.' + decodeURIComponent(new URL(route.request().url()).pathname));
    if (!path.startsWith(dist + sep)) return route.abort();
    try { await route.fulfill({ contentType: types[extname(path)] ?? 'application/octet-stream', body: await readFile(path) }); }
    catch { await route.fulfill({ status: 404, body: '' }); }
  });
  const errors = [];
  context.on('page', page => page.on('pageerror', error => { errors.push(error.message); console.error(error.message); }));
  const page = await context.newPage();
  await page.goto(origin + '/index.html');
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const panes = page.locator('.wm-pane');
  await expect(panes).toHaveCount(2);
  const paneBoxes = () => panes.evaluateAll(nodes => nodes.map(node => { const { x, y, width, height } = node.getBoundingClientRect(); return { x, y, width, height }; }));
  await expect(page.locator('[data-group-id="pane-fixture-live"] .xterm')).toBeVisible();
  // The terminal's minimap is Imago's overview strip, 20px wide at the right edge of the terminal view.
  const strip = await page.locator('[data-group-id="pane-fixture-live"] .terminal-view').evaluate(view => {
    const overview = view.querySelector(':scope > .im-overview');
    if (!overview) return null;
    const box = overview.getBoundingClientRect(), host = view.getBoundingClientRect();
    return { width: box.width, right: host.right - box.right, top: box.top - host.top, hidden: overview.getAttribute('aria-hidden') };
  });
  assert.ok(strip && Math.abs(strip.width - 20) <= 1 && Math.abs(strip.right - 4) <= 1 && Math.abs(strip.top - 10) <= 1 && strip.hidden === 'true',
    'The minimap is Imago\'s overview strip in its place: ' + JSON.stringify(strip));
  const [left, right] = await paneBoxes();
  assert.ok(left.width > 250 && left.height > 400 && right.x >= left.x + left.width - 1, JSON.stringify([left, right]));
  async function more(name) {
    await page.getByRole('button', { name: 'More options' }).click();
    await page.getByRole('menuitem', { name, exact: true }).click();
  }
  await more('Layout');
  await expect.poll(async () => (await paneBoxes())[1].y).toBeGreaterThan(left.y + 100);
  await more('Settings');
  const dialog = page.locator('#settings-dialog');
  await expect(dialog).toBeVisible();
  assert.ok(await dialog.evaluate(node => node instanceof HTMLDialogElement && node.open));
  await expect(page.locator('#settings-status')).toContainText('Reload before saving');
  const zoom = page.locator('#settings-zoom');
  await zoom.selectOption('1.25');
  await page.keyboard.press('Control+=');
  await expect(zoom).toHaveValue('1.5');
  await expect.poll(() => page.evaluate(() => localStorage.getItem('shinatama.zoom'))).toBe('1.5');
  assert.deepEqual(await page.evaluate(() => window.smoke.calls.filter(({ command }) => command === 'plugin:webview|set_webview_zoom').map(({ args }) => args.value)), [1.25, 1.5]);
  const palette = [];
  for (const theme of ['light', 'dark']) {
    await page.locator('#presentation-theme').selectOption(theme);
    await expect(page.locator('html')).toHaveAttribute('data-im-theme', theme);
    palette.push(await page.locator('html').evaluate(node => getComputedStyle(node).getPropertyValue('--im-sheet').trim()));
    await expect.poll(() => page.evaluate(() => window.smoke.calls.filter(call => call.command === 'presentation_window_theme').at(-1)?.args.theme)).toBe(theme);
  }
  assert.ok(palette.every(Boolean) && palette[0] !== palette[1], 'The Imago sheet color follows the theme');
  // Settings is a drawer of Imago's one layer stack, beside live panels: Escape in the terminal belongs to the
  // agent, anywhere else it closes the drawer, and focus goes back to the focused pane (settings-panel.ts).
  assert.equal(await dialog.getAttribute('data-im-layer'), 'drawer');
  // Beside the panes, not over them: the work area gives the drawer its width.
  const drawerBox = await dialog.boundingBox();
  await expect.poll(async () => Math.max(...(await paneBoxes()).map(box => box.x + box.width))).toBeLessThanOrEqual(drawerBox.x + 1);
  assert.equal(await page.locator('.wm-pane').evaluateAll(nodes => nodes.some(node => node.closest('[inert]'))), false, 'The panes stay live beside a drawer');
  await page.locator('[data-group-id="pane-fixture-live"] .xterm-helper-textarea').focus();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeVisible();
  await zoom.focus();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  // settings-panel.ts gives focus back when the close event comes, after the dialog has closed.
  await expect.poll(() => page.evaluate(() => Boolean(document.activeElement?.closest('[data-group-id="pane-fixture-live"]'))), { message: 'Focus is back in the focused pane' }).toBe(true);
  // One press, one action: the control that opened the drawer closes it, and opens nothing else. The drawer
  // covers it, so it is pressed from the keyboard.
  await more('Settings');
  await expect(dialog).toBeVisible();
  await page.getByRole('button', { name: 'More options' }).focus();
  await page.keyboard.press('Enter');
  await expect(dialog).toBeHidden();
  await expect(page.getByRole('menu')).toHaveCount(0);
  await more('Settings');
  await expect(dialog).toBeVisible();
  await page.getByRole('button', { name: 'Close settings' }).click();
  await expect(dialog).toBeHidden();
  await page.evaluate(() => { window.smoke.failure = 'plugin:window|minimize'; });
  await page.locator('#window-minimize').click();
  await expect(page.locator('#notice')).toBeVisible();
  await expect(page.locator('#notice')).toContainText(/minimize|fixture refused/i);

  await page.evaluate(() => window.smoke.emit('confirm-close', null));
  const confirmation = page.getByRole('alertdialog', { name: 'Close workspace?' });
  await expect(confirmation).toBeVisible();
  await page.keyboard.press('Control+Shift+t');
  await expect(page.locator('#session-menu')).toBeHidden();
  await expect(confirmation).toBeVisible();
  await confirmation.getByRole('button', { name: 'Cancel' }).click();
  await expect(confirmation).toBeHidden();
  assert.ok(await page.evaluate(() => !window.smoke.calls.some(call => call.command === 'terminal_shutdown')));

  const widget = await context.newPage();
  await widget.setViewportSize({ width: 440, height: 190 });
  await widget.goto(origin + '/widget.html');
  const card = widget.locator('#attention');
  await expect(card).toBeHidden();
  async function announce(id, sessionId) {
    await widget.evaluate(({ id, sessionId }) => {
      window.smoke.notice = { id, kind: 'input', title: 'Input requested', body: 'Review fixture session.', sessionId, createdAt: 1 };
      window.smoke.emit('attention-changed', null);
    }, { id, sessionId });
    await expect(card).toBeVisible();
    await expect(widget.locator('#attention-title')).toHaveText('Input requested');
    for (const button of [widget.locator('#attention-open'), widget.locator('#attention-dismiss')]) {
      const box = await button.boundingBox();
      assert.ok(box && box.x >= 0 && box.y >= 0 && box.x + box.width <= 441 && box.y + box.height <= 191,
        'Widget actions fit its native 440 × 190 window: ' + JSON.stringify(box));
    }
  }
  await announce('notice-one', 'session-one');
  await widget.evaluate(() => { window.smoke.failure = 'attention_open'; });
  await widget.locator('#attention-open').click();
  await expect(widget.locator('#attention-error')).toBeVisible();
  await widget.evaluate(() => { window.smoke.failure = null; });
  await widget.locator('#attention-open').click();
  await expect(card).toBeHidden();
  await announce('notice-two', null);
  await expect(widget.locator('#attention-open')).toContainText('Apri Shinatama');
  await widget.locator('#attention-dismiss').click();
  await expect(card).toBeHidden();
  const actions = await widget.evaluate(() => window.smoke.calls.filter(({ command }) => ['attention_open', 'attention_dismiss'].includes(command)));
  assert.deepEqual(actions.map(({ command, args }) => [command, args.id]), [['attention_open', 'notice-one'], ['attention_open', 'notice-one'], ['attention_dismiss', 'notice-two']]);
  assert.deepEqual(errors, []);
  console.log('Production desktop and widget startup, pane geometry, toolbar, modal, themes, error feedback and attention actions passed.');
} finally {
  await browser.close();
}
