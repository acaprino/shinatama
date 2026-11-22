import { uiPart, uiButton, uiTraits } from '@acaprino/imago/ui';

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
  const element = uiPart('stack');
  const warnings = uiPart('stack'); warnings.setAttribute('role', 'group'); warnings.setAttribute('aria-label', 'Possible conflicts'); warnings.hidden = true;
  const list = uiPart('list'); list.classList.add('instances-list'); list.setAttribute('aria-label', 'Running sessions');
  const timelineHeading = uiPart('subheading', 'Recent hops'); timelineHeading.hidden = true;
  const timeline = uiTraits(uiPart('orderedList'), 'scroll', 'bounded'); timeline.classList.add('instances-timeline');
  element.append(warnings, list, timelineHeading, timeline);

  const legend = uiPart('paragraph'); legend.classList.add('instances-legend');
  const certain = uiPart('text'); certain.append('● Certain, from the process');
  const inferred = uiPart('text'); inferred.append('○ Inferred from the terminal');
  legend.append(certain, inferred, uiPart('text', 'States come only from what Shinatama observed. Running states are unsure until native agent hooks are connected; a terminal signal never approves anything.'));
  // Activity titles are switched on and off in Settings (settings-panel.ts), with every other preference.

  let reading = false; let drawn = ''; let revision = 0; let rows: Row[] = []; let query = '';

  function instance(row: Row, titles: Titles | null): HTMLLIElement {
    const item = uiPart('item'); item.classList.add('instances-item'); item.dataset.instanceId = row.id; item.dataset.state = row.state;
    const marker = uiPart('text', row.certain ? '●' : '○'); marker.classList.add('instances-marker'); marker.setAttribute('aria-hidden', 'true');
    const head = uiPart('row'); head.classList.add('instances-head');
    const name = uiPart('text', agentName(row.agent)); name.classList.add('instances-agent');
    const tag = uiPart('text', row.id.slice(-4)); tag.classList.add('instances-id'); tag.title = row.id;
    const actions = uiPart('actions'); actions.classList.add('instances-actions');
    const thread = options.threadOf(row.id);
    if (thread) {
      const paused = titles?.excluded.includes(thread) ?? false;
      const toggle = uiButton(paused ? 'Resume summaries' : 'Pause summaries'); toggle.dataset.key = `${row.id}:summaries`;
      toggle.onclick = async () => { toggle.disabled = true; await options.invoke('thread_summaries_pause', { threadId: thread, paused: !paused }).catch(() => {}); void load(true).catch(() => {}); };
      actions.append(toggle);
    }
    const focus = uiButton('Focus'); focus.dataset.key = `${row.id}:focus`;
    focus.setAttribute('aria-label', `Focus ${agentName(row.agent)} ${row.id.slice(-4)}`);
    focus.onclick = () => { if (options.focus(row.id)) options.focused(); else options.notice('This instance has no pane in this window.'); };
    actions.append(focus);
    head.append(name, tag, actions);
    const state = uiPart('paragraph'); state.classList.add('instances-state');
    const stateName = uiPart('text', STATES[row.state] ?? row.state); stateName.classList.add('instances-state-name');
    state.append(stateName, uiPart('text', ` · ${SOURCES[row.stateSource] ?? row.stateSource}`));
    const facts = uiPart('descriptionList'); facts.classList.add('instances-facts');
    const fact = (label: string, value: string, className = '') => {
      const term = uiPart('term', label); const detail = uiPart('definition', value); if (className) detail.classList.add(className); detail.title = value; facts.append(term, detail);
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
    const item = uiPart('item'); const path = uiPart('code', entry.path);
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
      const collisions = titles?.collisions ?? [];
      warnings.hidden = !collisions.length;
      const warningList = uiPart('list'); warningList.append(...collisions.map(entry => collision(entry, next)));
      warnings.replaceChildren(uiPart('subheading', collisions.length === 1 ? 'Possible conflict' : 'Possible conflicts'), warningList);
      list.replaceChildren(...next.map(row => instance(row, titles)));
      list.hidden = !next.length;
      timelineHeading.hidden = !hops.length;
      timeline.replaceChildren(...hops.map(hop => {
        const item = uiPart('item'); const time = uiPart('time', hopTime(hop.updatedAtMs)); time.dateTime = new Date(hop.updatedAtMs).toISOString();
        item.append(time, uiPart('text', `${agentName(hop.sourceAgent)} → ${agentName(hop.destinationAgent)}`), uiPart('text', `${hop.mode} · ${hop.state}`));
        return item;
      }));
      applyFilter();
      if (focused) element.querySelector<HTMLElement>(`[data-key="${CSS.escape(focused)}"]`)?.focus();
    } catch (error) {
      if (mine === revision) { drawn = ''; throw error; }
    } finally { if (mine === revision) reading = false; }
  }
  return {
    element, legend, load,
    rows: () => rows,
    filter(text: string) { query = text; return applyFilter(); },
    /** Forgets what was drawn, so the next load paints again. */
    reset() { drawn = ''; },
  };
}
export type InstancesSection = ReturnType<typeof createInstancesSection>;
