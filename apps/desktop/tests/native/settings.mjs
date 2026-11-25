// Settings on the built exe, Windows only: the page reaches the launch settings Rust keeps (launch-settings.json in the data folder). Full access is
// on when the file is new; a GitHub username, a token and two variables, one of them secret, are saved with Save; the token and the secret are in
// the file as no text, and a page that reads the settings again never gets them back (the token field is empty and says "Saved", the secret value is
// not in the page); the switches that apply at once (activity titles, indexing) reach the backend. Roles and names only.
// How the agents start with full access on or off (Claude's per-instance settings, Codex's argv) is Rust's, proved by the tests of
// crates/engine (claude_launch, codex_launch); the old proof started the real agents and read their command lines.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/settings.mjs
// It stands for legacy launch-settings-smoke, resources-smoke (the resources badge is in workspace-native.mjs) and state-smoke (a restart keeps what
// was saved, below).
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { check, clearNotices, finish, fixtureFolder, goTo, openApp, toggle, until } from './lib.mjs';

const fixture = await fixtureFolder('settings');
const TOKEN = 'ghp_smoketoken0123456789abcdefghijklmnopqrs';
const SECRET = 'smoke-secret-value-0123456789';
let handle;
const open = async () => {
  handle = await openApp(fixture);
  await clearNotices(handle.page);
  await goTo(handle.page, 'Settings');
  await handle.page.getByRole('region', { name: 'GitHub' }).waitFor({ timeout: 30000 });
  return handle.page;
};
try {
  let page = await open();

  check('Full access is on for a new settings file', await page.getByRole('switch', { name: 'Full access' }).isChecked());

  // A GitHub sign-in and two variables, saved by Save.
  const github = page.getByRole('region', { name: 'GitHub' });
  await github.getByRole('textbox', { name: 'Username' }).fill('octocat');
  await github.getByRole('textbox', { name: 'Personal access token' }).fill(TOKEN);
  const variables = page.getByRole('region', { name: 'Environment variables' });
  for (const [name, value, secret] of [['SMOKE_PLAIN', 'visible-value', false], ['SMOKE_SECRET', SECRET, true]]) {
    await variables.getByRole('button', { name: 'Add variable' }).click();
    const row = variables.getByRole('group', { name: /^Variable / }).last();
    await row.getByRole('textbox', { name: 'Variable name' }).fill(name);
    if (secret) await toggle(page, row.getByRole('checkbox', { name: 'Secret' }));
    await row.getByLabel(/^Value of /).fill(value);
  }
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByText(/Saved\./).first().waitFor({ timeout: 30000 });
  const onDisk = await readFile(resolve(fixture.data, 'launch-settings.json'), 'utf8');
  check('the token and the secret are in the file as no text', !onDisk.includes(TOKEN) && !onDisk.includes(SECRET));
  check('and the plain variable is', onDisk.includes('visible-value'));

  // The switches that apply at once reach the backend.
  const titles = page.getByRole('switch', { name: 'Activity titles' });
  const was = await titles.isChecked();
  await toggle(page, titles);
  check('Activity titles applies at once', !!await until(async () => (await handle.ipc('titles_enabled')) === !was, 10000, 'not applied').catch(() => null));
  await toggle(page, titles);

  // A restart keeps what was saved, and the page never gets a secret back.
  await handle.stop();
  page = await open();
  const again = page.getByRole('region', { name: 'GitHub' });
  check('the token field is empty and says a token is saved', (await again.getByRole('textbox', { name: 'Personal access token' }).inputValue()) === '' && await again.getByText(/^Saved/).first().isVisible());
  const html = await page.evaluate(() => document.documentElement.outerHTML);
  check('and the page holds no secret', !html.includes(SECRET) && !html.includes('smoketoken'));
  check('the username was kept', (await again.getByRole('textbox', { name: 'Username' }).inputValue()) === 'octocat');
  check('the page said nothing wrong', handle.errors.length === 0, handle.errors.join(' | '));
} catch (error) {
  check('the probe ran to its end', false, String(error.message ?? error));
} finally {
  await handle?.stop();
  await fixture.remove();
}
finish();
