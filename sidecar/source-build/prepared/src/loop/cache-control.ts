import type { AnthropicRequestBody } from "acp-kernel/wire";
import { noCacheControl as knobNoCacheControl } from "../knobs.js";

/**
 * #1637: Anthropic prompt caching is EXPLICIT — content blocks must carry
 * `cache_control` breakpoints or nothing upstream caches. bili used to emit
 * none, so Anthropic-style upstreams never cached anything bili sent, and
 * relays that breakpoint only the system block left the ENTIRE history
 * re-billing at full price on every request (#1631: cached pinned at the
 * system head, 9–13%, for 40+ requests straight).
 *
 * Both the steady request path (prepareAnthropic) and the compress-loop
 * round-2 rebuild (adapter-anthropic) apply THE SAME marks through the
 * kernel's own `coreToAnthropic(messages, cacheControls)` machinery — one
 * applier, identical bytes on both paths (the dual-builder divergence class
 * the cache matrix pins; see #1554).
 *
 * Musashi Terminal cadence: a MOVING pair. Each request writes at its last stable
 * message and keeps the previous request's write point as its read point,
 * which stays inside Anthropic's lookback however many blocks the turn
 * added. `cache_control` is not part of the cached prefix: Claude Code moves
 * its own breakpoint every request at 95-100% cache hits (20 October 2026
 * logs). The former cumulative cadence stopped at 3 marks, so a client
 * without its own breakpoints never had its newer history written. Marks
 * never change a message's shape: the kernel always emits block arrays.
 * 2 message marks + 1 system breakpoint stay within the 4-block limit.
 *
 * Client-managed caching wins: any pre-existing cache_control (harvested
 * from the client's messages, present on system blocks, or on tools entries —
 * the 4-breakpoint budget counts all three) suppresses our stamps entirely
 * (pass no marks / return the system unchanged).
 * BILI_NO_CACHE_CONTROL / compat.noCacheControl disables everything.
 */

type AnthropicCacheMarks = Map<string, { type: "ephemeral" }>;

export function computeAnthropicMessageMarks(
    processed: { id?: string }[],
    ephemeralTailCount: number,
    session: { metadata: Record<string, unknown> },
): AnthropicCacheMarks {
    const marks = new Map<string, { type: "ephemeral" }>();
    if (knobNoCacheControl()) {
        session.metadata["anthropicCacheMarkIds"] = [];
        return marks;
    }
    if (processed.length === 0 || processed.length <= ephemeralTailCount) {
        session.metadata["anthropicCacheMarkIds"] = [];
        return marks;
    }
    const liveIds = new Set(processed.map((m) => m.id).filter((id): id is string => typeof id === "string" && id.length > 0));
    // The previous write point survives as the read point unless it folded away.
    const read = ((session.metadata["anthropicCacheMarkIds"] as string[] | undefined) ?? []).at(-1);
    const kept = read !== undefined && liveIds.has(read) ? [read] : [];
    const lastIndex = processed.length - 1 - ephemeralTailCount;
    const currentId = processed[lastIndex]?.id;
    if (typeof currentId === "string" && currentId.length > 0 && !kept.includes(currentId)) {
        kept.push(currentId);
    }
    session.metadata["anthropicCacheMarkIds"] = kept;
    for (const id of kept) marks.set(id, { type: "ephemeral" });
    return marks;
}

/** WC-010: Anthropic's 4-breakpoint budget spans system blocks + tools entries
 *  + message blocks COMBINED; anthropicToCore only harvests messages, so a
 *  tools-only client mark must suppress ours too or the 5th breakpoint 400s. */
export function anthropicToolsCarryCacheControl(tools: unknown): boolean {
    if (!Array.isArray(tools)) return false;
    return tools.some((t) => typeof t === "object" && t !== null && (t as { cache_control?: unknown }).cache_control !== undefined);
}

/** #2701: a Threads re-request carries only the loop's delta, and Anthropic
 *  looks the stored Thread up only at the breakpoints that request sends — a
 *  delta without one reads the cache no further than system/tools and re-bills
 *  the whole conversation. The round-1 body's last message breakpoint (the
 *  client's TTL included) is the one to repeat. */
export function lastAnthropicMessageCacheControl(messages: unknown): unknown {
    if (!Array.isArray(messages)) return undefined;
    for (let i = messages.length - 1; i >= 0; i--) {
        const content = (messages[i] as { content?: unknown } | null)?.content;
        if (!Array.isArray(content)) continue;
        for (let j = content.length - 1; j >= 0; j--) {
            const mark = (content[j] as { cache_control?: unknown } | null)?.cache_control;
            if (mark !== undefined) return mark;
        }
    }
    return undefined;
}

/** Puts `cacheControl` on the delta's last block that may carry one (Anthropic
 *  rejects it on thinking and empty text blocks). The delta joins the stored
 *  Thread, so the mark sits on stable content, and it stands in for the
 *  round-1 mark the delta replaces: the 4-breakpoint budget holds. */
export function stampAnthropicDeltaCacheControl<T extends { content?: unknown }>(messages: T[], cacheControl: unknown): T[] {
    if (cacheControl === undefined || knobNoCacheControl()) return messages;
    const last = messages.length - 1;
    const content = messages[last]?.content;
    if (!Array.isArray(content)) return messages;
    for (let j = content.length - 1; j >= 0; j--) {
        const block = content[j] as { type?: unknown; text?: unknown };
        if (block.type === "thinking" || block.type === "redacted_thinking") continue;
        if (block.type === "text" && !block.text) continue;
        // Stamp by replacement: delta blocks may share references with the
        // loop's core messages (raw client blocks).
        const stamped = [...content];
        stamped[j] = { ...block, cache_control: cacheControl };
        const out = [...messages];
        out[last] = { ...messages[last], content: stamped };
        return out;
    }
    return messages;
}

export function stampAnthropicSystemCacheControl(systemOut: AnthropicRequestBody["system"], ours = true): AnthropicRequestBody["system"] {
    if (knobNoCacheControl()) return systemOut;
    if (!ours) return systemOut;
    if (typeof systemOut === "string" && systemOut.length > 0) {
        // Deterministic string→block conversion (semantically identical for
        // Anthropic; stable across turns). A string system can carry no
        // client cache_control, so conversion never discards one.
        return [{ type: "text", text: systemOut, cache_control: { type: "ephemeral" } }];
    }
    if (!Array.isArray(systemOut) || systemOut.length === 0) return systemOut;
    for (const block of systemOut) {
        if ((block as { cache_control?: unknown }).cache_control) return systemOut;
    }
    // Stamp by replacement, never in place: the array may share block
    // references with the frozen client head (parsed.system), which must stay
    // unmarked (#1085).
    const last = systemOut.length - 1;
    systemOut[last] = { ...systemOut[last], cache_control: { type: "ephemeral" } };
    return systemOut;
}
