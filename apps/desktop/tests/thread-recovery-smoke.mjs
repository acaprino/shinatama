// Root schedules this metadata/UI proof. It does NOT simulate a real native crash.
// Only a newly created private database is seeded; no harness/account is started.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdir, mkdtemp, realpath, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { setTimeout as delay } from 'node:timers/promises';

const fail = code => { throw Object.assign(new Error(code), { proofCode: code }); };
const require = (value, code) => { if (!value) fail(code); };
require(process.platform === 'win32', 'windows-required');
const workspace = process.cwd();
const executable = process.env.SHINATAMA_SMOKE_APP_EXE || resolve(workspace, 'target/debug/shinatama-desktop.exe');
require(isAbsolute(executable), 'absolute-executable-required');
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-thread-recovery-'));
const data = resolve(fixture, 'data'), projectPath = resolve(fixture, 'project'), otherPath = resolve(fixture, 'other-project');
for (const path of [projectPath, otherPath, resolve(fixture, 'native-store'), resolve(fixture, 'tmp')]) await mkdir(path);
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = { check: 'thread-recovery-ui', status: 'running', stage: 'prepare', checks: {},
  scope: 'Actual Recover session UI and IPC with synthetic metadata in a fresh private DB. No harness, account, native crash or real job-cleanup qualification.' };
