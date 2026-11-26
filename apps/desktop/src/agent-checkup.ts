// The check of an agent before it is opened (settings, Agents): what the backend says about its program, its private home, its model proxy
// and its sets, as redacted facts only (no paths, argv, environment, identity or CLI output). Read strictly: an answer that is not one is refused.
export type AgentId = 'claude_code' | 'codex' | 'opencode' | 'pi' | 'hermes_agent' | 'deepseek_harness';
export type StepState = 'unknown' | 'ready' | 'missing' | 'unavailable' | 'unsupported';
export type Checkup = {
  agentId: AgentId; displayName: string;
  program: { state: 'missing' | 'found' | 'unavailable'; version?: string };
  home: { state: StepState }; sidecar: { state: StepState }; sets: { state: StepState };
  authentication: 'unknown' | 'authenticated' | 'unauthenticated';
  canOpen: boolean; canLogin: boolean; issues: string[];
};
export type Provider = { agentId: AgentId; displayName: string; checkable: boolean };

/** The agents Shinatama knows, and which of them can be checked today. */
export const PROVIDERS: readonly Provider[] = [
  { agentId: 'claude_code', displayName: 'Claude Code', checkable: true },
  { agentId: 'codex', displayName: 'Codex', checkable: false },
  { agentId: 'opencode', displayName: 'OpenCode', checkable: false },
  { agentId: 'pi', displayName: 'Pi', checkable: false },
  { agentId: 'hermes_agent', displayName: 'Hermes Agent', checkable: false },
  { agentId: 'deepseek_harness', displayName: 'DeepSeek Harness', checkable: false },
];

export const STEP_LABELS: Record<StepState, string> = { unknown: 'Not checked', ready: 'Ready', missing: 'Missing', unavailable: 'Unavailable', unsupported: 'Not qualified' };
/** The tone a step's word is drawn in. */
export const stepTone = (state: string): 'success' | 'danger' | 'warning' | undefined =>
  ['ready', 'authenticated', 'found'].includes(state) ? 'success' : ['missing', 'unavailable'].includes(state) ? 'danger' : state === 'unsupported' ? 'warning' : undefined;

const text = (value: unknown, maximum: number): value is string => typeof value === 'string' && value.length > 0 && value.length <= maximum && !/[\u0000-\u001f\u007f]/.test(value);
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);

/** The backend's answer for `agentId`, or null when it is not a checkup of that agent. */
export function parseCheckup(value: unknown, agentId: AgentId): Checkup | null {
  if (!record(value) || value.agentId !== agentId || !text(value.displayName, 80) || !record(value.program)
    || typeof value.program.state !== 'string' || !['missing', 'found', 'unavailable'].includes(value.program.state)
    || (value.program.version !== undefined && (!text(value.program.version, 80) || !/^[0-9a-z.+-]+$/i.test(value.program.version)))
    || typeof value.canOpen !== 'boolean' || typeof value.canLogin !== 'boolean'
    || typeof value.authentication !== 'string' || !['unknown', 'authenticated', 'unauthenticated'].includes(value.authentication)
    || !Array.isArray(value.issues) || value.issues.length > 8 || !value.issues.every(issue => text(issue, 240))) return null;
  for (const key of ['home', 'sidecar', 'sets']) {
    const step = value[key];
    if (!record(step) || typeof step.state !== 'string' || !Object.hasOwn(STEP_LABELS, step.state)) return null;
  }
  const program = value.program as { state: Checkup['program']['state']; version?: string };
  const step = (key: string) => ({ state: (value[key] as { state: StepState }).state });
  return {
    agentId, displayName: value.displayName as string,
    program: { state: program.state, ...(program.version ? { version: program.version } : {}) },
    home: step('home'), sidecar: step('sidecar'), sets: step('sets'),
    authentication: value.authentication as Checkup['authentication'], canOpen: value.canOpen, canLogin: value.canLogin, issues: [...(value.issues as string[])],
  };
}
