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
  /** How the proxy process last ended by itself; kept after it recovers. */
  lastExit?: ProxyExit;
  /** When the supervisor last reported anything; null when it never did. */
  observedAtUnixMs?: number | null;
  /** The supervisor has not reported for more than 15 s while it should be working. */
  stalled?: boolean;
  /** The phase it last reported, sent only while it is stalled. */
  lastPhase?: string | null;
  /** The internal error that stopped the supervisor, in its own words. */
  fault?: string | null;
  /** Free space of the disk that holds Shinatama's data; absent until the first sample. */
  disk?: DiskStatus | null;
  /** The proxy's archive after the last cleanup run; absent before one ran. */
  archive?: ArchiveStatus | null;
};
export type ArchiveStatus = { fileBytes: number | null; lastRunUnixMs: number | null; lastRemoved: number; lastLeft: number };
export type DiskStatus = { root: string; freeBytes: number; level: 'ok' | 'low' | 'full' };
/** `detail` is what the runtime last said outside the proxy's own log, when it said anything. */
export type ProxyExit = { code: number; uptimeMs: number; atUnixMs: number | null; detail: string | null; count: number };
export type ConfigDifference = { setting: string; read: string; baseline: string };

const LABELS: Record<string, string> = {
  stopped: 'Stopped', preparing: 'Preparing…', starting: 'Starting…', ready: 'Ready',
  busy: 'Busy', draining: 'Stopping after current requests…', maintaining: 'Cleaning its archive…', degraded: 'Temporarily unavailable', recovering: 'Recovering…',
  failed: 'Stopped after a failure', retired: 'Recovery needs attention', unavailable: 'Status unavailable',
};
const RUNNING = ['starting', 'ready', 'busy', 'draining', 'degraded', 'recovering'];
const DOWN = ['stopped', 'failed', 'retired', 'unavailable'];
const MESSAGE_LIMIT = 400;
const DIFFERENCE_LIMIT = 16;
const DIFFERENCE_TEXT_LIMIT = 80;
const EXIT_DETAIL_LIMIT = 300;
const FAULT_LIMIT = 300;
const DISK_ROOT_LIMIT = 64;
const DISK_LEVELS = ['ok', 'low', 'full'];
const isCount = (value: unknown): value is number => typeof value === 'number' && Number.isInteger(value) && value >= 0;
/** One line: the backend never sends a control character, so one here is not its reply. */
const isLine = (value: unknown, limit: number): value is string => typeof value === 'string' && value.length <= limit && !/[\u0000-\u001f\u007f]/.test(value);

/** `undefined` for no run yet (absent or null), `null` for a malformed one. */
function parseArchive(value: unknown): ArchiveStatus | undefined | null {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'object' || Array.isArray(value)) return null;
  const { fileBytes, lastRunUnixMs, lastRemoved, lastLeft } = value as Record<string, unknown>;
  if (!(fileBytes === null || isCount(fileBytes)) || !isCount(lastRemoved) || !isCount(lastLeft)) return null;
  if (!(lastRunUnixMs === null || (typeof lastRunUnixMs === 'number' && Number.isFinite(lastRunUnixMs)))) return null;
  return { fileBytes, lastRunUnixMs, lastRemoved, lastLeft };
}

/** `undefined` for no reading (absent or null), `null` for a malformed one. */
function parseDisk(value: unknown): DiskStatus | undefined | null {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'object' || Array.isArray(value)) return null;
  const { root, freeBytes, level } = value as Record<string, unknown>;
  if (typeof root !== 'string' || root.length === 0 || !isLine(root, DISK_ROOT_LIMIT)) return null;
  if (!isCount(freeBytes) || typeof level !== 'string' || !DISK_LEVELS.includes(level)) return null;
  return { root, freeBytes, level: level as DiskStatus['level'] };
}

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

