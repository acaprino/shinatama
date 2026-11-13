// Musashi Terminal: the past is rewritten only when the frontier moves, at a human message (the rule of
// 20 October 2026, shared with Tri-Tech Code's billion-flow/core/frontier.ts). Every request of an
// epoch sends the same view of the past, so the provider's prompt cache keeps it. Here the folds
// live in the session state: a fold accepted during an epoch waits in `pending`, and the session
// keeps the epoch's state until the frontier moves.
import { createHash } from 'node:crypto';
import { defaultCountTokens, defaultPrompts, type CompressionState, type CoreMessage } from 'acp-kernel';
import { markDirty, type Session } from '../session.js';

/** Why the provider's prompt cache is already cold when a turn starts: a rewrite of the past then costs nothing more. */
export type FrontierCold = 'compaction' | 'model' | 'idle';
export type FrontierCause = 'pressure' | 'compress' | 'emergency' | FrontierCold;
/** `threshold`: the curve's pressure, the least history worth a free rewrite. `start`: where the epochs begin.
 *  `quantum`: the history one epoch adds before the next. */
export type FrontierSettings = { readonly threshold: number; readonly start: number; readonly quantum: number };
/** The message the view's past ends at, the measure of the history up to the end of its group when it moved there, and
 *  why. `invite` records whether the epoch carries the invitation to compress; `step` marks an emergency frontier. */
type Frontier = { id: string; tokens: number; cause: FrontierCause; step?: true; invite?: boolean };
type Pending = Pick<CompressionState, 'blocks' | 'absorbed' | 'nextBlockId' | 'nextRunId' | 'deadRefs' | 'terminalStreak' | 'stats'>;
/** `reached`: the pressure level of the current frontier. `seen`: the last human message evaluated.
 *  `at`: the last request, to tell an idle cache. `pending`: the state the tools changed during the epoch.
 *  `sent`: a fingerprint of each message the last request sent. `stripped`: signed reasoning removed for good. */
export type EpochRecord = { version: 1; epoch: number; reached: number; at: number; seen?: string; model?: string; frontier?: Frontier; pending?: Pending; sent?: string[]; stripped?: string[] };
export type EpochView = { tail: number; tokens: number; invite?: boolean };

const KEY = 'musashiTerminalEpoch';
/** The cache entry outlives its lifetime by a little: the margin keeps a warm cache from being taken for a cold one. */
export const IDLE_MARGIN_MS = 60_000;
export const DEFAULT_CACHE_TTL_SECONDS = 3600;
/** Past this share of the window a long turn may move the frontier to its last step. */
export const EMERGENCY_WINDOW_RATIO = 0.85;

export const idleBeyond = (gapMs: number, ttlMs: number): boolean => Number.isFinite(gapMs) && gapMs > ttlMs + IDLE_MARGIN_MS;

export function epochRecord(session: Session): EpochRecord | undefined {
    const saved = session.metadata[KEY] as EpochRecord | undefined;
    return saved?.version === 1 && Number.isSafeInteger(saved.epoch) && Number.isFinite(saved.reached) ? saved : undefined;
}
function save(session: Session, record: EpochRecord): void {
    session.metadata[KEY] = record;
    markDirty(session);
}
/** A session that leaves the adaptive path keeps its accepted folds: they apply now and the epochs end. */
export function endEpochs(session: Session): void {
    const record = epochRecord(session);
    if (!record) return;
    if (record.pending) session.state = withPending(session.state, record.pending);
    delete session.metadata[KEY];
    markDirty(session);
}

/** The curve's threshold; on a window the curve has bent (a budget above `bend`) the epochs start at half the budget,
 *  on a smaller one at the threshold. The quantum is `epochTokens` when set, otherwise half the threshold, at least 32 000. */
export function frontierSettings(threshold: number, usable: number, bend: number, epochTokens?: number): FrontierSettings {
    const start = usable > bend ? Math.max(threshold, Math.floor(usable / 2)) : threshold;
    const quantum = epochTokens !== undefined && epochTokens > 0 ? epochTokens : Math.max(Math.floor(threshold / 2), 32_000);
    return { threshold: Math.round(threshold), start: Math.round(start), quantum };
}
const level = (tokens: number, settings: FrontierSettings) => tokens >= settings.start ? Math.floor((tokens - settings.start) / settings.quantum) : -1;

