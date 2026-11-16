// Musashi Terminal: on adaptive sessions with a known capacity the past changes only when the frontier moves
// (epoch.ts); unknown capacity and disabled policies keep the ordinary kernel turn.
import { createContentStore, createRenderRefsNode, defaultCountTokens, runPipeline, storeCoveredOriginals, syncBlocks, type CompressionCore, type CompressionState, type MessageContentStore, type CoreMessage, type ProcessTurnInput, type ProcessTurnResult } from 'acp-kernel';
import type { ResolvedKernelConfig } from '../compress-settings.js';
import type { Session } from '../session.js';
import { estimateWireOverhead } from '../server/budget.js';
import { outputBudgetField, readOutputBudget } from '../server/side-request.js';
import type { WireProtocol } from '../util.js';
import { normalizeToolName } from '../tool-ring.js';
import { adaptivePressureThreshold, type AdaptivePressureSettings } from './adaptive-pressure.js';
import { protectAdaptiveMessages } from './request-protection.js';
import { adoptContentStore, ccrEnabled, contentStoreOf } from '../store.js';
import { advanceFrontier, DEFAULT_CACHE_TTL_SECONDS, EMERGENCY_WINDOW_RATIO, emergencyFrontier, endEpochs, epochRecord, FRONTIER_NUDGE_ID, frontierEnd, frontierNudge, frontierSettings, lastAssistantIds, materializePending, settleFrontier, settleView, tokensOf, touchEpoch, type EpochView, type FrontierSettings } from './epoch.js';

type AdaptiveTurn = ProcessTurnResult & { epoch?: EpochView };
/** `injectsCompress`: the proxy adds its compress tool to this request's tools; `injectsNudge: false`: the settings turn
 *  the proxy's invitations off. */
type TurnContext = { session: Session; body: object; protocol: WireProtocol; nativeWindow: number; enabled: boolean; inheritedCompress?: boolean; injectsCompress?: boolean; injectsNudge?: boolean };

function finalOutputTokens(body: Record<string, unknown>, protocol: WireProtocol): number {
    // OpenAI's effective completion alias outranks its deprecated max_tokens field.
    if (protocol === 'openai' && typeof body.max_completion_tokens === 'number') return body.max_completion_tokens;
    const field = outputBudgetField(body);
    return (field ? readOutputBudget(body, field) : undefined) ?? 0;
}

function object(value: unknown): value is Record<string, unknown> { return value !== null && typeof value === 'object' && !Array.isArray(value); }
export function compressDeclared(tools: unknown): boolean {
    if (!Array.isArray(tools)) return false;
    return tools.some(tool => {
        if (!object(tool)) return false;
        if (tool.type === 'namespace') return compressDeclared(tool.tools);
        if (Array.isArray(tool.functionDeclarations)) return compressDeclared(tool.functionDeclarations);
        const fn = object(tool.function) ? tool.function : tool;
        return typeof fn.name === 'string' && (tool.type === undefined || tool.type === 'function' || tool.type === 'custom')
            && normalizeToolName(fn.name) === 'compress';
    });
}
/** An invitation to compress is useful only when the request can execute compress. */
function canUseCompress(body: Record<string, unknown>, protocol: WireProtocol, inheritedCompress = false): boolean {
    if (!compressDeclared(body.tools) && !(body.tools === undefined && inheritedCompress)) return false;
    const choice = body.tool_choice;
    if (choice !== undefined && !['auto', 'any', 'required'].includes(String(choice))) {
        if (!object(choice)) return false;
        if (!['auto', 'any', 'required'].includes(String(choice.type))) {
            if (choice.type === 'allowed_tools') {
                if (choice.mode === 'none' || !compressDeclared(choice.tools)) return false;
            } else {
                const forced = object(choice.function) ? choice.function.name : choice.name;
                if (!['function', 'tool', 'custom'].includes(String(choice.type)) || typeof forced !== 'string' || normalizeToolName(forced) !== 'compress') return false;
            }
        }
    }
    if (protocol === 'google' && object(body.toolConfig) && object(body.toolConfig.functionCallingConfig)) {
        const calling = body.toolConfig.functionCallingConfig;
        if (calling.mode === 'NONE') return false;
        if (calling.allowedFunctionNames !== undefined && (!Array.isArray(calling.allowedFunctionNames)
            || !calling.allowedFunctionNames.some(name => typeof name === 'string' && normalizeToolName(name) === 'compress'))) return false;
    }
    return true;
}

export function processAdaptiveTurn(core: CompressionCore, input: ProcessTurnInput, context: TurnContext): AdaptiveTurn {
    const settings = (input.config as ResolvedKernelConfig).adaptivePressure;
    const sourceKind = context.session.metadata.lastWindowSource;
    if (!settings?.enabled || !sourceKind || sourceKind === 'fallback' || sourceKind === 'unknown' || sourceKind === 'default') {
        endEpochs(context.session);
        return core.processTurn({ ...input, state: context.session.state });
    }
    if (!context.enabled) return core.processTurn(input);
    // A window too small for the curve has no frontier: the ordinary kernel turn, as with an unknown capacity.
    return processEpochTurn(core, input, context, settings) ?? core.processTurn({ ...input, state: context.session.state });
}

