// The hooks of the screens that draw nothing: the controllers as stores, and the one `Shinatama` of the window.
import { createContext, useContext, useEffect, useState, useSyncExternalStore } from 'react';
import type { Shinatama } from './app.ts';
import type { Store } from './store.ts';

export const AppContext = createContext<Shinatama | null>(null);

/** The controllers of the window. Throws outside the window of Shinatama: a screen is always drawn inside it. */
export function useApp(): Shinatama {
  const app = useContext(AppContext);
  if (app === null) throw new Error('outside the window of Shinatama');
  return app;
}

/** A render whenever the store changes; gives the store back, so `useController(app.layout).layout` reads at once. */
export function useController<T extends Store>(store: T): T {
  useSyncExternalStore(store.subscribe, store.snapshot);
  return store;
}

/** How long a wait is hidden before it is shown: an answer that comes at once never flashes a loading state. */
export const LOADING_DELAY_MS = 250;

/** True once `active` has held for `ms`; false as soon as it does not. */
export function useAfter(active: boolean, ms: number = LOADING_DELAY_MS): boolean {
  const [late, setLate] = useState(false);
  useEffect(() => {
    if (!active) { setLate(false); return; }
    const timer = setTimeout(() => setLate(true), ms);
    return () => clearTimeout(timer);
  }, [active, ms]);
  return active && late;
}