export function tokensOf(messages: readonly CoreMessage[]): number {
    let tokens = 0;
    for (const message of messages) tokens += defaultCountTokens(message.text ?? '');
    return tokens;
}
const synthetic = (message: CoreMessage) => message.id.startsWith('acp_') || message.id.startsWith('bili_');
/** The core messages one protocol message was flattened into share its wireIndex. */
function wireGroups(messages: readonly CoreMessage[]): Array<{ start: number; end: number }> {
    const groups: Array<{ start: number; end: number }> = [];
    for (let start = 0; start < messages.length;) {
        const wire = (messages[start] as { wireIndex?: number }).wireIndex;
        let end = start + 1;
        while (wire !== undefined && end < messages.length && (messages[end] as { wireIndex?: number }).wireIndex === wire && messages[end]!.role === messages[start]!.role) end++;
        groups.push({ start, end });
        start = end;
    }
    return groups;
}
const carriesToolResult = (messages: readonly CoreMessage[], start: number, end: number) => messages.slice(start, end).some(message => message.contentType === 'tool-result');
/** The end of a boundary's group: the message and the user messages sent with it, up to the first answer. */
function groupEnd(messages: readonly CoreMessage[], start: number): number {
    let end = start + 1;
    while (end < messages.length && messages[end]!.role !== 'assistant') end++;
    return end;
}
/** The human messages in order: protocol messages from the user that carry no tool result. */
export function humanBoundaries(messages: readonly CoreMessage[]): Array<{ id: string; end: number }> {
    const boundaries: Array<{ id: string; end: number }> = [];
    let after = 0;
    for (const { start, end } of wireGroups(messages)) {
        if (start < after || messages[start]!.role !== 'user' || synthetic(messages[start]!) || carriesToolResult(messages, start, end)) continue;
        after = groupEnd(messages, start);
        boundaries.push({ id: messages[start]!.id, end: after });
    }
    return boundaries;
}
export function frontierEnd(messages: readonly CoreMessage[], id: string): number | undefined {
    const at = messages.findIndex(message => message.id === id);
    return at < 0 ? undefined : groupEnd(messages, at);
}
/** The measure of the history up to a message: its tokens as they arrive (refs, CCR placeholders), less what the folds
 *  and absorptions of `state` save there, each counted once the messages it covers all come before that point (Tri-Tech
 *  Code's compressSavings). It depends only on the history, the configuration and the accepted folds, never on what the
 *  proxy sent before, so a restart measures the same. */
function measurer(messages: readonly CoreMessage[], state: CompressionState, overhead: number): (end: number) => number {
    const index = new Map<string, number>(), prefix = [0];
    messages.forEach((message, at) => {
        index.set(message.id, at);
        prefix.push(prefix[at]! + defaultCountTokens(message.text ?? ''));
    });
    const tokensAt = (at: number) => prefix[at + 1]! - prefix[at]!;
    const saved: Array<{ last: number; tokens: number }> = [];
    for (const block of state.blocks) {
        if (!block.active || block.effectiveMessageIds.length === 0) continue;
        let last = -1, covered = 0;
        for (const id of block.effectiveMessageIds) {
            const at = index.get(id);
            if (at === undefined) { last = -1; break; }
            last = Math.max(last, at);
            covered += tokensAt(at);
        }
        if (last >= 0) saved.push({ last, tokens: Math.max(0, covered - defaultCountTokens(block.summary ?? '')) });
    }
    for (const record of state.absorbed ?? []) {
        const at = index.get(record.resultMessageId);
        if (at !== undefined) saved.push({ last: at, tokens: Math.max(0, tokensAt(at) - defaultCountTokens(record.summary ?? '')) });
    }
    return (end) => overhead + prefix[end]! - saved.reduce((sum, entry) => entry.last < end ? sum + entry.tokens : sum, 0);
}

/** `arrive` gives the messages as every request sends them once they arrived (same ids, same order): the measure reads
 *  their tokens there, so a large result CCR replaces at arrival weighs its placeholder. */
export type FrontierInput = { overhead: number; settings: FrontierSettings; model?: string; now: number; ttlMs: number; arrive?: (messages: readonly CoreMessage[]) => readonly CoreMessage[] };
/** Evaluates the human messages the session has not seen yet. The frontier moves to one when the request reaches a new
 *  pressure level, at the first one after a fold was accepted, and, above the threshold, when the cache is cold anyway.
 *  Messages already evaluated are never judged again, so the view of an epoch cannot move under it. */
