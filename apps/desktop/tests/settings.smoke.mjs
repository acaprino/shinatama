// Smoke test of Settings: the page on Vite with the mocked commands (src/mock/settings.ts), driven in a browser by what a person and a screen
// reader see (roles, names and aria attributes, never a class). One stacked page with its index in the sidebar; what applies as it changes
// (the terminal, the switches of Sessions, alerts) and what waits for Save (agents, models, memory, GitHub, variables) are both followed to
// their words; Muro and the model proxy are asked; a stop that cuts agents off asks first; and nothing runs off the page at 340px.
//   npm run test:settings
import { launch, serve, watch } from './smoke-lib.mjs';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(new URL('.', import.meta.url));
mkdirSync(`${here}out`, { recursive: true });
const { server } = await serve(1474);
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
  await page.getByRole('button', { name: 'Settings', exact: true }).first().click();
  await page.getByRole('region', { name: 'Appearance' }).waitFor();
  await page.getByText(/New sessions start with:/).waitFor();
  await clearNotices(page);
  return page;
}
/** The notices are put away so that they never sit over what a step presses. */
const clearNotices = async page => {
  const close = page.getByRole('status', { name: 'Notices' }).getByRole('button');
  for (let n = await close.count(), tries = 0; n > 0 && tries < 8; n = await close.count(), tries += 1) { await close.first().click({ timeout: 1500 }).catch(() => {}); await page.waitForTimeout(150); }
};
const overflow = page => page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
const choose = async (page, name, option) => {
  await clearNotices(page);
  await page.getByRole('combobox', { name, exact: true }).click();
  await page.getByRole('option', { name: option }).click();
};
const goTo = async (page, section) => {
  await page.getByLabel('Sections').getByRole('button', { name: section, exact: true }).click();
  await page.getByRole('region', { name: section, exact: true }).waitFor();
};
/** A switch is turned with the keyboard: its control is drawn over the field that holds the state. */
const toggle = async (page, name) => { await page.getByRole('switch', { name }).focus(); await page.keyboard.press('Space'); };

