// Pure decisions for the Model proxy panel: the panel calls these and nothing else decides.
export type ProxyStatus = {
  phase: string; healthy: boolean; message: string | null;
  inFlight: number | null; agentSessions: number;
  drainDeadlineUnixMs: number | null; configDrift: boolean | null;
  refused?: boolean;
  /** Absent when the backend sent none: an older reply is still a valid reply. */
  agentSessionsKnown?: boolean;
  /** The values read from the running process that differ from the baseline. */
  configDifferences?: ConfigDifference[];
};
export type ConfigDifference = { setting: string; read: string; baseline: string };

const LABELS: Record<string, string> = {
  stopped: 'Stopped', preparing: 'Preparing…', starting: 'Starting…', ready: 'Ready',
  busy: 'Busy', draining: 'Stopping after current requests…', degraded: 'Temporarily unavailable', recovering: 'Recovering…',
  failed: 'Stopped after a failure', retired: 'Recovery needs attention', unavailable: 'Status unavailable',
};
const RUNNING = ['starting', 'ready', 'busy', 'draining', 'degraded', 'recovering'];
const DOWN = ['stopped', 'failed', 'retired', 'unavailable'];
const MESSAGE_LIMIT = 400;
const DIFFERENCE_LIMIT = 16;
const DIFFERENCE_TEXT_LIMIT = 80;
const isCount = (value: unknown): value is number => typeof value === 'number' && Number.isInteger(value) && value >= 0;

function parseDifferences(value: unknown): ConfigDifference[] | null {
  if (!Array.isArray(value) || value.length > DIFFERENCE_LIMIT) return null;
  const out: ConfigDifference[] = [];
  for (const item of value) {
    // One unusable item is skipped; it never costs the panel the whole reply.
    if (!item || typeof item !== 'object' || Array.isArray(item)) continue;
    const { setting, read, baseline } = item as Record<string, unknown>;
    if ([setting, read, baseline].some((text) => typeof text !== 'string' || text.length === 0 || text.length > DIFFERENCE_TEXT_LIMIT)) continue;
    out.push({ setting: setting as string, read: read as string, baseline: baseline as string });
  }
  return out;
}

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
  if (raw.agentSessionsKnown !== undefined && typeof raw.agentSessionsKnown !== 'boolean') return null;
  const differences = raw.configDifferences === undefined ? undefined : parseDifferences(raw.configDifferences);
  if (differences === null) return null;
  const status: ProxyStatus = {
    phase: raw.phase, healthy: raw.healthy, message: raw.message, inFlight: raw.inFlight, agentSessions: raw.agentSessions,
    drainDeadlineUnixMs: raw.drainDeadlineUnixMs, configDrift: raw.configDrift,
  };
  if (raw.refused !== undefined) status.refused = raw.refused;
  if (raw.agentSessionsKnown !== undefined) status.agentSessionsKnown = raw.agentSessionsKnown;
  if (differences !== undefined) status.configDifferences = differences;
  return status;
}

export function phaseLabel(phase: string): string {
  return Object.hasOwn(LABELS, phase) ? LABELS[phase] : LABELS.unavailable;
}

/** The indicator light's tone for a phase: live steady, busy and transit sweep, warn and fault blink, off is dark. */
export type IndicatorTone = 'live' | 'busy' | 'transit' | 'warn' | 'fault' | 'off';
const TONES: Record<string, IndicatorTone> = {
  ready: 'live', busy: 'busy', preparing: 'transit', starting: 'transit', recovering: 'transit',
  draining: 'warn', degraded: 'warn', failed: 'fault', retired: 'fault', stopped: 'off', unavailable: 'off',
};
export function indicatorTone(phase: string): IndicatorTone {
  return Object.hasOwn(TONES, phase) ? TONES[phase] : 'off';
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
  // An unreadable count is not "no agents": ask, as for a live one.
  if (status.agentSessionsKnown === false) {
    return 'Open agent sessions could not be counted, so stopping may cut them off from the model. Requests already running are allowed to finish.';
  }
  const n = status.agentSessions;
  return n > 0 ? `${n} agent session${n === 1 ? '' : 's'} will lose the connection to the model. Requests already running are allowed to finish.` : null;
}

/** Unknown traffic is never reported as idle. */
export function trafficLine(status: ProxyStatus): string {
  const n = status.inFlight;
  if (n === null) return 'Traffic not observed';
  return n === 0 ? 'No request running' : `${n} request${n === 1 ? '' : 's'} running`;
}

const DRIFT_SHOWN = 4;
export function driftLine(status: ProxyStatus): string | null {
  if (status.configDrift !== true) return null;
  const found = status.configDifferences ?? [];
  if (found.length === 0) return 'Custom configuration: values differ from the Shinatama baseline.';
  const shown = found.slice(0, DRIFT_SHOWN).map((item) => `${item.setting} ${item.read} (baseline ${item.baseline})`);
  const more = found.length > DRIFT_SHOWN ? `; and ${found.length - DRIFT_SHOWN} more` : '';
  return `Custom configuration: ${shown.join('; ')}${more}.`;
}

