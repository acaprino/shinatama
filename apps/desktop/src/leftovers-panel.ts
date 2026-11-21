import { uiPart, uiButton, uiTraits } from '@acaprino/imago/ui';
import { listen } from '@tauri-apps/api/event';
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

/**
 * The side panel of what agent sessions create in their projects (Xiox's observer): every
 * worktree, copy, bench or backup folder and session folder, newest first, with who could have
 * made it. Read-only: it copies paths and commands and selects a folder in Explorer; it never
 * removes anything.
 */
export function createLeftoversPanel(options: Options) {
  const panel = uiTraits(uiPart('aside', undefined, 'leftovers-sidebar'), 'inspectionRail'); panel.setAttribute('aria-label', 'Leftovers');
  const heading = uiPart('header');
  const title = uiPart('heading', 'Leftovers');
  const close = uiButton('×', { onClick: () => setHidden(true), title: 'Close the Leftovers panel' });
  close.setAttribute('aria-label', 'Close the Leftovers panel');
  heading.append(title, close);
  const status = uiPart('status');
  const list = uiTraits(uiPart('stack'), 'scroll');
  panel.append(heading, status, list);

  const toggle = uiButton('Leftovers', { onClick: () => setHidden(!panel.hidden), title: 'What agent sessions created: worktrees, copies, bench and backup folders' });
  toggle.id = 'leftovers-toggle';
  const badge = uiPart('text'); badge.hidden = true;
  toggle.append(badge);

  let state: LeftoverStatus = { available: false, reason: null, projects: [] };
  let message = '';

  // Whether the viewer opened or closed the rail themselves; until then Xiox decides (refresh).
  let chosen = false;
  function show(visible: boolean) { panel.hidden = !visible; toggle.setAttribute('aria-pressed', String(visible)); }
  function setHidden(hidden: boolean) {
    chosen = true;
    show(!hidden);
    try { localStorage.setItem(HIDDEN_KEY, hidden ? '1' : '0'); } catch { /* a per-viewer convenience */ }
    if (!hidden) void refresh();
  }
  function say(line: string, error = false) { message = line; status.dataset.state = error ? 'error' : 'ready'; status.textContent = line; }

  function itemElement(project: Leftovers, item: LeftoverItem, now: number) {
    const row = uiPart('item');
    row.dataset.category = item.category; row.dataset.level = item.level;
    if (item.blocked.length) row.dataset.blocked = 'true';
    const top = uiPart('row');
    const where = item.path ?? item.label ?? item.key;
    const name = uiPart('strong', item.path ? baseName(item.path) : where);
    const bytes = item.bytes === null ? '' : `${item.bytesPartial ? '≥ ' : ''}${size(item.bytes)}`;
    top.append(uiPart('text', TAGS[item.category] ?? item.category), name, uiPart('text', bytes));
    const path = uiPart('hint', where); path.title = where;
    const why = uiPart('hint', [item.reason, ago(item.firstSeenMs, now)].filter(Boolean).join(' · '));
    row.append(top, path, why);
    const suspects = project.suspects[item.key] ?? [];
    const author = authorText(item, suspects);
    if (author) {
      const by = uiPart('hint', author);
      row.append(by);
      const open = item.sessions.length ? [] : suspects.filter(s => s.open);
      if (open.length) {
        const go = uiPart('actions');
        for (const suspect of open) go.append(uiButton(`Go to ${terminalLabel(suspect)}`, { onClick: () => options.focusSession(suspect.terminalId) }));
        row.append(go);
      }
    }
    if (item.blocked.length) row.append(uiPart('hint', `Cannot be removed: ${item.blocked.join(', ')}`));
    if (item.path) {
      const actions = uiPart('actions');
      const target = item.path;
      actions.append(
        uiButton('Copy path', { async onClick() {
          try { await navigator.clipboard.writeText(target); say(`Copied ${target}`); } catch { say('The clipboard is unavailable.', true); }
        } }),
        uiButton('Show in Explorer', { title: 'Selects it in Explorer. Nothing is opened or removed.', async onClick() {
          try { await options.invoke('leftovers_reveal', { path: target }); } catch (error) { say(text(error), true); }
        } }),
      );
      row.append(actions);
    }
    return row;
  }

  function projectElement(project: Leftovers, now: number) {
    const section = uiPart('region');
    if (project.attention.length) section.dataset.attention = 'true';
    const head = uiPart('header');
    head.append(uiPart('strong', project.name ?? baseName(project.folders[0] ?? '')));
    const live = project.liveSessions ? `${project.liveSessions} session${project.liveSessions === 1 ? '' : 's'} open` : 'no session open';
    const phase = project.phase === 'starting' ? 'first snapshot…' : project.phase === 'ended' ? `stopped${project.message ? `: ${project.message}` : ''}` : `checked ${ago(project.checkedAtMs ?? project.startedAtMs, now) || 'at start'}`;
    head.append(uiPart('hint', `${live} · ${phase}`));
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
    const summary = uiPart('hint', facts);
    section.append(summary);
    if (project.attention.length) {
      const words = project.attention.map(kind => kind === 'growth' ? 'grew over the limit' : kind === 'worktrees' ? 'too many new worktrees' : kind);
      section.append(uiPart('hint', `Attention: ${words.join(', ')}`));
    }
    if (items.length) {
      const ul = uiPart('list');
      ul.append(...items.map(item => itemElement(project, item, now)));
      section.append(ul);
    }
    const actions = uiPart('actions');
    const folder = project.root ?? project.folders[0];
    if (folder) {
      actions.append(uiButton(project.phase === 'ended' ? 'Restart Xiox' : 'Check now', { async onClick() {
        try { await options.invoke('project_leftovers_check', { path: folder }); say(`Xiox is checking ${project.name ?? folder}…`); }
        catch (error) { say(text(error), true); }
      } }));
    }
    if (project.cleanCommand) {
      const command = project.cleanCommand;
      actions.append(uiButton('Copy clean command', { title: 'Only the new recommended items. Copies and named folders need your own choice.', async onClick() {
        try {
          await navigator.clipboard.writeText(command);
          say(`Clean command copied${project.cleanCommandTruncated ? ' (first 25 items)' : ''}. Paste it into a terminal: xiox clean lists what it removes and asks first.`);
        } catch { say('The clipboard is unavailable.', true); }
      } }));
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
    // Unless the viewer chose, the rail opens only when Xiox watches: a rail opened at every start
    // to say xiox.exe is missing took the workspace's room (on a narrow window, over its tabs).
    if (!chosen && panel.hidden === state.available) show(state.available);
    render();
  }

  try {
    const stored = localStorage.getItem(HIDDEN_KEY);
    chosen = stored !== null;
    show(stored === '0');
  } catch { show(false); }
  void listen('project-leftovers', () => { void refresh(); }).catch(() => {});
  // "5 min ago" moves on, and an observer that started meanwhile shows up.
  window.setInterval(() => { if (!document.hidden) void refresh(); }, 30_000);
  void refresh();
  return { element: panel, toggle, refresh };
}
