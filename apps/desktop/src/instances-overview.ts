import './instances-overview.css';

type Git = { state: 'known'; branch: string | null; dirty: number; untracked: number } | { state: 'not_repository' } | { state: 'unknown'; reason: string };
export type InstanceRow = { id: string; agent: string; cwd: string; state: string; stateSource: string; certain: boolean; git: Git; usage: string };
type Row = InstanceRow;
type Usage = { requests: number; promptTokens: number; outputTokens: number; hitRatePermille: number | null };
type Collision = { kind: 'file_collision'; worktree: string; path: string; instances: string[] } | { kind: 'possible_collision'; worktree: string; path: string };
type Titles = { enabled: boolean; excluded: string[]; collisions: Collision[]; rows: { instanceId: string; threadId: string | null; title: string | null; reason: string | null; touched: number; usage: Usage | null }[] };
type HopLine = { id: string; state: string; mode: string; sourceAgent: string; destinationAgent: string; createdAtMs: number; updatedAtMs: number };
type Options = {
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  /** Brings the instance's pane to the front; false when this window has none. */
  focus: (id: string) => boolean;
  threadOf: (id: string) => string | null;
  /** A Focus succeeded: the panel that holds the section may close. */
  focused(): void;
  /** A short message for the panel's status line. */
  notice(text: string): void;
};

