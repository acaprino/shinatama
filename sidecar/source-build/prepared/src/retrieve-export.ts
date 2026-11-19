import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { RetrievalExport } from "acp-kernel";

/** Directory name for one session's retrieve exports: a short hash of the
 *  session id, never the id itself, so no header-derived text reaches a path.
 *  mNNNNN refs are PER-SESSION counters, so two sessions that start alike hold
 *  the same ref for different tool results; a flat <ref>.txt would let the
 *  second retrieve overwrite the file the first session's pointer still names. */
function retrievalSessionKey(sessionId: string): string {
    return createHash("sha256").update(sessionId).digest("hex").slice(0, 16);
}

/** <stateRoot>/retrieve/<session key>: the kernel appends <ref>.txt. The key is
 *  16 hex characters, so it cannot collide with the retrieve/img spill tree. */
export function retrievalExportDirFor(stateRoot: string, sessionId: string): string {
    return join(stateRoot, "retrieve", retrievalSessionKey(sessionId));
}

/** Host side of the kernel RetrievalExport effect: 0600 (folded conversation
 *  content is not world-readable on multi-user hosts), parents created on
 *  demand. Within one session a ref names one original, so a repeated retrieve
 *  rewrites identical bytes (idempotent). */
export function writeRetrievalExport(exported: RetrievalExport, warn: (message: string) => void): boolean {
    try {
        mkdirSync(dirname(exported.path), { recursive: true });
        writeFileSync(exported.path, exported.text, { encoding: "utf8", mode: 0o600 });
        return true;
    } catch (e) {
        warn(`[ccr] retrieve export write failed (${exported.path}): ${String(e)}`);
        return false;
    }
}
