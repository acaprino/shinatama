// The new session dialog's decisions, pure: which agents are offered and in what order, which one is chosen first, and whether what was
// typed can start a session. Opening it (the queue, the thread it joins) is `OpensController`'s.
import type { HarnessCapabilities } from './api/terminals.ts';

const control = /[\u0000-\u001f\u007f-\u009f]/;

/** A folder that exists as a place on Windows: a drive path or a network share, no control characters, at most 4096 characters. */
export function absoluteFolder(value: string): boolean {
  return value.length > 0 && value.length <= 4096 && !control.test(value)
    && (/^[a-z]:[\\/]/i.test(value) || /^\\\\[^\\/]+[\\/][^\\/]+(?:[\\/]|$)/.test(value));
}

/** The order the agents are offered in: Claude Code, Codex, OpenCode, then the others, the shell last. */
export function agentOrder(id: string): number {
  const rank = ['claude_code', 'codex', 'opencode'].indexOf(id);
  return rank < 0 ? (id === 'powershell' ? 99 : 50) : rank;
}

export function sortedHarnesses(harnesses: readonly HarnessCapabilities[]): HarnessCapabilities[] {
  return [...harnesses].sort((a, b) => agentOrder(a.id) - agentOrder(b.id));
}

let last = '';
/** The agent the last session was started with, in this window. */
export const lastHarness = (): string => last;
export const rememberHarness = (id: string): void => { last = id; };

/** The agent chosen when the dialog opens: the one asked for, else the last used, else Claude Code, else the first. */
export function firstHarness(harnesses: readonly HarnessCapabilities[], asked?: string, used: string = last): string {
  const has = (id: string | undefined) => !!id && harnesses.some(item => item.id === id);
  if (has(asked)) return asked!;
  if (has(used)) return used;
  if (!used && has('claude_code')) return 'claude_code';
  return sortedHarnesses(harnesses)[0]?.id ?? '';
}

/** What is wrong with the choice, in one sentence; empty when a session can start. */
export function startProblem(path: string, harness: string, harnesses: readonly HarnessCapabilities[]): string {
  if (!absoluteFolder(path.trim())) return 'Choose a project from the list, or browse for a folder.';
  if (!harnesses.some(item => item.id === harness)) return 'Choose an available agent.';
  return '';
}
