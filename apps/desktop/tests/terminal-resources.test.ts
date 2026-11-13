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
