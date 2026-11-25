// Projects' threads (worksets: a thread is a set of sessions with a layout of its own) and the sessions in them (ADR-0055).
import type { ThreadSidebarState, ThreadSummary } from '../thread-sidebar.ts';
import { invoke } from './core.ts';

export type WorksetScope = { id: string; projectId: string };
export type WorksetLead = { threadId: string; title: string | null; harness: string | null };
export type WorksetSummary = {
  id: string; projectId: string; title: string | null; archived: boolean; revision: number;
  usedAt: number; sessions: number; open: number; lead: WorksetLead | null;
};
export type WorksetPage = { rows: WorksetSummary[]; more: boolean };
export type WorksetUpdate = { worksetId: string; expectedRevision: number; title?: string; archived?: boolean };
export type SidebarSnapshot = { revision: number; settings: ThreadSidebarState };
/** The name an agent gave a thread: read apart because the read shares a worker with the launches. */
export type ThreadName = { threadId: string; name: string | null };

export const threadsApi = {
  worksetList: (request: { projectId: string; archived: boolean }) => invoke<WorksetPage>('workset_list', { request }),
  worksetCreate: (request: { projectId: string }) => invoke<unknown>('workset_create', { request }),
  worksetUpdate: (request: WorksetUpdate) => invoke<unknown>('workset_update', { request }),
  worksetDelete: (request: { worksetId: string; expectedRevision: number }) => invoke<void>('workset_delete', { request }),
  /** The sessions of one thread, oldest first, closed ones included. */
  worksetThreads: (worksetId: string) => invoke<ThreadSummary[]>('workset_threads', { request: { worksetId } }),
  worksetAdopt: (worksetId: string, threadId: string) => invoke<void>('workset_adopt', { request: { worksetId, threadId } }),
  worksetOf: (threadId: string) => invoke<WorksetScope | null>('workset_of', { request: { threadId } }),
  threadRename: (request: { projectId: string; threadId: string; expectedRevision: number; title: string }) => invoke<unknown>('thread_rename', { request }),
  threadArchive: (request: { projectId: string; threadId: string; expectedRevision: number; archived: boolean }) => invoke<unknown>('thread_archive', { request }),
  threadForget: (request: { projectId: string; threadId: string; expectedRevision: number }) => invoke<unknown>('thread_forget', { request }),
  /** Closes the record a Shinatama that is gone left open (verifies that owner is gone; starts nothing). */
  threadRecover: (request: { threadId: string; projectId: string; expectedRevision: number }) => invoke<void>('thread_recover', { request }),
  sidebarLoad: () => invoke<SidebarSnapshot>('thread_sidebar_load'),
  sidebarSave: (expectedRevision: number, settings: ThreadSidebarState) => invoke<SidebarSnapshot>('thread_sidebar_save', { request: { expectedRevision, settings } }),
  names: (threadIds: string[]) => invoke<ThreadName[]>('thread_names', { threadIds }),
  summariesPause: (threadId: string, paused: boolean) => invoke<string[]>('thread_summaries_pause', { threadId, paused }),
  instanceTitles: (requests: unknown[]) => invoke<unknown>('instance_titles', { requests }),
  titlesEnabled: () => invoke<boolean>('titles_enabled'),
  titlesSetEnabled: (enabled: boolean) => invoke<void>('titles_set_enabled', { enabled }),
};

export type ChatOverview = {
  service: string;
  rooms: Record<string, { latest: string; lastActivity: string | null; unread: boolean }>;
  connected: Record<string, string[]>;
  launched: Record<string, string[]>;
};
export type ChatSeen = { eventId: string; timestamp: string };

/** The conversation of a thread whose sessions talk to each other (ADR-0057). */
export const chatApi = {
  list: () => invoke<string[]>('thread_chat_list'),
  set: (worksetId: string, enabled: boolean) => invoke<{ enabled: string[]; warning: string | null }>('thread_chat_set', { request: { worksetId, enabled } }),
  overview: (worksetIds: string[], seen: Record<string, ChatSeen>) => invoke<ChatOverview>('thread_chat_overview', { request: { worksetIds, seen } }),
  snapshot: (request: unknown) => invoke<unknown>('thread_chat_snapshot', { request }),
  send: (request: unknown) => invoke<unknown>('thread_chat_send', { request }),
};
