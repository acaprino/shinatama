import { invoke } from '@tauri-apps/api/core';

export type TerminalResourcePhase = 'starting' | 'running' | 'closing' | 'ended' | 'retired' | 'released';
export type TerminalResourceReason = 'awaiting_measurement' | 'observed' | 'partial_sample' | 'observation_unavailable'
  | 'resource_limit' | 'stop_requested' | 'process_ended' | 'admission_refused' | 'local_failure'
  | 'cleanup_unconfirmed' | 'released' | 'cache_busy';
type Observation = {
  privateCommitBytes: number; residentBytes: number; peakJobCommitBytes: number; activeProcesses: number;
  cpuTime100ns: number; membershipComplete: boolean;
};
type Limits = { jobCommitBytes: number; processCommitBytes: number; memoryLimitEnforced: boolean; processes: number; cpuBasisPoints: number; queueBytes: number; queueItems: number };
type Queues = { bytes: number; items: number; peakBytes: number; peakItems: number; refusals: number; closed: boolean };
export type TerminalResourceStatus = {
  id: string; phase: TerminalResourcePhase; observed: Observation | null; applied: Limits | null;
  queues: Queues | null; sampleAgeMs: number | null; reason: TerminalResourceReason;
};
const PHASES = new Set(['starting', 'running', 'closing', 'ended', 'retired', 'released']);
const REASONS: Record<TerminalResourceReason, string> = {
  awaiting_measurement: 'Waiting for a resource measurement', observed: 'Resource consumption measured',
  partial_sample: 'Partial measurement; current totals are a lower bound', observation_unavailable: 'Resource measurement unavailable',
  resource_limit: 'Stopped for its attributed resource limit; saved work is preserved', stop_requested: 'Stop requested',
  process_ended: 'Shell ended', admission_refused: 'Resource admission refused', local_failure: 'A local terminal operation failed',
  cleanup_unconfirmed: 'Cleanup unconfirmed; its resource reservation remains held', released: 'Cleanup confirmed; reservation released',
  cache_busy: 'Cached resource information is temporarily unavailable',
};
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
function numbers<T extends Record<string, unknown>>(value: unknown, names: readonly string[]): value is T {
  return record(value) && names.every(name => integer(value[name]));
}
/** Copy only the fixed redacted fields. No paths, identities, native errors or payloads. */
export function decodeTerminalResources(value: unknown): readonly TerminalResourceStatus[] | null {
  if (!Array.isArray(value) || value.length > 8) return null;
  const result: TerminalResourceStatus[] = [];
  const ids = new Set<string>();
  for (const raw of value) {
    if (!record(raw) || typeof raw.id !== 'string' || !/^(?:[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}|i_[a-z2-7]{12})$/i.test(raw.id)
      || ids.has(raw.id) || typeof raw.phase !== 'string' || !PHASES.has(raw.phase)
      || typeof raw.reason !== 'string' || !Object.hasOwn(REASONS, raw.reason)
      || (raw.sampleAgeMs !== null && !integer(raw.sampleAgeMs))) return null;
    let observed: Observation | null = null;
    if (raw.observed !== null) {
      const item = raw.observed;
      if (!numbers(item, ['privateCommitBytes', 'residentBytes', 'peakJobCommitBytes', 'activeProcesses', 'cpuTime100ns'])
        || typeof item.membershipComplete !== 'boolean' || (item.activeProcesses as number) > 256) return null;
      observed = { privateCommitBytes: item.privateCommitBytes as number, residentBytes: item.residentBytes as number,
        peakJobCommitBytes: item.peakJobCommitBytes as number, activeProcesses: item.activeProcesses as number,
        cpuTime100ns: item.cpuTime100ns as number, membershipComplete: item.membershipComplete };
    }
    let applied: Limits | null = null;
    if (raw.applied !== null) {
      const item = raw.applied;
      if (!numbers(item, ['jobCommitBytes', 'processCommitBytes', 'processes', 'cpuBasisPoints', 'queueBytes', 'queueItems'])
        || typeof item.memoryLimitEnforced !== 'boolean'
        || (item.processes as number) < 1 || (item.processes as number) > 256 || (item.cpuBasisPoints as number) < 1
        || (item.cpuBasisPoints as number) > 10_000 || (item.processCommitBytes as number) > (item.jobCommitBytes as number)) return null;
      applied = { jobCommitBytes: item.jobCommitBytes as number, processCommitBytes: item.processCommitBytes as number,
        memoryLimitEnforced: item.memoryLimitEnforced,
        processes: item.processes as number, cpuBasisPoints: item.cpuBasisPoints as number,
        queueBytes: item.queueBytes as number, queueItems: item.queueItems as number };
    }
    let queues: Queues | null = null;
    if (raw.queues !== null) {
      const item = raw.queues;
      if (!numbers(item, ['bytes', 'items', 'peakBytes', 'peakItems', 'refusals']) || typeof item.closed !== 'boolean') return null;
      queues = { bytes: item.bytes as number, items: item.items as number, peakBytes: item.peakBytes as number,
        peakItems: item.peakItems as number, refusals: item.refusals as number, closed: item.closed };
    }
    ids.add(raw.id);
    result.push({ id: raw.id, phase: raw.phase as TerminalResourcePhase, reason: raw.reason as TerminalResourceReason,
      observed, applied, queues, sampleAgeMs: raw.sampleAgeMs as number | null });
  }
  // All retained strings are validated ASCII UUIDs/enums, so chars equal bytes.
  return JSON.stringify(result).length <= 8192 ? result : null;
}

