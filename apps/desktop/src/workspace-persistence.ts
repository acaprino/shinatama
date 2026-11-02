// Layout-only persistence. The Rust bridge owns window/process identity and CAS;
// this controller never launches a resource, supplies an owner or falls back to
// a second store. Exactly one IPC and one latest local layout can be pending.
import { parseWorkspaceLayout, serializeWorkspaceLayout, WORKSPACE_LIMITS } from './workspace-model.ts';
import type { LayoutResult, WorkspaceLayout } from './workspace-model.ts';

export interface WorkspaceScope { readonly projectId: string; readonly workspaceId: string }
export type WorkspaceOwnership = 'unclaimed' | 'ours' | 'other';
export interface WorkspaceSnapshot extends WorkspaceScope {
  readonly revision: number; readonly schemaVersion: number; readonly layoutJson: string;
  readonly ownership: WorkspaceOwnership; readonly readOnly: boolean;
}
export interface WorkspaceWrite extends WorkspaceScope { readonly expectedRevision: number; readonly schemaVersion: 1; readonly layoutJson: string }
export interface WorkspaceRelease extends WorkspaceScope { readonly expectedRevision: number }
export interface WorkspaceTransport {
  load(scope: WorkspaceScope): Promise<unknown>;
  claim(request: WorkspaceWrite): Promise<unknown>;
  save(request: WorkspaceWrite): Promise<unknown>;
  release(request: WorkspaceRelease): Promise<unknown>;
}
export type WorkspaceInvoke = (command: string, args: { readonly request: unknown }) => Promise<unknown>;
export function createTauriWorkspaceTransport(invoke: WorkspaceInvoke): WorkspaceTransport {
  return {
    load: request => invoke('workspace_load', { request }),
    claim: request => invoke('workspace_save', { request }),
    save: request => invoke('workspace_save', { request }),
    release: request => invoke('workspace_release', { request }),
  };
}
export type WorkspaceFailureCode = 'busy' | 'conflict' | 'owned_elsewhere' | 'read_only' | 'invalid' | 'unavailable' | 'uncertain' | 'timeout' | 'not_loaded' | 'not_owned' | 'disposed';
export type WorkspacePhase = 'idle' | 'loading' | 'loaded' | 'claiming' | 'owned' | 'saving' | 'releasing' | 'released' | 'blocked' | 'disposed';
export interface WorkspacePersistenceStatus {
  readonly phase: WorkspacePhase; readonly revision: number | null; readonly ownership: WorkspaceOwnership;
  readonly dirty: boolean; readonly inFlight: boolean; readonly closing: boolean;
  readonly automaticSaves: boolean; readonly error: WorkspaceFailureCode | null; readonly message: string | null;
}
export type WorkspaceOutcome = { readonly ok: true; readonly revision: number; readonly dirty: boolean }
  | { readonly ok: false; readonly error: WorkspaceFailureCode };
export type WorkspaceLoadOutcome = { readonly ok: true; readonly layout: WorkspaceLayout; readonly restored: boolean; readonly exists: boolean; readonly revision: number; readonly ownership: WorkspaceOwnership }
  | { readonly ok: false; readonly error: WorkspaceFailureCode };
export interface WorkspacePersistenceOptions {
  readonly scope: WorkspaceScope; readonly initialLayout: WorkspaceLayout; readonly transport: WorkspaceTransport;
  readonly debounceMs?: number;
  /** A synchronous observer; do not start another persistence operation here. */
  readonly onStatus?: (status: WorkspacePersistenceStatus) => void;
}
export interface WorkspacePersistence {
  status(): WorkspacePersistenceStatus;
  layout(): WorkspaceLayout;
  load(): Promise<WorkspaceLoadOutcome>;
  claim(): Promise<WorkspaceOutcome>;
  update(layout: WorkspaceLayout): LayoutResult<void>;
  /** Concurrent callers share one bounded flush; no additional request queue. */
  flush(timeoutMs?: number): Promise<WorkspaceOutcome>;
  release(timeoutMs?: number): Promise<WorkspaceOutcome>;
  /** Stops scheduling only; disposal is never presented as an ownership release. */
  dispose(): void;
}

