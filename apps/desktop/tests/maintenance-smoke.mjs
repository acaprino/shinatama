// Real embedded WebView2, real ConPTY and a synthetic private database only.
// Run under verify/P14's wrap-run after building the current desktop executable.
// The real 60-second idle interval is exercised; it is never shortened for the test.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { performance } from 'node:perf_hooks';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('The embedded maintenance check requires Windows/WebView2.');
const root = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-maintenance-smoke-'));
const data = resolve(fixture, 'data');
const project = resolve(fixture, 'synthetic-project');
const databasePath = resolve(data, 'shinatama.db');
const evidenceDirectory = resolve(root, '.prototype-evidence');
const evidencePath = resolve(evidenceDirectory, 'maintenance.json');
const fakeSecret = 'SHINATAMA_FAKE_SECRET_DO_NOT_COPY_maintenance_fixture';
const port = 19231;
const MAX_UI_RTT_MS = 1500;
const MAX_MARKER_AGE_MS = 2000;
const MAX_MARKER_GAP_MS = 3000;
const BURST_REQUESTS = 16;
await mkdir(data); await mkdir(project); await mkdir(evidenceDirectory, { recursive: true });
const legacy = Buffer.from(`${JSON.stringify({ schema_version: 1, projects: [
  { path: project, name: 'synthetic-project', last_opened: 1700000001000 },
] })}\n`);
await writeFile(resolve(data, 'projects.json'), legacy);

const evidence = {
  check: 'C03M-embedded-maintenance', status: 'running', stage: 'start',
  thresholds: { uiRttMs: MAX_UI_RTT_MS, markerAgeMs: MAX_MARKER_AGE_MS, markerGapMs: MAX_MARKER_GAP_MS },
  checks: {}, metrics: {}, outcomes: {},
};
await saveEvidence();
let app;
let browser;
let page;
let pageOwned = false;
let databaseLock;
let closedNormally = false;
let spawnError;
const pageErrors = [];
const uiRtts = [];
const markerAges = [];
const markerGaps = [];
let lastSequence = -1;
let lastAdvance = performance.now();
let burstReturnMs = 0;

async function saveEvidence() {
  // Deliberately omit raw rows, paths, report payloads, terminal text and error messages.
  await writeFile(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);
}

function invoke(command, args) {
  return page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
}

function pathKey(path) {
  if (path.startsWith('\\\\?\\UNC\\')) path = `\\\\${path.slice(8)}`;
  else if (path.startsWith('\\\\?\\')) path = path.slice(4);
  return path.replaceAll('/', '\\').toLowerCase();
}

function observeStatus(status) {
  expect(typeof status.phase).toBe('string');
  expect(JSON.stringify(status)).not.toContain(fakeSecret);
  if (status.report) {
    const report = status.report;
    expect(report.vm_steps_approx).toBeLessThanOrEqual(50_000);
    expect(report.observed_rows).toBeLessThanOrEqual(128);
    expect(report.inspected_payload_bytes).toBeLessThanOrEqual(64 * 1024);
    expect(report.anomalies.length).toBeLessThanOrEqual(128);
    expect(report.deferred_rows.length).toBeLessThanOrEqual(128);
    const outcome = report.outcome;
    expect(['completed', 'deferred_budget', 'deferred_pressure', 'deferred_busy', 'deferred_capacity',
      'read_only', 'in_transaction', 'invalid_metadata', 'corruption_detected']).toContain(outcome);
    evidence.outcomes[outcome] = (evidence.outcomes[outcome] || 0) + 1;
  }
  return status;
}

function snapshot() {
  const database = new DatabaseSync(databasePath, { readOnly: true });
  try {
    const projects = database.prepare('SELECT * FROM project ORDER BY id').all();
    const settings = database.prepare("SELECT key,value FROM setting WHERE key NOT IN ('maintenance.lease.v1','maintenance.state.v1') ORDER BY key").all();
    const jobRows = database.prepare("SELECT key,value FROM setting WHERE key IN ('maintenance.lease.v1','maintenance.state.v1') ORDER BY key").all();
    const stateRow = jobRows.find(row => row.key === 'maintenance.state.v1');
    const state = stateRow ? JSON.parse(stateRow.value) : null;
    return {
      digest: createHash('sha256').update(JSON.stringify({ projects, settings })).digest('hex'),
      projects: projects.length, settings: settings.length,
      operationalRows: jobRows.length,
      metadataBytes: jobRows.reduce((sum, row) => sum + Buffer.byteLength(row.value), 0),
      anomalyCount: state?.anomalies?.length || 0,
      schema: database.prepare('PRAGMA user_version').get().user_version,
      integrity: database.prepare('PRAGMA integrity_check').get().integrity_check,
    };
  } finally { database.close(); }
}

