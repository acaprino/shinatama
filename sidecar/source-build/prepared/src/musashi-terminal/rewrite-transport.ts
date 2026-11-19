import { createHash } from "node:crypto";
import type { IncomingMessage } from "node:http";
import type { ProxyOptions } from "../config.js";
import { buildForwardTarget, forwardUpstreamUrl, type resolveUpstream } from "../server.js";
import { applyLaneCredential, laneCredential } from "../lane-credentials.js";
import { fetchWithTimeout } from "../fetch-util.js";
import { proxyDispatcher } from "../upstream-proxy.js";
import { extractBillingAttributionBlock } from "../preflight.js";
import type { WireProtocol } from "../util.js";
import { isCodexRewriteBackend, normalizeRewriteCatalog, type RewriteModelCatalog, type RewriteModelScope, type RewriteOutputTokenField, type RewriteThinkingOff } from "./rewrite-models.js";
import { PROMPT_REWRITE_INSTRUCTIONS, PROMPT_REWRITE_MAX_BYTES, PROMPT_REWRITE_MAX_TOKENS } from "./prompt-rewrite-contract.js";

export type RewriteCompletion = { text: string; complete: boolean; toolCalls?: boolean };
export type RewriteTransport = {
    scope: RewriteModelScope;
    isCurrent(): boolean;
    listCatalog(signal: AbortSignal): Promise<RewriteModelCatalog | undefined>;
    complete(model: string, text: string, signal: AbortSignal, outputTokenField?: RewriteOutputTokenField, thinkingOff?: RewriteThinkingOff): Promise<RewriteCompletion>;
};
type TransportInput = {
    req: IncomingMessage; opts: ProxyOptions; route: ReturnType<typeof resolveUpstream>;
    protocol: WireProtocol; model: string; body: Record<string, unknown>;
    log(level: string, message: string): void;
};
type Snapshot = { url: URL; headers: Record<string, string>; proxyUrl: string | undefined; scope: RewriteModelScope };
type Row = Record<string, unknown>;
const row = (value: unknown): value is Row => value !== null && typeof value === "object" && !Array.isArray(value);
const hash = (text: string): string => createHash("sha256").update(text).digest("hex");
const TIMEOUT_MS = 3000;
const RESPONSE_BYTES = 24 * 1024;
const CATALOG_BYTES = 256 * 1024;
function header(headers: Record<string, string>, name: string): string | undefined {
    const key = Object.keys(headers).find(key => key.toLowerCase() === name);
    return key === undefined ? undefined : headers[key];
}
function accountMetadata(value: unknown): Row | undefined {
    if (!row(value)) return undefined;
    return Object.fromEntries(["account_id", "organization_id", "org_id", "tenant_id", "project_id"].filter(key => typeof value[key] === "string").map(key => [key, value[key]]));
}
function snapshot(input: TransportInput): Snapshot | undefined {
    const url = new URL(forwardUpstreamUrl(input.req, input.opts, input.route));
    if (!["https:", "http:"].includes(url.protocol) || url.username || url.password || url.hash) return undefined;
    if (Object.keys(input.req.headers).some(name => /signature|(?:^|-)hmac(?:-|$)|^x-bili-resign(?:-|$)|^(?:content-)?digest$|^content-md5$|^x-(?:amz|goog)-content-sha256$/i.test(name))
        || [...url.searchParams.keys()].some(name => /signature|(?:^|[-_])hmac(?:[-_]|$)/i.test(name))) return undefined;
    const originalAuthorization = input.req.headers.authorization;
    if (originalAuthorization !== undefined && (Array.isArray(originalAuthorization) || !/^Bearer\s+\S+$/i.test(originalAuthorization))) return undefined;
    const endpointMatches = input.protocol === "openai" ? /\/chat\/completions\/?$/.test(url.pathname)
        : input.protocol === "responses" ? /\/responses\/?$/.test(url.pathname)
        : input.protocol === "anthropic" ? /\/messages\/?$/.test(url.pathname)
        : /\/models\/[^/]+:(?:streamGenerateContent|generateContent)$/.test(url.pathname);
    if (!endpointMatches || input.protocol === "google" && (url.hostname.toLowerCase() === "cloudcode-pa.googleapis.com" || row(input.body.request))) return undefined;
    const target = buildForwardTarget(input.req, input.opts, input.route);
    const credential = laneCredential(input.opts.routes, target.upstreamUrl, () => {});
    if (credential) applyLaneCredential(target.headers, credential, () => {});
    const authorization = header(target.headers, "authorization");
    if (authorization && !/^Bearer\s+\S+$/i.test(authorization)) return undefined;
    if (!authorization && !header(target.headers, "x-api-key") && !header(target.headers, "x-goog-api-key") && !(input.protocol === "google" && url.searchParams.get("key"))) return undefined;
    const identity = Object.entries(target.headers).filter(([key]) => /authorization|api[-_]?key|cookie|token|secret|credential|account|organization|project|user-agent|originator|tenant|workspace|residency/i.test(key)).map(([key, value]) => [key.toLowerCase(), value]).sort(([a], [b]) => a.localeCompare(b));
    const scope: RewriteModelScope = {
        route: url.origin + url.pathname + (url.search ? "?scope=" + hash(url.search) : ""),
        protocol: input.protocol,
        authScope: hash(JSON.stringify([identity, url.search, credential?.reference, accountMetadata(input.body.metadata)])),
    };
    return { url, headers: target.headers, proxyUrl: target.proxyUrl, scope };
}
function sameScope(a: RewriteModelScope, b: RewriteModelScope): boolean {
    return a.route === b.route && a.protocol === b.protocol && a.authScope === b.authScope;
}
async function boundedText(response: Response, maximum: number, signal: AbortSignal): Promise<string> {
    if (signal.aborted) throw new Error("Rewrite request aborted");
    if (!response.body) return "";
    const reader = response.body.getReader(), chunks: Uint8Array[] = [];
    let length = 0;
    let abortRead: (() => void) | undefined;
    const aborted = new Promise<never>((_resolve, reject) => {
        abortRead = () => reject(new Error("Rewrite request aborted"));
        signal.addEventListener("abort", abortRead, { once: true });
    });
    try {
        if (signal.aborted) throw new Error("Rewrite request aborted");
        if (Number(response.headers.get("content-length") ?? 0) > maximum) throw new Error("Rewrite response exceeds its byte limit");
        while (true) {
            const part = await Promise.race([reader.read(), aborted]);
            if (signal.aborted) throw new Error("Rewrite request aborted");
            if (part.done) break;
            length += part.value.byteLength;
            if (length > maximum) throw new Error("Rewrite response exceeds its byte limit");
            chunks.push(part.value);
        }
        const bytes = new Uint8Array(length); let offset = 0;
        for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
        return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } finally { if (abortRead) signal.removeEventListener("abort", abortRead); await reader.cancel().catch(() => {}); reader.releaseLock(); }
}
function chatResult(value: unknown): RewriteCompletion {
    if (!row(value) || !Array.isArray(value.choices) || value.choices.length !== 1 || !row(value.choices[0])) return { text: "", complete: false };
    const choice = value.choices[0], message = row(choice.message) ? choice.message : {};
    const toolCalls = Array.isArray(message.tool_calls) && message.tool_calls.length > 0 || message.function_call !== undefined && message.function_call !== null;
    return { text: typeof message.content === "string" ? message.content : "", complete: choice.finish_reason === "stop" && !toolCalls && !message.refusal, ...(toolCalls ? { toolCalls: true } : {}) };
}


