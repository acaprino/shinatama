// The command table: every key of the window that is Shinatama's own, as data. `screens/commands.tsx` declares each row with Sigrid's
// `useCommand`; Sigrid's registry owns the binding, the key caps, the palette and the foot's key list. Pure, so a test reads the table
// with no window.
//
// Not here: the views of the title bar (the shell's «Go to» commands), Ctrl+K (the palette), Ctrl+F (the search of the view) and F6 (the
// next region), which are the shell's; light, dark and the system's look (the palette's); the keys of a terminal, which are the program's.

export interface CommandSpec { id: string; title: string; keys?: string }

export const COMMANDS: readonly CommandSpec[] = [
  { id: 'new-session', title: 'New session', keys: 'Ctrl+Shift+T' },
  { id: 'pane-left', title: 'Focus the pane on the left', keys: 'Ctrl+Shift+ArrowLeft' },
  { id: 'pane-right', title: 'Focus the pane on the right', keys: 'Ctrl+Shift+ArrowRight' },
  { id: 'find-in-terminal', title: 'Find in terminal', keys: 'Ctrl+Shift+F' },
  { id: 'open-folder', title: 'Open a folder as a project', keys: 'F3' },
  { id: 'choose-project', title: 'Choose the project' },
  { id: 'new-project', title: 'New project' },
  { id: 'projects', title: 'Projects at a glance' },
  { id: 'team-map', title: 'Team map of the thread' },
  { id: 'thread-chat', title: 'Conversation of the thread' },
  { id: 'thread-settings', title: 'Settings of the thread' },
  { id: 'new-thread', title: 'New thread in the current project' },
  { id: 'maximize-pane', title: 'Maximize the pane, or give the room back' },
  { id: 'files', title: 'Project files' },
  { id: 'leftovers', title: 'What agents left on disk' },
  { id: 'instances', title: 'Shinatama windows and their agents' },
  { id: 'swap-axis', title: 'Panes side by side, or one under the other' },
];

/** The commands the foot names in the Workspace view, before the shell's own keys (Ctrl+F, Ctrl+K, F6): ids of the registry. */
export const FOOTER: readonly string[] = ['new-session', 'pane-left', 'pane-right'];

/** The parts of a keyboard event that name a chord (`KeyboardEvent` has them, and so has a fake one in a test). */
export interface KeyLike { type?: string; key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }

const parsed = COMMANDS.filter(spec => spec.keys).map(spec => {
  const parts = spec.keys!.split('+');
  const key = parts.at(-1)!;
  return { id: spec.id, key: key.toLowerCase(), ctrl: parts.includes('Ctrl'), shift: parts.includes('Shift'), alt: parts.includes('Alt') };
});

/** The command a key press is the chord of, if it is one of the table's (a letter in either case; Ctrl is Cmd on a Mac). */
export function commandOfKey(event: KeyLike): string | null {
  const key = event.key.toLowerCase();
  const found = parsed.find(one => one.key === key && one.ctrl === (event.ctrlKey || event.metaKey) && one.shift === event.shiftKey && one.alt === event.altKey);
  return found?.id ?? null;
}

/** The keys a terminal leaves to the window instead of sending them to the program: the chords of the table, F6 and Shift+F6 (the next
 *  region, so the keyboard is never trapped in a terminal) and the zoom (Ctrl with +, - or 0). Every other key is the program's. */
export function terminalLeavesKey(event: KeyLike): boolean {
  if (commandOfKey(event) !== null || event.key === 'F6') return true;
  return (event.ctrlKey || event.metaKey) && !event.altKey && ['+', '=', '-', '_', '0'].includes(event.key);
}
