// The answers of the browser for the settings (see mock/index.ts): the presentation, the launch settings (full access, GitHub, variables), the
// shared models (one connection with two models), memory safety, Muro, the model proxy, the check of an agent and the switches of Sessions.
// What is saved is kept for the next read, as Rust keeps it, until the page is loaded again.
import type { Answers } from './index.ts';

type Json = Record<string, unknown>;
let launch: Json = {
  fullAccess: true, githubUsername: 'acme-dev', hasGithubToken: true,
  variables: [
    { name: 'NPM_CONFIG_REGISTRY', secret: false, value: 'https://npm.acme.internal', hasValue: true },
    { name: 'DB_PASSWORD', secret: true, value: null, hasValue: true },
  ],
};
let models: Json = {
  revision: 1,
  connections: [{
    connection: {
      id: 'acme', name: 'Acme gateway', api: 'openai-completions', baseUrl: 'https://llm.acme.internal/v1',
      models: [
        { id: 'gpt-5.5-mini', contextWindow: 128000, maxTokens: 16384, reasoning: true, vision: false },
        { id: 'qwen3-coder-30b', contextWindow: 262144, maxTokens: 32768, reasoning: false, vision: false },
      ],
    },
    hasApiKey: true,
  }],
  assignments: { opencode: null, pi: null, hermes: null },
};
let memory: Json = { enabled: true, marginMib: 1024 };
let titles = true;
let indexing = false;
let proxyPhase = 'ready';

const proxy = () => ({
  phase: proxyPhase, healthy: proxyPhase === 'ready', message: null, inFlight: proxyPhase === 'ready' ? 0 : null, agentSessions: 2, agentSessionsKnown: true,
  drainDeadlineUnixMs: null, stalled: false, refused: false, configDrift: false,
});

export const answers: Answers = {
  presentation_load: { revision: 1, settings: { version: 1, skin: 'shinatama', followSkin: false, fontFamily: 'profile', fontSize: 13, lineHeight: 115 } },
  presentation_save: (args) => ({ revision: 2, settings: (args?.request as { settings: unknown }).settings }),
  storage_location: { directory: 'C:\\Users\\me\\AppData\\Roaming\\Shinatama', database_path: 'C:\\Users\\me\\AppData\\Roaming\\Shinatama\\shinatama.db', read_only: false, message: null },
  attention_configure: {},
  attention_preview: null,
  maintenance_activity: null,
  maintenance_status: { phase: 'idle', completed_at: null, message: null },
  launch_settings_load: () => launch,
  launch_settings_save: (args) => {
    const request = args?.request as { fullAccess: boolean; githubUsername: string; githubToken: { op: string }; variables: { name: string; secret: boolean; value: { op: string; value?: string } }[] };
    launch = {
      fullAccess: request.fullAccess, githubUsername: request.githubUsername, hasGithubToken: request.githubToken.op === 'set' || (request.githubToken.op === 'keep' && launch.hasGithubToken === true),
      variables: request.variables.map(variable => ({ name: variable.name, secret: variable.secret, value: variable.secret ? null : variable.value.value ?? null, hasValue: true })),
    };
    return launch;
  },
  model_settings_load: () => models,
  model_settings_save: (args) => {
    const request = args?.request as { revision: number; connections: { connection: unknown; apiKey: { op: string } }[]; assignments: unknown };
    models = { revision: request.revision + 1, connections: request.connections.map(item => ({ connection: item.connection, hasApiKey: item.apiKey.op !== 'clear' })), assignments: request.assignments };
    return models;
  },
  memory_settings_load: () => memory,
  memory_settings_save: (args) => { memory = args?.request as Json; return memory; },
  muro_status: { state: 'running', message: null, sync: { lastCheck: Math.floor(Date.now() / 1000) - 600, lastSync: Math.floor(Date.now() / 1000) - 600, pending: 0, blockers: 0, current: true, targets: 4, plugins: 6, lastRun: null } },
  muro_sync_now: { state: 'running', message: null, sync: { lastCheck: Math.floor(Date.now() / 1000), lastSync: Math.floor(Date.now() / 1000), pending: 0, blockers: 0, current: true, targets: 4, plugins: 6, lastRun: { time: Math.floor(Date.now() / 1000), failed: false, message: 'Nothing to change' } } },
  muro_open: null,
  sidecar_status: () => proxy(),
  sidecar_start: () => { proxyPhase = 'ready'; return proxy(); },
  sidecar_stop: () => { proxyPhase = 'stopped'; return proxy(); },
  sidecar_retry: () => { proxyPhase = 'ready'; return proxy(); },
  agent_check: (args) => ({
    agentId: args?.agentId, displayName: 'Claude Code', program: { state: 'found', version: '2.1.4' }, home: { state: 'ready' }, sidecar: { state: 'ready' }, sets: { state: 'missing' },
    authentication: 'authenticated', canOpen: true, canLogin: true, issues: ['The default empty set needs no optional additions.'],
  }),
  titles_enabled: () => titles,
  titles_set_enabled: (args) => { titles = args?.enabled === true; return null; },
  session_index_status: () => ({ index: { enabled: indexing } }),
  session_index_enable: (args) => { indexing = args?.enabled === true; return null; },
};
