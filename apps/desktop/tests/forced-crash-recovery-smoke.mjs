// Minimal forced-crash proof: one real crash before any cleanup receipt, one
// restart, one Recover refusal, database intact. Every other recovery path is
// already covered by thread-recovery-smoke.mjs (synthetic) and the resume/race
// smokes. Real app/ConPTY/UI and real Codex; synthetic loopback key only.
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer, request as httpRequest } from 'node:http';
import { createConnection, createServer as createPortReservation } from 'node:net';
import { mkdir, mkdtemp, readFile, stat, writeFile } from 'node:fs/promises';
import { freemem, tmpdir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('windows-required');
const workspace = process.cwd();
const executable = process.env.SHINATAMA_SMOKE_APP_EXE || resolve(workspace, 'target/debug/shinatama-desktop.exe');
if (!isAbsolute(executable)) throw new Error('absolute-app-executable-required');
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-forced-crash-'));
const data = resolve(fixture, 'data'); const projectPath = resolve(fixture, 'synthetic-project');
for (const name of ['synthetic-project', 'home', 'roaming', 'local', 'tmp']) await mkdir(resolve(fixture, name));
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const PROMPT = 'SHINATAMA_UI_CRASH_PROMPT_A';
const DUMMY_KEY = 'sk-shinatama-synthetic-codex-no-provider-key';
const evidence = { check: 'forced-crash-recovery', status: 'running', stage: 'prepare', checks: {},
  scope: 'Minimal real crash: one TerminateProcess of the app before any cleanup receipt, one restart, one ADR-0051 certificate recovery that closes the activation once and releases the dead host ticket. Synthetic loopback credential, guarded external routes, no completed model turn.' };
const save = () => writeFile(resolve(output, 'forced-crash-recovery.json'), `${JSON.stringify(evidence, null, 2)}\n`);
const stage = async name => { evidence.stage = name; await save(); };
await save();
const fail = code => { throw Object.assign(new Error(code), { proofCode: code }); };
const require = (condition, code) => { if (!condition) fail(code); };
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
const inside = (child, parent) => pathKey(child).startsWith(`${pathKey(parent)}\\`);
let aborted = false; const began = Date.now();
function checkDeadline() { if (aborted || Date.now() - began > 300000) fail('overall-proof-deadline'); }
async function poll(check, milliseconds, code) {
  const deadline = Date.now() + milliseconds;
  while (Date.now() < deadline) { checkDeadline(); if (await check()) return; await delay(100); }
  fail(code);
}
function processDead(pid) { try { process.kill(pid, 0); return false; } catch { return true; } }
function dbQuery(sql, ...params) {
  const db = new DatabaseSync(resolve(data, 'shinatama.db'), { readOnly: true });
  try { db.exec('PRAGMA busy_timeout=200'); return db.prepare(sql).all(...params); }
  finally { db.close(); }
}
const system = process.env.SystemRoot || process.env.SYSTEMROOT;
require(system && process.env.USERPROFILE, 'required-windows-discovery-paths');
let codexDirectory = null;
for (const entry of (process.env.PATH || '').split(';')) {
  if (!entry) continue;
  try { if ((await stat(join(entry, 'codex.exe'))).isFile()) { codexDirectory = resolve(entry); break; } } catch { /* keep searching */ }
}
require(!!codexDirectory, 'installed-codex-required');

// Guarded provider: external routes are refused; any provider hit fails.
let providerHits = 0; const sockets = new Set();
const provider = createServer((request, response) => { request.on('error', () => {}); response.on('error', () => {}); providerHits++; request.destroy(); });
provider.maxConnections = 8;
provider.on('connection', socket => { sockets.add(socket); socket.setTimeout(5000, () => socket.destroy()); socket.on('error', () => {}); socket.once('close', () => sockets.delete(socket)); });
const reservation = createPortReservation();
let providerPort, guardPort, port;
const guardCounts = { allowed: 0, denied: 0, overflow: 0 };
function trackSocket(socket) { sockets.add(socket); socket.setTimeout(5000, () => socket.destroy()); socket.on('error', () => {}); socket.once('close', () => sockets.delete(socket)); }
const guard = createServer({ maxHeaderSize: 16384 }, (request, response) => {
  request.on('error', () => {}); response.on('error', () => {});
  let target;
  try { if (request.url.length <= 4096) target = new URL(request.url); } catch { /* refuse malformed */ }
  if (request.socket.remoteAddress !== '127.0.0.1' || target?.protocol !== 'http:'
      || !request.url.startsWith(`http://127.0.0.1:${providerPort}/`) || target.host !== `127.0.0.1:${providerPort}`) {
    guardCounts.denied++; response.writeHead(403, { connection: 'close' }); response.end(); return;
  }
  if (++guardCounts.allowed > 64) { guardCounts.overflow++; request.destroy(); return; }
  let body = Buffer.alloc(0);
  request.on('data', chunk => { if (body.length + chunk.length > 512 * 1024) { guardCounts.overflow++; request.destroy(); } else body = Buffer.concat([body, chunk]); });
  request.on('end', () => {
    const headers = { ...request.headers, host: `127.0.0.1:${providerPort}`, connection: 'close' };
    delete headers['proxy-authorization']; delete headers['proxy-connection'];
    const upstream = httpRequest({ hostname: '127.0.0.1', port: providerPort, path: target.pathname + target.search, method: request.method, headers, agent: false }, incoming => { incoming.on('error', () => response.destroy()); response.writeHead(incoming.statusCode, incoming.headers); incoming.pipe(response); });
    upstream.on('socket', trackSocket); upstream.on('error', () => response.destroy()); response.once('close', () => upstream.destroy()); upstream.end(body);
  });
});
guard.on('connect', (request, client, head) => {
  if (client.remoteAddress !== '127.0.0.1' || request.url !== `127.0.0.1:${providerPort}` || head.length > 16384) {
    guardCounts.denied++; client.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n'); return;
  }
  if (++guardCounts.allowed > 64) { guardCounts.overflow++; client.destroy(); return; }
  const upstream = createConnection({ host: '127.0.0.1', port: providerPort }); trackSocket(upstream);
  let sent = head.length, received = 0;
  client.on('data', chunk => { if ((sent += chunk.length) > 1024 * 1024) { guardCounts.overflow++; client.destroy(); upstream.destroy(); } });
  upstream.on('data', chunk => { if ((received += chunk.length) > 65536) { guardCounts.overflow++; client.destroy(); upstream.destroy(); } });
  client.once('close', () => upstream.destroy()); upstream.once('close', () => client.destroy());
  upstream.once('connect', () => { client.write('HTTP/1.1 200 Connection Established\r\n\r\n'); if (head.length) upstream.write(head); client.pipe(upstream); upstream.pipe(client); });
});
guard.on('connection', trackSocket); guard.on('clientError', (_error, socket) => socket.destroy());
async function prepareGuardedConfig() {
  const root = resolve(data, 'sidecar');
  const configuration = {
    host: '127.0.0.1', port: 8787, log: true, debug: false, logFile: resolve(root, 'state/bili.log'),
    autoUpdate: false, autoRestartOnUpdate: false, advisoryCheck: false, releaseNotesCheck: false,
    storage: { unified: 'full', backend: 'sqlite', sqliteAutoDownload: false }, promptRewrite: { mode: 'auto' },
    compress: { tiers: true, absorb: { enabled: true, minToolTokens: 4000 }, ccr: { enabled: true, minToolTokens: 4000 },
      adaptivePressure: { enabled: true, maxTokens: 160000, targetRatio: 0.75, growthWindow: 20 },
      outputSteering: { enabled: true, verbosityLevel: 2, effortRouting: false } },
    upstreamProxyMode: 'manual', upstreamProxy: `http://127.0.0.1:${guardPort}`,
  };
  const script = String.raw`
$ErrorActionPreference = 'Stop'
try {
  $inputText = [Console]::In.ReadToEnd()
  $value = ConvertFrom-Json -InputObject $inputText
  $root = [IO.Path]::GetFullPath($value.fixture)
  if ($root -ne $value.fixture -or [IO.Path]::GetFileName($root) -notmatch '^shinatama-forced-crash-[A-Za-z0-9]+$') { throw 'fresh-fixture-only' }
  $info = [IO.DirectoryInfo]::new($root)
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
  if (!$info.Exists -or $info.GetAccessControl().GetOwner([Security.Principal.SecurityIdentifier]) -ne $sid) { throw 'ordinary-owned-fixture-only' }
  $sddl = 'O:' + $sid.Value + 'D:P(A;OICI;FA;;;' + $sid.Value + ')'
  $sections = [Security.AccessControl.AccessControlSections]::Owner -bor [Security.AccessControl.AccessControlSections]::Access
  $directoryAcl = [Security.AccessControl.DirectorySecurity]::new()
  $directoryAcl.SetSecurityDescriptorSddlForm($sddl, $sections)
  foreach ($relative in @('data', 'data\sidecar', 'data\sidecar\config')) {
    $path = [IO.Path]::Combine($root, $relative)
    [void][IO.Directory]::CreateDirectory($path, $directoryAcl)
  }
  $path = [IO.Path]::Combine($root, 'data\sidecar\config\billion-context.json')
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes($value.configuration)
  $file = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try { $file.Write($bytes, 0, $bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
  $fileAcl = [Security.AccessControl.FileSecurity]::new()
  $fileAcl.SetSecurityDescriptorSddlForm($sddl, $sections)
  [IO.File]::SetAccessControl($path, $fileAcl)
  [Console]::Out.Write('private-config-ready')
} catch { [Console]::Error.Write('private-config-preparation-failed'); exit 1 }
`;
  const { spawn: spawnChild } = await import('node:child_process');
  const child = spawnChild(resolve(system, 'System32/WindowsPowerShell/v1.0/powershell.exe'),
    ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true,
      env: { SystemRoot: system, WINDIR: system, PATH: resolve(system, 'System32'), TEMP: resolve(fixture, 'tmp'), TMP: resolve(fixture, 'tmp') },
      stdio: ['pipe', 'pipe', 'pipe'] });
  let stdout = ''; child.stdout.on('data', chunk => { stdout += chunk; }); child.stderr.on('data', () => {});
  child.stdin.end(JSON.stringify({ fixture, configuration: `${JSON.stringify(configuration, null, 2)}\n` }));
  const code = await new Promise(accept => child.once('close', accept));
  require(code === 0 && stdout === 'private-config-ready', 'private-config-preparation-failed');
  const path = resolve(root, 'config/billion-context.json');
  require(inside(path, fixture) && (await stat(path)).size <= 16384, 'private-config-boundary');
  evidence.checks.guardAndOwnerOnlyConfigPrepared = true;
}
try {
  await new Promise((accept, reject) => { provider.once('error', reject); provider.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
  providerPort = provider.address().port;
  await new Promise((accept, reject) => { guard.once('error', reject); guard.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
  guardPort = guard.address().port;
  await stage('guard-config'); await prepareGuardedConfig();
  await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
  port = reservation.address().port;
  await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
} catch (error) {
  for (const socket of sockets) socket.destroy();
  reservation.close(); guard.close(); provider.close();
  evidence.status = 'failed'; evidence.failure = error.proofCode || 'guarded-fixture-preparation-failed'; await save();
  throw new Error(evidence.failure);
}

let app = null, browser = null, page = null, owned = false, frameClosed = false, pageErrors = 0, activeTerminal = null;
function environment() {
  const values = {};
  for (const name of ['SystemRoot', 'WINDIR', 'SystemDrive', 'COMSPEC', 'PATHEXT', 'PROCESSOR_ARCHITECTURE',
    'NUMBER_OF_PROCESSORS', 'ProgramFiles', 'ProgramFiles(x86)', 'ProgramW6432', 'ProgramData', 'ALLUSERSPROFILE',
    'OS', 'USERNAME', 'USERDOMAIN', 'USERPROFILE', 'HOMEDRIVE', 'HOMEPATH', 'APPDATA', 'LOCALAPPDATA']) {
    const key = Object.keys(process.env).find(key => key.toLowerCase() === name.toLowerCase());
    if (key) values[name] = process.env[key];
  }
  Object.assign(values, { SystemRoot: system, WINDIR: system, PATH: [codexDirectory, resolve(system, 'System32')].join(';'),
    HOME: values.USERPROFILE, TEMP: resolve(fixture, 'tmp'), TMP: resolve(fixture, 'tmp'), SHINATAMA_HOME: data,
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
    OPENAI_API_KEY: DUMMY_KEY });
  return values;
}
async function launchApp() {
  // Simple sanity wait on real host memory (no admission gates); the product
  // itself refuses under pressure and the test surfaces that as its failure.
  await poll(() => freemem() / 1073741824 >= 4, 120000, 'host-memory-insufficient');
  let spawnFailed = false;
  app = spawn(executable, [], { cwd: process.env.SHINATAMA_SMOKE_APP_EXE ? dirname(executable) : workspace, windowsHide: true, stdio: ['ignore', 'ignore', 'ignore'], env: environment() });
  app.once('error', () => { spawnFailed = true; });
  app.exited = () => new Promise(accept => app.once('exit', accept));
  await poll(async () => {
    require(!spawnFailed && app.exitCode === null, 'owned-app-startup-failed');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); return true; }
    catch { return false; }
  }, 30000, 'owned-webview-startup-deadline');
  await poll(async () => {
    page = browser.contexts().flatMap(context => context.pages()).find(candidate => {
      try { const url = new URL(candidate.url()); return url.hostname === 'tauri.localhost' && ['/', '/index.html'].includes(url.pathname); }
      catch { return false; }
    }); return !!page;
  }, 30000, 'main-workspace-webview-missing');
  page.setDefaultTimeout(25000); page.on('pageerror', () => { pageErrors++; });
  await poll(async () => (await page.locator('#runtime-status').textContent()).includes('Local runtime'), 15000, 'runtime-unavailable');
  if ((await ipc('sidecar_status')).phase !== 'ready') await ipc('sidecar_start').catch(() => {});
  await poll(async () => { const status = await ipc('sidecar_status'); return status.phase === 'ready' && status.healthy === true; }, 90000, 'sidecar-not-ready');
  require(pathKey((await ipc('storage_location')).directory) === pathKey(data), 'private-app-storage-mismatch');
  owned = true;
}
async function ipc(command, args = {}) {
  // Read-only metadata commands may answer busy/unavailable while the owner is
  // starting; retry only those, briefly, and surface everything else at once.
  const readOnly = ['terminal_list', 'global_workspace_load', 'storage_location', 'project_recent', 'sidecar_status', 'thread_list'].includes(command);
  const deadline = Date.now() + 5000;
  for (;;) {
    const result = await page.evaluate(async ({ command, args }) => {
      try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
      catch (error) { return { ok: false, code: typeof error?.code === 'string' ? error.code : 'refused' }; }
    }, { command, args });
    if (result.ok) return result.value;
    if (!(readOnly && ['busy', 'unavailable'].includes(result.code)) || Date.now() >= deadline) {
      throw Object.assign(new Error(`${command}:${result.code}`), { ipcCode: result.code, ipcCommand: command });
    }
    await delay(150);
  }
}
const sessions = () => ipc('terminal_list');
const pane = id => page.locator(`.wm-pane:has([data-view-id="${id}"])`);
const layout = async () => { const row = await ipc('global_workspace_load', { request: { workspaceId: 'main' } }); return row ? JSON.parse(row.layoutJson) : null; };
async function screen(id) { const value = await pane(id).locator('.xterm-accessibility-tree').innerText(); require(value.length <= 32768, 'bounded-visible-terminal'); return value; }
async function input(id, text, key) {
  activeTerminal = id;
  const field = pane(id).locator('.xterm-helper-textarea'); await field.focus();
  if (text) { require(text === PROMPT, 'fixed-synthetic-prompt-only'); await field.pressSequentially(text, { delay: 5 }); await poll(async () => (await screen(id)).includes(text), 5000, 'synthetic-prompt-not-visible-before-submit'); }
  if (key) await page.keyboard.press(key);
}
async function ready(id) {
  const handled = new Set();
  await poll(async () => {
    const text = await screen(id);
    require(await pane(id).locator('.exit-banner').count() === 0, 'native-process-exited-before-readiness');
    require(!/Review hooks|Trust all and continue|Continue without trusting/.test(text), 'hook-trust-review-screen-appeared');
    if (/Sign in with ChatGPT/.test(text) && /(Provide your own API key|Use an OpenAI API key)/.test(text) && !handled.has('auth')) { handled.add('auth'); await input(id, null, '3'); return false; }
    if (/Use your own OpenAI API key/.test(text) && !handled.has('key')) { handled.add('key'); if (!/Detected OPENAI_API_KEY environment variable/.test(text)) await input(id, DUMMY_KEY, null); await input(id, null, 'Enter'); return false; }
    if (/Trust this folder\?/.test(text) && !handled.has('trust')) { handled.add('trust'); await input(id, null, 'Enter'); return false; }
    return /Ask Codex to do anything/.test(text) && /for shortcuts/i.test(text);
  }, 60000, 'unrecognized-or-blocked-native-screen');
}
async function openSession() {
  await page.locator('#new-terminal').click(); const dialog = page.locator('#new-session-dialog');
  await poll(async () => await dialog.isVisible(), 15000, 'new-session-dialog-did-not-open');
  if (await dialog.locator('#new-session-project').isVisible()) await dialog.locator('#new-session-project').fill(projectPath);
  await dialog.locator('#new-session-harness').selectOption('codex');
  await dialog.getByRole('button', { name: 'Open', exact: true }).click();
  await poll(async () => {
    if (await dialog.locator('#new-session-error').isVisible()) { evidence.openError = (await dialog.locator('#new-session-error').innerText()).slice(0, 240); fail('open-session-inline-refusal'); }
    return !await dialog.isVisible();
  }, 120000, 'new-session-deadline');
  const list = await sessions(); require(list.length === 1, 'expected-terminal-count');
  const session = list[0]; activeTerminal = session.id;
  await poll(async () => !!(await layout())?.views.find(view => view.id === session.id)?.resource.threadId, 15000, 'saved-thread-view-binding');
  const threadId = (await layout()).views.find(view => view.id === session.id).resource.threadId;
  await ready(session.id);
  await input(session.id, PROMPT, 'Enter');
  await poll(async () => {
    const rows = dbQuery('SELECT instance_id,owner_pid,ended_at FROM activation WHERE thread_id=? AND ended_at IS NULL', threadId);
    return rows.length === 1 && rows[0].instance_id === session.id;
  }, 60000, 'native-binding-deadline');
  return { session, threadId };
}
async function closeFrame() {
  const closing = Promise.all([page.waitForEvent('close', { timeout: 15000 }), app.exited()]); void closing.catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click(); await closing; frameClosed = true;
}
let projectId = null;
try {
  // ---- App #1: one live session, then a real crash before any receipt ----
  await stage('app1-start'); await launchApp();
  await stage('session-binding'); const bound = await openSession();
  const cleanupKey = 'operational.native_session_cleanup.v1';
  const before = dbQuery('SELECT entry.value AS entry FROM setting journal,json_each(journal.value,\'$.entries\') entry WHERE journal.key=?', cleanupKey);
  require(!before.some(row => JSON.parse(row.entry).cleanup !== null), 'no-receipt-before-crash');
  projectId = (await ipc('project_recent')).find(item => pathKey(item.path) === pathKey(projectPath))?.id;
  require(!!projectId, 'saved-project');
  const agentPid = dbQuery('SELECT owner_pid FROM activation WHERE instance_id=?', bound.session.id)[0].owner_pid;
  await stage('crash-app1'); const crashedPid = app.pid;
  process.kill(crashedPid);
  await poll(async () => processDead(crashedPid) && processDead(agentPid), 15000, 'crashed-processes-still-alive');
  await browser?.close().catch(() => {}); browser = null; page = null; app = null; owned = false;
  const crashed = dbQuery('SELECT ended_at,revision FROM activation WHERE instance_id=?', bound.session.id)[0];
  require(crashed.ended_at === null, 'activation-kept-open-without-receipt');
  const ledgerSlotIds = () => JSON.parse(dbQuery('SELECT value FROM setting WHERE key=?', 'operational.resource_reservations.v1')[0]?.value ?? '{"slots":[]}').slots.map(slot => slot.id);
  const journalFor = () => dbQuery("SELECT entry.value AS entry FROM setting journal,json_each(journal.value,'$.entries') entry WHERE journal.key=?", cleanupKey)
    .map(row => JSON.parse(row.entry)).filter(entry => entry.owner.activation.instanceId === bound.session.id);
  const association = journalFor();
  require(association.length === 1 && association[0].cleanup === null, 'association-without-receipt-after-crash');
  const ticketId = association[0].owner.reservation.id;
  require(ledgerSlotIds().includes(ticketId), 'crash-ticket-still-reserved');
  evidence.checks.crashLeftActivationOpenAndTicketReserved = true;

  // ---- App #2: restart; ADR-0051 recovery closes it through the certificate ----
  await stage('app2-start'); await launchApp();
  require((await sessions()).length === 0, 'no-terminal-after-restart');
  const sidebar = page.locator('.thread-sidebar');
  await sidebar.locator('#thread-sidebar-project').selectOption(projectId);
  await sidebar.locator(`button.thread-sidebar-anchor[data-thread-id="${bound.threadId}"]`).click();
  const recover = sidebar.getByRole('button', { name: 'Recover session', exact: true });
  await poll(async () => await recover.isVisible(), 10000, 'recover-button-missing');
  await recover.click();
  const statusText = async () => (await sidebar.locator('.thread-sidebar-status').innerText().catch(() => '')).replace(/\s+/g, ' ').slice(0, 160);
  await poll(async () => { const text = await statusText(); evidence.recoverStatus = text; return /Session recovered/i.test(text) || /not confirmed|still open|unavailable/i.test(text); }, 30000, 'recover-status-missing');
  require(/Session recovered/i.test(evidence.recoverStatus), 'pre-receipt-recovery-refused');
  const after = dbQuery('SELECT ended_at,revision FROM activation WHERE instance_id=?', bound.session.id)[0];
  require(after.ended_at !== null && after.revision === crashed.revision + 1, 'activation-not-closed-exactly-once');
  require(journalFor().length === 0, 'association-not-consumed');
  require(!ledgerSlotIds().includes(ticketId), 'dead-host-ticket-not-released');
  await poll(async () => await sidebar.getByRole('button', { name: 'Resume', exact: true }).isVisible(), 10000, 'resume-not-offered-after-recovery');
  evidence.checks.preReceiptCrashRecoveredByCertificateWithTicketReleased = true;
  require(guardCounts.overflow === 0 && providerHits === 0 && pageErrors === 0, 'bounded-guard-and-ui-clean');
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only' };
  await stage('close'); await closeFrame(); require(app.exitCode === 0, 'owned-app-clean-exit');
  evidence.status = 'passed'; evidence.stage = 'complete'; await save();
  console.log('PASS: real crash before receipt left the activation open and the ticket reserved; restart recovered it through the ADR-0051 certificate, closed it once, released the ticket and offered Resume.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.proofCode || error.ipcCode || `proof-failed-at-${evidence.stage}`;
  evidence.errorMessage = String(error.message).split('\n')[0].slice(0, 300);
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only' };
  await save(); process.exitCode = 1; console.error(`FAIL: ${evidence.failure}`);
} finally {
  if (app && app.exitCode === null && app.signalCode === null) { try { process.kill(app.pid); } catch { /* gone */ } }
  await browser?.close().catch(() => {});
  for (const socket of sockets) socket.destroy();
  await new Promise(accept => guard.close(accept));
  await new Promise(accept => provider.close(accept));
  aborted = true;
}
