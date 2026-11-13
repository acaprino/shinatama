// Pure decisions for the Model proxy panel: the panel calls these and nothing else decides.
export type ProxyStatus = {
  phase: string; healthy: boolean; message: string | null;
  inFlight: number | null; agentSessions: number;
  drainDeadlineUnixMs: number | null; configDrift: boolean | null;
  refused?: boolean;
  /** Values the running process reported; shown only while they differ from the baseline. */
  resolved?: Record<string, string | number | boolean>;
};

const LABELS: Record<string, string> = {
  stopped: 'Stopped', preparing: 'Preparing…', starting: 'Starting…', ready: 'Ready',
  busy: 'Busy', draining: 'Stopping after current requests…', degraded: 'Temporarily unavailable', recovering: 'Recovering…',
  failed: 'Stopped after a failure', retired: 'Recovery needs attention', unavailable: 'Status unavailable',
};
const MESSAGE_LIMIT = 400;
const isCount = (value: unknown): value is number => typeof value === 'number' && Number.isInteger(value) && value >= 0;

/** The accepted shape of a status reply, or null: a reply is never half-trusted. */
export function parseStatus(value: unknown): ProxyStatus | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const raw = value as Record<string, unknown>;
  if (typeof raw.phase !== 'string' || !Object.hasOwn(LABELS, raw.phase) || typeof raw.healthy !== 'boolean') return null;
  if (!(raw.message === null || (typeof raw.message === 'string' && raw.message.length <= MESSAGE_LIMIT))) return null;
  if (!(raw.inFlight === null || isCount(raw.inFlight)) || !isCount(raw.agentSessions)) return null;
  if (!(raw.drainDeadlineUnixMs === null || (typeof raw.drainDeadlineUnixMs === 'number' && Number.isFinite(raw.drainDeadlineUnixMs)))) return null;
  if (!(raw.configDrift === null || typeof raw.configDrift === 'boolean')) return null;
  if (raw.refused !== undefined && typeof raw.refused !== 'boolean') return null;
  const status: ProxyStatus = {
    phase: raw.phase, healthy: raw.healthy, message: raw.message, inFlight: raw.inFlight, agentSessions: raw.agentSessions,
    drainDeadlineUnixMs: raw.drainDeadlineUnixMs, configDrift: raw.configDrift,
  };
  if (raw.refused !== undefined) status.refused = raw.refused;
  if (raw.resolved && typeof raw.resolved === 'object' && !Array.isArray(raw.resolved)) {
    const kept: Record<string, string | number | boolean> = {};
    for (const [key, entry] of Object.entries(raw.resolved)) {
      if (typeof entry === 'number' || typeof entry === 'boolean' || (typeof entry === 'string' && entry.length <= 64)) kept[key] = entry;
    }
    if (Object.keys(kept).length > 0) status.resolved = kept;
  }
  return status;
}

export function phaseLabel(phase: string): string {
  return Object.hasOwn(LABELS, phase) ? LABELS[phase] : LABELS.unavailable;
}

/** True = enabled. While a command from the panel is outstanding nothing is. */
export function buttons(status: ProxyStatus | null, outstanding: boolean): { start: boolean; stop: boolean; retry: boolean } {
  if (outstanding) return { start: false, stop: false, retry: false };
  if (status === null) return { start: true, stop: false, retry: false };
  const { phase } = status;
  return {
    // While draining, Start is "Cancel stop".
    start: ['stopped', 'failed', 'draining'].includes(phase),
    stop: !['stopped', 'unavailable', 'draining'].includes(phase),
    retry: ['failed', 'retired'].includes(phase),
  };
}

export function startLabel(status: ProxyStatus | null): string {
  return status?.phase === 'draining' ? 'Cancel stop' : 'Start';
}

/** What stopping would cost, or null when no confirmation is needed. */
export function stopConsequence(status: ProxyStatus): string | null {
  const n = status.agentSessions;
  return n > 0 ? `${n} agent session${n === 1 ? '' : 's'} will lose the connection to the model. Requests already running are allowed to finish.` : null;
}

/** Unknown traffic is never reported as idle. */
export function trafficLine(status: ProxyStatus): string {
  const n = status.inFlight;
  if (n === null) return 'Traffic not observed';
  return n === 0 ? 'No request running' : `${n} request${n === 1 ? '' : 's'} running`;
}

export function driftLine(status: ProxyStatus): string | null {
  return status.configDrift === true ? 'Custom configuration: values differ from the Shinatama baseline.' : null;
}
