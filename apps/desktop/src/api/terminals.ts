// The terminals Rust holds (the pseudo-terminals live in the process, not in the page) and how a session is started in one.
import { invoke } from './core.ts';

export type SessionInfo = { id: string; cwd: string; shell: string };
export type OutputBatch = { chunks: number[][]; eof: boolean; exit_code: number | null; error: string | null };
/** A session that was opened: its terminal, and the thread record it is saved in. */
export type ThreadStarted = { session: SessionInfo; threadId: string };
export type HarnessCapabilities = { id: string; label: string; nativeResume: boolean };

export const terminalsApi = {
  start: (cwd: string, rows: number, cols: number) => invoke<SessionInfo>('terminal_start', { cwd, rows, cols }),
  list: () => invoke<SessionInfo[]>('terminal_list'),
  /** What the terminal printed since the last read; waits a little when there is nothing. */
  read: (id: string) => invoke<OutputBatch>('terminal_read', { id }),
  write: (id: string, data: string) => invoke<void>('terminal_write', { id, data: Array.from(new TextEncoder().encode(data)) }),
  resize: (id: string, rows: number, cols: number) => invoke<void>('terminal_resize', { id, rows, cols }),
  stop: (id: string) => invoke<void>('terminal_stop', { id }),
  /** The window closes: every terminal ends, and the layout keeps them as running (the next start restores them). */
  shutdown: () => invoke<void>('terminal_shutdown'),
  /** The measures of every terminal (memory, processes), as `terminal-resources.ts` reads them. */
  resources: () => invoke<unknown>('terminal_resources'),
  /** The image on the clipboard, saved to a file: its path. */
  pasteImage: () => invoke<string>('terminal_paste_image'),
  /** The files on the clipboard, as paths. */
  clipboardFiles: () => invoke<string[]>('terminal_clipboard_files'),
  /** A shell in a folder, saved as a session of its thread. */
  shellOpen: (cwd: string, rows: number, cols: number) => invoke<ThreadStarted>('shell_open', { cwd, rows, cols }),
  /** An agent in a folder, in a thread (`worksetId`) when it has one. */
  agentOpen: (agentId: string, cwd: string, rows: number, cols: number, worksetId: string | null) =>
    invoke<ThreadStarted>('agent_open', { agentId, cwd, rows, cols, worksetId }),
  /** Resumes a saved session through its agent's native resume. */
  threadResume: (request: { threadId: string; projectId: string; expectedRevision: number; rows: number; cols: number }) =>
    invoke<ThreadStarted>('thread_resume', { request }),
  harnessCatalog: () => invoke<unknown>('harness_catalog'),
};
