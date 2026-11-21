import { uiPart, uiButton, uiSelect, uiOption, uiTraits, uiTone } from '@acaprino/imago/ui';
import { openModal } from './modal.ts';

export type AgentId = 'claude_code' | 'codex' | 'opencode' | 'pi' | 'hermes_agent' | 'deepseek_harness';
export type AgentStepState = 'unknown' | 'ready' | 'missing' | 'unavailable' | 'unsupported';
/** Backend-redacted facts only: no paths, argv, environment, identity or CLI output. */
export type AgentCheckup = {
  agentId: AgentId; displayName: string;
  program: { state: 'missing' | 'found' | 'unavailable'; version?: string };
  home: { state: AgentStepState }; sidecar: { state: AgentStepState }; sets: { state: AgentStepState };
  authentication: 'unknown' | 'authenticated' | 'unauthenticated';
  canOpen: boolean; canLogin: boolean; issues: string[];
};
export type AgentProvider = { agentId: AgentId; displayName: string; checkable: boolean };
type Options = {
  check: (agentId: AgentId) => Promise<AgentCheckup>;
  open: (agentId: AgentId) => Promise<void>;
  login: (agentId: AgentId) => Promise<void>;
  reportError: (message: string) => void;
  restoreFocus?: () => void;
  providers?: readonly AgentProvider[];
};
const PROVIDERS: readonly AgentProvider[] = [
  { agentId: 'claude_code', displayName: 'Claude Code', checkable: true },
  { agentId: 'codex', displayName: 'Codex', checkable: false },
  { agentId: 'opencode', displayName: 'OpenCode', checkable: false },
  { agentId: 'pi', displayName: 'Pi', checkable: false },
  { agentId: 'hermes_agent', displayName: 'Hermes Agent', checkable: false },
  { agentId: 'deepseek_harness', displayName: 'DeepSeek Harness', checkable: false },
];
const STEP_LABELS: Record<AgentStepState, string> = {
  unknown: 'Not checked', ready: 'Ready', missing: 'Missing', unavailable: 'Unavailable', unsupported: 'Not qualified',
};
const text = (value: unknown, maximum: number): value is string => typeof value === 'string'
  && value.length > 0 && value.length <= maximum && !/[\u0000-\u001f\u007f]/.test(value);
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
function validCheckup(value: unknown, agentId: AgentId): value is AgentCheckup {
  if (!record(value) || value.agentId !== agentId || !text(value.displayName, 80) || !record(value.program)
    || typeof value.program.state !== 'string' || !['missing', 'found', 'unavailable'].includes(value.program.state)
    || (value.program.version !== undefined && (!text(value.program.version, 80) || !/^[0-9a-z.+-]+$/i.test(value.program.version)))
    || typeof value.canOpen !== 'boolean' || typeof value.canLogin !== 'boolean'
    || typeof value.authentication !== 'string' || !['unknown', 'authenticated', 'unauthenticated'].includes(value.authentication)
    || !Array.isArray(value.issues) || value.issues.length > 8 || !value.issues.every(issue => text(issue, 240))) return false;
  return ['home', 'sidecar', 'sets'].every(key => {
    const step = value[key]; return record(step) && typeof step.state === 'string' && Object.hasOwn(STEP_LABELS, step.state);
  });
}