/** The frontier rule (epoch.ts): the past up to the frontier goes out as the epoch's view, the rest as it came, and the
 *  invitation to compress stands at the frontier. Below the first frontier nothing is rewritten. */
function processEpochTurn(core: CompressionCore, input: ProcessTurnInput, context: TurnContext, settings: AdaptivePressureSettings): AdaptiveTurn | undefined {
    const body = context.body as Record<string, unknown>;
    // An allowance above a quarter of the window counts as a quarter: the outgoing clamp keeps input plus output inside
    // the window anyway, and an impossible request must not leave the session without a threshold.
    const output = Math.min(finalOutputTokens(body, context.protocol) || 8192, Math.floor(context.nativeWindow / 4));
    const config = input.config as ResolvedKernelConfig;
    const threshold = adaptivePressureThreshold({ contextTokens: context.nativeWindow, outputTokens: output, ratio: config.nudge.maxContextLimitPct, maxTokens: settings.maxTokens });
    if (threshold === undefined) { endEpochs(context.session); return undefined; }
    const usable = context.nativeWindow - output - Math.max(4096, Math.ceil(context.nativeWindow * 0.05));
    const frontier = frontierSettings(threshold, usable, settings.maxTokens, settings.epochTokens);
    const overhead = estimateWireOverhead(context.protocol, body);
    const { session } = context;
    // A Threads continue carries only the delta: the stored past cannot change here, and its human messages are judged
    // when a create brings the whole history again. The delta gets what every arriving message gets.
    const thread = body.thread as { type?: unknown } | undefined;
    if (thread?.type === 'continue') {
        touchEpoch(session, Date.now());
        const arrived = arrivalView(core, input.messages, session.state, input.config, session.contentStore ?? input.contentStore ?? createContentStore(), input.renderTags, epochRecord(session)?.frontier?.tokens ?? 0);
        return { messages: arrived.messages, state: arrived.state, contentStore: arrived.contentStore, nudge: undefined, epoch: { tail: arrived.messages.length, tokens: input.tokenCount ?? 0 } } as AdaptiveTurn;
    }
    // Measured as the messages arrive: refs and CCR placeholders, never the store it would write or the tags.
    const arrive = (messages: readonly CoreMessage[]) => arrivalView(core, [...messages], session.state, input.config,
        session.contentStore ?? input.contentStore ?? createContentStore(), 'none', 0).messages;
    const advanced = advanceFrontier(session, input.messages, { overhead, settings: frontier, model: typeof body.model === 'string' ? body.model : undefined,
        now: Date.now(), ttlMs: (settings.cacheTtlSeconds ?? DEFAULT_CACHE_TTL_SECONDS) * 1000, arrive });
    if (advanced.moved) land(session, input, materializePending(session, advanced.record));
    let turn = epochView(core, { ...input, contentStore: session.contentStore ?? input.contentStore }, context, frontier);
    const tokens = overhead + tokensOf(turn.messages);
    if (overflowing(session, tokens, Math.min(context.nativeWindow, config.modelContextLimit || context.nativeWindow))
        && emergencyFrontier(session, input.messages, tokens, frontier)) {
        land(session, input, materializePending(session, epochRecord(session)!));
        turn = epochView(core, { ...input, contentStore: session.contentStore ?? input.contentStore }, context, frontier);
    }
    // Signed reasoning after the first message the view changed is removed whole; in an emergency inside a turn the
    // last assistant message keeps its own, which the provider requires.
    const step = epochRecord(session)?.frontier?.step === true;
    turn.messages = settleView(session, turn.messages, turn.messages.length - turn.epoch!.tail, step ? lastAssistantIds(input.messages) : new Set());
    settleFrontier(session, turn.epoch!.invite === true);
    return turn;
}

/** Whether the proxy would have to fold this request before sending it. It judges the input against the window less the
 *  share kept for the output (`modelContextLimit`), and on the provider's last count as well as the local one: the
 *  provider bills images and reasoning the local count does not see. From here on the past must carry every accepted
 *  fold, whatever the cache costs: folds held behind the frontier leave the proxy nothing to fit the request with, and it
 *  refuses it. */
function overflowing(session: Session, tokens: number, window: number): boolean {
    const { lastInputTokens, lastInputTokensSource } = session.stats;
    const billed = lastInputTokensSource === 'usage' || lastInputTokensSource === 'overflow-arm' ? lastInputTokens : 0;
    return Math.max(tokens, billed) >= EMERGENCY_WINDOW_RATIO * window;
}

