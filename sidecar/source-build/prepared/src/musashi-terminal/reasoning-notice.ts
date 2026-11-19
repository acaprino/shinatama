import type { WireProtocol } from "../util.js";

export const REASONING_ONLY_TURN_NOTE = "[Musashi Terminal] The turn ended without a final answer.";

type Row = Record<string, unknown>;
const row = (value: unknown): value is Row => value !== null && typeof value === "object" && !Array.isArray(value);
const nonblank = (value: unknown): boolean => typeof value === "string" && value.trim().length > 0;
const absent = (value: unknown): boolean => value === undefined || value === null;
const hasCalls = (value: unknown): boolean => !absent(value) && (!Array.isArray(value) || value.length > 0);

function emptyTextContent(content: unknown, textType: string): boolean {
    if (absent(content)) return true;
    if (typeof content === "string") return !nonblank(content);
    return Array.isArray(content) && content.every(part =>
        row(part) && part.type === textType && typeof part.text === "string" && !nonblank(part.text));
}

function appendOpenai(json: Row): boolean {
    if (!Array.isArray(json.choices) || json.choices.length === 0) return false;
    let target: Row | undefined;
    for (const choice of json.choices) {
        if (!row(choice) || choice.finish_reason !== "stop" || !row(choice.message)) return false;
        const message = choice.message;
        if (message.role !== "assistant" || !emptyTextContent(message.content, "text") ||
            (!absent(message.refusal) && message.refusal !== "") || hasCalls(message.tool_calls) ||
            !absent(message.function_call) || !absent(message.audio)) return false;
        if (nonblank(message.reasoning_content) || nonblank(message.reasoning)) target ??= message;
    }
    if (!target) return false;
    if (Array.isArray(target.content)) target.content.push({ type: "text", text: REASONING_ONLY_TURN_NOTE });
    else target.content = REASONING_ONLY_TURN_NOTE;
    return true;
}

function appendAnthropic(json: Row): boolean {
    if ((json.stop_reason !== "end_turn" && json.stop_reason !== "stop_sequence") ||
        json.role !== "assistant" || !Array.isArray(json.content)) return false;
    let reasoning = false;
    for (const part of json.content) {
        if (!row(part)) return false;
        if (part.type === "thinking" || part.type === "redacted_thinking") reasoning = true;
        else if (part.type !== "text" || typeof part.text !== "string" || nonblank(part.text)) return false;
    }
    if (!reasoning) return false;
    json.content.push({ type: "text", text: REASONING_ONLY_TURN_NOTE });
    return true;
}

function appendResponses(json: Row): boolean {
    if (json.status !== "completed" || !absent(json.incomplete_details) ||
        nonblank(json.output_text) || !Array.isArray(json.output)) return false;
    let reasoning = false;
    let message: Row | undefined;
    for (const item of json.output) {
        if (!row(item)) return false;
        if (item.type === "reasoning") { reasoning = true; continue; }
        // Tools, refusals, media and unknown outputs retain the native result.
        if (item.type !== "message" || item.role !== "assistant" ||
            (item.status !== undefined && item.status !== "completed") ||
            !Array.isArray(item.content) || !emptyTextContent(item.content, "output_text")) return false;
        message ??= item;
    }
    if (!reasoning) return false;
    const text = { type: "output_text", text: REASONING_ONLY_TURN_NOTE, annotations: [] };
    if (message) (message.content as unknown[]).push(text);
    else json.output.push({
        id: `msg-proxy-${Date.now()}-notice`,
        type: "message", role: "assistant", status: "completed", content: [text],
    });
    return true;
}

function appendGoogle(json: Row): boolean {
    if (!absent(json.promptFeedback) && (!row(json.promptFeedback) || !absent(json.promptFeedback.blockReason))) return false;
    if (!Array.isArray(json.candidates) || json.candidates.length === 0) return false;
    let target: unknown[] | undefined;
    for (const candidate of json.candidates) {
        if (!row(candidate) || candidate.finishReason !== "STOP" ||
            !row(candidate.content) || candidate.content.role !== "model" ||
            !Array.isArray(candidate.content.parts)) return false;
        let reasoning = false;
        for (const part of candidate.content.parts) {
            if (!row(part)) return false;
            if (Object.keys(part).some(key => !["text", "thought", "thoughtSignature", "thought_signature"].includes(key))) return false;
            if (part.thought === true) reasoning = true;
            else if (typeof part.text !== "string" || nonblank(part.text)) return false;
        }
        if (reasoning) target ??= candidate.content.parts;
    }
    if (!target) return false;
    target.push({ text: REASONING_ONLY_TURN_NOTE });
    return true;
}

/** Gemini's streaming endpoint can return a JSON array instead of SSE.
 *  Classify the whole turn before adding to its terminal candidate. */
function appendGoogleArray(chunks: unknown[]): boolean {
    const candidates = new Map<number, { parts: unknown[]; terminal?: Row; reason?: unknown }>();
    for (const chunk of chunks) {
        if (!row(chunk) || !absent(chunk.error) || !absent(chunk.errors) ||
            (row(chunk.promptFeedback) && !absent(chunk.promptFeedback.blockReason))) return false;
        if (!Array.isArray(chunk.candidates)) continue;
        for (const [at, candidate] of chunk.candidates.entries()) {
            if (!row(candidate)) return false;
            const index = typeof candidate.index === "number" ? candidate.index : at;
            const state = candidates.get(index) ?? { parts: [] };
            if (row(candidate.content)) {
                if (candidate.content.role !== "model" || !Array.isArray(candidate.content.parts)) return false;
                state.parts.push(...candidate.content.parts);
            }
            if (candidate.finishReason) { state.terminal = candidate; state.reason = candidate.finishReason; }
            candidates.set(index, state);
        }
    }
    if (!candidates.size || [...candidates.values()].some(state => !state.terminal)) return false;
    const merged = { candidates: [...candidates.values()].map(state => ({
        finishReason: state.reason, content: { role: "model", parts: [...state.parts] },
    })) };
    if (!appendGoogle(merged)) return false;
    const index = merged.candidates.findIndex(candidate => candidate.content.parts.some(part => row(part) && part.text === REASONING_ONLY_TURN_NOTE));
    const terminal = [...candidates.values()][index]!.terminal!;
    const content = row(terminal.content) ? terminal.content : { role: "model", parts: [] };
    const parts = Array.isArray(content.parts) ? content.parts : [];
    terminal.content = { ...content, parts: [...parts, { text: REASONING_ONLY_TURN_NOTE }] };
    return true;
}

/** Call after prose filtering. Only an explicit clean reasoning-only completion
 *  receives a local note; status, reasoning and provider usage stay untouched. */
export function appendReasoningOnlyNotice(json: Row | unknown[], protocol: WireProtocol | undefined): boolean {
    if (Array.isArray(json)) return protocol === "google" && appendGoogleArray(json);
    if (!absent(json.error) || !absent(json.errors)) return false;
    switch (protocol) {
        case "openai": return appendOpenai(json);
        case "anthropic": return appendAnthropic(json);
        case "responses": return appendResponses(json);
        case "google": return appendGoogle(json);
        default: return false;
    }
}
