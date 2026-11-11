// Root schedules this native proof. Real app/ConPTY/UI; synthetic local provider only.
// Concurrent duplicate Resume attempts must never bind the same saved session twice.
// Never print, persist or attach terminal text, provider bodies or child secrets.
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer, request as httpRequest } from 'node:http';
import { createConnection, createServer as createPortReservation } from 'node:net';
import { mkdir, mkdtemp, readFile, readdir, realpath, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('windows-required');
const workspace = process.cwd();
const executable = process.env.SHINATAMA_SMOKE_APP_EXE || resolve(workspace, 'target/debug/shinatama-desktop.exe');
if (!isAbsolute(executable)) throw new Error('absolute-app-executable-required');
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-claude-race-'));
const data = resolve(fixture, 'data'); const projectPath = resolve(fixture, 'synthetic-project');
for (const name of ['synthetic-project', 'home', 'roaming', 'local', 'tmp']) await mkdir(resolve(fixture, name));
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const PROMPT_A = 'SHINATAMA_UI_START_PROMPT_A'; const MARKER_A = 'SHINATAMA_UI_START_REPLY_A';
const PROMPT_B = 'SHINATAMA_UI_RESUME_PROMPT_B'; const MARKER_B = 'SHINATAMA_UI_RESUME_REPLY_B';
const DUMMY_KEY = 'shinatama-synthetic-ui-no-provider-key';
const evidence = { check: 'claude-resume-race', status: 'running', stage: 'prepare', checks: {},
  scope: 'Actual Shinatama Claude thread with two same-tick concurrent thread_resume IPC attempts, refused-source variants and one functional UI Resume, against a synthetic local HTTP provider through a refusing upstream guard; no account or paid provider. Cross-process double binding is architecturally excluded by the data-owner lock and the activation uniqueness indexes (unit-proven); this proof exercises the concurrent-request leg at the real IPC/service seam. Proxy forwarding is guarded, not an OS firewall.' };
const save = () => writeFile(resolve(output, 'claude-resume-race.json'), `${JSON.stringify(evidence, null, 2)}\n`);
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
    anthropicStartupConnection: /(?:unable|failed) to connect to Anthropic (?:services|API)/i,
    connectionFailure: /(?:unable|failed) to connect|fetch failed|network error|connection (?:refused|reset|closed)/i,
    proxyRefusal: /proxy.{0,80}(?:403|forbidden|refused|failed)|(?:CONNECT|tunnel).{0,80}(?:403|forbidden|refused|failed)/i,
    tlsCertificate: /certificate|CERT_|TLS handshake|SSL error/i,
    timeout: /timed?\s*out|timeout|deadline/i,
    authentication: /invalid (?:API |authentication )?key|authentication (?:failed|error)|unauthorized|API Error:\s*401/i,
    forbidden: /forbidden|API Error:\s*403/i,
    apiResponse: /API Error|invalid (?:API )?response|unexpected (?:status|response)/i,
    processSpawn: /spawn|CreateProcess|failed to start|failed to launch/i,
    gitBash: /git.bash|CLAUDE_CODE_GIT_BASH_PATH|bash\.exe/i,
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
function checkDeadline() { if (aborted || Date.now() - began > 300000) fail('overall-proof-deadline'); }
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
const claudeDirectory = resolve(process.env.USERPROFILE, '.local/bin');
require((await stat(resolve(claudeDirectory, 'claude.exe'))).isFile(), 'installed-claude-required');
const bash = 'C:\\Program Files\\Git\\usr\\bin\\bash.exe';
require((await stat(bash)).isFile(), 'installed-git-bash-required');

// Fixed provider protocol. Other requests receive a refusal, never a redirect.
let providerPhase = 'idle'; const requests = { start: 0, resume: 0, invalid: 0, wrongCredentials: 0, overflow: 0 };
let previousContextObserved = false; const sockets = new Set(); const guardedProviderPorts = new Set();
const hasText = (messages, role, text) => Array.isArray(messages) && messages.some(message => message?.role === role &&
  (typeof message.content === 'string' ? message.content.includes(text) : Array.isArray(message.content) &&
    message.content.some(block => block?.type === 'text' && typeof block.text === 'string' && block.text.includes(text))));
function respond(response, model, marker, operation, stream) {
  const message = { id: `msg_shinatama_race_${operation}`, type: 'message', role: 'assistant', model,
    content: [{ type: 'text', text: marker }], stop_reason: 'end_turn', stop_sequence: null,
    usage: { input_tokens: 7, output_tokens: 8 } };
  if (!stream) { response.setHeader('content-type', 'application/json'); response.end(JSON.stringify(message)); return; }
  response.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache', connection: 'close' });
  for (const event of [
    { type: 'message_start', message: { ...message, content: [], stop_reason: null, usage: { input_tokens: 7, output_tokens: 0 } } },
    { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } },
    { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: marker } },
    { type: 'content_block_stop', index: 0 },
    { type: 'message_delta', delta: { stop_reason: 'end_turn', stop_sequence: null }, usage: message.usage },
    { type: 'message_stop' },
  ]) response.write(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`);
  response.end();
}
const provider = createServer((request, response) => {
  request.on('error', () => {}); response.on('error', () => {});
  if (request.socket.remoteAddress !== '127.0.0.1' || !guardedProviderPorts.has(request.socket.remotePort)) {
    requests.invalid++; response.writeHead(403); response.end(); return;
  }
  let body = Buffer.alloc(0); request.setTimeout(3000, () => request.destroy());
  request.on('data', chunk => {
    if (body.length + chunk.length > 512 * 1024) { requests.overflow++; request.destroy(); }
    else body = Buffer.concat([body, chunk]);
  });
  request.on('end', () => {
    try {
      if (request.headers['x-api-key'] !== DUMMY_KEY && request.headers.authorization !== `Bearer ${DUMMY_KEY}`) {
        requests.wrongCredentials++; response.writeHead(401); response.end(); return;
      }
      const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
      if (pathname === '/v1/messages/count_tokens') { response.setHeader('content-type', 'application/json'); response.end('{"input_tokens":7}'); return; }
      if (pathname === '/v1/models') { response.setHeader('content-type', 'application/json'); response.end(JSON.stringify({
        data: [{ id: 'claude-sonnet-4-6', type: 'model', display_name: 'Synthetic local model', created_at: '2026-01-01T00:00:00Z' }], has_more: false })); return; }
      if (pathname !== '/v1/messages' || request.method !== 'POST') { requests.invalid++; response.writeHead(404); response.end(); return; }
      const value = JSON.parse(body); const resume = providerPhase === 'resume';
      const valid = providerPhase === 'start' ? hasText(value.messages, 'user', PROMPT_A)
        : resume && hasText(value.messages, 'assistant', MARKER_A) && hasText(value.messages, 'user', PROMPT_B);
      if (!valid || requests[providerPhase] !== 0) { requests.invalid++; response.writeHead(409); response.end(); return; }
      requests[providerPhase]++; if (resume) previousContextObserved = true;
      respond(response, value.model, resume ? MARKER_B : MARKER_A, providerPhase, value.stream === true);
    } catch { requests.invalid++; if (!response.headersSent) response.writeHead(400); response.end(); }
  });
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
  if ($root -ne $value.fixture -or [IO.Path]::GetFileName($root) -notmatch '^shinatama-claude-race-[A-Za-z0-9]+$') { throw 'fresh-fixture-only' }
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
  PATH: [claudeDirectory, dirname(bash), resolve(system, 'System32')].join(';'),
  // Keep Windows' real OS profile identity for WebView2. The production Claude
  // launcher separately rehomes HOME/USERPROFILE/AppData into its private box.
  HOME: environment.USERPROFILE,
  TEMP: resolve(fixture, 'tmp'), TMP: resolve(fixture, 'tmp'), SHINATAMA_HOME: data,
  WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview-profile'),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  // Normal custom-gateway bearer authentication avoids the first-party OAuth
  // onboarding preflight. The token is synthetic and accepted only by this local
  // fixture; this does not qualify a real account or first-party sign-in.
  ANTHROPIC_AUTH_TOKEN: DUMMY_KEY, ANTHROPIC_BASE_URL: `http://127.0.0.1:${providerPort}`, ANTHROPIC_MODEL: 'claude-sonnet-4-6',
  CLAUDE_CODE_GIT_BASH_PATH: bash, CLAUDE_CODE_MAX_RETRIES: '0', API_TIMEOUT_MS: '5000',
  CLAUDE_CODE_GZIP_REQUEST_BODIES: '0', CLAUDE_CODE_MAX_OUTPUT_TOKENS: '128', MAX_THINKING_TOKENS: '0',
  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1', DISABLE_TELEMETRY: '1', DISABLE_ERROR_REPORTING: '1', DISABLE_AUTOUPDATER: '1',
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
// One thread_resume attempt; resolves to a bounded outcome, never throws.
const resumeAttempt = request => within(page.evaluate(async ({ request }) => {
  const at = Date.now();
  try {
    const value = await window.__TAURI_INTERNALS__.invoke('thread_resume', { request });
    return { at, ok: true, sessionId: value?.session?.id ?? null, shell: value?.session?.shell ?? null,
      cwd: value?.session?.cwd ?? null, threadId: value?.thread_id ?? value?.threadId ?? null, trial: value?.trial ?? null };
  } catch (error) {
    return { at, ok: false, code: typeof error?.code === 'string' && /^[a-z_]{1,32}$/.test(error.code) ? error.code : 'unknown' };
  }
}, { request }), 120000, 'resume-attempt-deadline');
const sessions = () => ipc('terminal_list');
const pane = id => page.locator(`.wm-pane:has([data-view-id="${id}"])`);
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
function selectedOption(text, label) {
  return text.split('\n').some(line => /^[\s]*[❯›▸>]\s*(?:\d+[.)]\s*)?/.test(line)
    && label.test(line.replace(/^[\s]*[❯›▸>]\s*(?:\d+[.)]\s*)?/, '')));
}
async function chooseYes(id, expectedTitle, yes) {
  let text = await screen(id); require(expectedTitle.test(text), 'expected-synthetic-consent-screen');
  if (!selectedOption(text, yes)) {
    require(selectedOption(text, /^No\b/), 'known-negative-option-required');
    await input(id, null, 'ArrowUp');
    await poll(async () => {
      text = await screen(id);
      require(expectedTitle.test(text), 'consent-screen-changed-before-confirmation');
      return selectedOption(text, yes);
    }, 3000, 'positive-option-not-selected');
  }
  require(selectedOption(await screen(id), yes), 'positive-option-must-be-visible');
  await input(id, null, 'Enter');
}
async function ready(id, operation) {
  const handled = new Set(); const allowed = []; let lastShape = '';
  evidence.onboarding ??= {}; evidence.onboarding[operation] = allowed;
  evidence.onboardingShapes ??= {};
  await poll(async () => {
    const text = await screen(id);
    require(await pane(id).locator('.exit-banner').count() === 0, 'native-process-exited-before-readiness');
    const shape = {
      customKey: /Detected a custom API key in your environment/.test(text) && /Do you want to use this API key\?/.test(text),
      loginChoices: /Select login method:/.test(text),
      loginCode: /Log in with your Claude account|Paste (?:the )?(?:authentication|authorization) code/i.test(text),
      theme: /Choose the text style that looks best with your terminal/.test(text),
      folderTrust: /Do you trust the files in this folder\?|Is this a project you created or one you trust\?/.test(text),
      securityNotes: /Security notes:/.test(text),
    };
    const encoded = JSON.stringify(shape);
    if (encoded !== lastShape) { lastShape = encoded; evidence.onboardingShapes[operation] = shape; await save(); }
    const choices = [
      ['theme', /Choose the text style that looks best with your terminal/, /Dark mode/, 'Enter'],
      ['api-key', /Detected a custom API key in your environment/, /Do you want to use this API key\?/, 'Yes'],
      ['folder-trust', /Do you trust the files in this folder\?|Is this a project you created or one you trust\?/, /Yes, I trust this folder/, 'Yes'],
      ['security-notes', /Security notes:/, /Press Enter to continue/, 'Enter'],
    ];
    for (const [name, title, option, action] of choices) {
      if (!title.test(text) || !option.test(text)) continue;
      if (handled.has(name)) return false;
      require(handled.size < 4, 'bounded-onboarding-steps'); handled.add(name); allowed.push(name);
      await save();
      if (action === 'Enter') await input(id, null, 'Enter');
      else {
        // This fixture supplies only its dummy key and verifies the native cwd.
        // Arrow navigation changes the actual menu selection; input-event-only
        // insertion does not exercise xterm's screen-reader keyboard path.
        const yes = name === 'api-key' ? /^Yes\b/ : /^Yes, I trust this folder\b/;
        await chooseYes(id, title, yes);
      }
      return false;
    }
    require(!shape.loginChoices && !shape.loginCode, 'unexpected-authentication-screen');
    // Readiness must be visible in the original terminal; no generic Enter loop.
    return /(?:\? for shortcuts|Try "|Try “|bypass permissions off|shift\+tab to cycle|for shortcuts)/i.test(text)
      && /[❯>]/.test(text);
  }, 30000, 'unrecognized-or-blocked-native-screen');
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
async function nativeFile(segment, bothTurns) {
  require(/^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/.test(segment.native_id), 'canonical-native-uuid');
  const root = await realpath(segment.resolved_path); require(inside(root, data), 'private-native-root-only');
  const projects = resolve(root, 'projects'); const entries = await readdir(projects, { withFileTypes: true });
  require(entries.length <= 32, 'bounded-private-projects'); const files = [];
  for (const entry of entries) {
    require(!entry.isSymbolicLink(), 'ordinary-private-projects'); if (!entry.isDirectory()) continue;
    const candidate = resolve(projects, entry.name, `${segment.native_id}.jsonl`);
    try { const actual = await realpath(candidate); require(inside(actual, root), 'private-transcript-boundary'); files.push(actual); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  require(files.length === 1, 'one-exact-native-transcript');
  require((await stat(files[0])).size <= 1024 * 1024, 'bounded-private-transcript');
  const bytes = await readFile(files[0]); require(bytes.length <= 1024 * 1024, 'bounded-private-transcript');
  const lines = bytes.toString('utf8').split('\n').filter(Boolean); require(lines.length <= 128, 'bounded-transcript-records');
  const markers = new Set();
  for (const line of lines) {
    require(Buffer.byteLength(line) <= 256 * 1024, 'bounded-transcript-line'); const value = JSON.parse(line);
    if (!['user', 'assistant'].includes(value.type)) continue;
    require(value.sessionId === segment.native_id, 'native-transcript-id');
    for (const [role, marker] of [['user', PROMPT_A], ['assistant', MARKER_A], ['user', PROMPT_B], ['assistant', MARKER_B]]) {
      if (hasText([{ role: value.type, content: value.message?.content }], role, marker)) markers.add(marker);
    }
  }
  require(markers.has(PROMPT_A) && markers.has(MARKER_A) && (!bothTurns || markers.has(PROMPT_B) && markers.has(MARKER_B)), 'saved-native-turns');
  return files[0];
}
function exited(milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return within(new Promise(accept => app.once('exit', accept)), milliseconds, 'owned-app-exit-deadline');
}
async function stop(id) {
  await page.locator(`.wm-tab[data-view-id="${id}"]`).click();
  await page.locator(`.wm-tabwrap:has(.wm-tab[data-view-id="${id}"]) .wm-tab-close`).click();
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
  await stage('new-session');
  await page.locator('#new-terminal').click(); const dialog = page.locator('#new-session-dialog');
  await dialog.locator('#new-session-project').fill(projectPath); await dialog.locator('#new-session-harness').selectOption('claude_code');
  await dialog.getByRole('button', { name: 'Open', exact: true }).click();
  await poll(async () => { require(!await dialog.locator('#new-session-error').isVisible(), 'open-session-inline-refusal'); return !await dialog.isVisible(); }, 90000, 'new-session-deadline');
  const opened = await sessions(); require(opened.length === 1, 'one-new-terminal');
  const first = opened[0]; require(first?.shell === 'claude_code' && pathKey(first.cwd) === pathKey(projectPath), 'exact-synthetic-claude-session');
  activeTerminal = first.id;
  await poll(async () => !!(await layout())?.views.find(view => view.id === first.id)?.resource.threadId, 15000, 'saved-thread-view-binding');
  const threadId = (await layout()).views.find(view => view.id === first.id).resource.threadId;
  const project = (await ipc('project_recent')).find(item => pathKey(item.path) === pathKey(projectPath)); require(project, 'saved-project');
  const rows = async () => (await ipc('thread_list', { request: { projectId: project.id, cursor: null, limit: 50 } })).rows;
  await stage('first-onboarding'); await ready(first.id, 'start');
  providerPhase = 'start'; await stage('first-prompt'); await input(first.id, PROMPT_A, 'Enter');
  await poll(async () => requests.start === 1 && (await screen(first.id)).includes(MARKER_A), 30000, 'first-marker-not-observed');
  await poll(async () => (await snapshot(threadId)).activations.length === 1, 10000, 'first-activation-not-recorded');
  const initial = await snapshot(threadId); require(initial.segments.length === 1 && initial.activations[0].instance_id === first.id && initial.activations[0].ended_at === null, 'first-exact-native-owner');
  requireActiveCleanupOwner(initial, first.id);
  const nativeUuid = initial.segments[0].native_id;
  await stage('stop-first'); await stop(first.id);
  await poll(async () => (await rows()).find(row => row.id === threadId)?.status === 'inactive', 10000, 'first-thread-not-inactive');
  const firstFile = await nativeFile(initial.segments[0], false);
  const stopped = await snapshot(threadId); require(stopped.activations.length === 1 && stopped.activations[0].ended_at !== null, 'first-owner-history-not-ended');
  require(!stopped.cleanupOwners.some(entry => entry.instance_id === first.id), 'first-cleanup-receipt-consumed-after-stop');
  evidence.checks.resumableThreadWithClosedFirstOwner = true;
  const thread = (await rows()).find(row => row.id === threadId); require(thread, 'saved-thread-row');
  const baseRequest = { threadId: thread.id, projectId: thread.projectId, expectedRevision: thread.revision, rows: 24, cols: 80 };
  // Refused-source variants while the thread is inactive: a foreign project id
  // and a stale revision must never reach a launch.
  await stage('refusals');
  const wrongProject = await resumeAttempt({ ...baseRequest, projectId: 'p_zzzzzzzzzzzz' });
  require(!wrongProject.ok && wrongProject.code === 'source_changed', 'foreign-project-resume-refused');
  const wrongRevision = await resumeAttempt({ ...baseRequest, expectedRevision: thread.revision + 1 });
  require(!wrongRevision.ok && wrongRevision.code === 'source_changed', 'stale-revision-resume-refused');
  require((await sessions()).length === 0, 'refused-resumes-started-nothing');
  evidence.refusals = { wrongProject: wrongProject.code, wrongRevision: wrongRevision.code };
  evidence.checks.refusedSourceVariantsStartedNothing = true;
  // The race: two same-tick thread_resume attempts on one saved thread.
  await stage('race');
  const raced = await within(page.evaluate(async ({ request }) => {
    const at = Date.now();
    const attempt = async () => {
      try {
        const value = await window.__TAURI_INTERNALS__.invoke('thread_resume', { request });
        return { at: Date.now() - at, ok: true, sessionId: value?.session?.id ?? null, shell: value?.session?.shell ?? null,
          cwd: value?.session?.cwd ?? null, threadId: value?.thread_id ?? value?.threadId ?? null, trial: value?.trial ?? null };
      } catch (error) {
        return { at: Date.now() - at, ok: false, code: typeof error?.code === 'string' && /^[a-z_]{1,32}$/.test(error.code) ? error.code : 'unknown' };
      }
    };
    // One synchronous tick starts both; neither awaits the other.
    const [one, two] = await Promise.all([attempt(), attempt()]);
    return [one, two];
  }, { request: baseRequest }), 150000, 'race-deadline');
  const winners = raced.filter(outcome => outcome.ok); const losers = raced.filter(outcome => !outcome.ok);
  evidence.race = { winners: winners.length, loserCodes: losers.map(outcome => outcome.code),
    winnerSettledAfterMs: winners[0]?.at ?? null, loserSettledAfterMs: losers[0]?.at ?? null,
    overlapped: winners.length === 1 && losers.length === 1 && losers[0].at < winners[0].at };
  require(winners.length === 1 && losers.length === 1, 'exactly-one-race-winner');
  require(['busy', 'owned_elsewhere', 'source_changed', 'unavailable'].includes(losers[0].code), 'bounded-loser-refusal-code');
  require(evidence.race.overlapped, 'loser-refused-while-winner-still-preparing');
  const winner = winners[0];
  require(winner.threadId === threadId && winner.trial === false && winner.shell === 'claude_code'
    && pathKey(winner.cwd) === pathKey(projectPath), 'exact-winner-resumed-session');
  const racedSessions = await sessions();
  require(racedSessions.length === 1 && racedSessions[0].id === winner.sessionId, 'one-terminal-after-race');
  await poll(async () => (await snapshot(threadId)).activations.filter(row => row.ended_at === null).length === 1, 30000, 'winner-activation-not-recorded');
  const afterRace = await snapshot(threadId);
  require(afterRace.segments.length === 1 && afterRace.segments[0].native_id === nativeUuid, 'race-kept-single-segment-uuid');
  require(afterRace.activations.length === 2 && afterRace.activations[1].instance_id === winner.sessionId
    && afterRace.activations[1].ended_at === null && afterRace.activations[1].native_id === nativeUuid, 'winner-exact-second-owner');
  requireActiveCleanupOwner(afterRace, winner.sessionId);
  require(!afterRace.cleanupOwners.some(entry => entry.instance_id !== winner.sessionId), 'loser-left-no-owner-trace');
  evidence.checks.concurrentDuplicateResumeBoundExactlyOneOwner = true;
  // A further attempt while the winner holds the open activation is refused by
  // the recorded owner, not by the request slot. The sidebar row is reloaded
  // first, exactly as the UI does: the winning bind bumped the thread revision.
  await stage('race-after-commit');
  const freshThread = (await rows()).find(row => row.id === threadId); require(freshThread, 'saved-thread-row-after-race');
  evidence.revisionAfterRace = { before: thread.revision, after: freshThread.revision };
  const ownedRefusal = await resumeAttempt({ ...baseRequest, expectedRevision: freshThread.revision });
  evidence.raceAfterCommit = ownedRefusal.ok ? 'unexpected-success' : ownedRefusal.code;
  await save();
  require(!ownedRefusal.ok && ownedRefusal.code === 'owned_elsewhere', 'committed-owner-refuses-duplicate');
  evidence.checks.openActivationRefusedDuplicateResume = true;
  // The winner never received a prompt; stop it through the real command seam.
  await stage('stop-winner');
  await ipc('terminal_stop', { id: winner.sessionId });
  await poll(async () => !(await sessions()).some(session => session.id === winner.sessionId), 15000, 'owned-terminal-cleanup-deadline');
  await poll(async () => (await rows()).find(row => row.id === threadId)?.status === 'inactive', 10000, 'winner-thread-not-inactive');
  await poll(async () => !(await snapshot(threadId)).cleanupOwners.some(entry => entry.instance_id === winner.sessionId), 10000, 'winner-receipt-not-consumed');
  const afterWinner = await snapshot(threadId);
  require(afterWinner.activations.length === 2 && afterWinner.activations.every(row => row.ended_at !== null), 'winner-owner-ended');
  evidence.checks.raceWinnerStoppedCleanlyAndThreadRemainsResumable = true;
  // The functional leg: one real UI Resume keeps thread, UUID and prior context.
  providerPhase = 'resume'; await stage('resume-button');
  const sidebar = page.locator('.thread-sidebar'); /* The list reloads itself on thread-changed; the Reload control is hidden in the simple workspace. */
  const anchor = sidebar.locator(`button.thread-sidebar-anchor[data-thread-id="${threadId}"]`); await anchor.click();
  require((await sessions()).length === 0, 'thread-selection-autostarted');
  await sidebar.getByRole('button', { name: 'Resume', exact: true }).click();
  await poll(async () => (await sessions()).length === 1, 90000, 'resume-button-deadline');
  const resumed = (await sessions())[0]; require(resumed.id !== first.id && resumed.id !== winner.sessionId
    && resumed.shell === 'claude_code' && pathKey(resumed.cwd) === pathKey(projectPath), 'new-resumed-owner-same-project');
  activeTerminal = resumed.id;
  await poll(async () => (await layout())?.views.find(view => view.id === resumed.id)?.resource.threadId === threadId, 15000, 'resume-reused-thread-view');
  await stage('resume-onboarding'); await ready(resumed.id, 'resume');
  await stage('resume-prompt'); await input(resumed.id, PROMPT_B, 'Enter');
  await poll(async () => requests.resume === 1 && previousContextObserved && (await screen(resumed.id)).includes(MARKER_B), 30000, 'resumed-context-marker-not-observed');
  const current = await snapshot(threadId); require(current.segments.length === 1 && current.segments[0].id === initial.segments[0].id && current.segments[0].native_id === nativeUuid, 'resume-same-segment-native-id');
  require(current.activations.length === 3 && current.activations[2].instance_id === resumed.id && current.activations[2].ended_at === null
    && current.activations[2].native_id === nativeUuid, 'resume-exact-third-owner');
  requireActiveCleanupOwner(current, resumed.id);
  require((await rows()).length === 1, 'resume-created-extra-thread');
  evidence.checks.functionalUiResumeRetainedThreadNativeUuidAndPriorContext = true;
  await stage('stop-resumed'); await stop(resumed.id);
  const final = await snapshot(threadId); require(final.activations.length === 3 && final.activations.every(row => row.ended_at !== null), 'three-closed-native-intervals');
  require(!final.cleanupOwners.some(entry => [first.id, winner.sessionId, resumed.id].includes(entry.instance_id)), 'all-cleanup-receipts-consumed-after-stop');
  require(await nativeFile(final.segments[0], true) === firstFile, 'same-native-file-both-turns');
  require((await rows()).length === 1 && (await sessions()).length === 0, 'one-thread-no-extra-terminal');
  require(requests.invalid === 0 && requests.wrongCredentials === 0 && requests.overflow === 0 && pageErrors === 0, 'bounded-provider-and-ui-clean');
  require(guardCounts.allowed >= 2 && guardCounts.overflow === 0, 'both-turns-guarded-and-bounded');
  evidence.checks.threeExactNativeOwnersClosedAndBothTurnsPersisted = true;
  evidence.provider = { startRequests: requests.start, resumeRequests: requests.resume, priorAssistantInResumeContext: previousContextObserved, syntheticLoopbackOnly: true };
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only', osFirewallQualified: false };
  await stage('frame-close'); await closeFrame(); require(app.exitCode === 0, 'owned-app-clean-exit');
  evidence.checks.ownedFrameAndSessionsClosed = true; evidence.status = 'passed'; evidence.stage = 'complete'; await save();
  console.log('PASS: concurrent duplicate Resume bound exactly one owner; refused variants started nothing; functional UI Resume kept thread, UUID and prior context.');
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.proofCode || `proof-failed-at-${evidence.stage}`;
  evidence.errorMessage = String(error.message).split('\n')[0].slice(0, 300);
  evidence.startup = startup;
  evidence.provider = { ...requests, priorAssistantInResumeContext: previousContextObserved };
  evidence.guard = { ...guardCounts, scope: 'proxy-forwarding-only', osFirewallQualified: false };
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
      const inline = page.locator('#new-session-error');
      if (await inline.isVisible()) evidence.openErrorFamilies = serviceErrorFamilies(await inline.innerText());
    } catch { evidence.openErrorFamilies = ['unavailable']; }
    try {
      const status = await within(ipc('agent_status'), 3000, 'diagnostic-agent-deadline');
      const facts = status.harnesses?.claude_code;
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
          replyAVisible: text.includes(MARKER_A), replyBVisible: text.includes(MARKER_B),
          loginChoice: /Select login method:|Paste (?:the )?(?:authentication|authorization) code/i.test(text),
          apiOrNetworkError: /API Error|Error:|Failed to connect|Unable to connect|ECONN|network error/i.test(text),
          permissionError: /permission denied|access denied|not permitted/i.test(text),
          terminalExited: await pane(activeTerminal).locator('.exit-banner').count() > 0,
        };
        const banner = await pane(activeTerminal).locator('.exit-banner > span').allTextContents();
        const code = banner.join('').match(/code (\d+)/)?.[1];
        evidence.terminalShape.exitCode = code === undefined ? null : Number(code);
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