const STATES: Record<string, string> = {
  starting: 'Starting', attention_requested: 'Asked for attention', producing_output: 'Producing output', quiet: 'Quiet', exited: 'Exited', error: 'Exited with an error',
};
const SOURCES: Record<string, string> = {
  process: 'from the process', terminal_signal: 'from a terminal signal, unsure', terminal_output: 'from terminal output, unsure', none: 'nothing observed yet',
};
const AGENTS: Record<string, string> = { claude_code: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi', hermes: 'Hermes Agent', deepseek: 'DeepSeek Harness', PowerShell: 'PowerShell' };
/** Each state's place in the summary line, in order of urgency. */
const SUMMARY: [string, string][] = [['attention_requested', 'asking for attention'], ['producing_output', 'producing output'], ['error', 'exited with an error'], ['starting', 'starting'], ['quiet', 'quiet'], ['exited', 'exited']];

export const agentName = (agent: string) => AGENTS[agent] ?? agent;
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
/** "2 running · 1 asking for attention · 1 producing output", most urgent first; '' when none runs. */
export function summaryLine(rows: { state: string }[]): string {
  if (!rows.length) return '';
  const parts: string[] = [];
  for (const [state, label] of SUMMARY) { const n = rows.filter(row => row.state === state).length; if (n) parts.push(`${n} ${label}`); }
  return parts.join(' · ');
}

/** The running instances as a section of the Sessions panel: state with its source, branch and a
 * Focus action only (FRD-0010 R14). The panel owns the heading, the status line and the refresh. */
export function createInstancesSection(options: Options) {
  const element = el('div', 'instances-section');
  const warnings = el('div', 'instances-warnings'); warnings.setAttribute('role', 'group'); warnings.setAttribute('aria-label', 'Possible conflicts'); warnings.hidden = true;
  const list = el('ul', 'instances-list'); list.setAttribute('aria-label', 'Running sessions');
  const timelineHeading = el('h3', 'instances-subheading', 'Recent hops'); timelineHeading.hidden = true;
  const timeline = el('ol', 'instances-timeline');
  element.append(warnings, list, timelineHeading, timeline);

  // Activity titles: a setting of this view, so a quiet switch rather than a form field.
  const settings = el('div', 'instances-options');
  const titleSwitch = el('div', 'instances-setting');
  const titleBox = el('input', 'instances-switch'); titleBox.type = 'checkbox'; titleBox.id = 'instances-titles'; titleBox.setAttribute('role', 'switch');
  const titleLabel = el('label', 'instances-setting-label', 'Activity titles'); titleLabel.htmlFor = titleBox.id;
  const titleHint = el('span', 'instances-setting-hint', 'Verbatim from the last tool call or message; nothing leaves this computer.'); titleHint.id = 'instances-titles-hint';
  titleBox.setAttribute('aria-describedby', titleHint.id);
  titleSwitch.append(titleBox, titleLabel, titleHint);
  titleBox.onchange = async () => {
    titleBox.disabled = true;
    await options.invoke('titles_set_enabled', { enabled: titleBox.checked }).catch(() => { titleBox.checked = !titleBox.checked; });
    titleBox.disabled = false; void load(true).catch(() => {});
  };
  const legend = el('p', 'instances-legend');
  const certain = el('span', 'instances-legend-item'); certain.append(el('i', 'instances-marker certain'), 'Certain, from the process');
  const inferred = el('span', 'instances-legend-item'); inferred.append(el('i', 'instances-marker'), 'Inferred from the terminal');
  legend.append(certain, inferred, el('span', 'instances-legend-note', 'States come only from what Shinatama observed. Running states are unsure until native agent hooks are connected; a terminal signal never approves anything.'));
  settings.append(titleSwitch, legend);

  let reading = false; let drawn = ''; let revision = 0; let rows: Row[] = []; let query = '';

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
      toggle.onclick = async () => { toggle.disabled = true; await options.invoke('thread_summaries_pause', { threadId: thread, paused: !paused }).catch(() => {}); void load(true).catch(() => {}); };
      actions.append(toggle);
    }
    const focus = el('button', '', 'Focus'); focus.type = 'button'; focus.dataset.key = `${row.id}:focus`;
    focus.setAttribute('aria-label', `Focus ${agentName(row.agent)} ${row.id.slice(-4)}`);
    focus.onclick = () => { if (options.focus(row.id)) options.focused(); else options.notice('This instance has no pane in this window.'); };
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

  function collision(entry: Collision, known: Row[]): HTMLLIElement {
    const name = (id: string) => { const row = known.find(candidate => candidate.id === id); return row ? `${agentName(row.agent)} (${id.slice(-4)})` : id; };
    const item = el('li'); const path = el('code', '', entry.path);
    if (entry.kind === 'file_collision') item.append(`Both ${entry.instances.map(name).join(' and ')} changed `, path);
    else item.append(path, " changed and no instance's own edits name it (possible collision)");
    return item;
  }
  /** Hides the rows the panel's search field does not match; answers how many stay. */
  function applyFilter(): number {
    const wanted = query.trim().toLocaleLowerCase(); let shown = 0;
    for (const item of list.querySelectorAll<HTMLLIElement>('.instances-item')) {
      const match = !wanted || (item.textContent ?? '').toLocaleLowerCase().includes(wanted);
      item.hidden = !match; if (match) shown++;
    }
    return shown;
  }

  /** Redrawn only when what it shows has changed: no flicker, and the focused button keeps focus.
   * Rejects with what the backend said when the overview cannot be read. */
  async function load(force = false): Promise<void> {
    // A refresh never overlaps a slow read (Git can take a while); an explicit reload does, and the newest wins.
    if (reading && !force) return;
    reading = true; const mine = ++revision;
    try {
      const next = await options.invoke<Row[]>('instances_overview');
      const titles = await options.invoke<Titles>('instance_titles', { requests: next.map(r => ({ instanceId: r.id, threadId: options.threadOf(r.id), state: r.state })) }).catch(() => null);
      const hops = await options.invoke<HopLine[]>('hops_recent').catch(() => [] as HopLine[]);
      if (mine !== revision) return;
      const signature = JSON.stringify([next, titles, hops, next.map(row => options.threadOf(row.id))]);
      if (signature === drawn) return;
      drawn = signature; rows = next;
      const focused = (document.activeElement as HTMLElement | null)?.dataset?.key;
      titleBox.checked = titles?.enabled ?? false;
      const collisions = titles?.collisions ?? [];
      warnings.hidden = !collisions.length;
      const warningList = el('ul'); warningList.append(...collisions.map(entry => collision(entry, next)));
      warnings.replaceChildren(el('h3', 'instances-warnings-title', collisions.length === 1 ? 'Possible conflict' : 'Possible conflicts'), warningList);
      list.replaceChildren(...next.map(row => instance(row, titles)));
      list.hidden = !next.length;
      timelineHeading.hidden = !hops.length;
      timeline.replaceChildren(...hops.map(hop => {
        const item = el('li'); const time = el('time', 'instances-hop-time', hopTime(hop.updatedAtMs)); time.dateTime = new Date(hop.updatedAtMs).toISOString();
        item.append(time, el('span', 'instances-hop-route', `${agentName(hop.sourceAgent)} → ${agentName(hop.destinationAgent)}`), el('span', 'instances-hop-state', `${hop.mode} · ${hop.state}`));
        return item;
      }));
      applyFilter();
      if (focused) element.querySelector<HTMLElement>(`[data-key="${CSS.escape(focused)}"]`)?.focus();
    } catch (error) {
      if (mine === revision) { drawn = ''; throw error; }
    } finally { if (mine === revision) reading = false; }
  }
  return {
    element, settings, load,
    rows: () => rows,
    filter(text: string) { query = text; return applyFilter(); },
    /** Forgets what was drawn, so the next load paints again. */
    reset() { drawn = ''; },
  };
}
export type InstancesSection = ReturnType<typeof createInstancesSection>;
