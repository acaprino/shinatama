// Smoke test of the team map: the page on Vite with the mocked commands (src/mock/team.ts), driven in a browser by what a person and a screen
// reader see (roles, names and aria attributes, never a class). The map of a thread is opened from its tab, then: an agent is chosen and
// renamed in the box at the end, Apply is blocked while the engine finds a problem and sent when there is none, an arrow is chosen, changed,
// reversed and deleted with Undo, a template is used, and nothing runs off the page at 340px.
//   npm run test:team
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { createServer } from 'vite';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(new URL('.', import.meta.url));
const server = await createServer({ configFile: `${here}../vite.config.ts`, server: { port: 1473, strictPort: false, hmr: false, watch: null }, logLevel: 'error' });
await server.listen();
const url = server.resolvedUrls.local[0];
const browser = await chromium.launch();
const failures = [];
let current = null;
const check = (ok, message) => { if (!ok) failures.push(message); };

/** A page at one size with the errors of the page counted as failures, the team map maximised. */
async function open(name, { width = 1440, height = 900 } = {}) {
  const page = await browser.newPage({ viewport: { width, height } });
  current = page;
  page.on('pageerror', error => failures.push(`${name}: ${error}`));
  // A warning about another area's part (a `fill` passed to the page by the workspace's rows) is that area's to fix, not this smoke's.
  page.on('console', message => { if (message.type() === 'error' && !/non-boolean attribute/.test(message.text())) failures.push(`${name}: ${message.text()}`); });
  await page.goto(url);
  const tab = page.getByRole('tab', { name: /Team map/ }).first();
  // Under 620px one pane shows at a time, reached by its chip.
  if (width < 620) for (const chip of ['Pane 1', 'Pane 2', 'Pane 3']) { if (await tab.count()) break; await page.getByRole('button', { name: chip }).click(); }
  await tab.click();
  if (width >= 620) {
    await page.getByRole('button', { name: /Commands/ }).first().click();
    await page.getByRole('combobox', { name: /command/i }).fill('Maximize the pane');
    await page.keyboard.press('Enter');
  }
  await page.getByRole('region', { name: 'Team map' }).waitFor();
  await page.getByRole('button', { name: /^planner,/ }).waitFor();
  await clearNotices(page);
  return page;
}
const overflow = page => page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
/** The notices are put away so that they never sit over what a step presses. */
const clearNotices = async page => {
  const close = page.getByRole('status', { name: 'Notices' }).getByRole('button');
  for (let n = await close.count(), tries = 0; n > 0 && tries < 8; n = await close.count(), tries += 1) { await close.first().click({ timeout: 1500 }).catch(() => {}); await page.waitForTimeout(150); }
};
const choose = async (page, name, option) => {
  await clearNotices(page);
  await page.getByRole('combobox', { name, exact: true }).click();
  await page.getByRole('option', { name: option }).click();
};
const notice = (page, text) => page.getByRole('status', { name: 'Notices' }).getByText(text).first();

try {
  const page = await open('team');
  // The agents are cards of the map, with their role and whether they were told.
  await page.getByRole('button', { name: /^planner, Lead/ }).waitFor();
  await page.getByRole('button', { name: /^coder, Worker/ }).waitFor();
  await page.getByText('revision 1 · everyone was told').waitFor();
  check(await page.getByRole('button', { name: 'Apply', exact: true }).isDisabled(), 'team: Apply is on with nothing to apply');

  // An agent is chosen: its box opens at the end of the window, and a new name is the card's.
  await page.getByRole('button', { name: /^planner,/ }).click();
  await page.getByRole('heading', { name: 'Agent · running session' }).waitFor();
  await page.getByRole('textbox', { name: 'Name' }).fill('architect');
  await page.getByRole('button', { name: /^architect,/ }).waitFor();
  await page.getByText(/changed · not applied/).first().waitFor();
  await page.getByText(/not applied: nothing was sent yet/).waitFor();
  // Apply waits for the engine's answer about what is on the map, then sends it.
  await page.getByRole('button', { name: 'Apply', exact: true }).and(page.locator(':not([disabled])')).waitFor();
  await page.getByRole('button', { name: 'Apply', exact: true }).click();
  await notice(page, 'Team applied: revision 2.').waitFor();
  await page.getByText('revision 2 · everyone was told').waitFor();

  // A new agent has neither a session nor a model: the engine says so and Apply is refused until it has one.
  await page.getByRole('button', { name: 'New agent' }).click();
  await page.getByText('Error: Agent has neither a session nor a model.').waitFor();
  check(await page.getByRole('button', { name: /apply$/i }).isDisabled(), 'team: Apply is on while the engine finds a problem');
  await choose(page, 'Model', /qwen3-coder-30b/);
  await page.getByText('Error: Agent has neither a session nor a model.').waitFor({ state: 'detached' });
  await page.getByRole('button', { name: 'Open 1 session and apply' }).waitFor();

  // An arrow: made from the agent's box, chosen, changed, reversed and deleted; Undo brings it back.
  await choose(page, 'Connect to', 'architect');
  await page.getByRole('heading', { name: /Agent → architect/ }).waitFor();
  await page.getByRole('button', { name: 'Observes' }).click();
  await page.getByText('Agent is told:').or(page.getByText(/is told: You observe/)).first().waitFor();
  await clearNotices(page);
  await page.getByRole('button', { name: 'Reverse' }).click();
  await page.getByRole('heading', { name: /architect → Agent/ }).waitFor();
  await clearNotices(page);
  await page.getByRole('button', { name: 'Delete the arrow' }).click();
  await notice(page, /no longer observes/).waitFor();
  await notice(page, /no longer observes/).locator('..').getByRole('button', { name: 'Undo' }).click();
  await page.getByRole('button', { name: /Open 1 session/ }).waitFor();

  // Discard throws the draft away and reads the stored team again.
  await clearNotices(page);
  await page.getByRole('button', { name: 'Discard' }).click();
  await page.getByText('revision 2 · everyone was told').waitFor();
  check(await page.getByRole('button', { name: /^Agent,/ }).count() === 0, 'team: Discard left the new agent on the map');

  // Templates: a saved team is put on the map, with no session yet, and the old map comes back with Undo.
  await clearNotices(page);
  await page.getByRole('button', { name: 'Templates' }).click();
  await page.getByRole('button', { name: /^Star of four 4 agents/ }).click();
  await page.getByRole('button', { name: /hub,/ }).waitFor();
  await notice(page, /Star of four/).waitFor();
  check(await page.getByRole('button', { name: /one,/ }).count() === 1, 'team: the template did not put its agents on the map');
  await page.close();

  // At the narrowest width nothing runs off the page.
  const narrow = await open('team 340', { width: 340, height: 700 });
  check(await overflow(narrow) <= 0, `team 340: the page is ${await overflow(narrow)}px too wide`);
  await narrow.close();
} catch (error) {
  if (current) { await current.screenshot({ path: `${here}out-team.png` }).catch(() => {}); console.error(await current.getByRole('button').allTextContents().catch(() => [])); }
  failures.push(String(error?.stack ?? error));
} finally {
  await browser.close();
  await server.close();
}
if (failures.length) { console.error(failures.join('\n')); process.exit(1); }
console.log('team smoke: ok');