function codexEndpoint(url: URL): boolean { return isCodexRewriteBackend({ route: url.href, protocol: "responses" }); }
function clientVersion(input: TransportInput, url: URL): string | undefined {
    const query = url.searchParams.get("client_version");
    if (query && /^[\w.+-]{1,80}$/.test(query)) return query;
    const agent = input.req.headers["user-agent"];
    return typeof agent === "string" ? /(?:codex_cli_rs|codex(?: desktop)?)[/\s]([\d]+\.[\d]+\.[\d]+[\w.+-]*)/i.exec(agent)?.[1] : undefined;
}
function catalogUrl(input: TransportInput, active: Snapshot): URL | undefined {
    const url = new URL(active.url);
    if (input.protocol === "google") {
        url.pathname = url.pathname.replace(/\/models\/[^/]+:(?:streamGenerateContent|generateContent)$/, "/models");
        url.searchParams.delete("alt");
        url.searchParams.set("pageSize", "512");
    } else if (input.protocol === "responses" && codexEndpoint(url)) {
        const version = clientVersion(input, url); if (!version) return undefined;
        url.pathname = url.pathname.replace(/\/responses\/?$/, "/models");
        url.searchParams.set("client_version", version);
    } else url.pathname = url.pathname.replace(/\/(?:chat\/completions|responses|messages)\/?$/, "/models");
    if (input.protocol === "anthropic") url.searchParams.set("limit", "512");
    return url;
}
function nativeIdentity(body: Row): string {
    const system = body.system;
    const parts = typeof system === "string" ? [system] : Array.isArray(system) ? system.filter(row).flatMap(part => typeof part.text === "string" ? [part.text] : []) : [];
    const prefix = "You are Claude Code, Anthropic's official CLI for Claude.";
    return parts.some(text => text.startsWith(prefix)) ? prefix + "\n\n" : "";
}
/** Select the editor's contract; the main model's output field says nothing
 * about a different editor. Opaque compatible servers retain max_tokens. */