const save = () => writeFile(resolve(output, 'thread-recovery.json'), `${JSON.stringify(evidence, null, 2)}\n`);
const stage = async value => { evidence.stage = value; await save(); };
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
const inside = (child, parent) => pathKey(child).startsWith(`${pathKey(parent)}\\`);
const began = Date.now();
function within(promise, milliseconds, code) {
  let timer; const remaining = Math.min(milliseconds, 180000 - (Date.now() - began));
  if (remaining <= 0) fail('overall-deadline');
  return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Object.assign(new Error(code), { proofCode: code })), remaining); })]).finally(() => clearTimeout(timer));
}
const system = process.env.SystemRoot || process.env.SYSTEMROOT;
require(system, 'windows-system-root-required');
const appEnvironment = {};
for (const key of ['SystemRoot', 'SYSTEMROOT', 'WINDIR', 'COMSPEC', 'PATH', 'PATHEXT', 'USERPROFILE', 'HOMEDRIVE', 'HOMEPATH', 'APPDATA', 'LOCALAPPDATA', 'PROGRAMDATA', 'ProgramFiles', 'ProgramFiles(x86)', 'ProgramW6432', 'CommonProgramFiles', 'CommonProgramFiles(x86)', 'USERNAME', 'USERDOMAIN', 'OS', 'PROCESSOR_ARCHITECTURE', 'NUMBER_OF_PROCESSORS']) {
  if (process.env[key] !== undefined) appEnvironment[key] = process.env[key];
}
// Exact identity of this proof process only. Numeric PID, fixed .NET calls,
// bounded output, no shell profile, environment credentials or file discovery.
async function ownIdentity() {
  const helper = spawn(resolve(system, 'System32/WindowsPowerShell/v1.0/powershell.exe'), ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference='Stop'; $p=[Diagnostics.Process]::GetProcessById([int]$env:SHINATAMA_PROOF_PID); [Console]::Out.Write($p.StartTime.ToUniversalTime().ToFileTimeUtc().ToString() + ',' + $p.SessionId.ToString())"],
  { windowsHide: true, stdio: ['ignore', 'pipe', 'ignore'], env: { SystemRoot: system, WINDIR: system, SHINATAMA_PROOF_PID: String(process.pid) } });
  let text = ''; helper.stdout.on('data', chunk => { if ((text += chunk.toString('ascii')).length > 128) helper.kill(); });
  const done = new Promise((accept, reject) => { helper.once('error', reject); helper.once('exit', code => accept(code)); });
  try {
    require(await within(done, 10000, 'identity-helper-deadline') === 0 && /^\d{1,20},\d{1,10}$/.test(text), 'identity-helper-invalid');
    const [creation, session] = text.split(','); return { creation: BigInt(creation), session: Number(session) };
  } finally { if (helper.exitCode === null && helper.signalCode === null) helper.kill(); }
}
// Preserve exact FILETIME integers; JSON numbers parsed by Rust must not pass
// through JavaScript's lossy Number representation.
const exactJson = value => JSON.stringify(value, (_key, item) => typeof item === 'bigint' ? `__u64_${item}` : item).replace(/"__u64_(\d+)"/g, '$1');
const cleanupKey = 'operational.native_session_cleanup.v1', ledgerKey = 'operational.resource_reservations.v1';
let browser, page, app, database, owned = false, closed = false, pageErrors = 0;
async function reply(command, args = {}) {
  return within(page.evaluate(async ({ command, args }) => {
    try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
    catch (error) { return { ok: false, code: error?.code ?? 'unavailable' }; }
  }, { command, args }), 10000, 'ipc-deadline');
}
async function read(command, args = {}) {
  const end = Date.now() + 3000;
  do { const result = await reply(command, args); if (result.ok) return result.value;
    if (!['busy', 'unavailable'].includes(result.code)) fail('read-refused'); await delay(100);
  } while (Date.now() < end); fail('read-busy');
}
function exited() {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return within(new Promise(accept => app.once('exit', accept)), 10000, 'owned-exit-deadline');
}
async function closeFrame() {
  const closing = Promise.all([page.waitForEvent('close', { timeout: 10000 }), exited()]); void closing.catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click(); await closing; closed = true;
}
try {
  const identity = await ownIdentity();
  const reservation = createServer();
  await within(new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); }), 2000, 'port-deadline');
  const port = reservation.address().port; await new Promise(accept => reservation.close(accept));
  app = spawn(executable, [], { cwd: dirname(executable), windowsHide: true, stdio: 'ignore', env: { ...appEnvironment,
    SHINATAMA_HOME: data, TEMP: resolve(fixture, 'tmp'), TMP: resolve(fixture, 'tmp'), WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` } });
  let spawnFailed = false; app.once('error', () => { spawnFailed = true; });
  await stage('private-app');
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline && !browser) {
    require(!spawnFailed && app.exitCode === null, 'app-start-failed');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); } catch { await delay(200); }
  }
  require(browser, 'webview-deadline');
  await expect.poll(() => {
    page = browser.contexts()[0].pages().find(candidate => { try { const url = new URL(candidate.url()); return url.hostname === 'tauri.localhost' && ['/', '/index.html'].includes(url.pathname); } catch { return false; } });
    return !!page;
  }, { timeout: 10000 }).toBe(true);
  page.setDefaultTimeout(10000); page.on('pageerror', () => { pageErrors++; });
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const storage = await read('storage_location');
  require(pathKey(storage.directory) === pathKey(data) && !storage.read_only, 'private-storage-required');
  require(inside(await realpath(storage.database_path), await realpath(data)), 'database-boundary');
  owned = true; require((await read('terminal_list')).length === 0, 'empty-terminals-required');
  const project = await reply('project_select', { path: projectPath }); require(project.ok, 'project-select-failed');
  const other = await reply('project_select', { path: otherPath }); require(other.ok, 'other-project-select-failed');
  const projectId = project.value.id;
  database = new DatabaseSync(storage.database_path); database.exec('PRAGMA busy_timeout=1000; PRAGMA foreign_keys=ON');
  require(database.prepare('PRAGMA user_version').get().user_version === 5 && database.prepare('SELECT count(*) AS n FROM thread').get().n === 0, 'fresh-schema5-required');
  const budget = { memory_bytes: 67108864, processes: 1, cpu_basis_points: 100, queue_bytes: 4096, queue_items: 16 };
  const policy = { stamp: { version: 1, id: 'synthetic-recovery-ui' }, aggregate_ceiling: budget, per_slot_ceiling: budget, max_slots: 64, max_lease_ms: 10000 };
  const previousLedger = database.prepare('SELECT value FROM setting WHERE key=?').get(ledgerKey);
  const ledger = previousLedger ? JSON.parse(previousLedger.value) : { format_version: 1, revision: 100, policy, slots: [] };
  require(ledger.slots.length === 0, 'empty-ledger-required');
  const at = Date.now() - 10000; const cases = ['missing', 'alive', 'dead'].map((kind, index) => {
    const suffix = String.fromCharCode(97 + index).repeat(12), creation = kind === 'alive' ? identity.creation : 1n;
    const activation = { instanceId: `i_${suffix}`, pid: process.pid, createdAt100ns: creation };
    const ticket = { id: randomUUID(), job_key: randomUUID(), owner: { pid: process.pid, created_at_100ns: creation, windows_session_id: identity.session, nonce: randomUUID() },
      budget: ledger.policy.per_slot_ceiling, state: 'active', lease_expires_at_ms: at + 9000, revision: 10 + index };
    return { kind, thread: `t_${suffix}`, segment: `s_${suffix}`, native: randomUUID(), activation, ticket,
      entry: { owner: { activation, policy: ledger.policy.stamp, reservation: ticket, registered_at_ms: at }, cleanup: { released_ticket: ticket, released_at_ms: at + 1000 } } };
  });
  await stage('seed-synthetic-metadata');
  database.exec('BEGIN IMMEDIATE');
  try {
    database.prepare("INSERT INTO store_root VALUES('synthetic-recovery-root','claude_code','box','primary',?,'configured',1,'read_write',1,NULL)").run(resolve(fixture, 'native-store'));
    for (const item of cases) {
      database.prepare("INSERT INTO thread VALUES(?,?,'work',NULL,NULL,?,0,1,?,?)").run(item.thread, projectId, `Synthetic ${item.kind}`, at, at);
      database.prepare("INSERT INTO segment VALUES(?,?,1,'claude_code','synthetic-recovery-root',1,?,0,1,?)").run(item.segment, item.thread, item.native, at);
      database.prepare("INSERT INTO activation(instance_id,owner_pid,owner_created_at_100ns,thread_id,segment_id,store_root_id,store_revision,native_id,revision,started_at,ended_at) VALUES(?,?,?,?,?,'synthetic-recovery-root',1,?,1,?,NULL)").run(item.activation.instanceId, process.pid, item.activation.createdAt100ns, item.thread, item.segment, item.native, at);
    }
    database.prepare('INSERT INTO setting(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value').run(ledgerKey, exactJson(ledger));
    database.prepare('INSERT INTO setting(key,value) VALUES(?,?)').run(cleanupKey, exactJson({ format_version: 1, entries: cases.filter(item => item.kind !== 'missing').map(item => item.entry) }));
    database.exec('COMMIT');
  } catch { database.exec('ROLLBACK'); fail('synthetic-seed-failed'); }
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const sidebar = page.locator('.thread-sidebar'); await sidebar.locator('#thread-sidebar-project').selectOption(projectId);
  const recover = sidebar.getByRole('button', { name: 'Recover session', exact: true });
  const resume = sidebar.getByRole('button', { name: 'Resume', exact: true });
  const saved = item => database.prepare('SELECT revision,ended_at FROM activation WHERE instance_id=?').get(item.activation.instanceId);
  const request = item => ({ projectId, threadId: item.thread, expectedRevision: 1 });
  await stage('refusals');
  for (const item of cases.slice(0, 2)) {
    await sidebar.locator(`button.thread-sidebar-anchor[data-thread-id="${item.thread}"]`).click(); await expect(recover).toBeVisible(); await expect(resume).toBeHidden();
    await recover.click(); await expect(sidebar.locator('.thread-sidebar-status')).toContainText(item.kind === 'missing' ? 'shutdown was not confirmed' : 'still open');
    require(saved(item).ended_at === null && saved(item).revision === 1, 'refused-activation-changed');
  }
  evidence.checks.liveOwnerAndMissingReceiptRefusedWithActivationPreserved = true;
  const dead = cases[2];
  const wrongProject = await reply('thread_recover', { request: { ...request(dead), projectId: other.value.id } });
  const stale = await reply('thread_recover', { request: { ...request(dead), expectedRevision: 2 } });
  require(!wrongProject.ok && !stale.ok && saved(dead).ended_at === null, 'scope-or-revision-not-enforced');
  evidence.checks.projectAndRevisionScopeEnforced = true;
  await stage('recover');
  await sidebar.locator(`button.thread-sidebar-anchor[data-thread-id="${dead.thread}"]`).click(); await expect(recover).toBeVisible(); await recover.click();
  await expect(sidebar.locator('.thread-sidebar-status')).toContainText('Session recovered.'); await expect(recover).toBeHidden(); await expect(resume).toBeVisible(); await expect(resume).toBeFocused();
  require(saved(dead).ended_at !== null && saved(dead).revision === 2, 'exact-activation-not-finalized');
  const entries = JSON.parse(database.prepare('SELECT value FROM setting WHERE key=?').get(cleanupKey).value).entries;
  require(entries.length === 1 && entries[0].owner.activation.instanceId === cases[1].activation.instanceId, 'wrong-receipt-consumed');
  const rows = await read('thread_list', { request: { projectId, cursor: null, limit: 50 } });
  require(rows.rows.length === 3 && rows.rows.find(row => row.id === dead.thread)?.status === 'inactive', 'thread-refresh-failed');
  const repeated = await reply('thread_recover', { request: { ...request(dead), expectedRevision: 2 } }); require(!repeated.ok && saved(dead).revision === 2, 'recovery-repeated');
  require((await read('terminal_list')).length === 0 && JSON.parse(database.prepare('SELECT value FROM setting WHERE key=?').get(ledgerKey).value).slots.length === 0, 'unexpected-native-launch');
  evidence.checks.recoverClosesOnceConsumesExactReceiptAndShowsResumeWithoutLaunching = true;
  require(pageErrors === 0, 'webview-script-error'); database.close(); database = null;
  await closeFrame(); require(app.exitCode === 0, 'owned-app-exit-code'); evidence.checks.ownedFrameExit = true;
  evidence.status = 'passed'; evidence.stage = 'complete'; await save(); console.log('PASS: synthetic Recover session UI, refusals, exact finalization and no harness launch.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.proofCode || 'ui-or-assertion-failed'; await save();
  throw new Error(`thread-recovery-proof:${evidence.failure}`);
} finally {
  database?.close();
  if (owned && page && !page.isClosed() && !closed) await closeFrame().catch(() => {});
  if (app && app.exitCode === null && app.signalCode === null) { app.kill(); await exited().catch(() => {}); }
  await browser?.close().catch(() => {});
}