/** Explicit user actions only. Closed or stale results cannot start another action. */
export function createAgentSetup(options: Options) {
  const providers = (options.providers ?? PROVIDERS).slice(0, 6).filter((item, index, all) =>
    PROVIDERS.some(known => known.agentId === item.agentId) && text(item.displayName, 80)
    && typeof item.checkable === 'boolean' && all.findIndex(other => other.agentId === item.agentId) === index);
  const button = uiButton('Agents'); button.id = 'agents';
  document.querySelector('.toolbar')?.append(button);
  const dialog = uiTraits(uiPart('dialog'), 'paddedDrawer'); dialog.id = 'agent-setup-dialog'; dialog.setAttribute('aria-labelledby', 'agent-setup-title');
  const heading = uiPart('row', undefined, 'dialog-heading');
  const title = uiPart('title', 'Agents'); title.id = 'agent-setup-title';
  const dismiss = uiButton('×'); dismiss.setAttribute('aria-label', 'Close agents');
  heading.append(title, dismiss);
  const description = uiPart('paragraph', 'Check an agent before opening it. Sign-in takes place in its original terminal interface.');
  const label = uiPart('label', 'Agent'); label.htmlFor = 'agent-provider';
  const provider = uiSelect(); provider.id = 'agent-provider';
  for (const item of providers) {
    const entry = uiOption(item.agentId, ''); entry.disabled = !item.checkable;
    entry.textContent = item.displayName + (item.checkable ? '' : ' — unavailable (not qualified)'); provider.append(entry);
  }
  provider.value = providers.find(item => item.checkable)?.agentId ?? '';
  const facts = uiPart('descriptionList'); facts.setAttribute('aria-label', 'Agent checkup');
  const issues = uiTone(uiPart('list'), 'warn');
  const status = uiPart('hint'); status.id = 'agent-setup-status'; status.setAttribute('role', 'status');
  const hint = uiPart('hint', 'The default empty set needs no optional additions. Checks do not install, sign in or send a message.');
  const actions = uiPart('actions');
  const check = uiButton('Check');
  const login = uiButton('Sign in in terminal');
  const launch = uiButton('Open agent', { primary: true });
  actions.append(check, login, launch);
  dialog.append(heading, description, label, provider, facts, issues, status, hint, actions);
  (document.querySelector('#app') ?? document.body).append(dialog);
  let snapshot: AgentCheckup | null = null;
  let busy: 'check' | 'open' | 'login' | null = null;
  let generation = 0; let destroyed = false; let failure = '';
  const selected = () => providers.find(item => item.agentId === provider.value && item.checkable);
  function row(name: string, value: string, state: string) {
    const term = uiPart('term', name);
    const detail = uiPart('definition', value); detail.dataset.state = state;
    if (['ready', 'authenticated'].includes(state)) uiTone(detail, 'ok');
    if (['missing', 'unavailable', 'unsupported'].includes(state)) uiTone(detail, 'warn'); facts.append(term, detail);
  }
  function render() {
    if (destroyed) return;
    provider.disabled = busy !== null;
    check.disabled = busy !== null || !selected();
    // Authorization comes only from the backend, never inferred from these facts.
    launch.disabled = busy !== null || !snapshot?.canOpen;
    login.disabled = busy !== null || !snapshot?.canLogin;
    dialog.setAttribute('aria-busy', String(busy !== null));
    facts.replaceChildren();
    const program = snapshot?.program;
    row('Program', program?.state === 'found' ? `Found${program.version ? ` · ${program.version}` : ''}`
      : program?.state === 'missing' ? 'Missing' : program?.state === 'unavailable' ? 'Unavailable' : 'Not checked', program?.state ?? 'unknown');
    for (const [key, name] of [['home', 'Private home'], ['sidecar', 'Model proxy'], ['sets', 'Sets']] as const) {
      const state = snapshot?.[key].state ?? 'unknown'; row(name, STEP_LABELS[state], state);
    }
    const auth = snapshot?.authentication ?? 'unknown';
    row('Authentication', auth === 'authenticated' ? 'Signed in' : auth === 'unauthenticated' ? 'Not signed in' : 'Unknown', auth);
    issues.replaceChildren(...(snapshot?.issues ?? []).map(message => { const item = uiPart('item', message); return item; }));
    issues.hidden = !issues.childElementCount;
    status.textContent = failure || (busy === 'check' ? 'Checking…' : busy === 'open' ? 'Opening the agent terminal…'
      : busy === 'login' ? 'Opening sign-in in the agent terminal…' : snapshot ? 'Check complete. Available actions are confirmed by the backend.'
        : 'Not checked. Opening and sign-in remain unavailable.');
  }
  function close() { generation++; if (dialog.open) dialog.close(); }
  async function act(kind: 'check' | 'open' | 'login') {
    const chosen = selected();
    if (busy || !chosen || (kind === 'open' && !snapshot?.canOpen) || (kind === 'login' && !snapshot?.canLogin)) return;
    const at = generation; busy = kind; failure = ''; render();
    try {
      if (kind === 'check') {
        const received = await options.check(chosen.agentId);
        if (!validCheckup(received, chosen.agentId)) throw new Error('Invalid checkup');
        if (at === generation && dialog.open && !destroyed) snapshot = {
          agentId: received.agentId, displayName: received.displayName,
          program: { state: received.program.state, ...(received.program.version ? { version: received.program.version } : {}) },
          home: { state: received.home.state }, sidecar: { state: received.sidecar.state }, sets: { state: received.sets.state },
          authentication: received.authentication, canOpen: received.canOpen, canLogin: received.canLogin, issues: [...received.issues],
        };
      } else {
        await options[kind](chosen.agentId);
        if (at === generation && dialog.open && !destroyed) close();
      }
    } catch {
      const message = kind === 'check' ? 'Agent check did not complete. Retry explicitly.' : 'The agent action did not complete. Check its status before retrying.';
      if (!destroyed && at === generation && dialog.open) failure = message;
      else if (!destroyed) options.reportError(message);
      snapshot = null;
    } finally { busy = null; render(); }
  }
  function show() {
    if (destroyed || dialog.open || document.querySelector('dialog[open][aria-modal="true"]')) return;
    generation++; snapshot = null; failure = ''; render(); openModal(dialog); (selected() ? check : dismiss).focus();
  }
  button.onclick = show; dismiss.onclick = close;
  check.onclick = () => { void act('check'); }; launch.onclick = () => { void act('open'); }; login.onclick = () => { void act('login'); };
  provider.onchange = () => { generation++; snapshot = null; failure = ''; render(); };
  dialog.addEventListener('close', () => { if (!dialog.open) { generation++; if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); } });
  render();
  return { show, close, destroy: () => { destroyed = true; close(); button.remove(); dialog.remove(); } };
}