export function advanceFrontier(session: Session, messages: readonly CoreMessage[], input: FrontierInput): { record: EpochRecord; moved: boolean; dropped: boolean } {
    const previous = epochRecord(session);
    const humans = humanBoundaries(messages);
    let record: EpochRecord = previous ?? { version: 1, epoch: 0, reached: -1, at: input.now };
    let known = record.seen === undefined ? -1 : humans.findIndex(boundary => boundary.id === record.seen);
    let cold: FrontierCold | undefined, dropped = false;
    if (previous && ((previous.seen !== undefined && known < 0) || (previous.frontier && frontierEnd(messages, previous.frontier.id) === undefined))) {
        // The client rewrote its history (native compaction, rewind, edit): the cache is cold, and the accepted folds
        // may cite messages that are gone.
        cold = 'compaction';
        dropped = previous.pending !== undefined;
        record = { version: 1, epoch: previous.epoch, reached: -1, at: input.now };
        known = -1;
    } else if (previous?.model !== undefined && input.model !== undefined && input.model !== previous.model) cold = 'model';
    else if (previous && idleBeyond(input.now - previous.at, input.ttlMs)) cold = 'idle';
    const fresh = humans.slice(known + 1);
    // A pending fold lands at the first new human message, so that message is measured with it.
    const measure = fresh.length > 0 ? measurer(input.arrive?.(messages) ?? messages, record.pending ? withPending(session.state, record.pending) : session.state, input.overhead) : undefined;
    let moved = false;
    fresh.forEach((boundary, at) => {
        const tokens = measure!(boundary.end), reached = level(tokens, input.settings);
        const here = at === fresh.length - 1 ? cold : undefined;
        // Compared with the current frontier's level: after a fold the next epoch comes when the history grows back.
        const cause = reached > record.reached ? 'pressure' : at === 0 && record.pending ? 'compress' : here && tokens >= input.settings.threshold ? here : undefined;
        if (cause) {
            record.frontier = { id: boundary.id, tokens, cause };
            record.reached = reached;
            record.epoch += 1;
            moved = true;
        }
    });
    if (fresh.length > 0) record.seen = fresh.at(-1)!.id;
    record.at = input.now;
    if (input.model !== undefined) record.model = input.model;
    save(session, record);
    return { record, moved, dropped };
}

/** A request that judges no frontier (a Threads continue) still counts for the idle cache. */
export function touchEpoch(session: Session, now: number): void {
    const record = epochRecord(session);
    if (!record) return;
    record.at = now;
    save(session, record);
}

/** A long turn near the window moves the frontier to its last step, once: then only a fold the model makes moves it again
 *  before the next human message. */
export function emergencyFrontier(session: Session, messages: readonly CoreMessage[], tokens: number, settings: FrontierSettings): boolean {
    const record = epochRecord(session);
    const groups = wireGroups(messages), last = groups.at(-1);
    if (!record || !last || !carriesToolResult(messages, last.start, last.end)) return false;
    const id = messages[last.start]!.id;
    if (record.frontier?.id === id || (record.frontier?.cause === 'emergency' && !record.pending)) return false;
    record.frontier = { id, tokens, cause: record.pending ? 'compress' : 'emergency', step: true };
    record.reached = Math.max(record.reached, level(tokens, settings));
    record.epoch += 1;
    save(session, record);
    return true;
}

/** Records, once per epoch, whether its view carries the invitation to compress. */
export function settleFrontier(session: Session, invite: boolean): void {
    const record = epochRecord(session);
    if (!record?.frontier || record.frontier.invite !== undefined) return;
    record.frontier.invite = invite;
    save(session, record);
}

function maxStats(view: CompressionState['stats'], pending: CompressionState['stats']): CompressionState['stats'] {
    const merged: Record<string, unknown> = { ...view };
    for (const [key, value] of Object.entries(pending)) {
        const current = merged[key];
        merged[key] = typeof value === 'number' && typeof current === 'number' ? Math.max(value, current) : value;
    }
    return merged as unknown as CompressionState['stats'];
}
function withPending(state: CompressionState, pending: Pending): CompressionState {
    return { ...state, blocks: pending.blocks, absorbed: pending.absorbed, nextBlockId: Math.max(state.nextBlockId, pending.nextBlockId),
        nextRunId: Math.max(state.nextRunId, pending.nextRunId), deadRefs: pending.deadRefs, terminalStreak: pending.terminalStreak, stats: maxStats(state.stats, pending.stats) };
}
/** The folds and absorptions accepted during the epoch reach the session when the frontier moves; returns the ids
 *  of the blocks that come into the view. */
export function materializePending(session: Session, record: EpochRecord): string[] | undefined {
    if (!record.pending) return undefined;
    const before = new Set(session.state.blocks.map(block => block.blockId));
    session.state = withPending(session.state, record.pending);
    delete record.pending;
    save(session, record);
    return session.state.blocks.filter(block => !before.has(block.blockId)).map(block => block.blockId);
}
/** A session on the epoch path below its first frontier: nothing in the past is worth rewriting yet. */
export function belowFrontier(session: Session): boolean {
    const record = epochRecord(session);
    return record !== undefined && record.frontier === undefined;
}
/** A proxy tool's change to the session state. Inside an epoch it lands in `pending` and the session keeps the epoch's
 *  state, so the next request still shares the cached prefix; the change appears when the frontier moves. */
