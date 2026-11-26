// The dialogs of the window, as data: what the one that is open is, and what it was opened for. A dialog holds its own form; the
// controller (`UiController.dialog`) only says which one is open. The dialogs agent owns this file and adds a kind for each dialog
// it draws; a screen opens one with `app.ui.open({ kind: ... })`.
import type { NewSessionScope } from '../../session-project.ts';
import type { WorksetScope } from '../../api/threads.ts';

export type DialogSpec =
  /** A new session: the harness menu, or the folder dialog when the folder is not known. `groupId`: the pane it opens in. */
  | { kind: 'new-session'; scope: NewSessionScope; groupId: string | null; path?: string; harness?: string; workset: WorksetScope | null }
  | { kind: 'new-project' }
  /** The projects of the window: the ones recent, the folders that could be projects, the roots that are scanned. */
  | { kind: 'project-picker'; filter?: boolean; browse?: boolean }
  | { kind: 'projects-overview' }
  /** Asks before what cannot be taken back. The confirming button carries the verb and the object; the description names the consequence. */
  | { kind: 'confirm'; title: string; description: string; confirm: string; danger: boolean; settle: (accepted: boolean) => void };
