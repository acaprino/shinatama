// The browser tab's controller, on a window with no Rust (fake-host.ts): the pages that are open and their tabs, what a tab sends (its
// input in batches, its size, whether it is in sight, the address and the steps) and the frames it gets back.
import test, { afterEach, beforeEach, mock } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { addressUrl, controlStrip, pageLabel, parseSaved, reopenTarget } from '../src/browser-model.ts';
import { POLL_MS } from '../src/controllers/browser.ts';
import { fakeHost, tick, type Answers } from './fake-host.ts';

const store = new Map<string, string>();
beforeEach(() => {
  store.clear();
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: { getItem: (key: string) => store.get(key) ?? null, setItem: (key: string, value: string) => { store.set(key, value); }, removeItem: (key: string) => { store.delete(key); } },
  });
});
afterEach(() => { overrideInvoke(null); mock.timers.reset(); });

const info = (id: string, extra: Record<string, unknown> = {}) => ({ id, url: `https://example.com/${id}`, title: `Page ${id}`, controller: 'user', live: true, openedBy: null, ...extra });
const mouse = (kind: 'pressed' | 'released' | 'moved', x: number) => ({ type: 'mouse' as const, kind, x, y: 0, button: 'left', buttons: 1, modifiers: 0, clickCount: kind === 'moved' ? 0 : 1 });

async function opened(answers: Answers = {}) {
  const host = fakeHost({ browser_open: info('p1'), browser_frames: null, browser_visibility: null, browser_resize: null, browser_input: null, browser_navigate: null, browser_info: info('p1'), ...answers });
  const view = await host.app.browser.open({ url: 'https://example.com/p1' });
  assert.equal(view, 'browser-p1');
  return { ...host, view: view! };
}
const callsOf = (calls: { command: string }[], command: string) => calls.filter(call => call.command === command);

test('the control strip tells who drives a page, and an address with no scheme is https', () => {
  assert.deepEqual(controlStrip({ controller: 'agent:i_a', openedBy: 'i_a' }), { text: "L'agente sta usando questa pagina", action: 'take' });
  assert.deepEqual(controlStrip({ controller: 'user', openedBy: 'i_a' }), { text: "Hai il controllo della pagina dell'agente", action: 'give' });
  assert.equal(controlStrip({ controller: 'user', openedBy: null }), null);
  assert.equal(addressUrl('  example.com/a '), 'https://example.com/a');
  assert.equal(addressUrl('http://localhost:3000'), 'http://localhost:3000');
  assert.equal(addressUrl('file:///c:/x.html'), 'file:///c:/x.html');
  assert.equal(addressUrl('   '), null);
});

test('a tab is named by its page, or by its address, and says when the page is closed', () => {
  assert.equal(pageLabel({ url: 'https://example.com/', title: 'Example' }, true), 'Example');
  assert.equal(pageLabel({ url: 'https://example.com/docs/', title: '' }, true), 'example.com/docs');
  assert.equal(pageLabel({ url: 'https://example.com', title: 'Example' }, false), 'Example · closed');
  assert.equal(pageLabel(null, false), 'Browser');
  assert.deepEqual(parseSaved('{"a":{"url":"u","title":3,"file":"f.html"},"b":1,"c":{"title":"x"}}'), { a: { url: 'u', title: '', file: 'f.html' } });
  assert.deepEqual(parseSaved('not json'), {});
  assert.deepEqual(reopenTarget({ url: 'https://x', title: '', file: 'C:\\doc.html' }), { file: 'C:\\doc.html' });
});

test('a page that opens is a tab of the layout, named by its title, and is kept to be offered again', async () => {
  const { app, view } = await opened();
  assert.equal(app.browser.isOpen(view), true);
  assert.equal(app.browser.label(view), 'Page p1');
  assert.equal(app.layout.view(view)?.resource.kind, 'browser');
  assert.deepEqual(JSON.parse(store.get('shinatama.browser.tabs')!), { 'browser-p1': { url: 'https://example.com/p1', title: 'Page p1' } });
  // After a restart the tab is in the layout, the page is not: the tab says so and has its last address.
  const restarted = fakeHost({}).app;
  assert.equal(restarted.browser.isOpen(view), false);
  assert.equal(restarted.browser.label(view), 'Page p1 · closed');
  assert.equal(restarted.browser.savedPage(view)?.url, 'https://example.com/p1');
});

