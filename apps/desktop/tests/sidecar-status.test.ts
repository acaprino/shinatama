import { test } from 'node:test';
import assert from 'node:assert/strict';
import { adoptStopReply, archiveLine, buttons, capabilityLines, confirmStillNeeded, diskLine, driftLine, faultLine, indicator, indicatorTone, leverText, messageLine, nextCapabilityLines, parseCapabilities, parseStatus, phaseLabel, restartOffered, showsTraffic, stallLine, startLabel, stopConsequence, trafficLine, type CapabilityState, type Lever, type ProxyStatus } from '../src/sidecar-status.ts';

const status = (over: Partial<ProxyStatus> = {}): ProxyStatus => ({
  phase: 'ready', healthy: true, message: null, inFlight: 0, agentSessions: 0,
  drainDeadlineUnixMs: null, configDrift: false, ...over,
});

test('stop needs confirmation only while agents are live', () => {
  assert.equal(stopConsequence(status({ agentSessions: 0 })), null);
  assert.match(stopConsequence(status({ agentSessions: 1 }))!, /^1 agent session will lose/);
  assert.match(stopConsequence(status({ agentSessions: 3 }))!, /^3 agent sessions will lose/);
  assert.equal(stopConsequence(status({ agentSessions: 2 })),
    '2 agent sessions will lose the connection to the model. Requests already running are allowed to finish.');
});
test('unknown traffic is never shown as idle', () => {
  assert.equal(trafficLine(status({ inFlight: null })), 'Traffic not observed');
  assert.equal(trafficLine(status({ inFlight: 0 })), 'No request running');
  assert.equal(trafficLine(status({ inFlight: 1 })), '1 request running');
  assert.equal(trafficLine(status({ inFlight: 4 })), '4 requests running');
});
test('draining disables stop and offers cancel through start', () => {
  assert.deepEqual(buttons(status({ phase: 'draining' }), false), { start: true, stop: false, retry: false });
  assert.equal(startLabel(status({ phase: 'draining' })), 'Cancel stop');
  assert.equal(startLabel(status({ phase: 'stopped' })), 'Start');
  assert.equal(phaseLabel('draining'), 'Stopping after current requests…');
});
test('ready no longer reads as a guarantee: stop stays available, start does not', () => {
  assert.deepEqual(buttons(status({ phase: 'ready' }), false), { start: false, stop: true, retry: false });
});
test('a command in flight disables every button; stopped and failed offer the right ones', () => {
  for (const phase of ['stopped', 'ready', 'draining', 'failed']) {
    assert.deepEqual(buttons(status({ phase }), true), { start: false, stop: false, retry: false });
  }
  assert.deepEqual(buttons(status({ phase: 'stopped' }), false), { start: true, stop: false, retry: false });
  assert.deepEqual(buttons(status({ phase: 'failed' }), false), { start: true, stop: true, retry: true });
  assert.deepEqual(buttons(status({ phase: 'retired' }), false), { start: false, stop: true, retry: true });
  assert.deepEqual(buttons(null, false), { start: true, stop: false, retry: false });
  assert.deepEqual(buttons(null, true), { start: false, stop: false, retry: false });
});
test('a malformed status is rejected whole', () => {
  assert.equal(parseStatus({ phase: 'ready', healthy: 'yes' }), null);
  assert.equal(parseStatus({ phase: 'nonsense', healthy: true, message: null }), null);
  assert.equal(parseStatus(null), null);
  assert.equal(parseStatus('ready'), null);
  const good = { phase: 'ready', healthy: true, message: null, inFlight: 0, agentSessions: 0, drainDeadlineUnixMs: null, configDrift: null };
  assert.notEqual(parseStatus(good), null);
  for (const bad of [{ message: 5 }, { message: 'x'.repeat(401) }, { inFlight: -1 }, { inFlight: '1' }, { agentSessions: null },
    { agentSessions: 1.5 }, { drainDeadlineUnixMs: 'soon' }, { configDrift: 'yes' }, { refused: 'true' }]) {
    assert.equal(parseStatus({ ...good, ...bad }), null, JSON.stringify(bad));
  }
});
test('a well-formed status keeps only the fields the panel uses', () => {
  const parsed = parseStatus({ phase: 'draining', healthy: true, message: null, inFlight: 2, agentSessions: 1,
    drainDeadlineUnixMs: 1700000000000, configDrift: true, refused: true, pid: 9, observation: { secret: 1 },
    resolved: { verbosityLevel: 3 } });
  assert.deepEqual(parsed, { phase: 'draining', healthy: true, message: null, inFlight: 2, agentSessions: 1,
    drainDeadlineUnixMs: 1700000000000, configDrift: true, refused: true });
});
test('drift is stated only when it is known to be true', () => {
  assert.equal(driftLine(status({ configDrift: true })), 'Custom configuration: values differ from the Shinatama baseline.');
  assert.equal(driftLine(status({ configDrift: false })), null);
  assert.equal(driftLine(status({ configDrift: null })), null);
});
test('drift names the values read from the process', () => {
  const one = [{ setting: 'Verbosity level', read: '3', baseline: '2' }];
  assert.equal(driftLine(status({ configDrift: true, configDifferences: one })),
    'Custom configuration: Verbosity level 3 (baseline 2).');
  const six = Array.from({ length: 6 }, (_, i) => ({ setting: `S${i}`, read: 'x', baseline: 'y' }));
  assert.equal(driftLine(status({ configDrift: true, configDifferences: six })),
    'Custom configuration: S0 x (baseline y); S1 x (baseline y); S2 x (baseline y); S3 x (baseline y); and 2 more.');
  assert.equal(driftLine(status({ configDrift: false, configDifferences: one })), null);
  const parsed = parseStatus({ phase: 'ready', healthy: true, message: null, inFlight: 0, agentSessions: 0,
    drainDeadlineUnixMs: null, configDrift: true, configDifferences: one });
  assert.deepEqual(parsed?.configDifferences, one);
  for (const bad of [{ configDifferences: 'x' }, { configDifferences: Array.from({ length: 17 }, () => one[0]) }]) {
    assert.equal(parseStatus({ phase: 'ready', healthy: true, message: null, inFlight: 0, agentSessions: 0,
      drainDeadlineUnixMs: null, configDrift: true, ...bad }), null, JSON.stringify(bad));
  }
});
test('an unusable difference is skipped without rejecting the status reply', () => {
  const one = [{ setting: 'Verbosity level', read: '3', baseline: '2' }];
  const base = { phase: 'ready', healthy: true, message: null, inFlight: 0, agentSessions: 0,
    drainDeadlineUnixMs: null, configDrift: true };
  const unusable = [null, { setting: 'a', read: 1, baseline: 'b' }, { setting: 'Prompt rewrite', read: '', baseline: 'auto' },
    { setting: 'Prompt rewrite', read: 'x'.repeat(81), baseline: 'auto' }];
  const parsed = parseStatus({ ...base, configDifferences: [unusable[0], one[0], ...unusable.slice(1)] });
  assert.ok(parsed);
  assert.deepEqual(parsed.configDifferences, one);
  assert.deepEqual(parseStatus({ ...base, configDifferences: unusable })?.configDifferences, []);
});
test('an unreadable agent count asks before stopping', () => {
  const unknown = status({ agentSessions: 0, agentSessionsKnown: false });
  assert.match(stopConsequence(unknown)!, /could not be counted/);
  assert.equal(confirmStillNeeded(unknown), true);
  assert.equal(stopConsequence(status({ agentSessions: 0, agentSessionsKnown: true })), null);
  const parsed = parseStatus({ phase: 'ready', healthy: true, message: null, inFlight: 0, agentSessions: 0,
    drainDeadlineUnixMs: null, configDrift: null, agentSessionsKnown: false });
  assert.equal(parsed?.agentSessionsKnown, false);
  assert.equal(parseStatus({ phase: 'ready', healthy: true, message: null, inFlight: 0, agentSessions: 0,
    drainDeadlineUnixMs: null, configDrift: null, agentSessionsKnown: 'no' }), null);
});
test('traffic is shown only while the proxy is up in some form', () => {
  for (const phase of ['starting', 'ready', 'busy', 'draining', 'degraded', 'recovering']) assert.equal(showsTraffic(phase), true, phase);
  for (const phase of ['stopped', 'preparing', 'failed', 'retired', 'unavailable', 'nonsense']) assert.equal(showsTraffic(phase), false, phase);
});
test('a stop reply is adopted only when the panel saw the proxy up', () => {
  for (const phase of ['ready', 'busy', 'starting', 'draining', 'degraded', 'recovering', 'preparing']) assert.equal(adoptStopReply(phase), true, phase);
  for (const phase of ['stopped', 'failed', 'retired', 'unavailable']) assert.equal(adoptStopReply(phase), false, phase);
  assert.equal(adoptStopReply(undefined), false);
});
test('the confirmation stays only while a stop is possible and costs agents', () => {
  assert.equal(confirmStillNeeded(status({ agentSessions: 2 })), true);
  assert.equal(confirmStillNeeded(status({ agentSessions: 0 })), false);
  assert.equal(confirmStillNeeded(status({ agentSessions: 2, phase: 'draining' })), false);
  assert.equal(confirmStillNeeded(status({ agentSessions: 2, phase: 'stopped' })), false);
  assert.equal(confirmStillNeeded(null), false);
});

