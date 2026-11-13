import { createHash } from 'node:crypto';
import type { Config, CoreMessage } from 'acp-kernel';

/** Shinatama declares its own control messages (hand-off seed, bootstrap, receipt) with this request header:
 *  `x-shinatama-control: <kind>;<sha256 hex of the canonical text of the current human message>`.
 *  The marker is structural: it is never inferred from the prompt text, its length or max_tokens. The header is
 *  honoured only when its hash equals the text actually received; any other request, however the header got
 *  there, gets the ordinary treatment. */
export const CONTROL_MARKER_HEADER = 'x-shinatama-control';

export type ControlKind = 'seed' | 'bootstrap' | 'receipt';

/** The human message of a request as the wire shows it: every text part, their join, and the wire index of its owner. */
export interface ControlHuman {
    text: string;
    parts: readonly string[];
    index?: number;
}

export interface ControlMarker {
    kind: ControlKind;
    /** Wire index of the message that owns the control text; absent when the wire has no message array. */
    index?: number;
    /** Canonical text the hash matched. */
    text: string;
    /** Canonical text of every text part of the owning message and of the whole message: the cores that belong to it. */
    texts: readonly string[];
}

export type ControlOutcome =
    | { marker: ControlMarker; reason?: undefined }
    | { marker?: undefined; reason: 'absent' | 'malformed' | 'mismatch'; kind?: ControlKind };

const VALUE = /^(seed|bootstrap|receipt);([0-9a-f]{64})$/;

/** Line ends become `\n` and outer spaces, tabs and line ends are dropped: the terminal an input is pasted through may
 *  convert the first and trim the second. Shinatama hashes with the same rule (crates/hop/src/channel.rs). */
export function canonicalControlText(text: string): string {
    return text.replace(/\r\n?/g, '\n').replace(/^[ \t\n]+|[ \t\n]+$/g, '');
}

export function controlTextHash(text: string): string {
    return createHash('sha256').update(canonicalControlText(text), 'utf8').digest('hex');
}

export function matchControlMarker(header: string | string[] | undefined, human: ControlHuman | undefined): ControlOutcome {
    if (header === undefined) return { reason: 'absent' };
    const parsed = typeof header === 'string' ? VALUE.exec(header.trim().toLowerCase()) : null;
    if (!parsed) return { reason: 'malformed' };
    const kind = parsed[1] as ControlKind;
    if (!human) return { reason: 'mismatch', kind };
    // A client may send reminders or attachments as further text parts of the same message: the control text must be
    // exactly one part, or the whole message when it is a single string.
    for (const candidate of [human.text, ...human.parts]) {
        if (controlTextHash(candidate) === parsed[2]) return { marker: { kind, index: human.index, text: canonicalControlText(candidate), texts: [human.text, ...human.parts].map(canonicalControlText) } };
    }
    return { reason: 'mismatch', kind };
}

/** True for the text cores flattened from the wire message that carries the control text. The wire index is a position in
 *  the raw body and can shift when the pipeline splices messages out before the codec stamps it, so the index alone never
 *  selects a message: its text must also be one of the texts the hash vouched for. A shifted index protects nothing, which
 *  leaves that message with the ordinary treatment. */
export function isControlMessage(message: CoreMessage, marker: ControlMarker): boolean {
    if (message.role !== 'user' || message.contentType !== 'text') return false;
    const text = canonicalControlText(message.text ?? '');
    if (marker.index === undefined) return text === marker.text;
    return (message as CoreMessage & { wireIndex?: number }).wireIndex === marker.index && marker.texts.includes(text);
}

const MAX_LOGGED_MISMATCHES = 16;

/** A static header rides every request of a hand-off session, so a mismatch is the normal case from the second human turn
 *  on. The one that matters is a mismatch on the first human turn, where the declared message is expected: that is logged
 *  at info, once per distinct received text and at most MAX_LOGGED_MISMATCHES per session (`seen` is the session's set);
 *  every other mismatch is debug and never uses up the info budget. */
export function controlMismatchLevel(seen: Set<string>, receivedText: string | undefined, firstHumanTurn: boolean): 'info' | 'debug' {
    if (!firstHumanTurn) return 'debug';
    const key = receivedText === undefined ? '' : controlTextHash(receivedText);
    if (seen.has(key) || seen.size >= MAX_LOGGED_MISMATCHES) return 'debug';
    seen.add(key);
    return 'info';
}

/** Message-level protection: the kernel gives a protected message no ref, so it is never tagged, never folded and
 *  never stored by CCR, whatever the pressure. It composes with any protection already present. */
export function protectControlMessage(config: Config, marker: ControlMarker): Config {
    const previous = config.isMessageProtected;
    return { ...config, isMessageProtected: message => isControlMessage(message, marker) || previous?.(message) === true };
}
