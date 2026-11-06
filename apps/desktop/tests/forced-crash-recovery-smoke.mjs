// Root schedules this native proof. Real app/ConPTY/UI and real Codex 0.166.0;
// synthetic loopback credentials only, external providers refused by the guard.
// Forced crashes are real process kills (TerminateProcess) of this proof's own
// app instances and agent groups inside this fixture. Recovery must stay exact:
// receipt-backed closure only, blocked without receipt, budget never invented.
// Never print, persist or attach terminal text, provider bodies or child secrets.
import { chromium } from '@playwright/test';
import { spawn, spawnSync } from 'node:child_process';
import { createServer, request as httpRequest } from 'node:http';
import { createConnection, createServer as createPortReservation } from 'node:net';
import { mkdir, mkdtemp, readFile, realpath, stat, writeFile } from 'node:fs/promises';
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
const PROMPTS = ['SHINATAMA_UI_CRASH_PROMPT_A', 'SHINATAMA_UI_CRASH_PROMPT_B', 'SHINATAMA_UI_CRASH_PROMPT_C',
  'SHINATAMA_UI_CRASH_PROMPT_D', 'SHINATAMA_UI_CRASH_PROMPT_E'];
const DUMMY_KEY = 'sk-shinatama-synthetic-codex-no-provider-key';
const UUID_PATTERN = /^[a-f0-9]{8}-[a-f0-9]{4}-[1-8][a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
const CLEANUP_KEY = 'operational.native_session_cleanup.v1';
const LEDGER_KEY = 'operational.resource_reservations.v1';
const evidence = { check: 'forced-crash-recovery', status: 'running', stage: 'prepare', checks: {},
  scope: 'Actual forced crashes (TerminateProcess) of proof-owned Shinatama app instances and Codex 0.166.0 agent groups against real ConPTY/UI/database state, with real Codex, tracked hooks and a forwarding-only guard over synthetic loopback credentials. Recovery is exercised through the real Recover session UI on restarted app instances. No account, no paid provider, no completed model turn. A crash before the durable cleanup receipt must stay blocked (no receipt, no unlock); the extra qualified group-termination protocol for that window remains an open documented gap. Proxy forwarding is guarded, not an OS firewall.' };
const save = () => writeFile(resolve(output, 'forced-crash-recovery.json'), `${JSON.stringify(evidence, null, 2)}\n`);
const stage = async name => { evidence.stage = name; await save(); };
await save();
const proofError = code => Object.assign(new Error(code), { proofCode: code });
const fail = code => { throw proofError(code); };
const require = (condition, code) => { if (!condition) fail(code); };
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
const inside = (child, parent) => pathKey(child).startsWith(`${pathKey(parent)}\\`);
let aborted = false; let began = Date.now();
function checkDeadline() { if (aborted || Date.now() - began > 900000) fail('overall-proof-deadline'); }
async function poll(check, milliseconds, code) {
  const deadline = Date.now() + milliseconds;
  while (Date.now() < deadline) { checkDeadline(); if (await check()) return; await delay(100); }
  fail(code);
}
function within(promise, milliseconds, code) {
  let timer;
  return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(proofError(code)), milliseconds); })])
    .finally(() => clearTimeout(timer));
}
function sleepSync(milliseconds) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}
function hostFreePhysicalGiB() {
  return Number((freemem() / 1073741824).toFixed(2));
}
// Coarse pre-launch gate on free PHYSICAL memory (node-native; the .NET/WMI
// probes are killed inside the p14 job — measured 0/6 successes). The binding
// commit-charge gate runs inside the launched app through resources_status,
// the product's own admission measurement.
async function awaitPhysicalGate(requiredGiB, maxWaitMs, label) {
  const beganWait = Date.now();
  const gate = { label, kind: 'physical', requiredGiB, samples: 0, streak: 0, lastValues: [] };
  evidence.physicalGates ??= [];
  evidence.physicalGates.push(gate);
  let streak = 0;
  while (Date.now() - beganWait < maxWaitMs) {
    const free = hostFreePhysicalGiB();
    gate.samples++;
    streak = free >= requiredGiB ? streak + 1 : 0;
    gate.streak = streak;
    gate.lastValues.push(free);
    if (gate.lastValues.length > 12) gate.lastValues.shift();
    if (streak >= 2) {
      gate.waitedMs = Date.now() - beganWait;
      gate.passedAtGiB = free;
      await save();
      // Host waiting is environmental, not proof activity: keep the bounded
      // overall deadline measured on actual proof work.
      began += Date.now() - beganWait;
      return free;
    }
    if (gate.samples % 12 === 0) await save();
    await delay(5000);
  }
  gate.timedOutMs = Date.now() - beganWait;
  await save();
  fail(`host-physical-insufficient-${label}`);
}
// Fine gate on the product's own remaining-commit measurement, queried inside
// the live app before any admission decision (sidecar prewarm, session open).
// Calibrated against the production governor hysteresis (resources.rs:
// pressure_enter 90%, pressure_exit 87.5% of the commit limit — on this 41.3
// GiB host that is 4.13/5.17 GiB free): thresholds stay clear above the exit
// point, and the 3-sample streak (≈9s) covers stable_ms + cooldown_ms + the
// two recovery stage hops out of DeferStarts.
async function awaitCommitGate(ipc, requiredGiB, maxWaitMs, label) {
  const beganWait = Date.now();
  const gate = { label, kind: 'commit', requiredGiB, samples: 0, ipcFailures: 0, streak: 0, lastValues: [] };
  evidence.commitGates ??= [];
  evidence.commitGates.push(gate);
  let streak = 0;
  while (Date.now() - beganWait < maxWaitMs) {
    // Under extreme host pressure even the WebView IPC can time out; a failed
    // probe is environmental, never fatal to the gate itself.
    let status;
    try { status = await ipc('resources_status'); }
    catch { status = new Error('ipc-probe-failed'); }
    gate.samples++;
    const bytes = status instanceof Error ? null : Number(status?.system_remaining_commit_bytes);
    if (!Number.isFinite(bytes) || bytes <= 0) { gate.ipcFailures++; }
    else {
      const free = Number((bytes / 1073741824).toFixed(2));
      streak = free >= requiredGiB ? streak + 1 : 0;
      gate.streak = streak;
      gate.lastValues.push(free);
      if (gate.lastValues.length > 12) gate.lastValues.shift();
      if (streak >= 3) {
        gate.waitedMs = Date.now() - beganWait;
        gate.passedAtGiB = free;
        await save();
        began += Date.now() - beganWait;
        return free;
      }
    }
    if (gate.samples % 10 === 0) await save();
    await delay(3000);
  }
  gate.timedOutMs = Date.now() - beganWait;
  await save();
  fail(`host-commit-insufficient-${label}`);
}
function processDead(pid) {
  try { process.kill(pid, 0); return false; } catch { return true; }
}
function dbQuery(sql, ...params) {
  const db = new DatabaseSync(resolve(data, 'shinatama.db'), { readOnly: true });
  try { db.exec('PRAGMA busy_timeout=200'); return db.prepare(sql).all(...params); }
  finally { db.close(); }
}
function dbGet(sql, ...params) { return dbQuery(sql, ...params)[0]; }
function snapshot(threadId) {
  const segments = dbQuery('SELECT s.id,s.thread_id,s.native_id,r.resolved_path FROM segment s JOIN store_root r ON r.id=s.store_root_id AND r.config_revision=s.store_revision WHERE s.thread_id=? ORDER BY s.ordinal LIMIT 3', threadId);
  const activations = dbQuery('SELECT activation_sequence,instance_id,owner_pid,segment_id,native_id,ended_at,revision FROM activation WHERE thread_id=? ORDER BY activation_sequence LIMIT 4', threadId);
  const cleanup = dbQuery(`SELECT json_extract(entry.value,'$.owner.activation.instanceId') AS instance_id,
      json_type(entry.value,'$.cleanup') AS cleanup_type,
      json_extract(entry.value,'$.owner.reservation.owner.pid') AS host_pid,
      json_extract(entry.value,'$.owner.reservation.id') AS ticket_id
    FROM setting journal,json_each(journal.value,'$.entries') entry WHERE journal.key=? LIMIT 64`, CLEANUP_KEY);
  const thread = dbGet('SELECT id,revision,archived FROM thread WHERE id=?', threadId);
  return { segments, activations, cleanup, thread };
}
function ledgerSlots() {
  const row = dbGet('SELECT value FROM setting WHERE key=?', LEDGER_KEY);
  return row ? JSON.parse(row.value).slots : [];
}
function ledgerCeilingBytes() {
  const row = dbGet('SELECT value FROM setting WHERE key=?', LEDGER_KEY);
  if (!row) return null;
  const value = JSON.parse(row.value);
  const bytes = Number(value?.policy?.aggregate_ceiling?.memory_bytes);
  return Number.isFinite(bytes) && bytes > 0 ? bytes : null;
}
// Exact budget check: the dead host's own ticket, not "some slot is active"
// (the live sidecar holds its own reservation and would mask the assertion).
function ticketIdFor(snapshotValue, instanceId) {
  return snapshotValue.cleanup.find(entry => entry.instance_id === instanceId)?.ticket_id ?? null;
}
function ticketSlotActive(ticketId) {
  return !!ticketId && ledgerSlots().some(slot => slot.id === ticketId && slot.state === 'active');
}
// Synchronous tight race for the crash window: the durable receipt exists while
// the activation is still open. Returns 'caught' only in that exact window.
function raceReceiptWindow(instanceId, milliseconds) {
  const db = new DatabaseSync(resolve(data, 'shinatama.db'), { readOnly: true });
  try {
    db.exec('PRAGMA busy_timeout=50');
    const query = db.prepare(`SELECT a.ended_at AS ended,
        EXISTS(SELECT 1 FROM setting j,json_each(j.value,'$.entries') e
          WHERE j.key=? AND json_extract(e.value,'$.owner.activation.instanceId')=?
            AND json_extract(e.value,'$.cleanup') IS NOT NULL) AS receipt
      FROM activation a WHERE a.instance_id=?`);
    const deadline = Date.now() + milliseconds;
    let ticks = 0;
    while (Date.now() < deadline) {
      ticks++;
      let row = null;
      try { row = query.get(CLEANUP_KEY, instanceId, instanceId); } catch { row = null; }
      if (row) {
        if (row.receipt === 1 && row.ended === null) return { result: 'caught', ticks };
        if (row.ended !== null) return { result: 'late', ticks };
      }
      if (ticks % 8 === 0) sleepSync(1);
    }
    return { result: 'timeout', ticks };
  } finally { db.close(); }
}