try {
  for (const scheme of ['dark', 'light']) {
    const page = await open(`settings ${scheme}`, { scheme });

    // The index lists every section, and a press brings one to the head of the page.
    for (const name of ['Appearance', 'Agents', 'Models & API keys', 'Memory', 'Alerts', 'GitHub', 'Environment variables', 'Sessions', 'Plugins', 'Model proxy']) {
      await page.getByLabel('Sections').getByRole('button', { name, exact: true }).waitFor();
    }
    await goTo(page, 'Models & API keys');
    check(await page.getByLabel('Sections').getByRole('button', { name: 'Models & API keys', exact: true }).getAttribute('aria-current') !== null, `${scheme}: the section in use is not marked`);

    // Appearance applies as it changes: a size out of range says why and is put back.
    await goTo(page, 'Appearance');
    const size = page.getByRole('spinbutton', { name: 'Terminal font size' });
    await size.fill('99');
    await page.getByText('Use a whole number from 8 to 32.').waitFor();
    await size.blur();
    check(await size.inputValue() === '13', `${scheme}: a size out of range was kept`);
    await size.fill('15');
    await page.getByText('Use a whole number from 8 to 32.').waitFor({ state: 'detached' });
    await choose(page, 'Terminal font', 'Consolas');

    // Agents: the check says what it found, in words and tones, and nothing is sent.
    await goTo(page, 'Agents');
    await page.getByRole('button', { name: 'Check Claude Code' }).click();
    await page.getByText('Found · 2.1.4').waitFor();
    await page.getByText('Signed in').waitFor();
    await page.getByText('The default empty set needs no optional additions.').waitFor();
    check(await page.getByRole('button', { name: 'Check Codex' }).isDisabled(), `${scheme}: an agent that is not qualified can be checked`);

    // Models: choosing one waits for Save, which says so; Save sends models, launch settings and memory in turn.
    await goTo(page, 'Models & API keys');
    await choose(page, 'OpenCode model', /qwen3-coder-30b/);
    await page.getByText(/Not saved yet/).waitFor();
    await page.getByRole('button', { name: 'Edit Acme gateway' }).click();
    await page.getByRole('button', { name: 'Add model' }).click();
    check(await page.getByRole('textbox', { name: 'Model ID' }).count() === 3, `${scheme}: Add model did not add a row`);
    await page.getByRole('textbox', { name: 'Model ID' }).nth(2).fill('new model!');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await page.getByText(/model IDs must be unique and use letters/).first().waitFor();
    await page.getByRole('textbox', { name: 'Model ID' }).nth(2).fill('local-llama');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await page.getByText(/Saved\. Shared models apply to new sessions/).waitFor();

    // Memory: a margin out of range is said, and Save keeps it from being sent.
    await goTo(page, 'Memory');
    await page.getByRole('spinbutton', { name: 'Memory kept free (MiB)' }).fill('99999');
    await page.getByText(/must be a whole number of MiB between 0 and 16384/).first().waitFor();
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await page.getByText(/must be a whole number of MiB between 0 and 16384/).first().waitFor();
    await page.getByRole('spinbutton', { name: 'Memory kept free (MiB)' }).fill('512');

    // GitHub and the variables: a saved secret never comes back; a new row needs a value.
    await goTo(page, 'GitHub');
    await page.getByRole('button', { name: 'Remove token' }).click();
    await page.getByText('The token will be removed when you save.').first().waitFor();
    await goTo(page, 'Environment variables');
    await page.getByRole('button', { name: 'Add variable' }).click();
    await page.getByRole('textbox', { name: 'Variable name' }).last().fill('REGION');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await page.getByText('REGION needs a value.').first().waitFor();
    await page.getByRole('textbox', { name: 'Value of REGION' }).fill('eu-west-1');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await page.getByText(/Saved\. Shared models apply to new sessions/).waitFor();
    await page.getByText(/4 variables|3 variables/).first().waitFor();
    check(await page.getByRole('button', { name: 'Remove token' }).count() === 0, `${scheme}: the token that was removed is still offered`);

    // Sessions: each switch applies at once, with its words.
    await goTo(page, 'Sessions');
    await clearNotices(page);
    await toggle(page, 'Index session text for search');
    await page.getByText(/Indexing (allowed|off)\./).waitFor();
    await toggle(page, 'Activity titles');
    await page.getByText('Activity titles off.').waitFor();

    // Alerts and Plugins.
    await goTo(page, 'Alerts');
    await toggle(page, 'Play a short sound');
    await page.getByText('Alerts on; sound off.').waitFor();
    await page.getByRole('button', { name: 'Test alert' }).click();
    await page.getByText('Test alert shown.').waitFor();
    await goTo(page, 'Plugins');
    await page.getByRole('button', { name: 'Sync now' }).click();
    await page.getByText(/6 plugins for 4 agents/).first().waitFor();

    // The model proxy: a stop that would cut agents off asks first, with what it costs; Keep it running leaves it alone.
    await goTo(page, 'Model proxy');
    await clearNotices(page);
    await page.getByRole('button', { name: 'Stop', exact: true }).click();
    await page.getByText('Stop the model proxy?').waitFor();
    await page.getByText(/2 agent sessions will lose the connection/).waitFor();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await page.getByText('Ready', { exact: true }).first().waitFor();
    await page.getByRole('button', { name: 'Stop', exact: true }).click();
    await page.getByRole('button', { name: 'Stop model proxy' }).click();
    await page.getByText('Stopped', { exact: true }).first().waitFor();
    await page.getByRole('button', { name: 'Start', exact: true }).click();
    await page.getByText('Ready', { exact: true }).first().waitFor();
    await page.close();
  }

  for (const width of [340, 620, 860, 1000]) {
    const page = await open(`settings ${width}`, { width, height: 800 });
    for (const name of ['Appearance', 'Agents', 'Models & API keys', 'Plugins']) {
      // Under 620px the sidebar is a rail: the sections are reached by scrolling.
      const section = page.getByRole('region', { name, exact: true });
      await section.scrollIntoViewIfNeeded();
      check(await overflow(page) <= 0, `settings ${width}: the page is ${await overflow(page)}px too wide at ${name}`);
    }
    await page.close();
  }
} catch (error) {
  if (current) { await current.screenshot({ path: `${here}out/settings-failed.png` }).catch(() => {}); }
  failures.push(String(error?.stack ?? error));
} finally {
  await browser.close();
  await server.close();
}
if (failures.length) { console.error(failures.join('\n')); process.exit(1); }
console.log('settings smoke: ok');