function stats(values) {
  const ordered = [...values].sort((a, b) => a - b);
  return {
    samples: ordered.length,
    p50: Math.round(ordered[Math.floor((ordered.length - 1) * 0.5)] || 0),
    p95: Math.round(ordered[Math.floor((ordered.length - 1) * 0.95)] || 0),
    maximum: Math.round(ordered.at(-1) || 0),
  };
}

async function probeHealthyTerminal() {
  const started = performance.now();
  const result = await page.evaluate(async () => {
    const sessions = await window.__TAURI_INTERNALS__.invoke('terminal_list');
    // The announcement live-region follows the screen rows in DOM order and
    // intentionally stops after 20 lines. Read current screen rows only.
    const text = document.querySelector('.xterm-accessibility-tree')?.textContent || '';
    const matches = [...text.matchAll(/SHINATAMA_HEALTHY:(\d+):(\d+)/g)];
    const newest = matches.at(-1);
    return { sessions: sessions.length, sequence: newest ? Number(newest[1]) : -1,
      timestamp: newest ? Number(newest[2]) : 0,
      project: document.querySelector('#project')?.dataset.project || '' };
  });
  const rtt = performance.now() - started;
  uiRtts.push(rtt);
  expect(rtt).toBeLessThan(MAX_UI_RTT_MS);
  expect(result.sessions).toBe(1);
  expect(result.project).toBe('synthetic-project');
  expect(result.sequence).toBeGreaterThanOrEqual(0);
  const age = Date.now() - result.timestamp;
  markerAges.push(age);
  expect(age).toBeGreaterThanOrEqual(-100);
  expect(age).toBeLessThan(MAX_MARKER_AGE_MS);
  const now = performance.now();
  if (result.sequence > lastSequence) {
    if (lastSequence >= 0) markerGaps.push(now - lastAdvance);
    lastSequence = result.sequence; lastAdvance = now;
  }
  expect(now - lastAdvance).toBeLessThan(MAX_MARKER_GAP_MS);
  return result;
}

async function checkAlongsideTerminal() {
  const pending = invoke('maintenance_check');
  await probeHealthyTerminal();
  const status = observeStatus(await pending);
  await probeHealthyTerminal();
  return status;
}

async function waitForExit(milliseconds) {
  if (!app || app.exitCode !== null || app.signalCode !== null) return;
  await new Promise((resolve, reject) => {
    const completed = () => { clearTimeout(timer); resolve(); };
    const timer = setTimeout(() => {
      app.removeListener('exit', completed);
      reject(new Error('Owned maintenance-test application exceeded its close deadline.'));
    }, milliseconds);
    app.once('exit', completed);
  });
}

