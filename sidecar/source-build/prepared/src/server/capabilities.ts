// Per-session capabilities report (GET /__bili/capabilities?native=<id>).
//
// Two things live here. The gate deciders compute, at the exact point the proxy
// stamps a session's CCR / acp_read_original_image arming, both the decision and
// the closed-set reason for it; the request stamps the pair and the report only
// ever reads it back. The report builder is pure: it never re-evaluates a gate.
// Imports are type-only so the module loads under a bare `node --test`.
import type { Session } from "../session.js";
import type { EncoderState } from "../encoder-status.js";

/** Closed set. A reason is stamped by the request that decided the gate. */
export type CapabilityReason =
    | "none"
    | "not_configured"
    | "inject_tool_off"
    | "codex_host"
    | "plugin_wire_not_allowed"
    | "text_protocol"
    | "passthrough"
    | "encoder_missing"
    | "not_observed";

export interface GateDecision {
    armed: boolean;
    reason: CapabilityReason;
}

/** Inputs of the CCR gate: exactly the terms of the expression the stamp used before. */
export interface CcrGateInput {
    /** opts.compress.injectTool */
    injectTool: boolean;
    /** False when the responses text/marker protocol (or ACP_NO_INJECT_TOOL) closes the channel. */
    storeChannelOk: boolean;
    /** True when the channel is closed by ACP_NO_INJECT_TOOL rather than by a text protocol. */
    noInjectToolKnob: boolean;
    /** protocol === "responses" && codex host headers */
    codexHost: boolean;
    pluginMode: boolean;
    /** ccrPluginWireOk(protocol) */
    pluginWireOk: boolean;
    /** The CCR block this lane arms from has enabled === true (base block in plugin mode, resolved block otherwise). */
    ccrEnabled: boolean;
}

/** Same truth table as `injectTool && storeChannelOk && !codexHost ? pluginCcrStamp : undefined`,
 *  where pluginCcrStamp is defined only when the lane's CCR block is enabled (and, in plugin
 *  mode, the wire is a plugin-CCR wire). The first failing term names the reason. */
export function decideCcrGate(i: CcrGateInput): GateDecision {
    if (!i.injectTool) return { armed: false, reason: "inject_tool_off" };
    if (!i.storeChannelOk) return { armed: false, reason: i.noInjectToolKnob ? "inject_tool_off" : "text_protocol" };
    if (i.codexHost) return { armed: false, reason: "codex_host" };
    if (i.pluginMode && !i.pluginWireOk) return { armed: false, reason: "plugin_wire_not_allowed" };
    if (!i.ccrEnabled) return { armed: false, reason: "not_configured" };
    return { armed: true, reason: "none" };
}

export interface ImageToolGateInput {
    /** imageRescaleEnabled() */
    rescaleEnabled: boolean;
    injectTool: boolean;
    storeChannelOk: boolean;
    noInjectToolKnob: boolean;
    codexHost: boolean;
    pluginMode: boolean;
}

/** Same truth table as `rescaleEnabled && !codexHost && ((injectTool && storeChannelOk) || pluginMode)`. */
export function decideImageToolGate(i: ImageToolGateInput): GateDecision {
    if (!i.rescaleEnabled) return { armed: false, reason: "not_configured" };
    if (i.codexHost) return { armed: false, reason: "codex_host" };
    if (i.pluginMode || (i.injectTool && i.storeChannelOk)) return { armed: true, reason: "none" };
    if (!i.injectTool) return { armed: false, reason: "inject_tool_off" };
    return { armed: false, reason: i.noInjectToolKnob ? "inject_tool_off" : "text_protocol" };
}

/** What the last served request decided. In memory only: it never enters the persisted session. */
export interface CapabilityStamp {
    ccr: { configured: boolean; applied: boolean; reason: CapabilityReason };
    readOriginalImage: { applied: boolean; reason: CapabilityReason };
    /** session.stats.requests when the stamp was written. The stamping site runs before prepare*
     *  counts the request, so the stamp describes the latest request exactly when the report's
     *  `requests` equals stampedAtRequest + 1; a larger gap means a later request skipped the
     *  stamping site (hop marker, chain skip) and the stamp is older than the report. */
    stampedAtRequest: number;
}

const MAX_STAMPS = 2048;
const stamps = new Map<string, CapabilityStamp>();

/** Record the gates this request decided (latest wins). Called at the same site as the arming stamps. */
export function stampCapabilities(session: Pick<Session, "id" | "stats">, ccr: GateDecision & { configured: boolean }, image: GateDecision): void {
    stamps.delete(session.id);
    stamps.set(session.id, {
        ccr: { configured: ccr.configured, applied: ccr.armed, reason: ccr.reason },
        readOriginalImage: { applied: image.armed, reason: image.reason },
        stampedAtRequest: session.stats.requests,
    });
    if (stamps.size > MAX_STAMPS) {
        const oldest = stamps.keys().next().value;
        if (oldest !== undefined) stamps.delete(oldest);
    }
}