test('a page that cannot be opened says so in a notice and no tab is made', async () => {
  const { app, notices } = fakeHost({ browser_open: () => { throw 'The page could not be opened.'; } });
  assert.equal(await app.browser.open({ url: 'https://example.com' }), null);
  assert.equal(notices.length, 1);
  assert.equal(notices[0]!.error, true);
  assert.deepEqual(app.layout.layout.views, []);
});

test('a page an agent opened is shown by its id, and a tab opens once', async () => {
  const { app, calls } = await opened();
  assert.equal(app.browser.viewOf('p1'), 'browser-p1');
  assert.equal(app.browser.viewOf('p9'), null);
  assert.equal(callsOf(calls, 'browser_info').length, 0);
  await app.browser.open({ page: 'p1' });
  assert.equal(callsOf(calls, 'browser_info').length, 1);
});

test('the frames of a page go to the tab that is drawn, and stop when it goes', async () => {
  let channel: { onmessage?: (bytes: ArrayBuffer) => void } = {};
  const { app, view } = await opened({ browser_frames: (args) => { channel = args?.channel as typeof channel; return null; } });
  const got: number[] = [];
  const disconnect = app.browser.connect(view, bytes => got.push(bytes.byteLength));
  await tick(1);
  channel.onmessage!(new ArrayBuffer(3));
  disconnect();
  channel.onmessage!(new ArrayBuffer(5));
  assert.deepEqual(got, [3]);
  app.browser.dispose();
});

test('what the person does is sent in batches: a burst of moves is one, in order, one call at a time', async () => {
  const batches: unknown[][] = [];
  let release: () => void = () => {};
  const { app, view } = await opened({ browser_input: (args) => { batches.push(args?.events as unknown[]); return new Promise<void>(resolve => { release = resolve; }); } });
  let run: () => void = () => {};
  app.browser.schedule = fn => { run = fn; };
  app.browser.connect(view, () => {});
  app.browser.input(view, mouse('pressed', 1));
  app.browser.input(view, mouse('moved', 2));
  app.browser.input(view, mouse('moved', 3));
  app.browser.input(view, mouse('released', 3));
  run();
  await tick(1);
  assert.deepEqual(batches, [[mouse('pressed', 1), mouse('moved', 3), mouse('released', 3)]]);
  // While that call is out, more input waits, and goes when it is answered.
  app.browser.input(view, mouse('moved', 9));
  run();
  await tick(1);
  assert.equal(batches.length, 1);
  release();
  await tick(1);
  assert.deepEqual(batches[1], [mouse('moved', 9)]);
  release();
  app.browser.dispose();
});

test('input for a tab that is not connected goes nowhere', async () => {
  const { app, view, calls } = await opened();
  let scheduled = 0;
  app.browser.schedule = () => { scheduled += 1; };
  app.browser.input(view, mouse('moved', 1));
  assert.equal(scheduled, 0);
  assert.equal(callsOf(calls, 'browser_input').length, 0);
});

test('the size goes to the page as whole pixels, and a box with no size is ignored', async () => {
  const { app, view, calls } = await opened();
  app.browser.resize(view, { width: 640.4, height: 480.6, dpr: 1.5 });
  app.browser.resize(view, { width: 0, height: 480, dpr: 1 });
  await tick(1);
  assert.deepEqual(callsOf(calls, 'browser_resize').map(call => call.args), [{ page: 'p1', width: 640, height: 481, dpr: 1.5 }]);
});

