import { openModal } from './modal.ts';
import './instances-overview.css';

type Git = { state: 'known'; branch: string | null; dirty: number; untracked: number } | { state: 'not_repository' } | { state: 'unknown'; reason: string };
type Row = { id: string; agent: string; cwd: string; state: string; stateSource: string; certain: boolean; git: Git; usage: string };
type Options = { invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>; focus: (id: string) => boolean;
  threadOf: (id: string) => string | null; restoreFocus?: () => void };

const STATES: Record<string, string> = {
  starting: 'Starting', attention_requested: 'Asked for attention', producing_output: 'Producing output', quiet: 'Quiet', exited: 'Exited', error: 'Exited with an error',
};
const SOURCES: Record<string, string> = {
  process: 'from the process', terminal_signal: 'from a terminal signal, unsure', terminal_output: 'from terminal output, unsure', none: 'nothing observed yet',
};
const AGENTS: Record<string, string> = { claude_code: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi', hermes: 'Hermes Agent', deepseek: 'DeepSeek Harness', PowerShell: 'PowerShell' };

/** Read-mostly overview: state with its source, branch, and a Focus action only (FRD-0010 R14). */
export function createInstancesOverview(options: Options) {
  const dialog = document.createElement('dialog'); dialog.id = 'instances-dialog'; dialog.setAttribute('aria-labelledby', 'instances-title');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'instances-title'; title.textContent = 'Instances';
  const closeButton = document.createElement('button'); closeButton.type = 'button'; closeButton.textContent = '×'; closeButton.setAttribute('aria-label', 'Close instances');
  heading.append(title, closeButton);
  const note = document.createElement('p'); note.className = 'hint';
  note.textContent = 'States come only from what Shinatama observed. Running states are unsure until native agent hooks are connected; a terminal signal never approves anything.';
  const table = document.createElement('table'); table.className = 'instances-table';
  const status = document.createElement('p'); status.className = 'instances-status'; status.setAttribute('role', 'status');
  dialog.append(heading, note, table, status);
  (document.querySelector('#app') ?? document.body).append(dialog);
  let timer = 0;
  function gitText(git: Git) {
    if (git.state === 'known') return `${git.branch ?? 'detached'} · ${git.dirty + git.untracked ? `${git.dirty} changed, ${git.untracked} untracked` : 'clean'}`;
    return git.state === 'not_repository' ? 'no Git' : 'Git unknown';
  }
  async function load() {
    try {
      const rows = await options.invoke<Row[]>('instances_overview');
      const head = document.createElement('tr');
      for (const label of ['Agent', 'Folder', 'Branch', 'State', '']) { const th = document.createElement('th'); th.textContent = label; head.append(th); }
      const body = rows.map(row => {
        const tr = document.createElement('tr'); tr.dataset.instanceId = row.id;
        const cells = [AGENTS[row.agent] ?? row.agent, row.cwd, gitText(row.git), `${STATES[row.state] ?? row.state} (${SOURCES[row.stateSource] ?? row.stateSource})`];
        for (const text of cells) { const td = document.createElement('td'); td.textContent = text; tr.append(td); }
        const action = document.createElement('td'); const focus = document.createElement('button'); focus.type = 'button'; focus.textContent = 'Focus';
        focus.onclick = () => { if (options.focus(row.id)) dialog.close(); else status.textContent = 'This instance has no pane in this window.'; };
        action.append(focus); tr.append(action);
        tr.title = options.threadOf(row.id) ? `Thread ${options.threadOf(row.id)}. ${row.usage}` : row.usage;
        return tr;
      });
      table.replaceChildren(head, ...body);
      status.textContent = rows.length ? `${rows.length} instance${rows.length === 1 ? '' : 's'}.` : 'No instance is running.';
    } catch (error) { status.textContent = typeof error === 'string' ? error : 'The overview could not be read.'; }
  }
  closeButton.onclick = () => dialog.close();
  dialog.addEventListener('close', () => { clearInterval(timer); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  async function show() { if (!dialog.open) openModal(dialog); await load(); clearInterval(timer); timer = window.setInterval(() => { if (dialog.open) void load(); }, 2000); }
  return { show };
}
