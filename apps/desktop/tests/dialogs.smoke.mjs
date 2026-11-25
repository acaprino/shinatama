// Smoke test of the dialogs: the page on Vite with the mocked commands (src/mock/dialogs.ts), driven in a browser by what a person and a screen
// reader see (roles, names and aria attributes, never a class). A new session (the agent, the project, what the launch review says), the
// projects of the window (the filter, the roots that can be added, made the default and removed with Undo, a folder typed), a new project (its
// name is checked before anything is sent), the overview (pins, a worktree that asks first), and nothing runs off the page at 340px.
//   npm run test:dialogs
import { launch, serve, watch } from './smoke-lib.mjs';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(new URL('.', import.meta.url));
mkdirSync(`${here}out`, { recursive: true });
const { server } = await serve(1475);
const url = server.resolvedUrls.local[0];
const browser = await launch();
const failures = [];
let current = null;
const check = (ok, message) => { if (!ok) failures.push(message); };

async function open(name, { width = 1440, height = 900, scheme = 'dark' } = {}) {
  const page = await browser.newPage({ viewport: { width, height }, colorScheme: scheme });
  current = page;
  watch(page, name, failures);
  await page.goto(url);
  await page.getByRole('button', { name: /Commands/ }).first().waitFor();
  await page.waitForTimeout(800);
  await clearNotices(page);
  return page;
}
/** The notices are put away so that they never sit over what a step presses. */
const clearNotices = async page => {
  const close = page.getByRole('status', { name: 'Notices' }).getByRole('button');
  for (let n = await close.count(), tries = 0; n > 0 && tries < 8; n = await close.count(), tries += 1) { await close.first().click({ timeout: 1500 }).catch(() => {}); await page.waitForTimeout(150); }
};
/** Runs a command of the palette by its name. */
const command = async (page, name) => {
  await clearNotices(page);
  await page.getByRole('button', { name: /Commands/ }).first().click();
  await page.getByRole('combobox', { name: /command/i }).fill(name);
  await page.keyboard.press('Enter');
};
const overflow = page => page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
const dialog = (page, name) => page.getByRole('dialog', { name });

