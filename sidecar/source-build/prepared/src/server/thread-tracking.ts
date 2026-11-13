import { createHash } from "node:crypto";
import { BILLING_ATTRIBUTION_PREFIX } from "../preflight.js";
import type { Session } from "../session.js";

// #2701: Anthropic Threads bookkeeping. A threads-continue request carries
// ONLY the turn's delta messages — the conversation lives server-side, keyed
// by `thread.previous_message_id`. bili's compress loop consumes the client's
// continue target with its own internal round-2 fetch, so the id the client
// believes in (round 1's message_start id, forwarded verbatim) goes stale the
// moment an internal round advances the tail. Without repair, every
// post-compress turn on a threads session dies with upstream 400:
//   "The requested `previous_message_id` has already been continued. Each
//    response `id` may be continued at most once; continue from the ..."
// The proxy therefore (a) remembers the ids it has observed on the wire
// (message_start of every round) and (b) rewrites an inbound continue whose
// target bili itself already consumed to the current tail. Ids the proxy has
// never seen (client used another route, proxy state lost) are passed through
// untouched — the client knows more than we do in that case.

const THREAD_TAIL_KEY = "anthropicThreadTail";
const CONSUMED_CAP = 20;

interface ThreadTailTracking {
    /** Latest response id observed on the wire — the real thread tail. */
    tail: string;
    /** Ids already continued past (superseded by later rounds), oldest first. */
    consumed: string[];
}

export function readThreadTail(session: Session): ThreadTailTracking | undefined {
    const v = session.metadata[THREAD_TAIL_KEY];
    if (!v || typeof v !== "object") return undefined;
    const t = v as { tail?: unknown; consumed?: unknown };
    if (typeof t.tail !== "string") return undefined;
    const consumed = Array.isArray(t.consumed)
        ? t.consumed.filter((x): x is string => typeof x === "string")
        : [];
    return { tail: t.tail, consumed };
}

/**
 * Record a response id observed on the wire, in observe order. The first id
 * of a turn rolls the previous tail into the consumed set (this turn's round-1
 * request continued past it); later ids are bili's internal rounds rolling the
 * tail forward.
 */
export function recordThreadTail(session: Session, id: string): void {
    const prev = readThreadTail(session);
    if (prev && prev.tail === id) return;
    if (prev) {
        const consumed = [...prev.consumed.filter((x) => x !== prev.tail && x !== id), prev.tail];
        while (consumed.length > CONSUMED_CAP) consumed.shift();
        session.metadata[THREAD_TAIL_KEY] = { tail: id, consumed };
    } else {
        session.metadata[THREAD_TAIL_KEY] = { tail: id, consumed: [] };
    }
}

/**
 * True when an inbound anthropic body continues a server-side Thread: the
 * body carries only the delta turn while upstream usage bills the stored
 * conversation too, so the body's local estimate cannot be compared with
 * that usage report.
 */
export function isThreadContinuation(parsed: { thread?: unknown }): boolean {
    const thread = parsed.thread;
    return !!thread && typeof thread === "object" && (thread as { type?: unknown }).type === "continue";
}

export function resetThreadTailTracking(session: Session): void {
    delete session.metadata[THREAD_TAIL_KEY];
}

// Musashi Terminal: Claude Code's Threads client ("tether") lets a continue inherit
// `system` and `tools` from the stored Thread. It sends only the system
// blocks its own projection keeps (the billing attribution plus fixed
// sections such as "# Reporting outcomes") and drops `tools` while their
// hash matches the stored copy. Anthropic stored what the proxy forwarded at
// create, the proxy's own additions included, so an inherited field must be
// forwarded exactly as the client sent it. Appending the compress prompt,
// stamping cache_control or injecting the ACP tools into it made every
// session's first continue fail with "differing field(s): cache_control,
// system[, tools]"; Claude Code then replayed the whole conversation and sent
// every field explicitly for the rest of the session.
const THREAD_FIELDS_KEY = "anthropicThreadFields";
const REMEMBERED_SYSTEMS = 4;
// Claude Code 2.1.302 projects the system with Uj(): it keeps the billing
// attribution and the block whose text is its "# Reporting outcomes" section.
const RETAINED_SECTION_PREFIX = "# Reporting outcomes";

interface ThreadFieldMemory {
    /** Digests of the non-billing system blocks of recent explicit Thread sends, newest last. */
    systems: string[][];
    /** Whether the tools last forwarded explicitly on a Thread declared compress. */
    compressTool?: boolean;
    /** Digests of what the proxy added to the system and to the tools of the stored Thread. */
    added?: Partial<ThreadAdditions>;
    /** Additions a new Thread was already requested for, so one change asks once. */
    replayFor?: string;
}

export interface ThreadInheritance {
    system: boolean;
    tools: boolean;
}

