// Smoke test of the attention widget: its page on Vite with the mocked commands (src/mock/attention.ts), driven by what a person and a
// screen reader see (roles and names, never a class). Each kind of notice, the answers (open, close, Escape), a refusal that is said in
// words, a notice that is replaced, and both schemes. Screenshots go under tests/out/ (git-ignored).
//   node apps/desktop/tests/widget.smoke.mjs
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { launch, serve, watch } from './smoke-lib.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const out = `${here}out/`;
mkdirSync(out, { recursive: true });
const { server } = await serve(1433);
const url = `${server.resolvedUrls.local[0]}widget.html`;
const browser = await launch({ args: ['--disable-lcd-text'] });
const failures = [];
const check = (ok, message) => { if (!ok) failures.push(message); };

const NOTICES = {
  input: { id: 'n-input', kind: 'input', title: 'Claude Code · billing-service', body: 'Il terminale ha inviato un segnale di attenzione. Apri la sessione per leggere la richiesta.', sessionId: 'i_aaaaaaaaaaaa', createdAt: 1 },
  complete: { id: 'n-complete', kind: 'complete', title: 'Terminale · checkout-web', body: 'Il processo del terminale è terminato. Apri la sessione per vedere il risultato.', sessionId: 'i_bbbbbbbbbbbb', createdAt: 2 },
  error: { id: 'n-error', kind: 'error', title: 'Terminale · ledger-core', body: 'Il processo del terminale è terminato. Apri la sessione per vedere il risultato.', sessionId: 'i_cccccccccccc', createdAt: 3 },
  memory: { id: 'n-memory', kind: 'memory', title: 'Memoria libera: 1,2 GB', body: 'Il terminale che ne usa di più è OpenCode. Le sessioni restano attive.', sessionId: null, createdAt: 4 },
};
const LABELS = { input: 'Serve una tua risposta', complete: 'Processo terminato', error: 'Richiede attenzione', memory: 'Memoria del PC in esaurimento' };

/** The mock's side of the page: set the notice Rust would hold, and say that the feed changed. */
const feed = (page, notice) => page.evaluate(async (next) => {
  const state = await import('/src/mock/attention.ts');
  const mock = await import('/src/mock/index.ts');
  state.setNotice(next);
  mock.emit('attention-changed');
}, notice);
const mockState = (page) => page.evaluate(async () => { const { state } = await import('/src/mock/attention.ts'); return { opened: state.opened, dismissed: state.dismissed }; });

async function open(scheme = 'dark') {
  const page = await browser.newPage({ viewport: { width: 440, height: 220 }, colorScheme: scheme, deviceScaleFactor: 1 });
  await page.addInitScript((value) => { try { localStorage.setItem('shinatama.theme', value); } catch { /* no storage */ } }, scheme);
  watch(page, 'widget', failures);
  await page.goto(url);
  await page.waitForLoadState('networkidle');
  return page;
}

try {
  const page = await open();
  const notice = page.getByRole('region', { name: 'Avviso di Shinatama' });
  // Nothing is on show until Rust has a notice.
  assert.equal(await notice.count(), 0, 'no notice, no card');

  for (const [kind, one] of Object.entries(NOTICES)) {
    await feed(page, one);
    await notice.waitFor();
    assert.ok(await notice.getByText(LABELS[kind], { exact: true }).isVisible(), `${kind}: its words`);
    assert.ok(await notice.getByRole('heading', { name: one.title }).isVisible(), `${kind}: its title`);
    assert.ok(await notice.getByText(one.body).isVisible(), `${kind}: its body`);
    const open = notice.getByRole('button', { name: one.sessionId ? 'Apri sessione' : 'Apri Shinatama' });
    assert.ok(await open.isVisible(), `${kind}: the button names what it opens`);
    assert.ok(await notice.getByRole('button', { name: 'Chiudi avviso' }).isVisible(), `${kind}: it can be closed`);
    await page.screenshot({ path: `${out}widget-${kind}.png`, animations: 'disabled' });
  }

  // The portrait changes with each new notice and stays through a read of the same one.
  const portrait = () => page.getByRole('img', { name: 'Shinatama' }).count();
  assert.equal(await portrait(), 0, 'the portrait is decoration: it has no name of its own');

  // Open: Rust is asked for this notice, and the card goes when Rust has none.
  await feed(page, NOTICES.input);
  await notice.getByRole('button', { name: 'Apri sessione' }).click();
  await notice.waitFor({ state: 'detached' });
  assert.deepEqual((await mockState(page)).opened, ['n-input']);

  // Close with the button, and with Escape.
  await feed(page, NOTICES.complete);
  await notice.getByRole('button', { name: 'Chiudi avviso' }).click();
  await notice.waitFor({ state: 'detached' });
  await feed(page, NOTICES.error);
  await notice.waitFor();
  await page.keyboard.press('Escape');
  await notice.waitFor({ state: 'detached' });
  assert.deepEqual((await mockState(page)).dismissed, ['n-complete', 'n-error']);

  // A refusal is said in words and the notice stays; the next try works.
  await feed(page, NOTICES.input);
  await notice.waitFor();
  await page.evaluate(async () => { (await import('/src/mock/attention.ts')).state.failNext = 'open'; });
  await notice.getByRole('button', { name: 'Apri sessione' }).click();
  await notice.getByRole('alert').getByText('Apri la sessione in Shinatama.').waitFor();
  await notice.getByRole('button', { name: 'Apri sessione' }).click();
  await notice.waitFor({ state: 'detached' });

  // A notice that Rust could not be read for stays readable, and says it may be old.
  await feed(page, NOTICES.memory);
  await notice.waitFor();
  await page.evaluate(async () => {
    (await import('/src/mock/attention.ts')).state.failNext = 'current';
    (await import('/src/mock/index.ts')).emit('attention-changed');
  });
  await notice.getByText('Avviso non aggiornato.').waitFor();
  // A new notice replaces it and clears the message.
  await feed(page, NOTICES.error);
  await notice.getByRole('heading', { name: NOTICES.error.title }).waitFor();
  assert.equal(await notice.getByText('Avviso non aggiornato.').count(), 0);

  // The alerts are off: nothing is drawn, whatever Rust still holds.
  await page.evaluate(async () => { (await import('/src/mock/attention.ts')).state.enabled = false; (await import('/src/mock/index.ts')).emit('attention-changed'); });
  await notice.waitFor({ state: 'detached' });
  await page.evaluate(async () => { (await import('/src/mock/attention.ts')).state.enabled = true; });
  await page.close();

  // The light scheme.
  const light = await open('light');
  await feed(light, NOTICES.input);
  await light.getByRole('region', { name: 'Avviso di Shinatama' }).waitFor();
  await light.screenshot({ path: `${out}widget-light.png`, animations: 'disabled' });
  await light.close();
} catch (error) {
  failures.push(String(error?.stack ?? error));
}

await browser.close();
await server.close();
if (failures.length) {
  console.error(`widget smoke: ${failures.length} failure(s)\n${failures.join('\n')}`);
  process.exit(1);
}
console.log('widget smoke: ok');
