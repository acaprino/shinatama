import { openModal } from './modal.ts';
import './instances-overview.css';

type Git = { state: 'known'; branch: string | null; dirty: number; untracked: number } | { state: 'not_repository' } | { state: 'unknown'; reason: string };
type Row = { id: string; agent: string; cwd: string; state: string; stateSource: string; certain: boolean; git: Git; usage: string };
type Usage = { requests: number; promptTokens: number; outputTokens: number; hitRatePermille: number | null };
type Collision = { kind: 'file_collision'; worktree: string; path: string; instances: string[] } | { kind: 'possible_collision'; worktree: string; path: string };
type Titles = { enabled: boolean; excluded: string[]; collisions: Collision[]; rows: { instanceId: string; threadId: string | null; title: string | null; reason: string | null; touched: number; usage: Usage | null }[] };
type HopLine = { id: string; state: string; mode: string; sourceAgent: string; destinationAgent: string; createdAtMs: number; updatedAtMs: number };
type Options = { invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>; focus: (id: string) => boolean;
  threadOf: (id: string) => string | null; restoreFocus?: () => void };

const STATES: Record<string, string> = {
  starting: 'Starting', attention_requested: 'Asked for attention', producing_output: 'Producing output', quiet: 'Quiet', exited: 'Exited', error: 'Exited with an error',
};
const SOURCES: Record<string, string> = {
  process: 'from the process', terminal_signal: 'from a terminal signal, unsure', terminal_output: 'from terminal output, unsure', none: 'nothing observed yet',
};
const AGENTS: Record<string, string> = { claude_code: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi', hermes: 'Hermes Agent', deepseek: 'DeepSeek Harness', PowerShell: 'PowerShell' };
/** Each state's place in the summary line, in order of urgency. */
const SUMMARY: [string, string][] = [['attention_requested', 'asking for attention'], ['producing_output', 'producing output'], ['error', 'exited with an error'], ['starting', 'starting'], ['quiet', 'quiet'], ['exited', 'exited']];
const REFRESH_MS = 2000;

const agentName = (agent: string) => AGENTS[agent] ?? agent;
const count = (value: number) => value.toLocaleString();
function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag); if (className) node.className = className; if (text !== undefined) node.textContent = text; return node;
}
function gitText(git: Git) {
  if (git.state === 'known') return `${git.branch ?? 'detached'} · ${git.dirty + git.untracked ? `${git.dirty} changed, ${git.untracked} untracked` : 'clean'}`;
  return git.state === 'not_repository' ? 'no Git' : 'Git unknown';
}
function usageText(usage: Usage) {
  return `${count(usage.promptTokens)} in · ${count(usage.outputTokens)} out · ${usage.requests} req${usage.hitRatePermille === null ? '' : ` · cache ${(usage.hitRatePermille / 10).toFixed(0)} %`}`;
}
function hopTime(ms: number) {
  const at = new Date(ms); const today = new Date().toDateString() === at.toDateString();
  return today ? at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : at.toLocaleString([], { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
}
/** "4 instances · 1 asking for attention · 2 producing output", most urgent first. */
export function summaryLine(rows: { state: string }[]): string {
  if (!rows.length) return 'No instance is running.';
  const parts = [`${rows.length} instance${rows.length === 1 ? '' : 's'}`];
  for (const [state, label] of SUMMARY) { const n = rows.filter(row => row.state === state).length; if (n) parts.push(`${n} ${label}`); }
  return parts.join(' · ');
}

/** Read-mostly overview: state with its source, branch, and a Focus action only (FRD-0010 R14). */
export function createInstancesOverview(options: Options) {
  const dialog = el('dialog'); dialog.id = 'instances-dialog'; dialog.setAttribute('aria-labelledby', 'instances-title');
  const heading = el('div', 'dialog-heading');
  const title = el('h1', '', 'Instances'); title.id = 'instances-title';
  const closeButton = el('button', '', '×'); closeButton.type = 'button'; closeButton.setAttribute('aria-label', 'Close instances');
  heading.append(title, closeButton);
  const status = el('p', 'instances-summary'); status.setAttribute('role', 'status');

  // Activity titles: a setting of this view, so a quiet switch rather than a form field.
  const titleSwitch = el('div', 'instances-setting');
  const titleBox = el('input', 'instances-switch'); titleBox.type = 'checkbox'; titleBox.id = 'instances-titles'; titleBox.setAttribute('role', 'switch');
  const titleLabel = el('label', 'instances-setting-label', 'Activity titles'); titleLabel.htmlFor = titleBox.id;
  const titleHint = el('span', 'instances-setting-hint', 'Verbatim from the last tool call or message; nothing leaves this computer.'); titleHint.id = 'instances-titles-hint';
  titleBox.setAttribute('aria-describedby', titleHint.id);
  titleSwitch.append(titleBox, titleLabel, titleHint);
  titleBox.onchange = async () => {
    titleBox.disabled = true;
    await options.invoke('titles_set_enabled', { enabled: titleBox.checked }).catch(() => { titleBox.checked = !titleBox.checked; });
    titleBox.disabled = false; void load(true);
  };

  const warnings = el('section', 'instances-warnings'); warnings.setAttribute('aria-label', 'Possible conflicts'); warnings.hidden = true;
  const list = el('ul', 'instances-list'); list.setAttribute('aria-label', 'Running instances');
  const timelineHeading = el('h2', 'instances-subheading', 'Recent hops'); timelineHeading.hidden = true;
  const timeline = el('ol', 'instances-timeline');
  const legend = el('p', 'instances-legend');
  const certain = el('span', 'instances-legend-item'); certain.append(el('i', 'instances-marker certain'), 'Certain, from the process');
  const inferred = el('span', 'instances-legend-item'); inferred.append(el('i', 'instances-marker'), 'Inferred from the terminal');
  legend.append(certain, inferred, el('span', 'instances-legend-note', 'States come only from what Shinatama observed. Running states are unsure until native agent hooks are connected; a terminal signal never approves anything.'));
  dialog.append(heading, status, titleSwitch, warnings, list, timelineHeading, timeline, legend);
  (document.querySelector('#app') ?? document.body).append(dialog);

  let timer = 0; let reading = false; let drawn = ''; let revision = 0; let notice = 0;

  function instance(row: Row, titles: Titles | null): HTMLLIElement {
    const item = el('li', 'instances-item'); item.dataset.instanceId = row.id; item.dataset.state = row.state;
    const marker = el('i', `instances-marker${row.certain ? ' certain' : ''}`); marker.setAttribute('aria-hidden', 'true');
    const head = el('div', 'instances-head');
    const name = el('span', 'instances-agent', agentName(row.agent));
    const tag = el('span', 'instances-id', row.id.slice(-4)); tag.title = row.id;
    const actions = el('div', 'instances-actions');
    const thread = options.threadOf(row.id);
    if (thread) {
      const paused = titles?.excluded.includes(thread) ?? false;
      const toggle = el('button', 'instances-quiet', paused ? 'Resume summaries' : 'Pause summaries'); toggle.type = 'button'; toggle.dataset.key = `${row.id}:summaries`;
      toggle.onclick = async () => { toggle.disabled = true; await options.invoke('thread_summaries_pause', { threadId: thread, paused: !paused }).catch(() => {}); void load(true); };
      actions.append(toggle);
    }
    const focus = el('button', '', 'Focus'); focus.type = 'button'; focus.dataset.key = `${row.id}:focus`;
    focus.setAttribute('aria-label', `Focus ${agentName(row.agent)} ${row.id.slice(-4)}`);
    focus.onclick = () => {
      if (options.focus(row.id)) { dialog.close(); return; }
      status.textContent = 'This instance has no pane in this window.'; status.dataset.tone = 'info';
      // The summary comes back on its own after a moment.
      clearTimeout(notice); notice = window.setTimeout(() => { drawn = ''; void load(true); }, 4000);
    };
    actions.append(focus);
    head.append(name, tag, actions);
    const state = el('p', 'instances-state');
    state.append(el('span', 'instances-state-name', STATES[row.state] ?? row.state), el('span', 'instances-state-source', ` · ${SOURCES[row.stateSource] ?? row.stateSource}`));
    const facts = el('dl', 'instances-facts');
    const fact = (label: string, value: string, className = '') => {
      const term = el('dt', '', label); const detail = el('dd', className, value); detail.title = value; facts.append(term, detail);
    };
    fact('Folder', row.cwd, 'path');
    fact('Git', gitText(row.git), row.git.state === 'known' ? '' : 'muted');
    const titled = titles?.rows.find(entry => entry.instanceId === row.id);
    if (titled?.title) fact('Doing', titled.title);
    else if (titled?.reason) fact('Doing', `(${titled.reason})`, 'muted');
    if (titled?.usage) fact('Tokens', usageText(titled.usage), 'numbers');
    item.title = thread ? `Thread ${thread}. ${row.usage}` : row.usage;
    item.append(marker, head, state, facts);
    return item;
  }

  function collision(entry: Collision, rows: Row[]): HTMLLIElement {
    const name = (id: string) => { const row = rows.find(candidate => candidate.id === id); return row ? `${agentName(row.agent)} (${id.slice(-4)})` : id; };
    const item = el('li'); const path = el('code', '', entry.path);
    if (entry.kind === 'file_collision') item.append(`Both ${entry.instances.map(name).join(' and ')} changed `, path);
    else item.append(path, " changed and no instance's own edits name it (possible collision)");
    return item;
  }

  /** Redrawn only when what it shows has changed: no flicker, and the focused button keeps focus. */
  async function load(force = false) {
    // A refresh never overlaps a slow read (Git can take a while); an explicit reload does, and the newest wins.
    if (reading && !force) return;
    reading = true; const mine = ++revision;
    try {
      const rows = await options.invoke<Row[]>('instances_overview');
      const titles = await options.invoke<Titles>('instance_titles', { requests: rows.map(r => ({ instanceId: r.id, threadId: options.threadOf(r.id), state: r.state })) }).catch(() => null);
      const hops = await options.invoke<HopLine[]>('hops_recent').catch(() => [] as HopLine[]);
      if (!dialog.open || mine !== revision) return;
      const signature = JSON.stringify([rows, titles, hops, rows.map(row => options.threadOf(row.id))]);
      if (signature === drawn && !status.dataset.tone) return;
      if (status.dataset.tone === 'info' && signature === drawn) return;
      drawn = signature;
      const focused = (document.activeElement as HTMLElement | null)?.dataset?.key;
      titleBox.checked = titles?.enabled ?? false;
      status.textContent = summaryLine(rows); delete status.dataset.tone;
      const collisions = titles?.collisions ?? [];
      warnings.hidden = !collisions.length;
      const warningList = el('ul'); warningList.append(...collisions.map(entry => collision(entry, rows)));
      warnings.replaceChildren(el('h2', 'instances-warnings-title', collisions.length === 1 ? 'Possible conflict' : 'Possible conflicts'), warningList);
      list.replaceChildren(...rows.map(row => instance(row, titles)));
      list.hidden = !rows.length;
      timelineHeading.hidden = !hops.length;
      timeline.replaceChildren(...hops.map(hop => {
        const item = el('li'); const time = el('time', 'instances-hop-time', hopTime(hop.updatedAtMs)); time.dateTime = new Date(hop.updatedAtMs).toISOString();
        item.append(time, el('span', 'instances-hop-route', `${agentName(hop.sourceAgent)} → ${agentName(hop.destinationAgent)}`), el('span', 'instances-hop-state', `${hop.mode} · ${hop.state}`));
        return item;
      }));
      if (focused) dialog.querySelector<HTMLElement>(`[data-key="${CSS.escape(focused)}"]`)?.focus();
    } catch (error) {
      if (mine !== revision) return;
      drawn = '';
      status.textContent = typeof error === 'string' ? error : 'The overview could not be read.'; status.dataset.tone = 'error';
    } finally { if (mine === revision) reading = false; }
  }
  closeButton.onclick = () => dialog.close();
  dialog.addEventListener('close', () => { clearInterval(timer); clearTimeout(notice); drawn = ''; if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  async function show() {
    if (!dialog.open) { status.textContent = 'Reading instances…'; delete status.dataset.tone; openModal(dialog); }
    await load(true); clearInterval(timer); timer = window.setInterval(() => { if (dialog.open) void load(); }, REFRESH_MS);
  }
  return { show };
}
