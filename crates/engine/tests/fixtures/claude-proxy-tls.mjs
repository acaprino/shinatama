// Ignored local qualification only. Fixed synthetic data, no real provider/key.
// Rust launches fixture, sidecar and Claude in separate frozen C09 jobs.
// This process serves only synthetic loopback endpoints; it never spawns children.
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import https from 'node:https';
import net from 'node:net';
import path from 'node:path';

const [privateRoot, proxyPortText, scenario] = process.argv.slice(2);
assert.ok(path.isAbsolute(privateRoot));
assert.ok(scenario === 'tls' || scenario === 'native-resume');
const proxyPort = Number(proxyPortText);
assert.ok(Number.isInteger(proxyPort) && proxyPort > 0 && proxyPort < 65536);
const marker = 'SHINATAMA_SYNTHETIC_CLAUDE_TLS_OK';
const startPrompt = 'SHINATAMA_SYNTHETIC_START_PROMPT_A';
const resumePrompt = 'SHINATAMA_SYNTHETIC_RESUME_PROMPT_B';
const startMarker = 'SHINATAMA_SYNTHETIC_START_REPLY_A';
const resumeMarker = 'SHINATAMA_SYNTHETIC_RESUME_REPLY_B';
const dummyKey = 'shinatama-synthetic-no-provider-key';
let stage = 'certificate-fixture';
let expectedCase = null;
const send = value => process.stdout.write(`${JSON.stringify(value)}\n`);
const watchdog = setTimeout(() => process.exit(90), 37_000);
const pause = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
async function within(promise, milliseconds, code) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(code)), milliseconds); })]); }
  finally { clearTimeout(timer); }
}
// At most one small command is pending. This private pipe carries only fixed
// stage acknowledgements and booleans, never argv, credentials or CLI output.
let input = Buffer.alloc(0); let inputBytes = 0; let pending = []; let waiter;
process.stdin.on('data', chunk => {
  inputBytes += chunk.length;
  if (inputBytes > 8192 || input.length + chunk.length > 2048) process.exit(91);
  input = Buffer.concat([input, chunk]);
  let end;
  while ((end = input.indexOf(10)) >= 0) {
    let value;
    try { value = JSON.parse(input.subarray(0, end)); } catch { process.exit(91); }
    input = input.subarray(end + 1);
    if (waiter) { const receive = waiter; waiter = null; receive(value); }
    else { if (pending.length !== 0) process.exit(91); pending.push(value); }
  }
});
async function command(keys) {
  const value = await within(pending.length ? Promise.resolve(pending.shift())
    : new Promise(resolve => { waiter = resolve; }), 15_000, 'owner-command-deadline');
  assert.ok(value && !Array.isArray(value) && typeof value === 'object');
  assert.deepEqual(Object.keys(value).sort(), keys.sort());
  return value;
}

// Minimal test-only DER writer: fresh RSA CA/leaf, signed using Node crypto.
// No external certificate utility, OS certificate store or package is involved.
const cat = (...parts) => Buffer.concat(parts);
function der(tag, value) {
  const length = value.length < 128 ? Buffer.from([value.length]) : (() => {
    const hex = value.length.toString(16).padStart(Math.ceil(value.length.toString(16).length / 2) * 2, '0');
    const bytes = Buffer.from(hex, 'hex'); return cat(Buffer.from([0x80 | bytes.length]), bytes);
  })();
  return cat(Buffer.from([tag]), length, value);
}
const seq = (...parts) => der(0x30, cat(...parts));
const oid = hex => der(0x06, Buffer.from(hex, 'hex'));
const integer = number => der(0x02, Buffer.from([number]));
const algorithm = seq(oid('2a864886f70d01010b'), der(0x05, Buffer.alloc(0)));
const name = value => seq(der(0x31, seq(oid('550403'), der(0x0c, Buffer.from(value)))));
const bitString = bytes => der(0x03, cat(Buffer.from([0]), bytes));
const pem = (label, bytes) => `-----BEGIN ${label}-----\n${bytes.toString('base64').match(/.{1,64}/g).join('\n')}\n-----END ${label}-----\n`;
function time(date) {
  return der(0x17, Buffer.from(date.toISOString().replaceAll(/[-:]/g, '').replace('T', '').slice(2, 14) + 'Z'));
}
const keys = () => crypto.generateKeyPairSync('rsa', { modulusLength: 2048 });
const caKeys = keys(); const leafKeys = keys();
const caName = name('Shinatama synthetic Claude TLS CA'); const leafName = name('localhost');
const validity = seq(time(new Date(Date.now() - 60_000)), time(new Date(Date.now() + 86_400_000)));
function certificate(publicKey, subject, issuer, issuerKey, serial, extensions) {
  const body = seq(der(0xa0, integer(2)), integer(serial), algorithm, issuer, validity, subject,
    publicKey.export({ format: 'der', type: 'spki' }), der(0xa3, seq(...extensions)));
  return pem('CERTIFICATE', seq(body, algorithm, bitString(crypto.sign('sha256', body, issuerKey))));
}
const extension = (hex, content, critical = false) => seq(oid(hex), ...(critical ? [der(0x01, Buffer.from([255]))] : []), der(0x04, content));
const caPem = certificate(caKeys.publicKey, caName, caName, caKeys.privateKey, 1,
  [extension('551d13', seq(der(0x01, Buffer.from([255]))), true)]);
