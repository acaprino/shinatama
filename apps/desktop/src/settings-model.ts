// What the Settings panel sends and receives. Secrets never come back from the backend:
// the page only learns that one exists, and can replace it, keep it or clear it.

export type SecretEdit = { op: 'keep' } | { op: 'set'; value: string } | { op: 'clear' };
export type ViewVariable = { name: string; secret: boolean; value: string | null; hasValue: boolean };
export type SettingsView = { fullAccess: boolean; githubUsername: string; hasGithubToken: boolean; variables: ViewVariable[] };
export type SaveRequest = {
  fullAccess: boolean; githubUsername: string; githubToken: SecretEdit;
  variables: { name: string; secret: boolean; value: SecretEdit }[];
};
/** One editable variable row. `touched` is set once the user types in its value box. */
export type Row = { name: string; secret: boolean; value: string; hasValue: boolean; touched: boolean };
export type Form = { fullAccess: boolean; githubUsername: string; tokenInput: string; removeToken: boolean; hasToken: boolean; rows: Row[] };

const NAME = /^[A-Za-z_][A-Za-z0-9_]{0,127}$/;
const GITHUB_USER = /^(?!-)[A-Za-z0-9-]{1,39}$/;
export const MAX_VARIABLES = 32;

export function parseView(value: unknown): SettingsView | null {
  if (typeof value !== 'object' || value === null) return null;
  const data = value as Record<string, unknown>;
  if (typeof data.fullAccess !== 'boolean' || typeof data.githubUsername !== 'string' || typeof data.hasGithubToken !== 'boolean' || !Array.isArray(data.variables)) return null;
  const variables: ViewVariable[] = [];
  for (const item of data.variables) {
    if (typeof item !== 'object' || item === null) return null;
    const row = item as Record<string, unknown>;
    if (typeof row.name !== 'string' || typeof row.secret !== 'boolean' || typeof row.hasValue !== 'boolean') return null;
    if (row.value !== null && row.value !== undefined && typeof row.value !== 'string') return null;
    variables.push({ name: row.name, secret: row.secret, value: typeof row.value === 'string' ? row.value : null, hasValue: row.hasValue });
  }
  return { fullAccess: data.fullAccess, githubUsername: data.githubUsername, hasGithubToken: data.hasGithubToken, variables };
}

export function formFromView(view: SettingsView): Form {
  return {
    fullAccess: view.fullAccess, githubUsername: view.githubUsername, tokenInput: '', removeToken: false, hasToken: view.hasGithubToken,
    rows: view.variables.map(variable => ({ name: variable.name, secret: variable.secret, value: variable.value ?? '', hasValue: variable.hasValue, touched: false })),
  };
}

/** Turns the form into a request, or says what is wrong in one sentence. */
export function buildSave(form: Form): { request: SaveRequest } | { problem: string } {
  const username = form.githubUsername.trim();
  if (username && !GITHUB_USER.test(username)) return { problem: 'The GitHub username may contain only letters, digits and hyphens.' };
  const token = form.tokenInput.trim();
  if (/\s/.test(token)) return { problem: 'The GitHub token must not contain spaces.' };
  const githubToken: SecretEdit = token ? { op: 'set', value: token } : form.removeToken ? { op: 'clear' } : { op: 'keep' };
  const variables: SaveRequest['variables'] = [];
  const seen = new Set<string>();
  for (const row of form.rows) {
    const name = row.name.trim();
    if (!name && !row.value && !row.hasValue) continue; // an empty row left behind
    if (!NAME.test(name)) return { problem: name ? `${name} is not a valid name: use letters, digits and underscores, and do not start with a digit.` : 'A variable has no name.' };
    const folded = name.toUpperCase();
    if (seen.has(folded)) return { problem: `${name} appears twice.` };
    seen.add(folded);
    if (row.touched || !row.hasValue) {
      if (!row.value) return { problem: `${name} needs a value.` };
      variables.push({ name, secret: row.secret, value: { op: 'set', value: row.value } });
    } else variables.push({ name, secret: row.secret, value: { op: 'keep' } });
  }
  if (variables.length > MAX_VARIABLES) return { problem: `At most ${MAX_VARIABLES} variables.` };
  return { request: { fullAccess: form.fullAccess, githubUsername: username, githubToken, variables } };
}

/** A one-line summary of what the next sessions will receive. */
export function summary(form: Form): string {
  const parts = [form.fullAccess ? 'full access' : 'agents keep their own protections'];
  const github = form.tokenInput.trim() ? true : form.hasToken && !form.removeToken;
  if (github) parts.push('GitHub sign-in');
  const count = form.rows.filter(row => row.name.trim()).length;
  if (count) parts.push(`${count} variable${count === 1 ? '' : 's'}`);
  return parts.join(' · ');
}

// Memory safety: whether a start first checks the free memory Windows measures, and how much
// it leaves free. Saved apart from the launch settings (memory_settings_load/save).
export type MemorySafety = { enabled: boolean; marginMib: number };
export const DEFAULT_MEMORY: MemorySafety = { enabled: true, marginMib: 128 };
export const MAX_MARGIN_MIB = 16384;

export function parseMemory(value: unknown): MemorySafety | null {
  if (typeof value !== 'object' || value === null) return null;
  const data = value as Record<string, unknown>;
  if (typeof data.enabled !== 'boolean' || !Number.isInteger(data.marginMib)) return null;
  const marginMib = data.marginMib as number;
  if (marginMib < 0 || marginMib > MAX_MARGIN_MIB) return null;
  return { enabled: data.enabled, marginMib };
}

/** The memory request from the checkbox and the margin box, or one sentence on what is wrong. */
export function buildMemory(enabled: boolean, marginText: string): { request: MemorySafety } | { problem: string } {
  const text = marginText.trim();
  if (!/^\d{1,5}$/.test(text)) return { problem: `The memory margin must be a whole number of MiB between 0 and ${MAX_MARGIN_MIB}.` };
  const marginMib = Number(text);
  if (marginMib > MAX_MARGIN_MIB) return { problem: `The memory margin must be a whole number of MiB between 0 and ${MAX_MARGIN_MIB}.` };
  return { request: { enabled, marginMib } };
}

/** How the next start treats memory, for the status line. */
export function memorySummary(memory: MemorySafety): string {
  return memory.enabled ? `memory check with ${memory.marginMib} MiB kept free` : 'no memory check';
}
