// Musashi Terminal: the MCP shim keeps the Claude Code session id it was started with (CLAUDE_CODE_SESSION_ID). After
// a /resume the same Claude Code process runs another session, and the shim's id names a session that a side request
// had already created in the proxy, so the shim's own 404 recovery (#656) never fired: every bili tool ran on the empty
// launch session (acp_retrieve "not found", 2026-10-26). A tool call the model of exactly one session just emitted, and
// that reached the client after the bound session's last response, belongs to that session.
import { witnessedAt } from "../tool-ring.js";

const RESPONSES_CAP = 1024;
/** sessionId → when its last model response finished, least recent first. */
const lastResponse = new Map<string, number>();

/** A model response of this session finished: the session is live from then. */
export function noteSessionResponse(sessionId: string, at: number = Date.now()): void {
    lastResponse.delete(sessionId);
    lastResponse.set(sessionId, at);
    for (const oldest of lastResponse.keys()) {
        if (lastResponse.size <= RESPONSES_CAP) break;
        lastResponse.delete(oldest);
    }
}

/** The session a call bound only by the shim's start-up id runs on instead of the bound one: the single session whose
 *  stream witnessed this exact call, when the bound session has had no response since. Otherwise undefined, and the
 *  ordinary routing rules stand (a conflicting witness is refused). */
export function staticBindingWitness(boundSessionId: string | undefined, witnessIds: ReadonlySet<string>, tool: string, args: unknown): string | undefined {
    if (witnessIds.size !== 1) return undefined;
    const [witness] = [...witnessIds];
    if (witness === undefined || witness === boundSessionId) return undefined;
    const at = witnessedAt(witness, tool, args);
    if (at === undefined) return undefined;
    const bound = boundSessionId === undefined ? undefined : lastResponse.get(boundSessionId);
    return bound === undefined || bound <= at ? witness : undefined;
}

export function resetMcpBindingForTest(): void {
    lastResponse.clear();
}