export function readCapabilityStamp(sessionId: string): CapabilityStamp | undefined {
    return stamps.get(sessionId);
}

export function clearCapabilityStampsForTest(): void {
    stamps.clear();
}

export interface CapabilityInputs {
    session: Pick<Session, "id" | "stats" | "meta" | "metadata">;
    nativeId: string;
    passthrough: boolean;
    /** opts.compress, read live: only the parts the report prints. */
    compress: {
        absorb?: { enabled?: boolean; mediaOnly?: boolean };
        tiers?: boolean;
        outputSteering?: { enabled?: boolean; verbosityLevel?: unknown; effortRouting?: unknown };
        ccr?: { enabled?: boolean };
    };
    promptRewriteMode: "auto" | "off" | undefined;
    /** imageRescaleEnabled() */
    imageRescaleEnabled: boolean;
    encoder: EncoderState;
}

const DEFAULT_VERBOSITY = 2;

function steeringLevel(raw: unknown): number {
    return typeof raw === "number" && Number.isInteger(raw) && raw >= 0 && raw <= 4 ? raw : DEFAULT_VERBOSITY;
}

function profileOf(metadata: Record<string, unknown> | undefined): "analysis" | "build" | null {
    const raw = metadata?.["musashiTerminalContextProfile"];
    const name = raw !== null && typeof raw === "object" ? (raw as { name?: unknown }).name : undefined;
    return name === "analysis" || name === "build" ? name : null;
}

/** The report for one known session. Pure: no gate is evaluated here, `applied` and `reason`
 *  of ccr / readOriginalImage come only from the stamp the last served request left. */
export function buildCapabilities(i: CapabilityInputs): Record<string, unknown> {
    const stamp = readCapabilityStamp(i.session.id);
    // Global passthrough means the pipeline (and so every gate) is bypassed: nothing is applied.
    const passthroughReason: CapabilityReason = "passthrough";
    const notObserved: CapabilityReason = "not_observed";
    const ccrSeen = stamp?.ccr ?? { configured: i.compress.ccr?.enabled === true, applied: false, reason: notObserved };
    const imageSeen = stamp?.readOriginalImage ?? { applied: false, reason: notObserved };
    const ccr = i.passthrough ? { ...ccrSeen, applied: false, reason: passthroughReason } : ccrSeen;
    const readOriginalImage = i.passthrough ? { applied: false, reason: passthroughReason } : imageSeen;
    const rescale: { configured: boolean; applied: boolean; reason: CapabilityReason } = !i.imageRescaleEnabled
        ? { configured: false, applied: false, reason: "not_configured" }
        : i.encoder === "loaded"
            ? { configured: true, applied: true, reason: "none" }
            : i.encoder === "missing"
                ? { configured: true, applied: false, reason: "encoder_missing" }
                : { configured: true, applied: false, reason: "not_observed" };
    const steering = i.compress.outputSteering;
    return {
        schemaVersion: 1,
        sessionId: i.session.id,
        nativeId: i.nativeId,
        protocol: i.session.meta.protocol ?? null,
        requests: i.session.stats.requests,
        stampedAtRequest: stamp?.stampedAtRequest ?? null,
        profile: profileOf(i.session.metadata),
        passthrough: i.passthrough,
        ccr,
        readOriginalImage,
        imageRescale: rescale,
        absorb: { configured: i.compress.absorb?.enabled === true, mediaOnly: i.compress.absorb?.mediaOnly ?? true },
        tiers: { configured: i.compress.tiers === true },
        steering: {
            configured: steering?.enabled === true,
            level: steeringLevel(steering?.verbosityLevel),
            // What the process uses: the kernel resolver defaults effortRouting to true, but
            // lowerEffort needs `enabled && effortRouting`, so a disabled/unset block routes nothing.
            effortRouting: steering?.enabled === true && steering.effortRouting !== false,
        },
        promptRewrite: { mode: i.promptRewriteMode ?? "off" },
    };
}

export interface CapabilitiesDeps extends Omit<CapabilityInputs, "session" | "nativeId"> {
    resolveSession(nativeId: string): CapabilityInputs["session"] | undefined;
}

/** Route body for GET /__bili/capabilities. An absent or unknown id is a 404, never a default report. */
export function capabilitiesResponse(url: string, deps: CapabilitiesDeps): { status: 200 | 404; body: Record<string, unknown> } {
    const nativeId = new URL(url, "http://localhost").searchParams.get("native");
    const session = nativeId ? deps.resolveSession(nativeId) : undefined;
    if (!nativeId || !session) return { status: 404, body: { error: "unknown session" } };
    return { status: 200, body: buildCapabilities({ ...deps, session, nativeId }) };
}