function chatOutputTokenField(url: URL, model: string, declared?: RewriteOutputTokenField): RewriteOutputTokenField {
    if (declared === "max_tokens" || declared === "max_completion_tokens") return declared;
    if (url.hostname.toLowerCase() === "api.openai.com" || /(?:^|[/:])(?:o[134](?:[-.]|$)|gpt-[56](?:[-.]|$))/i.test(model)) return "max_completion_tokens";
    return "max_tokens";
}
function completionPacket(input: TransportInput, active: Snapshot, model: string, text: string, outputTokenField?: RewriteOutputTokenField, thinkingOff?: RewriteThinkingOff): { url: URL; body: Row; sse: boolean } {
    const url = new URL(active.url), user = [{ role: "user", content: [{ type: "input_text", text }] }];
    // #2189: a subscription credential answers 429 "Error" to a system without
    // the client's billing-attribution block, so the editor carries it too.
    const billing = input.protocol === "anthropic" && Array.isArray(input.body.system) ? extractBillingAttributionBlock(input.body.system as Row[]) : undefined;
    const instructions = nativeIdentity(input.body) + PROMPT_REWRITE_INSTRUCTIONS;
    // Editing needs no reasoning, and adaptive thinking made Haiku 5.5 miss the
    // 3 s budget. The value comes from the editor's catalog entry: Sonnet 5.5
    // answers 400 to "disabled", and no model name is known in advance.
    const thinking = thinkingOff ? { thinking: { type: thinkingOff } } : {};
    if (input.protocol === "anthropic") return { url, sse: false, body: { model, system: billing ? [billing, { type: "text", text: instructions }] : instructions, messages: [{ role: "user", content: text }], max_tokens: PROMPT_REWRITE_MAX_TOKENS, stream: false, ...thinking, ...(row(input.body.metadata) && typeof input.body.metadata.user_id === "string" ? { metadata: { user_id: input.body.metadata.user_id } } : {}) } };
    if (input.protocol === "responses") {
        const native = codexEndpoint(url);
        // The ChatGPT backend requires streaming and store:false, and may reject
        // max_output_tokens. Its output is bounded locally by bytes and timeout.
        return { url, sse: native, body: { model, instructions: PROMPT_REWRITE_INSTRUCTIONS, input: user, stream: native, store: false, ...(native ? { tools: [], tool_choice: "none" } : { max_output_tokens: PROMPT_REWRITE_MAX_TOKENS }) } };
    }
    if (input.protocol === "google") {
        const id = model.startsWith("models/") ? model.slice(7) : model;
        if (!/^[A-Za-z0-9._-]+$/.test(id)) throw new Error("Unsupported Gemini model identifier");
        url.pathname = url.pathname.replace(/\/models\/[^/]+:(streamGenerateContent|generateContent)$/, "/models/" + encodeURIComponent(id) + ":$1");
        return { url, sse: url.searchParams.get("alt") === "sse", body: { systemInstruction: { parts: [{ text: PROMPT_REWRITE_INSTRUCTIONS }] }, contents: [{ role: "user", parts: [{ text }] }], generationConfig: { maxOutputTokens: PROMPT_REWRITE_MAX_TOKENS } } };
    }
    return { url, sse: false, body: { model, messages: [{ role: "system", content: PROMPT_REWRITE_INSTRUCTIONS }, { role: "user", content: text }], [chatOutputTokenField(url, model, outputTokenField)]: PROMPT_REWRITE_MAX_TOKENS, stream: false } };
}
function anthropicResult(value: unknown): RewriteCompletion {
    if (!row(value) || !Array.isArray(value.content)) return { text: "", complete: false };
    const toolCalls = value.content.some(part => row(part) && part.type !== "text" && part.type !== "thinking" && part.type !== "redacted_thinking");
    const text = value.content.filter(row).flatMap(part => part.type === "text" && typeof part.text === "string" ? [part.text] : []).join("");
    return { text, complete: value.stop_reason === "end_turn" && !toolCalls, ...(toolCalls ? { toolCalls: true } : {}) };
}
const toolItem = (type: unknown): boolean => typeof type === "string" && /(?:function_call|tool_call|computer_call|web_search|code_interpreter|image_generation|local_shell|mcp_)/i.test(type);
function responsesResult(value: unknown): RewriteCompletion {
    if (!row(value) || !Array.isArray(value.output)) return { text: "", complete: false };
    let text = "", toolCalls = false, refused = false;
    for (const output of value.output) {
        if (!row(output)) continue;
        toolCalls ||= toolItem(output.type);
        if (output.type !== "message" || output.role !== "assistant" || !Array.isArray(output.content)) continue;
        for (const part of output.content) if (row(part)) {
            if (part.type === "output_text" && typeof part.text === "string") text += part.text;
            if (part.type === "refusal") refused = true;
        }
    }
    return { text, complete: value.status === "completed" && !value.incomplete_details && !toolCalls && !refused, ...(toolCalls ? { toolCalls: true } : {}) };
}
function googleResult(value: unknown): RewriteCompletion {
    if (!row(value) || !Array.isArray(value.candidates) || value.candidates.length !== 1 || !row(value.candidates[0])) return { text: "", complete: false };
    const candidate = value.candidates[0], content = row(candidate.content) ? candidate.content : {};
    const parts = Array.isArray(content.parts) ? content.parts.filter(row) : [], toolCalls = parts.some(part => part.functionCall !== undefined || part.executableCode !== undefined || part.codeExecutionResult !== undefined);
    const text = parts.flatMap(part => !part.thought && typeof part.text === "string" ? [part.text] : []).join("");
    return { text, complete: candidate.finishReason === "STOP" && !toolCalls, ...(toolCalls ? { toolCalls: true } : {}) };
}
function streamResult(text: string, protocol: WireProtocol): RewriteCompletion {
    const normalized = text.replace(/\r\n|\r/g, "\n");
    if (!normalized.endsWith("\n\n")) return { text: "", complete: false };
    const frames: unknown[] = [];
    for (const block of normalized.split("\n\n")) {
        const data = block.split("\n").filter(line => line.startsWith("data:")).map(line => line.slice(5).trimStart()).join("\n");
        if (!data || data === "[DONE]") continue;
        try { frames.push(JSON.parse(data)); } catch { return { text: "", complete: false }; }
    }
    let output = "", complete = false, toolCalls = false, failed = false, anthropicStop = false;
    for (const value of frames) {
        if (complete) failed = true;
        if (!row(value)) { failed = true; continue; }
        if (value.error || value.type === "error" || value.type === "response.failed" || value.type === "response.incomplete") { failed = true; continue; }
        if (protocol === "responses") {
            if (value.type === "response.output_text.delta" && typeof value.delta === "string") output += value.delta;
            if (row(value.item)) toolCalls ||= toolItem(value.item.type);
            toolCalls ||= toolItem(value.type);
            if (value.type === "response.completed" && row(value.response)) {
                const terminal = responsesResult(value.response); output = terminal.text || output; complete = terminal.complete; toolCalls ||= terminal.toolCalls === true;
            }
        } else if (protocol === "openai") {
            if (!Array.isArray(value.choices) || value.choices.length !== 1 || !row(value.choices[0])) { failed = true; continue; }
            const choice = value.choices[0], delta = row(choice.delta) ? choice.delta : {};
            if (typeof delta.content === "string") output += delta.content;
            // Compatible relays send null for absent tool fields.
            toolCalls ||= Array.isArray(delta.tool_calls) && delta.tool_calls.length > 0 || delta.function_call !== undefined && delta.function_call !== null;
            if (delta.refusal) failed = true;
            if (choice.finish_reason !== undefined && choice.finish_reason !== null) complete = choice.finish_reason === "stop";
        } else if (protocol === "anthropic") {
            if (row(value.content_block)) toolCalls ||= value.content_block.type === "tool_use";
            if (value.type === "content_block_delta" && row(value.delta) && typeof value.delta.text === "string") output += value.delta.text;
            if (value.type === "message_delta" && row(value.delta)) anthropicStop = value.delta.stop_reason === "end_turn";
            if (value.type === "message_stop") complete = anthropicStop;
        } else {
            const part = googleResult(value); output += part.text; complete ||= part.complete; toolCalls ||= part.toolCalls === true;
        }
    }
    return { text: output, complete: complete && !failed && !toolCalls, ...(toolCalls ? { toolCalls: true } : {}) };
}
function completionResult(response: Response, text: string, protocol: WireProtocol, requireStream: boolean): RewriteCompletion {
    const sse = (response.headers.get("content-type") ?? "").toLowerCase().includes("text/event-stream");
    if (requireStream && !sse) return { text: "", complete: false };
    if (sse) return streamResult(text, protocol);
    const value: unknown = JSON.parse(text);
    return protocol === "openai" ? chatResult(value) : protocol === "responses" ? responsesResult(value) : protocol === "anthropic" ? anthropicResult(value) : googleResult(value);
}

