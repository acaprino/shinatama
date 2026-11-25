// The terminals that are drawn, by session: what a screen outside a terminal's tab needs of them (the search of its output, the history
// over it). A tab registers its terminal when it is made and takes it away when it goes. Nothing here draws.
import type { TerminalHandle } from '@acaprino/sigrid/terminal';
import type { TerminalHistory } from '../../terminal-history.ts';

interface Drawn { handle: TerminalHandle; history: TerminalHistory }
const drawn = new Map<string, Drawn>();

export function rememberHandle(id: string, handle: TerminalHandle, history: TerminalHistory): void {
  drawn.set(id, { handle, history });
}

export function forgetHandle(id: string): void {
  drawn.delete(id);
}

export const handleOf = (id: string): TerminalHandle | undefined => drawn.get(id)?.handle;
