// Thread sidebar grouped by project: the real component in Imago's shell in Edge, threads served from memory.
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
  await page.waitForFunction(() => window.__mounted === true);
  const sidebar = page.locator('#thread-sidebar');
  const list = sidebar.locator('.thread-sidebar-list');
  const groups = () => page.locator('.thread-sidebar-group').evaluateAll(list => list.map(head => head.querySelector('.thread-sidebar-group-name').textContent));
  const titles = () => page.locator('.thread-sidebar-title').allTextContents();
  const log = () => page.evaluate(() => window.__log);
  const anchorOf = id => page.locator(`.thread-sidebar-anchor[data-workset-id="${id}"]`);
  const rowOf = id => page.locator('li.thread-sidebar-row', { has: anchorOf(id) });
  const shellAttribute = name => page.locator('#app').getAttribute(name);
  /** The colour of a category of the package, as the browser computes it: the fill of a dot of that category. */
  const category = n => page.evaluate(n => {
    const probe = document.createElement('i'); probe.className = 'im-dot'; probe.dataset.imCat = String(n); document.getElementById('app').append(probe);
    const colour = getComputedStyle(probe).backgroundColor; probe.remove(); return colour;
  }, n);

  // The region is the canon's sidebar, in the shell's body: a titled panel whose body is a listbox of rows, and a foot.
  await expect.poll(groups).toEqual(['beta', 'alpha']);
  await expect.poll(titles).toEqual(['Beta one', 'Alpha one', 'Alpha two']);
  expect(await page.evaluate(() => {
    const region = document.getElementById('thread-sidebar');
    const header = region.querySelector(':scope > .im-panel-header');
    const fold = header?.querySelector('.im-sidebar-collapse');
    const listbox = region.querySelector(':scope > [role="listbox"]');
    return {
      region: region.matches('section.im-panel.im-sidebar.thread-sidebar') && region.parentElement.matches('.im-shell > .im-body') && region.getAttribute('aria-label'),
      title: header?.querySelector('.im-panel-title')?.textContent, count: header?.querySelector('.im-count')?.textContent,
      fold: [fold?.dataset.imLabelCollapse, fold?.dataset.imLabelExpand, fold?.getAttribute('aria-controls')],
      list: listbox?.matches('.im-panel-body.im-list') && listbox.getAttribute('aria-multiselectable'),
      rows: [...listbox.children].every(row => row.matches('li.im-row[role="option"]') && !!row.querySelector(':scope > .im-rail-item, :scope > .im-row-text')),
      railItems: [...listbox.children].every(row => row.matches('.thread-sidebar-note') || !!row.querySelector(':scope > .im-rail-item')),
      foot: !!region.querySelector(':scope > .im-panel-foot'),
      shell: document.getElementById('app').dataset.imSidebar,
      selfStyled: [...region.querySelectorAll('[style]')].map(node => node.getAttribute('style')),
    };
  })).toEqual({ region: 'Threads', title: 'Threads', count: '3', fold: ['Collapse threads', 'Expand threads', 'thread-sidebar'], list: 'true', rows: true, railItems: true, foot: true, shell: 'on', selfStyled: [] });
  await expect(page.locator('#thread-sidebar-project')).toHaveCount(0);
  await expect(page.locator('.thread-sidebar-group[data-current]')).toHaveText(/alpha/);
  passed.push('the sidebar is Imago\'s region in the shell; saved projects come back, the toolbar project joins them, threads are grouped under a row per project');

  await expect.poll(async () => (await log()).saved.at(-1)?.activeProjects).toEqual(['p_bbbbbbbbbbbb', 'p_aaaaaaaaaaaa']);
  passed.push('the listed projects are saved, after the saved state was read');

  const beta = page.locator('.thread-sidebar-group[data-project-id="p_bbbbbbbbbbbb"]');
  await beta.locator('.thread-sidebar-group-toggle').click();
  await expect(beta.locator('.thread-sidebar-group-toggle')).toHaveAttribute('aria-expanded', 'false');
  await expect(beta).toHaveAttribute('aria-label', 'beta, 1 thread, folded');
  await expect.poll(titles).toEqual(['Alpha one', 'Alpha two']);
  await beta.locator('.thread-sidebar-group-toggle').click();
  await expect.poll(titles).toEqual(['Beta one', 'Alpha one', 'Alpha two']);
  passed.push('a group closes and opens');

  const betaOne = rowOf('w_bbbbbbbbbbb3');
  await expect(betaOne).toHaveAttribute('data-im-badge', '');
  await expect(betaOne).toHaveAttribute('data-im-tone', 'warn');
  await expect(betaOne).toHaveAttribute('title', /^Beta one · beta · waiting for you\n/);
  await expect(betaOne.locator('.im-row-sub .thread-sidebar-state')).toHaveText('1 session · waiting for you');
  await expect(rowOf('w_aaaaaaaaaaa3')).not.toHaveAttribute('data-im-badge');
  passed.push('a thread waiting for the user carries the badge (warn), and its title and its line say so');

  await page.getByRole('button', { name: 'Add a session to Alpha two', exact: true }).click();
  expect((await log()).added).toEqual(['w_aaaaaaaaaaa4']);
  passed.push('+ on a thread asks for a session in that thread');

  await page.getByRole('button', { name: 'New thread in beta', exact: true }).click();
  await expect.poll(async () => (await log()).created).toEqual(['p_bbbbbbbbbbbb']);
  passed.push('+ on a group makes a thread in that project');

  const chat = rowOf('w_aaaaaaaaaaa3').locator('.thread-sidebar-chat');
  await expect(chat).toBeVisible();
  await expect(chat).toHaveAttribute('aria-label', 'Conversation of Alpha one · new messages');
  await expect(chat.locator('.im-dot')).toBeVisible();
  await expect(rowOf('w_aaaaaaaaaaa4').locator('.thread-sidebar-chat')).toBeHidden();
  await chat.click();
  expect((await log()).chats).toEqual(['w_aaaaaaaaaaa3']);
  passed.push('Chat is offered on a thread whose sessions talk, with its unread mark in words and a dot');

  const shot = async name => { if (process.env.SHOTS) await page.screenshot({ path: resolve(process.env.SHOTS, name + '.png') }); };
  const alphaTwo = anchorOf('w_aaaaaaaaaaa4');
  const alphaTwoRow = rowOf('w_aaaaaaaaaaa4');
  const state = alphaTwo.locator('.thread-sidebar-state');
  const avatar = alphaTwoRow.locator('.thread-sidebar-switch');
  await expect(alphaTwo).toHaveAttribute('aria-pressed', 'false');
  await expect(alphaTwoRow).toHaveAttribute('aria-selected', 'false');
  await expect(state).toHaveText('2 running · tabs hidden');
  await expect(avatar).not.toHaveAttribute('data-im-cat');
  const offFill = await avatar.evaluate(node => getComputedStyle(node).backgroundColor);
  await alphaTwo.click();
  await expect(alphaTwo).toHaveAttribute('aria-pressed', 'true');
  await expect(alphaTwoRow).toHaveAttribute('aria-selected', 'true');
  await expect(state).toHaveText('3 sessions · 2 running');
  await expect(alphaTwoRow).toHaveAttribute('data-im-cat', '1');
  await expect(avatar).toHaveAttribute('data-im-cat', '1');
  await expect(avatar).toHaveText('AT');
  const onFill = await avatar.evaluate(node => getComputedStyle(node).backgroundColor);
  expect(onFill).toBe(await category(1));
  expect(onFill).not.toBe(offFill);
  await shot('sidebar-on');
  await alphaTwo.click();
  await expect(alphaTwo).toHaveAttribute('aria-pressed', 'false');
  await expect(alphaTwoRow).toHaveAttribute('aria-selected', 'false');
  await expect(avatar).not.toHaveAttribute('data-im-cat');
  expect((await log()).closed).toEqual(['w_aaaaaaaaaaa4']);
  passed.push('a click turns a thread on and off; its row is selected while it is on, its mark and avatar take its category');

  await page.getByRole('button', { name: 'Settings of Alpha two', exact: true }).click();
  const details = page.locator('.thread-sidebar-details');
  await expect(details).toBeVisible();
  expect(await details.evaluate(node => !node.closest('[role="listbox"]'))).toBe(true);
  await expect(details.locator('.thread-sidebar-details-title')).toHaveText('Settings of Alpha two');
  await expect(details.locator('#thread-rename-title')).toHaveValue('Alpha two');
  await expect(details.locator('.thread-sidebar-closed-head')).toHaveText('Closed sessions (1)');
  await expect(details.locator('.thread-sidebar-closed-name')).toHaveText('Codex');
  await expect(page.getByRole('button', { name: 'Settings of Alpha two', exact: true })).toHaveAttribute('aria-expanded', 'true');
  await shot('sidebar-details');
  await details.locator('#thread-rename-title').fill('Billing');
  await details.locator('#thread-rename-title').press('Enter');
  await expect.poll(async () => (await log()).renamed.at(-1)).toEqual({ worksetId: 'w_aaaaaaaaaaa4', expectedRevision: 1, title: 'Billing' });
  await expect(details).toBeHidden();
  await page.getByRole('button', { name: 'Settings of Alpha two', exact: true }).click();
  await expect(details).toBeVisible();
  await page.getByRole('button', { name: 'Settings of Alpha two', exact: true }).click();
  await expect(details).toBeHidden();
  passed.push('⋯ opens a thread\'s name, closed sessions and actions under the list, outside the listbox');

  // Deleting a thread asks in a dialog that names it; Cancel keeps it.
  await page.getByRole('button', { name: 'Settings of Alpha one', exact: true }).click();
  await details.getByRole('button', { name: 'Delete thread', exact: true }).click();
  const dialog = page.getByRole('alertdialog', { name: 'Delete the thread Alpha one?', exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Cancel', exact: true })).toBeFocused();
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect((await log()).deleted).toEqual([]);
  await details.getByRole('button', { name: 'Delete thread', exact: true }).click();
  await dialog.getByRole('button', { name: 'Delete thread', exact: true }).click();
  await expect.poll(async () => (await log()).deleted).toEqual(['w_aaaaaaaaaaa3']);
  await expect(details).toBeHidden();
  passed.push('Delete thread asks in a dialog that names the thread, Cancel first; confirmed, the thread is deleted');

  // The keyboard: the list is one tab stop, the arrows move, Space turns a thread on and off, the Menu keys open its actions.
  await beta.focus();
  await expect(list.locator(':scope > li[tabindex="0"]')).toHaveCount(1);
  await expect(beta).toHaveAttribute('tabindex', '0');
  await page.keyboard.press('ArrowDown');
  await expect(betaOne).toBeFocused();
  await page.keyboard.press('Space');
  await expect(betaOne).toHaveAttribute('aria-selected', 'true');
  await expect(anchorOf('w_bbbbbbbbbbb3')).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Enter');
  await expect(betaOne).toHaveAttribute('aria-selected', 'false');
  expect((await log()).closed).toEqual(['w_aaaaaaaaaaa4', 'w_bbbbbbbbbbb3']);
  await page.keyboard.press('Shift+F10');
  const menu = page.getByRole('menu');
  await expect(menu.getByRole('menuitem')).toHaveText(['Add a session', 'Thread settings']);
  await menu.getByRole('menuitem', { name: 'Thread settings', exact: true }).click();
  await expect(details).toBeVisible();
  await expect(details.locator('#thread-rename-title')).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(details).toBeHidden();
  await expect(betaOne).toBeFocused();
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Space');
  await expect(beta.locator('.thread-sidebar-group-toggle')).toHaveAttribute('aria-expanded', 'false');
  await page.keyboard.press('Space');
  await expect(beta.locator('.thread-sidebar-group-toggle')).toHaveAttribute('aria-expanded', 'true');
  await beta.click({ button: 'right' });
  await expect(menu.getByRole('menuitem')).toHaveText(['New thread', 'Remove from the sidebar']);
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
  passed.push('the list is one tab stop: the arrows move, Space and Enter act on the row, Shift+F10 and the right click open the row\'s actions');

  await page.getByLabel('Show archived threads').check();
  await expect.poll(titles).toEqual([]);
  await expect(beta.locator('xpath=following-sibling::li[1]')).toHaveText('No archived threads.');
  await expect(beta.locator('xpath=following-sibling::li[1]')).toHaveAttribute('aria-disabled', 'true');
  await page.getByLabel('Show archived threads').uncheck();
  await expect.poll(titles).toEqual(['Beta one', 'Alpha one', 'Alpha two']);
  passed.push('the archived view says so under a project with none, and the list comes back');

  // The width: Imago's separator at the sidebar's edge, saved.
  const separator = sidebar.getByRole('separator', { name: 'Thread sidebar width', exact: true });
  await expect(separator).toHaveAttribute('aria-valuenow', '264');
  await separator.focus(); await separator.press('ArrowRight');
  await expect(separator).toHaveAttribute('aria-valuenow', '272');
  await expect.poll(async () => (await log()).saved.at(-1)?.width).toBe(272);
  expect(await page.locator('#app').evaluate(node => node.style.getPropertyValue('--im-w-sidebar'))).toBe('272px');
  await expect(sidebar.locator('.thread-sidebar-resize')).toHaveCount(1);
  passed.push('the separator resizes the sidebar and the width is saved');

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

  // The rail: Imago's fold shows each row's rail item, an avatar per thread; a click turns it on. Closed: no sidebar
  // in the shell; shown again as it was.
  await page.getByRole('button', { name: 'Settings of Alpha two', exact: true }).click();
  await expect(details).toBeVisible();
  await page.getByRole('button', { name: 'Collapse threads', exact: true }).click();
  expect(await shellAttribute('data-im-sidebar-collapsed')).toBe('true');
  await expect(sidebar).toHaveClass(/is-collapsed/);
  const squares = page.locator('.thread-sidebar-rail-item');
  await expect(squares).toHaveCount(3);
  await expect(squares).toHaveText(['AO', 'AT', 'BO']);
  for (const index of [0, 1, 2]) await expect(squares.nth(index)).toBeVisible();
  await expect(page.locator('.thread-sidebar-group .im-rail-item')).toHaveCount(2);
  await expect(page.locator('.thread-sidebar-group .im-rail-item').first()).toBeVisible();
  expect((await alphaTwo.boundingBox())?.width).toBeLessThanOrEqual(1);
  await expect(details).toBeHidden();
  await squares.nth(1).click();
  await expect(alphaTwoRow).toHaveAttribute('aria-selected', 'true');
  await expect(alphaTwo).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(async () => squares.nth(1).evaluate(item => getComputedStyle(item).backgroundColor)).toBe(await category(1));
  await expect.poll(async () => (await log()).saved.at(-1)?.collapsed).toBe(true);
  await shot('sidebar-compact');
  await page.evaluate(() => window.__sidebar.setMode('hidden'));
  await expect(sidebar).toBeHidden();
  expect(await shellAttribute('data-im-sidebar')).toBe('off');
  await expect.poll(async () => (await log()).saved.at(-1)?.hidden).toBe(true);
  await page.evaluate(() => window.__sidebar.setMode(window.__sidebar.compactWhenShown() ? 'compact' : 'open'));
  await expect(squares.first()).toBeVisible();
  expect(await shellAttribute('data-im-sidebar')).toBe('on');
  await page.getByRole('button', { name: 'Expand threads', exact: true }).click();
  expect(await shellAttribute('data-im-sidebar-collapsed')).toBe(null);
  await expect(page.locator('.thread-sidebar-list')).toBeVisible();
  await expect(squares.first()).toBeHidden();
  await expect(details).toBeVisible();
  await page.getByRole('button', { name: 'Settings of Alpha two', exact: true }).click();
  await expect(details).toBeHidden();
  await expect.poll(async () => (await log()).saved.at(-1)).toMatchObject({ collapsed: false });
  await page.getByRole('button', { name: 'Close the thread sidebar', exact: true }).click();
  await expect(sidebar).toBeHidden();
  await page.evaluate(() => window.__sidebar.setMode(window.__sidebar.compactWhenShown() ? 'compact' : 'open'));
  await expect(page.locator('.thread-sidebar-list')).toBeVisible();
  expect((await log()).modes).toEqual(['compact', 'hidden', 'compact', 'open', 'hidden', 'open']);
  await expect.poll(async () => (await log()).saved.at(-1)).toMatchObject({ collapsed: false });
  passed.push('the sidebar folds into the rail (an avatar per thread), closes altogether and comes back as it was; each is saved');

  // Under 620 the title bar's button opens the sidebar as a drawer, also when it was closed above 620.
  await page.evaluate(() => window.__sidebar.setMode('hidden'));
  await page.setViewportSize({ width: 560, height: 700 });
  await page.getByRole('button', { name: 'Threads', exact: true }).click();
  await expect(sidebar).toBeVisible();
  await expect(list.locator(':scope > li[tabindex="0"]')).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(sidebar).toBeHidden();
  await page.setViewportSize({ width: 900, height: 700 });
  await page.evaluate(() => window.__sidebar.setMode('open'));
  await expect(page.locator('.thread-sidebar-list')).toBeVisible();
  passed.push('under 620 the drawer opens the sidebar even when it is closed in the wide window');

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
