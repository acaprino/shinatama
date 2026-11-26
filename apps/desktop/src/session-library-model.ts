// What the Sessions view and the inspector's session panels agree on, as pure functions: the shape of what the session worker answers
// (checked before it is trusted: a reply that is not exactly the shape asked for is no reply), the words of its refusals, and the
// words a row says. No React, no DOM, no Tauri.

export type SessionRootSelector = { storeRootId: string; configurationRevision: number };
export type SessionCursor = { offset: number; fingerprint: string };
export type SessionLibraryRoot = SessionRootSelector & {
  harness: string; kind: 'box' | 'system'; access: 'read_only' | 'read_write'; available: boolean; label?: string;
};
export type SessionMetadataRow = {
  nativeId: string; title: string | null; workingDirectory: string | null; updatedAt: string | null;
  createdAt: string | null; turns: number | null; bytes: number | null; parentNativeId: string | null;
  archived: boolean | null; metadataTruncated: boolean;
};
export type SessionListPage = {
  root: SessionRootSelector; harness: string; fingerprint: string; total: number;
  rows: SessionMetadataRow[]; next: SessionCursor | null;
};
export type SessionListRequest = { root: SessionRootSelector; cursor: SessionCursor | null; limit: 50; includeChildren: true };
export type SessionArchiveAddRequest = { harness: 'claude_code' | 'codex' | 'pi'; path: string };

export type SearchHit = {
  storeRootId: string; configurationRevision: number; nativeId: string; harness: string;
  title: string | null; workingDirectory: string | null; updatedAt: string | null;
  snippet: string; sourceDeleted: boolean;
};
export type SearchRequest = { query: string; harness?: string; project?: string; from?: string; to?: string };

// --- the guards every reply goes through -------------------------------------------------------------------------------------

export const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
export const text = (value: unknown, cap: number): value is string => typeof value === 'string' && value.length <= cap && !/[\u0000-\u001f\u007f]/.test(value);
export const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
export const digest = (value: unknown): value is string => typeof value === 'string' && /^[0-9a-f]{64}$/i.test(value);
export const nativeIdValue = (value: unknown): value is string => typeof value === 'string' && /^[a-z0-9_-]{1,256}$/i.test(value);
const nullableText = (value: unknown, cap: number) => value === null || text(value, cap);
const nullableInteger = (value: unknown) => value === null || integer(value);

export function rootValue(value: unknown): value is SessionRootSelector & Record<string, unknown> {
  return record(value) && text(value.storeRootId, 256) && value.storeRootId.length > 0
    && integer(value.configurationRevision) && value.configurationRevision > 0;
}
export const sameRoot = (a: SessionRootSelector, b: SessionRootSelector) => a.storeRootId === b.storeRootId && a.configurationRevision === b.configurationRevision;
/** One string for a row of one archive: what a table or a tree names it by. */
export const sessionKey = (root: SessionRootSelector, nativeId: string) => `${root.storeRootId}\u0000${root.configurationRevision}\u0000${nativeId}`;
export const selectorOf = (root: SessionRootSelector): SessionRootSelector => ({ storeRootId: root.storeRootId, configurationRevision: root.configurationRevision });

// --- the library -------------------------------------------------------------------------------------------------------------

export function rootsValue(value: unknown): SessionLibraryRoot[] | null {
  if (!Array.isArray(value) || value.length > 100) return null;
  const result: SessionLibraryRoot[] = [];
  for (const item of value) {
    if (!rootValue(item) || !text(item.harness, 64) || !/^[a-z0-9_]+$/.test(item.harness)
      || typeof item.kind !== 'string' || !['box', 'system'].includes(item.kind)
      || typeof item.access !== 'string' || !['read_only', 'read_write'].includes(item.access)
      || typeof item.available !== 'boolean' || (item.label !== undefined && !text(item.label, 128))
      || result.some(root => sameRoot(root, item))) return null;
    result.push({
      storeRootId: item.storeRootId, configurationRevision: item.configurationRevision, harness: item.harness,
      kind: item.kind as 'box' | 'system', access: item.access as 'read_only' | 'read_write', available: item.available,
      ...(item.label !== undefined ? { label: item.label as string } : {}),
    });
  }
  return result;
}

