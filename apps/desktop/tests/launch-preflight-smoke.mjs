// One shared engine result through the real CLI and embedded Tauri WebView.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readdir, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir as systemTmpdir } from 'node:os';
import { realpathSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from '@playwright/test';
// The temporary folder by its long name: a runner's is C:\Users\RUNNER~1\..., and the app answers with the long one.
const tmpdir = () => realpathSync.native(systemTmpdir());

if (process.platform !== 'win32') throw new Error('This preflight bridge check qualifies Windows only.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-launch-preflight-'));
const data = resolve(fixture, 'data');
const port = 19236;
const evidence = { check: 'C11.4-common-preflight', checks: {}, nativeExecutionQualified: false, status: 'running' };
let app, browser, page;

const invoke = (command, args) => page.evaluate(
  ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const exitWithin = async milliseconds => {
  if (!app || app.exitCode !== null || app.signalCode !== null) return;
  await new Promise((done, reject) => {
    const completed = () => { clearTimeout(timer); done(); };
    const timer = setTimeout(() => { app.removeListener('exit', completed); reject(new Error('Owned preflight-test app exit deadline exceeded.')); }, milliseconds);
    app.once('exit', completed);
  });
};
function cli(request) {
  const result = spawnSync(resolve(workspace, 'target/debug/shinatama.exe'), ['launch', 'preflight', '--json'], {
    input: JSON.stringify(request), encoding: 'utf8', timeout: 5000, maxBuffer: 64 * 1024,
    windowsHide: true, env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'cli-unused-home') },
  });
  assert.ifError(result.error);
  assert.equal(result.status, 2);
  return JSON.parse(result.stdout);
}

try {
  await new Promise((done, reject) => {
    const probe = createServer();
    probe.once('error', () => reject(new Error('Preflight-test CDP port is occupied; no existing app was attached.')));
    probe.listen({ host: '127.0.0.1', port, exclusive: true }, () => probe.close(error => error ? reject(error) : done()));
  });
  app = spawn(resolve(workspace, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: workspace, windowsHide: true, stdio: 'ignore',
    // Marionette's background install writes its store once the data folder is accepted, at a moment of
    // its own: off here, so "preflight writes nothing" compares the folder without it.
    env: { ...process.env, SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'), SHINATAMA_MARIONETTE: 'off',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
  });
  let spawnError;
  app.once('error', error => { spawnError = error; });
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (spawnError) throw new Error('Cannot start owned preflight-test app.');
    if (app.exitCode !== null) throw new Error(`Owned preflight-test app exited: ${app.exitCode}`);
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 750 }); break; }
    catch { await delay(200); }
  }
  assert.ok(browser, 'Embedded preflight-test WebView startup deadline exceeded.');
  const context = browser.contexts()[0];
  // The attention widget is a page too; the workspace is the one that is not widget.html.
  for (const deadline = Date.now() + 10000; !(page = context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'))) && Date.now() < deadline;) await delay(100);
  if (!page) throw new Error('The workspace page did not load.');
  await page.waitForURL(url => url.hostname === 'tauri.localhost', { timeout: 10000 });
  const storage = await invoke('storage_location');
  const pathKey = value => value.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();
  assert.equal(pathKey(storage.directory), pathKey(data));
  assert.deepEqual(await invoke('terminal_list'), []);
  const beforeFiles = (await readdir(data)).sort();

  for (const action of ['start', 'resume']) {
    for (const program of ['system', 'own']) {
      for (const home of ['system', 'box']) {
        const request = { agent: 'codex', action, placement: { program, home } };
        if (program === 'own' && home === 'system') {
          const refusal = await invoke('launch_preflight', { request }).then(
            () => null, error => String(error));
          assert.ok(refusal?.includes('Invalid launch selectors.'));
          assert.equal(cli(request).status, 'refused');
          continue;
        }
        const ui = await invoke('launch_preflight', { request });
        assert.deepEqual(ui, cli(request));
        assert.equal(ui.status, 'refused');
        assert.ok(ui.issues.length > 0);
        assert.equal(ui.synthetic, false);
      }
    }
  }
  evidence.checks.uiCliStartResumeConverge = true;
  evidence.checks.forbiddenPlacementRefused = true;

  const marker = 'PRIVATE_INPUT_MUST_NOT_BE_REFLECTED';
  for (const request of [
    { agent: marker, action: 'start', placement: { program: 'system', home: 'box' } },
    { agent: 'codex', action: 'start', placement: { program: 'system', home: 'box' }, token: marker },
  ]) {
    const response = await invoke('launch_preflight', { request }).then(
      value => ({ value }), error => ({ error: String(error) }));
    assert.ok(response.error);
    assert.ok(!response.error.includes(marker));
  }
  evidence.checks.invalidInputRedacted = true;
  assert.deepEqual(await invoke('terminal_list'), []);
  assert.deepEqual((await readdir(data)).sort(), beforeFiles);
  assert.ok(!(await readdir(fixture)).includes('cli-unused-home'));
  evidence.checks.noLaunchOrStorePreparation = true;
  assert.equal(await page.evaluate(() => new Promise(done => requestAnimationFrame(() => done('responsive')))), 'responsive');
  evidence.checks.webviewResponsive = true;
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  await exitWithin(10000);
  assert.equal(app.exitCode, 0);
  evidence.status = 'passed';
  console.log('PASS: C11.4 real UI/CLI preflight convergence, refusal, redaction and no launch.');
} catch (error) {
  evidence.status = 'failed';
  throw error;
} finally {
  if (app && app.exitCode === null && app.signalCode === null) { app.kill(); await exitWithin(3000).catch(() => {}); }
  if (browser) await browser.close().catch(() => {});
  const output = resolve(workspace, '.prototype-evidence');
  await mkdir(output, { recursive: true });
  await writeFile(resolve(output, 'launch-preflight.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  const absolute = resolve(fixture);
  assert.equal(dirname(absolute), resolve(tmpdir()));
  assert.ok(basename(absolute).startsWith('shinatama-launch-preflight-'));
  await rm(absolute, { recursive: true, force: true, maxRetries: 8, retryDelay: 100 });
}
