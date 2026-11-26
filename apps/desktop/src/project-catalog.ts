import { invoke } from './api/core.ts';

export type Project = { id: string; path: string; name: string; last_opened: number; available: boolean };
export type Candidate = Omit<Project, 'id' | 'last_opened'> & { id: string | null; last_opened: number | null; modifiedMs: number | null; rootId: string };
export type RootConfig = { version: 1; revision: number; roots: { id: string; path: string }[]; defaultRootId: string | null; nonStandardHistory: string[] };
export type CatalogRoot = { id: string; path: string; state: 'ready' | 'scanning' | 'missing' | 'unavailable' | 'timed_out' | 'truncated'; message: string | null; projectCount: number; projects: Candidate[] };
export type Catalog = { scanId: string; revision: number; rootsConfig: RootConfig; roots: CatalogRoot[]; projects: Candidate[]; history: Project[]; recent: Project[]; scanning: boolean; configLoading: boolean; total: number; offset: number; limit: number; error: string | null; truncated: boolean };
export type RootDraft = { id?: string; path: string };
export type BrowseResult = { kind: 'cancelled' } | { kind: 'project'; project: Project } | { kind: 'root'; path: string };

export function createRequestId() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 15) | 64; bytes[8] = (bytes[8] & 63) | 128;
  const hex = [...bytes].map(byte => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function orderedCandidates(catalog: Catalog) {
  return catalog.projects.slice(0, 100);
}

export const saveRoots = (expectedRevision: number, roots: RootDraft[], defaultRootIndex: number | null, requestId: string) =>
  invoke<RootConfig>('catalog_roots_set', { request: { expectedRevision, roots, defaultRootIndex }, requestId });
export const browseFolder = (purpose: 'project' | 'root', initialRootId: string | undefined, requestId: string) =>
  invoke<BrowseResult>('catalog_browse', { request: { purpose, initialRootId }, requestId });
export const cancelMutation = (requestId: string) =>
  invoke<{ status: 'cancelled' | 'already_completed' }>('catalog_cancel', { requestId });
export const beginMutation = (requestId: string) => invoke<void>('catalog_begin', { requestId });

type Session = { scanId: string | null; filter: string; offset: number; queryRevision: number; pending: boolean; again: boolean; timer: number };

/** One in-flight IPC per picker. Filter changes coalesce; closed generations cannot render. */
export function createCatalogClient(onSnapshot: (value: Catalog) => void, onError: (error: unknown) => void) {
  let current: Session | null = null;
  let inFlight = false;
  function cancel(session: Session) {
    clearTimeout(session.timer);
    if (session.scanId) void invoke('catalog_read', { request: { action: 'cancel', scanId: session.scanId } }).catch(() => {});
  }
  async function fetch(session: Session, action: 'refresh' | 'snapshot') {
    if (current !== session) return;
    if (inFlight) { session.again = true; return; }
    clearTimeout(session.timer);
    inFlight = true; session.pending = true; session.again = false;
    const queryRevision = session.queryRevision;
    let scanning = false;
    try {
      const snapshot = await invoke<Catalog>('catalog_read', { request: {
        action, ...(action === 'snapshot' ? { scanId: session.scanId } : {}), filter: session.filter.trim(), offset: session.offset, limit: 100,
      } });
      if (current !== session) {
        if (action === 'refresh') void invoke('catalog_read', { request: { action: 'cancel', scanId: snapshot.scanId } }).catch(() => {});
        return;
      }
      if (session.scanId && snapshot.scanId !== session.scanId) {
        onError('Project roots changed. Refresh the project list.'); return;
      }
      session.scanId = snapshot.scanId; scanning = snapshot.scanning;
      if (queryRevision === session.queryRevision) {
        onSnapshot(snapshot); if (snapshot.error) onError(snapshot.error);
      }
      else session.again = true;
    } catch (error) { if (current === session && queryRevision === session.queryRevision) onError(error); }
    finally {
      session.pending = false; inFlight = false;
      if (current && current !== session) {
        void fetch(current, current.scanId ? 'snapshot' : 'refresh');
      } else if (current === session && (session.again || scanning)) {
        session.timer = window.setTimeout(() => void fetch(session, 'snapshot'), session.again ? 0 : 250);
      }
    }
  }
  return {
    open(filter = '') {
      if (current) cancel(current);
      const session: Session = { scanId: null, filter, offset: 0, queryRevision: 0, pending: false, again: false, timer: 0 };
      current = session; void fetch(session, 'refresh');
    },
    query(filter: string, offset = 0) {
      const session = current; if (!session) return;
      session.filter = filter; session.offset = offset; session.queryRevision++;
      clearTimeout(session.timer);
      if (session.pending) session.again = true;
      else session.timer = window.setTimeout(() => void fetch(session, session.scanId ? 'snapshot' : 'refresh'), 120);
    },
    close() { if (current) cancel(current); current = null; },
  };
}
