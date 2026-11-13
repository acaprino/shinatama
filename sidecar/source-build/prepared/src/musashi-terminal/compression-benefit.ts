import {
    defaultCountTokens, hideAbsorbedMessages, prune, storeLargeResults,
    type CompressionState, type Config, type CoreMessage, type MessageContentStore,
} from "acp-kernel";
import type { AdaptivePressureSettings } from "./adaptive-pressure.js";
import { coreMessageTextBytes } from './media-budget.js';
import { imagePlaceholdersForSummary } from "../image-note.js";

export type CompressionBenefitSettings = Pick<AdaptivePressureSettings, "enabled" | "maxTokens">;
export interface CompressionViewInput {
    messages: CoreMessage[];
    state: CompressionState;
    config: Config;
    contentStore?: MessageContentStore;
}
export interface CompressionBenefitInput extends CompressionViewInput {
    nextState: CompressionState;
    settings?: CompressionBenefitSettings;
}
export interface CompressionBenefit {
    beforeBytes: number;
    afterBytes: number;
    savedBytes: number;
    minimumBytes: number;
    accepted: boolean;
}

export function adaptiveBenefitSettings(config: Config): CompressionBenefitSettings | undefined {
    return (config as Config & { adaptivePressure?: AdaptivePressureSettings }).adaptivePressure;
}

/** Read-only derived view: existing CCR originals retain their placeholders;
 * unseen results are never stored while checking a candidate's benefit. */
export function compressionView(input: CompressionViewInput): CoreMessage[] {
    const messages = prune(hideAbsorbedMessages(input.messages, input.state), input.state);
    if (input.config.ccr?.enabled !== true || !input.contentStore) return messages;
    return storeLargeResults({
        messages, state: input.state, store: input.contentStore,
        config: { ...input.config, ccr: { ...input.config.ccr, minToolTokens: Number.MAX_SAFE_INTEGER } },
        countTokens: defaultCountTokens,
    }).messages;
}

/** JSON wrappers and hidden thinking are part of both sides of the saving. */
export function compressionViewBytes(input: CompressionViewInput): number {
    return compressionView(input).reduce((bytes, message) => bytes + coreMessageTextBytes(message)
        + (typeof message.thinkingTokens === "number" && Number.isFinite(message.thinkingTokens) && message.thinkingTokens > 0 ? message.thinkingTokens * 4 : 0), 0);
}

export function minimumCompressionBenefitBytes(beforeBytes: number, settings?: CompressionBenefitSettings): number {
    return settings?.enabled ? 4 * Math.max(1000, Math.ceil(Math.min(beforeBytes / 4, settings.maxTokens) * 0.05)) : 1;
}

/** Shared by recommendation previews and publication; never commits state,
 * assigns refs, or changes the content store. nextState includes host footers. */
export function previewCompressionBenefit(input: CompressionBenefitInput): CompressionBenefit {
    const beforeBytes = compressionViewBytes(input);
    const afterBytes = compressionViewBytes({ ...input, state: input.nextState });
    const savedBytes = beforeBytes - afterBytes;
    const minimumBytes = minimumCompressionBenefitBytes(beforeBytes, input.settings ?? adaptiveBenefitSettings(input.config));
    return { beforeBytes, afterBytes, savedBytes, minimumBytes, accepted: savedBytes >= minimumBytes };
}

/** Host image reminders survive in summaries and must be priced by previews
 * and publication alike. Neither the input state nor the candidate is changed. */
export function withCompressionImageNotes(input: Pick<CompressionBenefitInput, "messages" | "state" | "nextState">): CompressionState {
    const previous = new Set(input.state.blocks.map((block) => block.blockId));
    let changed = false;
    const blocks = input.nextState.blocks.map((block) => {
        if (previous.has(block.blockId)) return block;
        const notes = block.directMessageIds.flatMap((id) => {
            const message = input.messages.find((message) => message.id === id);
            return message ? imagePlaceholdersForSummary(message, input.nextState.messageRefs?.byRaw[id]) : [];
        });
        if (notes.length === 0) return block;
        const footer = "[folded images: " + notes.join(" ") + " — decompress({ imageRef }) restores pixels]";
        if (block.summary.endsWith(footer)) return block;
        changed = true;
        return { ...block, summary: block.summary + "\n\n" + footer };
    });
    return changed ? { ...input.nextState, blocks } : input.nextState;
}
