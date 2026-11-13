import type { IncomingMessage } from 'node:http';
import type { CoreMessage } from 'acp-kernel';
import type { WireProtocol } from '../util.js';
import { outputBudgetField, readOutputBudget } from '../server/side-request.js';
import { modelVisiblePacketTextBytes, survivingHiddenThinking } from './media-budget.js';
import { discardBoundPromptRewrite } from './rewrite-wire.js';

type RewriteBudget = {
    protocol: WireProtocol;
    nativeWindow: number;
    inputWindow: number;
    messages: CoreMessage[];
    imageReserve(body: object): number;
    ledgerFloorTokens?: number;
};
/** Optional guidance never spends the headroom reserved for the actual task.
 * Judge the final wire after output clamping, including all existing notes and
 * tools, without counting transport media bytes as text or changing kernel IDs.
 */
export function retainBoundPromptRewrite(request: IncomingMessage | undefined, body: Record<string, unknown>, budget: RewriteBudget): boolean {
    let fits = false;
    try {
        const { nativeWindow, inputWindow, protocol } = budget;
        const field = outputBudgetField(body);
        const configuredOutput = protocol === 'openai' && typeof body.max_completion_tokens === 'number'
            ? body.max_completion_tokens : field ? readOutputBudget(body, field) : undefined;
        // A missing wire bound still has the host's reserved output headroom;
        // retain a conservative default when that window was not reduced.
        const output = configuredOutput ?? Math.max(8192, nativeWindow - inputWindow);
        const images = budget.imageReserve(body);
        const input = Math.max(Math.ceil(modelVisiblePacketTextBytes(body, protocol) / 4) + images + survivingHiddenThinking(budget.messages, body, protocol), budget.ledgerFloorTokens ?? 0);
        const reserve = Math.max(4096, Math.ceil(nativeWindow * 0.05));
        fits = [nativeWindow, inputWindow, input, images, output].every(Number.isFinite)
            && nativeWindow > 0 && inputWindow > 0 && images >= 0 && output >= 0
            && input <= inputWindow && input + output + reserve <= nativeWindow;
    } catch { /* Unmeasurable optional guidance is discarded, never the original. */ }
    if (!fits) discardBoundPromptRewrite(request, body);
    return fits;
}