const lever = (over: Partial<Lever> = {}): Lever => ({ configured: true, applied: true, reason: 'none', ...over });
const observed = (over: Record<string, Lever> = {}): CapabilityState => ({ state: 'observed', capabilities: {
  ccr: lever(), readOriginalImage: lever(), imageRescale: lever(), ...over } });
const wire = (over: Record<string, unknown> = {}) => ({ schemaVersion: 1, protocol: 'anthropic', requests: 3, profile: null, passthrough: false,
  ccr: lever(), readOriginalImage: lever(), imageRescale: lever(), steeringLevel: 2, effortRouting: false, promptRewriteMode: 'auto',
  stampedAtRequest: 3, ...over });

test('unobserved_session_renders_unknown', () => {
  assert.deepEqual(capabilityLines({ state: 'not_observed' }),
    ['Not observed yet: this session has sent no request through the model proxy.']);
});
test('an unavailable proxy state is one fixed line', () => {
  assert.deepEqual(capabilityLines({ state: 'unavailable' }), ['Model proxy state unavailable.']);
});
test('a configured but disarmed lever states why', () => {
  assert.equal(leverText('Large-result storage', { configured: true, applied: false, reason: 'codex_host' }),
    'Large-result storage: configured, not active here — this agent cannot call the retrieval tool');
});
test('an unknown reason is never shown raw', () => {
  assert.match(leverText('Image downscaling', { configured: true, applied: false, reason: '<x>' }), /reason not reported$/);
  for (const reason of ['constructor', '__proto__', 'toString', '']) {
    assert.match(leverText('Image downscaling', { configured: true, applied: false, reason }), /reason not reported$/, reason);
  }
});
test('every known reason has its fixed wording', () => {
  const expected: Record<string, string> = {
    codex_host: 'this agent cannot call the retrieval tool', plugin_wire_not_allowed: 'not supported on this connection',
    inject_tool_off: 'tool injection is disabled', text_protocol: 'this connection has no tool channel',
    passthrough: 'transformations are paused', encoder_missing: 'the image encoder is not installed', not_observed: 'not attempted yet',
  };
  for (const [reason, text] of Object.entries(expected)) {
    assert.equal(leverText('X', { configured: true, applied: false, reason }), `X: configured, not active here — ${text}`, reason);
  }
});
test('an applied lever is active whatever else it reports', () => {
  assert.equal(leverText('Large-result storage', lever()), 'Large-result storage: active');
  assert.equal(leverText('Large-result storage', { configured: null, applied: true, reason: 'none' }), 'Large-result storage: active');
});
test('a lever that is not configured is off', () => {
  assert.equal(leverText('Image downscaling', { configured: false, applied: false, reason: 'none' }), 'Image downscaling: off');
  assert.equal(leverText('Image downscaling', { configured: false, applied: null, reason: 'not_observed' }), 'Image downscaling: off');
});
test('an unreported configuration is read from the reason: a reason means configured, none means off', () => {
  // configured === null and applied !== true: "configured, not active here" only when a real reason is set.
  assert.equal(leverText('Image downscaling', { configured: null, applied: null, reason: 'not_observed' }),
    'Image downscaling: configured, not active here — not attempted yet');
  assert.equal(leverText('Image downscaling', { configured: null, applied: false, reason: 'encoder_missing' }),
    'Image downscaling: configured, not active here — the image encoder is not installed');
  assert.equal(leverText('Image downscaling', { configured: null, applied: false, reason: 'none' }), 'Image downscaling: off');
  assert.equal(leverText('Image downscaling', { configured: null, applied: null, reason: '' }), 'Image downscaling: off');
});
test('a stale stamp reads as not attempted yet, not as off', () => {
  assert.equal(leverText('Original image retrieval', { configured: true, applied: null, reason: 'not_observed' }),
    'Original image retrieval: configured, not active here — not attempted yet');
});
test('observed state names the three levers in order and never claims a saving', () => {
  const lines = capabilityLines(observed({ readOriginalImage: lever({ applied: false, reason: 'codex_host' }), imageRescale: lever({ configured: false, applied: false }) }));
  assert.deepEqual(lines, [
    'Large-result storage: active',
    'Original image retrieval: configured, not active here — this agent cannot call the retrieval tool',
    'Image downscaling: off',
    'Economic saving is not measured.',
  ]);
  assert.equal(capabilityLines(observed()).at(-1), 'Economic saving is not measured.');
  for (const state of [{ state: 'not_observed' }, { state: 'unavailable' }] as CapabilityState[]) {
    assert.ok(!capabilityLines(state).some(line => /saving/i.test(line)), state.state);
  }
});
test('a well-formed capability reply is read, keeping only what the lines use', () => {
  const parsed = parseCapabilities({ state: 'observed', capabilities: wire({ secret: 'x' }) });
  assert.deepEqual(parsed, { state: 'observed', capabilities: { ccr: lever(), readOriginalImage: lever(), imageRescale: lever() } });
  assert.deepEqual(parseCapabilities({ state: 'not_observed' }), { state: 'not_observed' });
  assert.deepEqual(parseCapabilities({ state: 'unavailable' }), { state: 'unavailable' });
  assert.deepEqual(parseCapabilities({ state: 'not_observed', capabilities: wire() }), { state: 'not_observed' });
});
test('a malformed capability reply is unavailable, never half-trusted', () => {
  const unavailable = { state: 'unavailable' };
  for (const bad of [null, undefined, 'observed', 5, [], {}, { state: 'nonsense' }, { state: 'observed' }, { state: 'observed', capabilities: null },
    { state: 'observed', capabilities: wire({ ccr: null }) }, { state: 'observed', capabilities: wire({ ccr: { configured: true, applied: true } }) },
    { state: 'observed', capabilities: wire({ ccr: { configured: 'yes', applied: true, reason: 'none' } }) },
    { state: 'observed', capabilities: wire({ readOriginalImage: { configured: true, applied: 1, reason: 'none' } }) },
    { state: 'observed', capabilities: wire({ imageRescale: { configured: true, applied: true, reason: 5 } }) },
    { state: 'observed', capabilities: wire({ imageRescale: { configured: true, applied: true, reason: 'x'.repeat(200) } }) },
    { state: 'observed', capabilities: wire({ imageRescale: undefined }) }]) {
    assert.deepEqual(parseCapabilities(bad), unavailable, JSON.stringify(bad));
  }
});

