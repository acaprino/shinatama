// The projects: the one the window works in, the recent ones, the catalogue of folders that could be projects (project-catalog.ts
// keeps its own client), a new project, the overview of the pinned ones and what agents left on disk.
import { invoke } from './core.ts';

export type ProjectEntry = { id: string; path: string; name: string; last_opened: number; available: boolean };

export const projectsApi = {
  /** The project the window was started in (the folder Shinatama was opened on). */
  path: () => invoke<string>('project_path'),
  recent: () => invoke<ProjectEntry[]>('project_recent'),
  select: (path: string, requestId?: string) => invoke<ProjectEntry>('project_select', { path, requestId }),
  newProjectOptions: () => invoke<unknown>('new_project_options'),
  newProjectCreate: (request: unknown, requestId: string) => invoke<unknown>('new_project_create', { request, requestId }),
  overview: () => invoke<unknown>('project_overview'),
  pin: (projectId: string, pinned: boolean) => invoke<string[]>('project_pin', { projectId, pinned }),
  worktreePlan: (project: string, name: string) => invoke<unknown>('project_worktree_plan', { project, name }),
  worktreeCreate: (project: string, name: string, confirmedPath: string) => invoke<string>('project_worktree_create', { project, name, confirmedPath }),
  instances: () => invoke<unknown>('instances_overview'),
  leftovers: () => invoke<unknown>('project_leftovers'),
  leftoversCheck: (path: string) => invoke<void>('project_leftovers_check', { path }),
  leftoversReveal: (path: string) => invoke<void>('leftovers_reveal', { path }),
};