/** One request at a time. A stalled request never starts replacement requests. */
export function startTerminalResources(
  callback: (statuses: readonly TerminalResourceStatus[] | null) => void,
  transport: () => Promise<unknown> = () => invoke('terminal_resources'),
): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const poll = async () => {
    let statuses: readonly TerminalResourceStatus[] | null = null;
    try { statuses = decodeTerminalResources(await transport()); } catch { /* No native error/payload reaches presentation. */ }
    if (stopped) return;
    try { callback(statuses); } catch { /* A renderer callback cannot create a polling backlog. */ }
    finally { if (!stopped) timer = setTimeout(() => { void poll(); }, 1000); }
  };
  void poll();
  return () => { stopped = true; if (timer !== undefined) clearTimeout(timer); };
}

const mib = (bytes: number) => `${Math.round(bytes / 1024 / 1024)} MiB`;
export function formatTerminalResources(status: TerminalResourceStatus): { summary: string; detail: string } {
  const { observed, applied, queues } = status;
  const partial = observed && !observed.membershipComplete ? '≥' : '';
  const age = status.sampleAgeMs;
  const stale = age !== null && age > 1500;
  const usage = observed ? `${partial}${mib(observed.privateCommitBytes)}` : 'unavailable';
  const summary = `${stale ? 'Last measured ' : ''}${usage}${applied?.memoryLimitEnforced ? ` / ${mib(applied.jobCommitBytes)}` : ''} · ${status.phase}`;
  const details = [REASONS[status.reason], `Phase: ${status.phase}`];
  if (observed) {
    details.push(`Private commit: ${partial}${mib(observed.privateCommitBytes)}`,
      `Summed working sets (includes shared pages): ${partial}${mib(observed.residentBytes)}`,
      `Peak job commit: ${mib(observed.peakJobCommitBytes)}`, `Processes: ${observed.activeProcesses}${applied ? ` / ${applied.processes}` : ''}`,
      `Accumulated CPU time: ${(observed.cpuTime100ns / 10_000_000).toFixed(1)} s`);
  } else details.push('Current consumption: unavailable');
  if (applied) details.push(applied.memoryLimitEnforced
    ? `Fixed applied memory limits: ${mib(applied.jobCommitBytes)} per job, ${mib(applied.processCommitBytes)} per process`
    : 'Memory monitored; no enforced memory cap',
    `CPU cap: ${(applied.cpuBasisPoints / 100).toFixed(1)}% of the machine; maximum ${applied.processes} processes`,
    `Queue ceilings: ${applied.queueBytes} bytes / ${applied.queueItems} items`);
  else details.push('Applied limits: unavailable');
  if (queues) details.push(`Queues: ${queues.bytes} bytes / ${queues.items} items${queues.closed ? ' (closed)' : ''}; refusals: ${queues.refusals}`);
  if (age !== null) details.push(`Measurement age: ${(age / 1000).toFixed(1)} s${stale ? ' (stale)' : ''}`);
  if (status.phase === 'retired') details.push('Capacity remains reserved; no automatic restart');
  return { summary, detail: details.join('\n') };
}
