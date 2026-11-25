// Smoke test of the Workspace view: the whole window on Vite with the mocked commands (src/mock/{workspace,terminals,threads}.ts), driven in a
// browser by what a person and a screen reader see: roles, names and aria attributes, never a class. The mock leaves a populated workspace
// (three panes, four terminals with output, a thread that talks), so this follows the flows end to end: the panes and their tabs, what a
// terminal prints and takes, the window's own chords from inside it, the history of what scrolled off, the search, the thread sidebar, a session
// that does not run, the conversation, one pane at a time in a narrow window, both schemes. Screenshots go under tests/out/ (git-ignored).
//   node apps/desktop/tests/workspace.smoke.mjs
// What a real window alone proves (the native drop of files, Snap Layouts, the first frame) is in docs/verification.
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { createServer } from 'vite';

const here = fileURLToPath(new URL('.', import.meta.url));
const out = `${here}out/`;
mkdirSync(out, { recursive: true });
const server = await createServer({ configFile: `${here}../vite.config.ts`, server: { port: 1452, strictPort: false }, logLevel: 'error' });
await server.listen();
const url = server.resolvedUrls.local[0];
const browser = await chromium.launch({ args: ['--disable-lcd-text'] });
const failures = [];
const pages = new Set();
const check = (ok, message) => { if (!ok) failures.push(message); };

/** The widths where the frame changes (340 is the smallest; 620, 860 and 1000 are its steps) and a wide one. */
const SIZES = [[340, 700], [620, 700], [860, 700], [1000, 700], [1440, 900]];

/** What sticks out of the page or of a region of the frame, sideways. */
const overflow = (page) => page.evaluate(() => {
  const found = [];
  const wide = document.documentElement;
  if (wide.scrollWidth > wide.clientWidth) found.push(`the page is ${wide.scrollWidth} wide in ${wide.clientWidth}`);
  for (const slot of ['title-bar', 'app-shell-status', 'sidebar', 'app-shell-content', 'app-shell-footer', 'workspace']) {
    for (const region of document.querySelectorAll(`[data-slot="${slot}"]`)) {
      if (region.clientWidth > 1 && region.scrollWidth > region.clientWidth) found.push(`${slot} is ${region.scrollWidth} wide in ${region.clientWidth}`);
    }
  }
  return found;
});

/** A page of its own, with the scheme kept and the errors of the page counted as failures. */
async function open(name, { width = 1440, height = 900, scheme = 'dark' } = {}) {
  const page = await browser.newPage({ viewport: { width, height }, colorScheme: scheme, deviceScaleFactor: 1 });
  await page.addInitScript((value) => { try { localStorage.setItem('shinatama.theme', value); } catch { /* no storage */ } }, scheme);
  page.on('pageerror', (error) => failures.push(`${name}: ${error}`));
  page.on('console', (message) => {
    if (message.type() !== 'error') return;
    // pending Sigrid terminal-fill: a pinned Cluster that does not know `fill` hands it to the page.
    if (/non-boolean attribute/.test(message.text()) && /\bfill\b/.test(message.text())) return;
    failures.push(`${name}: ${message.text()}`);
  });
  pages.add(page);
  page.on('close', () => pages.delete(page));
  await page.goto(url);
  // The saved layout is read and the terminals are reattached a moment after the first frame (the sidebar is a drawer in a narrow window).
  await page.getByRole('region', { name: 'Pane 1' }).waitFor({ state: 'attached' });
  // A narrow window shows one pane at a time: the others are not drawn.
  if (width >= 620) await page.getByRole('region', { name: 'Pane 3' }).waitFor({ state: 'attached' });
  return page;
}

const tab = (page, name) => page.getByRole('tab', { name });
const thread = (page, name) => page.getByRole('navigation', { name: 'Threads' }).getByRole('button', { name: new RegExp(`^${name}`) });
const terminalText = (page, name) => page.getByRole('tabpanel', { name }).first().textContent();
const notice = (page, text) => page.getByRole('status', { name: 'Notices' }).getByText(text).first();
const palette = async (page, command) => {
  await page.keyboard.press('Control+k');
  await page.getByRole('combobox', { name: /command/i }).fill(command);
  await page.keyboard.press('Enter');
  await page.getByRole('combobox', { name: /command/i }).waitFor({ state: 'detached' });
};

