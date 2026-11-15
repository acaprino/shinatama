// Thread sidebar grouped by project: the real component in Edge, threads served from memory.
import { chromium, expect } from '@playwright/test';
import { resolve } from 'node:path';
import { createServer } from 'vite';

const root = process.cwd();
const server = await createServer({ configFile: resolve(root, 'apps/desktop/vite.config.ts'), server: { port: 1440, strictPort: true }, logLevel: 'error' });
await server.listen();
const passed = [];
let browser;
try {
  browser = await chromium.launch({ channel: 'msedge', headless: true });
  const page = await browser.newPage({ viewport: { width: 900, height: 700 } });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto('http://127.0.0.1:1440/tests/sidebar-groups.html');
  const groups = () => page.locator('.thread-sidebar-group').evaluateAll(list => list.map(head => head.querySelector('.thread-sidebar-group-name').textContent));
  const titles = () => page.locator('.thread-sidebar-title').allTextContents();
  const log = () => page.evaluate(() => window.__log);

  await expect.poll(groups).toEqual(['beta', 'alpha']);
  await expect.poll(titles).toEqual(['Beta one', 'Alpha one', 'Alpha two']);
  await expect(page.locator('#thread-sidebar-project')).toHaveCount(0);
  await expect(page.locator('.thread-sidebar-group[data-current]')).toHaveText(/alpha/);
  passed.push('saved projects come back, the toolbar project joins them, threads are grouped');

  await expect.poll(async () => (await log()).saved.at(-1)?.activeProjects).toEqual(['p_bbbbbbbbbbbb', 'p_aaaaaaaaaaaa']);
  passed.push('the listed projects are saved, after the saved state was read');

  const beta = page.locator('.thread-sidebar-group[data-project-id="p_bbbbbbbbbbbb"]');
  await beta.locator('.thread-sidebar-group-toggle').click();
  await expect(beta.locator('.thread-sidebar-group-toggle')).toHaveAttribute('aria-expanded', 'false');
  await expect.poll(titles).toEqual(['Alpha one', 'Alpha two']);
  await beta.locator('.thread-sidebar-group-toggle').click();
  await expect.poll(titles).toEqual(['Beta one', 'Alpha one', 'Alpha two']);
  passed.push('a group closes and opens');

  await page.getByRole('button', { name: 'Add a session to Alpha two', exact: true }).click();
  expect((await log()).added).toEqual(['w_aaaaaaaaaaa4']);
  passed.push('+ on a thread asks for a session in that thread');

  await page.getByRole('button', { name: 'New thread in beta', exact: true }).click();
  await expect.poll(async () => (await log()).created).toEqual(['p_bbbbbbbbbbbb']);
  passed.push('+ on a group makes a thread in that project');

  await page.getByRole('button', { name: 'Remove beta from the sidebar', exact: true }).click();
  await expect.poll(groups).toEqual(['alpha']);
  await expect.poll(async () => (await log()).saved.at(-1)?.activeProjects).toEqual(['p_aaaaaaaaaaaa']);
  await page.evaluate(() => window.__sidebar.toggleActive('p_bbbbbbbbbbbb'));
  await expect.poll(groups).toEqual(['alpha', 'beta']);
  passed.push('× and the Project menu tick remove and add a project');

  expect(errors).toEqual([]);
  console.log(passed.map(line => `ok - ${line}`).join('\n'));
} catch (error) {
  console.error('Passed before failure:\n- ' + passed.join('\n- '));
  throw error;
} finally {
  await browser?.close(); await server.close();
}
