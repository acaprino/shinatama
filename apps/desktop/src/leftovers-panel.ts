import { LOADING_DELAY_MS, setPanelState, showAfter, uiPart, uiButton, uiTraits } from '@acaprino/imago/ui';
import { closeSidePanel, openSidePanel, sidePanelShows, sideView } from './side-panel.ts';
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
  // A view of the window's side panel, beside the panes.
  const view = sideView('leftovers-panel', 'Leftovers', 'Close the Leftovers panel');
  const panel = view.root; panel.classList.add('leftovers-sidebar');
  view.close.onclick = () => setHidden(true);
  const status = uiPart('status');
  const list = uiPart('stack');
  view.body.append(status, list);
  const shown = () => sidePanelShows(panel);

  const toggle = uiButton('Leftovers', { onClick: () => setHidden(shown()), title: 'What agent sessions created: worktrees, copies, bench and backup folders' });
  toggle.id = 'leftovers-toggle';
  const badge = uiPart('text'); badge.hidden = true;
  toggle.append(badge);

  let state: LeftoverStatus = { available: false, reason: null, projects: [] };
  let message = '';
  // The first read, and a read that failed with nothing to show: the panel's own states (a failed read of a list already
  // on screen stays a status line, as does the result of an action).
  let loaded = false; let readFailure: string | null = null; let cancelLoading: (() => void) | null = null;

  // Whether the viewer opened or closed it themselves; until then Xiox decides, once, at the first read (refresh).
  let chosen = false; let decided = false;
  function show(visible: boolean) {
    if (visible) openSidePanel(panel, { opener: toggle, onLeave: () => toggle.setAttribute('aria-pressed', 'false') }); else closeSidePanel(panel);
    toggle.setAttribute('aria-pressed', String(visible));
  }
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
    if (!shown()) return;
    if (!loaded) { waitForFirstRead(); return; }
    if (!state.available) {
      list.replaceChildren();
      if (readFailure !== null) setPanelState(panel, { kind: 'error', text: readFailure, retry: { label: 'Retry', run: () => { void refresh(); } } });
      else setPanelState(panel, { kind: 'empty', title: 'Not watching', text: state.reason ?? 'Xiox is not running.' });
      return;
    }
    if (!state.projects.length) {
      list.replaceChildren();
      setPanelState(panel, { kind: 'empty', title: 'Nothing watched yet', text: 'Xiox starts with the first agent session of a project.' });
      return;
    }
    setPanelState(panel, { kind: 'ready' });
    if (!message || status.dataset.state !== 'error') {
      status.dataset.state = 'ready';
      status.textContent = `${count} new across ${state.projects.length} project${state.projects.length === 1 ? '' : 's'}. Nothing is removed.`;
    }
    const ordered = [...state.projects].sort((a, b) => visibleItems(b).length - visibleItems(a).length || b.liveSessions - a.liveSessions);
    list.replaceChildren(...ordered.map(project => projectElement(project, now)));
  }

  /** Nothing is drawn before 300 ms; then the bars stand in for the rows. */
  function waitForFirstRead() {
    if (loaded || !shown()) return;
    cancelLoading ??= showAfter(LOADING_DELAY_MS, () => { cancelLoading = null; if (!loaded && shown()) setPanelState(panel, { kind: 'loading', rows: 3 }); });
  }
  async function refresh() {
    waitForFirstRead();
    try { state = await options.invoke<LeftoverStatus>('project_leftovers'); readFailure = null; }
    catch (error) { readFailure = text(error); if (shown() && state.available) say(readFailure, true); }
    loaded = true; cancelLoading?.(); cancelLoading = null;
    // Unless the viewer chose, it opens at the first read only, and only when Xiox watches: the side panel is the
    // window's one, and a read every 30 s must not take it from the conversation or the Model proxy.
    if (!chosen && !decided) { decided = true; if (state.available) show(true); }
    render();
  }

  try {
    const stored = localStorage.getItem(HIDDEN_KEY);
    chosen = stored !== null;
    if (stored === '0') show(true); else toggle.setAttribute('aria-pressed', 'false');
  } catch { toggle.setAttribute('aria-pressed', 'false'); }
  void listen('project-leftovers', () => { void refresh(); }).catch(() => {});
  // "5 min ago" moves on, and an observer that started meanwhile shows up.
  window.setInterval(() => { if (!document.hidden) void refresh(); }, 30_000);
  void refresh();
  return { element: panel, toggle, refresh };
}