export function pageValue(value: unknown, root: SessionLibraryRoot, requested: SessionCursor | null): SessionListPage | null {
  if (!record(value) || !rootValue(value.root) || !sameRoot(value.root, root) || value.harness !== root.harness
    || !digest(value.fingerprint) || !integer(value.total) || !Array.isArray(value.rows) || value.rows.length > 50
    || (requested !== null && requested.fingerprint !== value.fingerprint)) return null;
  const rows: SessionMetadataRow[] = [];
  for (const item of value.rows) {
    if (!record(item) || !text(item.nativeId, 256) || !item.nativeId || rows.some(row => row.nativeId === item.nativeId)
      || !nullableText(item.title, 512) || !nullableText(item.workingDirectory, 1024) || !nullableText(item.updatedAt, 64)
      || !nullableText(item.createdAt, 64) || !nullableText(item.parentNativeId, 256) || !nullableInteger(item.turns)
      || !nullableInteger(item.bytes) || (item.archived !== null && typeof item.archived !== 'boolean') || typeof item.metadataTruncated !== 'boolean') return null;
    rows.push({
      nativeId: item.nativeId, title: item.title as string | null, workingDirectory: item.workingDirectory as string | null,
      updatedAt: item.updatedAt as string | null, createdAt: item.createdAt as string | null, parentNativeId: item.parentNativeId as string | null,
      turns: item.turns as number | null, bytes: item.bytes as number | null, archived: item.archived as boolean | null, metadataTruncated: item.metadataTruncated,
    });
  }
  const offset = requested?.offset ?? 0;
  if (offset + rows.length > value.total || (rows.length === 0 && value.total > offset)) return null;
  let next: SessionCursor | null = null;
  if (value.next !== null) {
    if (!record(value.next) || !integer(value.next.offset) || value.next.offset !== offset + rows.length
      || value.next.offset <= offset || value.next.offset >= value.total || value.next.fingerprint !== value.fingerprint) return null;
    next = { offset: value.next.offset, fingerprint: value.fingerprint };
  }
  if (offset + rows.length < value.total && !next) return null;
  return { root: selectorOf(root), harness: root.harness, fingerprint: value.fingerprint, total: value.total, rows, next };
}

/** The hits of a text search, or null when the reply is not a list of what was asked for. */
export function hitsValue(value: unknown): SearchHit[] | null {
  if (!Array.isArray(value) || value.length > 200) return null;
  const hits: SearchHit[] = [];
  for (const item of value) {
    if (!record(item) || !text(item.storeRootId, 256) || !integer(item.configurationRevision) || !text(item.nativeId, 256)
      || !text(item.harness, 64) || !nullableText(item.title, 512) || !nullableText(item.workingDirectory, 1024)
      || !nullableText(item.updatedAt, 64) || typeof item.snippet !== 'string' || item.snippet.length > 4096 || typeof item.sourceDeleted !== 'boolean') return null;
    hits.push({
      storeRootId: item.storeRootId, configurationRevision: item.configurationRevision, nativeId: item.nativeId, harness: item.harness,
      title: item.title as string | null, workingDirectory: item.workingDirectory as string | null, updatedAt: item.updatedAt as string | null,
      snippet: item.snippet, sourceDeleted: item.sourceDeleted,
    });
  }
  return hits;
}