/** `undefined` for no exit (absent or null), `null` for a malformed one. */
function parseExit(value: unknown): ProxyExit | undefined | null {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'object' || Array.isArray(value)) return null;
  const { code, uptimeMs, atUnixMs, detail, count } = value as Record<string, unknown>;
  if (!isCount(code) || !isCount(uptimeMs) || !isCount(count) || count === 0) return null;
  if (!(atUnixMs === null || (typeof atUnixMs === 'number' && Number.isFinite(atUnixMs)))) return null;
  // One line: the backend never sends a control character, so one here is not its reply.
  if (!(detail === null || (typeof detail === 'string' && detail.length <= EXIT_DETAIL_LIMIT && !/[\u0000-\u001f\u007f]/.test(detail)))) return null;
  return { code, uptimeMs, atUnixMs, detail: detail === '' ? null : detail, count };
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
  const lastExit = parseExit(raw.lastExit);
  if (lastExit === null) return null;
  if (!(raw.observedAtUnixMs === undefined || raw.observedAtUnixMs === null || (typeof raw.observedAtUnixMs === 'number' && Number.isFinite(raw.observedAtUnixMs)))) return null;
  if (raw.stalled !== undefined && typeof raw.stalled !== 'boolean') return null;
  if (!(raw.lastPhase === undefined || raw.lastPhase === null || (typeof raw.lastPhase === 'string' && Object.hasOwn(LABELS, raw.lastPhase)))) return null;
  if (!(raw.fault === undefined || raw.fault === null || isLine(raw.fault, FAULT_LIMIT))) return null;
  const disk = parseDisk(raw.disk);
  if (disk === null) return null;
  const archive = parseArchive(raw.archive);
  if (archive === null) return null;
  const status: ProxyStatus = {
    phase: raw.phase, healthy: raw.healthy, message: raw.message, inFlight: raw.inFlight, agentSessions: raw.agentSessions,
    drainDeadlineUnixMs: raw.drainDeadlineUnixMs, configDrift: raw.configDrift,
  };
  if (raw.refused !== undefined) status.refused = raw.refused;
  if (raw.agentSessionsKnown !== undefined) status.agentSessionsKnown = raw.agentSessionsKnown;
  if (differences !== undefined) status.configDifferences = differences;
  if (lastExit !== undefined) status.lastExit = lastExit;
  if (raw.observedAtUnixMs !== undefined) status.observedAtUnixMs = raw.observedAtUnixMs as number | null;
  if (raw.stalled !== undefined) status.stalled = raw.stalled;
  if (raw.lastPhase !== undefined) status.lastPhase = raw.lastPhase as string | null;
  if (raw.fault !== undefined) status.fault = raw.fault === '' ? null : raw.fault as string | null;
  if (disk !== undefined) status.disk = disk;
  if (archive !== undefined) status.archive = archive;
  return status;
}

export function phaseLabel(phase: string): string {
  return Object.hasOwn(LABELS, phase) ? LABELS[phase] : LABELS.unavailable;
}

