import { test } from 'node:test';
import assert from 'node:assert/strict';
import { adoptStopReply, buttons, confirmStillNeeded, driftLine, parseStatus, phaseLabel, showsTraffic, startLabel, stopConsequence, trafficLine, type ProxyStatus } from '../src/sidecar-status.ts';

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
