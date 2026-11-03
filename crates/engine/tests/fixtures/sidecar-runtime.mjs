// Private local qualification only. The canonical source/artifact is never edited.
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import https from 'node:https';
import net from 'node:net';
import tls from 'node:tls';
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import readline from 'node:readline';
import { DatabaseSync } from 'node:sqlite';
import { pathToFileURL } from 'node:url';
import { fileURLToPath } from 'node:url';

const [mode, prepared, privateRoot, dependencies] = process.argv.slice(2);
assert.ok(['probe', 'inspect', 'latency', 'tls', 'tls-client'].includes(mode));
for (const root of [prepared, privateRoot, dependencies]) assert.ok(path.isAbsolute(root));
const requireLocal = createRequire(path.join(dependencies, 'esbuild/package.json'));
const send = value => process.stdout.write(JSON.stringify(value) + '\n');
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const indexNames = [
  'idx_messages_content_hash', 'idx_blocks_summary_hash', 'idx_blocks_one_hash',
  'idx_checkpoints_state_hash', 'idx_ccr_entries_content_hash', 'idx_message_media_hash',
];
const watchdog = setTimeout(() => process.exit(90), 27_000);
watchdog.unref();
let stage = 'initialization';
// Every command is a fixed local test instruction. Bound both count and length.
const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
const pending = [];
const waiting = [];
let received = 0;
input.on('line', line => {
  assert.ok(++received <= 8 && line.length <= 64);
  const waiter = waiting.shift();
  if (waiter) waiter(line); else pending.push(line);
});
const command = () => pending.length ? Promise.resolve(pending.shift()) : new Promise(resolve => waiting.push(resolve));