/** The traffic light for a phase: green serving, amber in transit or degraded, red failed, off when down. */
export type IndicatorTone = 'green' | 'amber' | 'red' | 'off';
const TONES: Record<string, IndicatorTone> = {
  ready: 'green', busy: 'green', preparing: 'amber', starting: 'amber', recovering: 'amber', draining: 'amber', maintaining: 'amber', degraded: 'amber',
  failed: 'red', retired: 'red', stopped: 'off', unavailable: 'off',
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
    // While it cleans its archive, Start or Stop ends the run after the command in progress.
    start: ['stopped', 'failed', 'draining', 'maintaining'].includes(phase),
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

function lasted(ms: number): string {
  if (ms < 1000) return 'less than a second';
  if (ms < 60_000) return `${Math.floor(ms / 1000)} s`;
  const minutes = Math.floor(ms / 60_000);
  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}
const wallClock = (unixMs: number) => new Date(unixMs).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

/** Why the proxy last went down, in the proxy's own words when it gave any. Shown while it
 * recovers and after: "it went down again" is otherwise a fact with no cause. */
export function exitLine(status: ProxyStatus, clock: (unixMs: number) => string = wallClock): string | null {
  const exit = status.lastExit;
  if (!exit) return null;
  const when = exit.atUnixMs === null ? '' : ` at ${clock(exit.atUnixMs)}`;
  const said = exit.detail === null ? 'It reported no error.' : `It said: ${exit.detail}`;
  const repeated = exit.count > 1 ? ` Interruptions since Shinatama started: ${exit.count}.` : '';
  return `Last interruption${when}: the proxy process ended with code ${exit.code} after ${lasted(exit.uptimeMs)}. ${said}${repeated}`;
}

/** Restart Shinatama is the one exit that works when the supervisor no longer answers: offered while
 * its status is stalled, or after three status reads in a row failed. Never for a fresh Failed. */
export function restartOffered(status: ProxyStatus | null, failedPolls: number): boolean {
  return status?.stalled === true || failedPolls >= 3;
}

/** Since when a stalled supervisor has been silent and what it said last. */
export function stallLine(status: ProxyStatus, clock: (unixMs: number) => string = wallClock): string | null {
  if (status.stalled !== true) return null;
  const last = status.lastPhase ? ` (last state: ${phaseLabel(status.lastPhase)})` : '';
  const silent = typeof status.observedAtUnixMs === 'number'
    ? `has not reported since ${clock(status.observedAtUnixMs)}${last}`
    : `has stopped reporting${last}`;
  // Its last message is history, not the present: it is framed here and not shown on its own.
  const said = status.message ? ` It last said: ${status.message}` : '';
  return `The proxy supervisor ${silent}.${said} Restart Shinatama to start a new proxy.`;
}

export function faultLine(status: ProxyStatus): string | null {
  return status.fault ? `The proxy supervisor stopped on an internal error: ${status.fault}` : null;
}

/** The proxy archive's size and what its last cleanup did; nothing before a cleanup ran. */
export function archiveLine(status: ProxyStatus, clock: (unixMs: number) => string = wallClock): string | null {
  const archive = status.archive;
  if (!archive) return null;
  const size = archive.fileBytes === null ? 'Archive size unknown' : `Archive ${(archive.fileBytes / 1e6).toFixed(1)} MB`;
  const when = archive.lastRunUnixMs === null ? 'last cleanup' : `last cleanup at ${clock(archive.lastRunUnixMs)}`;
  const removed = archive.lastRemoved === 0 ? 'removed nothing'
    : `removed ${archive.lastRemoved} session${archive.lastRemoved === 1 ? '' : 's'}`;
  const left = archive.lastLeft > 0 ? `; ${archive.lastLeft} left for the next one` : '';
  return `${size} · ${when} ${removed}${left}`;
}
/** A disk that is low or full, named; nothing while the disk is fine or not yet measured. */
export function diskLine(status: ProxyStatus): string | null {
  const disk = status.disk;
  if (!disk || disk.level === 'ok') return null;
  const megabytes = Math.floor(disk.freeBytes / 2 ** 20);
  return disk.level === 'full'
    ? `Disk ${disk.root} is full (${megabytes} MB free): saving fails for the proxy and for Shinatama.`
    : `Disk ${disk.root} has ${megabytes} MB free.`;
}

/** The backend's message; without one, only what is known: a healthy proxy is responding. */
export function messageLine(status: ProxyStatus | null): string | null {
  if (status?.stalled) return null;
  return status?.message ?? (status?.healthy ? 'Local service is responding.' : null);
}

/** The toolbar light: a stalled supervisor is red, whatever phase it last reported. */
export function indicator(status: ProxyStatus | null): { tone: IndicatorTone; label: string } {
  if (status?.stalled) return { tone: 'red', label: 'Model proxy: Not reporting' };
  const phase = status?.phase ?? 'unavailable';
  return { tone: indicatorTone(phase), label: `Model proxy: ${phaseLabel(phase)}` };
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
