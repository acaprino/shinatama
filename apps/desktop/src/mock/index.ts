// What the browser answers instead of Rust: development only (`npm run dev`, the browser tests). Each area keeps its own fixtures
// in a file of this folder (`mock/<area>.ts`, an object of `command -> answer`), and this file puts them together. A command with
// no answer refuses, so a screen that asks for one that is not mocked says so instead of staying empty.
import { answers as layout } from './workspace.ts';
import { answers as threads } from './threads.ts';
import { answers as sessions } from './sessions.ts';
import { answers as files } from './files.ts';
import { answers as inspector } from './inspector.ts';
import { answers as settings } from './settings.ts';
import { answers as projects } from './projects.ts';
import { answers as terminals } from './terminals.ts';
import { answers as team } from './team.ts';
import { answers as dialogs } from './dialogs.ts';
import { answers as attention } from './attention.ts';
import { answers as browser } from './browser.ts';

export type Answer = ((args: Record<string, unknown> | undefined) => unknown | Promise<unknown>) | object | string | number | boolean | null;
export type Answers = Record<string, Answer>;

const all: Answers = { ...layout, ...threads, ...files, ...inspector, ...settings, ...sessions, ...projects, ...terminals, ...team, ...dialogs, ...attention, ...browser };

export async function answer(command: string, args?: Record<string, unknown>): Promise<unknown> {
  if (!(command in all)) throw `The browser preview has no answer for ${command}.`;
  const one = all[command];
  return typeof one === 'function' ? (one as (args: Record<string, unknown> | undefined) => unknown)(args) : one;
}

const handlers = new Map<string, Set<(payload: unknown) => void>>();

/** Events of the mock: a test or a fixture says `emit('thread-changed', {...})`. */
export function emit(event: string, payload?: unknown): void {
  for (const handler of [...(handlers.get(event) ?? [])]) handler(payload);
}

export function listen(event: string, handler: (payload: unknown) => void): () => void {
  const set = handlers.get(event) ?? new Set();
  set.add(handler);
  handlers.set(event, set);
  return () => { set.delete(handler); };
}
