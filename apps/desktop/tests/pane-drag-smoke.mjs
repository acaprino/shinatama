// Pane layout by mouse: the real renderer and layout model in Edge, without the app around them.
import { chromium, expect } from '@playwright/test';
import { resolve } from 'node:path';
import { createServer } from 'vite';
import { dragTab } from './session-flow.mjs';

const root = process.cwd();
const server = await createServer({ configFile: resolve(root, 'apps/desktop/vite.config.ts'), server: { port: 1438, strictPort: true }, logLevel: 'error' });
await server.listen();
const passed = [];
let browser;
try {
  browser = await chromium.launch({ channel: 'msedge', headless: true });
  const page = await browser.newPage({ viewport: { width: 1200, height: 760 } });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto('http://127.0.0.1:1438/tests/pane-drag.html');
  const tab = id => page.locator(`.wm-tab[data-view-id="${id}"]`);
  const paneOf = id => page.locator(`.wm-pane:has(.wm-tab[data-view-id="${id}"])`);
  const panes = () => page.evaluate(() => window.__panes());
  await expect(tab('c')).toBeVisible();
  await expect(page.locator('.wm-more, .wm-controls')).toHaveCount(0);
  passed.push('no layout menu: the pane has only its tabs, + and the pane tools');

  await tab('c').click();
  expect(await panes()).toEqual([['a', 'b', 'c']]);
  await expect(tab('c')).toHaveAttribute('aria-selected', 'true');
  passed.push('a tab pressed and released in place is a click');

  await dragTab(page, tab('c'), paneOf('c'), 'right');
  await expect.poll(panes).toEqual([['a', 'b'], ['c']]);
  expect(await page.evaluate(() => window.__split())).toBe('columns');
  await expect(page.locator('.wm-drop-preview')).toBeHidden();
  await expect(page.locator('.wm-drag-ghost')).toBeHidden();
  passed.push('drag to the right side splits into two columns');

  await dragTab(page, tab('b'), paneOf('a'), 'bottom');
  await expect.poll(panes).toEqual([['a'], ['b'], ['c']]);
  passed.push('drag to the bottom splits into rows');

  await dragTab(page, tab('b'), paneOf('c'), 'center');
  await expect.poll(panes).toEqual([['a'], ['c', 'b']]);
  passed.push('drag to the centre joins the tabs of that pane');

  // Reorder in a tab strip: drop b on the left half of c.
  const from = await tab('b').boundingBox(); const target = await tab('c').boundingBox();
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2); await page.mouse.down();
  await page.mouse.move(target.x + 4, target.y + target.height / 2, { steps: 10 });
  await expect(page.locator('.wm-drop-marker')).toBeVisible();
  await page.mouse.up();
  await expect.poll(panes).toEqual([['a'], ['b', 'c']]);
  passed.push('drop on the tab strip reorders, with an insertion mark');

  const before = await panes(); const box = await tab('b').boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await page.mouse.down();
  await page.mouse.move(box.x - 300, box.y + 300, { steps: 10 });
  await expect(page.locator('.wm-drop-preview')).toBeVisible();
  await page.keyboard.press('Escape'); await page.mouse.up();
  expect(await panes()).toEqual(before);
  await expect(page.locator('.wm-drop-preview')).toBeHidden();
  passed.push('Escape cancels a drag');

  await tab('c').focus(); await page.keyboard.press('Alt+Shift+ArrowUp');
  await expect.poll(panes).toEqual([['a'], ['c'], ['b']]);
  await tab('a').focus(); await page.keyboard.press('Alt+Shift+ArrowRight');
  await expect.poll(async () => (await panes()).flat().sort()).toEqual(['a', 'b', 'c']);
  expect((await panes()).length).toBe(2);
  passed.push('Alt+Shift+arrow splits a tab off, or moves a pane\'s only tab to the pane on that side');

  expect(errors).toEqual([]);
  console.log(passed.map(line => `ok - ${line}`).join('\n'));
} catch (error) {
  console.error('Passed before failure:\n- ' + passed.join('\n- '));
  throw error;
} finally {
  await browser?.close(); await server.close();
}