test('a tab in sight tells Rust, and reads its page again every so often; one out of sight does not', async () => {
  mock.timers.enable({ apis: ['setInterval'] });
  const { app, view, calls } = await opened();
  app.browser.connect(view, () => {});
  app.browser.setVisible(view, true);
  await tick(1);
  assert.deepEqual(callsOf(calls, 'browser_visibility').map(call => call.args), [{ page: 'p1', visible: true }]);
  const reads = () => callsOf(calls, 'browser_info').length;
  const first = reads();
  mock.timers.tick(POLL_MS * 2);
  await tick(1);
  assert.equal(reads(), first + 2);
  app.browser.setVisible(view, false);
  const after = reads();
  mock.timers.tick(POLL_MS * 3);
  await tick(1);
  assert.equal(reads(), after);
  app.browser.dispose();
});

test('the address, back, forward and reload ask Rust and read the page again', async () => {
  let url = 'https://example.com/p1';
  const { app, view, calls } = await opened({
    browser_navigate: (args) => { if (args?.action === 'go') url = String(args.url); return null; },
    browser_info: () => info('p1', { url }),
  });
  assert.equal(await app.browser.go(view, 'example.org/a'), true);
  assert.equal(app.browser.pages.get(view)?.url, 'https://example.org/a');
  assert.equal(await app.browser.go(view, '  '), false);
  await app.browser.navigate(view, 'back');
  await app.browser.navigate(view, 'forward');
  await app.browser.navigate(view, 'reload');
  assert.deepEqual(callsOf(calls, 'browser_navigate').map(call => call.args), [
    { page: 'p1', action: 'go', url: 'https://example.org/a' }, { page: 'p1', action: 'back', url: undefined },
    { page: 'p1', action: 'forward', url: undefined }, { page: 'p1', action: 'reload', url: undefined },
  ]);
});

test('a refused step is said in a notice', async () => {
  const { app, view, notices } = await opened({ browser_navigate: () => { throw 'Navigation was refused.'; } });
  await app.browser.navigate(view, 'reload');
  assert.deepEqual(notices.map(notice => notice.text), ['Navigation was refused.']);
});

test('the person takes an agent\'s page and gives it back; the system browser opens its address', async () => {
  const taken = info('p1', { controller: 'user', openedBy: 'i_a' });
  const { app, view, calls } = await opened({ browser_open: info('p1', { controller: 'agent:i_a', openedBy: 'i_a' }), browser_control: (args) => (args?.take === true ? taken : info('p1', { controller: 'agent:i_a', openedBy: 'i_a' })), browser_external: null });
  assert.equal(controlStrip(app.browser.pages.get(view)!)?.action, 'take');
  await app.browser.control(view, true);
  assert.equal(controlStrip(app.browser.pages.get(view)!)?.action, 'give');
  await app.browser.control(view, false);
  assert.equal(controlStrip(app.browser.pages.get(view)!)?.action, 'take');
  await app.browser.external(view);
  assert.deepEqual(callsOf(calls, 'browser_external').map(call => call.args), [{ page: 'p1' }]);
});

test('closing a tab closes its page, takes the tab out of the layout and forgets it', async () => {
  const { app, view, calls } = await opened({ browser_close: null });
  await app.browser.close(view);
  assert.deepEqual(callsOf(calls, 'browser_close').map(call => call.args), [{ page: 'p1' }]);
  assert.equal(app.browser.isOpen(view), false);
  assert.equal(app.layout.view(view), undefined);
  assert.deepEqual(JSON.parse(store.get('shinatama.browser.tabs')!), {});
});

test('a saved tab whose page is closed opens that page again in a new tab, from its file when it had one, and the old tab goes', async () => {
  const opens: unknown[] = [];
  const { app, view } = await opened({ browser_open: (args) => { opens.push(args); return info(opens.length === 1 ? 'p1' : 'p2'); } });
  // Opened from a local document first: reopening is from the file again.
  await app.browser.close(view);
  await app.browser.open({ file: 'C:\\docs\\a.html' });
  const second = app.browser.viewOf('p2')!;
  const restarted = fakeHost({ browser_open: (args) => { opens.push(args); return info('p3'); } }).app;
  assert.equal(await restarted.browser.reopen(second), 'browser-p3');
  assert.deepEqual(opens.at(-1), { file: 'C:\\docs\\a.html' });
  assert.equal(restarted.browser.savedPage(second), null);
  assert.equal(await restarted.browser.reopen('browser-nothing'), null);
});
