// Smoke test of the browser tab: its screen on Vite with the mocked pages (src/mock/browser.ts), in a harness page that mounts it as the
// workspace's tab renderer does (tests/harness/browser.html). Driven by roles and names, never a class: the bar (address, back, forward,
// reload, the system browser), an agent's page and the strip that offers to take it over, a tab whose page is closed, the tab sent to Rust
// as in sight, and both schemes. The frames are not drawn here: Sigrid's FrameSurface is pending (src/screens/browser/surface.tsx).
// Screenshots go under tests/out/ (git-ignored).
//   node apps/desktop/tests/browser.smoke.mjs
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { launch, serve, watch } from './smoke-lib.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const out = `${here}out/`;
mkdirSync(out, { recursive: true });
const { server } = await serve(1434);
const base = `${server.resolvedUrls.local[0]}tests/harness/browser.html`;
const browser = await launch({ args: ['--disable-lcd-text'] });
const failures = [];

async function open(scheme = 'dark', query = '', width = 900) {
  const page = await browser.newPage({ viewport: { width, height: 600 }, colorScheme: scheme, deviceScaleFactor: 1 });
  await page.addInitScript((value) => { try { localStorage.setItem('shinatama.theme', value); } catch { /* no storage */ } }, scheme);
  watch(page, 'browser', failures);
  await page.goto(base + query);
  await page.waitForFunction(() => window.__app !== undefined);
  return page;
}
const app = (page, run, ...args) => page.evaluate(async ([source, args]) => {
  const { state, openAgentPage } = await import('/src/mock/browser.ts');
  const { emit } = await import('/src/mock/index.ts');
  return new Function('app', 'state', 'openAgentPage', 'emit', 'args', `return (async () => { ${source} })()`)(window.__app, state, openAgentPage, emit, args);
}, [run, args]);

try {
  const page = await open();
  await app(page, "await app.browser.open({ url: 'https://example.com/start' });");
  const bar = page.getByRole('group', { name: 'Browser navigation' });
  const address = bar.getByRole('textbox', { name: 'Address' });
  await address.waitFor();
  assert.equal(await address.inputValue(), 'https://example.com/start');
  assert.equal(await page.getByText('Questa pagina è chiusa.').count(), 0);
  await page.screenshot({ path: `${out}browser-open.png`, animations: 'disabled' });

  // The tab is told it is in sight.
  assert.deepEqual(await app(page, 'return state.visibility;'), [true]);

  // An address with no scheme is https; the field shows the page's own address again after the step.
  await address.fill('example.org/docs');
  await address.press('Enter');
  await page.waitForFunction(() => document.querySelector('input[name="address"]')?.value === 'https://example.org/docs');
  await bar.getByRole('button', { name: 'Back' }).click();
  await page.waitForFunction(() => document.querySelector('input[name="address"]')?.value === 'https://example.com/start');
  await bar.getByRole('button', { name: 'Forward' }).click();
  await page.waitForFunction(() => document.querySelector('input[name="address"]')?.value === 'https://example.org/docs');
  await bar.getByRole('button', { name: 'Reload' }).click();
  // What was typed and not confirmed is dropped when the field is left.
  await address.fill('half typed');
  await address.blur();
  assert.equal(await address.inputValue(), 'https://example.org/docs');
  await bar.getByRole('button', { name: 'Open in the system browser' }).click();
  await page.waitForFunction(() => window.__app.browser.pages.size === 1);
  assert.deepEqual(await app(page, 'return state.external;'), ['https://example.org/docs']);

  // A page an agent opened becomes a tab the person can take over, and give back.
  await app(page, "emit('browser-agent-page', { page: openAgentPage('https://agent.example/page'), instance: 'i_aaaaaaaaaaaa' });");
  await page.waitForFunction(() => document.querySelector('input[name="address"]')?.value === 'https://agent.example/page');
  await page.getByText("L'agente sta usando questa pagina").waitFor();
  await page.screenshot({ path: `${out}browser-agent.png`, animations: 'disabled' });
  await page.getByRole('button', { name: 'Prendi il controllo' }).click();
  await page.getByText("Hai il controllo della pagina dell'agente").waitFor();
  await page.getByRole('button', { name: 'Restituisci' }).click();
  await page.getByText("L'agente sta usando questa pagina").waitFor();
  // The agent closes its page: the tab goes.
  await app(page, "const id = [...app.browser.pages.values()].at(-1).id; emit('browser-page-closed', { page: id });");
  await page.waitForFunction(() => window.__app.browser.pages.size === 1);
  await page.close();

  // A tab the layout kept whose page is not open: its last address, and a button that opens it again.
  const again = await browser.newPage({ viewport: { width: 900, height: 600 }, colorScheme: 'dark' });
  watch(again, 'browser (closed page)', failures);
  await again.addInitScript(() => { try { localStorage.setItem('shinatama.browser.tabs', JSON.stringify({ 'browser-gone': { url: 'https://gone.example/a', title: 'Gone page' } })); } catch { /* no storage */ } });
  await again.goto(`${base}?closed=browser-gone`);
  await again.getByText('Gone page', { exact: true }).waitFor();
  await again.getByText('https://gone.example/a').waitFor();
  await again.screenshot({ path: `${out}browser-closed.png`, animations: 'disabled' });
  await again.getByRole('button', { name: 'Riapri' }).click();
  await again.waitForFunction(() => window.__app.browser.pages.size === 1);
  await again.close();

  // The light scheme, and the bar at the narrowest width.
  const light = await open('light', '', 340);
  await app(light, "await app.browser.open({ url: 'https://example.com/a/very/long/address/that/does/not/fit/in/the/bar' });");
  await light.getByRole('textbox', { name: 'Address' }).waitFor();
  const wide = await light.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
  assert.equal(wide, false, 'nothing runs off the page at 340px');
  await light.screenshot({ path: `${out}browser-light-340.png`, animations: 'disabled' });
  await light.close();
} catch (error) {
  failures.push(String(error?.stack ?? error));
}

await browser.close();
await server.close();
if (failures.length) {
  console.error(`browser smoke: ${failures.length} failure(s)\n${failures.join('\n')}`);
  process.exit(1);
}
console.log('browser smoke: ok');
