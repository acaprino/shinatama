import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';
import { build } from 'vite';

// Exercise the mounted desktop components, including their real styles and CodeMirror, without
// spawning a native browser host or accessing a project. Only the IPC results are fixtures.
async function withSpecialistPage(run) {
  const root = fileURLToPath(new URL('..', import.meta.url));
  const entry = 'virtual:specialist-fixture';
  const bundle = await build({
    root, configFile: false, logLevel: 'error', plugins: [{
      name: 'specialist-layout-fixture',
      resolveId(id) { if (id.endsWith(entry)) return entry; },
      load(id) {
        if (id === entry) return `
          import '@acaprino/imago/css/imago.css';
          export { createFilesPanel } from '/src/files-panel.ts';
          export { createBrowserTabs } from '/src/browser-tab.ts';
          export { createTerminalFind } from '/src/terminal-find.ts';
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
      <html data-im-look="oni" data-im-theme="dark"><body><main class="im-shell im-desktop-frame" id="app">
      <div id="host" style="width:73vw;height:55vh"></div></main></body></html>` }));
    await page.goto('https://fixture.invalid/');
    await page.addStyleTag({ content: output.find(asset => asset.fileName.endsWith('.css')).source });
    await page.addScriptTag({ content: output.find(asset => asset.type === 'chunk').code });
    page.setDefaultTimeout(5000);
    await run(page);
  } finally {
    await browser.close();
  }
}

test('browser frames and file editor occupy their allocated panes', t => withSpecialistPage(async page => {
    await page.evaluate(async () => {
      const pageInfo = { id: 'fixture', title: 'Fixture', url: 'https://fixture.invalid/', openedBy: null, controller: 'user' };
      const tabs = window.fixture.createBrowserTabs({
        invoke: async command => command === 'browser_open' || command === 'browser_info' ? pageInfo : undefined,
        channel() {}, notify: message => { throw Error(message); },
      });
      const { element } = await tabs.open({ url: pageInfo.url }, 'browser-proof');
      document.querySelector('#host').append(element);
      const file = { path: 'note.md', name: 'note.md', kind: 'file', size: 6, linkInside: null };
      window.files = window.fixture.createFilesPanel({
        project: () => '/fixture', decide: async () => true, reportError: message => { throw Error(message); },
        async invoke(command) {
          switch (command) {
            case 'project_recent': return [{ name: 'Fixture', path: '/fixture', available: true }];
            case 'files_list': return { path: '', entries: [file], truncated: false };
            case 'files_read': return { path: file.path, text: '# Note', sha256: 'fixture', truncated: false, binary: false, bytes: 6 };
            case 'files_visual_parse': return { exact: true, blocks: [], editableBlocks: 1, preservedBlocks: 0 };
            case 'files_markdown': return { html: '<h1>Note</h1>', truncated: false };
            default: throw Error(command);
          }
        },
      });
    });
    const sizes = await page.evaluate(() => {
      const rect = selector => document.querySelector(selector).getBoundingClientRect();
      const host = rect('#host'), frame = rect('.browser-canvas'), bar = rect('.browser-bar');
      return [host.width, host.height, frame.width, frame.height, bar.height];
    });
    assert.ok(Math.abs(sizes[0] - sizes[2]) < 1 && sizes[3] > 100 && Math.abs(sizes[1] - sizes[3] - sizes[4]) < 2, JSON.stringify(sizes));
    t.diagnostic(`Browser host ${sizes[0]} × ${sizes[1]}, canvas ${sizes[2]} × ${sizes[3]}, navigation ${sizes[4]} px`);
    await page.evaluate(() => window.files.show());
    await page.getByRole('treeitem', { name: /note.md/ }).click({timeout: 3000});
    await page.locator('.files-preview h1').waitFor({ timeout: 5000 }).catch(async error => { throw Error(await page.locator('.files-status').textContent(), { cause: error }); });
    for (const width of [1280, 820, 540]) {
      await page.setViewportSize({ width, height: 800 });
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const boxes = await page.evaluate(() => {
        const rect = selector => { const box = document.querySelector(selector).getBoundingClientRect(); return { x: box.x, width: box.width, height: box.height }; };
        return { tree: rect('[role=tree]'), split: rect('[role=separator]'), editor: rect('.cm-editor'), preview: rect('.files-preview'), dialog: rect('#files-dialog') };
      });
      assert.ok(boxes.tree.width > 100 && boxes.editor.width > 70 && boxes.preview.width > 70, JSON.stringify(boxes));
      assert.ok(boxes.editor.height > 180 && boxes.preview.height === boxes.editor.height, JSON.stringify(boxes));
      assert.ok(boxes.tree.x < boxes.split.x && boxes.split.x < boxes.editor.x && boxes.editor.x < boxes.preview.x, JSON.stringify(boxes));
      t.diagnostic(`Viewport ${width}, dialog ${boxes.dialog.width} × ${boxes.dialog.height}, tree ${boxes.tree.width}, editor ${boxes.editor.width} × ${boxes.editor.height}, preview ${boxes.preview.width} × ${boxes.preview.height}`);
      assert.ok(boxes.dialog.width <= width && boxes.preview.x + boxes.preview.width <= width, JSON.stringify(boxes));
    }
    const editor = await page.locator('.cm-editor').elementHandle();
    await page.locator('label').filter({ has: page.getByRole('radio', { name: 'Source' }) }).click({ timeout: 3000 });
    assert.ok(await editor.evaluate(node => node === document.querySelector('.cm-editor')));
    assert.equal(await page.locator('.files-preview').isVisible(), false);
    await page.getByRole('button', { name: 'Close files' }).click();
    await page.evaluate(() => {
      const anchor = document.querySelector('#host');
      anchor.classList.add('im-tile-body');
      anchor.tabIndex = 0;
      window.fixture.createTerminalFind({
        current: () => ({ id: 'terminal', terminal: {
          element: anchor, cols: 80, select() {}, scrollToLine() {},
          buffer: { active: { length: 3, getLine: i => ({ translateToString: () => ['Find me', 'Other', 'Find me again'][i] }) } },
        } }),
        returnFocus: () => anchor.focus(), notify: message => { throw Error(message); },
      }).open();
    });
    const find = page.getByRole('dialog', { name: 'Find in terminal' });
    await find.getByRole('searchbox').fill('find');
    await find.getByText('1 of 2 matching lines').waitFor();
    await find.getByRole('button', { name: 'Next' }).click();
    await find.getByText('2 of 2 matching lines').waitFor();
    await find.getByRole('button', { name: 'Close search' }).click();
    await page.waitForFunction(() => document.activeElement.id === 'host');
}));
test('pending edits, saves and conflict decisions belong to the original file and project', () => withSpecialistPage(async page => {
  await page.evaluate(async () => {
    window.saves = [];
    window.applies = [];
    window.decisions = [];
    const entry = path => ({ path, name: path, kind: 'file', size: 8, linkInside: null });
    const panel = window.fixture.createFilesPanel({
      project: () => '/first', reportError: message => { throw Error(message); },
      decide: title => new Promise(resolve => window.decisions.push({ title, resolve })),
      async invoke(command, args) {
        switch (command) {
          case 'project_recent': return ['/first', '/second'].map(path => ({ path, name: path, available: true }));
          case 'files_list': return { path: '', entries: [entry('a.md'), entry('b.txt')], truncated: false };
          case 'files_read': return { path: args.path, sha256: 'original', bytes: 8, binary: false, truncated: false, text: args.path === 'a.md' ? '# Original' : 'Other document' };
          case 'files_visual_parse': return { exact: true, editableBlocks: 1, preservedBlocks: 0,
            blocks: [{ kind: 'heading', text: args.text.slice(2), prefix: '# ', suffix: '', level: 1, editable: true }] };
          case 'files_visual_apply': return new Promise(resolve => window.applies.push({ args, resolve }));
          case 'files_save': return new Promise(resolve => window.saves.push({ request: args.request, resolve }));
          case 'files_markdown': return { html: '<p>Preview</p>', truncated: false };
          default: throw Error(command);
        }
      },
    });
    await panel.show();
  });
  const open = name => page.getByRole('treeitem', { name: new RegExp(name) }).click();
  const mode = name => page.locator('label').filter({ has: page.getByRole('radio', { name }) }).click();
  const editor = page.locator('.cm-content');
  const save = page.getByRole('button', { name: 'Save', exact: true });
  const project = page.getByRole('combobox', { name: 'Project' });
  await open('a.md');
  await mode('Visual');
  const heading = page.locator('.files-visual input');
  await heading.fill('First edit');
  await page.waitForFunction(() => window.applies.length === 1);
  await heading.fill('Latest edit');
  await open('b.txt');
  await page.waitForFunction(() => document.querySelector('.cm-content').textContent === 'Other document');
  await page.evaluate(() => window.applies[0].resolve('# First edit'));
  await page.waitForFunction(() => window.applies.length === 2);
  assert.deepEqual(await page.evaluate(() => window.applies[1].args), { text: '# Original', edits: [{ index: 0, text: 'Latest edit' }] });
  await page.evaluate(() => window.applies[1].resolve('# Latest edit'));
  assert.equal(await editor.textContent(), 'Other document');
  await open('a.md');
  await page.waitForFunction(() => document.querySelector('.cm-content').textContent === '# Latest edit');

  await project.selectOption('/second');
  await page.waitForFunction(() => window.decisions.length === 1);
  assert.equal(await project.inputValue(), '/first');
  await page.evaluate(() => window.decisions[0].resolve(false));
  await page.waitForFunction(() => !document.querySelector('.files-project').disabled);
  assert.equal(await project.inputValue(), '/first');

  await mode('Source');
  await editor.fill('# Submitted');
  await save.click();
  await page.waitForFunction(() => window.saves.length === 1);
  assert.deepEqual(await page.evaluate(() => window.saves[0].request), {
    project: '/first', path: 'a.md', text: '# Submitted', baseSha256: 'original', overwrite: false,
  });
  await editor.fill('# Newer');
  await page.evaluate(() => window.saves[0].resolve({ state: 'saved', sha256: 'submitted' }));
  await page.getByText('Saved. Newer edits are still unsaved.').waitFor();
  assert.equal(await save.isEnabled(), true);
  await save.click();
  await page.waitForFunction(() => window.saves.length === 2);
  assert.equal(await page.evaluate(() => window.saves[1].request.baseSha256), 'submitted');
  await page.evaluate(() => window.saves[1].resolve({ state: 'conflict', disk_sha256: 'reviewed' }));
  await page.waitForFunction(() => window.decisions.length === 2);
  await open('b.txt');
  await page.waitForFunction(() => document.querySelector('.cm-content').textContent === 'Other document');
  await page.evaluate(() => window.decisions[1].resolve(true));
  await page.waitForFunction(() => window.saves.length === 3);
  assert.deepEqual(await page.evaluate(() => window.saves[2].request), {
    project: '/first', path: 'a.md', text: '# Newer', baseSha256: 'reviewed', overwrite: false,
  });
  await page.evaluate(() => window.saves[2].resolve({ state: 'saved', sha256: 'newer' }));
  assert.equal(await editor.textContent(), 'Other document');
  assert.equal(await save.isDisabled(), true);

  // A conflict prompt for a discarded project must never overwrite a same-named file in its successor.
  await open('a.md');
  await page.waitForFunction(() => document.querySelector('.cm-content').textContent === '# Newer');
  await editor.fill('# Discarded');
  await save.click();
  await page.waitForFunction(() => window.saves.length === 4);
  await page.evaluate(() => window.saves[3].resolve({ state: 'conflict', disk_sha256: 'external' }));
  await page.waitForFunction(() => window.decisions.length === 3);
  await project.selectOption('/second');
  await page.waitForFunction(() => window.decisions.length === 4);
  await page.evaluate(() => window.decisions[3].resolve(true));
  await page.waitForFunction(() => document.querySelector('.files-project').value === '/second');
  await open('a.md');
  await page.waitForFunction(() => document.querySelector('.cm-content').textContent === '# Original');
  await page.evaluate(() => window.decisions[2].resolve(true));
  assert.equal(await page.evaluate(() => window.saves.length), 4);
  assert.equal(await save.isDisabled(), true);
}));
