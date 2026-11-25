// The panels of the inspector, as data. The inspector is one stack: a panel opened from inside another covers it and gives it back
// (`UiController.closePanel`, the back button of the inspector). A screen opens one with `app.ui.openPanel({ kind: ... })`.
import type { DetailCursor, RawSelection } from '../../session-detail-model.ts';
import type { ExportSelection, HopSelection } from '../../session-export-model.ts';
import type { SessionRootSelector } from '../../session-library-model.ts';

export type PanelSpec =
  /** The conversation of one saved session (a page of its events, its overview, its lineage), by the store that holds it and the id its
   *  store gave it; `at` opens the page of one event (a match of a search in it). */
  | { kind: 'session'; root: SessionRootSelector; nativeId: string; at?: DetailCursor }
  /** The facts of one saved session, from the list the Sessions view shows: what it is, and the way to its conversation or a hop. */
  | { kind: 'summary'; root: SessionRootSelector; nativeId: string }
  /** A search inside one session the detail verified (`DetailRequest` of the first page). */
  | { kind: 'find'; root: SessionRootSelector; nativeId: string; fingerprint: string }
  /** One raw entry of a session, behind a warning that must be checked. */
  | { kind: 'raw'; selection: RawSelection }
  /** Exporting one session to a file. */
  | { kind: 'export'; selection: ExportSelection }
  /** The project files: a tree and the preview of the file chosen (Source, Preview). */
  | { kind: 'files' }
  /** The archives of sessions the person added, and the pages of one. */
  | { kind: 'archives' }
  /** What agent sessions left on disk. */
  | { kind: 'leftovers' }
  /** The Shinatama windows and their agents that run (instances). */
  | { kind: 'instances' }
  /** The index of session text that the search reads. */
  | { kind: 'search-index' }
  /** The settings of one thread, by the id of the thread (its workset): its name, its room, its sessions. */
  | { kind: 'thread'; threadId: string }
  /** The preview of a hop (the exact seed that will be pasted into the destination). */
  | { kind: 'hop'; selection: HopSelection };

export type PanelKind = PanelSpec['kind'];

/** The title a panel has in the inspector's head. */
export const PANEL_TITLES: Record<PanelKind, string> = {
  session: 'Session detail', summary: 'Session', find: 'Find in this session', raw: 'RAW native entry', export: 'Export session',
  files: 'Project files', archives: 'Archive browser', leftovers: 'Leftovers', instances: 'Running sessions', 'search-index': 'Search index',
  thread: 'Thread settings', hop: 'Hop to another agent',
};

/** The panels a panel can be opened from inside of: it then covers that one, which is given back by the way back. Any other panel opened
 *  while one is shown (a button of the toolbar, a command of the palette) takes the inspector for itself. */
const OPENED_FROM: Partial<Record<PanelKind, readonly PanelKind[]>> = {
  session: ['summary', 'archives', 'find', 'session'],
  find: ['session'],
  raw: ['session'],
  export: ['session'],
  hop: ['session', 'summary'],
};

/** Whether `next` covers `top` (it was opened from inside it) rather than replacing it. */
export function covers(next: PanelKind, top: PanelKind | undefined): boolean {
  return top !== undefined && (OPENED_FROM[next]?.includes(top) ?? false);
}
