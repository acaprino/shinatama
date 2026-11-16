import { listen } from '@tauri-apps/api/event';
import './leftovers-panel.css';
import {
  ago, authorText, baseName, countText, size, terminalLabel, visibleCount, visibleItems,
  type LeftoverItem, type LeftoverStatus, type Leftovers,
} from './leftovers-model.ts';

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
type Options = {
  invoke: Invoke;
  /** Takes the person to a running agent terminal. */
  focusSession: (terminalId: string) => void;
  reportError: (message: string) => void;
};

const HIDDEN_KEY = 'shinatama.leftovers.hidden';
const TAGS: Record<string, string> = {
  worktrees: 'Worktree', copies: 'Copy', named: 'Named', scratch: 'Session dir', builds: 'Build', logs: 'Log',
};
const text = (error: unknown) => typeof error === 'string' ? error : error instanceof Error ? error.message : 'The action failed.';
const el = <K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, content?: string) => {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (content !== undefined) node.textContent = content;
  return node;
};
const button = (label: string, onclick: () => void, title?: string) => {
  const node = el('button', undefined, label); node.type = 'button'; node.onclick = onclick;
  if (title) node.title = title;
  return node;
};

/**
 * The side panel of what agent sessions create in their projects (Xiox's observer): every
 * worktree, copy, bench or backup folder and session folder, newest first, with who could have
 * made it. Read-only: it copies paths and commands and selects a folder in Explorer; it never
 * removes anything.
 */
