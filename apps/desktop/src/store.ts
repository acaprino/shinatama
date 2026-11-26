// THE STORE OF SHINATAMA: what the page remembers, and how the screens read it and ask it for changes.
//
// The contract (every screen and every controller follows it; docs/desktop-ui.md says it again, shorter):
//
// 1. What the page remembers lives in CONTROLLERS (`src/controllers/<name>.ts`): plain classes with no React and no DOM in them,
//    so `npm test` runs them with node. A controller extends `Store`, holds its state in public fields, changes them only in its
//    own methods, and calls `this.changed()` after each change that a screen should draw. Nothing else writes its fields.
// 2. `Shinatama` (src/app.ts) holds the controllers, one field each (`app.ui`, `app.layout`, `app.terminals`, `app.threads`...),
//    and is the one place that wires them to each other (a controller is given the `Shinatama` and reaches the others through it,
//    never by importing them). There is one `Shinatama` in the window; `Window` (src/screens/shinatama.tsx) makes it and provides it.
// 3. A screen reads a controller with `useController(app.threads)`: it renders again whenever that controller says it changed, and
//    only then. A screen that reads two controllers calls it twice. A screen never keeps a copy of a controller's state in
//    `useState`: it keeps only what is the screen's alone (a field's draft, a menu that is open).
// 4. A screen asks for a change by calling a method of the controller (`app.layout.change(action)`), never by writing a field.
//    A method that talks to Rust is async, says what went wrong through `app.ui.report(text, { error: true })` (a notice) and
//    resolves; it does not throw to the screen unless the doc of the method says so.
// 5. Rust is called through `src/api/<area>.ts` only, from controllers (a screen that needs a one-off read asks its controller for
//    it). The browser has no Rust: `src/mock` answers the same commands there (development only).
// 6. A controller that listens to something (an event of Rust, a timer, a window event) starts it in `start()` and stops it in
//    `dispose()`; `Shinatama.start()` and `Shinatama.dispose()` call them for every controller, in the order of `app.ts`.
// 7. The model modules (`*-model.ts`, `workspace-model.ts`, `session-opens.ts`...) are pure functions and types; controllers use them
//    and screens may use their types and pure helpers. They import neither React nor Tauri.
//
// A `Store` is a number that goes up (the snapshot of `useSyncExternalStore`) and the listeners to wake.

export class Store {
  private version = 0;
  private readonly listeners = new Set<() => void>();

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  /** What changes whenever the state does: the snapshot of `useSyncExternalStore`. */
  snapshot = (): number => this.version;

  /** Tells the screens that the state changed. */
  protected changed(): void {
    this.version += 1;
    for (const listener of [...this.listeners]) listener();
  }

  /** The same, for a part that holds a store and lets it change from outside (a controller another one asks to draw). */
  touch(): void {
    this.changed();
  }

  /** Starts what the controller listens to; called once, when the window has its first frame. */
  start(): void | Promise<void> {}

  /** Stops what `start` started. */
  dispose(): void {}
}