const leafPem = certificate(leafKeys.publicKey, leafName, caName, caKeys.privateKey, 2,
  [extension('551d13', seq(), true), extension('551d25', seq(oid('2b06010505070301'))),
    extension('551d11', seq(der(0x82, Buffer.from('localhost')), der(0x87, Buffer.from([127, 0, 0, 1]))))]);
assert.equal(new crypto.X509Certificate(leafPem).checkIssued(new crypto.X509Certificate(caPem)), true);
const caDirectory = path.join(privateRoot, 'data/ca');
for (const [filename, bytes] of [
  ['root-ca.pem', caPem], ['combined-ca.pem', caPem],
  ['root-ca-key.pem', caKeys.privateKey.export({ format: 'pem', type: 'pkcs1' })],
]) fs.writeFileSync(path.join(caDirectory, filename), bytes, { flag: 'wx', mode: 0o600 });
const sockets = new Set(); const servers = [];
function track(socket) {
  sockets.add(socket); socket.setTimeout(6000, () => socket.destroy());
  socket.once('close', () => sockets.delete(socket)); socket.on('error', () => {});
}
async function listen(server) {
  servers.push(server); server.on('connection', track);
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  return server.address().port;
}
async function localJson(port, pathname, method = 'GET') {
  return new Promise((resolve, reject) => {
    const request = http.request({ host: '127.0.0.1', port, path: pathname, method, agent: false }, response => {
      let body = Buffer.alloc(0);
      response.on('data', chunk => { if (body.length + chunk.length > 8192) request.destroy(); else body = cat(body, chunk); });
      response.on('end', () => { try { resolve({ status: response.statusCode, body: JSON.parse(body) }); } catch { reject(new Error('local-json')); } });
    });
    request.setTimeout(800, () => request.destroy()); request.on('error', () => reject(new Error('local-request'))); request.end();
  });
}
function streamMessage(response, model, replyMarker) {
  const usage = { input_tokens: 7, output_tokens: 8 };
  const message = { id: messageId(), type: 'message', role: 'assistant', model,
    content: [], stop_reason: null, stop_sequence: null, usage: { input_tokens: 7, output_tokens: 0 } };
  response.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache', connection: 'close' });
  for (const event of [
    { type: 'message_start', message },
    { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } },
    { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: replyMarker } },
    { type: 'content_block_stop', index: 0 },
    { type: 'message_delta', delta: { stop_reason: 'end_turn', stop_sequence: null }, usage },
    { type: 'message_stop' },
  ]) response.write(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`);
  response.end();
}
function messageId() {
  return scenario === 'tls' ? 'msg_shinatama_synthetic' : `msg_shinatama_synthetic_${expectedCase}`;
}
let originMessages = 0; let wrongCredentials = 0; let wrongOriginPath = 0;
let wrongHistory = 0; let resumeHistoryObserved = false;
function hasMessage(messages, role, text) {
  return Array.isArray(messages) && messages.some(message => message?.role === role &&
    (typeof message.content === 'string' ? message.content.includes(text)
      : Array.isArray(message.content) && message.content.some(block => block?.type === 'text' && typeof block.text === 'string' && block.text.includes(text))));
}
const origin = https.createServer({ cert: leafPem, key: leafKeys.privateKey.export({ format: 'pem', type: 'pkcs8' }) }, (request, response) => {
  const pathname = new URL(request.url, 'https://localhost').pathname;
  let body = Buffer.alloc(0);
  request.on('data', chunk => { if (body.length + chunk.length > 256 * 1024) request.destroy(); else body = cat(body, chunk); });
  request.on('end', () => {
    if (request.headers['x-api-key'] !== dummyKey && request.headers.authorization !== `Bearer ${dummyKey}`) {
      wrongCredentials++; response.writeHead(401); response.end(); return;
    }
    if (pathname === '/v1/messages/count_tokens') { response.setHeader('content-type', 'application/json'); response.end('{"input_tokens":7}'); return; }
    if (pathname === '/v1/models') { response.setHeader('content-type', 'application/json'); response.end(JSON.stringify({ data: [{ id: 'claude-sonnet-4-6', type: 'model', display_name: 'Synthetic test', created_at: '2026-01-01T00:00:00Z' }], has_more: false })); return; }
    if (pathname !== '/v1/messages' || request.method !== 'POST') { wrongOriginPath++; response.writeHead(404); response.end(); return; }
    let value; try { value = JSON.parse(body); } catch { response.writeHead(400); response.end(); return; }
    originMessages++;
    let replyMarker = marker;
    if (scenario === 'tls') {
      if (expectedCase !== 'private-ca') { response.writeHead(409); response.end(); return; }
    } else {
      const valid = expectedCase === 'start' ? hasMessage(value.messages, 'user', startPrompt)
        : expectedCase === 'resume' && hasMessage(value.messages, 'assistant', startMarker) && hasMessage(value.messages, 'user', resumePrompt);
      if (!valid) { wrongHistory++; response.writeHead(409); response.end(); return; }
      if (expectedCase === 'resume') resumeHistoryObserved = true;
      replyMarker = expectedCase === 'start' ? startMarker : resumeMarker;
    }
    if (value.stream) streamMessage(response, value.model, replyMarker);
    else { response.setHeader('content-type', 'application/json'); response.end(JSON.stringify({ id: messageId(), type: 'message', role: 'assistant', model: value.model,
      content: [{ type: 'text', text: replyMarker }], stop_reason: 'end_turn', stop_sequence: null, usage: { input_tokens: 7, output_tokens: 8 } })); }
  });
});
origin.on('tlsClientError', () => {});

let guardDenied = 0; let frontTunnels = 0; let backTunnels = 0;
function guard(destinationPort, originPort, front) {
  const server = http.createServer((_request, response) => { guardDenied++; response.writeHead(403); response.end(); });
  server.on('connect', (request, socket, head) => {
    if (request.url !== `localhost:${originPort}` && request.url !== `127.0.0.1:${originPort}`) {
      guardDenied++; socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n'); return;
    }
    // Every connection target is a numeric loopback address. No remote resolve,
    // fallback, redirect or user proxy setting is used by either guard.
    const upstream = net.connect({ host: '127.0.0.1', port: destinationPort }); track(upstream);
    if (front) frontTunnels++; else backTunnels++;
    upstream.once('connect', () => {
      if (front) {
        upstream.write(`CONNECT localhost:${originPort} HTTP/1.1\r\nHost: localhost:${originPort}\r\n\r\n`);
        if (head.length) upstream.write(head);
        socket.pipe(upstream); upstream.pipe(socket);
      } else {
        socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
        if (head.length) upstream.write(head); socket.pipe(upstream); upstream.pipe(socket);
      }
    });
    socket.once('close', () => upstream.destroy()); upstream.once('close', () => socket.destroy());
  });
  return server;
}

try {
  const originPort = await listen(origin);
  const backPort = await listen(guard(originPort, originPort, false));
  const frontPort = await listen(guard(proxyPort, originPort, true));
  stage = 'canonical-sidecar-startup';
  send({ phase: 'fixture-ready', originPort, frontPort, backPort });
  const { sidecarPid } = await command(['sidecarPid']);
  assert.ok(Number.isInteger(sidecarPid) && sidecarPid > 0);
  let health;
  for (let i = 0; i < 80; i++) {
    try { const value = await localJson(proxyPort, '/__bili/health'); if (value.status === 200 && value.body.ok && value.body.pid === sidecarPid) { health = value.body; break; } } catch {}
    await pause(50);
  }
  assert.equal(health?.pid, sidecarPid, 'canonical-sidecar-health');
  const cases = [];
  if (scenario === 'tls') {
  for (const privateCa of [false, true]) {
    expectedCase = privateCa ? 'private-ca' : 'no-ca'; stage = expectedCase;
    const beforeMessages = originMessages; const beforeTunnels = frontTunnels;
    send({ phase: 'case-ready', privateCa });
    const result = await command(['privateCa', 'exitCode', 'markerObserved', 'certificateRefusal', 'outputBounded', 'ownerReleased']);
    assert.equal(result.privateCa, privateCa);
    for (const key of ['markerObserved', 'certificateRefusal', 'outputBounded', 'ownerReleased']) assert.equal(typeof result[key], 'boolean');
    assert.ok(Number.isInteger(result.exitCode) && result.exitCode >= 0);
    assert.equal(result.ownerReleased, true, 'claude-owner-must-be-released');
    const { exitCode, markerObserved, certificateRefusal, outputBounded } = result;
    const record = { privateCa, exitCode, markerObserved, certificateRefusal, messageRequests: originMessages - beforeMessages,
      guardedTunnels: frontTunnels - beforeTunnels, outputBounded, ownerReleased: true };
    assert.equal(outputBounded, true, 'claude-output-cap'); assert.ok(record.guardedTunnels > 0, 'claude-must-use-proxy');
    if (privateCa) { assert.equal(exitCode, 0, 'trusted-claude-exit'); assert.equal(markerObserved, true, 'trusted-claude-marker'); assert.ok(record.messageRequests > 0); }
    else { assert.notEqual(exitCode, 0, 'untrusted-claude-exit'); assert.equal(certificateRefusal, true, 'negative-must-be-certificate-refusal'); assert.equal(markerObserved, false); assert.equal(record.messageRequests, 0); }
    cases.push(record); send({ phase: 'case-finished', ...record });
  }
  } else {
    for (const operation of ['start', 'resume']) {
      expectedCase = operation; stage = operation;
      const beforeMessages = originMessages; const beforeTunnels = frontTunnels;
      send({ phase: 'case-ready', operation });
      const result = await command(['operation', 'exitCode', 'markerObserved', 'ownerReleased', 'hookObserved', 'nativeIdMatches', 'outputBounded']);
      assert.equal(result.operation, operation); assert.equal(result.exitCode, 0);
      for (const name of ['markerObserved', 'ownerReleased', 'hookObserved', 'nativeIdMatches', 'outputBounded']) assert.equal(result[name], true);
      const record = { ...result, messageRequests: originMessages - beforeMessages,
        guardedTunnels: frontTunnels - beforeTunnels, resumeHistoryObserved: operation === 'resume' && resumeHistoryObserved };
      assert.ok(record.messageRequests > 0); assert.ok(record.guardedTunnels > 0);
      assert.equal(wrongHistory, 0);
      if (operation === 'resume') assert.equal(resumeHistoryObserved, true);
      cases.push(record); send({ phase: 'case-finished', ...record });
    }
  }
  stage = 'cleanup';
  assert.equal(wrongCredentials, 0); assert.equal(wrongOriginPath, 0); assert.ok(backTunnels > 0);
  await localJson(proxyPort, '/__bili/shutdown', 'POST');
  send({ phase: 'sidecar-stop' });
  const stopped = await command(['sidecarReleased', 'exitCode', 'outputBounded']);
  assert.equal(stopped.sidecarReleased, true); assert.equal(stopped.exitCode, 0);
  assert.equal(stopped.outputBounded, true, 'sidecar-output-cap');
  send({ phase: 'complete', scenario, cases, canonicalSidecar: true, guardDenied, loopbackOnlyForwarding: true,
    freshSyntheticCa: true, osTrustChanged: false, syntheticLoopbackEndpoint: true, guardForwardedRemote: false, userAuthenticationRead: false,
    normalC11AuthorityQualified: false, nativeTuiQualified: false });
} catch {
  // Exact stage only. Raw native/API output and dummy credential never persist.
  send({ phase: 'failed', stage }); process.exitCode = 1;
} finally {
  for (const socket of sockets) socket.destroy();
  await Promise.all(servers.map(server => new Promise(resolve => server.close(resolve))));
  process.stdin.destroy();
  clearTimeout(watchdog);
}