/** Traffic is meaningful only while the proxy is up in some form. */
export function showsTraffic(phase: string): boolean {
  return RUNNING.includes(phase);
}

/** A stop answered for a proxy the panel believed down (or never saw) says nothing true about it:
 * the reply is not adopted and the next poll decides. */
export function adoptStopReply(previousPhase: string | undefined): boolean {
  return previousPhase !== undefined && !DOWN.includes(previousPhase);
}

/** The inline confirmation stays only while a stop is possible and would cost agent sessions. */
export function confirmStillNeeded(status: ProxyStatus | null): boolean {
  return status !== null && stopConsequence(status) !== null && buttons(status, false).stop;
}

// Per-session capabilities: what the proxy was configured to do for one session versus what its
// last served request actually applied. The lines are the only place this is worded.
export type Lever = { configured: boolean | null; applied: boolean | null; reason: string };
export type Capabilities = { ccr: Lever; readOriginalImage: Lever; imageRescale: Lever };
export type CapabilityState =
  | { state: 'observed'; capabilities: Capabilities }
  | { state: 'not_observed' } | { state: 'unavailable' };

const REASONS: Record<string, string> = {
  codex_host: 'this agent cannot call the retrieval tool', plugin_wire_not_allowed: 'not supported on this connection',
  inject_tool_off: 'tool injection is disabled', text_protocol: 'this connection has no tool channel',
  passthrough: 'transformations are paused', encoder_missing: 'the image encoder is not installed', not_observed: 'not attempted yet',
};
const REASON_LIMIT = 64;
const LEVERS: [keyof Capabilities, string][] = [
  ['ccr', 'Large-result storage'], ['readOriginalImage', 'Original image retrieval'], ['imageRescale', 'Image downscaling'],
];

function parseLever(value: unknown): Lever | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const raw = value as Record<string, unknown>;
  const flag = (v: unknown): v is boolean | null => v === null || typeof v === 'boolean';
  if (!flag(raw.configured) || !flag(raw.applied) || typeof raw.reason !== 'string' || raw.reason.length > REASON_LIMIT) return null;
  return { configured: raw.configured, applied: raw.applied, reason: raw.reason };
}

/** The accepted shape of a `sidecar_capabilities` reply; anything malformed is `unavailable`. */
export function parseCapabilities(value: unknown): CapabilityState {
  const unavailable: CapabilityState = { state: 'unavailable' };
  if (!value || typeof value !== 'object' || Array.isArray(value)) return unavailable;
  const raw = value as Record<string, unknown>;
  if (raw.state === 'not_observed') return { state: 'not_observed' };
  if (raw.state === 'unavailable') return unavailable;
  if (raw.state !== 'observed' || !raw.capabilities || typeof raw.capabilities !== 'object' || Array.isArray(raw.capabilities)) return unavailable;
  const source = raw.capabilities as Record<string, unknown>;
  const ccr = parseLever(source.ccr); const readOriginalImage = parseLever(source.readOriginalImage); const imageRescale = parseLever(source.imageRescale);
  if (!ccr || !readOriginalImage || !imageRescale) return unavailable;
  return { state: 'observed', capabilities: { ccr, readOriginalImage, imageRescale } };
}

/** One lever. Applied wins. Otherwise `configured === false` is off, and `configured === true` is
 * "configured, not active here". An unreported configuration (`null`) is read from the reason: a real
 * reason (neither empty nor "none") means the lever was configured but is not active, no reason means off.
 * A reason outside the fixed list is never shown raw. */
export function leverText(name: string, lever: Lever): string {
  if (lever.applied === true) return `${name}: active`;
  const reasoned = lever.reason !== '' && lever.reason !== 'none';
  if (lever.configured === true || (lever.configured === null && reasoned)) {
    const why = Object.hasOwn(REASONS, lever.reason) ? REASONS[lever.reason] : 'reason not reported';
    return `${name}: configured, not active here — ${why}`;
  }
  return `${name}: off`;
}

export function capabilityLines(read: CapabilityState): string[] {
  if (read.state === 'not_observed') return ['Not observed yet: this session has sent no request through the model proxy.'];
  if (read.state === 'unavailable') return ['Model proxy state unavailable.'];
  return [...LEVERS.map(([key, name]) => leverText(name, read.capabilities[key])), 'Economic saving is not measured.'];
}

/** The lines a poll tick should paint, or null when they equal what is already shown: an unchanged
 * reading touches nothing, and a changed one replaces only the capability lines. */
export function nextCapabilityLines(shown: readonly string[] | null, read: CapabilityState): string[] | null {
  const lines = capabilityLines(read);
  return shown !== null && shown.length === lines.length && shown.every((line, index) => line === lines[index]) ? null : lines;
}