test('a poll tick repaints only when the capability lines changed', () => {
  const same = observed();
  const first = nextCapabilityLines(null, same);
  assert.deepEqual(first, capabilityLines(same));
  assert.equal(nextCapabilityLines(first, observed()), null);
  assert.equal(nextCapabilityLines(capabilityLines({ state: 'unavailable' }), { state: 'unavailable' }), null);
  const changed = nextCapabilityLines(first, observed({ ccr: lever({ applied: false, reason: 'codex_host' }) }));
  assert.equal(changed?.[0], 'Large-result storage: configured, not active here — this agent cannot call the retrieval tool');
  assert.deepEqual(nextCapabilityLines(first, { state: 'not_observed' }), capabilityLines({ state: 'not_observed' }));
  assert.deepEqual(nextCapabilityLines([], same), capabilityLines(same));
});
test('the traffic light follows the phase, and an unknown phase is dark', () => {
  for (const phase of ['ready', 'busy']) assert.equal(indicatorTone(phase), 'green');
  for (const phase of ['preparing', 'starting', 'recovering', 'draining', 'degraded']) assert.equal(indicatorTone(phase), 'amber');
  for (const phase of ['failed', 'retired']) assert.equal(indicatorTone(phase), 'red');
  for (const phase of ['stopped', 'unavailable', 'nonsense', 'toString']) assert.equal(indicatorTone(phase), 'off');
});

