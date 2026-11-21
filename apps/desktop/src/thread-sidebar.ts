/** Thread types shared by the workset sidebar (workset-sidebar.ts), session tabs and main.ts. The sidebar that
 *  lived here, createThreadSidebar, was replaced by createWorksetSidebar and had no caller left. */
export type CurrentThreadProject = { projectId: string; label?: string };
export type ThreadCursor = { updatedAt: number; threadId: string };
export type ThreadSummary = {
  id: string; projectId: string; kind: 'work' | 'roundtable' | 'companion'; title: string | null;
  revision: number; updatedAt: number; harness: string | null; nativeAvailable: boolean | null;
  status: 'unbound' | 'inactive' | 'recorded_open' | 'archived';
};
export type ThreadListPage = { rows: ThreadSummary[]; next: ThreadCursor | null };
export type ThreadListRequest = { projectId: string; cursor: ThreadCursor | null; limit: 50; archived: boolean };
export type ThreadArchiveRequest = { projectId: string; threadId: string; expectedRevision: number; archived: boolean };
export type ThreadForgetRequest = { projectId: string; threadId: string; expectedRevision: number };
export type ThreadRenameRequest = { projectId: string; threadId: string; expectedRevision: number; title: string };
export type ThreadSidebarState = { version: 1; width: number; collapsed: boolean; activeProjects?: string[]; hidden?: boolean };
