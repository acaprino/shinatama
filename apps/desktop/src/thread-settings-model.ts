// The words the settings of a thread say when something it was asked to do is refused. Pure.
import { record } from './session-library-model.ts';

/** What a refused operation on a thread or one of its sessions says. `doing` is the verb of what was asked ("renamed", "deleted"). */
export function operationFailure(error: unknown, doing: string, what: 'thread' | 'session' = 'thread'): string {
  const code = typeof error === 'string' ? error : record(error) ? error.code ?? error.issue : null;
  if (code === 'project_missing') return 'The project is unavailable. Choose it again.';
  if (code === 'busy') return 'Thread storage is busy. Try again.';
  if (code === 'conflict') return `This ${what} changed. Reload and try again.`;
  if (code === 'owned_elsewhere') return 'A session of this thread is running. Stop it first.';
  // Nothing runs, but a session is still marked open by a Shinatama that closed badly, and its end could not be proven.
  if (code === 'open_unverified') return 'A session of this thread is still marked open by a Shinatama that is no longer running, and that could not be verified. The thread was kept.';
  return `The ${what} could not be ${doing}. Nothing was changed.`;
}

/** A name a person typed, or null when it is empty or cannot be kept. */
export function cleanName(value: string): string | null {
  const name = value.trim();
  return name && name.length <= 256 && !/[\u0000-\u001f\u007f]/.test(name) ? name : null;
}