try {
  // --- the frame at every width, in both schemes ------------------------------------------------------------------------
  for (const scheme of ['dark', 'light']) {
    for (const [width, height] of SIZES) {
      const page = await open(`${scheme} ${width}`, { width, height, scheme });
      await page.waitForTimeout(400);
      for (const problem of await overflow(page)) failures.push(`${scheme} ${width}: ${problem}`);
      if (width === 1440 || width === 340) await page.screenshot({ path: `${out}workspace-${scheme}-${width}.png`, animations: 'disabled' });
      await page.close();
    }
  }

  // --- the panes, their tabs and what the terminals print -----------------------------------------------------------------
  const page = await open('flows');
  for (const pane of ['Pane 1', 'Pane 2', 'Pane 3']) check(await page.getByRole('region', { name: pane }).count() === 1, `${pane} is there`);
  const names = await page.getByRole('tab').allTextContents();
  check(names.some(name => /^Claude Code · Migrate to Imago/.test(name)), `a tab names the session and its thread: ${names.join(' | ')}`);
  check(names.some(name => /OpenCode · Migrate to Imago · not running/.test(name)), 'a session that does not run says so on its tab');
  check(names.some(name => /^Conversation · Migrate to Imago/.test(name)) && names.some(name => /^Team map · Migrate to Imago/.test(name)), 'the conversation and the team map are tabs of the pane');
  await page.getByText('4 terminals · 3 panes').waitFor();
  check((await terminalText(page, /^Claude Code · Migrate to Imago/)).includes('Update(src/controllers/layout.ts)'), 'the terminal of the agent prints what the agent printed');
  await page.getByRole('textbox', { name: 'Terminal of Claude Code' }).waitFor({ state: 'attached' });
  await page.getByRole('group', { name: 'Terminal details' }).first().getByText('Running', { exact: true }).waitFor();
  await page.getByText('Resources: available').waitFor();
  await page.screenshot({ path: `${out}workspace-panes.png`, animations: 'disabled' });

  // --- a terminal takes what is typed, and the window's own chords reach the window from inside it ---------------------------
  await tab(page, /^PowerShell · Migrate to Imago/).click();
  const shell = page.getByRole('textbox', { name: 'Terminal of PowerShell' });
  await shell.focus();
  await page.keyboard.type('echo hello');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.body.textContent.includes('echo hello'));
  // Ctrl+K is the program's inside a terminal: the palette does not open. Ctrl+Shift+T is the window's: a new session is asked for.
  await page.keyboard.press('Control+k');
  check(await page.getByRole('combobox', { name: /command/i }).count() === 0, 'Ctrl+K inside a terminal is the program\'s');
  await page.keyboard.press('Control+Shift+T');
  await page.getByRole('dialog').waitFor();
  await page.screenshot({ path: `${out}workspace-new-session.png`, animations: 'disabled' });
  await page.keyboard.press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'detached' });

  // --- what scrolled off the live buffer is kept, and read in the history -----------------------------------------------------
  await shell.focus();
  await page.keyboard.type('flood');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.body.textContent.includes('output line 2500'), null, { timeout: 15000 });
  const strip = page.getByRole('scrollbar', { name: 'Lines of the terminal' }).first();
  await strip.focus();
  await page.keyboard.press('Home');
  const earlier = page.getByRole('button', { name: /^Earlier lines · / });
  await earlier.waitFor();
  await earlier.click();
  const history = page.getByRole('dialog', { name: 'Earlier terminal lines' });
  await history.waitFor();
  check(/Earlier lines 1[–-]/.test(await history.textContent()), 'the history says which lines it shows');
  check(await history.getByRole('button', { name: 'Later' }).isDisabled() === false || await history.getByRole('button', { name: 'Earlier' }).isDisabled(), 'the history can be moved');
  await page.screenshot({ path: `${out}workspace-history.png`, animations: 'disabled' });
  await history.getByRole('button', { name: 'Back to live' }).click();
  await history.waitFor({ state: 'detached' });

  // --- the Workspace stays mounted behind the other views: a terminal keeps what it printed ---------------------------------------
  await page.getByRole('button', { name: 'Sessions', exact: true }).first().click();
  await page.getByRole('region', { name: 'Pane 1' }).waitFor({ state: 'hidden' });
  await page.getByRole('button', { name: 'Workspace', exact: true }).first().click();
  await page.getByRole('region', { name: 'Pane 1' }).waitFor({ state: 'visible' });
  // It is where it was (scrolled to its top, with the archive of what fell out of its live buffer), and its end is still there.
  await page.getByRole('button', { name: /^Earlier lines · / }).waitFor();
  await page.getByRole('scrollbar', { name: 'Lines of the terminal' }).first().focus();
  await page.keyboard.press('End');
  await page.waitForFunction(() => document.body.textContent.includes('output line 2500'));

  // --- the search of a terminal ---------------------------------------------------------------------------------------------
  await tab(page, /^Claude Code · Migrate to Imago/).click();
  await tab(page, /^Claude Code · Migrate to Imago/).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Find in terminal' }).click();
  const find = page.getByRole('dialog', { name: 'Find in terminal' });
  await find.waitFor();
  await find.getByRole('searchbox', { name: 'Text in this terminal' }).fill('Bash(');
  await find.getByText('1 of 2 matching lines').waitFor();
  await page.keyboard.press('Enter');
  await find.getByText('2 of 2 matching lines').waitFor();
  await find.getByRole('searchbox', { name: 'Text in this terminal' }).fill('no such text anywhere');
  await find.getByText('No matching lines in the retained buffer.').waitFor();
  await page.keyboard.press('Escape');
  await find.waitFor({ state: 'detached' });

  // --- the conversation of the thread ------------------------------------------------------------------------------------------
  await tab(page, /^Conversation · Migrate to Imago/).click();
  const log = page.getByRole('log', { name: 'Conversation of the thread' });
  await log.getByText('Reviewed it. Also reject payloads older than 5 minutes').waitFor();
  await page.getByRole('textbox', { name: 'Message' }).fill('@codex-1 add the age check');
  await page.getByRole('button', { name: 'Send' }).click();
  await log.getByText('@codex-1 add the age check').waitFor();
  await page.getByText('Sent to claude-1.').waitFor();
  await page.getByRole('combobox', { name: 'Show' }).click();
  await page.getByRole('option', { name: 'Messages only' }).click();
  check(await log.getByText('joined the room').count() === 0, 'the filter keeps the messages alone');
  await page.screenshot({ path: `${out}workspace-chat.png`, animations: 'disabled' });

  // --- the thread sidebar ----------------------------------------------------------------------------------------------------------
  const migrate = thread(page, 'Migrate to Imago');
  check(await migrate.getAttribute('aria-pressed') === 'true', 'a thread that is on is pressed');
  check(await thread(page, 'Terminal history').getAttribute('aria-pressed') === 'false', 'a thread that is off is not');
  // Off asks first, because sessions run; cancelled, it stays on.
  await migrate.click();
  const turnOff = page.getByRole('dialog', { name: 'Turn the thread off?' });
  await turnOff.getByText('3 running sessions').waitFor();
  await turnOff.getByRole('button', { name: 'Cancel' }).click();
  check(await migrate.getAttribute('aria-pressed') === 'true', 'a cancelled question leaves the thread on');
  // On shows the tabs of its sessions, not running; Resume brings one back.
  await thread(page, 'Terminal history').click();
  await tab(page, /^Claude Code · Terminal history · not running/).waitFor();
  await tab(page, /^Claude Code · Terminal history · not running/).click();
  await page.getByRole('button', { name: 'Resume' }).click();
  await tab(page, /^Claude Code · Terminal history$/).waitFor({ timeout: 10000 });
  // A thread's menu: archive takes it out of the list; the archived are listed on request.
  await thread(page, 'Release notes').click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Archive' }).click();
  await page.getByRole('navigation', { name: 'Threads' }).getByRole('button', { name: /^Release notes/ }).waitFor({ state: 'detached' });
  await page.getByRole('checkbox', { name: 'Show archived threads' }).check();
  await thread(page, 'Release notes').waitFor();
  await thread(page, 'Old spike').waitFor();
  check((await thread(page, 'Old spike').textContent()).includes('Archived'), 'an archived thread says so');
  await page.getByRole('checkbox', { name: 'Show archived threads' }).uncheck();
  // A new thread is on at once.
  await page.getByRole('button', { name: 'New thread in shinatama' }).click();
  await page.getByRole('navigation', { name: 'Threads' }).getByRole('button', { name: /^New thread/, pressed: true }).first().waitFor();
  // The + of a thread asks for a session in it.
  await page.getByRole('button', { name: 'Add a session to Migrate to Imago' }).click();
  await page.getByRole('dialog').waitFor();
  await page.keyboard.press('Escape');
  await page.screenshot({ path: `${out}workspace-sidebar.png`, animations: 'disabled' });

  // --- a pane fills the area, and gives it back -------------------------------------------------------------------------------------
  await page.getByRole('region', { name: 'Pane 2' }).getByRole('button', { name: 'Fill the workspace with this pane' }).click();
  const filled = page.getByRole('region', { name: 'Pane 2' }).getByRole('button', { pressed: true });
  await filled.waitFor();
  check(await page.getByRole('region', { name: 'Pane 1' }).isVisible() === false, 'the other panes give way to the one that fills the area');
  await filled.click();
  await page.getByRole('region', { name: 'Pane 1' }).waitFor({ state: 'visible' });
  await page.close();

  // --- a narrow window shows one pane at a time -------------------------------------------------------------------------------------
  const narrow = await open('narrow', { width: 500, height: 800 });
  const panes = narrow.getByRole('group', { name: 'Panes' });
  await panes.getByRole('button', { name: 'Pane 3' }).click();
  await narrow.getByRole('tabpanel', { name: /^Conversation · Migrate to Imago/ }).waitFor({ state: 'visible' });
  await panes.getByRole('button', { name: 'Pane 1' }).click();
  await narrow.getByRole('tabpanel', { name: /^Claude Code · Migrate to Imago/ }).waitFor({ state: 'visible' });
  for (const problem of await overflow(narrow)) failures.push(`narrow: ${problem}`);
  await narrow.screenshot({ path: `${out}workspace-narrow.png`, animations: 'disabled' });
  await narrow.close();
} catch (error) {
  failures.push(`a flow stopped: ${error?.stack ?? error}`);
  for (const [index, page] of [...pages].entries()) { try { await page.screenshot({ path: `${out}workspace-stopped-${index}.png` }); } catch { /* the page is gone */ } }
} finally {
  await browser.close();
  await server.close();
}

if (failures.length) {
  console.error(`workspace smoke: ${failures.length} failure${failures.length === 1 ? '' : 's'}\n- ${failures.join('\n- ')}`);
  process.exit(1);
}
console.log('workspace smoke: ok');
