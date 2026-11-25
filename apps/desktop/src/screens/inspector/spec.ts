// The panels of the inspector, as data. The inspector is one stack: a panel opened from inside another covers it and gives it back
// (`UiController.closePanel`). The inspector agent owns this file and adds a kind for each panel it draws; a screen opens one with
// `app.ui.openPanel({ kind: ... })`.
export type PanelSpec =
  /** The details of one saved or running session, by the store that holds it and the id its store gave it. */
  | { kind: 'session'; root: string; nativeId: string; at?: { offset: number; fingerprint: string } }
  /** The project files: a tree and the preview of the file chosen (Source, Preview). */
  | { kind: 'files' }
  /** The archives of sessions the person added, and the index of search. */
  | { kind: 'archives' }
  /** What agent sessions left on disk. */
  | { kind: 'leftovers' }
  /** The Shinatama windows and their agents that run (instances). */
  | { kind: 'instances' }
  /** The settings of one thread: its name, its room, its team. */
  | { kind: 'thread'; threadId: string }
  /** The preview of a hop (the exact seed that will be pasted into the destination). */
  | { kind: 'hop'; selection: { root: string; nativeId: string } };

export type PanelKind = PanelSpec['kind'];