export function changeAtFrontier<T>(session: Session, change: (deferred: boolean) => T): T {
    const record = epochRecord(session);
    if (!record?.frontier) return change(false);
    const view = session.state, start = record.pending ? withPending(view, record.pending) : view;
    session.state = start;
    try {
        return change(true);
    } finally {
        const after = session.state;
        session.state = view;
        if (after !== start) {
            record.pending = { blocks: after.blocks, absorbed: after.absorbed, nextBlockId: after.nextBlockId, nextRunId: after.nextRunId,
                deadRefs: after.deadRefs, terminalStreak: after.terminalStreak, stats: after.stats };
            save(session, record);
        }
    }
}
/** A change that does not alter the view (a flag) applies to the session and to the pending state alike. */
export function alsoPending(session: Session, change: (state: CompressionState) => CompressionState): void {
    const record = epochRecord(session);
    if (!record?.pending) return;
    const changed = change(withPending(session.state, record.pending));
    record.pending = { ...record.pending, blocks: changed.blocks };
    save(session, record);
}

const signedReasoning = (message: CoreMessage): boolean => {
    const raw = message as { thinkingSignature?: string; rawAnthropicBlock?: { type?: unknown } };
    return message.role === 'assistant' && ((message.contentType === 'reasoning' && !!raw.thinkingSignature) || raw.rawAnthropicBlock?.type === 'redacted_thinking');
};
const fingerprint = (message: CoreMessage): string => createHash('sha256').update(JSON.stringify([message.id, message.role, message.contentType, message.text ?? '',
    message.toolCallId, message.toolName, (message as { thinkingSignature?: string }).thinkingSignature, (message as { rawAnthropicBlock?: unknown }).rawAnthropicBlock])).digest('base64url').slice(0, 16);
/** Compares the view with what the last request sent. Signed reasoning of the past, from the first message that changed
 *  on, is removed whole and stays removed: a rewritten past never carries it (`keep` excepted). */
export function settleView(session: Session, messages: CoreMessage[], past: number, keep: ReadonlySet<string>): CoreMessage[] {
    const record = epochRecord(session);
    if (!record) return messages;
    const stripped = new Set(record.stripped ?? []);
    let view = messages, end = past;
    if (stripped.size > 0) {
        view = messages.filter(message => !stripped.has(message.id) || keep.has(message.id));
        end = past - (messages.length - view.length);
    }
    let prints = view.map(fingerprint);
    const sent = record.sent ?? [];
    let changed = 0;
    while (changed < sent.length && changed < prints.length && sent[changed] === prints[changed]) changed++;
    if (changed < sent.length) {
        const removed = new Set(view.slice(changed, end).filter(message => signedReasoning(message) && !keep.has(message.id)).map(message => message.id));
        if (removed.size > 0) {
            for (const id of removed) stripped.add(id);
            const kept: CoreMessage[] = [], keptPrints: string[] = [];
            view.forEach((message, at) => { if (!removed.has(message.id)) { kept.push(message); keptPrints.push(prints[at]!); } });
            view = kept;
            prints = keptPrints;
            record.stripped = [...stripped];
        }
    }
    record.sent = prints;
    save(session, record);
    return view;
}
export function lastAssistantIds(messages: readonly CoreMessage[]): Set<string> {
    const group = wireGroups(messages).filter(({ start }) => messages[start]!.role === 'assistant').at(-1);
    return new Set(group ? messages.slice(group.start, group.end).map(message => message.id) : []);
}

/** The invitation to compress, at the frontier: the same text for the whole epoch. */
export function frontierNudge(tokens: number, threshold: number, step: boolean): string {
    const k = (n: number) => `${(n / 1000).toFixed(1)}K`;
    const lead = step
        ? `The history up to this step holds about ${k(tokens)} tokens, near the model's window. Compress large, consumed ranges of it with the compress tool, using the mNNNNN references its messages carry; the latest step stays as it is.`
        : `The history before this turn holds about ${k(tokens)} tokens, past the ${k(threshold)} pressure threshold. Compress large, consumed ranges of it with the compress tool, using the mNNNNN references its messages carry; this turn stays as it is.`;
    return `${lead}\n\n${defaultPrompts.compressPhilosophy}`;
}
export const FRONTIER_NUDGE_ID = 'acp_nudge';

/** The view's own changes apply only up to the frontier: the messages after it go out as they came. */
export function beforeFrontier<T>(turn: { epoch?: EpochView }, messages: T[], change: (past: T[]) => T[]): T[] {
    if (!turn.epoch) return change(messages);
    const cut = messages.length - turn.epoch.tail;
    return [...change(messages.slice(0, cut)), ...messages.slice(cut)];
}
/** The token count the view's changes read: fixed for an epoch, so they cannot appear in the middle of one. */
export const viewTokenCount = (turn: { epoch?: EpochView }, tokenCount: number): number => turn.epoch?.tokens ?? tokenCount;
