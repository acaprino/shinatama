// Musashi Terminal: a compress or absorb that cannot run is answered with why and with "do not repeat"; asked
// again with the same arguments in the same human turn, or refused a third time in it, the tool closes
// until the next human message (the rule of Tri-Tech Code 98c32608, after two chats repeated one refused
// compress 90 and 99 times). The state is a few keys in the session metadata, bound to the turn's human
// message and replaced at the next one: nothing reaches the next turn, and without a known turn nothing
// is counted.
import { createHash } from 'node:crypto';
import type { CoreMessage } from 'acp-kernel';
import { compressResult, toolOk, type ProxyToolResult } from '../proxy-tool-result.js';
import { markDirty, type Session } from '../session.js';
import { humanBoundaries } from './epoch.js';

type TurnTool = 'compress' | 'absorb';
type Refusals = { turn: string; refused: Record<string, number>; total: number; closed?: true };

const KEY = 'musashiTerminalTurnRefusals';
const MAX_ARGUMENTS = 8;
export const CLOSED_REASON = 'neutral:closed-for-turn';

export const closedForTurnText = (tool: TurnTool): string =>
    `[${tool} is not available for the rest of this turn: the requests made in it were refused more than once, so nothing was done. Do not call ${tool} again in this turn; continue with the task. It is available again at the next user message.]`;

function refusalsOf(session: Session, messages: readonly CoreMessage[]): Refusals | undefined {
    const turn = humanBoundaries(messages).at(-1)?.id;
    if (turn === undefined) return undefined;
    const saved = session.metadata[KEY] as Refusals | undefined;
    return saved?.turn === turn ? saved : { turn, refused: {}, total: 0 };
}

/** Whether compress and absorb gave up for the turn these messages belong to. */
export function closedForTurn(session: Session, messages: readonly CoreMessage[]): boolean {
    return refusalsOf(session, messages)?.closed === true;
}

/** A refusal is any call that changed nothing: a failure, a refused fold, or a neutral "not needed" receipt. */
function refused(result: ProxyToolResult): boolean {
    return result.outcome === 'failure' || result.outcome === 'refused' || result.reason?.startsWith('neutral:') === true;
}

/** Runs one compress or absorb call under the turn's refusals: closed, it answers without running; refused again with
 *  the same arguments, or a third time, it closes and says so in the receipt. */
export function underTurnRefusals(session: Session, messages: readonly CoreMessage[], tool: TurnTool, args: unknown, run: () => ProxyToolResult): ProxyToolResult {
    const state = refusalsOf(session, messages);
    if (!state) return run();
    if (state.closed) return tool === 'compress' ? compressResult(closedForTurnText(tool), 'applied', 0, CLOSED_REASON) : { ...toolOk(closedForTurnText(tool)), reason: CLOSED_REASON };
    const result = run();
    if (!refused(result)) return result;
    const key = `${tool}:${createHash('sha256').update(JSON.stringify(args) ?? 'null').digest('base64url').slice(0, 16)}`;
    const same = (state.refused[key] ?? 0) + 1;
    if (key in state.refused || Object.keys(state.refused).length < MAX_ARGUMENTS) state.refused[key] = same;
    state.total += 1;
    if (same >= 2 || state.total >= 3) state.closed = true;
    session.metadata[KEY] = state;
    markDirty(session);
    return state.closed ? { ...result, text: `${result.text} ${closedForTurnText(tool)}` } : result;
}
