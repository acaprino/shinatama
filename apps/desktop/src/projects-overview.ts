import { openModal } from './modal.ts';
import './projects-overview.css';

type Git = { state: 'known'; branch: string | null; dirty: number; untracked: number } | { state: 'not_repository' } | { state: 'unknown'; reason: string };
type LastThread = { id: string; title: string | null; harness: string | null; revision: number; updatedAt: number };
export type ProjectRow = { id: string; path: string; name: string; available: boolean; pinned: boolean; lastOpened: number; git: Git; lastThread: LastThread | null };
type Plan = { project: string; path: string; branch: string };
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
type Options = {
  invoke: Invoke;
  newSession: (path: string) => void;
  resume: (row: ProjectRow, thread: LastThread) => Promise<void>;
  decide: (title: string, message: string, accept: string) => Promise<boolean>;
  reportError: (message: string) => void;
  restoreFocus?: () => void;
};

const text = (error: unknown) => typeof error === 'string' ? error : error instanceof Error ? error.message : 'The project action failed.';
function gitText(git: Git): string {
  if (git.state === 'not_repository') return 'Not a Git repository';
  if (git.state === 'unknown') return `Git: unknown (${git.reason})`;
  const changes = git.dirty + git.untracked;
  return `${git.branch ?? 'detached'} · ${changes ? `${git.dirty} changed, ${git.untracked} untracked` : 'clean'}`;
}

/** Pins, branch and dirty counts, last thread, and new instances (optionally in a worktree). */
export function createProjectsOverview(options: Options) {
  const dialog = document.createElement('dialog'); dialog.id = 'projects-overview-dialog'; dialog.setAttribute('aria-labelledby', 'projects-overview-title');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'projects-overview-title'; title.textContent = 'Projects';
  const closeButton = document.createElement('button'); closeButton.type = 'button'; closeButton.textContent = '×'; closeButton.setAttribute('aria-label', 'Close projects');
  heading.append(title, closeButton);
  const status = document.createElement('p'); status.className = 'projects-overview-status'; status.setAttribute('role', 'status');
  const list = document.createElement('ul'); list.className = 'projects-overview-list';
  dialog.append(heading, status, list);
  (document.querySelector('#app') ?? document.body).append(dialog);
  let generation = 0;

  async function worktree(row: ProjectRow, name: string) {
    if (!name) { status.textContent = 'Enter a worktree name.'; return; }
    try {
      const plan = await options.invoke<Plan>('project_worktree_plan', { project: row.path, name });
      if (!await options.decide('Create a worktree?', `Git will create ${plan.path} on a new branch ${plan.branch}, then a new session opens there. Nothing existing is changed or deleted.`, 'Create')) return;
      const path = await options.invoke<string>('project_worktree_create', { project: row.path, name, confirmedPath: plan.path });
      dialog.close(); options.newSession(path);
    } catch (error) { status.dataset.state = 'error'; status.textContent = text(error); }
  }
  function rowElement(row: ProjectRow) {
    const item = document.createElement('li'); item.className = 'projects-overview-row'; item.dataset.projectId = row.id;
    const pin = document.createElement('button'); pin.type = 'button'; pin.className = 'projects-pin'; pin.textContent = row.pinned ? '★' : '☆';
    pin.setAttribute('aria-pressed', String(row.pinned)); pin.setAttribute('aria-label', row.pinned ? `Unpin ${row.name}` : `Pin ${row.name}`);
    pin.onclick = async () => { try { await options.invoke('project_pin', { projectId: row.id, pinned: !row.pinned }); await load(); } catch (error) { status.textContent = text(error); } };
    const info = document.createElement('div'); info.className = 'projects-overview-info';
    const name = document.createElement('strong'); name.textContent = row.name || row.path;
    const path = document.createElement('small'); path.textContent = row.available ? row.path : `${row.path} · unavailable`;
    const facts = document.createElement('small'); facts.className = 'projects-git'; facts.textContent = gitText(row.git);
    const last = document.createElement('small');
    last.textContent = row.lastThread ? `Last: ${row.lastThread.title ?? 'Untitled thread'} · ${row.lastThread.harness ?? 'no agent yet'}` : 'No thread yet';
    info.append(name, path, facts, last);
    const actions = document.createElement('div'); actions.className = 'projects-overview-actions';
    const resume = document.createElement('button'); resume.type = 'button'; resume.textContent = 'Resume last';
    resume.disabled = !row.available || !row.lastThread?.harness;
    resume.onclick = async () => { if (!row.lastThread) return; try { dialog.close(); await options.resume(row, row.lastThread); } catch (error) { options.reportError(text(error)); } };
    const open = document.createElement('button'); open.type = 'button'; open.textContent = 'New session'; open.disabled = !row.available;
    open.onclick = () => { dialog.close(); options.newSession(row.path); };
    const tree = document.createElement('button'); tree.type = 'button'; tree.textContent = 'New in worktree…';
    tree.disabled = !row.available || row.git.state !== 'known';
    const form = document.createElement('div'); form.className = 'projects-worktree'; form.hidden = true;
    const field = document.createElement('input'); field.type = 'text'; field.value = 'task-1'; field.setAttribute('aria-label', `Worktree name for ${row.name}`);
    field.pattern = '[a-z0-9-]{1,40}';
    const create = document.createElement('button'); create.type = 'button'; create.className = 'primary'; create.textContent = 'Create and open';
    create.onclick = () => { void worktree(row, field.value.trim()); };
    field.addEventListener('keydown', event => { if (event.key === 'Enter') create.click(); });
    form.append(field, create);
    tree.onclick = () => { form.hidden = !form.hidden; if (!form.hidden) field.select(); };
    actions.append(resume, open, tree, form);
    item.append(pin, info, actions); return item;
  }
  async function load() {
    const at = ++generation; status.dataset.state = 'ready'; status.textContent = 'Reading projects and Git status…';
    try {
      const rows = await options.invoke<ProjectRow[]>('project_overview');
      if (at !== generation) return;
      list.replaceChildren(...rows.map(rowElement));
      status.textContent = rows.length ? `${rows.length} project${rows.length === 1 ? '' : 's'}. Pinned first.` : 'No projects yet. Open a folder first.';
    } catch (error) { if (at === generation) { status.dataset.state = 'error'; status.textContent = text(error); } }
  }
  closeButton.onclick = () => dialog.close();
  dialog.addEventListener('close', () => { generation++; if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  async function show() { if (!dialog.open) openModal(dialog); await load(); }
  return { show };
}
