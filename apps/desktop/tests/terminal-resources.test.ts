import assert from 'node:assert/strict';
import test from 'node:test';
import { decodeTerminalResources, formatTerminalResources } from '../src/terminal-resources.ts';

const MIB = 1024 * 1024;
const observation = (limitsEnforced: boolean) => ({
  id: 'bd1fbebc-5673-4b0b-a5fe-5206ef740dda', phase: 'running', reason: 'observed', sampleAgeMs: 100,
  observed: { privateCommitBytes: 768 * MIB, residentBytes: 500 * MIB, peakJobCommitBytes: 800 * MIB,
    activeProcesses: 3, cpuTime100ns: 10_000_000, membershipComplete: true },
  applied: { jobCommitBytes: 512 * MIB, processCommitBytes: 256 * MIB, limitsEnforced,
    processes: 8, cpuBasisPoints: 1000, queueBytes: 4096, queueItems: 16 },
  queues: null,
});

test('advisory limits preserve measured use above the reservation without presenting any cap', () => {
  const raw = observation(false);
  raw.observed.activeProcesses = 40;
  const statuses = decodeTerminalResources([raw]);
  assert.ok(statuses);
  const rendered = formatTerminalResources(statuses[0]);
  assert.equal(rendered.summary, '768 MiB · running');
  assert.match(rendered.detail, /Monitored only; no enforced memory, CPU or process cap/);
  assert.match(rendered.detail, /Processes: 40\n/);
  assert.doesNotMatch(rendered.detail, /fixed.*limits|512 MiB per job|CPU cap|maximum \d+ processes|40 \/ 8/i);
});

test('hard limits remain explicit for contained workloads', () => {
  const statuses = decodeTerminalResources([observation(true)]);
  assert.ok(statuses);
  const rendered = formatTerminalResources(statuses[0]);
  assert.equal(rendered.summary, '768 MiB / 512 MiB · running');
  assert.match(rendered.detail, /Fixed applied memory limits: 512 MiB per job, 256 MiB per process/);
  assert.match(rendered.detail, /CPU cap: 10\.0% of the machine; maximum 8 processes/);
  assert.match(rendered.detail, /Processes: 3 \/ 8/);
});

test('unknown or malformed enforcement state is not silently reported as advisory', () => {
  for (const invalid of [undefined, null, 0, 'false']) {
    const raw = observation(false);
    Object.assign(raw.applied, { limitsEnforced: invalid });
    assert.equal(decodeTerminalResources([raw]), null);
  }
});

test('advisory samples keep stale and partial measurement qualifications', () => {
  const raw = observation(false);
  raw.sampleAgeMs = 5000;
  raw.observed.membershipComplete = false;
  const statuses = decodeTerminalResources([raw]);
  assert.ok(statuses);
  const rendered = formatTerminalResources(statuses[0]);
  assert.match(rendered.summary, /^Last measured /);
  assert.match(rendered.detail, /\(stale\)/);
  assert.doesNotMatch(rendered.summary, /\/ 512 MiB/);
});

// --- what the backend says of the machine, after the window's report ------------------------------------------------------------------

import { describeResources, parseResourceHealth } from '../src/terminal-resources.ts';

const health = (stage: string, reason = 'observed') => ({
  accepted: true,
  policy: { stage, reason, reduce_optional: stage !== 'normal', defer_new_starts: stage === 'defer_starts' },
  resources: { owned_private_commit_bytes: 1_073_741_824, system_remaining_commit_bytes: null, observed_processes: 9 },
});

test('the answer to a report is read strictly: anything else is refused', () => {
  assert.equal(parseResourceHealth(null), null);
  assert.equal(parseResourceHealth({}), null);
  assert.equal(parseResourceHealth({ resources: { owned_private_commit_bytes: 'a' } }), null);
  assert.equal(parseResourceHealth({ ...health('normal'), policy: { stage: 'bogus', reason: 'x', reduce_optional: false, defer_new_starts: false } }), null);
  const parsed = parseResourceHealth(health('normal'))!;
  assert.equal(parsed.policy?.stage, 'normal');
  assert.equal(parsed.resources.system_remaining_commit_bytes, null);
  assert.equal(parseResourceHealth({ resources: {} })!.policy, null, 'no policy yet: the strip says it is checking');
});

test('the strip says checking, available or reduced activity, with the measures under it', () => {
  assert.deepEqual(describeResources(null).label, 'Resources: checking');
  assert.equal(describeResources(parseResourceHealth(health('normal', 'missing_telemetry'))).tone, 'info');
  const ok = describeResources(parseResourceHealth(health('normal')));
  assert.deepEqual([ok.label, ok.tone], ['Resources: available', 'success']);
  assert.match(ok.detail, /Observed process memory: 1024 MiB/);
  assert.match(ok.detail, /System commit headroom: unavailable/);
  const reduced = describeResources(parseResourceHealth(health('defer_starts')));
  assert.deepEqual([reduced.label, reduced.tone], ['Resources: reduced activity', 'warning']);
  assert.match(reduced.detail, /Optional background work: paused/);
  assert.match(reduced.detail, /New sessions: waiting for resources/);
});