try {
  for (const scheme of ['dark', 'light']) {
    const page = await open(`dialogs ${scheme}`, { scheme });

    // --- a new session: the agent, the project, the review of the launch --------------------------------------------------
    await command(page, 'New session');
    const session = dialog(page, 'New session');
    await session.waitFor();
    check(await session.getByRole('button', { name: 'Claude Code' }).getAttribute('aria-pressed') === 'true', `${scheme}: Claude Code is not the agent first chosen`);
    await session.getByRole('button', { name: 'Codex' }).click();
    check(await session.getByRole('button', { name: 'Codex' }).getAttribute('aria-pressed') === 'true', `${scheme}: the agent chosen is not marked`);
    // A path that is no place is refused where it is typed.
    await session.getByRole('textbox', { name: 'Project folder' }).fill('work\\xiox');
    await session.getByRole('button', { name: 'Open', exact: true }).click();
    await session.getByText('Choose a project from the list, or browse for a folder.').waitFor();
    await session.getByRole('button', { name: /^xiox/ }).click();
    check(await session.getByRole('textbox', { name: 'Project folder' }).inputValue() === 'C:\\work\\xiox', `${scheme}: a project of the list was not taken`);
    // The review is read-only and says what the engine says.
    await session.getByText('Launch review').click();
    await session.getByRole('button', { name: 'Review the launch' }).click();
    await session.getByText('The engine cannot finish this review yet.').waitFor();
    await page.keyboard.press('Escape');
    await session.waitFor({ state: 'detached' });

    // --- the projects of the window ------------------------------------------------------------------------------------
    await command(page, 'Choose the project');
    const picker = dialog(page, 'Projects');
    await picker.waitFor();
    await picker.getByText('3 matching projects').waitFor();
    await picker.getByRole('searchbox', { name: 'Filter projects' }).fill('xi');
    await picker.getByText('1 matching projects').waitFor();
    check(await picker.getByRole('button', { name: /^mukade/ }).count() === 0, `${scheme}: the filter kept a project that does not match`);
    await picker.getByRole('searchbox', { name: 'Filter projects' }).fill('');
    // A root is added, made the default, and removed with Undo.
    await picker.getByRole('textbox', { name: 'Root path' }).fill('D:\\code');
    await picker.getByRole('button', { name: 'Add root' }).click();
    await picker.getByRole('button', { name: 'Make default: D:\\code' }).waitFor();
    await picker.getByRole('button', { name: 'Make default: D:\\code' }).click();
    await picker.getByRole('button', { name: 'Make default: C:\\work' }).waitFor();
    await picker.getByRole('button', { name: 'Remove root D:\\code' }).click();
    await page.getByText('D:\\code is no longer a project root. The folder is not touched.').waitFor();
    await page.getByRole('status', { name: 'Notices' }).getByRole('button', { name: 'Undo' }).click();
    await picker.getByRole('button', { name: 'Remove root D:\\code' }).waitFor();
    // A root that is not a place is refused in words.
    await picker.getByRole('textbox', { name: 'Root path' }).fill('code');
    await picker.getByRole('button', { name: 'Add root' }).click();
    await picker.getByText('Enter the absolute path of the folder that holds your projects.').waitFor();

    // --- a new project: the name is checked before anything is sent ----------------------------------------------------------
    await picker.getByRole('button', { name: 'New project…' }).click();
    const created = dialog(page, 'New project');
    await created.waitFor();
    await created.getByRole('button', { name: 'Create project' }).click();
    await created.getByText('Enter a project folder name.').waitFor();
    await created.getByRole('textbox', { name: 'Project folder name' }).fill('a/b');
    await created.getByText(/without path separators/).waitFor();
    await created.getByRole('textbox', { name: 'Project folder name' }).fill('imago-demo');
    await created.getByText(/Destination: .*imago-demo/).waitFor();
    await created.getByRole('button', { name: 'Create project' }).click();
    await created.waitFor({ state: 'detached' });

    // --- the overview ----------------------------------------------------------------------------------------------------
    await command(page, 'Projects at a glance');
    const overview = dialog(page, 'Projects');
    await overview.getByText('3 projects. Pinned first.').waitFor();
    await overview.getByRole('button', { name: 'Unpin shinatama' }).waitFor();
    await overview.getByRole('button', { name: 'Pin xiox' }).click();
    await overview.getByRole('button', { name: 'Unpin xiox' }).waitFor();
    // A worktree is a Git change: the exact path and branch are asked about first.
    await overview.getByRole('article', { name: 'xiox' }).getByRole('button', { name: 'New in worktree…' }).click();
    await overview.getByRole('textbox', { name: 'Worktree name for xiox' }).fill('Bad Name');
    await overview.getByRole('button', { name: 'Create and open' }).click();
    await overview.getByText('Use 1 to 40 lowercase letters, digits or hyphens.').waitFor();
    await overview.getByRole('textbox', { name: 'Worktree name for xiox' }).fill('task-2');
    await overview.getByRole('button', { name: 'Create and open' }).click();
    await page.getByText(/Git will create C:\\work\\xiox-task-2 on a new branch agent\/task-2/).waitFor();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await page.close();
  }

  // At the narrowest width a dialog fits the page.
  const narrow = await open('dialogs 340', { width: 340, height: 700 });
  await narrow.getByRole('button', { name: 'Workspace' }).first().click();
  await command(narrow, 'New project');
  await dialog(narrow, 'New project').waitFor();
  check(await overflow(narrow) <= 0, `dialogs 340: the page is ${await overflow(narrow)}px too wide`);
  const box = await dialog(narrow, 'New project').boundingBox();
  check(!!box && box.x >= 0 && box.x + box.width <= 340.5, 'dialogs 340: the dialog runs off the page');
  await narrow.close();
} catch (error) {
  if (current) { await current.screenshot({ path: `${here}out/dialogs-failed.png` }).catch(() => {}); }
  failures.push(String(error?.stack ?? error));
} finally {
  await browser.close();
  await server.close();
}
if (failures.length) { console.error(failures.join('\n')); process.exit(1); }
console.log('dialogs smoke: ok');