const MAX_WAIT_MS = 5000;
const MAX_FLUSH_WRITES = 4;
const messages: Record<WorkspaceFailureCode, string> = {
  busy: 'The layout is still changing or saving. Keep this window open and try again.',
  conflict: 'The saved workspace changed. Your current layout is kept; reload before saving again.',
  owned_elsewhere: 'Another window owns this workspace. Your current layout is kept.',
  read_only: 'The workspace is read-only. Your current layout is kept.',
  invalid: 'The saved layout could not be verified. Previous data and your current layout are kept.',
  unavailable: 'Workspace storage is unavailable. Your current layout is kept.',
  uncertain: 'Save confirmation was lost. Your current layout is kept; review before saving again.',
  timeout: 'Saving is taking longer than expected. Keep this window open; no extra save was queued.',
  not_loaded: 'Load the workspace before claiming it.', not_owned: 'Claim this workspace before saving it.',
  disposed: 'This workspace connection is closed.',
};
function failure(value: unknown, write: boolean): WorkspaceFailureCode {
  if (typeof value === 'object' && value !== null && 'code' in value) {
    switch (value.code) {
      case 'conflict': case 'owned_elsewhere': case 'read_only': case 'invalid': case 'unavailable': case 'uncertain': return value.code;
    }
  }
  return write ? 'uncertain' : 'unavailable';
}
function isRevision(value: unknown): value is number { return typeof value === 'number' && Number.isSafeInteger(value) && value >= 1; }
function readSnapshot(value: unknown, scope: WorkspaceScope): { readonly snapshot: WorkspaceSnapshot; readonly layout: WorkspaceLayout; readonly canonicalJson: string } | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some(key => !['projectId', 'workspaceId', 'revision', 'schemaVersion', 'layoutJson', 'ownership', 'readOnly'].includes(key))
    || record.projectId !== scope.projectId || record.workspaceId !== scope.workspaceId || !isRevision(record.revision)
    || record.schemaVersion !== 1 || typeof record.layoutJson !== 'string' || record.layoutJson.length > WORKSPACE_LIMITS.jsonBytes
    || new TextEncoder().encode(record.layoutJson).byteLength > WORKSPACE_LIMITS.jsonBytes || typeof record.readOnly !== 'boolean'
    || !['unclaimed', 'ours', 'other'].includes(String(record.ownership))) return null;
  let parsed: ReturnType<typeof parseWorkspaceLayout>;
  try { parsed = parseWorkspaceLayout(JSON.parse(record.layoutJson) as unknown); } catch { return null; }
  if (!parsed.ok) return null;
  const serialized = serializeWorkspaceLayout(parsed.value); if (!serialized.ok) return null;
  const ownership = record.ownership;
  if (ownership !== 'unclaimed' && ownership !== 'ours' && ownership !== 'other') return null;
  return { snapshot: { ...scope, revision: record.revision, schemaVersion: 1, layoutJson: record.layoutJson, ownership, readOnly: record.readOnly }, layout: parsed.value, canonicalJson: serialized.value };
}

