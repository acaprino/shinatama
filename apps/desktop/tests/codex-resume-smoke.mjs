// Root schedules this native proof. Real app/ConPTY/UI and real Codex 0.167.1;
// synthetic loopback credentials only, external providers refused by the guard.
// Resume must reuse the exact saved native conversation, never fabricate one.
// Never print, persist or attach terminal text, provider bodies or child secrets.
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer, request as httpRequest } from 'node:http';
import { createConnection, createServer as createPortReservation } from 'node:net';
import { mkdir, mkdtemp, readFile, readdir, realpath, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { setTimeout as delay } from 'node:timers/promises';
import { startSession } from './session-flow.mjs';

if (process.platform !== 'win32') throw new Error('windows-required');
const workspace = process.cwd();
const executable = process.env.SHINATAMA_SMOKE_APP_EXE || resolve(workspace, 'target/debug/shinatama-desktop.exe');
if (!isAbsolute(executable)) throw new Error('absolute-app-executable-required');
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-codex-resume-'));
const data = resolve(fixture, 'data'); const projectPath = resolve(fixture, 'synthetic-project');
for (const name of ['synthetic-project', 'home', 'roaming', 'local', 'tmp']) await mkdir(resolve(fixture, name));
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const PROMPT_A = 'SHINATAMA_UI_CODEX_PROMPT_A';
const PROMPT_B = 'SHINATAMA_UI_CODEX_RESUME_PROMPT_B';
const DUMMY_KEY = 'sk-shinatama-synthetic-codex-no-provider-key';
const UUID_PATTERN = /^[a-f0-9]{8}-[a-f0-9]{4}-[1-8][a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
const TRACKED_LIMITS = { jobCommitBytes: 1073741824, processCommitBytes: 805306368, limitsEnforced: false, processes: 8,
  cpuBasisPoints: 1000, queueBytes: 4194304, queueItems: 512 };
const evidence = { check: 'codex-resume-continuity', status: 'running', stage: 'prepare', checks: {},
  scope: 'Actual Shinatama New session, Stop and UI Resume with real Codex 0.167.1, tracked hook recipe, ConPTY and UI against synthetic loopback credentials; every external provider route is refused by a forwarding-only guard. No account, no paid provider, no completed model turn. Resume fidelity means the exact saved native conversation: sealed UUID, prior transcript rendered by Codex itself and its own rollout growing under the same UUID. Hook confirmation is enforced negatively (a source/UUID mismatch retains the cleanup owner and blocks finalization; unit-proven). Proxy forwarding is guarded, not an OS firewall.' };
const save = () => writeFile(resolve(output, 'codex-resume.json'), `${JSON.stringify(evidence, null, 2)}\n`);
const stage = async name => { evidence.stage = name; await save(); };
await save();
const proofError = code => Object.assign(new Error(code), { proofCode: code });
const fail = code => { throw proofError(code); };
const require = (condition, code) => { if (!condition) fail(code); };
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
const inside = (child, parent) => pathKey(child).startsWith(`${pathKey(parent)}\\`);
// Fixed error families only: no native output, paths, URLs, keys or request
// payloads leave this function. The screen itself is already bounded to 32 KiB.
function nativeErrorFamilies(text) {
  return Object.entries({
    connectionFailure: /(?:unable|failed) to connect|fetch failed|network error|connection (?:refused|reset|closed)|stream disconnected/i,
    proxyRefusal: /proxy.{0,80}(?:403|forbidden|refused|failed)|(?:CONNECT|tunnel).{0,80}(?:403|forbidden|refused|failed)/i,
    tlsCertificate: /certificate|CERT_|TLS handshake|SSL error/i,
    timeout: /timed?\s*out|timeout|deadline/i,
    authentication: /invalid (?:API |authentication )?key|authentication (?:failed|error)|unauthorized|401/i,
    forbidden: /forbidden|403/i,
    apiResponse: /API Error|invalid (?:API )?response|unexpected (?:status|response)|error sending request/i,
    websocket: /websocket|wss:/i,
    sessionNotFound: /session.{0,40}not found|no session|unknown session|thread.{0,40}not found/i,
    hookFailure: /hook.{0,80}(?:failed|error|timed out)|SessionStart.{0,40}failed/i,
    processSpawn: /spawn|CreateProcess|failed to start|failed to launch/i,
    permission: /permission denied|access denied|not permitted|EACCES|EPERM/i,
    missingFile: /no such file|file not found|ENOENT/i,
    memory: /out of memory|heap.{0,80}(?:exhausted|limit)|allocation (?:failed|failure)|ENOMEM/i,
    runtimeFailure: /panic|segmentation fault|assertion (?:failed|failure)|unhandled (?:exception|rejection)|TypeError|ReferenceError/i,
    invalidArgument: /unknown option|invalid argument|unrecognized option|invalid value/i,
  }).filter(([, pattern]) => pattern.test(text)).map(([family]) => family);
}
function serviceErrorFamilies(text) {
  if (typeof text !== 'string' || text.length > 2048) return ['unavailable'];
  return Object.entries({
    privateStorage: /private.{0,80}(?:config|storage|directory)|owner lock.{0,80}verification/i,
    resourceAdmission: /resource|process slot|admission|memory/i,
    packageIdentity: /fingerprint|pinned package|runtime inventory|runtime is missing/i,
    anotherOwner: /another Shinatama|already retained|owner is busy/i,
    localAddress: /local proxy address|local proxy port/i,
    preparation: /preparation failed|launch environment/i,
    startup: /confirm startup|could not start|exited twice/i,
    cleanup: /cleanup|saved files were preserved/i,
    qualification: /qualification|version probe|canary|fixed time budget/i,
    tracking: /tracking|binding|session component/i,
  }).filter(([, pattern]) => pattern.test(text)).map(([family]) => family);
}
let aborted = false; const began = Date.now();
function checkDeadline() { if (aborted || Date.now() - began > 420000) fail('overall-proof-deadline'); }
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

// Resolve these fixed prerequisites before opening any listening socket.
// Only executable discovery uses the original profile path; no personal file is read.
const system = process.env.SystemRoot || process.env.SYSTEMROOT;
require(system && process.env.USERPROFILE, 'required-windows-discovery-paths');
let codexDirectory = null;
for (const entry of (process.env.PATH || '').split(';')) {
  if (!entry) continue;
  try { if ((await stat(join(entry, 'codex.exe'))).isFile()) { codexDirectory = resolve(entry); break; } } catch { /* keep searching */ }
}
require(!!codexDirectory, 'installed-codex-required');

// The synthetic provider must never be reached: Codex 0.167.1 has no honored
// loopback base-URL environment override, so the guard refuses every external
// route and any provider hit is a proof failure, not a fallback.
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
// Every outbound connection below names the numeric provider address directly.
// Refused authorities are never resolved, followed or used as socket targets.
const guard = createServer({ maxHeaderSize: 16384 }, (request, response) => {
  request.on('error', () => {}); response.on('error', () => {});
  let target;
  try { if (request.url.length <= 4096) target = new URL(request.url); } catch { /* Refuse malformed absolute targets. */ }
  if (request.socket.remoteAddress !== '127.0.0.1' || target?.protocol !== 'http:'
      || !request.url.startsWith(`http://127.0.0.1:${providerPort}/`)
      || target.host !== `127.0.0.1:${providerPort}` || target.username || target.password || target.hash) {
    guardCounts.deniedAbsolute++; response.writeHead(403, { connection: 'close' }); response.end(); return;
  }
  if (++guardCounts.allowed > 128) { guardCounts.overflow++; request.destroy(); return; }
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
  if (++guardCounts.allowed > 128 || head.length > 16384) { guardCounts.overflow++; client.destroy(); return; }
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
  // Keep the production baseline; CLI --port overrides this baseline port.
  // No provider routes may override global manual proxy (even proxy: "" is direct).
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
  if ($root -ne $value.fixture -or [IO.Path]::GetFileName($root) -notmatch '^shinatama-codex-resume-[A-Za-z0-9]+$') { throw 'fresh-fixture-only' }
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
const environment = {};
for (const name of ['SystemRoot', 'WINDIR', 'SystemDrive', 'COMSPEC', 'PATHEXT', 'PROCESSOR_ARCHITECTURE',
  'NUMBER_OF_PROCESSORS', 'ProgramFiles', 'ProgramFiles(x86)', 'ProgramW6432', 'ProgramData', 'ALLUSERSPROFILE',
  'OS', 'USERNAME', 'USERDOMAIN', 'USERPROFILE', 'HOMEDRIVE', 'HOMEPATH', 'APPDATA', 'LOCALAPPDATA']) {
  const key = Object.keys(process.env).find(key => key.toLowerCase() === name.toLowerCase());
  if (key) environment[name] = process.env[key];
}
Object.assign(environment, { SystemRoot: system, WINDIR: system,
  PATH: [codexDirectory, resolve(system, 'System32')].join(';'),
  // Keep Windows' real OS profile identity for WebView2. The production Codex
  // launcher separately rehomes HOME/USERPROFILE/AppData into its private box.
  HOME: environment.USERPROFILE,
  TEMP: resolve(fixture, 'tmp'), TMP: resolve(fixture, 'tmp'), SHINATAMA_HOME: data,
  WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  // Codex 0.167.1 never accepts this variable as a credential; it only prefills
  // the explicit API-key onboarding entry, which saves it inside the private
  // box. The key is synthetic and every provider route is refused.
  OPENAI_API_KEY: DUMMY_KEY,
});
const app = spawn(executable, [], { cwd: process.env.SHINATAMA_SMOKE_APP_EXE ? dirname(executable) : workspace,
  windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'], env: environment });
let browser, page, activeTerminal = null, owned = false, frameClosed = false, spawnFailed = false, pageErrors = 0;
app.once('error', () => { spawnFailed = true; });
const startup = { connectionRefused: 0, connectionTimeout: 0, otherConnectionFailure: 0,
  stderrBytes: 0, stderrOverflow: false, webviewFailure: false, permissionRefusal: false };
app.stderr.on('data', chunk => {
  startup.stderrBytes += chunk.length;
  if (startup.stderrBytes > 16384) { startup.stderrOverflow = true; return; }
  // Classify in memory; never retain text, command lines, paths or provider data.
  const text = chunk.toString('utf8');
  startup.webviewFailure ||= /(?:webview|browser).{0,80}(?:failed|error)|(?:failed|error).{0,80}(?:webview|browser)/i.test(text);
  startup.permissionRefusal ||= /access.denied|permission.denied|0x80070005/i.test(text);
});
async function ipc(command, args = {}) {
  const retryRead = ['terminal_list', 'terminal_resources', 'resources_status', 'global_workspace_load', 'storage_location', 'project_recent', 'thread_list', 'sidecar_status'].includes(command);
  const deadline = Date.now() + 3000;
  do {
    const result = await within(page.evaluate(async ({ command, args }) => {
      try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; }
      catch (error) { return { ok: false, busy: ['busy', 'unavailable'].includes(error?.code) }; }
    }, { command, args }), 15000, 'metadata-ipc-deadline');
    if (result.ok) return result.value;
    require(retryRead && result.busy, 'metadata-ipc-refused'); await delay(100);
  } while (Date.now() < deadline);
  fail('metadata-ipc-busy-deadline');
}
const sessions = () => ipc('terminal_list');
async function resourceSnapshot(id) {
  const resources = (await ipc('terminal_resources')).find(item => item.id === id);
  if (!resources) return { tracked: false };
  const { phase, reason, observed, applied, queues, sampleAgeMs } = resources;
  return { phase, reason, observed, applied, queues, sampleAgeMs };
}
const pane = id => page.locator(`.im-pane:has([data-view-id="${id}"])`);
const layout = async () => { const row = await ipc('global_workspace_load', { request: { workspaceId: 'main' } }); return row ? JSON.parse(row.layoutJson) : null; };
async function screen(id) {
  const value = await pane(id).locator('.xterm-accessibility-tree').innerText();
  require(value.length <= 32768, 'bounded-visible-terminal'); return value;
}
async function input(id, text, key) {
  activeTerminal = id;
  const field = pane(id).locator('.xterm-helper-textarea'); await field.focus();
  if (text) {
    require(text === PROMPT_A || text === PROMPT_B, 'fixed-synthetic-prompt-only');
    // xterm6 intentionally ignores Input.insertText in screen-reader mode.
    // Actual key events exercise the same input path as a user typing.
    await field.pressSequentially(text, { delay: 5 });
    await poll(async () => (await screen(id)).includes(text), 5000, 'synthetic-prompt-not-visible-before-submit');
  }
  if (key) await page.keyboard.press(key);
}
// The bundled plugins bring hooks that Codex does not trust (ADR-0052, item 7), so the first turn
// of a session stops at "Hooks need review". The proof answers as a user who grants nothing:
// "3. Continue without trusting (hooks won't run)". Shinatama's own hook is trusted by its hash
// and still runs, which is what the binding checks below prove.
const reviewsDeclined = new Set();
async function declineHookReview(id) {
  if (reviewsDeclined.has(id)) return;
  const text = await screen(id);
  if (!/Hooks need review/.test(text) || !/3\. Continue without trusting/.test(text)) return;
  reviewsDeclined.add(id); evidence.checks.pluginHookReviewDeclined = true; await save();
  await input(id, null, '3'); await input(id, null, 'Enter');
}
// Codex onboarding is a fixed, bounded set of product screens. Anything else,
// and in particular any hook-trust review, fails the proof with its own code.
async function ready(id, operation) {
  const handled = new Set(); const allowed = []; let lastShape = '';
  evidence.onboarding ??= {}; evidence.onboarding[operation] = allowed;
  evidence.onboardingShapes ??= {};
  await poll(async () => {
    const text = await screen(id);
    require(await pane(id).locator('.exit-banner').count() === 0, 'native-process-exited-before-readiness');
    const shape = {
      welcome: /Welcome to Codex/.test(text),
      authPick: /Sign in with ChatGPT/.test(text) && /(Provide your own API key|Use an OpenAI API key|Sign in another way)/.test(text),
      apiKeyEntry: /Use your own OpenAI API key/.test(text),
      apiKeyPrefilled: /Detected OPENAI_API_KEY environment variable/.test(text),
      apiKeyConfigured: /API key configured/.test(text),
      trust: /Trust this folder\?/.test(text) && /Trust and continue/.test(text),
      hookReview: /Review hooks|Trust all and continue|Continue without trusting/.test(text),
      deviceCode: /one-time code|verification url|device code/i.test(text) && !/Sign in with Device Code/.test(text),
      sessionPicker: /Select a session|resume.*picker|No sessions found/i.test(text),
      composer: /Ask Codex to do anything/.test(text),
    };
    const encoded = JSON.stringify(shape);
    if (encoded !== lastShape) { lastShape = encoded; evidence.onboardingShapes[operation] = shape; evidence.lastNativeScreen = text.replace(/\s+/g, ' ').slice(0, 1200); await save(); }
    require(!shape.hookReview, 'hook-trust-review-screen-appeared');
    require(!shape.deviceCode, 'unexpected-device-code-screen');
    require(!shape.sessionPicker, 'unexpected-resume-picker-instead-of-exact-session');
    require(handled.size < 3, 'bounded-onboarding-steps');
    if (shape.authPick && !handled.has('auth-pick')) {
      handled.add('auth-pick'); allowed.push('auth-pick'); await save();
      await input(id, null, '3'); await input(id, null, 'Enter');
      return false;
    }
    if (shape.apiKeyEntry && !handled.has('api-key')) {
      handled.add('api-key'); allowed.push('api-key'); await save();
      if (!shape.apiKeyPrefilled) {
        // Fallback only: type the synthetic key when the environment prefill
        // did not appear. The key never leaves the private box or this process.
        await input(id, DUMMY_KEY, null);
      }
      await input(id, null, 'Enter');
      return false;
    }
    if (shape.trust && !handled.has('trust')) {
      handled.add('trust'); allowed.push('trust'); await save();
      // "Trust and continue" is the product's default-highlighted choice.
      await input(id, null, 'Enter');
      return false;
    }
    // Readiness must be visible in the original terminal; no generic Enter loop.
    return shape.composer && /for shortcuts/i.test(text);
  }, 60000, 'unrecognized-or-blocked-native-screen');
  return allowed;
}
async function snapshot(threadId) {
  require(owned && /^t_[a-z2-7]{12}$/.test(threadId), 'owned-thread-snapshot-only');
  const path = resolve(data, 'shinatama.db'); require(inside(await realpath(path), fixture), 'private-db-boundary');
  require((await stat(path)).size <= 32 * 1024 * 1024, 'bounded-private-database');
  const db = new DatabaseSync(path, { readOnly: true });
  try {
    db.exec('PRAGMA query_only=ON; PRAGMA busy_timeout=1000; BEGIN');
    const segments = db.prepare('SELECT s.id,s.thread_id,s.native_id,s.store_root_id,s.store_revision,r.resolved_path FROM segment s JOIN store_root r ON r.id=s.store_root_id AND r.config_revision=s.store_revision WHERE s.thread_id=? ORDER BY s.ordinal LIMIT 3').all(threadId);
    const activations = db.prepare('SELECT activation_sequence,instance_id,owner_pid,CAST(owner_created_at_100ns AS TEXT) AS owner_created,segment_id,native_id,ended_at FROM activation WHERE thread_id=? ORDER BY activation_sequence LIMIT 3').all(threadId);
    for (const [key, maximum, array] of [
      ['operational.native_session_cleanup.v1', 131072, 'entries'],
      ['operational.resource_reservations.v1', 65536, 'slots'],
    ]) {
      const size = db.prepare('SELECT octet_length(value) AS bytes FROM setting WHERE key=?').get(key);
      require(size && size.bytes <= maximum, 'bounded-cleanup-owner-setting');
      const shape = db.prepare('SELECT json_extract(value,\'$.format_version\') AS version,json_array_length(value,?) AS count FROM setting WHERE key=?').get(`$.${array}`, key);
      require(shape.version === 1 && Number.isInteger(shape.count) && shape.count >= 0 && shape.count <= 64, 'bounded-cleanup-owner-records');
    }
    // Keep FILETIME equality in SQLite; JavaScript numbers would round it.
    const cleanupOwners = db.prepare(`SELECT json_extract(entry.value,'$.owner.activation.instanceId') AS instance_id,
      json_type(entry.value,'$.cleanup') AS cleanup_type,
      json_extract(entry.value,'$.owner.reservation.owner.pid') AS host_pid,
      EXISTS(SELECT 1 FROM activation a WHERE a.thread_id=? AND a.instance_id=json_extract(entry.value,'$.owner.activation.instanceId')
        AND a.owner_pid=json_extract(entry.value,'$.owner.activation.pid')
        AND a.owner_created_at_100ns=json_extract(entry.value,'$.owner.activation.createdAt100ns') AND a.ended_at IS NULL) AS exact_activation,
      EXISTS(SELECT 1 FROM setting ledger,json_each(ledger.value,'$.slots') slot WHERE ledger.key='operational.resource_reservations.v1'
        AND json_extract(slot.value,'$.id')=json_extract(entry.value,'$.owner.reservation.id')
        AND json_extract(slot.value,'$.job_key')=json_extract(entry.value,'$.owner.reservation.job_key')
        AND json_extract(slot.value,'$.owner')=json_extract(entry.value,'$.owner.reservation.owner')
        AND json_extract(slot.value,'$.budget')=json_extract(entry.value,'$.owner.reservation.budget')
        AND json_extract(slot.value,'$.state')='active') AS exact_ticket
      FROM setting journal,json_each(journal.value,'$.entries') entry
      WHERE journal.key='operational.native_session_cleanup.v1' LIMIT 64`).all(threadId);
    db.exec('COMMIT'); return { segments, activations, cleanupOwners };
  } finally { db.close(); }
}
function requireActiveCleanupOwner(snapshot, instance) {
  const entries = snapshot.cleanupOwners.filter(entry => entry.instance_id === instance);
  require(entries.length === 1 && entries[0].cleanup_type === 'null' && entries[0].host_pid === app.pid
    && entries[0].exact_activation === 1 && entries[0].exact_ticket === 1, 'active-owner-linked-to-exact-process-and-ticket');
}
// Cross-evidence: Codex's own rollout files for the exact captured UUID inside
// the private box. A resume either appends to the original file or writes a
// second one under the same UUID; both forms are accepted, bounded and scanned
// only for the fixed synthetic prompt markers, never persisted as content.
async function rolloutEvidence(segment, prompts) {
  require(UUID_PATTERN.test(segment.native_id), 'canonical-native-uuid');
  const root = await realpath(segment.resolved_path); require(inside(root, data), 'private-native-root-only');
  const matches = [];
  let visited = 0;
  async function walk(directory, depth) {
    if (depth > 6 || matches.length > 2) return;
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      if (++visited > 4096) return;
      const path = resolve(directory, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (entry.isDirectory()) { await walk(path, depth + 1); continue; }
      if (/^rollout-.*\.jsonl$/.test(entry.name) && entry.name.includes(segment.native_id)) matches.push(path);
    }
  }
  await walk(root, 0);
  require(matches.length >= 1 && matches.length <= 2, 'bounded-exact-native-rollouts');
  const found = Object.fromEntries(prompts.map(prompt => [prompt, false]));
  let total = 0; let metaMatched = 0;
  for (const file of matches) {
    const actual = await realpath(file); require(inside(actual, root), 'private-rollout-boundary');
    const size = (await stat(file)).size; require(size > 0 && size <= 4 * 1024 * 1024, 'bounded-native-rollout');
    total += size;
    const text = await readFile(file, 'utf8');
    for (const prompt of prompts) if (text.includes(prompt)) found[prompt] = true;
    const firstLine = text.split('\n', 1)[0];
    if (firstLine.length <= 65536) {
      try { const meta = JSON.parse(firstLine); if (meta?.payload?.id === segment.native_id || meta?.id === segment.native_id) metaMatched++; } catch { /* not a meta first line */ }
    }
  }
  require(total <= 8 * 1024 * 1024, 'bounded-native-rollout-total');
  require(metaMatched >= 1, 'rollout-session-id-matches-hook-uuid');
  require(prompts.every(prompt => found[prompt]), 'saved-native-turns-in-rollout');
  return { files: matches.length, bytes: total, metaMatched, promptsFound: Object.fromEntries(prompts.map(prompt => [prompt, found[prompt]])) };
}
function exited(milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return within(new Promise(accept => app.once('exit', accept)), milliseconds, 'owned-app-exit-deadline');
}
async function stop(id) {
  await page.locator(`.im-tile-tab[data-view-id="${id}"]`).click();
  await page.locator(`.im-tile-tabwrap:has(.im-tile-tab[data-view-id="${id}"]) .im-tile-tab-close`).click();
  await page.locator('#confirm-dialog').getByRole('button', { name: 'Stop', exact: true }).click();
  await poll(async () => !(await sessions()).some(session => session.id === id), 15000, 'owned-terminal-cleanup-deadline');
}
async function closeFrame() {
  const closing = Promise.all([page.waitForEvent('close', { timeout: 15000 }), exited(15000)]); void closing.catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click(); await closing; frameClosed = true;
}
try {
  await stage('app-start');
  await poll(async () => {
    require(!spawnFailed && app.exitCode === null, 'owned-app-startup-failed');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); return true; }
    catch (error) {
      const text = String(error.message);
      if (/ECONNREFUSED|connection refused/i.test(text)) startup.connectionRefused++;
      else if (/timeout|timed out/i.test(text)) startup.connectionTimeout++;
      else startup.otherConnectionFailure++;
      return false;
    }
  }, 30000, 'owned-webview-startup-deadline');
  await poll(async () => {
    page = browser.contexts().flatMap(context => context.pages()).find(candidate => {
      try { const url = new URL(candidate.url()); return url.hostname === 'tauri.localhost' && ['/', '/index.html'].includes(url.pathname); }
      catch { return false; }
    }); return !!page;
  }, 30000, 'main-workspace-webview-missing');
  page.setDefaultTimeout(15000); page.on('pageerror', () => { pageErrors++; });
  await poll(async () => (await page.locator('#runtime-status').textContent()).includes('Local runtime'), 10000, 'runtime-unavailable');
  // Open refuses while the sidecar is still starting; start it through the
  // real UI command seam when stopped, then wait for its qualified ready phase
  // instead of racing the fixed five-second preparation window.
  if ((await ipc('sidecar_status')).phase === 'stopped') await ipc('sidecar_start').catch(() => {});
  await poll(async () => { const status = await ipc('sidecar_status'); return status.phase === 'ready' && status.healthy === true; }, 45000, 'sidecar-not-ready-before-open');
  require(pathKey((await ipc('storage_location')).directory) === pathKey(data), 'private-app-storage-mismatch'); owned = true;
  require((await sessions()).length === 0, 'no-session-before-user-action');
  await stage('first-session-open');
  const flow = await startSession(page, { project: projectPath, harness: 'codex' });
  await poll(async () => { require(!await flow.refusal(), 'open-session-inline-refusal'); return !await flow.pending(); }, 120000, 'new-session-deadline');
  const openedList = await sessions(); require(openedList.length === 1, 'expected-terminal-count');
  const first = openedList[0];
  require(first?.shell === 'codex' && pathKey(first.cwd) === pathKey(projectPath), 'exact-synthetic-codex-session');
  activeTerminal = first.id;
  await poll(async () => !!(await layout())?.views.find(view => view.id === first.id)?.resource.threadId, 15000, 'saved-thread-view-binding');
  const threadId = (await layout()).views.find(view => view.id === first.id).resource.threadId;
  const project = (await ipc('project_recent')).find(item => pathKey(item.path) === pathKey(projectPath)); require(project, 'saved-project');
  const rows = async () => (await ipc('thread_list', { request: { projectId: project.id, cursor: null, limit: 50 } })).rows;
  await stage('first-onboarding'); await ready(first.id, 'first');
  const preTurn = await snapshot(threadId);
  require(preTurn.segments.length === 0 && preTurn.activations.length === 0, 'no-native-identity-before-first-turn');
  require((await rows()).find(row => row.id === threadId)?.status === 'unbound', 'thread-unbound-before-first-turn');
  await stage('first-prompt');
  await input(first.id, PROMPT_A, 'Enter');
  await poll(async () => { await declineHookReview(first.id); return (await snapshot(threadId)).activations.length === 1; }, 60000, 'first-native-binding-deadline');
  const bound = await snapshot(threadId);
  require(bound.segments.length === 1 && UUID_PATTERN.test(bound.segments[0].native_id), 'first-segment-native-uuid');
  require(bound.activations[0].instance_id === first.id && bound.activations[0].native_id === bound.segments[0].native_id
    && bound.activations[0].ended_at === null, 'first-exact-native-owner');
  requireActiveCleanupOwner(bound, first.id);
  const nativeUuid = bound.segments[0].native_id;
  const segmentId = bound.segments[0].id;
  evidence.checks.firstSessionStartHookDeliveredExactNativeUuid = true;
  evidence.rolloutFirst = await rolloutEvidence(bound.segments[0], [PROMPT_A]);
  await poll(async () => guardCounts.deniedConnect + guardCounts.deniedAbsolute >= 1, 45000, 'no-external-route-refused');
  require(guardCounts.allowed === 0 && guardCounts.overflow === 0 && providerHits === 0, 'synthetic-provider-never-reachable');
  await stage('stop-first'); await delay(5000); await stop(first.id);
  await poll(async () => (await rows()).find(row => row.id === threadId)?.status === 'inactive', 10000, 'first-thread-not-inactive');
  const stopped = await snapshot(threadId);
  require(stopped.activations.length === 1 && stopped.activations[0].ended_at !== null, 'first-owner-history-ended');
  require(!stopped.cleanupOwners.some(entry => entry.instance_id === first.id), 'first-cleanup-receipt-consumed-after-stop');
  evidence.checks.stopClosedOwnerAndConsumedCleanupReceipt = true;
  // The functional resume leg: one click on the saved, closed thread in the UI.
  await stage('resume-button');
  const sidebar = page.locator('.thread-sidebar'); /* The list reloads itself on thread-changed; the Reload control is hidden in the simple workspace. */
  const anchor = sidebar.locator(`button.thread-sidebar-anchor[data-thread-id="${threadId}"]`);
  await poll(async () => (await anchor.locator('.thread-sidebar-state').getAttribute('data-state')) === 'resumable', 5000, 'resume-action-unavailable-for-codex');
  require((await sessions()).length === 0, 'thread-listing-autostarted');
  await anchor.click(); /* a click on a closed thread resumes it */
  await poll(async () => (await sessions()).length === 1, 120000, 'resume-open-deadline');
  const resumed = (await sessions())[0];
  require(resumed.id !== first.id && resumed.shell === 'codex' && pathKey(resumed.cwd) === pathKey(projectPath), 'new-resumed-owner-same-project');
  activeTerminal = resumed.id;
  await poll(async () => (await layout())?.views.find(view => view.id === resumed.id)?.resource.threadId === threadId, 15000, 'resume-reused-thread-view');
  // Resume seals the exact saved UUID before any hook: activation #2 is open
  // as soon as the launch is admitted, on the same single segment.
  await poll(async () => (await snapshot(threadId)).activations.length === 2, 30000, 'resume-sealed-binding-deadline');
  const sealed = await snapshot(threadId);
  require(sealed.segments.length === 1 && sealed.segments[0].id === segmentId && sealed.segments[0].native_id === nativeUuid, 'resume-same-segment-native-id');
  require(sealed.activations[1].instance_id === resumed.id && sealed.activations[1].native_id === nativeUuid
    && sealed.activations[1].ended_at === null && sealed.activations[1].owner_created !== sealed.activations[0].owner_created, 'resume-exact-second-owner');
  requireActiveCleanupOwner(sealed, resumed.id);
  require((await rows()).find(row => row.id === threadId)?.status === 'recorded_open', 'thread-recorded-open-after-resume');
  evidence.checks.uiResumeSealedExactSavedUuidBeforeAnyHook = true;
  await stage('resume-onboarding'); const resumeAllowed = await ready(resumed.id, 'resume');
  evidence.checks.boxCredentialsAndTrustPersistedWithoutRepeatOnboarding = resumeAllowed.length === 0;
  // Native continuity: Codex itself renders the saved conversation, including
  // the first turn's synthetic prompt, before anything new is typed.
  await stage('resume-transcript');
  await poll(async () => (await screen(resumed.id)).includes(PROMPT_A), 30000, 'resumed-transcript-missing-prior-conversation');
  evidence.checks.resumedNativeTranscriptShowsPriorConversation = true;
  await stage('resume-prompt');
  await input(resumed.id, PROMPT_B, 'Enter');
  // The resumed turn fires SessionStart(source=resume); the lifecycle accepts
  // only the exact sealed UUID. A mismatch would fail tracking and retain the
  // cleanup owner, so the clean stop below is the confirmation evidence.
  await poll(async () => {
    await declineHookReview(resumed.id);
    const value = await snapshot(threadId);
    return value.activations.length === 2 && value.activations[1].ended_at === null
      && value.segments.length === 1 && value.segments[0].native_id === nativeUuid;
  }, 30000, 'resume-binding-disturbed-after-prompt');
  const confirmed = await snapshot(threadId);
  requireActiveCleanupOwner(confirmed, resumed.id);
  const running = await resourceSnapshot(resumed.id);
  evidence.resources ??= {}; evidence.resources.afterResumePrompt = running;
  require(running.phase === 'running' && running.reason !== 'resource_limit', 'tracked-job-remains-within-limits');
  require(!!running.applied && Object.entries(TRACKED_LIMITS).every(([key, value]) => running.applied[key] === value), 'applied-limits-match-tracked-profile');
  await poll(async () => {
    try { const value = await rolloutEvidence(confirmed.segments[0], [PROMPT_A, PROMPT_B]); evidence.rolloutResume = value; return true; }
    catch { return false; }
  }, 30000, 'resumed-rollout-missing-second-turn');
  evidence.checks.codexOwnRolloutGrewUnderTheSameNativeUuid = true;
  await stage('stop-resumed'); await delay(5000); await stop(resumed.id);
  await poll(async () => (await rows()).find(row => row.id === threadId)?.status === 'inactive', 10000, 'resumed-thread-not-inactive');
  const final = await snapshot(threadId);
  require(final.activations.length === 2 && final.activations.every(row => row.ended_at !== null), 'two-closed-native-intervals');
  require(!final.cleanupOwners.some(entry => [first.id, resumed.id].includes(entry.instance_id)), 'both-cleanup-receipts-consumed-after-stop');
  require((await rows()).length === 1 && (await sessions()).length === 0, 'one-thread-no-extra-terminal');
  evidence.checks.resumeConfirmedAndSecondOwnerClosedWithReceiptConsumed = true;
  require(guardCounts.allowed === 0 && guardCounts.overflow === 0 && providerHits === 0 && pageErrors === 0, 'bounded-guard-and-ui-clean');
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only', osFirewallQualified: false };
  evidence.provider = { hits: providerHits, syntheticLoopbackOnly: true, modelTurnCompleted: false };
  await stage('frame-close'); await closeFrame(); require(app.exitCode === 0, 'owned-app-clean-exit');
  evidence.checks.ownedFrameAndSessionsClosed = true; evidence.status = 'passed'; evidence.stage = 'complete'; await save();
  console.log('PASS: actual Codex 0.167.1 UI Resume kept the exact saved native UUID, rendered the prior conversation, confirmed source=resume and closed its owner with a consumed receipt.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.proofCode || `proof-failed-at-${evidence.stage}`;
  // First line only; Playwright locator errors carry no secrets or native text.
  evidence.errorMessage = String(error.message).split('\n')[0].slice(0, 300);
  evidence.startup = startup;
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only', osFirewallQualified: false };
  evidence.provider = { hits: providerHits, syntheticLoopbackOnly: true };
  if (owned && page && !page.isClosed()) {
    try {
      const resources = await within(ipc('resources_status'), 3000, 'diagnostic-resource-deadline');
      const numbers = ['completed_cycles', 'cycle_age_ms', 'observed_processes', 'measured_processes',
        'host_private_commit_bytes', 'owned_private_commit_bytes', 'system_remaining_commit_bytes', 'system_available_physical_bytes'];
      evidence.hostResources = Object.fromEntries(numbers.map(key => [key,
        Number.isSafeInteger(resources[key]) && resources[key] >= 0 ? resources[key] : null]));
      evidence.hostResources.complete = resources.complete === true;
    } catch { evidence.hostResources = { unavailable: true }; }
    try {
      const inline = page.locator('#new-session-error:visible, .im-notice[data-im-tone="err"] .im-notice-text').first();
      if (await inline.isVisible()) {
        const text = await inline.innerText();
        evidence.openErrorFamilies = serviceErrorFamilies(text);
        // Product-owned static refusal message; same practice as new-session-smoke.
        evidence.openError = text.replace(/[\r\n]+/g, ' ').slice(0, 512);
      }
    } catch { evidence.openErrorFamilies = ['unavailable']; }
    try {
      const status = await within(ipc('agent_status'), 3000, 'diagnostic-agent-deadline');
      const facts = status.harnesses?.codex;
      evidence.agent = { checking: facts?.checking, blocked: facts?.blocked,
        startupFailure: typeof facts?.startupFailure === 'string' ? facts.startupFailure.replace(/[\r\n]+/g, ' ').slice(0, 240) : null,
        versionProbe: facts?.versionProbe ? { issue: facts.versionProbe.issue, elapsedMs: facts.versionProbe.elapsed_ms } : null };
    } catch { evidence.agent = 'unavailable'; }
    if (activeTerminal) {
      try {
        const text = await within(screen(activeTerminal), 2000, 'classified-screen-deadline');
        evidence.terminalShape = {
          errorFamilies: nativeErrorFamilies(text),
          errorCodes: ['ECONNREFUSED', 'ECONNRESET', 'ENOTFOUND', 'ETIMEDOUT', 'EPIPE', 'EACCES', 'EPERM', 'ENOENT', 'ENOMEM'].filter(code => text.includes(code)),
          promptAVisible: text.includes(PROMPT_A), promptBVisible: text.includes(PROMPT_B),
          hookReview: /Review hooks|Trust all and continue|Continue without trusting/.test(text),
          authPick: /Sign in with ChatGPT/.test(text),
          apiKeyEntry: /Use your own OpenAI API key/.test(text),
          trustScreen: /Trust this folder\?/.test(text),
          sessionPicker: /Select a session|No sessions found/i.test(text),
          composerReady: /Ask Codex to do anything/.test(text),
          apiOrNetworkError: /API Error|Error:|Failed to connect|Unable to connect|ECONN|network error|stream disconnected/i.test(text),
          permissionError: /permission denied|access denied|not permitted/i.test(text),
          terminalExited: await pane(activeTerminal).locator('.exit-banner').count() > 0,
        };
        const banner = await pane(activeTerminal).locator('.exit-banner > span').allTextContents();
        const code = banner.join('').match(/code (\d+)/)?.[1];
        evidence.terminalShape.exitCode = code === undefined ? null : Number(code);
        evidence.resources ??= {};
        evidence.resources.failure = await resourceSnapshot(activeTerminal);
      } catch { evidence.terminalShape = { unavailable: true }; }
    }
    try {
      const service = await within(ipc('sidecar_status'), 3000, 'diagnostic-deadline');
      evidence.service = { phase: service.phase, healthy: service.healthy, errorFamilies: serviceErrorFamilies(service.message),
        reason: typeof service.observation?.reason === 'string' && /^[a-z_]{1,48}$/.test(service.observation.reason) ? service.observation.reason : null };
    } catch { evidence.service = { unavailable: true }; }
  }
  await save(); process.exitCode = 1; console.error(`FAIL: ${evidence.failure}`);
} finally {
  if (owned && page && !page.isClosed() && !frameClosed) {
    try {
      for (const session of await sessions()) await ipc('terminal_stop', { id: session.id });
      const dialog = page.locator('#new-session-dialog');
      if (await dialog.isVisible() && await dialog.getByRole('button', { name: 'Cancel', exact: true }).isEnabled()) await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
      await closeFrame();
    } catch { /* Only this fresh proof app may receive fallback termination. */ }
  }
  if (app.exitCode === null && app.signalCode === null) { app.kill(); await exited(3000).catch(() => {}); }
  await browser?.close().catch(() => {});
  for (const socket of sockets) socket.destroy();
  await new Promise(accept => guard.close(accept));
  await new Promise(accept => provider.close(accept));
  aborted = true;
}
