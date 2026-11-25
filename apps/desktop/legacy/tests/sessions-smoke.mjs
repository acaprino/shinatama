// One coordinated native Windows/CDP proof. All archive contents are synthetic.
// Run after rebuilding the normal desktop and shinatama-session-helper.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-sessions-smoke-'));
const data = resolve(fixture, 'data');
const archive = resolve(fixture, 'synthetic-claude');
const project = resolve(archive, 'projects/fixture-project');
await mkdir(project, { recursive: true });
const detailSentinel = 'SHINATAMA_DETAIL_SAFE_TEXT';
const literalHtml = '<img id="detail-untrusted-image" src="data:," onerror="globalThis.__sessionDetailExecuted=true"><script>globalThis.__sessionDetailExecuted=true</script>';
const rawSentinel = 'SHINATAMA_DETAIL_RAW_NEVER_RENDER';
const reasoningSentinel = 'SHINATAMA_DETAIL_REASONING_NEVER_RENDER';
const opaqueSentinel = 'SHINATAMA_DETAIL_OPAQUE_NEVER_RENDER';
function canonicalDetail(id) {
  const records = []; let parent = null;
  for (let turn = 0; turn < 26; turn++) {
    const suffix = String(turn).padStart(2, '0'); const user = `entry-user-${suffix}`; const assistant = `entry-assistant-${suffix}`;
    records.push({ type: 'user', uuid: user, parentUuid: parent, sessionId: id, cwd: 'C:\\synthetic-session-project',
      timestamp: `2026-11-03T00:00:${suffix}Z`, rawOnlySentinel: rawSentinel,
      message: { role: 'user', content: turn === 0 ? `${detailSentinel} ${literalHtml}` : `SHINATAMA_DETAIL_USER_${suffix}` } });
    records.push({ type: 'assistant', uuid: assistant, parentUuid: user, sessionId: id,
      message: { id: `message-${suffix}`, role: 'assistant', model: 'fixture-model', stop_reason: 'end_turn',
        content: [{ type: 'text', text: `SHINATAMA_DETAIL_ASSISTANT_${suffix}` },
          ...(turn === 0 ? [{ type: 'thinking', thinking: reasoningSentinel, signature: opaqueSentinel }] : [])] } });
    parent = assistant;
  }
  return `${records.map(record => JSON.stringify(record)).join('\n')}\n`;
}
const originals = new Map();
for (let index = 0; index < 53; index++) {
  const id = `idsession-${String(index).padStart(2, '0')}`;
  const name = `${id}.jsonl`;
  const bytes = Buffer.from(index === 0 ? canonicalDetail(id) : `${JSON.stringify({ sessionId: id, cwd: 'C:\\synthetic-session-project' })}\n`);
  originals.set(name, bytes); await writeFile(resolve(project, name), bytes);
}
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const resultPath = resolve(output, 'sessions-library.json');
const evidence = { check: 'C22-C23-native-session-library-and-detail', status: 'running', checks: {},
  scope: '53 synthetic Claude sessions; 53-event detail, explicit RAW with mocked clipboard, full-session literal search and read-only jump; no native agent start' };
