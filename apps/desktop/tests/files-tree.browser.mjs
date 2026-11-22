import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';
import { build } from 'vite';

// Run separately from the model tests. This exercises the adapter with real browser focus,
// keyboard events, lazy loading and the same public Imago entry point as the desktop.
test('project tree preserves focus, link boundaries and listing state', async () => {
  const bundle = await build({
    configFile: false,
    logLevel: 'warn',
    build: {
      write: false, minify: false,
      lib: { entry: fileURLToPath(new URL('../src/files-tree.ts', import.meta.url)), name: 'ProjectFiles', formats: ['iife'] },
    },
  });
  const script = bundle[0].output.find(output => output.type === 'chunk').code;
  const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;
  const browser = await chromium.launch({ executablePath, channel: !executablePath && process.platform === 'win32' ? 'msedge' : undefined });
  try {
    const page = await browser.newPage();
    await page.addScriptTag({ content: script });
    await page.evaluate(async () => {
      const entry = (path, kind = 'file', linkInside = null) => ({
        path, name: path.split('/').at(-1), kind, size: kind === 'file' ? 1250 : null, linkInside,
      });
      window.opened = [];
      window.reads = [];
      window.tree = window.ProjectFiles.createFilesTree({
        async list(path) {
          window.reads.push(path);
          if (path === 'linked.txt') throw Error('File');
          return { path, truncated: path === 'src', entries: path === '' ? [
            entry('src', 'directory'), entry('outside', 'link', false), entry('linked.txt', 'link', true),
          ] : [entry('src/file.txt')] };
        },
        open: file => window.opened.push(file.path),
        failed: issue => { throw Error(issue); },
      });
      document.body.append(window.tree.element);
      await window.tree.load();
    });
    const row = name => page.getByRole('treeitem').filter({ has: page.locator('.im-tree-label', { hasText: name }) });
    await row('src').click();
    await row('file.txt').waitFor();
    assert.match(await row('file.txt').textContent(), /1\.2 KB/);
    await row('only the first').waitFor();
    await row('outside').focus();
    await page.keyboard.press('Enter');
    assert.deepEqual(await page.evaluate(() => window.opened), []);
    await row('linked.txt').click();
    await page.waitForFunction(() => window.opened.includes('linked.txt'));
    await row('file.txt').click();
    assert.deepEqual(await page.evaluate(() => window.opened), ['linked.txt', 'src/file.txt']);
    assert.deepEqual(await page.evaluate(() => window.reads), ['', 'src', 'linked.txt']);
    await page.evaluate(() => {
      window.original = document.querySelector('[data-im-entry="src/file.txt"]');
      window.tree.results([{ path: 'src/file.txt', name: 'file.txt', kind: 'file', size: 1250, linkInside: null }]);
      window.tree.focus();
    });
    assert.equal(await page.evaluate(() => document.activeElement.closest('[role=tree]').getAttribute('aria-label')), 'Matching files');
    await page.evaluate(() => window.tree.reveal('src/file.txt'));
    assert.equal(await page.evaluate(() => window.original === document.activeElement && window.original.getAttribute('aria-selected') === 'true'), true);
  } finally {
    await browser.close();
  }
});
