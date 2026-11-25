// Smoke test of the Sessions view: its page on Vite with the mocked commands (src/mock/sessions.ts), driven by what a person and a screen
// reader see (roles and names, never a class). The table with its Running and Saved groups, the sidebar that narrows it, the filter that
// types, the text search that Enter makes, the older sessions, the facts of a session in the inspector and the way from them to the
// conversation, the menu of a row, the index in the sidebar, both schemes and a narrow window. Screenshots go under tests/out/ (git-ignored).
//   node apps/desktop/tests/sessions.smoke.mjs
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { createServer } from 'vite';

const here = fileURLToPath(new URL('.', import.meta.url));
const out = `${here}out/`;
mkdirSync(out, { recursive: true });
// No watcher: another edit of the sources must not reload the page in the middle of a check.
const server = await createServer({ configFile: `${here}../vite.config.ts`, server: { port: 1437, strictPort: false, watch: null }, logLevel: 'error' });
await server.listen();
const url = server.resolvedUrls.local[0];
const browser = await chromium.launch({ args: ['--disable-lcd-text'] });
const failures = [];

async function open(scheme = 'dark', width = 1280) {
  const page = await browser.newPage({ viewport: { width, height: 860 }, colorScheme: scheme, deviceScaleFactor: 1 });
  await page.addInitScript((value) => { try { localStorage.setItem('shinatama.theme', value); } catch { /* no storage */ } }, scheme);
  page.on('pageerror', (error) => failures.push(`pageerror: ${error}`));
  page.on('console', (message) => {
    // The workspace's terminal tab hands `fill` to an element of the page (a warning of React, not of this area): said once by its owner.
    if ((message.type() === 'error' || message.type() === 'warning') && !/non-boolean attribute/.test(message.text())) failures.push(`console: ${message.text()}`);
  });
  await page.goto(url);
  await page.getByRole('link', { name: 'Sessions' }).or(page.getByRole('tab', { name: 'Sessions' })).or(page.getByRole('button', { name: 'Sessions' })).first().click();
  await page.getByRole('treegrid', { name: 'Sessions' }).waitFor();
  return page;
}
const saved = (page) => page.getByRole('row').filter({ hasNot: page.getByRole('columnheader') });
const text = async (locator) => (await locator.allTextContents()).map((value) => value.trim());