// Why the proxy last went down: it used to be read nowhere, and the panel only said "Recovering…".
import { exitLine } from '../src/sidecar-status.ts';
const reply = (over: Record<string, unknown> = {}) => ({
  phase: 'recovering', healthy: false, message: null, inFlight: null, agentSessions: 2,
  drainDeadlineUnixMs: null, configDrift: null, ...over,
});
const clock = () => '12:41:47';
test('the panel says why and when the proxy process last ended', () => {
  const refused = parseStatus(reply({ lastExit: { code: 1, uptimeMs: 412, atUnixMs: 1795001224667, count: 14,
    detail: "bili: failed to start: Error: EBUSY: resource busy or locked, open 'D:\data\ca\combined-ca.pem'" } }))!;
  assert.equal(exitLine(refused, clock),
    "Last interruption at 12:41:47: the proxy process ended with code 1 after less than a second. It said: bili: failed to start: Error: EBUSY: resource busy or locked, open 'D:\data\ca\combined-ca.pem' Interruptions since Shinatama started: 14.");
  const silent = parseStatus(reply({ phase: 'ready', healthy: true, lastExit: { code: 134, uptimeMs: 881_000, atUnixMs: null, count: 1, detail: null } }))!;
  assert.equal(exitLine(silent, clock), 'Last interruption: the proxy process ended with code 134 after 14 min. It reported no error.');
  const lasted = (uptimeMs: number) => exitLine(parseStatus(reply({ lastExit: { code: 1, uptimeMs, atUnixMs: null, count: 1, detail: null } }))!, clock)!;
  assert.match(lasted(999), /after less than a second\./);
  assert.match(lasted(59_999), /after 59 s\./);
  assert.match(lasted(3_599_999), /after 59 min\./);
  assert.match(lasted(3 * 3_600_000 + 5 * 60_000), /after 3 h 5 min\./);
});
test('a reply without an exit, or from an older backend, shows no interruption line', () => {
  assert.equal(exitLine(parseStatus(reply())!, clock), null);
  assert.equal(exitLine(parseStatus(reply({ lastExit: null }))!, clock), null);
});
test('a malformed exit is not half-trusted', () => {
  const exit = { code: 1, uptimeMs: 5, atUnixMs: null, count: 1, detail: null };
  for (const bad of [{ code: -1 }, { code: 1.5 }, { uptimeMs: '5' }, { atUnixMs: 'now' }, { count: 0 }, { detail: 7 }, { detail: 'x'.repeat(301) }, { detail: 'two\nlines' }]) {
    assert.equal(parseStatus(reply({ lastExit: { ...exit, ...bad } })), null, JSON.stringify(bad));
  }
  assert.equal(parseStatus(reply({ lastExit: 'exited' })), null);
});

