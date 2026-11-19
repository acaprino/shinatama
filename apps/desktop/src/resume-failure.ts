/** What a refused or failed thread_resume says to the user. Every code the backend returns has
 * its own sentence, so a refusal never reads as a random failure to retry blindly. */
export function resumeFailureCode(error: unknown): string | null {
  if (typeof error === 'string') return error;
  if (typeof error !== 'object' || error === null || error instanceof Error) return null;
  const { code, issue } = error as { code?: unknown; issue?: unknown };
  return typeof code === 'string' ? code : typeof issue === 'string' ? issue : null;
}

/** The harness's own reason, when its start failed after every check passed (`message`). */
export function resumeFailureReason(error: unknown): string | null {
  if (typeof error !== 'object' || error === null || error instanceof Error) return null;
  const { message } = error as { message?: unknown };
  if (typeof message !== 'string') return null;
  const reason = message.replace(/\s+/g, ' ').trim().slice(0, 400);
  return reason || null;
}

export function resumeFailureText(error: unknown): string {
  switch (resumeFailureCode(error)) {
    case 'resources': return 'Not enough free memory to open the saved conversation right now; nothing was started. Close some programs or sessions, then resume again.';
    case 'reader_blocked': return 'Saved conversations cannot be read until Shinatama restarts; nothing was started. Restart Shinatama, then resume again.';
    case 'slow_read': return 'Reading the saved conversation took too long, probably because the computer is busy; nothing was started. Resume again in a moment, or open a new session.';
    case 'busy': return 'Another session is opening, or saved conversations are being read. Resume again in a moment.';
    case 'owned_elsewhere': return 'This conversation is already open.';
    case 'source_changed': case 'conflict': return 'This conversation changed. Reload threads and try again.';
    case 'unbound': case 'native_missing': return 'No saved conversation yet: nothing was said in this session.';
    case 'native_unreachable': return 'The saved conversation cannot be reached now; nothing was started. Check that its folder is available, then resume again.';
    case 'project_unavailable': return 'The project folder of this conversation cannot be opened. Check that it still exists.';
    case 'unsupported_harness': return 'This agent cannot reopen its sessions.';
    case 'invalid_request': return 'This thread record is not valid. Reload threads.';
    default: {
      const reason = resumeFailureReason(error);
      return reason ? `The conversation could not be resumed: ${reason} Its saved conversation is kept.`
        : 'The conversation could not be resumed; its saved conversation is kept. Try again.';
    }
  }
}