const FAILURES: Record<string, string> = {
  root_unavailable: 'The selected archive is unavailable. No other archive was used.', root_changed: 'The archive configuration changed. Reload archives and select it explicitly.',
  source_changed: 'Session metadata changed while paging. Reload sessions to start again.', unsupported_harness: 'This agent has no reader in the reviewed Mukade version.',
  unsupported_root_shape: 'This archive shape is not supported by the reviewed reader.', unsafe_root: 'The archive failed read-only path verification.',
  metadata_too_large: 'Session metadata exceeds the fixed limits.', timed_out: 'The metadata reader reached its time limit.',
  output_limit: 'The metadata reader reached its output limit.', cleanup_unconfirmed: 'Reader cleanup is unconfirmed. Its resources remain reserved. Reload archives or sessions to retry only that cleanup.',
  invalid_request: 'Choose a supported agent and an existing absolute archive folder.',
  read_unavailable: 'Archive storage is busy or unavailable. Retry explicitly.', worker_unavailable: 'The contained metadata reader is unavailable. Retry explicitly when resources are ready.',
};
export const INVALID_ARCHIVE = FAILURES.invalid_request;
/** The code a refusal carries: a string, or `{ issue }` or `{ code }`. */
export function refusalCode(value: unknown): string | null {
  const code = typeof value === 'string' ? value : value instanceof Error ? value.message : record(value) ? value.issue ?? value.code : null;
  return typeof code === 'string' ? code : null;
}
export function failureText(value: unknown): string {
  const code = refusalCode(value);
  return code !== null && Object.hasOwn(FAILURES, code) ? FAILURES[code] : 'Session information could not be loaded. Retry explicitly.';
}
const SEARCH_REASONS: Record<string, string> = {
  invalid_request: 'The search index is off or cannot take this query. Turn indexing on in Settings, then index the sessions.',
};
export function searchFailureText(value: unknown): string {
  const code = refusalCode(value);
  return (code !== null && SEARCH_REASONS[code]) || 'The search index is busy or unavailable. Try again.';
}

// --- the words a row says ----------------------------------------------------------------------------------------------------

const AGENTS: Record<string, string> = { claude_code: 'Claude Code', claude: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi', hermes: 'Hermes Agent', deepseek: 'DeepSeek Harness', PowerShell: 'PowerShell' };
export const agentName = (agent: string): string => AGENTS[agent] ?? agent;
/** The agents a destination, an archive or a filter may name, in the order they are offered. */
export const AGENT_CHOICES: readonly [string, string][] = [
  ['claude_code', 'Claude Code'], ['codex', 'Codex'], ['opencode', 'OpenCode'], ['pi', 'Pi'], ['hermes', 'Hermes Agent'], ['deepseek', 'DeepSeek Harness'],
];
/** The agents whose archives can be added by hand. */
export const ARCHIVE_AGENTS: readonly [string, string][] = [['claude_code', 'Claude Code'], ['codex', 'Codex'], ['pi', 'Pi']];

/** "14 Nov 10:22", or "10:22" today; a value that is not a date is shown as recorded. */
export function when(value: string | null | undefined, now: Date = new Date()): string {
  if (!value) return 'Unknown';
  const at = new Date(value);
  if (Number.isNaN(at.getTime())) return value;
  const time = at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (at.toDateString() === now.toDateString()) return time;
  const day = at.toLocaleDateString([], { day: '2-digit', month: 'short', year: at.getFullYear() === now.getFullYear() ? undefined : 'numeric' });
  return `${day} ${time}`;
}

/** "1.2 MB"; Unknown for a session whose size the reader did not say. */
export function sizeText(bytes: number | null): string {
  return bytes === null ? 'Unknown' : bytes < 1024 ? `${bytes} B` : bytes < 1048576 ? `${(bytes / 1024).toFixed(0)} KB` : `${(bytes / 1048576).toFixed(1)} MB`;
}

/** The name of an archive in a menu or a list: its label, or its agent, and whose it is. */
export function archiveName(root: SessionLibraryRoot): string {
  return `${root.label ?? agentName(root.harness)} · ${root.kind === 'box' ? 'Shinatama' : 'system'}${root.available ? '' : ' · unavailable'}`;
}

// --- a snippet of a search hit -----------------------------------------------------------------------------------------------

export type SnippetPart = { text: string; match: boolean };
const MARK_OPEN = '«';
const MARK_CLOSE = '»';

/** A snippet's «match» markers as parts, so that the screen draws the match marked and never parses the text as markup. */
export function snippetParts(snippet: string): SnippetPart[] {
  const parts: SnippetPart[] = [];
  let rest = snippet;
  while (rest.length) {
    const start = rest.indexOf(MARK_OPEN);
    const end = start < 0 ? -1 : rest.indexOf(MARK_CLOSE, start + 1);
    if (start < 0 || end < 0) { parts.push({ text: rest, match: false }); break; }
    if (start > 0) parts.push({ text: rest.slice(0, start), match: false });
    parts.push({ text: rest.slice(start + 1, end), match: true });
    rest = rest.slice(end + 1);
  }
  return parts;
}