const save = () => writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`);
await save();
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
const app = spawn(resolve(workspace, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: workspace, windowsHide: true, stdio: 'ignore', env: { ...process.env,
    SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let browser, page, owned = false, spawnError;
const errors = [];
app.once('error', error => { spawnError = error; });
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();
function within(promise, milliseconds, label) {
  let timer;
  return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} exceeded its deadline.`)), milliseconds); })])
    .finally(() => clearTimeout(timer));
}
async function reply(command, args = {}) {
  return within(page.evaluate(async ({ command, args }) => {
    try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
    catch (issue) { return { ok: false, issue }; }
  }, { command, args }), 16000, command);
}
async function ipc(command, args = {}) {
  const result = await reply(command, args);
  if (!result.ok) throw new Error(`${command} refused (${typeof result.issue === 'string' ? result.issue : 'typed error'}).`);
  return result.value;
}
function exitWithin(milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return new Promise((accept, reject) => {
    const done = () => { clearTimeout(timer); accept(); };
    const timer = setTimeout(() => { app.removeListener('exit', done); reject(new Error('Owned sessions-test app exit deadline exceeded.')); }, milliseconds);
    app.once('exit', done);
  });
}
async function assertSourceUnchanged() {
  expect((await readdir(archive)).sort()).toEqual(['projects']);
  expect((await readdir(resolve(archive, 'projects'))).sort()).toEqual(['fixture-project']);
  expect((await readdir(project)).sort()).toEqual([...originals.keys()].sort());
  const hash = createHash('sha256');
  for (const [name, bytes] of originals) {
    const after = await readFile(resolve(project, name)); expect(after.equals(bytes)).toBe(true);
    hash.update(name); hash.update(after);
  }
  return hash.digest('hex');
}
try {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (spawnError) throw new Error(`Cannot start owned sessions-test app (${spawnError.code}).`);
    if (app.exitCode !== null) throw new Error('Owned sessions-test app exited during startup.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
    catch { await delay(250); }
  }
  if (!browser) throw new Error('Owned sessions-test WebView startup exceeded 30 seconds.');
  const context = browser.contexts()[0];
  const mainDeadline = Date.now() + 10000;
  while (Date.now() < mainDeadline && !page) {
    page = context.pages().find(candidate => {
      try { const url = new URL(candidate.url()); return url.hostname === 'tauri.localhost' && ['/', '/index.html'].includes(url.pathname); }
      catch { return false; }
    });
    if (!page) await delay(100);
  }
  if (!page) throw new Error('Owned main workspace WebView was not found.');
  await page.waitForURL(url => url.hostname === 'tauri.localhost', { timeout: 10000 });
  await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 30000 });
  const storage = await ipc('storage_location');
  if (pathKey(storage.directory) !== pathKey(data)) throw new Error('Synthetic storage identity mismatch; no UI was touched.');
  owned = true; page.on('pageerror', error => { errors.push(error.message.slice(0, 1024)); });
  expect(await ipc('terminal_list')).toEqual([]);
  const sourceBefore = await assertSourceUnchanged();
  await page.locator('#toolbar-more').click(); await page.locator('#sessions').click();
  const dialog = page.locator('#session-library-dialog');
  await dialog.getByRole('button', { name: 'Load archives', exact: true }).click();
  await expect(dialog.locator('#session-library-status')).toContainText('No registered archives');
  expect(await ipc('session_roots')).toEqual([]);
  await expect(dialog.locator('#session-library-root option')).toHaveCount(1);
  evidence.checks.noAmbientArchiveDiscovery = true;

  await dialog.getByText('Add a read-only archive', { exact: true }).click();
  await dialog.locator('#session-archive-agent').selectOption('claude_code');
  await dialog.locator('#session-archive-path').fill(archive);
  await dialog.getByRole('button', { name: 'Add archive', exact: true }).click();
  await expect(dialog.locator('#session-library-status')).toContainText('Archive registered', { timeout: 15000 });
  const roots = await ipc('session_roots'); expect(roots).toHaveLength(1);
  const root = roots[0]; expect(root.harness).toBe('claude_code'); expect(root.kind).toBe('system'); expect(root.access).toBe('read_only');
  const selector = { storeRootId: root.storeRootId, configurationRevision: root.configurationRevision };
  expect(root.available).toBe(true); expect(root.label).toContain('synthetic-claude');
  evidence.checks.explicitReadOnlyArchive = true;

  await expect(page.locator('#resource-status')).toContainText('Resources: available', { timeout: 20000 });
  await dialog.getByRole('button', { name: 'Reload sessions', exact: true }).click();
  const rows = dialog.locator('tbody tr');
  await expect(rows).toHaveCount(50, { timeout: 15000 });
  await expect(dialog.locator('.session-library-pagination')).toContainText('1–50 of 53');
  const firstId = await rows.first().getByRole('button').getAttribute('data-native-id'); expect(firstId).toBe('idsession-00');
  await dialog.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(rows).toHaveCount(3, { timeout: 15000 });
  await expect(dialog.locator('.session-library-pagination')).toContainText('51–53 of 53');
  await dialog.getByRole('button', { name: 'Previous', exact: true }).click();
  await expect(rows).toHaveCount(50, { timeout: 15000 });
  await dialog.locator('#session-library-filter').fill(firstId);
  await expect(rows).toHaveCount(1);
  await rows.locator(`button[data-native-id="${firstId}"]`).click();
  await expect(dialog.locator('.session-library-metadata')).toContainText('Unknown');
  const openDetail = dialog.getByRole('button', { name: 'Open detail', exact: true });
  await expect(openDetail).toBeEnabled();
  await expect(dialog.getByRole('button', { name: 'Resume unavailable', exact: true })).toBeDisabled();
  await expect(dialog.getByRole('button', { name: 'Hop unavailable', exact: true })).toBeDisabled();
  evidence.checks.pages = [50, 3, 50]; evidence.checks.localFilterAndUnknownMetadata = true;

  const originalUrl = page.url();
  expect(await page.evaluate(() => Boolean(globalThis.__sessionDetailExecuted))).toBe(false);
  await openDetail.click();
  const detail = page.locator('#session-detail-dialog');
  const events = detail.locator('.session-detail-events article');
  await expect(detail).toBeVisible();
  expect(await detail.locator('h1').evaluate(element => {
    const rect = element.getBoundingClientRect();
    return document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2)?.closest('dialog')?.id;
  })).toBe('session-detail-dialog');
  await expect(events).toHaveCount(50, { timeout: 15000 });
  await expect(detail.locator('.session-detail-pagination')).toContainText('1–50 of 53 events');
  await expect(events.first().locator('pre')).toContainText(detailSentinel);
  await expect(events.first().locator('pre')).toContainText(literalHtml);
  await expect(detail.locator('img,script')).toHaveCount(0);
  await expect(detail).not.toContainText(rawSentinel);
  await expect(detail).not.toContainText(reasoningSentinel);
  await expect(detail).not.toContainText(opaqueSentinel);
  expect(await page.evaluate(() => Boolean(globalThis.__sessionDetailExecuted))).toBe(false);
  expect(page.url()).toBe(originalUrl);
  await detail.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(events).toHaveCount(3, { timeout: 15000 });
  await expect(detail.locator('.session-detail-pagination')).toContainText('51–53 of 53 events');
  await detail.getByRole('button', { name: 'Previous', exact: true }).click();
  await expect(events).toHaveCount(50, { timeout: 15000 });
  await detail.getByRole('button', { name: 'Overview', exact: true }).click();
  await expect(detail.locator('section[aria-label="Overview"]')).toBeVisible();
  await expect(detail.locator('.session-detail-overview')).toContainText('Unknown');
  await detail.getByRole('button', { name: 'Lineage', exact: true }).click();
  await expect(detail.locator('section[aria-label="Lineage"]')).toContainText('Parent session ID: Unknown');
  await expect(detail.locator('section[aria-label="Lineage"]')).toContainText('Direct children recorded: Unknown');
  await detail.getByRole('button', { name: 'Transcript', exact: true }).click();
  await expect(events.first().locator('pre')).toContainText(detailSentinel);
  await page.screenshot({ path: resolve(output, 'sessions-detail.png'), timeout: 3000 });
  await page.evaluate(() => {
    // Tauri's invoke property is immutable. Observe the real custom-protocol
    // transport without replacing results or recording payloads/headers.
    const original=window.fetch.bind(window);
    globalThis.__rawReads=0; globalThis.__findReads=0; globalThis.__listReads=0;
    window.fetch=(input,...args)=>{
      const url=new URL(typeof input==='string'?input:input instanceof URL?input.href:input.url);
      if(url.hostname==='ipc.localhost'){
        if(url.pathname==='/session_raw')globalThis.__rawReads++;
        if(url.pathname==='/session_find')globalThis.__findReads++;
        if(url.pathname==='/session_list')globalThis.__listReads++;
      }
      return original(input,...args);
    };
    Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async value=>{globalThis.__rawCopyText=value;}}});
  });
  await detail.getByRole('button',{name:'RAW entry',exact:true}).first().click();
  const raw=page.locator('#session-raw-dialog');
  await expect(raw).toBeVisible();
  await expect(raw.locator('#session-raw-content')).toHaveText('');
  await expect(raw.locator('#session-raw-load')).toBeDisabled();
  expect(await page.evaluate(()=>globalThis.__rawReads)).toBe(0);
  await raw.locator('#session-raw-confirm').check();
  expect(await page.evaluate(()=>globalThis.__rawReads)).toBe(0);
  await raw.locator('#session-raw-load').click();
  await expect(raw.locator('#session-raw-content')).toContainText(rawSentinel,{timeout:15000});
  expect(await page.evaluate(()=>globalThis.__rawReads)).toBe(1);
  await expect(raw.locator('img,script')).toHaveCount(0);
  await raw.locator('#session-raw-copy').click();
  expect(await page.evaluate(sentinel=>typeof globalThis.__rawCopyText==='string'&&globalThis.__rawCopyText.includes(sentinel),rawSentinel)).toBe(true);
  await raw.getByRole('button',{name:'Close RAW entry',exact:true}).click();
  await expect(raw).toBeHidden(); await expect(raw.locator('#session-raw-content')).toHaveText('');
  await expect(detail).not.toContainText(rawSentinel);
  evidence.checks.rawExplicitConfirmationAndCloseWipe=true;
  evidence.checks.rawCopyContractMockedWithoutSystemClipboardWrite=true;
  await detail.getByRole('button',{name:'Find in conversation',exact:true}).click();
  const find=page.locator('#session-find-dialog');
  await expect(find).toBeVisible();
  expect(await page.evaluate(()=>globalThis.__findReads)).toBe(0);
  await find.locator('#session-find-query').fill('SHINATAMA_DETAIL_');
  expect(await page.evaluate(()=>globalThis.__findReads)).toBe(0);
  await find.getByRole('button',{name:'Search this session',exact:true}).click();
  const hits=find.locator('.session-find-match');
  await expect(hits).toHaveCount(50,{timeout:15000});
  await expect(find.locator('.session-find-pagination')).toContainText('1–50 of 52');
  await expect(find).not.toContainText(rawSentinel); await expect(find).not.toContainText(reasoningSentinel);
  await expect(find.locator('img,script')).toHaveCount(0);
  await find.getByRole('button',{name:'Next matches',exact:true}).click();
  await expect(hits).toHaveCount(2,{timeout:15000});
  await expect(find.locator('.session-find-pagination')).toContainText('51–52 of 52');
  await hits.last().click(); await expect(find).toBeHidden();
  await expect(events).toHaveCount(1,{timeout:15000});
  await expect(events.first().locator('pre')).toContainText('SHINATAMA_DETAIL_ASSISTANT_25');
  await expect(detail.locator('.session-detail-pagination')).toContainText('53–53 of 53 events');
  await detail.getByRole('button',{name:'Find in conversation',exact:true}).click();
  await expect(find.locator('#session-find-query')).toHaveValue('');
  await expect(hits).toHaveCount(0);
  await find.locator('#session-find-query').fill(rawSentinel);
  await find.getByRole('button',{name:'Search this session',exact:true}).click();
  await expect(find.locator('.session-find-status')).toContainText('No matching canonical text',{timeout:15000});
  await expect(hits).toHaveCount(0);
  await find.getByRole('button',{name:'Close session search',exact:true}).click();
  evidence.checks.fullSessionSearchPagesAndReadOnlyJump=[50,2,53];
  evidence.checks.searchExplicitOnlyAndRawExcluded=true;
  expect(await ipc('terminal_list')).toEqual([]);
  await detail.getByRole('button', { name: 'Close session detail', exact: true }).click();
  await expect(detail).toBeHidden(); await expect(dialog).toBeVisible();
  await expect(openDetail).toBeEnabled();
  await dialog.getByRole('button',{name:'Projects / sessions',exact:true}).click();
  const groups=dialog.locator('#session-library-projects details.session-project-group');
  await expect(groups).toHaveCount(1);
  await expect(groups.first().locator('summary')).toContainText('C:\\synthetic-session-project');
  await groups.first().locator('summary').click();
  await groups.first().locator(`button[data-native-id="${firstId}"]`).click();
  await expect(openDetail).toBeEnabled();
  expect(await page.evaluate(()=>globalThis.__listReads)).toBe(0);
  await dialog.getByRole('button',{name:'List',exact:true}).click();
  await expect(rows).toHaveCount(1);
  evidence.checks.currentPageProjectGroupsAreLocalAndPreserveSelection=true;
  evidence.checks.detailPages = [50, 3, 50]; evidence.checks.detailOverviewAndLineageUnknown = true;
  evidence.checks.literalHtmlInertAndNoRawOrReasoning = true; evidence.checks.detailReturnsToLibraryWithoutLaunch = true;

  const duplicate = await ipc('session_archive_add', { request: { harness: 'claude_code', path: archive } });
  expect(duplicate.storeRootId).toBe(root.storeRootId); expect(duplicate.configurationRevision).toBe(root.configurationRevision);
  expect(await ipc('session_roots')).toHaveLength(1);
  const request = { root: selector, cursor: null, limit: 50, includeChildren: true };
  const missing = await reply('session_list', { request: { ...request, root: { storeRootId: 'missing-explicit-root', configurationRevision: 1 } } });
  expect(missing.ok).toBe(false); expect(missing.issue).toBe('root_unavailable');
  const stale = await reply('session_list', { request: { ...request, root: { ...selector, configurationRevision: selector.configurationRevision + 1 } } });
  expect(stale.ok).toBe(false); expect(stale.issue).toBe('root_changed');
  evidence.checks.duplicateIdentityStable = true; evidence.checks.missingAndStaleRefused = true;
  expect(await assertSourceUnchanged()).toBe(sourceBefore); evidence.checks.all53SourceFilesUnchanged = true;
  expect(await ipc('terminal_list')).toEqual([]); evidence.checks.noAgentOrTerminalStarted = true;
  expect(errors).toEqual([]);
  await page.screenshot({ path: resolve(output, 'sessions-library.png'), timeout: 3000 });
  await dialog.getByRole('button', { name: 'Close sessions', exact: true }).click();
  await expect(dialog).toBeHidden();
  const closing = Promise.all([page.waitForEvent('close', { timeout: 15000 }), exitWithin(15000)]);
  void closing.catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  await closing; expect(app.exitCode).toBe(0);
  expect(await assertSourceUnchanged()).toBe(sourceBefore);
  evidence.status = 'passed'; await save();
  console.log('PASS: bounded archive/detail pages, inert HTML, default privacy, explicit RAW and mocked copy, complete-session search and read-only jump, unchanged sources, no agent startup, owned frame close.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = String(error.message).slice(0, 4096);
  if (owned && page && !page.isClosed()) {
    try {
      evidence.diagnostic = await within(page.evaluate(() => ({
        status: document.querySelector('#session-library-status')?.textContent?.slice(0, 2048),
        rowCount: document.querySelectorAll('#session-library-dialog tbody tr').length,
        pagination: document.querySelector('.session-library-pagination')?.textContent?.slice(0, 256),
        detailStatus: document.querySelector('#session-detail-status')?.textContent?.slice(0, 2048),
        detailEventCount: document.querySelectorAll('.session-detail-events article').length,
        detailPagination: document.querySelector('.session-detail-pagination')?.textContent?.slice(0, 256),
        resourceStatus: document.querySelector('#resource-status')?.textContent?.slice(0, 256),
      })), 2000, 'Synthetic UI diagnostics');
      await page.screenshot({ path: resolve(output, 'sessions-library-failure.png'), timeout: 2000 });
    } catch { evidence.diagnostic = 'unavailable'; }
  }
  await save(); throw error;
} finally {
  if (app.exitCode === null && app.signalCode === null) { app.kill(); await exitWithin(3000).catch(() => {}); }
  await browser?.close().catch(() => {});
}
