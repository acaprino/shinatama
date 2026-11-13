/** What the worker has done so far. `fileBytes` is context (the session file's size), never a percentage. */
export type ProgressEvent = { requestId: string; kind: 'export' | 'search'; entries: number; bytes: number; hits: number; fileBytes: number };

/** Short operations must not flash a line: it shows this long after the worker's first report. */
export const SHOW_AFTER_MS = 500;
const MIB = 1024 * 1024;

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < MIB) return `${Math.round(bytes / 1024)} KB`;
  const mib = bytes / MIB;
  if (mib < 1024) return `${mib < 10 ? mib.toFixed(1) : Math.round(mib)} MB`;
  return `${(mib / 1024).toFixed(1)} GB`;
}
export function formatElapsed(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  return seconds < 60 ? `${seconds} s` : `${Math.floor(seconds / 60)} min ${String(seconds % 60).padStart(2, '0')} s`;
}
/** "Exporting… 12,400 entries · 38 MB written · 14 s · session file 230 MB" (the file size is context only). */
export function describeProgress(kind: 'export' | 'search', event: ProgressEvent | null, elapsedMs: number): string {
  const parts: string[] = [];
  if (event) {
    parts.push(`${event.entries.toLocaleString('en-US')} ${event.entries === 1 ? 'entry' : 'entries'}`);
    if (kind === 'export') parts.push(`${formatBytes(event.bytes)} written`);
    else parts.push(`${event.hits.toLocaleString('en-US')} ${event.hits === 1 ? 'match' : 'matches'}`);
  }
  parts.push(formatElapsed(elapsedMs));
  if (event && event.fileBytes > 0) parts.push(`session file ${formatBytes(event.fileBytes)}`);
  return `${kind === 'export' ? 'Exporting' : 'Searching'}… ${parts.join(' · ')}`;
}

/**
 * Whether `event` belongs to the request now running. An event of another request (one that was
 * closed or replaced, or of the other kind) must never update this one.
 */
export function acceptsEvent(currentId: string | null, kind: 'export' | 'search', event: ProgressEvent): boolean {
  return currentId !== null && event.requestId === currentId && event.kind === kind;
}
