import type { SecretEdit } from './settings-model.ts';

export const APIS = ['openai-completions', 'openai-responses', 'anthropic-messages', 'google-generative-ai'] as const;
export type Api = typeof APIS[number];
export type Model = { id: string; contextWindow: number; maxTokens: number; reasoning: boolean; vision: boolean };
export type Connection = { id: string; name: string; api: Api; baseUrl: string; models: Model[] };
export type Selection = { connectionId: string; modelId: string };
export const HARNESSES = ['opencode', 'pi', 'hermes'] as const;
export type Harness = typeof HARNESSES[number];
export type Assignments = Record<Harness, Selection | null>;
export type View = { revision: number; connections: { connection: Connection; hasApiKey: boolean }[]; assignments: Assignments };
export type Edit = { connection: Connection; hasApiKey: boolean; keyInput: string; removeKey: boolean };
export type Form = { revision: number; connections: Edit[]; assignments: Assignments };
export type Save = { revision: number; connections: { connection: Connection; apiKey: SecretEdit }[]; assignments: Assignments };
export const MAX_CONNECTIONS = 16;
export const MAX_MODELS = 64;
export const PRESETS: { name: string; api: Api; baseUrl: string }[] = [
  { name: 'OpenAI', api: 'openai-responses', baseUrl: 'https://api.openai.com/v1' },
  { name: 'Anthropic', api: 'anthropic-messages', baseUrl: 'https://api.anthropic.com' },
  { name: 'Google Gemini', api: 'google-generative-ai', baseUrl: 'https://generativelanguage.googleapis.com/v1beta' },
  { name: 'OpenRouter', api: 'openai-completions', baseUrl: 'https://openrouter.ai/api/v1' },
  { name: 'DeepSeek', api: 'openai-completions', baseUrl: 'https://api.deepseek.com/v1' },
  { name: 'Ollama', api: 'openai-completions', baseUrl: 'http://localhost:11434/v1' },
  { name: 'LM Studio', api: 'openai-completions', baseUrl: 'http://localhost:1234/v1' },
  { name: 'Custom endpoint', api: 'openai-completions', baseUrl: '' },
];
export const newModel = (): Model => ({ id: '', contextWindow: 128000, maxTokens: 8192, reasoning: false, vision: false });
export const emptyForm = (): Form => ({ revision: 0, connections: [], assignments: { opencode: null, pi: null, hermes: null } });
export function fromView(view: View): Form {
  return { revision: view.revision, assignments: structuredClone(view.assignments), connections: view.connections.map(c => ({
    connection: structuredClone(c.connection), hasApiKey: c.hasApiKey, keyInput: '', removeKey: false,
  })) };
}

const object = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null;
export function parseModelView(value: unknown): View | null {
  if (!object(value) || !Number.isSafeInteger(value.revision) || Number(value.revision) < 0 || !Array.isArray(value.connections) || !object(value.assignments)) return null;
  const connections: View['connections'] = [];
  for (const item of value.connections) {
    if (!object(item) || !object(item.connection) || typeof item.hasApiKey !== 'boolean') return null;
    const c = item.connection;
    if (typeof c.id !== 'string' || typeof c.name !== 'string' || typeof c.baseUrl !== 'string' || !APIS.some(api => api === c.api) || !Array.isArray(c.models)) return null;
    const models: Model[] = [];
    for (const m of c.models) {
      if (!object(m) || typeof m.id !== 'string' || !Number.isInteger(m.contextWindow) || !Number.isInteger(m.maxTokens) || typeof m.reasoning !== 'boolean' || typeof m.vision !== 'boolean') return null;
      models.push({ id: m.id, contextWindow: Number(m.contextWindow), maxTokens: Number(m.maxTokens), reasoning: m.reasoning, vision: m.vision });
    }
    connections.push({ connection: { id: c.id, name: c.name, baseUrl: c.baseUrl, api: c.api as Api, models }, hasApiKey: item.hasApiKey });
  }
  const assignments: Assignments = { opencode: null, pi: null, hermes: null };
  for (const harness of HARNESSES) {
    const s = value.assignments[harness];
    if (s === null) continue;
    if (!object(s) || typeof s.connectionId !== 'string' || typeof s.modelId !== 'string') return null;
    assignments[harness] = { connectionId: s.connectionId, modelId: s.modelId };
  }
  const view = { revision: Number(value.revision), connections, assignments };
  return 'problem' in buildModelSave(fromView(view)) ? null : view;
}

const byteLength = (text: string) => new TextEncoder().encode(text).length;
export function buildModelSave(form: Form): { request: Save } | { problem: string } {
  if (form.connections.length > MAX_CONNECTIONS) return { problem: 'At most 16 connections are allowed.' };
  const connections: Save['connections'] = []; const ids = new Set<string>();
  for (const edit of form.connections) {
    const connection = structuredClone(edit.connection);
    connection.name = connection.name.trim(); connection.baseUrl = connection.baseUrl.trim();
    if (!/^[A-Za-z0-9_.][A-Za-z0-9_.-]{0,63}$/.test(connection.id) || ids.has(connection.id)) return { problem: 'Connection ids must be valid and unique.' };
    ids.add(connection.id);
    if (!connection.name || byteLength(connection.name) > 100 || /[{}\p{Cc}]/u.test(connection.name)) return { problem: 'Give each connection a name of at most 100 bytes without braces or control characters.' };
    if (!APIS.includes(connection.api)) return { problem: 'Choose a supported API protocol.' };
    try {
      const url = new URL(connection.baseUrl);
      if (!/^https?:\/\//.test(connection.baseUrl) || !['http:', 'https:'].includes(url.protocol) || !url.hostname || url.username || url.password || url.search || url.hash
        || byteLength(connection.baseUrl) > 2048 || /[{}\\\s\p{Cc}]/u.test(connection.baseUrl)) throw new Error();
    } catch { return { problem: `${connection.name}: use an HTTP(S) base URL without credentials, query parameters or fragments.` }; }
    if (!connection.models.length || connection.models.length > MAX_MODELS) return { problem: `${connection.name}: add between 1 and 64 models.` };
    const models = new Set<string>();
    for (const model of connection.models) {
      model.id = model.id.trim();
      if (!/^[A-Za-z0-9_./:][A-Za-z0-9_./:-]{0,199}$/.test(model.id) || models.has(model.id)) return { problem: `${connection.name}: model IDs must be unique and use letters, digits, dots, underscores, hyphens, slashes or colons.` };
      models.add(model.id);
      if (!Number.isInteger(model.contextWindow) || model.contextWindow < 1 || model.contextWindow > 10000000 || !Number.isInteger(model.maxTokens) || model.maxTokens < 1 || model.maxTokens > model.contextWindow)
        return { problem: `${connection.name}: output must be between 1 and the context limit (maximum 10,000,000 tokens).` };
    }
    const key = edit.keyInput.trim();
    if (byteLength(key) > 4096 || /[\s\p{Cc}]/u.test(key)) return { problem: `${connection.name}: the API key is too long or contains spaces.` };
    connections.push({ connection, apiKey: key ? { op: 'set', value: key } : edit.removeKey ? { op: 'clear' } : { op: 'keep' } });
  }
  for (const harness of HARNESSES) {
    const selection = form.assignments[harness];
    if (selection && !connections.some(c => c.connection.id === selection.connectionId && c.connection.models.some(m => m.id === selection.modelId)))
      return { problem: `Choose an existing model for ${harness}, or select native settings.` };
  }
  return { request: { revision: form.revision, connections, assignments: structuredClone(form.assignments) } };
}