test('a stalled supervisor offers Restart and says since when, a fresh failed one does not', () => {
  const clock = () => '20:52:40';
  const stalled = status({ phase: 'unavailable', healthy: false, stalled: true, observedAtUnixMs: 1, lastPhase: 'recovering' });
  assert.equal(restartOffered(stalled, 0), true);
  assert.equal(restartOffered(status({ phase: 'failed', healthy: false, stalled: false }), 0), false);
  assert.equal(restartOffered(null, 2), false);
  assert.equal(restartOffered(null, 3), true);
  assert.equal(stallLine(stalled, clock), 'The proxy supervisor has not reported since 20:52:40 (last state: Recovering…). Restart Shinatama to start a new proxy.');
  assert.equal(stallLine(status(), clock), null);
  assert.deepEqual(indicator(stalled), { tone: 'red', label: 'Model proxy: Not reporting' });
  assert.deepEqual(indicator(status()), { tone: 'green', label: 'Model proxy: Ready' });
});
test('fault, disk and the default message say only what is known', () => {
  assert.equal(faultLine(status({ fault: 'boom' })), 'The proxy supervisor stopped on an internal error: boom');
  assert.equal(faultLine(status()), null);
  const MiB = 1024 * 1024;
  assert.equal(diskLine(status({ disk: { root: 'D:', freeBytes: 5 * MiB, level: 'full' } })), 'Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama.');
  assert.equal(diskLine(status({ disk: { root: 'D:', freeBytes: 512 * MiB, level: 'low' } })), 'Disk D: has 512 MB free.');
  assert.equal(diskLine(status({ disk: { root: 'D:', freeBytes: 9e9, level: 'ok' } })), null);
  assert.equal(diskLine(status()), null);
  assert.equal(messageLine(status({ healthy: false, message: null })), null);
  assert.equal(messageLine(status({ healthy: true, message: null })), 'Local service is responding.');
});
test('the new fields are validated like the rest of the reply', () => {
  const ok = { phase: 'ready', healthy: true, message: null, inFlight: 0, agentSessions: 0, drainDeadlineUnixMs: null, configDrift: false };
  assert.ok(parseStatus({ ...ok, stalled: true, observedAtUnixMs: 5, lastPhase: 'recovering', fault: null, disk: { root: 'D:', freeBytes: 1, level: 'full' } }));
  for (const bad of [{ stalled: 'yes' }, { lastPhase: 'nope' }, { fault: 'a\nb' }, { fault: 'x'.repeat(301) },
    { disk: { root: 'D:', freeBytes: -1, level: 'low' } }, { disk: { root: 'D:', freeBytes: 1, level: 'huge' } }, { disk: { root: '', freeBytes: 1, level: 'low' } }, { observedAtUnixMs: 'now' }]) {
    assert.equal(parseStatus({ ...ok, ...bad }), null, JSON.stringify(bad));
  }
});

