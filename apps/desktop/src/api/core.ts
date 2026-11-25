// The way from the page to Rust: one `invoke`, the events, and the log. Every command of src-tauri/src/main.rs is called through a
// typed function of one of the files of this folder (`api/<area>.ts`), never with a literal name from a screen or a controller.
//
// Outside Tauri (the browser at `npm run dev`, the browser tests) the same calls answer from `src/mock`, so every screen can be
// seen and tested with no window; a production build leaves the mock out and says the app runs inside its window.
import { invoke as tauriInvoke, isTauri } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';

export const inTauri: boolean = isTauri();

/** What a command refuses with: a string, or `{ code, message }` (the store's refusals), or an `Error`. */
export type CmdError = { code?: string; message?: string; issue?: string };

type Mock = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
type MockEvents = (event: string, handler: (payload: unknown) => void) => () => void;
let mock: Promise<{ answer: Mock; listen: MockEvents }> | null = null;

/** The mock of the browser, loaded when it is first needed and only in development. */
function mocked() {
  mock ??= import('../mock/index.ts');
  return mock;
}

let override: Mock | null = null;

/** A test answers the commands itself, with no window and no mock (`null` takes the answers away). */
export function overrideInvoke(answer: Mock | null): void {
  override = answer;
}

/** One call of one command. Outside the window it is the mock's answer, or a refusal that says the app needs its window. */
export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (override) return (await override(command, args)) as T;
  if (inTauri) return tauriInvoke<T>(command, args);
  if (!import.meta.env?.DEV) throw 'Open the desktop app: Shinatama runs inside its window.';
  return (await (await mocked()).answer(command, args)) as T;
}

/** What to listen to stops with the function this gives. Outside the window it is the mock's events. */
export type Unlisten = () => void;
export async function listen<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (inTauri) return tauriListen<T>(event, ({ payload }) => handler(payload));
  if (!import.meta.env?.DEV) return () => {};
  return (await mocked()).listen(event, payload => handler(payload as T));
}

/** The words of whatever a command threw (a string, `{ message }` or an `Error`). */
export function errorText(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === 'object') {
    const { message, code, issue } = error as CmdError;
    if (typeof message === 'string') return message;
    if (typeof code === 'string') return code;
    if (typeof issue === 'string') return issue;
  }
  return JSON.stringify(error ?? '');
}

/** The code of a refusal (`conflict`, `owned_elsewhere`...), when it has one. */
export function errorCode(error: unknown): string | null {
  if (error && typeof error === 'object' && 'code' in error) {
    const { code } = error as CmdError;
    return typeof code === 'string' ? code : null;
  }
  return null;
}

/** Writes one line in shinatama.log (the data folder); a log that cannot be written changes nothing. */
export function appLog(source: 'layout' | 'page', message: string): void {
  if (inTauri) void invoke('app_log', { source, message }).catch(() => {});
}

/** A wait that a test can shorten: the controllers wait through this, never with a timer of their own. */
export const sleep = (ms: number): Promise<void> => new Promise(resolve => setTimeout(resolve, ms));

/** Closes the window of the app (the close of the page's own confirmation). Nothing outside the window. */
export async function closeWindow(): Promise<void> {
  if (inTauri) await (await import('@tauri-apps/api/window')).getCurrentWindow().close();
}