export function createLeftoversPanel(options: Options) {
  const panel = el('aside', 'leftovers-sidebar'); panel.setAttribute('aria-label', 'Leftovers');
  const heading = el('div', 'leftovers-heading');
  const title = el('h2', undefined, 'Leftovers');
  const close = button('×', () => setHidden(true), 'Close the Leftovers panel');
  close.setAttribute('aria-label', 'Close the Leftovers panel');
  heading.append(title, close);
  const status = el('p', 'leftovers-status'); status.setAttribute('role', 'status');
  const list = el('div', 'leftovers-projects');
  panel.append(heading, status, list);

  const toggle = button('Leftovers', () => setHidden(!panel.hidden), 'What agent sessions created: worktrees, copies, bench and backup folders');
  toggle.id = 'leftovers-toggle';
  const badge = el('span', 'leftovers-badge'); badge.hidden = true;
  toggle.append(badge);

  let state: LeftoverStatus = { available: false, reason: null, projects: [] };
  let message = '';

  function setHidden(hidden: boolean) {
    panel.hidden = hidden;
    toggle.setAttribute('aria-pressed', String(!hidden));
    try { localStorage.setItem(HIDDEN_KEY, hidden ? '1' : '0'); } catch { /* a per-viewer convenience */ }
    if (!hidden) void refresh();
  }
  function say(line: string, error = false) { message = line; status.dataset.state = error ? 'error' : 'ready'; status.textContent = line; }

  function itemElement(project: Leftovers, item: LeftoverItem, now: number) {
    const row = el('li', 'leftovers-item');
    row.dataset.category = item.category; row.dataset.level = item.level;
    if (item.blocked.length) row.dataset.blocked = 'true';
    const top = el('div', 'leftovers-item-top');
    const where = item.path ?? item.label ?? item.key;
    const name = el('strong', undefined, item.path ? baseName(item.path) : where);
    const bytes = item.bytes === null ? '' : `${item.bytesPartial ? '≥ ' : ''}${size(item.bytes)}`;
    top.append(el('span', 'leftovers-tag', TAGS[item.category] ?? item.category), name, el('span', 'leftovers-size', bytes));
    const path = el('small', 'leftovers-path selectable', where); path.title = where;
    const why = el('small', 'leftovers-why', [item.reason, ago(item.firstSeenMs, now)].filter(Boolean).join(' · '));
    row.append(top, path, why);
    const suspects = project.suspects[item.key] ?? [];
    const author = authorText(item, suspects);
    if (author) {
      const by = el('small', 'leftovers-by', author);
      row.append(by);
      const open = item.sessions.length ? [] : suspects.filter(s => s.open);
      if (open.length) {
        const go = el('div', 'leftovers-actions');
        for (const suspect of open) go.append(button(`Go to ${terminalLabel(suspect)}`, () => options.focusSession(suspect.terminalId)));
        row.append(go);
      }
    }
    if (item.blocked.length) row.append(el('small', 'leftovers-blocked', `Cannot be removed: ${item.blocked.join(', ')}`));
    if (item.path) {
      const actions = el('div', 'leftovers-actions');
      const target = item.path;
      actions.append(
        button('Copy path', async () => {
          try { await navigator.clipboard.writeText(target); say(`Copied ${target}`); } catch { say('The clipboard is unavailable.', true); }
        }),
        button('Show in Explorer', async () => {
          try { await options.invoke('leftovers_reveal', { path: target }); } catch (error) { say(text(error), true); }
        }, 'Selects it in Explorer. Nothing is opened or removed.'),
      );
      row.append(actions);
    }
    return row;
  }

  function projectElement(project: Leftovers, now: number) {
    const section = el('section', 'leftovers-project');
    if (project.attention.length) section.dataset.attention = 'true';
    const head = el('div', 'leftovers-project-head');
    head.append(el('strong', undefined, project.name ?? baseName(project.folders[0] ?? '')));
    const live = project.liveSessions ? `${project.liveSessions} session${project.liveSessions === 1 ? '' : 's'} open` : 'no session open';
    const phase = project.phase === 'starting' ? 'first snapshot…' : project.phase === 'ended' ? `stopped${project.message ? `: ${project.message}` : ''}` : `checked ${ago(project.checkedAtMs ?? project.startedAtMs, now) || 'at start'}`;
    head.append(el('small', undefined, `${live} · ${phase}`));
    section.append(head);
    const items = visibleItems(project);
    const archived = project.newItems.length - items.length;
    const facts = [
      items.length ? countText(items) : 'nothing new',
      project.growthBytes ? `+${size(project.growthBytes)}` : '',
      archived ? `${archived} archived session${archived === 1 ? '' : 's'}` : '',
      project.newItemsOmitted ? `${project.newItemsOmitted} more not listed` : '',
      project.stale ? 'last check failed' : '',
    ].filter(Boolean).join(' · ');
    const summary = el('small', 'leftovers-summary', facts);
    section.append(summary);
    if (project.attention.length) {
      const words = project.attention.map(kind => kind === 'growth' ? 'grew over the limit' : kind === 'worktrees' ? 'too many new worktrees' : kind);
      section.append(el('small', 'leftovers-attention', `Attention: ${words.join(', ')}`));
    }
    if (items.length) {
      const ul = el('ul', 'leftovers-items');
      ul.append(...items.map(item => itemElement(project, item, now)));
      section.append(ul);
    }
    const actions = el('div', 'leftovers-actions');
    const folder = project.root ?? project.folders[0];
    if (folder) {
      actions.append(button(project.phase === 'ended' ? 'Restart Xiox' : 'Check now', async () => {
        try { await options.invoke('project_leftovers_check', { path: folder }); say(`Xiox is checking ${project.name ?? folder}…`); }
        catch (error) { say(text(error), true); }
      }));
    }
    if (project.cleanCommand) {
      const command = project.cleanCommand;
      actions.append(button('Copy clean command', async () => {
        try {
          await navigator.clipboard.writeText(command);
          say(`Clean command copied${project.cleanCommandTruncated ? ' (first 25 items)' : ''}. Paste it into a terminal: xiox clean lists what it removes and asks first.`);
        } catch { say('The clipboard is unavailable.', true); }
      }, 'Only the new recommended items. Copies and named folders need your own choice.'));
    }
    section.append(actions);
    return section;
  }

  function render() {
    const now = Date.now();
    const count = visibleCount(state.projects);
    badge.hidden = count === 0; badge.textContent = String(count);
    toggle.dataset.attention = String(state.projects.some(p => p.attention.length > 0));
    if (panel.hidden) return;
    if (!state.available) {
      list.replaceChildren();
      say(state.reason ? `Not watching: ${state.reason}` : 'Reading Xiox…');
      return;
    }
    if (!message || status.dataset.state !== 'error') {
      status.dataset.state = 'ready';
      status.textContent = state.projects.length
        ? `${count} new across ${state.projects.length} project${state.projects.length === 1 ? '' : 's'}. Nothing is removed.`
        : 'Nothing watched yet. Xiox starts with the first agent session of a project.';
    }
    const ordered = [...state.projects].sort((a, b) => visibleItems(b).length - visibleItems(a).length || b.liveSessions - a.liveSessions);
    list.replaceChildren(...ordered.map(project => projectElement(project, now)));
  }

  async function refresh() {
    try { state = await options.invoke<LeftoverStatus>('project_leftovers'); } catch (error) { if (!panel.hidden) say(text(error), true); }
    render();
  }

  let hidden = false;
  try { hidden = localStorage.getItem(HIDDEN_KEY) === '1'; } catch { /* shown by default */ }
  panel.hidden = hidden; toggle.setAttribute('aria-pressed', String(!hidden));
  void listen('project-leftovers', () => { void refresh(); }).catch(() => {});
  // "5 min ago" moves on, and an observer that started meanwhile shows up.
  window.setInterval(() => { if (!document.hidden) void refresh(); }, 30_000);
  void refresh();
  return { element: panel, toggle, refresh };
}
