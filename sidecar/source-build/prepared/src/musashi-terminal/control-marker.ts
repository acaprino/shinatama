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
        if (controlTextHash(candidate) === parsed[2]) return { marker: { kind, index: human.index, text: canonicalControlText(candidate) } };
    }
    return { reason: 'mismatch', kind };
}

/** True for the core messages flattened from the wire message that carries the control text. */
export function isControlMessage(message: CoreMessage, marker: ControlMarker): boolean {
    if (message.role !== 'user') return false;
    if (marker.index !== undefined) return (message as CoreMessage & { wireIndex?: number }).wireIndex === marker.index;
    return message.contentType === 'text' && canonicalControlText(message.text ?? '') === marker.text;
}

/** Message-level protection: the kernel gives a protected message no ref, so it is never tagged, never folded and
 *  never stored by CCR, whatever the pressure. It composes with any protection already present. */
export function protectControlMessage(config: Config, marker: ControlMarker): Config {
    const previous = config.isMessageProtected;
    return { ...config, isMessageProtected: message => isControlMessage(message, marker) || previous?.(message) === true };
}
