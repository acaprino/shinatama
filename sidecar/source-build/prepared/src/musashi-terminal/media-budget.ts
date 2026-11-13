import type { CoreMessage } from 'acp-kernel';
import { anthropicToCore, coreToAnthropic, coreToGoogle, coreToOpenai, coreToResponses, googleToCore, openaiToCore, responsesToCore, type AnthropicRequestBody, type BiliMessage, type GoogleRequestBody, type OpenAIRequestBody, type ResponsesRequestBody } from 'acp-kernel/wire';
import type { WireProtocol } from '../util.js';
import { modelVisibleTools } from '../server/tool-budget.js';

type Row = Record<string, unknown>;
const object = (value: unknown): value is Row => value !== null && typeof value === 'object' && !Array.isArray(value);
const sidecars = new Set(['imageBase64', 'rawAnthropicBlock', 'rawOpenaiContent', 'rawOpenaiContentParts', 'rawOpenaiReasoningDetails', 'rawResponsesItem', 'rawGoogleParts', 'reasoningContent', 'thinkingSignature', 'googleThoughtSignature']);

/** JSON transport structure and text, excluding media charged by the image channel
 * and opaque signatures. No wire or input object is modified. */
export function packetTextBytes(body: object): number {
    return Buffer.byteLength(JSON.stringify(body, function (key, value: unknown): unknown {
        if ((key === 'thoughtSignature' || key === 'thought_signature') && typeof value === 'string') return '';
        if (!object(value)) return value;
        if (value.type === 'image_url' || value.type === 'input_image') return { ...value, image_url: '' };
        if (value.type === 'image' && object(value.source)) return { ...value, source: { ...value.source, data: '', url: '' } };
        if (value.type === 'file' && object(value.file)) return { ...value, file: { ...value.file, file_data: '', url: '' } };
        if (object(value.inlineData) || object(value.fileData) || object(value.inline_data) || object(value.file_data)) {
            return { ...value, ...(object(value.inlineData) ? { inlineData: { ...value.inlineData, data: '' } } : {}),
                ...(object(value.fileData) ? { fileData: { ...value.fileData, fileUri: '' } } : {}),
                ...(object(value.inline_data) ? { inline_data: { ...value.inline_data, data: '' } } : {}),
                ...(object(value.file_data) ? { file_data: { ...value.file_data, file_uri: '' } } : {}) };
        }
        if (value.type === 'thinking') return { ...value, signature: '' };
        if (value.type === 'redacted_thinking') return { ...value, data: '' };
        // Musashi Terminal: encrypted Chat proof consumes transport bytes, not visible-text tokens.
        if (value.type === 'reasoning.encrypted' && typeof value.data === 'string') return { ...value, data: '' };
        if (value.type === 'reasoning' && typeof value.encrypted_content === 'string') return { ...value, encrypted_content: '' };
        return value;
    }), 'utf8');
}

/** Model-visible packet estimate, not transport size. Only the Responses tool
 * catalog can be deferred by this projection. Definitions loaded into input
 * remain fully counted, as do namespace descriptions and all eager tools.
 * Other protocols keep their conservative estimate; no wire object is changed. */
export function modelVisiblePacketTextBytes(body: object, protocol: WireProtocol): number {
    const packet = body as Row;
    return packetTextBytes(protocol === 'responses' && Array.isArray(packet.tools)
        ? { ...packet, tools: modelVisibleTools(packet.tools) } : body);
}

/** Wire sidecars duplicate the core's text/media; they cannot fund a summary. */
export function coreMessageTextBytes(message: CoreMessage): number {
    return packetTextBytes(Object.fromEntries(Object.entries(message).filter(([key]) => !sidecars.has(key))));
}

/** Rebuild only the conversation view so folded images are no longer reserved. */
export function messageViewBody(body: object, protocol: WireProtocol, messages: CoreMessage[]): Row {
    const visible = messages as BiliMessage[];
    switch (protocol) {
        case 'anthropic': return { ...body, messages: coreToAnthropic(visible) };
        case 'openai': return { ...body, messages: coreToOpenai(visible) };
        case 'responses': return { ...body, input: coreToResponses(visible) };
        case 'google': return { ...body, contents: coreToGoogle(visible) };
    }
}

/** Host-projected hidden thinking is paid only while its owner survives on wire. */
export function survivingHiddenThinking(messages: CoreMessage[], body: object, protocol: WireProtocol): number {
    if (!messages.some(message => typeof message.thinkingTokens === 'number' && Number.isFinite(message.thinkingTokens) && message.thinkingTokens > 0)) return 0;
    let visible: BiliMessage[];
    switch (protocol) {
        case 'anthropic': visible = anthropicToCore(body as AnthropicRequestBody).msgs; break;
        case 'openai': visible = openaiToCore(body as OpenAIRequestBody).msgs; break;
        case 'responses': visible = responsesToCore(body as ResponsesRequestBody).msgs; break;
        case 'google': visible = googleToCore(body as GoogleRequestBody).msgs; break;
    }
    return messages.reduce((sum, core) => {
        const amount = core.thinkingTokens;
        if (typeof amount !== 'number' || !Number.isFinite(amount) || amount <= 0) return sum;
        const message = core as BiliMessage;
        const survives = visible.some(next => next.role === message.role && next.contentType === message.contentType &&
            (message.thinkingSignature ? next.thinkingSignature === message.thinkingSignature
                : message.googleThoughtSignature ? next.googleThoughtSignature === message.googleThoughtSignature
                    : message.text ? next.text?.includes(message.text) : next.text === message.text));
        return sum + (survives ? amount : 0);
    }, 0);
}