// Resolve the fixed Codex prerequisite before opening any listening socket.
const system = process.env.SystemRoot || process.env.SYSTEMROOT;
require(system && process.env.USERPROFILE, 'required-windows-discovery-paths');
let codexDirectory = null;
for (const entry of (process.env.PATH || '').split(';')) {
  if (!entry) continue;
  try { if ((await stat(join(entry, 'codex.exe'))).isFile()) { codexDirectory = resolve(entry); break; } } catch { /* keep searching */ }
}
require(!!codexDirectory, 'installed-codex-required');

// Guarded synthetic provider: never reachable (Codex 0.166.0 honors no
// loopback base-URL override); any hit fails the proof.
let providerHits = 0; const sockets = new Set(); const guardedProviderPorts = new Set();
const provider = createServer((request, response) => {
  request.on('error', () => {}); response.on('error', () => {});
  providerHits++; request.destroy();
});
provider.maxConnections = 8; provider.headersTimeout = 3000; provider.requestTimeout = 5000;
provider.on('connection', socket => { sockets.add(socket); socket.setTimeout(5000, () => socket.destroy()); socket.on('error', () => {}); socket.once('close', () => sockets.delete(socket)); });
const reservation = createPortReservation();
let providerPort, port, guardPort;
const guardCounts = { allowed: 0, deniedAbsolute: 0, deniedConnect: 0, overflow: 0 };
function trackSocket(socket) {
  sockets.add(socket); socket.setTimeout(5000, () => socket.destroy());
  socket.on('error', () => {}); socket.once('close', () => sockets.delete(socket));
}
function trackGuardUpstream(socket) {
  trackSocket(socket);
  socket.once('connect', () => {
    const port = socket.localPort; guardedProviderPorts.add(port);
    socket.once('close', () => guardedProviderPorts.delete(port));
  });
}
const guard = createServer({ maxHeaderSize: 16384 }, (request, response) => {
  request.on('error', () => {}); response.on('error', () => {});
  let target;
  try { if (request.url.length <= 4096) target = new URL(request.url); } catch { /* Refuse malformed absolute targets. */ }
  if (request.socket.remoteAddress !== '127.0.0.1' || target?.protocol !== 'http:'
      || !request.url.startsWith(`http://127.0.0.1:${providerPort}/`)
      || target.host !== `127.0.0.1:${providerPort}` || target.username || target.password || target.hash) {
    guardCounts.deniedAbsolute++; response.writeHead(403, { connection: 'close' }); response.end(); return;
  }
  if (++guardCounts.allowed > 256) { guardCounts.overflow++; request.destroy(); return; }
  let body = Buffer.alloc(0);
  request.on('data', chunk => {
    if (body.length + chunk.length > 512 * 1024) { guardCounts.overflow++; request.destroy(); }
    else body = Buffer.concat([body, chunk]);
  });
  request.on('end', () => {
    const headers = { ...request.headers, host: `127.0.0.1:${providerPort}`, connection: 'close' };
    delete headers['proxy-authorization']; delete headers['proxy-connection'];
    const upstream = httpRequest({ hostname: '127.0.0.1', port: providerPort, path: target.pathname + target.search,
      method: request.method, headers, agent: false, maxHeaderSize: 16384 }, incoming => {
      let bytes = 0; incoming.on('error', () => response.destroy());
      response.writeHead(incoming.statusCode, incoming.headers);
      incoming.on('data', chunk => { if ((bytes += chunk.length) > 65536) { guardCounts.overflow++; incoming.destroy(); response.destroy(); } });
      incoming.pipe(response);
    });
    upstream.on('socket', trackGuardUpstream); upstream.on('error', () => response.destroy());
    upstream.setTimeout(5000, () => upstream.destroy()); response.once('close', () => upstream.destroy()); upstream.end(body);
  });
});
guard.on('connect', (request, client, head) => {
  if (client.remoteAddress !== '127.0.0.1' || request.url !== `127.0.0.1:${providerPort}`) {
    guardCounts.deniedConnect++; client.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n'); return;
  }
  if (++guardCounts.allowed > 256 || head.length > 16384) { guardCounts.overflow++; client.destroy(); return; }
  const upstream = createConnection({ host: '127.0.0.1', port: providerPort }); trackGuardUpstream(upstream);
  let sent = head.length, received = 0;
  client.on('data', chunk => { if ((sent += chunk.length) > 1024 * 1024) { guardCounts.overflow++; client.destroy(); upstream.destroy(); } });
  upstream.on('data', chunk => { if ((received += chunk.length) > 65536) { guardCounts.overflow++; client.destroy(); upstream.destroy(); } });
  client.once('close', () => upstream.destroy()); upstream.once('close', () => client.destroy());
  upstream.once('connect', () => { client.write('HTTP/1.1 200 Connection Established\r\n\r\n'); if (head.length) upstream.write(head); client.pipe(upstream); upstream.pipe(client); });
});
guard.maxConnections = 16; guard.headersTimeout = 3000; guard.requestTimeout = 5000;
guard.on('connection', trackSocket); guard.on('clientError', (_error, socket) => socket.destroy());
function refusedByGuard(method, path) {
  return new Promise((accept, reject) => {
    const request = httpRequest({ hostname: '127.0.0.1', port: guardPort, method, path, agent: false }, response => {
      response.resume(); response.once('end', () => response.statusCode === 403 ? accept() : reject(proofError('guard-refusal-required')));
    });
    request.once('connect', (response, socket) => { socket.destroy(); response.statusCode === 403 ? accept() : reject(proofError('guard-refusal-required')); });
    request.on('error', reject); request.setTimeout(2000, () => request.destroy(proofError('guard-self-check-deadline'))); request.end();
  });
}
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
  // Windows PowerShell's .NET Framework overload creates fresh directories with
  // C01's owner-only protected DACL. Never change an existing path or user ACL.
  const script = String.raw`
$ErrorActionPreference = 'Stop'
$phase = 'input'
try {
  $inputText = [Console]::In.ReadToEnd()
  if ($inputText.Length -gt 16384) { throw 'bounded-input' }
  $value = ConvertFrom-Json -InputObject $inputText
  $root = [IO.Path]::GetFullPath($value.fixture)
  if ($root -ne $value.fixture -or [IO.Path]::GetFileName($root) -notmatch '^shinatama-forced-crash-[A-Za-z0-9]+$') { throw 'fresh-fixture-only' }
  $info = [IO.DirectoryInfo]::new($root)
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
  $phase = 'fixture-owner'
  if (!$info.Exists -or ($info.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $info.GetAccessControl().GetOwner([Security.Principal.SecurityIdentifier]) -ne $sid) { throw 'ordinary-owned-fixture-only' }
  $sddl = 'O:' + $sid.Value + 'D:P(A;OICI;FA;;;' + $sid.Value + ')'
  $sections = [Security.AccessControl.AccessControlSections]::Owner -bor [Security.AccessControl.AccessControlSections]::Access
  $directoryAcl = [Security.AccessControl.DirectorySecurity]::new()
  $directoryAcl.SetSecurityDescriptorSddlForm($sddl, $sections)
  $phase = 'create-directories'
  foreach ($relative in @('data', 'data\sidecar', 'data\sidecar\config')) {
    $path = [IO.Path]::Combine($root, $relative)
    if ([IO.Directory]::Exists($path) -or [IO.File]::Exists($path)) { throw 'new-directories-only' }
    [void][IO.Directory]::CreateDirectory($path, $directoryAcl)
  }
  $path = [IO.Path]::Combine($root, 'data\sidecar\config\billion-context.json')
  $phase = 'create-config'
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes($value.configuration)
  $file = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try { $file.Write($bytes, 0, $bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
  $fileAcl = [Security.AccessControl.FileSecurity]::new()
  $fileAcl.SetSecurityDescriptorSddlForm($sddl, $sections)
  $phase = 'protect-config'
  [IO.File]::SetAccessControl($path, $fileAcl)
  $actual = [IO.File]::GetAccessControl($path)
  $rules = $actual.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])
  $phase = 'verify-config'
  if (!$actual.AreAccessRulesProtected -or $actual.GetOwner([Security.Principal.SecurityIdentifier]) -ne $sid -or $rules.Count -ne 1 -or $rules[0].IdentityReference -ne $sid -or $rules[0].IsInherited -or $rules[0].AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or $rules[0].FileSystemRights -ne [Security.AccessControl.FileSystemRights]::FullControl) { throw 'owner-only-acl-required' }
  [Console]::Out.Write('private-config-ready')
} catch { [Console]::Error.Write('private-config-preparation-failed:' + $phase); exit 1 }
`;
  const child = spawn(resolve(system, 'System32/WindowsPowerShell/v1.0/powershell.exe'),
    ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true,
      env: { SystemRoot: system, WINDIR: system, PATH: resolve(system, 'System32'), TEMP: resolve(fixture, 'tmp'), TMP: resolve(fixture, 'tmp') },
      stdio: ['pipe', 'pipe', 'pipe'] });
  let output = '', diagnostic = '', bytes = 0;
  for (const stream of [child.stdout, child.stderr]) stream.on('data', chunk => {
    bytes += chunk.length; if (bytes > 1024) child.kill(); else if (stream === child.stdout) output += chunk.toString('utf8'); else diagnostic += chunk.toString('utf8');
  });
  child.stdin.on('error', () => {});
  const finished = new Promise((accept, reject) => { child.once('error', reject); child.once('close', code => accept(code)); });
  child.stdin.end(JSON.stringify({ fixture, configuration: `${JSON.stringify(configuration, null, 2)}\n` }));
  let code;
  try { code = await within(finished, 10000, 'private-config-preparation-deadline'); }
  finally { if (child.exitCode === null && child.signalCode === null) { child.kill(); await within(finished, 3000, 'private-config-cleanup-deadline').catch(() => {}); } }
  if (code !== 0 || bytes > 1024 || output !== 'private-config-ready') {
    evidence.configPreparation = { code, bytes, phase: diagnostic.match(/private-config-preparation-failed:(input|fixture-owner|create-directories|create-config|protect-config|verify-config)/)?.[1] ?? 'helper-startup' };
    fail('private-config-preparation-failed');
  }
  const path = resolve(root, 'config/billion-context.json');
  require(inside(await realpath(path), fixture) && (await stat(path)).size <= 16384, 'private-config-boundary');
  require(JSON.stringify(JSON.parse(await readFile(path, 'utf8'))) === JSON.stringify(configuration), 'exact-guarded-config-required');
  evidence.checks.manualGuardConfigAndOwnerOnlyAclPreparedBeforeApp = true;
}
try {
  await new Promise((accept, reject) => { provider.once('error', reject); provider.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
  providerPort = provider.address().port;
  await new Promise((accept, reject) => { guard.once('error', reject); guard.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
  guardPort = guard.address().port;
  await stage('guard-self-check');
  await refusedByGuard('GET', 'http://forbidden.shinatama.invalid/proof');
  await refusedByGuard('CONNECT', 'forbidden.shinatama.invalid:443');
  require(guardCounts.allowed === 0 && guardCounts.deniedAbsolute === 1 && guardCounts.deniedConnect === 1, 'guard-refusal-self-check');
  evidence.checks.forbiddenAbsoluteAndConnectRefusedBeforeApp = true;
  await stage('guard-private-config'); await prepareGuardedConfig(); await save();
  await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
  port = reservation.address().port;
  await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
} catch (error) {
  for (const socket of sockets) socket.destroy();
  reservation.close(); guard.close(); provider.close();
  evidence.status = 'failed'; evidence.failure = error.proofCode || 'guarded-fixture-preparation-failed';
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only' }; await save(); throw proofError(evidence.failure);
}
if (process.argv.includes('--guard-only')) {
  for (const socket of sockets) socket.destroy();
  await Promise.all([new Promise(accept => guard.close(accept)), new Promise(accept => provider.close(accept))]);
  evidence.status = 'prepared'; evidence.stage = 'guard-only-complete';
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only', appExecuted: false };
  await save(); console.log('PASS: local guard refusal and private configuration; app not started.'); process.exit(0);
}

// ---- multi-app orchestration --------------------------------------------
let app = null, browser = null, page = null, appIndex = 0, pageErrors = 0, owned = false;
let activeTerminal = null;
let lastSidecarStatus = null;
const appRecords = [];
function makeEnvironment(debugPort) {
  const environment = {};
  for (const name of ['SystemRoot', 'WINDIR', 'SystemDrive', 'COMSPEC', 'PATHEXT', 'PROCESSOR_ARCHITECTURE',
    'NUMBER_OF_PROCESSORS', 'ProgramFiles', 'ProgramFiles(x86)', 'ProgramW6432', 'ProgramData', 'ALLUSERSPROFILE',
    'OS', 'USERNAME', 'USERDOMAIN', 'USERPROFILE', 'HOMEDRIVE', 'HOMEPATH', 'APPDATA', 'LOCALAPPDATA']) {
    const key = Object.keys(process.env).find(key => key.toLowerCase() === name.toLowerCase());
    if (key) environment[name] = process.env[key];
  }
  Object.assign(environment, { SystemRoot: system, WINDIR: system,
    PATH: [codexDirectory, resolve(system, 'System32')].join(';'),
    HOME: environment.USERPROFILE,
    TEMP: resolve(fixture, 'tmp'), TMP: resolve(fixture, 'tmp'), SHINATAMA_HOME: data,
    WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, `webview-profile-${appIndex}`),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort} --remote-debugging-address=127.0.0.1`,
    OPENAI_API_KEY: DUMMY_KEY,
  });
  return environment;
}
async function reserveDebugPort() {
  const server = createPortReservation();
  await new Promise((accept, reject) => { server.once('error', reject); server.listen({ host: '127.0.0.1', port: 0, exclusive: true }, accept); });
  const reservedPort = server.address().port;
  await new Promise((accept, reject) => server.close(error => error ? reject(error) : accept()));
  return reservedPort;
}
async function launchApp() {
  appIndex++;
  await awaitPhysicalGate(3.5, appIndex === 1 ? 5400000 : 1800000, `app${appIndex}`);
  for (let attempt = 1; ; attempt++) {
    try {
      return await launchAppOnce();
    } catch (error) {
      // One controlled relaunch when the startup itself died under host
      // pressure (page/browser closed or app exited before ownership), plus
      // up to three relaunches when the ledger ceiling was initialized below
      // MAX_TOTAL because the window arrived just after the previous startup.
      // Proof-coded refusals (gates, storage mismatch) are never retried.
      const message = String(error.message);
      const ceilingRetry = error.proofCode === 'ledger-ceiling-below-max';
      const startupDeath = !owned
        && (/Target page, context or browser has been closed|Target closed|Connection closed/i.test(message)
          || error.proofCode === 'owned-app-startup-failed');
      const retryable = ceilingRetry ? attempt < 4 : startupDeath && attempt < 2;
      if (!retryable) throw error;
      evidence.launchRetries ??= [];
      evidence.launchRetries.push({ appIndex, attempt, reason: message.split('\n')[0].slice(0, 160) });
      await save();
      try { if (app && app.exitCode === null && app.signalCode === null) process.kill(app.pid); } catch { /* gone */ }
      await browser?.close().catch(() => {});
      browser = null; page = null; app = null;
      await delay(ceilingRetry ? 1000 : 5000);
      if (!ceilingRetry) await awaitPhysicalGate(3.5, 900000, `app${appIndex}-retry`);
    }
  }
}
async function launchAppOnce() {
  const debugPort = await reserveDebugPort();
  const record = { index: appIndex, debugPort, connectionRefused: 0, otherConnectionFailure: 0 };
  appRecords.push(record);
  let spawnFailed = false;
  app = spawn(executable, [], { cwd: process.env.SHINATAMA_SMOKE_APP_EXE ? dirname(executable) : workspace,
    windowsHide: true, stdio: ['ignore', 'ignore', 'ignore'], env: makeEnvironment(debugPort) });
  record.pid = app.pid;
  app.once('error', () => { spawnFailed = true; });
  const exited = new Promise(accept => app.once('exit', accept));
  app.exited = () => exited;
  await poll(async () => {
    require(!spawnFailed && app.exitCode === null, 'owned-app-startup-failed');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`, { timeout: 1000 }); return true; }
    catch (error) {
      /ECONNREFUSED|connection refused/i.test(String(error.message)) ? record.connectionRefused++ : record.otherConnectionFailure++;
      return false;
    }
  }, 30000, 'owned-webview-startup-deadline');
  await poll(async () => {
    page = browser.contexts().flatMap(context => context.pages()).find(candidate => {
      try { const url = new URL(candidate.url()); return url.hostname === 'tauri.localhost' && ['/', '/index.html'].includes(url.pathname); }
      catch { return false; }
    }); return !!page;
  }, 30000, 'main-workspace-webview-missing');
  page.setDefaultTimeout(25000); page.on('pageerror', () => { pageErrors++; });
  await poll(async () => (await page.locator('#runtime-status').textContent()).includes('Local runtime'), 15000, 'runtime-unavailable');
  // Fine gate on the product's own commit measurement before any admission.
  // Later apps need the ledger ceiling at its hard 4 GiB maximum: a blocked
  // pre-receipt ticket (1 GiB) stays reserved, so the startup headroom must
  // satisfy floor(commit/2) >= MAX_TOTAL = 4 GiB, i.e. ≥8.0 GiB (observed
  // steady windows at 8.3 GiB), with a small margin.
  await awaitCommitGate(ipc, appIndex === 1 ? 6.5 : 8.1, 14400000, `app${appIndex}`);
  if (appIndex > 1) {
    // The ledger ceiling is derived from the headroom at APP STARTUP, before
    // this in-app gate can see the window. If the startup missed the window,
    // the ceiling stays below MAX_TOTAL and a new canary + session cannot fit
    // beside the blocked ticket: relaunch NOW, while the gate has just
    // confirmed the window (bounded relaunches, evidence recorded).
    const required = 4 * 1024 * 1024 * 1024;
    const ceiling = ledgerCeilingBytes();
    if (ceiling !== null && ceiling < required) {
      evidence.ceilingRetries ??= [];
      evidence.ceilingRetries.push({ appIndex, ceilingGiB: Number((ceiling / 1073741824).toFixed(2)) });
      await save();
      throw Object.assign(new Error('ledger-ceiling-below-max'), { proofCode: 'ledger-ceiling-below-max' });
    }
  }
  // Prewarm through the real UI command seam whenever the sidecar is not
  // already ready and healthy; Controller.start is idempotent via its CAS.
  const initialStatus = await ipc('sidecar_status');
  lastSidecarStatus = initialStatus instanceof Error ? { ipcError: String(initialStatus.message).slice(0, 120) } : initialStatus;
  if (!(initialStatus instanceof Error) && !(initialStatus.phase === 'ready' && initialStatus.healthy === true)) {
    const started = await ipc('sidecar_start');
    if (started instanceof Error) lastSidecarStatus = { ...lastSidecarStatus, startError: String(started.message).slice(0, 120) };
    else lastSidecarStatus = started;
  }
  // Bounded start retries: the governor may defer admission while host
  // headroom recovers; a refused start is retryable by design.
  await poll(async () => {
    const status = await ipc('sidecar_status');
    if (!(status instanceof Error)) {
      lastSidecarStatus = status;
      if (status.phase === 'ready' && status.healthy === true) return true;
      if (['failed', 'stopped', 'unavailable'].includes(status.phase)) {
        const started = await ipc('sidecar_start');
        if (started instanceof Error) lastSidecarStatus = { ...lastSidecarStatus, startError: String(started.message).slice(0, 120) };
      }
    }
    return false;
  }, 120000, 'sidecar-not-ready-before-open');
  require(pathKey((await ipc('storage_location')).directory) === pathKey(data), 'private-app-storage-mismatch');
  owned = true;
  return record;
}
// Forced crash of the proof-owned app: immediate TerminateProcess. Cleanup of
// the group is by design, not by sweep: the session job carries
// KILL_ON_JOB_CLOSE (agent group dies with the host's last handle — verified
// by the phase-1 agent-death assertion), WebView2 children exit with their
// browser process, and the supervised sidecar exits on stdin EOF; a lingering
// sidecar is absorbed by the next app's bounded start-retry loop. (.NET/WMI
// enumeration is killed inside the p14 job, so no external sweep is used.)
async function crashApp(record) {
  const pid = app.pid;
  record.crashedAt = Date.now();
  try { process.kill(pid); } catch { /* already dead */ }
  await within(app.exited(), 10000, 'crashed-app-exit-deadline').catch(() => {});
  await poll(async () => processDead(pid), 10000, 'crashed-app-still-alive');
  await browser?.close().catch(() => {});
  browser = null; page = null; app = null; owned = false;
}
async function ipc(command, args = {}) {
  const retryRead = ['terminal_list', 'terminal_resources', 'resources_status', 'global_workspace_load', 'storage_location', 'project_recent', 'thread_list', 'sidecar_status'].includes(command);
  const deadline = Date.now() + 3000;
  do {
    const result = await within(page.evaluate(async ({ command, args }) => {
      try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
      catch (error) { return { ok: false, code: typeof error?.code === 'string' ? error.code : null, busy: ['busy', 'unavailable'].includes(error?.code) }; }
    }, { command, args }), 30000, 'metadata-ipc-deadline');
    if (result.ok) return result.value;
    if (!retryRead || !result.busy) return Object.assign(new Error(result.code || 'ipc-refused'), { ipcCode: result.code || 'refused' });
    await delay(100);
  } while (Date.now() < deadline);
  return Object.assign(new Error('busy'), { ipcCode: 'busy-deadline' });
}
async function ipcOk(command, args = {}) {
  const result = await ipc(command, args);
  require(!(result instanceof Error), `ipc-failed-${command}`);
  return result;
}
const sessions = () => ipcOk('terminal_list');
const pane = id => page.locator(`.wm-pane:has([data-view-id="${id}"])`);
const layout = async () => { const row = await ipcOk('global_workspace_load', { request: { workspaceId: 'main' } }); return row ? JSON.parse(row.layoutJson) : null; };
async function screen(id) {
  const value = await pane(id).locator('.xterm-accessibility-tree').innerText();
  require(value.length <= 32768, 'bounded-visible-terminal'); return value;
}
async function input(id, text, key) {
  activeTerminal = id;
  const field = pane(id).locator('.xterm-helper-textarea'); await field.focus();
  if (text) {
    require(PROMPTS.includes(text), 'fixed-synthetic-prompt-only');
    await field.pressSequentially(text, { delay: 5 });
    await poll(async () => (await screen(id)).includes(text), 5000, 'synthetic-prompt-not-visible-before-submit');
  }
  if (key) await page.keyboard.press(key);
}
async function ready(id, operation) {
  const handled = new Set(); const allowed = []; let lastShape = '';
  evidence.onboarding ??= {}; evidence.onboarding[operation] = allowed;
  await poll(async () => {
    const text = await screen(id);
    require(await pane(id).locator('.exit-banner').count() === 0, 'native-process-exited-before-readiness');
    const shape = {
      authPick: /Sign in with ChatGPT/.test(text) && /(Provide your own API key|Use an OpenAI API key)/.test(text),
      apiKeyEntry: /Use your own OpenAI API key/.test(text),
      apiKeyPrefilled: /Detected OPENAI_API_KEY environment variable/.test(text),
      trust: /Trust this folder\?/.test(text) && /Trust and continue/.test(text),
      hookReview: /Review hooks|Trust all and continue|Continue without trusting/.test(text),
      composer: /Ask Codex to do anything/.test(text),
    };
    const encoded = JSON.stringify(shape);
    if (encoded !== lastShape) { lastShape = encoded; await save(); }
    require(!shape.hookReview, 'hook-trust-review-screen-appeared');
    require(handled.size < 3, 'bounded-onboarding-steps');
    if (shape.authPick && !handled.has('auth-pick')) {
      handled.add('auth-pick'); allowed.push('auth-pick'); await save();
      await input(id, null, '3');
      return false;
    }
    if (shape.apiKeyEntry && !handled.has('api-key')) {
      handled.add('api-key'); allowed.push('api-key'); await save();
      if (!shape.apiKeyPrefilled) await input(id, DUMMY_KEY, null);
      await input(id, null, 'Enter');
      return false;
    }
    if (shape.trust && !handled.has('trust')) {
      handled.add('trust'); allowed.push('trust'); await save();
      await input(id, null, 'Enter');
      return false;
    }
    return shape.composer && /for shortcuts/i.test(text);
  }, 60000, 'unrecognized-or-blocked-native-screen');
  return allowed;
}
function requireActiveCleanupOwner(snapshotValue, instance) {
  const entries = snapshotValue.cleanup.filter(entry => entry.instance_id === instance);
  require(entries.length === 1 && entries[0].cleanup_type === 'null', 'active-owner-linked-to-exact-process');
}
async function openCodexSession(promptIndex) {
  // Bounded retry for resource-family inline refusals only: the governor is
  // correct under host pressure, and the oscillating shared host may dip
  // between the launch gate and the open. Any other refusal fails at once.
  evidence.openAttempts ??= [];
  let attempt = 0;
  for (;;) {
    attempt++;
    // Bounded retry of the dialog interaction itself: under host load the
    // WebView can leave fields non-actionable (fill/click timeouts) without
    // any product refusal. Three consecutive UI failures stay fatal.
    for (let ui = 1; ; ui++) {
      try {
        await poll(async () => await page.locator('#new-terminal').isEnabled(), 15000, 'new-terminal-not-rearmed');
        await page.locator('#new-terminal').click();
        const dialog = page.locator('#new-session-dialog');
        await poll(async () => await dialog.isVisible(), 15000, 'new-session-dialog-did-not-open');
        // The product hides the free-text project fields when a project is
        // already selected (new-session.ts: choosingProject=false shows the
        // context line instead): filling a hidden input would time out.
        const field = dialog.locator('#new-session-project');
        const context = dialog.locator('#new-session-project-context');
        await poll(async () => (await field.isVisible()) || (await context.isVisible()), 10000, 'project-input-or-context-missing');
        if (await field.isVisible()) {
          await field.fill(projectPath, { timeout: 30000 });
        } else {
          const shown = (await dialog.locator('#new-session-project-path').innerText()).trim();
          require(pathKey(shown) === pathKey(projectPath), 'preset-project-mismatch');
        }
        await dialog.locator('#new-session-harness').selectOption('codex', { timeout: 30000 });
        await dialog.getByRole('button', { name: 'Open', exact: true }).click({ timeout: 30000 });
        break;
      } catch (error) {
        if (ui >= 3) throw error;
        evidence.dialogRetries ??= [];
        evidence.dialogRetries.push({ promptIndex, ui, reason: String(error.message).split('\n')[0].slice(0, 140) });
        await save();
        await page.keyboard.press('Escape').catch(() => {});
        await page.locator('#new-session-dialog').getByRole('button', { name: 'Cancel', exact: true }).click({ timeout: 5000 }).catch(() => {});
        await delay(3000);
      }
    }
    const dialog = page.locator('#new-session-dialog');
    let refusal = null;
    await poll(async () => {
      if (await dialog.locator('#new-session-error').isVisible()) {
        const text = await dialog.locator('#new-session-error').innerText();
        refusal = text.replace(/[\r\n]+/g, ' ').slice(0, 240);
        return true;
      }
      return !await dialog.isVisible();
    }, 120000, 'new-session-deadline');
    if (!refusal) break;
    evidence.openAttempts.push({ attempt, refusal });
    await save();
    // Retryable families only: governor resource refusals (correct under host
    // pressure) and the released canary variant whose cleanup is confirmed —
    // its fixed 7s TUI-startup budget can time out under external CPU
    // contention. The retained-cleanup variant and every other refusal fail
    // immediately; three identical failures are treated as real.
    const resource = /resource|memory|admission|slot|pressure|headroom|reservation|budget/i.test(refusal);
    const transientCanary = /qualified startup receipt|contained TUI check did not start/i.test(refusal);
    require((resource || transientCanary) && attempt <= 5, 'open-session-inline-refusal');
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click().catch(() => {});
    await poll(async () => !await dialog.isVisible(), 5000, 'dialog-stuck-after-cancel');
    await delay(10000);
    await awaitCommitGate(ipc, 6.5, 1200000, `open-retry-${attempt}`);
  }
  const list = await sessions();
  const session = list.find(item => pathKey(item.cwd) === pathKey(projectPath) && item.shell === 'codex' && item.id !== activeTerminal);
  require(!!session, 'expected-codex-session');
  activeTerminal = session.id;
  await poll(async () => !!(await layout())?.views.find(view => view.id === session.id)?.resource.threadId, 15000, 'saved-thread-view-binding');
  const threadId = (await layout()).views.find(view => view.id === session.id).resource.threadId;
  await ready(session.id, `open-${promptIndex}`);
  await input(session.id, PROMPTS[promptIndex], 'Enter');
  await poll(async () => snapshot(threadId).activations.some(row => row.ended_at === null), 60000, 'native-binding-deadline');
  const bound = snapshot(threadId);
  require(bound.segments.length === 1 && UUID_PATTERN.test(bound.segments[0].native_id), 'segment-native-uuid');
  const open = bound.activations.find(row => row.ended_at === null);
  require(!!open && open.instance_id === session.id && open.native_id === bound.segments[0].native_id, 'exact-native-owner');
  requireActiveCleanupOwner(bound, session.id);
  return { session, threadId, nativeUuid: bound.segments[0].native_id, activation: open };
}
async function selectProjectInSidebar(projectId) {
  const sidebar = page.locator('.thread-sidebar');
  await sidebar.getByRole('button', { name: 'Reload threads', exact: true }).click().catch(() => {});
  await sidebar.locator('#thread-sidebar-project').selectOption(projectId);
  return sidebar;
}
async function clickRecover(sidebar, threadId) {
  await sidebar.locator(`button.thread-sidebar-anchor[data-thread-id="${threadId}"]`).click();
  const recover = sidebar.getByRole('button', { name: 'Recover session', exact: true });
  await poll(async () => await recover.isVisible(), 10000, 'recover-button-missing');
  await recover.click();
  await poll(async () => {
    const text = await sidebar.locator('.thread-sidebar-status').innerText().catch(() => '');
    return text.length > 0 && !/Working|…|\.\.\.$/.test(text);
  }, 20000, 'recover-status-deadline');
  return (await sidebar.locator('.thread-sidebar-status').innerText()).replace(/\s+/g, ' ').slice(0, 200);
}
async function stopTerminal(id) {
  await page.locator(`.wm-tab[data-view-id="${id}"]`).click();
  await pane(id).getByRole('button', { name: 'Stop terminal', exact: true }).click();
  await page.locator('#confirm-dialog').getByRole('button', { name: 'Stop', exact: true }).click();
  await poll(async () => !(await sessions()).some(session => session.id === id), 15000, 'owned-terminal-cleanup-deadline');
}
async function closeFrame() {
  const closing = Promise.all([page.waitForEvent('close', { timeout: 15000 }), within(app.exited(), 15000, 'owned-app-exit-deadline')]); void closing.catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click(); await closing;
}
let projectId = null;
try {
  // ---- Phase 1: app #1, agent force-kill while the host is alive (before
  // any held ticket exists: the pre-receipt leak below consumes a full slot
  // of the 4 GiB MAX_TOTAL ledger ceiling, so healthy-capacity phases run
  // first) ----
  await stage('app1-start');
  await launchApp();
  await stage('session-k-binding');
  const sessionK = await openCodexSession(0);
  projectId = (await ipcOk('project_recent')).find(item => pathKey(item.path) === pathKey(projectPath))?.id;
  require(!!projectId, 'saved-project');
  const agentPidK = snapshot(sessionK.threadId).activations.find(row => row.ended_at === null).owner_pid;
  await stage('force-kill-agent-k');
  spawnSync('taskkill', ['/F', '/T', '/PID', String(agentPidK)], { windowsHide: true, timeout: 15000 });
  await poll(async () => {
    const value = snapshot(sessionK.threadId);
    const activation = value.activations.find(row => row.instance_id === sessionK.session.id);
    return activation.ended_at !== null
      && !value.cleanup.some(entry => entry.instance_id === sessionK.session.id);
  }, 30000, 'agent-crash-not-finalized-by-live-host');
  await poll(async () => await pane(sessionK.session.id).locator('.exit-banner').count() > 0, 10000, 'exit-banner-missing-after-agent-crash').catch(() => {});
  evidence.checks.agentForcedTerminationClosedExactlyWithReceiptOnLiveHost = true;
  await ipcOk('terminal_stop', { id: sessionK.session.id });
  await poll(async () => !(await sessions()).some(session => session.id === sessionK.session.id), 15000, 'crashed-terminal-removal-deadline');
  const rowsAfterK = (await ipcOk('thread_list', { request: { projectId, cursor: null, limit: 50 } })).rows;
  require(rowsAfterK.find(row => row.id === sessionK.threadId)?.status === 'inactive', 'crashed-agent-thread-inactive');

  // ---- Phase 2: app #1 again, live session, forced crash BEFORE any receipt ----
  await stage('session-a-binding');
  const sessionA = await openCodexSession(1);
  const preCrashA = snapshot(sessionA.threadId);
  require(preCrashA.activations.length === 1 && preCrashA.activations[0].ended_at === null, 'activation-open-before-crash');
  require(!preCrashA.cleanup.some(entry => entry.instance_id === sessionA.session.id && entry.cleanup_type !== 'null'), 'no-receipt-before-crash');
  const agentPidA = preCrashA.activations[0].owner_pid;
  const ticketIdA = ticketIdFor(preCrashA, sessionA.session.id);
  require(!!ticketIdA, 'association-ticket-recorded-before-crash');
  const appOne = appRecords[0];
  await stage('crash-app1-before-receipt');
  await crashApp(appOne);
  await poll(async () => processDead(agentPidA), 15000, 'agent-group-survived-host-crash');
  const afterCrashA = snapshot(sessionA.threadId);
  require(afterCrashA.activations[0].ended_at === null, 'activation-must-stay-open-without-receipt');
  require(afterCrashA.cleanup.some(entry => entry.instance_id === sessionA.session.id && entry.cleanup_type === 'null'), 'owner-association-recorded');
  evidence.crashOne = { pid: appOne.pid, agentPid: agentPidA, ledgerSlots: ledgerSlots().length };
  require(ticketSlotActive(ticketIdA), 'crashed-ticket-still-reserved');

  // ---- Phase 3: app #2, real crash data, Recover must stay blocked ----
  await stage('app2-start');
  await launchApp();
  require((await sessions()).length === 0, 'no-terminal-after-restart');
  const sidebar2 = await selectProjectInSidebar(projectId);
  await stage('recover-refused-without-receipt');
  const statusA = await clickRecover(sidebar2, sessionA.threadId);
  require(/shutdown was not confirmed/i.test(statusA), 'blocked-crash-must-report-unconfirmed-shutdown');
  const preservedA = snapshot(sessionA.threadId);
  require(preservedA.activations[0].ended_at === null && preservedA.activations[0].revision === afterCrashA.activations[0].revision, 'refused-recovery-changed-activation');
  require(ticketSlotActive(ticketIdA), 'blocked-thread-budget-stays-reserved');
  evidence.checks.crashBeforeReceiptBlockedAndActivationAndBudgetPreserved = true;

  // ---- Phase 4: crash inside the receipt window (receipt durable, activation open) ----
  let windowSession = null; const windowAttempts = [];
  evidence.windowAttempts = windowAttempts;
  for (let promptIndex = 2; promptIndex < PROMPTS.length && !windowSession; promptIndex++) {
    if (!page) {
      // A previous attempt killed the app after the close landed; restart for
      // the next bounded attempt against the same private data directory.
      await stage('relaunch-after-window-miss');
      await launchApp();
    }
    await stage(`session-window-${promptIndex}`);
    const candidate = await openCodexSession(promptIndex);
    await page.locator(`.wm-tab[data-view-id="${candidate.session.id}"]`).click();
    await pane(candidate.session.id).getByRole('button', { name: 'Stop terminal', exact: true }).click();
    await page.locator('#confirm-dialog').getByRole('button', { name: 'Stop', exact: true }).click();
    const race = raceReceiptWindow(candidate.session.id, 30000);
    const attempt = { promptIndex, race: race.result, ticks: race.ticks };
    if (race.result === 'caught') {
      const appRecord = appRecords[appRecords.length - 1];
      const crashedPid = app.pid;
      attempt.crashPid = crashedPid;
      attempt.agentPid = snapshot(candidate.threadId).activations.find(row => row.ended_at === null)?.owner_pid ?? null;
      windowAttempts.push(attempt);
      await stage('crash-app2-in-receipt-window');
      try { process.kill(crashedPid); } catch { /* already dead */ }
      await within(app.exited(), 10000, 'crashed-app-exit-deadline').catch(() => {});
      await poll(async () => processDead(crashedPid), 10000, 'crashed-app-still-alive');
      appRecord.crashedAt = Date.now();
      if (attempt.agentPid !== null) {
        await poll(async () => processDead(attempt.agentPid), 15000, 'window-agent-survived-host-crash');
      }
      await browser?.close().catch(() => {});
      browser = null; page = null; app = null; owned = false;
      const state = snapshot(candidate.threadId);
      const activation = state.activations.find(row => row.instance_id === candidate.session.id);
      const receipt = state.cleanup.find(entry => entry.instance_id === candidate.session.id);
      attempt.activationEnded = activation.ended_at !== null;
      attempt.receiptPresent = !!receipt && receipt.cleanup_type !== 'null';
      if (activation.ended_at === null && attempt.receiptPresent) {
        // The receipt transaction released the ledger ticket before the crash:
        // capacity is already back while the activation stays open for recovery.
        const ticketId = receipt.ticket_id ?? null;
        attempt.ticketReleased = !ticketSlotActive(ticketId);
        require(!!ticketId && attempt.ticketReleased, 'window-ticket-must-be-released-with-receipt');
        windowSession = { ...candidate, attempt, ticketId };
      } else attempt.outcome = 'closed-before-kill-landed';
    } else {
      windowAttempts.push(attempt);
      // The stop completed normally; wait for the clean close and retry with a
      // fresh session. No crash happened, so the app stays alive.
      await poll(async () => !(await sessions()).some(session => session.id === candidate.session.id), 15000, 'window-miss-cleanup-deadline');
      await poll(async () => {
        const value = snapshot(candidate.threadId);
        return value.activations.every(row => row.ended_at !== null);
      }, 15000, 'window-miss-activation-not-closed');
    }
    await save();
  }
  require(!!windowSession, 'receipt-window-never-caught');
  evidence.checks.crashInsideReceiptWindowLeftDurableReceiptAndOpenActivation = true;

  // ---- Phase 5: app #3, real recovery consumes exactly that receipt ----
  await stage('app3-start');
  await launchApp();
  require((await sessions()).length === 0, 'no-terminal-after-window-crash');
  const sidebar3 = await selectProjectInSidebar(projectId);
  await stage('recover-window-crash');
  const beforeRecover = snapshot(windowSession.threadId);
  const statusC = await clickRecover(sidebar3, windowSession.threadId);
  require(/Session recovered/i.test(statusC), 'real-recovery-refused-or-different-message');
  const afterRecover = snapshot(windowSession.threadId);
  const closed = afterRecover.activations.find(row => row.instance_id === windowSession.session.id);
  const openBefore = beforeRecover.activations.find(row => row.instance_id === windowSession.session.id);
  require(closed.ended_at !== null && closed.revision === openBefore.revision + 1, 'exact-activation-not-finalized');
  require(afterRecover.segments.length === 1 && afterRecover.segments[0].native_id === windowSession.nativeUuid, 'segment-untouched-by-recovery');
  require(!afterRecover.cleanup.some(entry => entry.instance_id === windowSession.session.id), 'recovered-receipt-consumed');
  // The blocked thread A keeps its open activation and its reserved budget;
  // its entry is untouched by C's recovery.
  const stillBlockedA = snapshot(sessionA.threadId);
  require(stillBlockedA.activations[0].ended_at === null, 'blocked-activation-must-not-change');
  require(stillBlockedA.cleanup.some(entry => entry.instance_id === sessionA.session.id && entry.cleanup_type === 'null'), 'blocked-owner-still-recorded');
  require(ticketSlotActive(ticketIdA), 'blocked-budget-still-reserved-after-other-recovery');
  require(!ticketSlotActive(windowSession.ticketId), 'recovered-ticket-must-stay-released');
  const rowsAfterRecover = (await ipcOk('thread_list', { request: { projectId, cursor: null, limit: 50 } })).rows;
  require(rowsAfterRecover.find(row => row.id === windowSession.threadId)?.status === 'inactive', 'recovered-thread-inactive');
  evidence.checks.recoveryConsumedExactReceiptOnlyAndLeftBlockedThreadIntact = true;

  // ---- Resume after recovery: the saved conversation resumes exactly ----
  await stage('resume-after-recovery');
  const resume = sidebar3.getByRole('button', { name: 'Resume', exact: true });
  await poll(async () => await resume.isEnabled(), 5000, 'resume-unavailable-after-recovery');
  await resume.click();
  await poll(async () => (await sessions()).length === 1, 120000, 'resume-open-deadline');
  const resumedSession = (await sessions())[0];
  require(resumedSession.shell === 'codex' && pathKey(resumedSession.cwd) === pathKey(projectPath), 'resumed-session-shape');
  activeTerminal = resumedSession.id;
  await poll(async () => {
    const value = snapshot(windowSession.threadId);
    return value.activations.some(row => row.instance_id === resumedSession.id && row.ended_at === null);
  }, 30000, 'resume-sealed-binding-deadline');
  const resumedState = snapshot(windowSession.threadId);
  const reopened = resumedState.activations.find(row => row.instance_id === resumedSession.id);
  require(reopened.native_id === windowSession.nativeUuid && resumedState.segments.length === 1, 'resume-same-segment-native-id');
  await ready(resumedSession.id, 'resume-after-recovery');
  await stopTerminal(resumedSession.id);
  await poll(async () => {
    const value = snapshot(windowSession.threadId);
    return reopened && value.activations.find(row => row.instance_id === resumedSession.id)?.ended_at !== null
      && !value.cleanup.some(entry => entry.instance_id === resumedSession.id);
  }, 15000, 'resumed-owner-not-closed');
  evidence.checks.resumeAfterRecoverySealedSameUuidAndClosedCleanly = true;

  require(guardCounts.allowed === 0 && guardCounts.overflow === 0 && providerHits === 0 && pageErrors === 0, 'bounded-guard-and-ui-clean');
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only', osFirewallQualified: false };
  evidence.provider = { hits: providerHits, syntheticLoopbackOnly: true, modelTurnCompleted: false };
  evidence.apps = appRecords.map(record => ({ index: record.index, pid: record.pid, crashed: record.crashedAt !== undefined }));
  await stage('frame-close'); await closeFrame(); require(app === null || app.exitCode === 0, 'owned-app-clean-exit');
  evidence.checks.ownedFrameClosedCleanly = true; evidence.status = 'passed'; evidence.stage = 'complete'; await save();
  console.log('PASS: real forced crashes — pre-receipt crash stays blocked with budget preserved, live host finalizes a killed agent exactly, receipt-window crash recovered once by the real UI, and Resume after recovery kept the same native UUID.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.proofCode || error.ipcCode || `proof-failed-at-${evidence.stage}`;
  evidence.errorMessage = String(error.message).split('\n')[0].slice(0, 300);
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only', osFirewallQualified: false };
  evidence.provider = { hits: providerHits, syntheticLoopbackOnly: true };
  evidence.apps = appRecords.map(record => ({ index: record.index, pid: record.pid, crashed: record.crashedAt !== undefined }));
  evidence.lastSidecarStatus = lastSidecarStatus === null ? 'unobserved'
    : JSON.parse(JSON.stringify(lastSidecarStatus, (key, value) => typeof value === 'string' ? value.slice(0, 200) : value));
  if (page && !page.isClosed()) {
    try {
      const status = await within(ipc('sidecar_status'), 3000, 'diagnostic-deadline');
      evidence.service = status instanceof Error ? 'unavailable' : { phase: status.phase, healthy: status.healthy };
    } catch { evidence.service = { unavailable: true }; }
    try {
      const status = await within(ipc('agent_status'), 3000, 'diagnostic-agent-deadline');
      const facts = status instanceof Error ? null : status.harnesses?.codex;
      evidence.agent = facts
        ? { checking: facts.checking, blocked: facts.blocked,
            startupFailure: typeof facts.startupFailure === 'string' ? facts.startupFailure.replace(/[\r\n]+/g, ' ').slice(0, 240) : null,
            versionProbe: facts.versionProbe ? { issue: facts.versionProbe.issue, elapsedMs: facts.versionProbe.elapsed_ms } : null }
        : 'unavailable';
    } catch { evidence.agent = 'unavailable'; }
    if (activeTerminal) {
      try {
        const text = await within(screen(activeTerminal), 2000, 'classified-screen-deadline');
        evidence.terminalShape = {
          hookReview: /Review hooks|Trust all and continue|Continue without trusting/.test(text),
          composerReady: /Ask Codex to do anything/.test(text),
          apiOrNetworkError: /API Error|Error:|Failed to connect|Unable to connect|ECONN|network error|stream disconnected/i.test(text),
          terminalExited: await pane(activeTerminal).locator('.exit-banner').count() > 0,
        };
      } catch { evidence.terminalShape = { unavailable: true }; }
    }
  }
  await save(); process.exitCode = 1; console.error(`FAIL: ${evidence.failure}`);
} finally {
  if (owned && page && !page.isClosed()) {
    try {
      for (const session of await sessions()) await ipc('terminal_stop', { id: session.id });
      await closeFrame();
    } catch {
      try { process.kill(app.pid); } catch { /* Only this fresh proof app may receive fallback termination. */ }
      try { spawnSync('taskkill', ['/F', '/T', '/PID', String(app.pid)], { windowsHide: true, timeout: 15000 }); } catch { /* best effort */ }
    }
  }
  if (app && app.exitCode === null && app.signalCode === null) { try { process.kill(app.pid); } catch { /* gone */ } }
  await browser?.close().catch(() => {});
  for (const socket of sockets) socket.destroy();
  await new Promise(accept => guard.close(accept));
  await new Promise(accept => provider.close(accept));
  aborted = true;
}