try {
  const page = await open();

  // --- the table: the ones that run, then the ones saved ---------------------------------------------------------------------
  await page.getByRole('row', { name: /Retry backoff for the charge worker/ }).waitFor();
  assert.ok(await page.getByRole('row', { name: /Running\s*2/ }).isVisible(), 'a heading for the running sessions, with their count');
  assert.ok(await page.getByRole('row', { name: /Saved sessions\s*6/ }).isVisible(), 'a heading for the saved ones');
  assert.ok(await page.getByText('Asked for attention').isVisible(), 'a running session says its state in a badge');
  assert.ok(await page.getByText('Bash: git push origin feature/retry-backoff').isVisible(), 'and what it is doing');
  assert.ok(await page.getByText('1 archive could not be read').isVisible(), 'an archive that is unavailable is said, not hidden');
  assert.equal(await page.getByText('6 saved · 2 running').count(), 1);
  assert.ok(await page.getByRole('button', { name: 'Load older sessions' }).isVisible(), 'an archive with more pages offers them');
  await page.screenshot({ path: `${out}sessions-table.png` });

  // --- the sidebar narrows the list ------------------------------------------------------------------------------------------
  const sidebar = page.getByRole('navigation', { name: 'Stores' });
  await sidebar.getByRole('button', { name: /^Codex/ }).or(sidebar.getByText('Codex', { exact: true })).first().click();
  await page.waitForTimeout(150);
  assert.equal(await page.getByRole('row', { name: /Retry backoff/ }).count(), 0, 'a Claude Code session is not listed under Codex');
  assert.ok(await page.getByRole('row', { name: /Idempotency keys on \/v1\/charges/ }).isVisible());
  assert.ok(await page.getByRole('row', { name: /Running\s*1/ }).isVisible(), 'the running sessions are narrowed too');
  await sidebar.getByText('All agents').click();
  await page.getByRole('row', { name: /Retry backoff/ }).waitFor();
  await sidebar.getByText('Codex · Shinatama').click();
  await page.waitForTimeout(150);
  assert.equal(await page.getByRole('row', { name: /Retry backoff/ }).count(), 0, 'one archive');
  await sidebar.getByText('All archives').click();
  await sidebar.getByText('This project').click();
  await page.getByRole('row', { name: /Retry backoff/ }).waitFor();
  assert.equal(await page.getByRole('row', { name: /Map the writers/ }).count(), 0, 'a session of another project is not in this one');
  await sidebar.getByText('All projects').click();

  // --- load older ------------------------------------------------------------------------------------------------------------
  await page.getByRole('button', { name: 'Load older sessions' }).click();
  await page.getByRole('row', { name: /Dunning email copy/ }).waitFor();
  await page.getByRole('row', { name: /Untitled session/ }).waitFor();
  assert.ok(await page.getByRole('row', { name: /Saved sessions\s*9/ }).isVisible(), 'the older ones join the list');
  assert.equal(await page.getByRole('button', { name: 'Load older sessions' }).count(), 0, 'nothing more to load');

  // --- the filter types, Enter searches the text -----------------------------------------------------------------------------
  const field = page.getByRole('searchbox', { name: /Filter sessions/ });
  await field.fill('idempotency');
  await page.waitForTimeout(150);
  assert.equal(await page.getByRole('row', { name: /Retry backoff/ }).count(), 0, 'typing filters');
  assert.ok(await page.getByRole('row', { name: /Idempotency keys/ }).isVisible());
  await field.press('Enter');
  await page.getByRole('list', { name: 'Matches in conversations' }).waitFor();
  assert.ok(await page.getByText(/Matches in conversations · 2/).isVisible(), 'the matches replace the table');
  assert.equal(await page.locator('mark').first().textContent(), 'idempotency', 'the match is marked, from the snippet');
  assert.ok(await page.getByText('deleted', { exact: true }).isVisible(), 'a session whose source is deleted says so');
  await page.screenshot({ path: `${out}sessions-search.png` });
  await page.getByRole('button', { name: /Idempotency keys on \/v1\/charges/ }).click();
  await page.getByRole('heading', { name: 'Session detail' }).waitFor();
  await field.press('Escape');
  await page.getByRole('treegrid', { name: 'Sessions' }).waitFor();
  assert.equal(await field.inputValue(), '', 'Escape leaves the search');
  await page.getByRole('button', { name: 'Close the inspector' }).or(page.getByRole('button', { name: /^Close/ })).first().click().catch(() => {});

  // --- the facts of a session, and the way to its conversation -----------------------------------------------------------------
  await page.getByRole('row', { name: /Postgres 16 migration plan/ }).click();
  await page.getByRole('heading', { name: 'Session', exact: true }).waitFor();
  const facts = page.getByRole('term').or(page.locator('dt'));
  assert.ok((await text(facts)).includes('Folder'), 'the facts of the session');
  assert.ok(await page.getByRole('button', { name: 'Open conversation' }).isEnabled());
  await page.getByRole('button', { name: 'Open conversation' }).click();
  await page.getByRole('heading', { name: 'Session detail' }).waitFor();
  assert.ok(await page.getByText('Carry the retry policy to the webhook handler too.').isVisible(), 'the first page of its conversation');
  await page.getByRole('button', { name: 'Back to Session' }).click();
  await page.getByRole('heading', { name: 'Session', exact: true }).waitFor();

  // --- the menu of a row ---------------------------------------------------------------------------------------------------------
  await page.getByRole('row', { name: /Map the writers/ }).click({ button: 'right' });
  const menu = page.getByRole('menu', { name: 'Actions for the session' });
  await menu.waitFor();
  assert.deepEqual(await text(menu.getByRole('menuitem')), ['Show its facts', 'Open conversation', 'Hop to another agent…', 'Copy the session id']);
  await menu.getByRole('menuitem', { name: 'Hop to another agent…' }).click();
  await page.getByRole('heading', { name: 'Hop to another agent' }).waitFor();
  await page.getByRole('button', { name: 'Prepare preview' }).click();
  await page.getByRole('button', { name: /^Hop to / }).waitFor();
  assert.ok(await page.getByText('This is exactly what the destination receives.').isVisible(), 'the preview comes before the hop');

  // --- a running session: its pane is the workspace's -------------------------------------------------------------------------------
  await page.getByRole('row', { name: /Codex bbbb/ }).click();
  await page.getByText('This instance has no pane in this window.').waitFor();

  // --- the index in the sidebar -------------------------------------------------------------------------------------------------------
  assert.ok(await sidebar.getByText(/1,284 sessions indexed/).isVisible());
  await sidebar.getByRole('button', { name: 'Forget' }).click();
  const dialog = page.getByRole('alertdialog').or(page.getByRole('dialog'));
  await dialog.getByText('Forget the indexed session text?').waitFor();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  assert.ok(await sidebar.getByText(/1,284 sessions indexed/).isVisible(), 'a no forgets nothing');
  await sidebar.getByRole('button', { name: 'Index sessions' }).click();
  await sidebar.getByText('Indexing…').waitFor();
  await page.screenshot({ path: `${out}sessions-facts.png` });

  // --- the inspector closes with the view ------------------------------------------------------------------------------------------------
  await page.getByRole('link', { name: 'Workspace' }).or(page.getByRole('tab', { name: 'Workspace' })).or(page.getByRole('button', { name: 'Workspace' })).first().click();
  assert.equal(await page.getByRole('heading', { name: 'Hop to another agent' }).count(), 0, 'another view starts without the panels of this one');
  await page.close();

  // --- light, and a narrow window ------------------------------------------------------------------------------------------------------------
  const light = await open('light');
  await light.getByRole('row', { name: /Retry backoff/ }).click();
  await light.getByRole('button', { name: 'Open conversation' }).waitFor();
  await light.screenshot({ path: `${out}sessions-light.png` });
  await light.close();
  const narrow = await open('dark', 420);
  await narrow.getByRole('treegrid', { name: 'Sessions' }).waitFor();
  const overflow = await narrow.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  assert.ok(overflow <= 0, `nothing overflows sideways at 420px (${overflow}px over)`);
  await narrow.screenshot({ path: `${out}sessions-narrow.png` });
  await narrow.close();
} catch (error) {
  failures.push(String(error?.stack ?? error));
} finally {
  await browser.close();
  await server.close();
}

if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
console.log('sessions smoke: ok');
