// Pure decisions for the Plugins section of Settings: what Shinatama's Muro (muro.rs) is doing,
// in one line, and why Sync now or Open Muro cannot run. The panel calls these and nothing else decides.

export type MuroState = 'missing' | 'off' | 'starting' | 'running' | 'failed';
/** The last automatic sync or Sync now, as Muro recorded it. Times are seconds since 1970. */
export type MuroRun = { time: number | null; failed: boolean; message: string };
export type MuroSync = {
  lastCheck: number | null; lastSync: number | null; pending: number | null; blockers: number | null;
  /** Whether the last check saw the registry as it is now. */
  current: boolean;
  /** Shinatama's agents in Muro, and the plugins chosen there. */
  targets: number; plugins: number;
  lastRun: MuroRun | null;
};
export type MuroStatus = { state: MuroState; message: string | null; sync: MuroSync | null };
export type Line = { text: string; tone: 'info' | 'error' };

const STATES: readonly MuroState[] = ['missing', 'off', 'starting', 'running', 'failed'];
const count = (value: unknown) => typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
const text = (value: unknown) => typeof value === 'string' ? value : null;

/** The backend's answer, read strictly: anything else is `null`. */
export function parseMuroStatus(value: unknown): MuroStatus | null {
  const raw = value as { state?: unknown; message?: unknown; sync?: unknown } | null;
  if (!raw || typeof raw !== 'object' || !STATES.includes(raw.state as MuroState)) return null;
  const sync = raw.sync as Record<string, unknown> | null | undefined;
  let parsed: MuroSync | null = null;
  if (sync && typeof sync === 'object') {
    const run = sync.lastRun as Record<string, unknown> | null | undefined;
    parsed = {
      lastCheck: count(sync.lastCheck), lastSync: count(sync.lastSync), pending: count(sync.pending), blockers: count(sync.blockers),
      current: sync.current === true, targets: count(sync.targets) ?? 0, plugins: count(sync.plugins) ?? 0,
      lastRun: run && typeof run === 'object' ? { time: count(run.time), failed: run.failed === true, message: text(run.message) ?? '' } : null,
    };
  }
  return { state: raw.state as MuroState, message: text(raw.message), sync: parsed };
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** The status line. `clock` writes a time (seconds since 1970) for people. */
export function pluginsLine(status: MuroStatus, clock: (seconds: number) => string): Line {
  switch (status.state) {
    case 'missing': return { text: `Muro is not in this build${status.message ? ` (no Muro in ${status.message})` : ''}.`, tone: 'info' };
    case 'off': return { text: 'Muro is switched off (SHINATAMA_MURO=off).', tone: 'info' };
    case 'starting': return { text: 'Muro is starting…', tone: 'info' };
    case 'failed': return { text: `Muro did not start: ${status.message ?? 'unknown error'}`, tone: 'error' };
  }
  const sync = status.sync;
  const note = status.message ? ` ${status.message}` : '';
  if (!sync) return { text: `Muro runs in the background.${note}`, tone: status.message ? 'error' : 'info' };
  const checked = sync.lastCheck !== null ? ` · checked ${clock(sync.lastCheck)}` : '';
  const synced = sync.lastSync !== null ? ` · last sync ${clock(sync.lastSync)}` : '';
  const agents = plural(sync.targets, 'agent', 'agents');
  if (sync.lastRun?.failed) {
    const when = sync.lastRun.time !== null ? ` at ${clock(sync.lastRun.time)}` : '';
    return { text: `The last sync${when} did not finish: ${sync.lastRun.message || 'Muro gave no reason'}.${note}`, tone: 'error' };
  }
  if (sync.plugins === 0) return { text: `No plugins chosen yet for Shinatama's ${agents}: open Muro to choose them.${note}`, tone: status.message ? 'error' : 'info' };
  let state: string;
  if (sync.lastCheck === null || !sync.current) state = 'Not checked yet';
  else if ((sync.pending ?? 0) > 0) state = `${plural(sync.pending!, 'change', 'changes')} waiting`;
  else if ((sync.blockers ?? 0) > 0) state = `Needs attention: ${plural(sync.blockers!, 'problem', 'problems')} in Muro`;
  else state = 'In sync';
  const done = sync.lastRun?.message ? ` (${sync.lastRun.message})` : '';
  const checkedPart = state === 'Not checked yet' ? '' : checked;
  return {
    text: `${state}${checkedPart}${synced}${done} · ${plural(sync.plugins, 'plugin', 'plugins')} for ${agents}.${note}`,
    tone: status.message !== null || state.startsWith('Needs') ? 'error' : 'info',
  };
}

/** Why each button cannot run now, or `null` when it can. Sync now after a failed start tries the start again. */
export function pluginsButtons(status: MuroStatus | null, desktop: boolean): { sync: string | null; open: string | null } {
  if (!desktop) { const why = 'Open the desktop app to use Muro.'; return { sync: why, open: why }; }
  if (!status) { const why = 'Muro\'s state could not be read.'; return { sync: why, open: why }; }
  switch (status.state) {
    case 'missing': return { sync: 'Muro is not in this build.', open: 'Muro is not in this build.' };
    case 'off': return { sync: 'Muro is switched off.', open: 'Muro is switched off.' };
    case 'starting': return { sync: 'Muro is still starting.', open: null };
    default: return { sync: null, open: null };
  }
}
