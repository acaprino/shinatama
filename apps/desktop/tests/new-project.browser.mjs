import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';
import { build } from 'vite';

// The New project dialog in a browser, its real module and styles; only the IPC results are fixtures.
async function withDialogPage(run) {
  const root = fileURLToPath(new URL('..', import.meta.url));
  const entry = 'virtual:new-project-fixture';
  const bundle = await build({
    root, configFile: false, logLevel: 'error', plugins: [{
      name: 'new-project-fixture',
      resolveId(id) { if (id.endsWith(entry)) return entry; },
      load(id) {
        if (id === entry) return `
          import '@acaprino/imago/css/imago.css';
          export { createNewProjectDialog } from '/src/new-project.ts';
        `;
      },
    }],
    build: { write: false, lib: { entry, name: 'fixture', formats: ['iife'] } },
  });
  const output = bundle[0].output;
  const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;
  const browser = await chromium.launch({ executablePath, channel: !executablePath && process.platform === 'win32' ? 'msedge' : undefined });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    await page.route('https://fixture.invalid/', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html>
      <html data-im-look="oni" data-im-theme="dark"><body><main class="im-shell" id="app"></main></body></html>` }));
    await page.goto('https://fixture.invalid/');
    await page.evaluate(() => {
      window.reads = 0;
      window.__TAURI_INTERNALS__ = {
        transformCallback: () => 0, unregisterCallback() {},
        async invoke(command) {
          if (command !== 'new_project_options') throw Error('unexpected ' + command);
          window.reads++;
          // The native answer takes a while: it arrives after the events already queued.
          await new Promise(done => setTimeout(done, 200));
          return { revision: 1, roots: [{ id: 'r1', path: 'D:\\Projects' }], defaultRootId: 'r1' };
        },
      };
    });
    await page.addStyleTag({ content: output.find(asset => asset.fileName.endsWith('.css')).source });
    await page.addScriptTag({ content: output.find(asset => asset.type === 'chunk').code });
    page.setDefaultTimeout(5000);
    await run(page);
  } finally {
    await browser.close();
  }
}

test('a close event that arrives after the dialog opened again does not drop its read', () => withDialogPage(async page => {
  await page.evaluate(() => {
    window.creator = window.fixture.createNewProjectDialog({ desktop: true, onCreated() {}, notify(message) { throw Error(message); } });
    window.creator.open();
  });
  const root = page.locator('#new-project-root');
  await root.waitFor();
  await page.waitForFunction(() => !document.querySelector('#new-project-root').disabled);
  assert.equal(await root.inputValue(), 'r1');
  // Closed and opened again before the browser fires the close's event, as a loaded WebView2 does when the next key
  // press is handled first: the event then arrives while the dialog is open again, with its read in flight.
  await page.evaluate(() => { document.querySelector('#new-project-dialog').close(); window.creator.open(); });
  await page.waitForFunction(() => !document.querySelector('#new-project-root').disabled, null, { timeout: 2000 })
    .catch(async () => { throw Error('The roots were not read: ' + await page.locator('#new-project-status').textContent()); });
  assert.equal(await root.inputValue(), 'r1');
  assert.equal(await page.locator('#new-project-status').textContent(), '');
  assert.equal(await page.evaluate(() => document.querySelector('#new-project-form, #new-project-dialog form').getAttribute('aria-busy')), 'false');
  assert.equal(await page.evaluate(() => window.reads), 2, 'each opening reads the roots once');
}));
