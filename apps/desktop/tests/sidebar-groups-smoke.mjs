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

  const shot = async name => { if (process.env.SHOTS) await page.screenshot({ path: resolve(process.env.SHOTS, name + '.png') }); };
  const alphaTwo = page.locator('.thread-sidebar-anchor[data-workset-id="w_aaaaaaaaaaa4"]');
  const state = alphaTwo.locator('.thread-sidebar-state');
  await expect(alphaTwo).toHaveAttribute('aria-pressed', 'false');
  await expect(state).toHaveText('2 running · tabs hidden');
  await alphaTwo.click();
  await expect(alphaTwo).toHaveAttribute('aria-pressed', 'true');
  await expect(state).toHaveText('3 sessions · 2 running');
  await expect(alphaTwo.locator('.thread-sidebar-switch')).toHaveCSS('background-color', /rgb/);
  await shot('sidebar-on');
  await alphaTwo.click();
  await expect(alphaTwo).toHaveAttribute('aria-pressed', 'false');
  expect((await log()).closed).toEqual(['w_aaaaaaaaaaa4']);
  passed.push('a click turns a thread on and off, and its row says so');

  await page.getByRole('button', { name: 'Settings of Alpha two', exact: true }).click();
  const details = page.locator('.thread-sidebar-details');
  await expect(details).toBeVisible();
  await expect(details.locator('#thread-rename-title')).toHaveValue('Alpha two');
  await expect(details.locator('.thread-sidebar-closed-head')).toHaveText('Closed sessions (1)');
  await expect(details.locator('.thread-sidebar-closed-name')).toHaveText('Codex');
  await shot('sidebar-details');
  await details.locator('#thread-rename-title').fill('Billing');
  await details.locator('#thread-rename-title').press('Enter');
  await expect.poll(async () => (await log()).renamed.at(-1)).toEqual({ worksetId: 'w_aaaaaaaaaaa4', expectedRevision: 1, title: 'Billing' });
  await expect(details).toBeHidden();
  await page.getByRole('button', { name: 'Settings of Alpha two', exact: true }).click();
  await expect(details).toBeVisible();
  await page.getByRole('button', { name: 'Settings of Alpha two', exact: true }).click();
  await expect(details).toBeHidden();
  passed.push('⋯ opens a thread\'s name, closed sessions and actions under its row');

  await page.getByRole('button', { name: 'Remove beta from the sidebar', exact: true }).click();
  await expect.poll(groups).toEqual(['alpha']);
  expect((await log()).removed).toEqual(['p_bbbbbbbbbbbb']);
  await expect.poll(async () => (await log()).saved.at(-1)?.activeProjects).toEqual(['p_aaaaaaaaaaaa']);
  await page.evaluate(() => window.__sidebar.toggleActive('p_bbbbbbbbbbbb'));
  await expect.poll(groups).toEqual(['alpha', 'beta']);
  await page.evaluate(() => window.__sidebar.toggleActive('p_bbbbbbbbbbbb'));
  await expect.poll(groups).toEqual(['alpha']);
  expect((await log()).removed).toEqual(['p_bbbbbbbbbbbb', 'p_bbbbbbbbbbbb']);
  await page.evaluate(() => window.__sidebar.toggleActive('p_bbbbbbbbbbbb'));
  passed.push('× and the Project menu tick remove and add a project, and the threads of a removed project are turned off');

  // Compact: one square per thread, a click turns it on; closed: no sidebar; shown again as it was.
  await page.getByRole('button', { name: 'Collapse threads', exact: true }).click();
  const squares = page.locator('.thread-sidebar-rail-item');
  await expect(squares).toHaveCount(3);
  await expect(squares).toHaveText(['AO', 'AT', 'BO']);
  await squares.nth(1).click();
  await expect(squares.nth(1)).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(async () => squares.nth(1).evaluate(item => getComputedStyle(item).backgroundColor)).not.toBe('rgba(0, 0, 0, 0)');
  await shot('sidebar-compact');
  await page.getByRole('button', { name: 'Close the thread sidebar', exact: true }).click();
  await expect(page.locator('.thread-sidebar')).toBeHidden();
  await expect.poll(async () => (await log()).saved.at(-1)?.hidden).toBe(true);
  await page.evaluate(() => window.__sidebar.setMode(window.__sidebar.compactWhenShown() ? 'compact' : 'open'));
  await expect(squares).toHaveCount(3);
  await page.getByRole('button', { name: 'Expand threads', exact: true }).click();
  await expect(page.locator('.thread-sidebar-list')).toBeVisible();
  expect((await log()).modes).toEqual(['compact', 'hidden', 'compact', 'open']);
  await expect.poll(async () => (await log()).saved.at(-1)).toMatchObject({ collapsed: false });
  passed.push('the sidebar goes compact (a square per thread), closes altogether and comes back as it was; each is saved');

  // The Project menu repaints its tick when toggleActive settles: after the confirmation, never before it.
  const removal = keep => page.evaluate(async keep => {
    const sidebar = window.__sidebar; const alpha = 'p_aaaaaaaaaaaa';
    if (!sidebar.activeIds().includes(alpha)) await sidebar.toggleActive(alpha);
    window.__holdRemovals();
    let settled = false;
    const pending = sidebar.toggleActive(alpha).then(() => { settled = true; });
    await new Promise(wait => setTimeout(wait, 100));
    const beforeAnswer = settled;
    window.__answerRemovals(keep);
    await pending;
    return { beforeAnswer, listed: sidebar.activeIds().includes(alpha) };
  }, keep);
  expect(await removal(true)).toEqual({ beforeAnswer: false, listed: true });
  expect(await removal(false)).toEqual({ beforeAnswer: false, listed: false });
  passed.push('a removal that asks first settles only after the answer: cancelled, the project stays; confirmed, it goes');

  expect(errors).toEqual([]);
  console.log(passed.map(line => `ok - ${line}`).join('\n'));
} catch (error) {
  console.error('Passed before failure:\n- ' + passed.join('\n- '));
  throw error;
} finally {
  await browser?.close(); await server.close();
}
