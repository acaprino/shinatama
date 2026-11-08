// C24 native proof: incremental persistent search over a synthetic Claude archive.
// All content is synthetic; no agent is started. Run after rebuilding desktop + session helper.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-search-smoke-'));
const data = resolve(fixture, 'data');
const archive = resolve(fixture, 'synthetic-claude');
const project = resolve(archive, 'projects/fixture-project');
await mkdir(project, { recursive: true });
const CWD = 'C:\\synthetic-search-project';
const words = ['zebracorn', 'wombatine', 'axolotlish', 'pangolinic', 'narwhalish', 'okapiform'];
const secrets = { tool: 'TOOLSECRETSENTINEL', thinking: 'THINKSECRETSENTINEL' };
const sessionId = index => `searchsession-${String(index).padStart(2, '0')}`;
function transcript(index, extra = []) {
  const id = sessionId(index); const records = []; let parent = null;
  const turn = (n, userText, assistantText) => {
    const suffix = String(n).padStart(2, '0');
    records.push({ type: 'user', uuid: `u-${suffix}`, parentUuid: parent, sessionId: id, cwd: CWD,
      timestamp: `2026-11-0${(index % 5) + 1}T00:00:${suffix}Z`, message: { role: 'user', content: userText } });
    records.push({ type: 'assistant', uuid: `a-${suffix}`, parentUuid: `u-${suffix}`, sessionId: id,
      message: { id: `m-${suffix}`, role: 'assistant', model: 'fixture-model', stop_reason: 'end_turn', content: [
        { type: 'text', text: assistantText },
        ...(n === 0 ? [{ type: 'thinking', thinking: secrets.thinking, signature: 'opaque' },
          { type: 'tool_use', id: 'tool-1', name: 'Read', input: { note: secrets.tool } }] : [])] } });
    parent = `a-${suffix}`;
  };
  turn(0, `please discuss the ${words[index]} design`, `The ${words[index]} approach is fine.`);
  turn(1, 'any follow up?', 'No, that is all for now.');
  extra.forEach(([user, assistant], n) => turn(n + 2, user, assistant));
  return `${records.map(record => JSON.stringify(record)).join('\n')}\n`;
}
const originals = new Map();
for (let index = 0; index < words.length; index++) {
  const name = `${sessionId(index)}.jsonl`; const bytes = Buffer.from(transcript(index));
  originals.set(name, bytes); await writeFile(resolve(project, name), bytes);
}
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = { check: 'C24-incremental-session-search', status: 'running', checks: {},
  scope: `${words.length} synthetic Claude sessions; index, filters, incremental, source-deleted, forget, disable; no agent start` };
