// The index of session text that the search reads: what its state says and which of its buttons can be pressed. Pure.
export type IndexProgress = {
  phase: 'idle' | 'running' | 'done' | 'stopped' | 'failed'; rootsTotal: number; rootsDone: number;
  indexed: number; skipped: number; failed: number; unsupportedArchives: number; markedSourceDeleted: number; message: string | null;
};
export type IndexStatus = { enabled: boolean; documents: number; sourceDeleted: number; bodyBytes: number };
export type IndexState = { progress: IndexProgress; index: IndexStatus };

const count = (value: unknown): number => typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : 0;
const PHASES = ['idle', 'running', 'done', 'stopped', 'failed'];

/** The state the backend answers, read as far as it can be trusted: a field it did not send is zero, a phase it did not name is idle
 *  (before the first run the backend reports none). Null when the reply is not an object with an index in it. */
export function indexValue(value: unknown): IndexState | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const { progress, index } = value as { progress?: Record<string, unknown> | null; index?: Record<string, unknown> | null };
  if (!index || typeof index !== 'object') return null;
  const run = progress && typeof progress === 'object' ? progress : {};
  return {
    progress: {
      phase: (PHASES.includes(run.phase as string) ? run.phase : 'idle') as IndexProgress['phase'],
      rootsTotal: count(run.rootsTotal), rootsDone: count(run.rootsDone), indexed: count(run.indexed), skipped: count(run.skipped), failed: count(run.failed),
      unsupportedArchives: count(run.unsupportedArchives), markedSourceDeleted: count(run.markedSourceDeleted), message: typeof run.message === 'string' ? run.message : null,
    },
    index: { enabled: index.enabled === true, documents: count(index.documents), sourceDeleted: count(index.sourceDeleted), bodyBytes: count(index.bodyBytes) },
  };
}

const VERBS: Record<string, string> = { running: 'Indexing', done: 'Finished', stopped: 'Stopped', failed: 'Failed' };
const mib = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MiB`;

export const isRunning = (state: IndexState | null): boolean => state?.progress.phase === 'running';

/** The sentences under the index: how much is indexed, and how the last run went. */
export function indexLines(state: IndexState | null): string[] {
  if (!state) return ['Index not loaded yet.'];
  const { index, progress } = state;
  const lines = [`${index.documents.toLocaleString()} sessions indexed · ${index.sourceDeleted.toLocaleString()} with deleted source · ${mib(index.bodyBytes)}`];
  const verb = VERBS[progress.phase];
  if (verb) {
    lines.push(`${verb}: ${progress.rootsDone}/${progress.rootsTotal} archives · ${progress.indexed} read · ${progress.skipped} unchanged`
      + (progress.failed ? ` · ${progress.failed} unreadable` : '') + (progress.unsupportedArchives ? ` · ${progress.unsupportedArchives} archives skipped` : ''));
    if (progress.message) lines.push(progress.message);
  }
  if (!index.enabled) lines.unshift('Indexing is off');
  return lines;
}

/** The one line the sidebar says: how many sessions are indexed. */
export function indexSummary(state: IndexState | null): string {
  if (!state) return 'Search index not read yet.';
  return state.index.enabled || state.index.documents ? `${state.index.documents.toLocaleString()} sessions indexed${state.index.enabled ? '' : ' · indexing is off'}` : 'Indexing is off.';
}

export function indexButtons(state: IndexState | null, busy: boolean): { start: boolean; stop: boolean; forget: boolean } {
  const running = isRunning(state);
  return { start: !busy && !running && !!state?.index.enabled, stop: running, forget: !busy && !running && !!state && state.index.documents > 0 };
}

const REASONS: Record<string, string> = {
  invalid_request: 'Check the query and filters, or turn indexing on.',
  read_unavailable: 'The index is busy or unavailable. Try again.',
  cleanup_unconfirmed: 'A reader cleanup is unconfirmed. Restart before indexing again.',
};
export function indexReason(error: unknown): string {
  const code = typeof error === 'string' ? error : (error as { issue?: string } | null)?.issue;
  return (code && REASONS[code]) || 'The operation could not be completed.';
}
