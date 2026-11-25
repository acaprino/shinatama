// The commands of the window that are Shinatama's own, declared with Sigrid's `useCommand` from the table of src/keys.ts: the palette
// (Ctrl+K) lists them with the shell's, their keys run from anywhere in the window but a field, and the foot names the ones that work now.
// A key that types belongs to a field, and an open dialog owns the keyboard: Sigrid keeps both (`useCommand`), and `when` keeps the rest.
import { useCommand } from '@acaprino/sigrid/assemblies';
import type { Shinatama } from '../app.ts';
import { COMMANDS } from '../keys.ts';

export function runners(app: Shinatama): Record<string, () => void> {
  const { ui, layout, opens } = app;
  const toward = (direction: 'left' | 'right') => () => {
    ui.leaveToWorkspace();
    const area = document.querySelector('[data-slot="workspace"]');
    const viewport = { width: area?.clientWidth ?? 1200, height: area?.clientHeight ?? 800 };
    const neighbor = layout.neighbor(layout.focusedGroup(), direction, viewport);
    if (neighbor) layout.focus(neighbor);
  };
  return {
    'new-session': () => { void opens.newSession(); },
    'pane-left': toward('left'),
    'pane-right': toward('right'),
    'open-folder': () => ui.open({ kind: 'project-picker', browse: true }),
    files: () => app.files.open(),
    leftovers: () => ui.openPanel({ kind: 'leftovers' }),
    instances: () => ui.openPanel({ kind: 'instances' }),
    'swap-axis': () => layout.swapAxis(),
  };
}

/** Declares them all. Called once by the window. */
export function useShinatamaCommands(app: Shinatama): void {
  const run = runners(app);
  const free = app.ui.dialog === null;
  for (const spec of COMMANDS) {
    // The table is a constant: the hooks are always called in the same order.
    useCommand({ id: spec.id, label: spec.title, group: 'Shinatama', keys: spec.keys, when: free, run: run[spec.id] ?? (() => {}) });
  }
}
    'find-in-terminal': () => app.terminals.openFind(),