/** Digests of the proxy's additions to an anthropic system and tools. */
export interface ThreadAdditions {
    system: string;
    tools: string;
}

function readThreadFields(session: Session): ThreadFieldMemory {
    const v = session.metadata[THREAD_FIELDS_KEY] as { systems?: unknown; compressTool?: unknown; added?: unknown; replayFor?: unknown } | undefined;
    const systems = Array.isArray(v?.systems)
        ? v.systems.filter((s): s is string[] => Array.isArray(s) && s.every((d) => typeof d === "string"))
        : [];
    const added = v?.added && typeof v.added === "object" ? v.added as { system?: unknown; tools?: unknown } : undefined;
    return {
        systems,
        compressTool: typeof v?.compressTool === "boolean" ? v.compressTool : undefined,
        added: added ? {
            ...(typeof added.system === "string" ? { system: added.system } : {}),
            ...(typeof added.tools === "string" ? { tools: added.tools } : {}),
        } : undefined,
        replayFor: typeof v?.replayFor === "string" ? v.replayFor : undefined,
    };
}

/** Digest of one addition (the appended system text, the appended tools); null when the proxy adds nothing. */
export function additionDigest(value: unknown): string {
    const empty = value === null || value === undefined || value === "" || (Array.isArray(value) && value.length === 0);
    return empty ? "none" : createHash("sha256").update(JSON.stringify(value)).digest("hex").slice(0, 16);
}

/** Digests of the system blocks a projection could keep, or undefined when
 *  some block is not plain text (a projection only ever carries text). */
function nonBillingDigests(system: unknown): string[] | undefined {
    const blocks = typeof system === "string" ? (system === "" ? [] : [{ type: "text", text: system }]) : system;
    if (blocks === undefined || blocks === null) return [];
    if (!Array.isArray(blocks)) return undefined;
    const digests: string[] = [];
    for (const block of blocks) {
        const b = block as { type?: unknown; text?: unknown } | null;
        if (!b || b.type !== "text" || typeof b.text !== "string") return undefined;
        if (b.text.trimStart().startsWith(BILLING_ATTRIBUTION_PREFIX)) continue;
        digests.push(createHash("sha256").update(b.text).digest("hex").slice(0, 16));
    }
    return digests;
}

/** Whether every non-billing block is one Claude Code's projection keeps. */
function onlyRetainedSections(system: unknown): boolean {
    const blocks = typeof system === "string" ? [{ type: "text", text: system }] : system;
    if (!Array.isArray(blocks)) return false;
    return blocks.every((block) => {
        const b = block as { type?: unknown; text?: unknown } | null;
        if (!b || b.type !== "text" || typeof b.text !== "string") return false;
        const text = b.text.trimStart();
        return text.startsWith(BILLING_ATTRIBUTION_PREFIX) || text.startsWith(RETAINED_SECTION_PREFIX);
    });
}

/**
 * Which fields an inbound anthropic continue leaves to the stored Thread.
 * `tools` is inherited when absent. `system` is inherited when it carries no
 * block beyond those Claude Code's projection keeps (unless this session sent
 * exactly those blocks as its full system), or only blocks of a full system
 * this session recently sent explicitly on a Thread, and fewer of them. The
 * projection rule needs no memory, so a session the proxy lost still keeps
 * the client's projection intact.
 */
export function threadInheritance(parsed: { thread?: unknown; system?: unknown; tools?: unknown }, session: Session): ThreadInheritance {
    if (!isThreadContinuation(parsed)) return { system: false, tools: false };
    const kept = nonBillingDigests(parsed.system);
    const systems = readThreadFields(session).systems;
    const system = kept !== undefined && (kept.length === 0
        || (onlyRetainedSections(parsed.system) && !systems.some((full) => JSON.stringify(full) === JSON.stringify(kept)))
        || systems.some((full) => kept.length < full.length && kept.every((d) => full.includes(d))));
    return { system, tools: parsed.tools === undefined };
}

/** Record the explicit fields of an inbound Thread request (create or
 *  continue): the client's own system, whether the tools the proxy
 *  forwarded declared compress, which an inheriting continue keeps, and the
 *  digests of what the proxy actually added to each explicit field. */
export function rememberThreadFields(
    parsed: { thread?: unknown },
    clientSystem: unknown,
    inherited: ThreadInheritance,
    forwardedToolsDeclareCompress: boolean,
    applied: ThreadAdditions,
    session: Session,
): void {
    if (!parsed.thread || typeof parsed.thread !== "object") return;
    const memory = readThreadFields(session);
    const digests = inherited.system ? undefined : nonBillingDigests(clientSystem);
    if (digests !== undefined && digests.length > 0) {
        const key = JSON.stringify(digests);
        memory.systems = [...memory.systems.filter((s) => JSON.stringify(s) !== key), digests];
        while (memory.systems.length > REMEMBERED_SYSTEMS) memory.systems.shift();
    }
    if (!inherited.tools) memory.compressTool = forwardedToolsDeclareCompress;
    memory.added = {
        ...memory.added,
        ...(inherited.system ? {} : { system: applied.system }),
        ...(inherited.tools ? {} : { tools: applied.tools }),
    };
    session.metadata[THREAD_FIELDS_KEY] = memory;
}

