import { uiPart, uiButton, uiInput, uiTraits } from '@acaprino/imago/ui';
import { closeControl } from './controls.ts';
import { listen } from '@tauri-apps/api/event';
import { openModal } from './modal.ts';
import { confirmsField } from './desktop-shell-model.ts';
import { itemText, leftoversFor, summaryText, type LeftoverStatus } from './leftovers-model.ts';

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

/** Pins, branch and dirty counts, last thread, new instances (optionally in a worktree), and
 * what agent sessions left in each project as Xiox sees it (read-only: the clean command is
 * only copied, never run). */
export function createProjectsOverview(options: Options) {
  const dialog = uiPart('dialog'); dialog.id = 'projects-overview-dialog'; dialog.setAttribute('aria-labelledby', 'projects-overview-title');
  const heading = uiPart('header'); heading.classList.add('dialog-heading');
  const title = uiPart('title'); title.id = 'projects-overview-title'; title.textContent = 'Projects';
  const closeButton = closeControl('Close projects');
  heading.append(title, closeButton);
  const status = uiPart('status');
  const list = uiTraits(uiPart('list'), 'scroll', 'bounded');
  dialog.append(heading, status, list);
  (document.querySelector('#app') ?? document.body).append(dialog);
  let generation = 0;
  let leftovers: LeftoverStatus = { available: false, reason: null, projects: [] };
  const slots = new Map<string, { row: ProjectRow; slot: HTMLElement }>();
  const expanded = new Set<string>();

  async function readLeftovers() {
    try { leftovers = await options.invoke<LeftoverStatus>('project_leftovers'); } catch { /* the overview works without Xiox */ }
  }
  function renderLeftovers(row: ProjectRow, slot: HTMLElement) {
    slot.replaceChildren();
    if (!leftovers.available || !row.available) return;
    const picture = leftoversFor(row.path, leftovers.projects);
    const head = uiPart('row');
    const line = uiPart('hint');
    const check = uiButton('');
    check.textContent = !picture ? 'Watch now' : picture.phase === 'ended' ? 'Restart Xiox' : 'Check now';
    check.title = 'Xiox lists the worktrees, copies, bench and backup folders and builds that appeared. It never removes anything.';
    check.disabled = picture?.phase === 'starting';
    check.onclick = async () => {
      check.disabled = true;
      try { await options.invoke('project_leftovers_check', { path: row.path }); status.dataset.state = 'ready'; status.textContent = `Xiox is checking ${row.name || row.path}…`; }
      catch (error) { status.dataset.state = 'error'; status.textContent = text(error); }
      await readLeftovers(); renderLeftovers(row, slot);
    };
    if (!picture) {
      line.textContent = 'Leftovers: not watched yet. Xiox starts with the first agent session here.';
      head.append(line, check); slot.append(head); return;
    }
    line.textContent = summaryText(picture);
    if (picture.attention.length) line.dataset.state = 'attention';
    head.append(line, check);
    slot.append(head);
    if (!picture.newItems.length) return;
    const toggle = uiButton('');
    const open = expanded.has(row.id);
    toggle.textContent = open ? 'Hide details' : 'Details'; toggle.setAttribute('aria-expanded', String(open));
    toggle.onclick = () => { if (expanded.has(row.id)) expanded.delete(row.id); else expanded.add(row.id); renderLeftovers(row, slot); };
    head.append(toggle);
    if (!open) return;
    const items = uiPart('list');
    items.append(...picture.newItems.map(entry => {
      const li = uiPart('item', itemText(entry));
      if (entry.blocked.length) li.dataset.blocked = 'true';
      return li;
    }));
    slot.append(items);
    if (!picture.cleanCommand) {
      const note = uiPart('hint', 'No clean command: the new items need your own choice (caution) or cannot be removed. See xiox clean --include.');
      slot.append(note); return;
    }
    const command = uiPart('code', picture.cleanCommand);
    const copy = uiButton('Copy clean command');
    copy.onclick = async () => {
      try {
        await navigator.clipboard.writeText(picture.cleanCommand ?? '');
        status.dataset.state = 'ready';
        status.textContent = `Clean command copied${picture.cleanCommandTruncated ? ' (the first 25 items; check again after running it)' : ''}. Paste it into a terminal: xiox clean lists what it will remove and asks first.`;
      } catch { status.dataset.state = 'error'; status.textContent = 'The clipboard is unavailable.'; }
    };
    slot.append(command, copy);
  }
  async function refreshLeftovers() {
    await readLeftovers();
    for (const { row, slot } of slots.values()) renderLeftovers(row, slot);
  }
  void listen('project-leftovers', () => { if (dialog.open) void refreshLeftovers(); }).catch(() => {});

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
    const item = uiPart('item'); item.classList.add('projects-overview-row'); item.dataset.projectId = row.id;
    const pin = uiButton(''); pin.classList.add('projects-pin'); pin.textContent = row.pinned ? '★' : '☆';
    pin.setAttribute('aria-pressed', String(row.pinned)); pin.setAttribute('aria-label', row.pinned ? `Unpin ${row.name}` : `Pin ${row.name}`);
    pin.onclick = async () => { try { await options.invoke('project_pin', { projectId: row.id, pinned: !row.pinned }); await load(); } catch (error) { status.textContent = text(error); } };
    const info = uiPart('stack');
    const name = uiPart('strong', row.name || row.path);
    const path = uiPart('hint', row.available ? row.path : `${row.path} · unavailable`);
    const facts = uiPart('hint'); facts.classList.add('projects-git'); facts.textContent = gitText(row.git);
    const last = uiPart('hint');
    last.textContent = row.lastThread ? `Last: ${row.lastThread.title ?? 'Untitled thread'} · ${row.lastThread.harness ?? 'no agent yet'}` : 'No thread yet';
    const leftoversSlot = uiPart('stack');
    slots.set(row.id, { row, slot: leftoversSlot }); renderLeftovers(row, leftoversSlot);
    info.append(name, path, facts, last);
    const actions = uiPart('actions');
    const resume = uiButton('Resume last');
    resume.disabled = !row.available || !row.lastThread?.harness;
    resume.onclick = async () => { if (!row.lastThread) return; try { dialog.close(); await options.resume(row, row.lastThread); } catch (error) { options.reportError(text(error)); } };
    const open = uiButton('New session'); open.disabled = !row.available;
    open.onclick = () => { dialog.close(); options.newSession(row.path); };
    const tree = uiButton('New in worktree…');
    tree.disabled = !row.available || row.git.state !== 'known';
    const form = uiPart('stack'); form.hidden = true;
    const field = uiInput('text'); field.value = 'task-1'; field.setAttribute('aria-label', `Worktree name for ${row.name}`);
    field.pattern = '[a-z0-9-]{1,40}';
    const create = uiButton('Create and open', { primary: true });
    create.onclick = () => { void worktree(row, field.value.trim()); };
    field.addEventListener('keydown', event => { if (confirmsField(event)) create.click(); });
    form.append(field, create);
    tree.onclick = () => { form.hidden = !form.hidden; if (!form.hidden) field.select(); };
    actions.append(resume, open, tree, form);
    item.append(pin, info, actions, leftoversSlot); return item;
  }
  async function load() {
    const at = ++generation; status.dataset.state = 'ready'; status.textContent = 'Reading projects and Git status…';
    try {
      const [rows] = await Promise.all([options.invoke<ProjectRow[]>('project_overview'), readLeftovers()]);
      if (at !== generation) return;
      slots.clear();
      list.replaceChildren(...rows.map(rowElement));
      const xiox = leftovers.available ? '' : leftovers.reason ? ` Leftovers are not watched: ${leftovers.reason}` : '';
      status.textContent = (rows.length ? `${rows.length} project${rows.length === 1 ? '' : 's'}. Pinned first.` : 'No projects yet. Open a folder first.') + xiox;
    } catch (error) { if (at === generation) { status.dataset.state = 'error'; status.textContent = text(error); } }
  }
  closeButton.onclick = () => dialog.close();
  dialog.addEventListener('close', () => { if (dialog.open) return; generation++; if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  async function show() { if (!dialog.open) openModal(dialog); await load(); }
  return { show };
}
