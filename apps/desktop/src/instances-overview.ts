import { openModal } from './modal.ts';
import './instances-overview.css';

type Git = { state: 'known'; branch: string | null; dirty: number; untracked: number } | { state: 'not_repository' } | { state: 'unknown'; reason: string };
type Row = { id: string; agent: string; cwd: string; state: string; stateSource: string; certain: boolean; git: Git; usage: string };
type Titles = { enabled: boolean; excluded: string[]; rows: { instanceId: string; threadId: string | null; title: string | null; reason: string | null }[] };
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
  const titleSwitch = document.createElement('label'); titleSwitch.className = 'instances-switch';
  const titleBox = document.createElement('input'); titleBox.type = 'checkbox';
  titleSwitch.append(titleBox, document.createTextNode(' Activity titles (verbatim from the last tool call or message; nothing leaves this computer)'));
  titleBox.onchange = async () => { await options.invoke('titles_set_enabled', { enabled: titleBox.checked }).catch(() => {}); void load(); };
  const table = document.createElement('table'); table.className = 'instances-table';
  const status = document.createElement('p'); status.className = 'instances-status'; status.setAttribute('role', 'status');
  dialog.append(heading, note, titleSwitch, table, status);
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
      const titles = await options.invoke<Titles>('instance_titles', { requests: rows.map(r => ({ instanceId: r.id, threadId: options.threadOf(r.id), state: r.state })) }).catch(() => null);
      titleBox.checked = titles?.enabled ?? false;
      for (const label of ['Agent', 'Folder', 'Branch', 'State', 'Doing', '']) { const th = document.createElement('th'); th.textContent = label; head.append(th); }
      const body = rows.map(row => {
        const tr = document.createElement('tr'); tr.dataset.instanceId = row.id;
        const cells = [AGENTS[row.agent] ?? row.agent, row.cwd, gitText(row.git), `${STATES[row.state] ?? row.state} (${SOURCES[row.stateSource] ?? row.stateSource})`];
        const t = titles?.rows.find(x => x.instanceId === row.id);
        cells.push(t?.title ?? (t?.reason ? `(${t.reason})` : '-'));
        for (const text of cells) { const td = document.createElement('td'); td.textContent = text; tr.append(td); }
        const action = document.createElement('td'); const focus = document.createElement('button'); focus.type = 'button'; focus.textContent = 'Focus';
        focus.onclick = () => { if (options.focus(row.id)) dialog.close(); else status.textContent = 'This instance has no pane in this window.'; };
        action.append(focus);
        const thread = options.threadOf(row.id);
        if (thread) {
          const paused = titles?.excluded.includes(thread) ?? false;
          const toggle = document.createElement('button'); toggle.type = 'button'; toggle.textContent = paused ? 'Resume summaries' : 'Pause summaries';
          toggle.onclick = async () => { await options.invoke('thread_summaries_pause', { threadId: thread, paused: !paused }).catch(() => {}); void load(); };
          action.append(toggle);
        }
        tr.append(action);
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