/** Borrows the active relay's native route and effective identity. It never
 * constructs another provider, follows redirects or retries a paid rewrite. */
export function createRewriteTransport(input: TransportInput): RewriteTransport | undefined {
    let initial: Snapshot | undefined;
    try { initial = snapshot(input); } catch { return undefined; }
    if (!initial) {
        input.log("debug", "[prompt-rewrite] native auth, signature or request wrapper cannot be reused safely; original request retained (CodeAssist wrappers require their native grant)");
        return undefined;
    }
    const scope = initial.scope;
    const current = (): Snapshot | undefined => {
        try { const value = snapshot(input); return value && sameScope(value.scope, scope) ? value : undefined; } catch { return undefined; }
    };
    async function request(url: URL, method: "GET" | "POST", body: Row | undefined, maximum: number, signal: AbortSignal): Promise<{ response: Response; text: string }> {
        const active = current();
        if (!active || signal.aborted || url.origin !== active.url.origin) throw new Error("Rewrite route identity changed");
        const headers: Record<string, string> = { ...active.headers, accept: "application/json, text/event-stream", "content-type": "application/json" };
        // A key identifies the main body. Reusing it for an auxiliary body can
        // collide with the main inference at the provider's deduplication store.
        for (const name of Object.keys(headers)) if (["content-length", "content-encoding"].includes(name.toLowerCase()) || /^(?:x-)?idempotency-key$/i.test(name)) delete headers[name];
        const deadline = AbortSignal.any([signal, AbortSignal.timeout(TIMEOUT_MS)]);
        const fetched = await fetchWithTimeout(url.href, { method, headers, ...(body ? { body: JSON.stringify(body) } : {}), redirect: "error", dispatcher: proxyDispatcher(active.proxyUrl, TIMEOUT_MS) }, TIMEOUT_MS, deadline);
        try {
            if (!fetched.response.ok) throw new Error("Rewrite upstream HTTP " + fetched.response.status);
            const text = await boundedText(fetched.response, maximum, deadline);
            if (!current()) throw new Error("Rewrite route identity changed");
            return { response: fetched.response, text };
        } finally { fetched.clearTimer(); }
    }
    return {
        scope,
        isCurrent: () => current() !== undefined,
        async listCatalog(signal) {
            try {
                const active = current(); if (!active) return undefined;
                const url = catalogUrl(input, active); if (!url) return undefined;
                const result = await request(url, "GET", undefined, CATALOG_BYTES, signal);
                const value: unknown = JSON.parse(result.text);
                if (row(value) && (value.has_more !== undefined && value.has_more !== false || ["nextPageToken", "next_page_token", "next", "next_page", "next_page_url", "continuationToken"].some(key => Boolean(value[key])))) return undefined;
                if (/rel\s*=\s*"?next"?/i.test(result.response.headers.get("link") ?? "")) return undefined;
                const models = Array.isArray(value) ? value : row(value) ? value.data ?? value.models : undefined;
                if (!models || (Array.isArray(models) ? models.length : row(models) ? Object.keys(models).length : 513) > 512) return undefined;
                return normalizeRewriteCatalog(value, scope);
            } catch { return undefined; }
        },
        async complete(model, text, signal, outputTokenField, thinkingOff) {
            if (!model || model.length > 512 || /[\u0000-\u001f\u007f]/u.test(model) || Buffer.byteLength(text) > PROMPT_REWRITE_MAX_BYTES) throw new Error("Invalid rewrite input");
            const active = current(); if (!active) throw new Error("Rewrite route identity changed");
            const packet = completionPacket(input, active, model, text, outputTokenField, thinkingOff);
            const result = await request(packet.url, "POST", packet.body, RESPONSE_BYTES, signal);
            return completionResult(result.response, result.text, input.protocol, packet.sse);
        },
    };
}
