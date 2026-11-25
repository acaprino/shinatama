// Every session Shinatama can read (running or saved, in every store), their details, search, export and the hop to another agent.
import { invoke } from './core.ts';

export const sessionsApi = {
  roots: () => invoke<unknown>('session_roots'),
  list: (request: unknown) => invoke<unknown>('session_list', { request }),
  archiveAdd: (request: unknown) => invoke<unknown>('session_archive_add', { request }),
  detail: (request: unknown) => invoke<unknown>('session_detail', { request }),
  raw: (request: unknown) => invoke<unknown>('session_raw', { request }),
  find: (request: unknown, requestId?: string) => invoke<unknown>('session_find', { request, requestId }),
  /** Cancels a long find or export (its progress arrives as `session-progress` events). */
  cancel: (requestId: string) => invoke<boolean>('session_cancel', { requestId }),
  export: (request: unknown, requestId?: string) => invoke<unknown>('session_export', { request, requestId }),
  search: (request: unknown) => invoke<unknown>('session_search', { request }),
  indexStart: () => invoke<unknown>('session_index_start'),
  indexCancel: () => invoke<void>('session_index_cancel'),
  indexStatus: () => invoke<unknown>('session_index_status'),
  indexEnable: (enabled: boolean) => invoke<void>('session_index_enable', { enabled }),
  indexForget: (storeRootId: string | null = null) => invoke<number>('session_index_forget', { storeRootId }),
  /** What the session's agent can do (resume, fork...), by the id its store gave it. */
  capabilities: (nativeId: string) => invoke<unknown>('sidecar_capabilities', { nativeId }),
  hopPreview: (request: unknown) => invoke<unknown>('hop_preview', { request }),
  hopLaunch: (request: { hopId: string; rows: number; cols: number }) => invoke<unknown>('hop_launch', { request }),
  hopDeliver: (hopId: string) => invoke<void>('hop_deliver', { hopId }),
  hopFinish: (hopId: string, outcome: 'receipt' | 'accept_without_receipt' | 'fail') => invoke<unknown>('hop_finish', { hopId, outcome }),
  hopStatus: (hopId: string) => invoke<unknown>('hop_status', { hopId }),
  hopsRecent: () => invoke<unknown>('hops_recent'),
};
