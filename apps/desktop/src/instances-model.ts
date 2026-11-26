// The agents that run now, as the Sessions view and the instances panel say them: the rows the backend answers, the words of their states
// and of what each is doing, and the summary line. Pure.
import { agentName } from './session-library-model.ts';

export type Git = { state: 'known'; branch: string | null; dirty: number; untracked: number } | { state: 'not_repository' } | { state: 'unknown'; reason: string };
export type InstanceRow = { id: string; agent: string; cwd: string; state: string; stateSource: string; certain: boolean; git: Git; usage: string };
export type Usage = { requests: number; promptTokens: number; outputTokens: number; hitRatePermille: number | null };
export type Collision = { kind: 'file_collision'; worktree: string; path: string; instances: string[] } | { kind: 'possible_collision'; worktree: string; path: string };
export type Titles = {
  enabled: boolean; excluded: string[]; collisions: Collision[];
  rows: { instanceId: string; threadId: string | null; title: string | null; reason: string | null; touched: number; usage: Usage | null }[];
};
export type HopLine = { id: string; state: string; mode: string; sourceAgent: string; destinationAgent: string; createdAtMs: number; updatedAtMs: number };

export const STATE_WORDS: Record<string, string> = {
  starting: 'Starting', attention_requested: 'Asked for attention', producing_output: 'Producing output', quiet: 'Quiet', exited: 'Exited', error: 'Exited with an error',
};
/** The tone of a state's badge: what asks for the person is a warning, what produces is a success. */
export function stateTone(state: string): 'success' | 'warning' | 'danger' | 'info' | undefined {
  return state === 'attention_requested' ? 'warning' : state === 'producing_output' ? 'success' : state === 'error' ? 'danger' : state === 'starting' ? 'info' : undefined;
}
export const SOURCE_WORDS: Record<string, string> = {
  process: 'from the process', terminal_signal: 'from a terminal signal, unsure', terminal_output: 'from terminal output, unsure', none: 'nothing observed yet',
};
/** Each state's place in the summary line, in order of urgency. */
const SUMMARY: [string, string][] = [
  ['attention_requested', 'asking for attention'], ['producing_output', 'producing output'], ['error', 'exited with an error'],
  ['starting', 'starting'], ['quiet', 'quiet'], ['exited', 'exited'],
];

const count = (value: number) => value.toLocaleString();

export function gitText(git: Git): string {
  if (git.state === 'known') return `${git.branch ?? 'detached'} · ${git.dirty + git.untracked ? `${git.dirty} changed, ${git.untracked} untracked` : 'clean'}`;
  return git.state === 'not_repository' ? 'no Git' : 'Git unknown';
}

export function usageText(usage: Usage): string {
  return `${count(usage.promptTokens)} in · ${count(usage.outputTokens)} out · ${usage.requests} req${usage.hitRatePermille === null ? '' : ` · cache ${(usage.hitRatePermille / 10).toFixed(0)} %`}`;
}

export function hopTime(ms: number, now: Date = new Date()): string {
  const at = new Date(ms);
  return now.toDateString() === at.toDateString()
    ? at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    : at.toLocaleString([], { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
}

/** "2 asking for attention · 1 producing output", most urgent first; '' when none runs. */
export function summaryLine(rows: readonly { state: string }[]): string {
  const parts: string[] = [];
  for (const [state, label] of SUMMARY) {
    const n = rows.filter(row => row.state === state).length;
    if (n) parts.push(`${n} ${label}`);
  }
  return parts.join(' · ');
}

/** What an instance is doing, from the titles read for it: the title, or why there is none. */
export function doingText(titled: Titles['rows'][number] | undefined): string | null {
  if (!titled) return null;
  return titled.title ?? (titled.reason ? `(${titled.reason})` : null);
}

export function collisionText(entry: Collision, known: readonly InstanceRow[]): string {
  const name = (id: string) => { const row = known.find(candidate => candidate.id === id); return row ? `${agentName(row.agent)} (${id.slice(-4)})` : id; };
  return entry.kind === 'file_collision'
    ? `Both ${entry.instances.map(name).join(' and ')} changed ${entry.path}`
    : `${entry.path} changed and no instance's own edits name it (possible collision)`;
}

/** Whether a row answers the Sessions view's filter. */
export function matchesFilter(row: InstanceRow, titled: Titles['rows'][number] | undefined, query: string): boolean {
  const wanted = query.trim().toLocaleLowerCase();
  if (!wanted) return true;
  return [agentName(row.agent), row.cwd, row.id, STATE_WORDS[row.state] ?? row.state, titled?.title ?? ''].some(value => value.toLocaleLowerCase().includes(wanted));
}
