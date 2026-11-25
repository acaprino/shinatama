// Smoke test of the whole window: the three views together, on Vite with the mocked commands (src/mock/), driven by what a person and a
// screen reader see (roles, names and aria attributes, never a class). What the smokes of the areas do not say, because it is between
// them: the views and the title of the window in each, the keys of the shell (Ctrl+K, Ctrl+F) reaching the view in use, the palette's
// commands, a new session from the palette to its terminal, the terminals staying alive while the view changes, a file opened in a pane,
// the theme, the zoom and the glass (which only a window of Shinatama's own has), and the frame at 340, 620, 1000 and 1440 in both schemes.
// Screenshots go under tests/out/ (git-ignored).
//   node apps/desktop/tests/app.smoke.mjs
// What a real window alone proves (the glass, the native zoom, the first frame, the title in Alt+Tab) is in tests/native/.
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { launch, serve, watch } from './smoke-lib.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const out = `${here}out/`;
mkdirSync(out, { recursive: true });
const { server, url } = await serve(1480);
const browser = await launch({ args: ['--disable-lcd-text'] });
const failures = [];
const check = (ok, message) => { if (!ok) failures.push(message); };

/** The widths where the frame changes (340 is the smallest; 620, 860 and 1000 are its steps) and a wide one. */
const SIZES = [[340, 700], [620, 700], [1000, 700], [1440, 900]];
const VIEWS = ['Workspace', 'Sessions', 'Settings'];

/** What sticks out of the page or of a region of the frame, sideways. */
const overflow = (page) => page.evaluate(() => {
  const found = [];
  const wide = document.documentElement;
  if (wide.scrollWidth > wide.clientWidth) found.push(`the page is ${wide.scrollWidth} wide in ${wide.clientWidth}`);
  for (const slot of ['title-bar', 'app-shell-status', 'sidebar', 'app-shell-content', 'app-shell-footer']) {
    for (const region of document.querySelectorAll(`[data-slot="${slot}"]`)) {
      if (region.clientWidth > 1 && region.scrollWidth > region.clientWidth) found.push(`${slot} is ${region.scrollWidth} wide in ${region.clientWidth}`);
    }
  }
  return found;
});

/** A page of its own, with the scheme kept, the errors counted as failures, and the window started (its terminals reattached). */
async function open(name, { width = 1440, height = 900, scheme = 'dark', storage = {} } = {}) {
  const page = await browser.newPage({ viewport: { width, height }, colorScheme: scheme, deviceScaleFactor: 1 });
  await page.addInitScript(([theme, extra]) => {
    try { localStorage.setItem('shinatama.theme', theme); for (const [key, value] of Object.entries(extra)) localStorage.setItem(key, value); } catch { /* no storage */ }
  }, [scheme, storage]);
  watch(page, name, failures);
  await page.goto(url);
  await page.getByRole('region', { name: 'Pane 1' }).waitFor({ state: 'attached' });
  await page.getByText(/^Local runtime/).waitFor({ state: 'attached' });
  return page;
}

/** The view of the bar: a button in the title bar, a tab in the row under it. */
const viewButton = (page, name) => page.getByRole('button', { name, exact: true }).or(page.getByRole('tab', { name, exact: true })).or(page.getByRole('link', { name, exact: true })).first();
const goTo = (page, name) => viewButton(page, name).click();
const titleIs = (page, view) => page.waitForFunction((text) => document.title === text, `${view} - Shinatama`);

/** Puts away the notices, so that none sits over what a step presses. */
const clearNotices = async (page) => {
  const close = page.getByRole('status', { name: 'Notices' }).getByRole('button');
  for (let n = await close.count(), tries = 0; n > 0 && tries < 8; n = await close.count(), tries += 1) { await close.first().click({ timeout: 1500 }).catch(() => {}); await page.waitForTimeout(100); }
};