const save = () => writeFile(resolve(output, 'session-search.json'), `${JSON.stringify(evidence, null, 2)}\n`);
await save();
const reservation = createServer();
await new Promise((ok, no) => { reservation.once('error', no); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, ok); });
const port = reservation.address().port;
await new Promise((ok, no) => reservation.close(error => error ? no(error) : ok()));
const app = spawn(resolve(workspace, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: workspace, windowsHide: true, stdio: 'ignore', env: { ...process.env, SHINATAMA_HOME: data,
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let browser, page; const errors = [];
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();
async function reply(command, args = {}) {
  return page.evaluate(async ({ command, args }) => {
    try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
    catch (issue) { return { ok: false, issue }; }
  }, { command, args });
}
async function ipc(command, args = {}) {
  const result = await reply(command, args);
  if (!result.ok) throw new Error(`${command} refused (${JSON.stringify(result.issue)}).`);
  return result.value;
}
const search = (request) => ipc('session_search', { request });
async function pass(label) {
  // Waits out transient admission refusals; one explicit pass per call.
  for (let attempt = 0; attempt < 30; attempt++) {
    const started = await reply('session_index_start');
    if (started.ok) break;
    if (attempt === 29) throw new Error(`${label}: index start refused ${JSON.stringify(started.issue)}`);
    await delay(500);
  }
  for (const deadline = Date.now() + 240000; Date.now() < deadline; await delay(500)) {
    const state = await ipc('session_index_status');
    if (state.progress.phase === 'running') continue;
    if (state.progress.phase !== 'done') throw new Error(`${label}: index ${state.progress.phase} ${state.progress.message ?? ''}`);
    return state;
  }
  throw new Error(`${label}: index did not finish in time`);
}
async function sourceHash(names) {
  const hash = createHash('sha256');
  for (const name of names) { hash.update(name); hash.update(await readFile(resolve(project, name))); }
  return hash.digest('hex');
}
try {
  for (const deadline = Date.now() + 30000; Date.now() < deadline;) {
    if (app.exitCode !== null) throw new Error('Owned app exited during startup.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; } catch { await delay(250); }
  }
  if (!browser) throw new Error('WebView startup exceeded 30 seconds.');
  const context = browser.contexts()[0];
  for (const deadline = Date.now() + 15000; !page && Date.now() < deadline; await delay(100)) {
    page = context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'));
  }
  if (!page) throw new Error('Main WebView not found.');
  page.on('pageerror', error => errors.push(error.message.slice(0, 512)));
  await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 30000 });
  expect(pathKey((await ipc('storage_location')).directory)).toBe(pathKey(data));
  await expect(page.locator('#resource-status')).toContainText('Resources: available', { timeout: 30000 });

  const root = await ipc('session_archive_add', { request: { harness: 'claude_code', path: archive } });
  expect(root.available).toBe(true);
  const before = await ipc('session_index_status');
  expect(before.index.documents).toBe(0); expect(before.index.enabled).toBe(true);
  expect(await search({ query: words[0] })).toEqual([]);
  evidence.checks.emptyBeforeIndexing = true;

  const first = await pass('first');
  expect(first.progress.indexed).toBe(words.length); expect(first.progress.skipped).toBe(0); expect(first.progress.failed).toBe(0);
  expect(first.index.documents).toBe(words.length);
  evidence.checks.allSessionsIndexed = true;

  const hits = await search({ query: words[0] });
  expect(hits).toHaveLength(1);
  expect(hits[0].nativeId).toBe(sessionId(0)); expect(hits[0].harness).toBe('claude_code');
  expect(hits[0].sourceDeleted).toBe(false); expect(hits[0].configurationRevision).toBe(root.configurationRevision);
  expect(hits[0].snippet).toContain(`«${words[0]}»`);
  expect(await search({ query: words[0].slice(0, 5) })).toHaveLength(1); // prefix on the last word
  expect(await search({ query: `${words[0]} OR ${words[1]}` })).toEqual([]); // operators are literal
  evidence.checks.matchAndSnippetAndLiteralQuery = true;

  expect(await search({ query: secrets.tool })).toEqual([]);
  expect(await search({ query: secrets.thinking })).toEqual([]);
  evidence.checks.toolAndReasoningTextNeverIndexed = true;

  expect(await search({ query: words[0], harness: 'codex' })).toEqual([]);
  expect(await search({ query: words[0], harness: 'claude_code' })).toHaveLength(1);
  expect(await search({ query: words[0], project: 'C:\\synthetic-search' })).toHaveLength(1);
  expect(await search({ query: words[0], project: 'D:\\elsewhere' })).toEqual([]);
  expect(await search({ query: words[0], from: '2030-01-01' })).toEqual([]);
  expect(await search({ query: words[0], to: '2020-01-01' })).toEqual([]);
  expect((await reply('session_search', { request: { query: words[0], from: 'not-a-date' } })).ok).toBe(false);
  evidence.checks.filtersByAgentProjectAndDate = true;

  const second = await pass('second');
  expect(second.progress.indexed).toBe(0); expect(second.progress.skipped).toBe(words.length);
  evidence.checks.unchangedSessionsNotReread = true;

  const grown = `${sessionId(1)}.jsonl`;
  await appendFile(resolve(project, grown), '');
  const bigger = Buffer.from(transcript(1, [['tell me about the quokkaform', 'The quokkaform is a marsupial.']]));
  await writeFile(resolve(project, grown), bigger); originals.set(grown, bigger);
  const third = await pass('third');
  expect(third.progress.indexed).toBe(1); expect(third.progress.skipped).toBe(words.length - 1);
  expect(await search({ query: 'quokkaform' })).toHaveLength(1);
  expect(await search({ query: words[1] })).toHaveLength(1);
  evidence.checks.changedSessionOnlyReindexed = true;

  const gone = `${sessionId(2)}.jsonl`;
  await rm(resolve(project, gone)); originals.delete(gone);
  const fourth = await pass('fourth');
  expect(fourth.progress.markedSourceDeleted).toBe(1); expect(fourth.index.sourceDeleted).toBe(1);
  const orphan = await search({ query: words[2] });
  expect(orphan).toHaveLength(1); expect(orphan[0].sourceDeleted).toBe(true);
  evidence.checks.deletedSourceStaysSearchableAndFlagged = true;

  // The visible UI: search, highlight, open detail.
  await page.locator('#session-search').click();
  const dialog = page.locator('#session-search-dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('#session-search-index-info')).toContainText(`${words.length} sessions indexed`);
  await dialog.locator('#session-search-query').fill(words[3]);
  await dialog.getByRole('button', { name: 'Search', exact: true }).click();
  const hitRows = dialog.locator('.session-search-hit');
  await expect(hitRows).toHaveCount(1, { timeout: 10000 });
  await expect(hitRows.first().locator('mark')).toHaveText(words[3]);
  await dialog.locator('#session-search-query').fill(words[2]);
  await dialog.getByRole('button', { name: 'Search', exact: true }).click();
  await expect(dialog.locator('.session-search-hit').first()).toContainText('Source deleted');
  await page.screenshot({ path: resolve(output, 'session-search.png'), timeout: 3000 });
  await dialog.locator('#session-search-query').fill(words[3]);
  await dialog.getByRole('button', { name: 'Search', exact: true }).click();
  await expect(hitRows).toHaveCount(1, { timeout: 10000 });
  await hitRows.first().locator('.session-search-open').click();
  await expect(page.locator('#session-detail-dialog')).toBeVisible({ timeout: 15000 });
  await expect(dialog).toBeHidden();
  await page.locator('#session-detail-dialog').getByRole('button', { name: /Close/i }).first().click();
  evidence.checks.uiSearchHighlightDeletedBadgeAndOpenDetail = true;

  // Controls: disable blocks indexing; forget removes only indexed text.
  await ipc('session_index_enable', { enabled: false });
  expect((await reply('session_index_start')).ok).toBe(false);
  await ipc('session_index_enable', { enabled: true });
  const sourceBefore = await sourceHash([...originals.keys()].sort());
  expect(await ipc('session_index_forget', { storeRootId: null })).toBe(words.length);
  expect(await search({ query: words[0] })).toEqual([]);
  expect((await ipc('session_index_status')).index.documents).toBe(0);
  expect(await sourceHash([...originals.keys()].sort())).toBe(sourceBefore);
  expect((await ipc('session_roots'))).toHaveLength(1);
  evidence.checks.disableAndForgetTouchOnlyTheIndex = true;

  const rebuilt = await pass('rebuild');
  expect(rebuilt.progress.indexed).toBe(words.length - 1);
  expect(await search({ query: words[0] })).toHaveLength(1);
  evidence.checks.rebuildFromScratch = true;

  expect(await ipc('terminal_list')).toEqual([]);
  expect(errors).toEqual([]);
  const closing = new Promise(ok => app.once('exit', ok));
  await ipc('workspace_allow_close').catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click().catch(() => {});
  await Promise.race([closing, delay(15000)]);
  evidence.status = 'passed'; await save();
  console.log('PASS: persistent search: index, filters, incremental skip/reindex, deleted source flagged, tool/reasoning text excluded, UI, disable/forget/rebuild.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = String(error.message).slice(0, 4096);
  if (page && !page.isClosed()) {
    evidence.diagnostic = await page.evaluate(() => ({
      status: document.querySelector('#session-search-status')?.textContent,
      index: document.querySelector('#session-search-index-info')?.textContent,
      notice: document.querySelector('#notice')?.textContent })).catch(() => 'unavailable');
    await page.screenshot({ path: resolve(output, 'session-search-failure.png'), timeout: 3000 }).catch(() => {});
  }
  await save(); throw error;
} finally {
  if (app.exitCode === null) app.kill();
  await browser?.close().catch(() => {});
}
