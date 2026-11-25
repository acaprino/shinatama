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
  { id: 'open-folder', title: 'Open a folder as a project', keys: 'F3' },
  { id: 'files', title: 'Project files' },
  { id: 'leftovers', title: 'What agents left on disk' },
  { id: 'instances', title: 'Shinatama windows and their agents' },
  { id: 'swap-axis', title: 'Panes side by side, or one under the other' },
];

/** The commands the foot names in the Workspace view, before the shell's own keys (Ctrl+F, Ctrl+K, F6): ids of the registry. */
export const FOOTER: readonly string[] = ['new-session', 'pane-left', 'pane-right'];