export type ThreadAdditionsCheck =
    | { replay: false; repeated?: string[] }
    | { replay: true; fields: string[] };

/**
 * Compare what the proxy would add now with what the stored Thread holds, for
 * each field an inbound continue inherits. A stored Thread cannot gain a tool
 * or a prompt section armed after its create (settings reloaded, CCR or image
 * compression turned on), so a difference asks the client for a new Thread
 * (see threadReplayBody). Without a record of the stored additions (a session
 * the proxy lost, a build that did not record them) the current ones are
 * presumed and recorded, so a later change is still detected. Each distinct
 * set of additions asks once: if the next continue still differs, the proxy
 * forwards it and reports `repeated` instead of asking again.
 */
export function checkThreadAdditions(
    parsed: { thread?: unknown },
    inherited: ThreadInheritance,
    desired: ThreadAdditions,
    session: Session,
): ThreadAdditionsCheck {
    if (!isThreadContinuation(parsed) || (!inherited.system && !inherited.tools)) return { replay: false };
    const memory = readThreadFields(session);
    const added: Partial<ThreadAdditions> = { ...memory.added };
    const fields: string[] = [];
    for (const field of ["system", "tools"] as const) {
        if (!inherited[field]) continue;
        if (added[field] === undefined) added[field] = desired[field];
        else if (added[field] !== desired[field]) fields.push(field);
    }
    memory.added = added;
    if (fields.length === 0) {
        delete memory.replayFor;
        session.metadata[THREAD_FIELDS_KEY] = memory;
        return { replay: false };
    }
    const key = `${desired.system}/${desired.tools}`;
    if (memory.replayFor === key) {
        session.metadata[THREAD_FIELDS_KEY] = memory;
        return { replay: false, repeated: fields };
    }
    memory.replayFor = key;
    session.metadata[THREAD_FIELDS_KEY] = memory;
    return { replay: true, fields };
}

/** The error that makes Claude Code replay the conversation on a new Thread:
 *  the shape Anthropic answers for an unknown Thread. Claude Code handles
 *  `thread_not_found` with a create and keeps inheriting fields afterwards,
 *  while a fingerprint mismatch on an inherited field would turn inheritance
 *  off for the rest of its session. */
export function threadReplayBody(fields: readonly string[]): string {
    return JSON.stringify({
        type: "error",
        error: {
            type: "not_found_error",
            message: `thread_not_found: the proxy's additions to ${fields.join(" and ")} changed since this Thread was created, so the stored Thread cannot carry them. Replay the full conversation with \`thread: {"type": "create"}\` to start a new Thread.`,
            details: { error_code: "thread_not_found" },
        },
    });
}

/** Whether the tools an inheriting continue keeps from the stored Thread declare compress. */
export function inheritedToolsDeclareCompress(session: Session): boolean {
    return readThreadFields(session).compressTool === true;
}

/**
 * Rewrite an inbound anthropic body's `thread` in place:
 * - `create` → drop tracking (new server-side thread; old ids are moot).
 * - `continue` whose `previous_message_id` the proxy itself already consumed
 *   → retarget to the tracked tail (the loop's internal rounds advanced it).
 * Unknown targets are left untouched.
 */
export function rewriteInboundThreadContinue(
    parsed: { thread?: unknown },
    session: Session,
    log: (level: string, msg: string) => void,
    sessionId: string,
): void {
    const thread = parsed.thread;
    if (!thread || typeof thread !== "object") return;
    const t = thread as { type?: unknown; previous_message_id?: unknown };
    if (t.type === "create") {
        if (session.metadata[THREAD_TAIL_KEY] !== undefined) {
            resetThreadTailTracking(session);
            log("info", `[${sessionId}] [threads] create — tail tracking reset (#2701)`);
        }
        return;
    }
    if (t.type !== "continue" || typeof t.previous_message_id !== "string") return;
    const tracking = readThreadTail(session);
    if (!tracking || t.previous_message_id === tracking.tail) return;
    if (!tracking.consumed.includes(t.previous_message_id)) return; // never saw it — trust the client
    const from = t.previous_message_id;
    t.previous_message_id = tracking.tail;
    log("info", `[${sessionId}] [threads] inbound continue target ${from} was already consumed by an internal loop round — retargeted to current tail ${tracking.tail} (#2701)`);
}
