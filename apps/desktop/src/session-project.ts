/** Which control asked for a new session:
 * - `thread`: a pane's + (tab bar, empty pane, + Session): the session joins the open thread, so it
 *   starts in that thread's project;
 * - `project`: the toolbar's New session beside the project picker (and its shortcut): the session
 *   starts in the project shown there, in a thread of its own when the open thread belongs to
 *   another project. */
export type NewSessionScope = 'thread' | 'project';

/** The folder a new session starts in. `explicit` (Other folder…, a restart, a project's own
 * New session) is used as given; otherwise the control's own project comes first, then the others. */
export function newSessionProject(scope: NewSessionScope, options: {
  explicit?: string; threadOpen?: boolean; openThread?: string; sidebar?: string; selected?: string; current: string;
}): string | null {
  const { explicit, openThread, sidebar, selected, current } = options;
  // A pane's + belongs to the open thread: when that thread's folder is unknown it starts nowhere
  // rather than in another project, which would also leave the thread.
  if (scope === 'thread' && !explicit && options.threadOpen && !openThread?.trim()) return null;
  const pick = scope === 'thread'
    ? [explicit, openThread, sidebar, selected, current]
    : [explicit, current, selected, sidebar, openThread];
  return pick.find((path): path is string => typeof path === 'string' && path.trim() !== '') ?? null;
}