function epochView(core: CompressionCore, input: ProcessTurnInput, context: TurnContext, settings: FrontierSettings): AdaptiveTurn {
    const { session } = context, frontier = epochRecord(session)?.frontier;
    const end = frontier ? frontierEnd(input.messages, frontier.id) : undefined;
    const cut = frontier && end !== undefined ? end : 0;
    const tail = input.messages.slice(cut);
    let past: CoreMessage[] = [], state = session.state, contentStore = input.contentStore ?? createContentStore();
    let processed: ProcessTurnResult | undefined, invite = false;
    if (frontier && cut > 0) {
        const prefix = input.messages.slice(0, cut);
        // The frontier's human message is the turn the past is viewed from: protected like a current task.
        const config = protectAdaptiveMessages(input.config, prefix) as ResolvedKernelConfig;
        processed = core.processTurn({ ...input, messages: prefix, state, config, tokenCount: frontier.tokens, contentStore });
        ({ state, contentStore } = processed);
        past = processed.messages;
        // The kernel's nudge follows its own cadence: the invitation to compress belongs to the frontier instead,
        // decided once for the epoch and only where the request can execute a compress call.
        invite = frontier.invite ?? (frontier.tokens >= settings.threshold && frontier.cause !== 'compress' && context.injectsNudge !== false && compressCallable(context));
        if (invite) {
            const nudge = { id: FRONTIER_NUDGE_ID, role: 'user', contentType: 'text', text: frontierNudge(frontier.tokens, settings.threshold, frontier.step === true) } as CoreMessage;
            const at = frontier.step ? past.length : past.findIndex(message => message.id === frontier.id);
            past = at < 0 ? [...past, nudge] : [...past.slice(0, at), nudge, ...past.slice(at)];
        }
    }
    // The messages after the frontier get their refs, tags, stored large results and absorb invitations once, as they
    // arrive. None of these depends on a message's position, so every later request sends them unchanged.
    const tokenCount = frontier && cut > 0 ? frontier.tokens : 0;
    const arrived = tail.length > 0 ? arrivalView(core, tail, state, input.config, contentStore, input.renderTags, tokenCount) : { messages: [], state, contentStore };
    return { ...processed, messages: [...past, ...arrived.messages], state: wholeRequest(input.messages, arrived.state), contentStore: arrived.contentStore, nudge: undefined,
        epoch: { tail: arrived.messages.length, tokens: frontier && cut > 0 ? frontier.tokens : input.tokenCount ?? 0, invite } };
}

/** The state an ordinary kernel turn leaves for the whole request. The kernel ran over the past alone, so it took every
 *  message after the frontier for gone: it deactivated the folds that cover them and kept only the past as the inbound
 *  by which the next pass tells a folded message resent from a new copy (instance-reid.ts). The next pass (the proxy's
 *  preflight, or the next request once the frontier moves) then took those messages for new copies and renamed them:
 *  their folds never left the wire, and the ranges it offered for folding cited ids the request does not carry, so
 *  nothing could be folded and the request was refused. The view sent is unchanged: after the frontier the messages
 *  still go as they arrived, their folds waiting for it. */
function wholeRequest(messages: readonly CoreMessage[], state: CompressionState): CompressionState {
    return { ...syncBlocks([...messages], state).state, lastPassIds: messages.map(message => message.id) };
}

/** Folds that land with the frontier store their covered originals now, as an immediate fold does at commit
 *  (stream.ts, #1179), so retrieve-by-ref works for them. */
function land(session: TurnContext['session'], input: ProcessTurnInput, blockIds: string[] | undefined): void {
    if (blockIds && blockIds.length > 0 && ccrEnabled(session)) {
        adoptContentStore(session, storeCoveredOriginals(contentStoreOf(session), input.messages, session.state, blockIds, defaultCountTokens));
    }
}

/** Whether the request can call compress: declared by the client or added by the proxy, and not excluded by tool_choice. */
function compressCallable(context: TurnContext): boolean {
    const body = context.body as Record<string, unknown>;
    if (!context.injectsCompress) return canUseCompress(body, context.protocol, context.inheritedCompress);
    return canUseCompress({ ...body, tools: [...(Array.isArray(body.tools) ? body.tools : []), { name: 'compress' }] }, context.protocol);
}

function arrivalView(core: CompressionCore, messages: CoreMessage[], state: CompressionState, config: ProcessTurnInput['config'], contentStore: MessageContentStore, strategy: ProcessTurnInput['renderTags'], tokenCount: number) {
    const nodes = core.defaultNodes().filter(node => ['assign-refs', 'ccr-store', 'absorb-prompt'].includes(node.name));
    if (strategy !== 'none') nodes.push(createRenderRefsNode(strategy ?? 'all'));
    const out = runPipeline(nodes, { messages, state, effects: {} }, { config, tokenCount, countTokens: defaultCountTokens, contentStore });
    return { messages: out.messages, state: out.state, contentStore: (out.effects.ccr as { store?: MessageContentStore } | undefined)?.store ?? contentStore };
}