/** Runs a command by its name in the palette. */
async function command(page, name) {
  await clearNotices(page);
  await page.getByRole('button', { name: /Commands/ }).first().click();
  await page.getByRole('combobox', { name: /command/i }).fill(name);
  await page.keyboard.press('Enter');
  await page.getByRole('combobox', { name: /command/i }).waitFor({ state: 'detached' });
}

/** The text a terminal shows (xterm keeps it in the rows of the screen reader's tree). */
const screenOf = (page) => page.evaluate(() => document.body.textContent ?? '');

try {
  // --- the frame at every width, in both schemes: every view, nothing sideways, nothing said in the console ------------------------
  for (const scheme of ['dark', 'light']) {
    for (const [width, height] of SIZES) {
      const page = await open(`${scheme} ${width}`, { width, height, scheme });
      for (const view of VIEWS) {
        await goTo(page, view);
        await titleIs(page, view);
        await page.waitForTimeout(250);
        for (const problem of await overflow(page)) failures.push(`${scheme} ${width} ${view}: ${problem}`);
        if (width === 1440 || width === 340) await page.screenshot({ path: `${out}app-${scheme}-${width}-${view.toLowerCase()}.png`, animations: 'disabled' });
      }
      await page.close();
    }
  }

  // --- the views: the title of the window, the status strip and the keys of each ------------------------------------------------------
  const page = await open('views');
  await titleIs(page, 'Workspace');
  const status = page.locator('[data-slot="app-shell-status"]');
  await status.getByText('4 terminals · 3 panes').waitFor();
  const footer = page.locator('[data-slot="app-shell-footer"]');
  check((await footer.textContent()).includes('New session'), 'the foot of the Workspace names the keys of its commands');

  await goTo(page, 'Sessions');
  await titleIs(page, 'Sessions');
  await page.getByRole('treegrid', { name: 'Sessions' }).waitFor();
  await status.getByText('Archives read').waitFor();
  await status.getByText('6 saved · 2 running').waitFor();
  await status.getByText('Search index on').waitFor();
  check(await status.getByText('terminals').count() === 0, 'the status of Sessions does not count the terminals of the Workspace');
  check(await page.getByRole('region', { name: 'Pane 1' }).isVisible() === false, 'the panes are not in sight in Sessions');
  // The focus is not left in a terminal that is hidden: a key typed now does not go to it.
  check(await page.evaluate(() => !document.activeElement?.closest('.xterm')), 'the focus is not in a hidden terminal');
  // Ctrl+F is the filter of the list.
  await page.keyboard.press('Control+f');
  check(await page.evaluate(() => document.activeElement?.getAttribute('aria-label')?.startsWith('Filter sessions') === true), 'Ctrl+F in Sessions puts the keyboard in the filter');
  await page.keyboard.type('idempotency');
  await page.getByRole('row', { name: /Idempotency keys/ }).waitFor();
  await status.getByText('1 saved · 0 running').waitFor();
  await page.getByRole('searchbox', { name: /Filter sessions/ }).fill('');

  await goTo(page, 'Settings');
  await titleIs(page, 'Settings');
  await page.getByRole('region', { name: 'Appearance' }).waitFor();
  await page.getByRole('button', { name: 'Save' }).waitFor();
  // Settings has no search: the shell says so.
  await page.keyboard.press('Control+f');
  await page.getByText('This view has no search.').first().waitFor();
  check((await footer.textContent()).includes('F6'), 'the foot always names F6');

  // Ctrl+F in the Workspace is the search of the terminal in the pane in use.
  await goTo(page, 'Workspace');
  await titleIs(page, 'Workspace');
  await page.getByRole('region', { name: 'Pane 1' }).waitFor({ state: 'visible' });
  await clearNotices(page);
  await page.keyboard.press('Control+f');
  const find = page.getByRole('dialog', { name: 'Find in terminal' });
  await find.waitFor();
  await page.keyboard.press('Escape');
  await find.waitFor({ state: 'detached' });

  // --- the palette: the shell's commands and Shinatama's ---------------------------------------------------------------------------------
  await clearNotices(page);
  await page.getByRole('button', { name: /Commands/ }).first().click();
  const palette = page.getByRole('combobox', { name: /command/i });
  await palette.waitFor();
  const listed = await page.getByRole('option').allTextContents();
  for (const name of ['New session', 'Choose the project', 'Projects at a glance', 'Project files', 'What agents left on disk', 'Go to the Workspace', 'Go to Sessions', 'Go to Settings', 'Light theme', 'Dark theme', 'System theme']) {
    check(listed.some((text) => text.startsWith(name)), `the palette lists "${name}"`);
  }
  check(!listed.some((text) => /^Glass/.test(text)), 'a browser has no glass: the palette does not offer it');
  await page.keyboard.press('Escape');
  await palette.waitFor({ state: 'detached' });
  // Every view is a command, and the one in use is checked.
  await command(page, 'Go to Sessions');
  await titleIs(page, 'Sessions');
  await page.getByRole('treegrid', { name: 'Sessions' }).waitFor();
  await command(page, 'Go to Settings');
  await titleIs(page, 'Settings');
  await command(page, 'Go to the Workspace');
  await titleIs(page, 'Workspace');
  // A command of Shinatama's own opens its screen from any view: Projects at a glance is a dialog, the files a panel of the inspector.
  await command(page, 'Go to Sessions');
  await command(page, 'Projects at a glance');
  const overview = page.getByRole('dialog', { name: /Projects/ });
  await overview.waitFor();
  await page.keyboard.press('Escape');
  await overview.waitFor({ state: 'detached' });
  await command(page, 'Go to the Workspace');
  await command(page, 'What agents left on disk');
  await page.getByRole('heading', { name: 'Leftovers', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await page.getByRole('heading', { name: 'Leftovers', exact: true }).waitFor({ state: 'detached' });

  // --- a new session, from the palette to its terminal ------------------------------------------------------------------------------------
  const tabsBefore = await page.getByRole('tab').allTextContents();
  await command(page, 'New session');
  const session = page.getByRole('dialog', { name: 'New session' });
  await session.waitFor();
  await session.getByRole('button', { name: 'Codex' }).click();
  await session.getByRole('button', { name: /^xiox/ }).click();
  check(await session.getByRole('textbox', { name: 'Project folder' }).inputValue() === 'C:\\work\\xiox', 'the project chosen from the list is the folder');
  await session.getByRole('button', { name: 'Open', exact: true }).click();
  await session.waitFor({ state: 'detached' });
  await status.getByText('5 terminals').waitFor({ timeout: 10000 });
  const tabsAfter = await page.getByRole('tab').allTextContents();
  const added = tabsAfter.filter((name) => !tabsBefore.includes(name));
  check(added.length === 1, `one tab is new: ${added.join(' | ')}`);
  const fresh = page.getByRole('tab', { name: added[0], exact: true });
  await fresh.click();
  const terminal = page.getByRole('tabpanel', { name: added[0] }).getByRole('textbox', { name: /^Terminal of / });
  await terminal.waitFor({ state: 'attached' });
  await terminal.focus();
  await page.keyboard.type('hello-from-the-new-session');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => (document.body.textContent ?? '').includes('hello-from-the-new-session'));
  await page.screenshot({ path: `${out}app-new-session.png`, animations: 'disabled' });

  // --- the terminals stay alive while the view changes ----------------------------------------------------------------------------------------
  for (const view of ['Sessions', 'Settings', 'Sessions', 'Workspace']) { await goTo(page, view); await titleIs(page, view); }
  await page.getByRole('region', { name: 'Pane 1' }).waitFor({ state: 'visible' });
  await page.waitForTimeout(300);
  check((await screenOf(page)).includes('hello-from-the-new-session'), 'a terminal keeps what it printed across the views');
  await status.getByText('5 terminals').waitFor();
  // It still takes what is typed.
  await terminal.focus();
  await page.keyboard.type('still-alive');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => (document.body.textContent ?? '').includes('still-alive'));

  // --- a file in a pane: from the files of the project to a tab beside the terminals -----------------------------------------------------------
  await command(page, 'Project files');
  const files = page.getByRole('complementary', { name: 'Project files' });
  await files.waitFor();
  const tree = files.getByRole('tree', { name: 'Folder tree' });
  await tree.waitFor();
  await tree.getByText('README.md').first().click().catch(() => {});
  let opened = await files.getByRole('button', { name: 'Open in a pane' }).count();
  if (!opened) {
    // The first level holds folders: open the first one, and a file of it.
    await tree.getByRole('treeitem').first().click();
    await page.waitForTimeout(200);
    const leaf = tree.getByRole('treeitem').filter({ hasNot: page.locator('[aria-expanded]') }).first();
    await leaf.click();
    opened = await files.getByRole('button', { name: 'Open in a pane' }).count();
  }
  check(opened === 1, 'a file chosen in the tree can be opened in a pane');
  await files.getByRole('button', { name: 'Open in a pane' }).click();
  const fileTab = page.getByRole('tab', { name: /\.(md|ts|json|rs)$/ }).last();
  await fileTab.waitFor();
  const code = page.getByRole('textbox', { name: /^Source of / }).last();
  await code.waitFor();
  await page.screenshot({ path: `${out}app-file-pane.png`, animations: 'disabled' });
  check(await files.getByRole('button', { name: 'Open in a pane' }).count() === 0, 'the panel no longer holds the file that went to a pane');
  await code.click();
  await page.keyboard.type('x');
  await page.getByText('Unsaved changes').first().waitFor();
  check(/•$/.test((await page.getByRole('tab', { name: /\.(md|ts|json|rs) •$/ }).last().textContent()) ?? ''), 'the tab of a file with changes is marked');
  await page.getByRole('button', { name: 'Save', exact: true }).last().click();
  await page.getByText('Saved.').first().waitFor();
  // Closing it is the tab's own.
  await fileTab.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Close' }).first().click();
  await code.waitFor({ state: 'detached' });

  // --- the theme: the switch of the title bar and the palette's commands ------------------------------------------------------------------------------
  const schemeNow = () => page.evaluate(() => document.documentElement.dataset['scheme']);
  const waitScheme = (wanted) => page.waitForFunction((value) => document.documentElement.dataset['scheme'] === value, wanted);
  const was = await schemeNow();
  await page.getByRole('button', { name: /^Switch to (light|dark)$/ }).click();
  await waitScheme(was === 'dark' ? 'light' : 'dark');
  check(await page.evaluate(() => localStorage.getItem('shinatama.theme')) === (was === 'dark' ? 'light' : 'dark'), 'the choice of the scheme is kept');
  await command(page, 'Dark theme');
  await waitScheme('dark');
  await command(page, 'Light theme');
  await waitScheme('light');
  await command(page, 'Dark theme');
  await waitScheme('dark');

  // --- the zoom and the glass belong to a window of Shinatama's own -----------------------------------------------------------------------------------
  await goTo(page, 'Settings');
  await page.getByRole('region', { name: 'Appearance' }).waitFor();
  check(await page.getByRole('switch', { name: 'Glass window' }).count() === 0, 'a browser has no glass: Settings does not draw the switch');
  const zoom = page.getByRole('combobox', { name: 'Interface zoom' });
  await zoom.waitFor();
  check(await zoom.isDisabled(), 'a browser cannot change its zoom: the picker is there and says so by being disabled');
  assert.equal(await zoom.textContent().then((text) => text?.trim()), '100%');
  await page.screenshot({ path: `${out}app-settings.png`, animations: 'disabled' });
  await page.close();
} catch (error) {
  failures.push(`the smoke stopped: ${error?.stack ?? error}`);
} finally {
  await browser.close();
  await server.close();
}

if (failures.length) {
  console.error(`app smoke: ${failures.length} failure${failures.length === 1 ? '' : 's'}\n- ${failures.join('\n- ')}`);
  process.exit(1);
}
console.log('app smoke: ok');
