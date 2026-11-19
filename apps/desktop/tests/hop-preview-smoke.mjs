// C27 native proof: a hop preview through the real app IPC, contained worker and hop crate.
// Synthetic Claude archive in a Git project; no agent is started and nothing is sent.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-hop-smoke-'));
const data = resolve(fixture, 'data');
const archive = resolve(fixture, 'synthetic-claude');
const repo = resolve(fixture, 'repo');
await mkdir(repo, { recursive: true });
execFileSync('git', ['init', '-q', '-b', 'main', repo]);
await writeFile(resolve(repo, 'AGENTS.md'), '# agents\n');
await writeFile(resolve(repo, 'notes.txt'), 'draft\n');
const project = resolve(archive, 'projects/fixture-project');
await mkdir(project, { recursive: true });
const id = 'hopsession-01';
const KEY = 'ghp_0123456789abcdefABCDEF0123456789abcd';
const THINK = 'THINKSECRETSENTINEL';
const records = [];
let parent = null;
for (let n = 0; n < 3; n++) {
  const s = String(n).padStart(2, '0');
  records.push({ type: 'user', uuid: `u-${s}`, parentUuid: parent, sessionId: id, cwd: repo, timestamp: `2026-11-09T00:00:${s}Z`,
    message: { role: 'user', content: n === 0 ? `refactor the parser, token ${KEY}` : n === 2 ? '/compact then finish the lexer' : 'continue' } });
  records.push({ type: 'assistant', uuid: `a-${s}`, parentUuid: `u-${s}`, sessionId: id,
    message: { id: `m-${s}`, role: 'assistant', model: 'fixture-model', stop_reason: 'end_turn', usage: { input_tokens: 5, cache_read_input_tokens: 95, output_tokens: 2 },
      content: [{ type: 'thinking', thinking: THINK, signature: 'opaque' }, { type: 'text', text: `step ${n} done` }] } });
  parent = `a-${s}`;
}
const source = Buffer.from(`${records.map(r => JSON.stringify(r)).join('\n')}\n`);
await writeFile(resolve(project, `${id}.jsonl`), source);
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = { check: 'C27-hop-preview', status: 'running', checks: {}, scope: 'one synthetic Claude session in a Git repo; preview to Codex; nothing sent' };
const save = () => writeFile(resolve(output, 'hop-preview.json'), `${JSON.stringify(evidence, null, 2)}\n`);
const reservation = createServer();
await new Promise((ok, no) => { reservation.once('error', no); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, ok); });
const port = reservation.address().port;
await new Promise((ok, no) => reservation.close(error => error ? no(error) : ok()));
const app = spawn(resolve(workspace, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: workspace, windowsHide: true, stdio: 'ignore', env: { ...process.env, SHINATAMA_HOME: data,
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let browser, page;
async function reply(command, args = {}) {
  return page.evaluate(async ({ command, args }) => {
    try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
    catch (issue) { return { ok: false, issue }; }
  }, { command, args });
}
async function ipc(command, args = {}, attempts = 1) {
  for (let attempt = 1; ; attempt++) {
    const result = await reply(command, args);
    if (result.ok) return result.value;
    if (attempt >= attempts) throw new Error(`${command} refused (${JSON.stringify(result.issue)}).`);
    await delay(500);
  }
}
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
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
  await expect(page.locator('#resource-status')).toContainText('Resources: available', { timeout: 30000 });
  const root = await ipc('session_archive_add', { request: { harness: 'claude_code', path: archive } });
  const selector = { storeRootId: root.storeRootId, configurationRevision: root.configurationRevision };
  const detail = await ipc('session_detail', { request: { root: selector, nativeId: id, cursor: null, limit: 1 } }, 20);
  expect(detail.issue).toBeNull();
  const fingerprint = detail.page.fingerprint;
  evidence.checks.detailRead = true;
  expect(detail.page.ledger.requests).toBe(3); expect(detail.page.ledger.turns).toBe(3);
  expect(detail.page.ledger.hitRatePermille).toBe(950); expect(detail.page.ledger.total.output).toBe(6);
  evidence.checks.tokenLedgerFromRecordedUsage = true;

  const same = await ipc('hop_preview', { request: { root: selector, nativeId: id, fingerprint, destination: 'claude_code' } }, 20);
  expect(same.state).toBe('refused'); expect(same.issue).toBe('same_agent');
  evidence.checks.sameAgentRefused = true;

  const outcome = await ipc('hop_preview', { request: { root: selector, nativeId: id, fingerprint, destination: 'codex' } }, 20);
  expect(outcome.state).toBe('ready');
  const preview = outcome.preview;
  expect(preview.seed.startsWith(`[shinatama handoff v1] nonce=${preview.nonce}\n`)).toBe(true);
  expect(preview.seed).not.toContain(KEY); expect(preview.seed).not.toContain(THINK);
  expect(preview.seed).toContain('[redacted:github_token]');
  expect(preview.seed).toContain('[user] /compact then finish the lexer');
  expect(preview.seed.split(String.fromCharCode(10)).some(line => /^[/!@#-]/.test(line))).toBe(false);
  expect(preview.seed).toContain('Working tree: 0 uncommitted, 2 untracked');
  expect(preview.seed).toContain('You load AGENTS.md yourself');
  expect(preview.redactions.github_token).toBeGreaterThanOrEqual(1);
  expect(preview.channel).toBe('argv_prompt');
  expect(Buffer.byteLength(preview.bootstrap)).toBeLessThanOrEqual(2048);
  evidence.checks.seedRedactedEscapedAndHeadered = true;

  expect(preview.files_in_repository).toBe(true);
  expect(preview.seed_path.toLowerCase()).toContain('\\.git\\shinatama\\handoffs\\');
  const seedBytes = await readFile(preview.seed_path);
  expect(sha(seedBytes)).toBe(preview.seed_sha256);
  expect(seedBytes.toString('utf8')).toBe(preview.seed);
  const full = await readFile(preview.full_path, 'utf8');
  expect(full).not.toContain(KEY); expect(full).not.toContain(THINK);
  expect(sha(Buffer.from(preview.bootstrap))).toBe(preview.bootstrap_sha256);
  const acl = execFileSync('icacls', [preview.seed_path], { encoding: 'utf8' });
  expect(acl).not.toMatch(/Everyone|BUILTIN\\Users|Authenticated Users/i);
  evidence.checks.privateFilesMatchPreviewBytes = true;

  const tree = (await readdir(repo)).sort();
  expect(tree).toEqual(['.git', 'AGENTS.md', 'notes.txt']);
  const status = execFileSync('git', ['-C', repo, 'status', '--porcelain'], { encoding: 'utf8' });
  expect(status.split('\n').filter(Boolean).sort()).toEqual(['?? AGENTS.md', '?? notes.txt']);
  expect(await readFile(resolve(project, `${id}.jsonl`))).toEqual(source);
  evidence.checks.nothingInWorkingTreeAndSourceUnchanged = true;

  // C30: pairs without a qualified launch or reader are refused before anything is prepared.
  for (const destination of ['hermes', 'deepseek']) {
    const refused = await ipc('hop_preview', { request: { root: selector, nativeId: id, fingerprint, destination } }, 20);
    expect(refused.state).toBe('refused'); expect(refused.issue).toBe('pair_unsupported');
  }
  const unknownHop = await reply('hop_deliver', { hopId: 'f'.repeat(32) });
  expect(unknownHop.ok).toBe(false);
  expect(await readFile(resolve(project, `${id}.jsonl`))).toEqual(source);
  evidence.checks.unsupportedPairsRefusedAndSourceUnchanged = true;
  evidence.status = 'pass';
} catch (error) {
  evidence.status = 'fail'; evidence.error = String(error?.message ?? error).slice(0, 2000);
  throw error;
} finally {
  await save();
  await browser?.close().catch(() => {});
  app.kill();
  for (let i = 0; i < 40 && app.exitCode === null; i++) await delay(100);
  await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }).catch(() => {});
}
console.log(`hop-preview smoke: ${evidence.status}`);