async function localJson(port, pathname, method = 'GET') {
  return new Promise((resolve, reject) => {
    const request = http.request({ host: '127.0.0.1', port, path: pathname, method, agent: false }, response => {
      let bytes = 0; const chunks = [];
      response.on('data', chunk => { bytes += chunk.length; if (bytes > 256 * 1024) request.destroy(new Error('bounded local response')); else chunks.push(chunk); });
      response.on('end', () => { try { resolve({ status: response.statusCode, value: JSON.parse(Buffer.concat(chunks).toString()) }); } catch { reject(new Error('invalid local response')); } });
    });
    request.setTimeout(1500, () => request.destroy(new Error('bounded local request')));
    request.on('error', () => reject(new Error('local request failed'))); request.end();
  });
}
function controlled(args, env, parse = false) {
  const child = spawn(process.execPath, args, { cwd: privateRoot, env, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let stdoutBytes = 0; let stderrBytes = 0; let partial = ''; const messages = []; const waiters = [];
  const closed = new Promise((resolve, reject) => { child.once('error', () => reject(new Error('controlled child startup'))); child.once('close', code => resolve(code)); });
  const boundedFailure = () => { child.kill(); };
  child.stderr.on('data', chunk => { stderrBytes += chunk.length; if (stderrBytes > 8192) boundedFailure(); });
  child.stdout.on('data', chunk => {
    stdoutBytes += chunk.length; if (stdoutBytes > (parse ? 8192 : 64 * 1024)) { boundedFailure(); return; }
    if (!parse) return;
    partial += chunk.toString();
    while (partial.includes('\n')) {
      const split = partial.indexOf('\n'); const line = partial.slice(0, split); partial = partial.slice(split + 1);
      try { const message = JSON.parse(line); const waiter = waiters.shift(); if (waiter) waiter(message); else messages.push(message); } catch { boundedFailure(); }
      assert.ok(messages.length <= 4 && waiters.length <= 4);
    }
  });
  return { child, closed, next: () => Promise.race([messages.length ? Promise.resolve(messages.shift()) : new Promise(resolve => waiters.push(resolve)), closed.then(() => { throw new Error('controlled child exited before message'); })]) };
}
function clientEnvironment(withCa, override = {}) {
  if (Object.entries(override).some(([key, value]) => key.toUpperCase() === 'NODE_TLS_REJECT_UNAUTHORIZED' && value === '0')) throw new Error('unsafe TLS override refused before spawn');
  const env = { ...process.env, ...override };
  delete env.NODE_EXTRA_CA_CERTS; delete env.NODE_TLS_REJECT_UNAUTHORIZED;
  if (withCa) env.NODE_EXTRA_CA_CERTS = path.join(privateRoot, 'data/ca/root-ca.pem');
  return env;
}
async function tunneledTls(proxyPort, originPort, target) {
  return new Promise((resolve, reject) => {
    const socket = net.connect({ host: '127.0.0.1', port: proxyPort });
    let header = Buffer.alloc(0); let secure;
    const timer = setTimeout(() => { secure?.destroy(); socket.destroy(); reject(new Error('TLS fixture timeout')); }, 2500);
    const fail = error => { clearTimeout(timer); secure?.destroy(); socket.destroy(); reject(error); };
    socket.on('error', fail);
    socket.once('connect', () => socket.write(`CONNECT ${target}:${originPort} HTTP/1.1\r\nHost: ${target}\r\n\r\n`));
    const readHeader = chunk => {
      header = Buffer.concat([header, chunk]);
      if (header.length > 1024) { fail(new Error('CONNECT response cap')); return; }
      const end = header.indexOf('\r\n\r\n'); if (end < 0) return;
      socket.off('data', readHeader);
      if (!header.toString().startsWith('HTTP/1.1 200')) { fail(new Error('CONNECT failed')); return; }
      assert.equal(header.length, end + 4);
      secure = tls.connect({ socket, servername: 'localhost', rejectUnauthorized: true });
      secure.once('error', fail);
      secure.once('secureConnect', () => {
        assert.equal(secure.authorized, true);
        const fingerprint = secure.getPeerCertificate().fingerprint256;
        let bytes = 0; const response = [];
        secure.on('data', data => { bytes += data.length; if (bytes > 8192) fail(new Error('HTTPS response cap')); else response.push(data); });
        secure.once('end', () => {
          clearTimeout(timer); const text = Buffer.concat(response).toString();
          if (!text.startsWith('HTTP/1.1 200') || !text.includes('synthetic-https-origin')) { fail(new Error('unexpected HTTPS origin')); return; }
          resolve({ authorized: true, peerFingerprint256: fingerprint, responseMarkerVerified: true });
        });
        secure.write(`GET /fixture HTTP/1.1\r\nHost: localhost:${originPort}\r\nConnection: close\r\n\r\n`);
      });
    };
    socket.on('data', readHeader);
  });
}

function inspect(db) {
  const version = db.prepare('PRAGMA user_version').get().user_version;
  assert.equal(version, 9);
  const indexes = db.prepare("SELECT name FROM sqlite_schema WHERE type='index'").all().map(row => row.name);
  for (const name of indexNames) assert.ok(indexes.includes(name), name);
  assert.equal(db.prepare('PRAGMA quick_check').get().quick_check, 'ok');
  assert.equal(db.prepare('PRAGMA foreign_key_check').all().length, 0);
  const details = db.prepare('EXPLAIN QUERY PLAN DELETE FROM content WHERE hash = ? AND refcount <= 0').all('absent');
  assert.equal(details.filter(row => /\bSCAN\b/.test(row.detail)).length, 0);
  return { schema: version, indexes: indexNames, quickCheck: 'ok', foreignKeys: 'ok', contentDeleteScans: 0 };
}
function plan(id, entries) {
  return {
    session: { sessionId: id, createdAt: 1, updatedAt: 2, nextBlockId: 1, nextRunId: 1,
      metaJson: '{}', statsJson: '{}', metadataJson: '{}', kstateJson: '{}',
      tailJson: '{"tailSeqs":[]}', ccrJson: null },
    messages: [], blocks: [], deadRefs: [], refs: [],
    ccrEntries: entries.map(([key, content], ord) => ({ ord, key, content })),
  };
}

try {
  if (mode === 'probe') {
    stage = 'builtin-sqlite-and-synthetic-ca';
    const db = new DatabaseSync(path.join(privateRoot, 'data', 'node-probe.db'));
    db.exec('CREATE TABLE probe(value INTEGER); INSERT INTO probe VALUES(7)');
    assert.equal(db.prepare('SELECT value FROM probe').get().value, 7);
    db.close();
    // Synthetic test CA only. A fresh combined bundle avoids canonical startup's
    // optional OS certificate-store export; no OS trust or real key is touched.
    const forge = requireLocal('node-forge');
    const keys = forge.pki.rsa.generateKeyPair({ bits: 2048 });
    const cert = forge.pki.createCertificate();
    cert.publicKey = keys.publicKey;
    cert.serialNumber = '01';
    cert.validity.notBefore = new Date(Date.now() - 60_000);
    cert.validity.notAfter = new Date(Date.now() + 86_400_000);
    const attrs = [{ name: 'commonName', value: 'Shinatama synthetic sidecar qualification CA' }];
    cert.setSubject(attrs); cert.setIssuer(attrs);
    cert.setExtensions([{ name: 'basicConstraints', cA: true }, { name: 'keyUsage', keyCertSign: true }]);
    cert.sign(keys.privateKey, forge.md.sha256.create());
    const pem = forge.pki.certificateToPem(cert);
    const ca = path.join(privateRoot, 'data', 'ca');
    fs.mkdirSync(ca, { recursive: true });
    for (const [name, bytes] of [['root-ca.pem', pem], ['root-ca-key.pem', forge.pki.privateKeyToPem(keys.privateKey)], ['combined-ca.pem', pem]]) {
      fs.writeFileSync(path.join(ca, name), bytes, { flag: 'wx', mode: 0o600 });
    }
    const leafKeys = forge.pki.rsa.generateKeyPair({ bits: 2048 });
    const leaf = forge.pki.createCertificate();
    leaf.publicKey = leafKeys.publicKey;
    leaf.serialNumber = '02'; leaf.validity.notBefore = cert.validity.notBefore; leaf.validity.notAfter = cert.validity.notAfter;
    leaf.setSubject([{ name: 'commonName', value: 'localhost' }]); leaf.setIssuer(attrs);
    leaf.setExtensions([{ name: 'basicConstraints', cA: false }, { name: 'keyUsage', digitalSignature: true, keyEncipherment: true },
      { name: 'extKeyUsage', serverAuth: true }, { name: 'subjectAltName', altNames: [{ type: 2, value: 'localhost' }, { type: 7, ip: '127.0.0.1' }] }]);
    leaf.sign(keys.privateKey, forge.md.sha256.create());
    fs.writeFileSync(path.join(ca, 'fixture-leaf.pem'), forge.pki.certificateToPem(leaf), { flag: 'wx', mode: 0o600 });
    fs.writeFileSync(path.join(ca, 'fixture-leaf-key.pem'), forge.pki.privateKeyToPem(leafKeys.privateKey), { flag: 'wx', mode: 0o600 });
    send({ phase: 'probe', nodeVersion: process.versions.node, modulesAbi: process.versions.modules,
      builtinSqlite: true, sqliteVersion: new DatabaseSync(':memory:').prepare('SELECT sqlite_version() AS version').get().version,
      syntheticCaOnly: true });
  } else if (mode === 'inspect') {
    stage = 'actual-bundle-schema';
    const db = new DatabaseSync(path.join(privateRoot, 'data', 'sessions', 'index.db'), { readOnly: true });
    send({ phase: 'inspected', ...inspect(db) });
    db.close();
  } else if (mode === 'tls-client') {
    stage = 'child-tls-validation';
    assert.equal(process.env.NODE_TLS_REJECT_UNAUTHORIZED, undefined);
    const [proxyPort, originPort, target, withCa] = process.argv.slice(6);
    assert.ok(['localhost', '127.0.0.1'].includes(target));
    assert.equal(process.env.NODE_EXTRA_CA_CERTS, withCa === 'true' ? path.join(privateRoot, 'data/ca/root-ca.pem') : undefined);
    send({ phase: 'client-ready' });
    assert.equal(await command(), 'go');
    let result;
    try { result = await tunneledTls(Number(proxyPort), Number(originPort), target); }
    catch (error) {
      const allowed = ['SELF_SIGNED_CERT_IN_CHAIN', 'UNABLE_TO_VERIFY_LEAF_SIGNATURE', 'DEPTH_ZERO_SELF_SIGNED_CERT', 'UNABLE_TO_GET_ISSUER_CERT_LOCALLY'];
      assert.ok(allowed.includes(error?.code), 'negative TLS must be certificate rejection, not network failure');
      result = { authorized: false, certificateRejected: true, certificateError: error.code };
    }
    assert.equal(result.authorized, withCa === 'true');
    send({ phase: 'client-result', ...result });
  } else if (mode === 'tls') {
    stage = 'tls-infrastructure';
    const proxyPort = Number(process.argv[6]);
    assert.ok(Number.isInteger(proxyPort) && proxyPort > 0 && proxyPort < 65536);
    assert.equal(process.env.NODE_EXTRA_CA_CERTS, undefined);
    assert.equal(process.env.NODE_TLS_REJECT_UNAUTHORIZED, undefined);
    const ownerTlsSettings = [process.env.NODE_EXTRA_CA_CERTS, process.env.NODE_TLS_REJECT_UNAUTHORIZED];
    assert.throws(() => clientEnvironment(true, { NODE_TLS_REJECT_UNAUTHORIZED: '0' }));
    const ca = path.join(privateRoot, 'data/ca');
    const leaf = fs.readFileSync(path.join(ca, 'fixture-leaf.pem'));
    const key = fs.readFileSync(path.join(ca, 'fixture-leaf-key.pem'));
    const originFingerprint = new crypto.X509Certificate(leaf).fingerprint256;
    const origin = https.createServer({ cert: leaf, key }, (_req, res) => res.end('synthetic-https-origin'));
    const mitmOrigin = https.createServer({ cert: leaf, key }, (_req, res) => res.end('synthetic-https-origin'));
    for (const server of [origin, mitmOrigin]) server.on('tlsClientError', () => {});
    origin.listen(0, '127.0.0.1'); mitmOrigin.listen(0, 'localhost');
    await Promise.all([new Promise(resolve => origin.once('listening', resolve)), new Promise(resolve => mitmOrigin.once('listening', resolve))]);
    assert.ok(['127.0.0.1', '::1'].includes(mitmOrigin.address().address));
    const cliEnv = { ...process.env, BILI_MITM_DOMAINS: 'localhost', NODE_EXTRA_CA_CERTS: path.join(ca, 'root-ca.pem') };
    const cli = controlled([path.join(prepared, 'dist/index.js'), 'start', '--host', '127.0.0.1', '--port', String(proxyPort), '--config', path.join(privateRoot, 'config/billion-context.json'), '--no-auto-update'], cliEnv);
    send({ phase: 'tls-cli-created' });
    assert.equal(await command(), 'census-cli');
    let health;
    for (let attempt = 0; attempt < 30; attempt++) {
      try { const response = await localJson(proxyPort, '/__bili/health'); if (response.status === 200 && response.value.ok) { health = response.value; break; } } catch {}
      await new Promise(resolve => setTimeout(resolve, 30));
    }
    assert.equal(health?.pid, cli.child.pid);
    const manifest = await localJson(proxyPort, '/__bili/plugin/manifest');
    assert.equal(manifest.status, 200); assert.equal(manifest.value.ok, true);
    const runtimeCcr = manifest.value.toolNames.includes('acp_retrieve');
    const runtimeAbsorb = manifest.value.toolNames.includes('absorb');
    assert.equal(runtimeCcr, true); assert.equal(runtimeAbsorb, true);
    send({ phase: 'tls-cli-ready', actualCliIdentityVerified: true, runtimeConfiguration: {
      source: 'running-cli-plugin-manifest', ccr: runtimeCcr, absorb: runtimeAbsorb,
      tiers: null, adaptivePressure: null, promptRewrite: null, outputSteering: null, effortRouting: null,
      completeEffectiveConfigurationAvailable: false,
    } });
    const cases = [];
    for (const [index, kind, withCa] of [[0, 'blind-connect', false], [1, 'blind-connect', true], [2, 'mitm', false], [3, 'mitm', true]]) {
      stage = `tls-${kind}-${withCa ? 'private-ca' : 'no-ca'}`;
      const target = kind === 'mitm' ? 'localhost' : '127.0.0.1';
      const port = kind === 'mitm' ? mitmOrigin.address().port : origin.address().port;
      const client = controlled([fileURLToPath(import.meta.url), 'tls-client', prepared, privateRoot, dependencies, String(proxyPort), String(port), target, String(withCa)], clientEnvironment(withCa), true);
      assert.equal((await client.next()).phase, 'client-ready');
      send({ phase: 'tls-client-created', index, kind, privateCa: withCa });
      assert.equal(await command(), 'census-client');
      client.child.stdin.end('go\n');
      const result = await client.next();
      assert.equal(result.phase, 'client-result'); assert.equal(await client.closed, 0);
      if (withCa) assert.equal(result.peerFingerprint256 === originFingerprint, kind === 'blind-connect');
      const record = { index, kind, privateCa: withCa, ...result,
        originLeafMatched: withCa ? result.peerFingerprint256 === originFingerprint : null,
        privateCaOnlyChildEnvironment: true };
      cases.push(record); send({ ...record, phase: 'tls-client-finished' });
    }
    stage = 'tls-graceful-cleanup';
    assert.equal((await localJson(proxyPort, '/__bili/shutdown', 'POST')).status, 202);
    assert.equal(await cli.closed, 0);
    for (const server of [origin, mitmOrigin]) server.closeAllConnections();
    await Promise.all([new Promise(resolve => origin.close(resolve)), new Promise(resolve => mitmOrigin.close(resolve))]);
    assert.deepEqual([process.env.NODE_EXTRA_CA_CERTS, process.env.NODE_TLS_REJECT_UNAUTHORIZED], ownerTlsSettings);
    send({ phase: 'tls-complete', cases, originLeafFingerprint256: originFingerprint, actualCliBundle: true, syntheticCaOnly: true,
      unsafeTlsSettingRefusedBeforeClientSpawn: true, fixtureOwnerTlsEnvironmentUnchanged: true, osTrustInstalled: false, runtimeConfiguration: {
        source: 'running-cli-plugin-manifest', ccr: runtimeCcr, absorb: runtimeAbsorb,
        tiers: null, adaptivePressure: null, promptRewrite: null, outputSteering: null, effortRouting: null,
        completeEffectiveConfigurationAvailable: false } });
  } else {
    stage = 'local-source-build';
    // Existing offline toolchain, no installer/restore/build scripts. Keep its
    // native service alive until the Rust owner has verified exact job members.
    const esbuild = requireLocal('esbuild');
    const artifact = path.join(privateRoot, 'temp', 'storage-runtime.mjs');
    await esbuild.build({
      stdin: { contents: [
        'export { UnifiedStore, sha256Hex } from "./src/storage/store.ts";',
        'export { migrateSchema } from "./src/storage/schema.ts";',
        'export { setupMitm } from "./src/mitm.ts";',
      ].join('\n'), resolveDir: prepared, loader: 'ts' },
      outfile: artifact, bundle: true, platform: 'node', format: 'esm', target: 'node22',
      mainFields: ['module', 'main'], nodePaths: [dependencies], external: ['jimp', 'mysql2'],
      alias: { 'acp-kernel': path.join(prepared, 'kernel/src/index.ts'), 'acp-kernel/wire': path.join(prepared, 'kernel/src/wire/index.ts') },
      banner: { js: 'import {createRequire as testRequire} from "node:module";const require=testRequire(import.meta.url);' },
      logLevel: 'silent',
    });
    const esbuildBinary = path.join(dependencies, '@esbuild/win32-x64/esbuild.exe');
    send({ phase: 'built', artifactSha256: digest(fs.readFileSync(artifact)),
      esbuildVersion: esbuild.version, esbuildBinarySha256: digest(fs.readFileSync(esbuildBinary)) });
    assert.equal(await command(), 'census-complete');
    esbuild.stop();
    stage = 'canonical-store-open';
    const runtime = await import(pathToFileURL(artifact).href);
    const store = runtime.UnifiedStore.open(path.join(privateRoot, 'data', 'latency.db'), 'node:sqlite');
    const db = store.db;
    const initial = inspect(db);
    db.exec(`INSERT INTO content(hash,byte_len,refcount,bytes) VALUES('background',2,200000,'{}');
      WITH RECURSIVE n(seq) AS (SELECT 1 UNION ALL SELECT seq+1 FROM n WHERE seq<200000)
      INSERT INTO messages(session_id,seq,msg_id,role,content_hash)
      SELECT 'unrelated-history',seq,'msg-'||seq,'user','background' FROM n;`);
    const entries = Array.from({ length: 600 }, (_, i) => [`key-${i}`, `old-content-${i}`]);
    store.ingestSession(plan('active', entries));
    store.ingestSession(plan('shared-session', [['shared-key', entries[0][1]]]));
    assert.equal(db.prepare('SELECT refcount FROM content WHERE hash=?').get(runtime.sha256Hex(entries[0][1])).refcount, 2);
    stage = 'schema-v8-migration';
    const beforeMigration = JSON.stringify(db.prepare('SELECT hash,refcount FROM content ORDER BY hash').all());
    for (const name of indexNames) db.exec(`DROP INDEX ${name}`);
    db.exec('PRAGMA user_version=8');
    runtime.migrateSchema(db);
    runtime.migrateSchema(db);
    assert.equal(JSON.stringify(db.prepare('SELECT hash,refcount FROM content ORDER BY hash').all()), beforeMigration);
    inspect(db);
    stage = 'incremental-ccr-invariants';
    db.exec(`CREATE TEMP TABLE content_changes(op TEXT);
      CREATE TEMP TRIGGER audit_insert AFTER INSERT ON content BEGIN INSERT INTO content_changes VALUES('insert'); END;
      CREATE TEMP TRIGGER audit_update AFTER UPDATE ON content BEGIN INSERT INTO content_changes VALUES('update'); END;
      CREATE TEMP TRIGGER audit_delete AFTER DELETE ON content BEGIN INSERT INTO content_changes VALUES('delete'); END;`);
    store.ingestSession(plan('active', entries));
    assert.equal(db.prepare('SELECT count(*) AS n FROM content_changes').get().n, 0);
    // Reorder and rename retained content without changing any blob refs.
    store.ingestSession(plan('active', entries.toReversed().map(([key, value]) => [`renamed-${key}`, value])));
    assert.equal(db.prepare('SELECT count(*) AS n FROM content_changes').get().n, 0);
    // Restore ordering before the measured 600-content replacement.
    store.ingestSession(plan('active', entries));
    const swapped = entries.map(([key, value], index) => [key, index === 0 ? entries[1][1] : index === 1 ? entries[0][1] : value]);
    store.ingestSession(plan('active', swapped));
    assert.equal(db.prepare('SELECT count(*) AS n FROM content_changes').get().n, 0);
    store.ingestSession(plan('active', entries));
    const server = http.createServer((_req, res) => res.end('healthy'));
    stage = 'canonical-connect-startup';
    runtime.setupMitm(server);
    server.listen(0, '127.0.0.1');
    await new Promise(resolve => server.once('listening', resolve));
    send({ phase: 'ready', port: server.address().port, ...initial,
      unrelatedMessages: db.prepare("SELECT count(*) AS n FROM messages WHERE session_id='unrelated-history'").get().n,
      unchangedReorderedRenamedAndSwappedBlobWrites: 0, sharedSessionReferencesVerified: true });
    for (let iteration = 0; iteration < 3; iteration++) {
      stage = 'measured-canonical-ccr-save';
      assert.equal(await command(), 'save');
      // Parent starts both separate-process probes after receiving this marker.
      fs.writeSync(1, JSON.stringify({ phase: 'saving', iteration }) + '\n');
      const startedAtMs = Date.now();
      const started = performance.now();
      const next = entries.map(([key], i) => [key, `replacement-${iteration}-${i}`]);
      store.ingestSession(plan('active', next));
      const saveMs = performance.now() - started;
      const observed = db.prepare("SELECT count(*) AS n FROM ccr_entries WHERE session_id='active'").get().n;
      assert.equal(observed, 600);
      assert.equal(db.prepare("SELECT count(*) AS n FROM content WHERE hash <> 'background' AND refcount <> 1").get().n, 0);
      send({ phase: 'saved', iteration, saveMs, startedAtMs, endedAtMs: Date.now(), ccrEntries: observed });
    }
    assert.equal(await command(), 'close');
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    const final = inspect(db);
    // The canonical invariant rejects duplicate ordinals (keys may repeat).
    stage = 'canonical-rollback';
    const before = JSON.stringify(db.prepare('SELECT hash,refcount FROM content ORDER BY hash').all());
    const invalid = plan('active', [['one', 'changed'], ['two', 'new']]);
    invalid.ccrEntries[1].ord = invalid.ccrEntries[0].ord;
    assert.throws(() => store.ingestSession(invalid));
    assert.equal(JSON.stringify(db.prepare('SELECT hash,refcount FROM content ORDER BY hash').all()), before);
    store.close();
    send({ phase: 'complete', repeatedEdits: 3, rejectedWriteRolledBack: true,
      populatedV8MigrationAndRepeatPreservedContent: true, ...final });
  }
  input.close();
  clearTimeout(watchdog);
} catch (error) {
  // No upstream errors/paths/configuration values are reflected into evidence.
  const nativeCode = typeof error?.code === 'string' && /^[A-Z0-9_]{1,64}$/.test(error.code) ? error.code : null;
  send({ phase: 'failed', code: 'sidecar_fixture_failed', stage, nativeCode });
  process.exitCode = 1;
  input.close();
}
