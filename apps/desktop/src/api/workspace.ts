// The saved layout of the window (layout only: no process is restored from it), the window's close, and the page's own reload.
import { invoke } from './core.ts';

export const workspaceApi = {
  // The load, save, release and recover of the layout are `global_workspace_<operation>`: `workspace-persistence.ts` calls them
  // through the transport that `LayoutController` gives it (one layout for the window, scope `main`).
  /** The window may close although the layout was not saved: the person said so. */
  allowClose: () => invoke<void>('workspace_allow_close'),
  /** The page saved what it held: the shell may reload it (reload_gate.rs). */
  reloadReady: (token: number) => invoke<void>('interface_reload_ready', { token }),
  relaunch: () => invoke<void>('app_relaunch'),
};