try {
  // Refuse a CDP endpoint already owned by another app before attaching.
  await new Promise((resolveProbe, reject) => {
    const probe = createServer();
    probe.once('error', () => reject(new Error('The maintenance CDP port is already in use; no existing app was attached.')));
    probe.listen({ host: '127.0.0.1', port, exclusive: true }, () => {
      probe.close(error => error ? reject(error) : resolveProbe());
    });
  });
  app = spawn(resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: root, windowsHide: true, stdio: 'ignore',
    env: { ...process.env, SHINATAMA_HOME: data,
      WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
  });
  app.once('error', error => { spawnError = error; });
  const connectDeadline = performance.now() + 30_000;
  while (performance.now() < connectDeadline) {
    if (spawnError) throw new Error(`Owned test application could not start: ${spawnError.code}`);
    if (app.exitCode !== null) throw new Error('Owned maintenance-test application exited during startup.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
    catch { await delay(250); }
  }
  if (!browser) throw new Error('Maintenance WebView2 startup exceeded its deadline.');
  page = browser.contexts()[0].pages()[0] || await browser.contexts()[0].waitForEvent('page', { timeout: 5000 });
  page.on('pageerror', error => pageErrors.push(error.name));
  await page.waitForURL(url => url.hostname === 'tauri.localhost', { timeout: 10000 });
  await expect(page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 15000 });
  const storage = await invoke('storage_location');
  expect(pathKey(storage.directory)).toBe(pathKey(data));
  expect(pathKey(storage.database_path)).toBe(pathKey(databasePath));
  expect(storage.read_only).toBe(false);
  pageOwned = true;
  evidence.checks.ownedSyntheticStorageConfirmedBeforeInteraction = true;
  await expect(page.locator('#project')).toHaveAttribute('data-project', 'synthetic-project');
  expect(await invoke('terminal_list')).toEqual([]);

  evidence.stage = 'start-healthy-pty';
  await page.locator('#new-terminal').click();
  await expect(page.locator('.xterm-accessibility').first()).toContainText('PS ', { timeout: 20000 });
  await page.locator('.xterm-helper-textarea').first().focus();
  // The echoed command contains split literals; only actual PTY output matches
  // the marker regex. No model, account, credential or project file is involved.
  const command = "$watch=[Diagnostics.Stopwatch]::StartNew(); $n=0; while($watch.Elapsed.TotalSeconds -lt 80) { [Console]::WriteLine(('SHINATAMA_'+'HEALTHY:'+$n+':'+[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())); $n++; Start-Sleep -Milliseconds 75 }; [Console]::WriteLine(('SHINATAMA_'+'HEALTHY_DONE'))";
  await page.keyboard.type(command); await page.keyboard.press('Enter');
  await expect(page.locator('.xterm-accessibility').first()).toContainText('SHINATAMA_HEALTHY:0:', { timeout: 15000 });
  const idleBeganAt = Date.now();
  await probeHealthyTerminal();

  evidence.stage = 'synthetic-anomaly';
  const injector = new DatabaseSync(databasePath);
  try {
    injector.exec('PRAGMA busy_timeout=5000;');
    injector.prepare("INSERT INTO setting(key,value) VALUES('ui.appearance.v1',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
      .run(JSON.stringify({ terminal_font_size: 255, reduced_motion: false }));
    injector.prepare("INSERT INTO setting(key,value) VALUES('synthetic.sensitive.v1',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
      .run(JSON.stringify({ synthetic: fakeSecret }));
  } finally { injector.close(); }
  const original = snapshot();
  expect(original.integrity).toBe('ok');
  let completedChecks = 0;
  let sawShapeAnomaly = false;
  let lastExplicitStatus;
  for (let check = 0; check < 6; check++) {
    const status = await checkAlongsideTerminal();
    if (status.report?.outcome === 'completed') completedChecks++;
    sawShapeAnomaly ||= status.report?.anomalies.some(item => item.reason === 'appearance_shape') || false;
    lastExplicitStatus = status;
    await delay(100);
  }
  expect(completedChecks).toBeGreaterThan(0);
  expect(sawShapeAnomaly).toBe(true);
  expect(snapshot().digest).toBe(original.digest);
  evidence.checks.manualFiniteChecksPreserveAuthoritativeRows = true;
  evidence.checks.anomalyReportContainsNoSyntheticSensitivePayload = true;

  evidence.stage = 'busy-and-burst';
  databaseLock = new DatabaseSync(databasePath);
  databaseLock.exec('PRAGMA busy_timeout=0; BEGIN IMMEDIATE;');
  const busy = await checkAlongsideTerminal();
  expect(busy.report?.outcome).toBe('deferred_busy');
  const burstStarted = performance.now();
  const burst = Promise.all(Array.from({ length: BURST_REQUESTS }, () => invoke('maintenance_check')));
  await probeHealthyTerminal();
  const burstStatuses = await burst;
  burstReturnMs = performance.now() - burstStarted;
  expect(burstReturnMs).toBeLessThan(3000);
  burstStatuses.forEach(observeStatus);
  await probeHealthyTerminal();
  databaseLock.exec('ROLLBACK;'); databaseLock.close(); databaseLock = null;
  for (let retry = 0; retry < 3; retry++) {
    lastExplicitStatus = await checkAlongsideTerminal();
    if (lastExplicitStatus.report?.outcome === 'completed') break;
    await delay(100);
  }
  expect(lastExplicitStatus.report?.outcome).toBe('completed');
  expect(snapshot().digest).toBe(original.digest);
  evidence.checks.busyWriterDefersMaintenanceAndKeepsTerminalResponsive = true;
  evidence.checks.boundedBurstRequestsReturnAndRecoverySucceeds = true;

  evidence.stage = 'real-idle-scheduler';
  await saveEvidence();
  console.log('Maintenance smoke: real PTY is healthy; waiting for the unmodified 60-second idle scheduler.');
  // No keyboard, pointer, focus, or explicit maintenance_check occurs in this
  // interval. Status probes and DOM reads cannot manufacture an automatic tick.
  const lastManualCompletion = lastExplicitStatus.completed_at;
  const automaticDeadline = idleBeganAt + 75_000;
  let automatic;
  while (Date.now() < automaticDeadline) {
    await probeHealthyTerminal();
    const status = observeStatus(await invoke('maintenance_status'));
    if (status.completed_at !== null && status.completed_at > lastManualCompletion) {
      automatic = status; break;
    }
    await delay(250);
  }
  if (!automatic) throw new Error('No real automatic idle tick completed before the fixed observation deadline.');
  expect(automatic.report?.outcome).toBe('completed');
  expect(automatic.report.scan_progress_saved).toBe(true);
  await probeHealthyTerminal();
  expect(Date.now() - idleBeganAt).toBeGreaterThan(30_000);
  evidence.checks.unchangedIdleSchedulerRunsAnActualJob = true;
  evidence.metrics.automaticAfterLastInputMs = Date.now() - idleBeganAt;

  evidence.stage = 'final-preservation-and-close';
  const final = snapshot();
  expect(final.digest).toBe(original.digest);
  expect(final.integrity).toBe('ok');
  expect(final.operationalRows).toBe(2);
  expect(final.metadataBytes).toBeLessThanOrEqual(65 * 1024);
  expect(final.anomalyCount).toBeLessThanOrEqual(128);
  expect(await readFile(resolve(data, 'projects.json'))).toEqual(legacy);
  expect(await readdir(project)).toEqual([]);
  evidence.checks.originalRowsLegacyAndProjectFilesPreserved = true;
  evidence.checks.repeatedJobsKeepOperationalRowsAndQuarantineBounded = true;
  evidence.metrics = { ...evidence.metrics, uiRttMs: stats(uiRtts), markerAgeMs: stats(markerAges),
    observedMarkerGapMs: stats(markerGaps), burstRequests: BURST_REQUESTS,
    burstReturnMs: Math.round(burstReturnMs), explicitCompletedChecks: completedChecks,
    operationalRows: final.operationalRows, operationalMetadataBytes: final.metadataBytes,
    anomalyCount: final.anomalyCount, schema: final.schema };
  expect(pageErrors).toEqual([]);
  const closeCompleted = Promise.all([page.waitForEvent('close', { timeout: 10000 }), waitForExit(10000)]);
  // Register rejection handling immediately while the confirmation is operated.
  void closeCompleted.catch(() => {});
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  await expect(page.getByRole('alertdialog', { name: 'Close workspace?', exact: true })).toBeVisible();
  await page.getByRole('alertdialog', { name: 'Close workspace?', exact: true }).getByRole('button', { name: 'Stop', exact: true }).click();
  await closeCompleted;
  expect(app.exitCode).toBe(0); closedNormally = true;
  evidence.checks.themedCloseStopsOwnedTerminal = true;
  evidence.status = 'passed'; evidence.stage = 'done';
  await saveEvidence();
  console.log('PASS: real idle maintenance, finite repeated checks, bounded concurrent requests, busy-database recovery, preserved synthetic history and a responsive real PTY; sanitized metrics saved.');
} catch (error) {
  evidence.status = 'failed'; evidence.failureKind = error.name;
  await saveEvidence();
  console.error(`Maintenance smoke failed during ${evidence.stage}.`);
  throw error;
} finally {
  if (databaseLock) {
    try { databaseLock.exec('ROLLBACK;'); } finally { databaseLock.close(); }
  }
  if (pageOwned && page && !page.isClosed()) await invoke('terminal_shutdown').catch(() => {});
  if (!closedNormally && app && app.exitCode === null && app.signalCode === null) {
    app.kill(); await waitForExit(3000).catch(() => {});
  }
  if (browser) await browser.close().catch(() => {});
  // Delete only the unique synthetic directory created by this test, after the app exits.
  if (dirname(fixture) === resolve(tmpdir()) && basename(fixture).startsWith('shinatama-maintenance-smoke-')) {
    await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  }
}