export function createWorkspacePersistence(options: WorkspacePersistenceOptions): WorkspacePersistence {
  if (!/^p_[a-z2-7]{12}$/.test(options.scope.projectId) || !/^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$/.test(options.scope.workspaceId)) throw new Error('Invalid workspace scope.');
  const initial = parseWorkspaceLayout(options.initialLayout); const initialJson = initial.ok && serializeWorkspaceLayout(initial.value);
  if (!initial.ok || !initialJson || !initialJson.ok) throw new Error('Invalid initial workspace layout.');
  const debounceMs = options.debounceMs ?? 250;
  if (!Number.isInteger(debounceMs) || debounceMs < 0 || debounceMs > 2000) throw new Error('Invalid workspace debounce.');
  const scope = Object.freeze({ ...options.scope });
  let live = initial.value; let liveJson = initialJson.value; let savedJson: string | null = null;
  let phase: WorkspacePhase = 'idle'; let revision: number | null = null; let ownership: WorkspaceOwnership = 'unclaimed';
  let readOnly = false; let loaded = false; let localEdits = false; let editVersion = 0;
  let blocked = false; let needsReload = false; let automatic = false; let closing = false; let disposed = false;
  let error: WorkspaceFailureCode | null = null; let timer: ReturnType<typeof setTimeout> | null = null;
  let flight: { readonly kind: 'load' | 'claim' | 'save' | 'release'; readonly promise: Promise<WorkspaceOutcome> } | null = null;
  let flushing: Promise<WorkspaceOutcome> | null = null; let releasing: Promise<WorkspaceOutcome> | null = null;
  const dirty = (): boolean => localEdits || (loaded && liveJson !== savedJson);
  const status = (): WorkspacePersistenceStatus => Object.freeze({ phase, revision, ownership, dirty: dirty(), inFlight: flight !== null, closing,
    automaticSaves: automatic && !blocked && !closing && !disposed, error, message: error === null ? null : messages[error] });
  const publish = (): void => options.onStatus?.(status());
  const stopTimer = (): void => { if (timer !== null) clearTimeout(timer); timer = null; };
  const outcome = (): WorkspaceOutcome => ({ ok: true, revision: revision ?? 0, dirty: dirty() });
  const refused = (code: WorkspaceFailureCode): WorkspaceOutcome => ({ ok: false, error: code });
  function stop(code: WorkspaceFailureCode): WorkspaceOutcome {
    automatic = false; blocked = true; needsReload = true; stopTimer(); error = code; if (!disposed) phase = 'blocked'; return refused(code);
  }
  function canWrite(): WorkspaceFailureCode | null {
    if (disposed) return 'disposed'; if (!loaded || revision === null) return 'not_loaded';
    if (blocked || needsReload) return error ?? 'uncertain'; if (readOnly) return 'read_only'; if (ownership !== 'ours') return 'not_owned'; return null;
  }
  function schedule(): void {
    stopTimer(); if (!automatic || blocked || closing || disposed || liveJson === savedJson) return;
    timer = setTimeout(() => { timer = null; if (!flight && !blocked && !closing && !disposed) void save(); }, debounceMs);
  }
  function run(kind: 'load' | 'claim' | 'save' | 'release', work: () => Promise<WorkspaceOutcome>): Promise<WorkspaceOutcome> {
    if (flight) return Promise.resolve(refused('busy'));
    const operation = { kind, promise: Promise.resolve(outcome()) };
    flight = operation;
    operation.promise = Promise.resolve().then(work).catch(cause => stop(failure(cause, kind !== 'load'))).finally(() => {
      if (flight === operation) flight = null;
      if (!disposed) { publish(); schedule(); }
    });
    publish(); return operation.promise;
  }
  function write(kind: 'claim' | 'save'): Promise<WorkspaceOutcome> {
    if (flight) return Promise.resolve(refused('busy'));
    const unavailable = kind === 'save' ? canWrite() : disposed ? 'disposed' : !loaded ? 'not_loaded' : blocked || needsReload ? error ?? 'uncertain' : readOnly ? 'read_only' : ownership === 'other' ? 'owned_elsewhere' : closing ? 'busy' : null;
    if (unavailable) return Promise.resolve(refused(unavailable));
    if (revision === null || revision >= Number.MAX_SAFE_INTEGER) return Promise.resolve(stop('invalid'));
    stopTimer(); const expected = revision; const json = liveJson;
    const request: WorkspaceWrite = { ...scope, expectedRevision: expected, schemaVersion: 1, layoutJson: json };
    phase = kind === 'claim' ? 'claiming' : 'saving';
    return run(kind, async () => {
      try {
        const raw = await options.transport[kind](request);
        if (disposed) return refused('disposed');
        const parsed = readSnapshot(raw, scope);
        if (!parsed || parsed.snapshot.revision !== expected + 1 || parsed.snapshot.ownership !== 'ours' || parsed.snapshot.readOnly || parsed.snapshot.layoutJson !== json) return stop('uncertain');
        revision = parsed.snapshot.revision; ownership = 'ours'; savedJson = json; localEdits = liveJson !== json;
        // A late acknowledgement may settle metadata, but never re-arm writes
        // after a timeout, ownership loss, refusal or explicit shutdown request.
        if (blocked || needsReload) return refused(error ?? 'uncertain');
        phase = 'owned'; automatic = !closing; error = null; return outcome();
      } catch (cause) { return stop(failure(cause, true)); }
    });
  }
  function save(): Promise<WorkspaceOutcome> { return liveJson === savedJson ? Promise.resolve(outcome()) : write('save'); }

  async function waitFor(operation: Promise<WorkspaceOutcome>, deadline: number): Promise<WorkspaceOutcome> {
    const remaining = deadline - performance.now();
    if (remaining <= 0) return refused('timeout');
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try { return await Promise.race([operation, new Promise<WorkspaceOutcome>(resolve => { timeout = setTimeout(() => resolve(refused('timeout')), remaining); })]); }
    finally { if (timeout !== undefined) clearTimeout(timeout); }
  }
  function flush(timeoutMs = 2000): Promise<WorkspaceOutcome> {
    if (flushing) return flushing;
    if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > MAX_WAIT_MS) return Promise.resolve(refused('invalid'));
    stopTimer(); const deadline = performance.now() + timeoutMs;
    const task = (async (): Promise<WorkspaceOutcome> => {
      let writes = 0;
      while (true) {
        if (disposed) return refused('disposed');
        if (flight) {
          const result = await waitFor(flight.promise, deadline);
          if (!result.ok) {
            if (result.error === 'timeout') { const stopped = stop('timeout'); publish(); return stopped; }
            return result;
          }
        }
        const unavailable = canWrite(); if (unavailable) return refused(unavailable);
        if (liveJson === savedJson) return outcome();
        if (performance.now() >= deadline) { const stopped = stop('timeout'); publish(); return stopped; }
        if (writes++ >= MAX_FLUSH_WRITES) { error = 'busy'; publish(); return refused('busy'); }
        const result = await waitFor(save(), deadline);
        if (!result.ok) { if (result.error === 'timeout') { const stopped = stop('timeout'); publish(); return stopped; } return result; }
      }
    })();
    flushing = task.finally(() => { flushing = null; }); return flushing;
  }

  return {
    status, layout: () => live,
    async load(): Promise<WorkspaceLoadOutcome> {
      if (disposed) return { ok: false, error: 'disposed' }; if (flight || releasing || flushing) return { ok: false, error: 'busy' };
      automatic = false; closing = false; stopTimer(); phase = 'loading'; error = null;
      const version = editVersion; const preserveLocal = localEdits; let restored = false; let exists = false;
      const result = await run('load', async () => {
        try {
          const raw = await options.transport.load(scope); if (disposed) return refused('disposed');
          if (raw === null) { loaded = true; revision = 0; ownership = 'unclaimed'; readOnly = false; savedJson = null; }
          else {
            const parsed = readSnapshot(raw, scope); if (!parsed) return stop('invalid');
            exists = true; loaded = true; revision = parsed.snapshot.revision; ownership = parsed.snapshot.ownership; readOnly = parsed.snapshot.readOnly; savedJson = parsed.canonicalJson;
            if (!preserveLocal && version === editVersion) { live = parsed.layout; liveJson = parsed.canonicalJson; localEdits = false; restored = true; }
            else localEdits = liveJson !== savedJson;
          }
          blocked = false; needsReload = false; phase = 'loaded';
          if (readOnly || ownership === 'other') { blocked = true; error = readOnly ? 'read_only' : 'owned_elsewhere'; phase = 'blocked'; }
          return outcome();
        } catch (cause) { return stop(failure(cause, false)); }
      });
      return result.ok ? { ok: true, layout: live, restored, exists, revision: revision ?? 0, ownership } : result;
    },
    claim: () => write('claim'),
    update(layout): LayoutResult<void> {
      if (disposed) return { ok: false, error: 'invalid-action' };
      const parsed = parseWorkspaceLayout(layout); if (!parsed.ok) return parsed;
      const serialized = serializeWorkspaceLayout(parsed.value); if (!serialized.ok) return serialized;
      if (serialized.value !== liveJson) { live = parsed.value; liveJson = serialized.value; editVersion += 1; localEdits = savedJson === null || liveJson !== savedJson; publish(); schedule(); }
      return { ok: true, value: undefined };
    },
    flush,
    release(timeoutMs = 2000): Promise<WorkspaceOutcome> {
      if (releasing) return releasing;
      if (disposed) return Promise.resolve(refused('disposed'));
      if (phase === 'released') return Promise.resolve(outcome());
      if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > MAX_WAIT_MS) return Promise.resolve(refused('invalid'));
      closing = true; automatic = false; stopTimer(); publish();
      const deadline = performance.now() + timeoutMs;
      const task = (async (): Promise<WorkspaceOutcome> => {
        const flushed = await waitFor(flush(timeoutMs), deadline);
        if (!flushed.ok) {
          if (flushed.error === 'timeout') { const stopped = stop('timeout'); publish(); return stopped; }
          return flushed;
        }
        if (performance.now() >= deadline) { const stopped = stop('timeout'); publish(); return stopped; }
        const expected = revision; if (expected === null || expected === 0) return refused('not_loaded');
        phase = 'releasing';
        const operation = run('release', async () => {
          try {
            const response = await options.transport.release({ ...scope, expectedRevision: expected });
            if (disposed) return refused('disposed');
            if (response !== null && response !== undefined) return stop('uncertain');
            if (blocked || needsReload) { ownership = 'unclaimed'; return refused(error ?? 'uncertain'); }
            ownership = 'unclaimed'; phase = 'released'; automatic = false; error = dirty() ? 'busy' : null;
            return outcome();
          } catch (cause) { return stop(failure(cause, true)); }
        });
        const result = await waitFor(operation, deadline);
        if (!result.ok && result.error === 'timeout') { const stopped = stop('timeout'); publish(); return stopped; }
        return result;
      })();
      releasing = task.finally(() => { releasing = null; }); return releasing;
    },
    dispose(): void { if (disposed) return; disposed = true; automatic = false; closing = true; stopTimer(); phase = 'disposed'; publish(); },
  };
}