test('the panel names the archive size and the last cleanup', () => {
  const at = Date.UTC(2026, 10, 20, 21, 30, 0);
  const clock = (unixMs: number) => `T${unixMs}`;
  const archive = (over: Record<string, unknown> = {}) => parseStatus({ ...status(), archive: { fileBytes: 18731008, lastRunUnixMs: at, lastRemoved: 487, lastLeft: 0, ...over } });
  assert.equal(archiveLine(archive()!, clock), `Archive 18.7 MB · last cleanup at T${at} removed 487 sessions`);
  assert.equal(archiveLine(archive({ lastRemoved: 1 })!, clock), `Archive 18.7 MB · last cleanup at T${at} removed 1 session`);
  assert.equal(archiveLine(archive({ lastRemoved: 0, lastLeft: 12 })!, clock), `Archive 18.7 MB · last cleanup at T${at} removed nothing; 12 left for the next one`);
  assert.equal(archiveLine(archive({ fileBytes: null })!, clock), `Archive size unknown · last cleanup at T${at} removed 487 sessions`);
  assert.equal(archiveLine(parseStatus({ ...status(), archive: null })!, clock), null);
  assert.equal(archiveLine(status(), clock), null);
  assert.equal(archive({ fileBytes: -1 }), null, 'a malformed archive is not half-trusted');
  assert.equal(archive({ lastRemoved: 'many' }), null);
});
test('cleaning its archive is a phase the panel knows, with no Start or Stop', () => {
  const cleaning = parseStatus({ ...status(), phase: 'maintaining', healthy: false });
  assert.ok(cleaning, 'the phase is accepted');
  assert.equal(phaseLabel('maintaining'), 'Cleaning its archive…');
  assert.deepEqual(buttons(cleaning, false), { start: false, stop: false, retry: false });
  assert.equal(indicatorTone('maintaining'), 'amber');
  assert.equal(showsTraffic('maintaining'), false);
});
