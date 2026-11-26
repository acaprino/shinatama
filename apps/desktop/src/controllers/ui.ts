// What the window itself remembers: the view that is shown, the dialog that is open, the stack of panels of the inspector, the
// notices, the line of the status strip. No process, no layout, no data of the person's work: those are the other controllers'.
import type { DialogSpec } from '../screens/dialogs/spec.ts';
import { covers, type PanelSpec } from '../screens/inspector/spec.ts';
import { Store } from '../store.ts';

export type ViewId = 'workspace' | 'sessions' | 'settings';
export const VIEW_IDS: readonly ViewId[] = ['workspace', 'sessions', 'settings'];

/** What a notice may carry besides its words: an error stays until it is closed, and Undo takes the action back. */
export interface Say { error?: boolean; undo?: () => void | Promise<void> }

/** What the confirm dialog says. */
export interface Ask { title: string; description: string; confirm: string; danger?: boolean }

export class UiController extends Store {
  view: ViewId = 'workspace';
  /** The one dialog that is open, or none. */
  dialog: DialogSpec | null = null;
  /** The inspector: a stack of panels, the last one shown. */
  panels: PanelSpec[] = [];
  /** The runtime, in the status strip: what is running, and whether the storage is read-only. */
  runtime = { text: 'Starting…', title: '', readOnly: false, failed: false };
  /** Says something in a notice (the window sets it: the shell's `useNotify`). */
  notice: (text: string, say?: Say) => void = () => {};
  /** Asks the shell to give the content the focus (the window sets it: `focusContent`). */
  focusContent: () => void = () => {};
  private after: (() => void) | null = null;

  // --- the view ------------------------------------------------------------------------------------

  setView(view: ViewId): void {
    if (this.view === view) return;
    this.view = view;
    // The inspector holds what the view it was opened in showed: another view starts without it.
    this.panels = [];
    this.changed();
  }

  /** Back to the Workspace when Settings or Sessions is the view: a pane the person asks for must be on screen. Only what the
   *  person does calls it; an agent's page, an exit or a restart never takes them out of Settings. */
  leaveToWorkspace(): void {
    this.setView('workspace');
  }

  // --- the notices -----------------------------------------------------------------------------------

  /** The result of something the person did, or of something that failed: a notice. An error stays until it is closed. */
  report(text: string, say?: Say): void {
    this.notice(text, say);
  }

  info(text: string): void {
    this.notice(text);
  }

  error(error: unknown): void {
    this.notice(typeof error === 'string' ? error : error instanceof Error ? error.message : JSON.stringify(error ?? ''), { error: true });
  }

  setRuntime(next: Partial<UiController['runtime']>): void {
    this.runtime = { ...this.runtime, ...next };
    this.changed();
  }

  // --- the dialog --------------------------------------------------------------------------------------

  open(dialog: DialogSpec): void {
    this.dialog = dialog;
    this.changed();
  }

  /** Closes the dialog that is open; a confirm that is closed unanswered is a no. */
  closeDialog(): void {
    const closing = this.dialog;
    if (closing === null) return;
    this.dialog = null;
    this.changed();
    if (closing.kind === 'confirm') closing.settle(false);
    this.runAfterDialog();
  }

  /** Runs `then` once no dialog is open: at once when none is. What asks for the page's attention while a dialog has it (an alert
   *  that names a session) waits for the person to answer first; the last one asked is the one that runs. */
  afterDialog(then: () => void): void {
    if (this.dialog === null) { then(); return; }
    this.after = then;
  }

  private runAfterDialog(): void {
    const then = this.after;
    this.after = null;
    then?.();
  }

  /** Whether a dialog takes the page: the terminals and the keys of the window leave it alone meanwhile. */
  modalOpen(): boolean {
    return this.dialog !== null;
  }

  /** Asks before something that cannot be taken back. False when the person says no, and when another question is already open
   *  (a window close can arrive over another dialog: it does not stack). */
  ask(question: Ask): Promise<boolean> {
    if (this.dialog?.kind === 'confirm') return Promise.resolve(false);
    return new Promise<boolean>(resolve => {
      this.open({
        kind: 'confirm', title: question.title, description: question.description, confirm: question.confirm, danger: question.danger ?? true,
        settle: accepted => resolve(accepted),
      });
    });
  }

  /** The confirm dialog was answered: closes it and tells the one who asked. */
  answer(accepted: boolean): void {
    const dialog = this.dialog;
    if (dialog?.kind !== 'confirm') return;
    this.dialog = null;
    this.changed();
    dialog.settle(accepted);
    this.runAfterDialog();
  }

  // --- the inspector -------------------------------------------------------------------------------------

  /** The panel shown now, if any. */
  get panel(): PanelSpec | null {
    return this.panels.at(-1) ?? null;
  }

  /** Shows a panel. One opened from inside another (a session's conversation from its facts, a search in it) covers it and gives it back;
   *  one of the same kind that is already on top is replaced (another file, another session); any other takes the inspector for itself. */
  openPanel(panel: PanelSpec): void {
    const top = this.panels.at(-1);
    this.panels = top?.kind === panel.kind ? [...this.panels.slice(0, -1), panel]
      : covers(panel.kind, top?.kind) ? [...this.panels, panel] : [panel];
    this.changed();
  }

  /** Gives back the panel that the one on top covered, or closes the inspector. */
  closePanel(): void {
    if (!this.panels.length) return;
    this.panels = this.panels.slice(0, -1);
    this.changed();
  }

  closePanels(): void {
    if (!this.panels.length) return;
    this.panels = [];
    this.changed();
  }
}
