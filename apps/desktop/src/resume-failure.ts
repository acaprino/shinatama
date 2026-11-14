/** What a refused or failed thread_resume says to the user. Every code the backend returns has
 * its own sentence, so a refusal never reads as a random failure to retry blindly. */
export function resumeFailureCode(error: unknown): string | null {
  if (typeof error === 'string') return error;
  if (typeof error !== 'object' || error === null || error instanceof Error) return null;
  const { code, issue } = error as { code?: unknown; issue?: unknown };
  return typeof code === 'string' ? code : typeof issue === 'string' ? issue : null;
}

export function resumeFailureText(error: unknown): string {
  switch (resumeFailureCode(error)) {
    case 'resources': return 'Not enough free memory to open the saved conversation right now; nothing was started. Close some programs or sessions, then resume again.';
    case 'reader_blocked': return 'Saved conversations cannot be read until Shinatama restarts; nothing was started. Restart Shinatama, then resume again.';
    case 'busy': return 'Another session is opening, or saved conversations are being read. Resume again in a moment.';
    case 'owned_elsewhere': return 'This conversation is already open.';
    case 'source_changed': case 'conflict': return 'This conversation changed. Reload threads and try again.';
    case 'unbound': case 'native_missing': return 'No saved conversation yet: nothing was said in this session.';
    case 'project_unavailable': return 'The project folder of this conversation cannot be opened. Check that it still exists.';
    case 'unsupported_harness': return 'This agent cannot reopen its sessions.';
    case 'invalid_request': return 'This thread record is not valid. Reload threads.';
    default: return 'The conversation could not be resumed; its saved conversation is kept. Try again.';
  }
}
