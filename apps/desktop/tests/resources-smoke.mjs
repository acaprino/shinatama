// C09R.1 measures the real app without injecting host pressure or starting agents.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from '@playwright/test';

if (process.platform !== 'win32') throw new Error('This resource observer check qualifies Windows only.');
const workspace = process.cwd();
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-resource-observer-'));
const data = resolve(fixture, 'data');
const port = 19237;
// Frozen before the run. This is observer qualification, not a calibrated workload policy.
const sampleForMs = 12_000;
const maxUiRoundtripMs = 1500;
const minimumCycles = 5;
const evidence = { check: 'C09R.1-real-resource-observer', checks: {}, thresholds: { sampleForMs, maxUiRoundtripMs, minimumCycles }, nativeFaultContainmentQualified: false, status: 'running' };
let app, browser, page;
const invoke = (command, args) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
async function exitWithin(milliseconds) {
  if (!app || app.exitCode !== null || app.signalCode !== null) return;
  await new Promise((done, reject) => {
    const completed = () => { clearTimeout(timer); done(); };
    const timer = setTimeout(() => { app.removeListener('exit', completed); reject(new Error('Owned observer-test app exit deadline exceeded.')); }, milliseconds);
    app.once('exit', completed);
  });
}
try {
  await new Promise((done, reject) => {
    const probe = createServer();
    probe.once('error', () => reject(new Error('Observer-test CDP port is occupied; no existing app was attached.')));
    probe.listen({ host: '127.0.0.1', port, exclusive: true }, () => probe.close(error => error ? reject(error) : done()));
  });
  app = spawn(resolve(workspace, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: workspace, windowsHide: true, stdio: 'ignore',
    env: { ...process.env, SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
  });
  let spawnError;
  app.once('error', error => { spawnError = error; });
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (spawnError || app.exitCode !== null) throw new Error('Owned observer-test app failed to start.');
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 750 }); break; }
    catch { await delay(200); }
  }
  assert.ok(browser, 'Owned observer-test WebView startup deadline exceeded.');
  const context = browser.contexts()[0];
  page = context.pages()[0] || await context.waitForEvent('page', { timeout: 5000 });
  await page.waitForURL(url => url.hostname === 'tauri.localhost', { timeout: 10000 });
  const storage = await invoke('storage_location');
  const pathKey = value => value.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();
  assert.equal(pathKey(storage.directory), pathKey(data));
  await page.getByRole('button', { name: 'Close window', exact: true }).waitFor({ state: 'visible' });
  await page.waitForFunction(() => {
    const button = document.querySelector('#window-close');
    return button && !button.disabled;
  });
  await page.keyboard.press('Shift');
  const observations = [];
  const roundtrips = [];
  const until = performance.now() + sampleForMs;
  let inputObserved = false;
  while (performance.now() < until) {
    const started = performance.now();
    const status = await invoke('resources_status');
    roundtrips.push(performance.now() - started);
    observations.push(status);
    assert.ok(!['failed', 'stopped', 'unavailable'].includes(status.phase));
    assert.ok(status.observed_processes === null || status.observed_processes <= 256);
    if (status.caller) {
      if (status.caller.input_latency_us !== null) inputObserved = true;
      assert.equal(status.caller.pending_input_events, null);
      assert.equal(status.caller.pending_output_bytes, null);
      assert.equal(status.caller.queued_launches, null);
    }
    await delay(100);
  }
  const measured = observations.filter(value => value.complete);
  evidence.lastObservation = observations.at(-1);
  assert.ok(measured.length > 0, 'No complete resource snapshot was observed.');
  const last = measured.at(-1);
  assert.ok(last.completed_cycles >= minimumCycles);
  assert.ok(last.observed_processes >= 2, 'Host and owned WebView processes must both be observed.');
  assert.ok(last.host_private_commit_bytes > 0);
  assert.ok(last.owned_private_commit_bytes >= last.host_private_commit_bytes);
  assert.ok(last.system_remaining_commit_bytes > 0);
  assert.ok(last.system_available_physical_bytes > 0);
  assert.ok(measured.some(value => value.observed_cpu_time_100ns !== null));
  assert.ok(measured.some(value => value.caller?.frame_time_us > 0 && value.caller.ipc_roundtrip_us > 0));
  assert.ok(inputObserved, 'A real numeric input-dispatch delay was not observed.');
  assert.ok(Math.max(...roundtrips) < maxUiRoundtripMs);
  assert.deepEqual(await invoke('terminal_list'), []);
  await page.locator('#resource-status').waitFor({ state: 'visible' });
  const governed = await invoke('resources_telemetry', { telemetry: {} });
  // The one-slot telemetry channel can already hold the frontend's report.
  // Refusing this extra report is bounded backpressure, not monitor failure.
  assert.equal(typeof governed.accepted, 'boolean');
  assert.equal(governed.policy.phase, 'running');
  assert.equal(governed.policy.advice_only, true);
  assert.equal(governed.policy.native_launch_enabled, false);
  assert.equal(governed.policy.native_workloads_protected, false);
  assert.equal(governed.policy.configured_profiles, 0);
  assert.equal(governed.policy.active_native_workloads, 0);
  assert.ok(governed.policy.monitor_age_ms < 3000);
  assert.ok(governed.resources.owned_private_commit_bytes > 0);
  assert.ok((await page.locator('#resource-status').getAttribute('title')).includes('Native agent launches: unavailable'));
  evidence.checks.liveGovernorAndHonestStatus = true;
  evidence.checks.nativeCountersAndProcessCoverage = true;
  evidence.checks.frontendFrameInputAndIpcMeasurements = true;
  evidence.checks.unknownQueuesRemainExplicit = true;
  evidence.checks.uiResponsiveAndNoAgentStarted = true;
  roundtrips.sort((a, b) => a - b);
  evidence.observed = {
    samples: observations.length, completeSamples: measured.length, completedCycles: last.completed_cycles,
    uiRoundtripP95Ms: roundtrips[Math.floor((roundtrips.length - 1) * 0.95)], uiRoundtripMaxMs: roundtrips.at(-1),
    maximumCycleUs: Math.max(...measured.map(value => value.cycle_elapsed_us)),
    maximumCounterReadUs: Math.max(...measured.map(value => value.sampling_elapsed_us)),
    measuredOwnedProcesses: last.observed_processes,
    hostPrivateCommitBytes: last.host_private_commit_bytes, ownedPrivateCommitBytes: last.owned_private_commit_bytes,
  };
  await page.getByRole('button', { name: 'Close window', exact: true }).click();
  await exitWithin(10000);
  assert.equal(app.exitCode, 0);
  evidence.status = 'passed';
  console.log('PASS: real resource counters, UI telemetry, live governor and responsive owned app.');
} catch (error) { evidence.status = 'failed'; throw error; }
finally {
  if (app && app.exitCode === null && app.signalCode === null) { app.kill(); await exitWithin(3000).catch(() => {}); }
  if (browser) await browser.close().catch(() => {});
  const output = resolve(workspace, '.prototype-evidence');
  await mkdir(output, { recursive: true });
  await writeFile(resolve(output, 'resources-observer.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  const absolute = resolve(fixture);
  assert.equal(dirname(absolute), resolve(tmpdir()));
  assert.ok(basename(absolute).startsWith('shinatama-resource-observer-'));
  await rm(absolute, { recursive: true, force: true, maxRetries: 8, retryDelay: 100 });
}
