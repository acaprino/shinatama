// The answers of the browser for the dialogs (see mock/index.ts): the catalogue of folders (one root with three projects, roots that can be
// changed and the answers of the operations that can be cancelled), the creation of a project, the overview of the projects with their Git
// state, a worktree, and the review of a launch. Kept for the next read until the page is loaded again.
import type { Answers } from './index.ts';

type Root = { id: string; path: string };
let revision = 1;
let nextRoot = 2;
let roots: Root[] = [{ id: 'r1', path: 'C:\\work' }];
let defaultRootId: string | null = 'r1';
let pins = new Set<string>(['p_aaaaaaaaaaaa']);

const FOLDERS = [
  { name: 'shinatama', path: 'C:\\work\\shinatama', available: true, id: 'p_aaaaaaaaaaaa', last_opened: 1_700_000_000_000, modifiedMs: 1, rootId: 'r1' },
  { name: 'xiox', path: 'C:\\work\\xiox', available: true, id: 'p_bbbbbbbbbbbb', last_opened: 1_690_000_000_000, modifiedMs: 1, rootId: 'r1' },
  { name: 'mukade', path: 'C:\\work\\mukade', available: true, id: null, last_opened: null, modifiedMs: 1, rootId: 'r1' },
];
const HISTORY = [{ id: 'p_cccccccccccc', path: 'D:\\scratch\\notes', name: 'notes', last_opened: 1_680_000_000_000, available: true }];

const config = () => ({ version: 1, revision, roots, defaultRootId, nonStandardHistory: [] });
const catalog = (request: { filter?: string; offset?: number; limit?: number }) => {
  const query = (request.filter ?? '').toLowerCase();
  const projects = roots.length ? FOLDERS.filter(folder => `${folder.name} ${folder.path}`.toLowerCase().includes(query)) : [];
  return {
    scanId: 'scan-1', revision, rootsConfig: config(),
    roots: roots.map(root => ({ id: root.id, path: root.path, state: 'ready', message: null, projectCount: root.id === 'r1' ? FOLDERS.length : 0, projects: root.id === 'r1' ? FOLDERS : [] })),
    projects, history: defaultRootId ? HISTORY : [], recent: [], scanning: false, configLoading: false,
    total: projects.length, offset: request.offset ?? 0, limit: request.limit ?? 100, error: null, truncated: false,
  };
};

const overview = () => [
  { id: 'p_aaaaaaaaaaaa', path: 'C:\\work\\shinatama', name: 'shinatama', available: true, pinned: pins.has('p_aaaaaaaaaaaa'), lastOpened: 1_700_000_000_000,
    git: { state: 'known', branch: 'develop', dirty: 2, untracked: 1 }, lastThread: { id: 'w_aaaaaaaaaaaa', title: 'Migrate to Imago', harness: 'claude_code', revision: 1, updatedAt: 1_700_000_000_000 } },
  { id: 'p_bbbbbbbbbbbb', path: 'C:\\work\\xiox', name: 'xiox', available: true, pinned: pins.has('p_bbbbbbbbbbbb'), lastOpened: 1_690_000_000_000,
    git: { state: 'known', branch: 'main', dirty: 0, untracked: 0 }, lastThread: null },
  { id: 'p_cccccccccccc', path: 'D:\\scratch\\notes', name: 'notes', available: true, pinned: false, lastOpened: 1_680_000_000_000, git: { state: 'not_repository' }, lastThread: null },
];

export const answers: Answers = {
  catalog_begin: null,
  catalog_cancel: { status: 'already_completed' },
  catalog_read: args => {
    const request = args?.request as { action: string; filter?: string; offset?: number; limit?: number };
    return request.action === 'cancel' ? null : catalog(request);
  },
  catalog_roots_set: args => {
    const request = args?.request as { roots: { id?: string; path: string }[]; defaultRootIndex: number | null };
    roots = request.roots.map(root => ({ id: `r${nextRoot++}`, path: root.path }));
    defaultRootId = request.defaultRootIndex === null ? null : roots[request.defaultRootIndex]?.id ?? null;
    revision += 1;
    return config();
  },
  catalog_browse: args => {
    const purpose = (args?.request as { purpose: string }).purpose;
    return purpose === 'root' ? { kind: 'root', path: 'D:\\code' }
      : { kind: 'project', project: { id: 'p_dddddddddddd', path: 'D:\\code\\browsed', name: 'browsed', last_opened: 1_700_000_001_000, available: true } };
  },
  project_select: args => {
    const path = String(args?.path);
    const known = FOLDERS.find(folder => folder.path.toLowerCase() === path.toLowerCase());
    return { id: known?.id ?? 'p_eeeeeeeeeeee', path, name: path.split(/[\\/]/).pop(), last_opened: Date.now(), available: true };
  },
  new_project_options: () => ({ revision, roots, defaultRootId }),
  new_project_create: args => {
    const request = args?.request as { rootId: string; name: string };
    const root = roots.find(entry => entry.id === request.rootId);
    return { id: 'p_ffffffffffff', path: `${root?.path}\\${request.name}`, name: request.name, last_opened: Date.now(), available: true };
  },
  project_overview: () => overview(),
  project_pin: args => { const id = String(args?.projectId); if (args?.pinned) pins.add(id); else pins.delete(id); return [...pins]; },
  project_worktree_plan: args => ({ project: String(args?.project), path: `${args?.project}-${args?.name}`, branch: `agent/${args?.name}` }),
  project_worktree_create: args => String(args?.confirmedPath),
  launch_preflight: { agent: 'claude_code', action: 'start', placement: { program: 'system', home: 'box' }, status: 'incomplete', issues: [{ code: 'prepared_inputs_required', reason: null }], synthetic: false },
};

/** Puts the stand-in back as it was (a test starts from it; the browser has it at each load). */
export function resetDialogsMock(): void {
  revision = 1; nextRoot = 2; roots = [{ id: 'r1', path: 'C:\\work' }]; defaultRootId = 'r1'; pins = new Set(['p_aaaaaaaaaaaa']);
}
