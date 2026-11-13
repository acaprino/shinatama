import type { WireProtocol } from "../util.js";

export const REWRITE_OUTPUT_TOKENS = 1024;
export const DEFAULT_REWRITE_MAX_ESTIMATED_COST = 0.01;
export type RewriteModelScope = { route: string; protocol: WireProtocol; authScope: string };
export type RewriteOutputTokenField = "max_tokens" | "max_completion_tokens";
/** The thinking value that turns an Anthropic editor's reasoning off, from its catalog entry. */
export type RewriteThinkingOff = "disabled" | "between_tools";
export type RewriteModel = {
    id: string;
    name?: string;
    /** USD per one million tokens, never raw per-token prices. */
    cost?: { input: number; output: number };
    outputLimit?: number;
    outputTokenField?: RewriteOutputTokenField;
    textInput?: boolean;
    textOutput?: boolean;
    lightweight?: boolean;
    available?: boolean;
    /** Catalog release date in epoch milliseconds (Anthropic `created_at`, OpenAI `created`). */
    createdAt?: number;
    thinkingOff?: RewriteThinkingOff;
};
export type RewriteModelCatalog = { scope: RewriteModelScope; models: RewriteModel[] };
export type RewriteModelSelection = { model: string; reason: "explicit" | "price" | "lightweight"; estimatedCost?: number; outputTokenField?: RewriteOutputTokenField; thinkingOff?: RewriteThinkingOff };
export type RewriteModelSelectionInput = {
    scope: RewriteModelScope;
    catalogs: readonly (RewriteModelCatalog | undefined)[];
    preferredModels?: readonly string[];
    /** The current model is not a fallback or a grant; it competes like any listed model. */
    currentModel?: string;
    inputTokens?: number;
    maxEstimatedCost?: number;
};
type Row = Record<string, unknown>;
const object = (value: unknown): value is Row => value !== null && typeof value === "object" && !Array.isArray(value);
const own = (row: Row, key: string): unknown => Object.hasOwn(row, key) ? row[key] : undefined;
const validId = (value: unknown): value is string => typeof value === "string" && value.trim() === value && value.length > 0 && value.length <= 512 && !/[\u0000-\u001f\u007f]/u.test(value);
const validPrice = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value) && value >= 0;
const protocol = (value: unknown): value is WireProtocol => value === "anthropic" || value === "openai" || value === "responses" || value === "google";
function validScope(scope: unknown): scope is RewriteModelScope {
    return object(scope) && typeof scope.route === "string" && scope.route.trim().length > 0
        && typeof scope.authScope === "string" && scope.authScope.trim().length > 0 && protocol(scope.protocol);
}
function sameScope(a: RewriteModelScope, b: RewriteModelScope): boolean {
    return a.route === b.route && a.protocol === b.protocol && a.authScope === b.authScope;
}
function first(...values: unknown[]): unknown { return values.find(value => value !== undefined); }
function modality(value: unknown): boolean | undefined {
    if (!Array.isArray(value)) return undefined;
    return value.some(entry => typeof entry === "string" && entry.toLowerCase() === "text");
}
function boolean(value: unknown): boolean | undefined { return typeof value === "boolean" ? value : undefined; }
function textCapability(...values: Array<boolean | undefined>): boolean | undefined {
    return values.includes(false) ? false : values.includes(true) ? true : undefined;
}
function openRouter(scope: RewriteModelScope): boolean {
    try { return new URL(scope.route).hostname.toLowerCase() === "openrouter.ai"; } catch { return false; }
}

/** Absent metadata is unknown. Malformed recognized prices reject the entry. */
function normalizedPrice(row: Row, scope: RewriteModelScope): RewriteModel["cost"] | null {
    for (const name of ["cost", "pricing"]) {
        const value = own(row, name);
        if (value === undefined) continue;
        if (!object(value)) return null;
        const input = own(value, "input"), output = own(value, "output");
        if (input !== undefined || output !== undefined) {
            const unit = own(value, "unit");
            if (unit !== undefined && !["per_million", "per_million_tokens", "usd_per_million_tokens", "per1m"].includes(String(unit).toLowerCase())) return null;
            return validPrice(input) && validPrice(output) ? { input, output } : null;
        }
        if (name === "pricing" && openRouter(scope) && (own(value, "prompt") !== undefined || own(value, "completion") !== undefined)) {
            const prompt = own(value, "prompt"), completion = own(value, "completion");
            const perMillion = (price: unknown): number | undefined => {
                if (typeof price !== "string") return undefined;
                const parts = /^((?:\d+(?:\.\d*)?|\.\d+))(?:e([+-]?\d+))?$/i.exec(price.trim());
                if (!parts) return undefined;
                const exponent = Number(parts[2] ?? 0) + 6;
                return Number.isSafeInteger(exponent) ? Number(parts[1] + "e" + exponent) : undefined;
            };
            const perInput = perMillion(prompt), perOutput = perMillion(completion);
            if (!validPrice(perInput) || !validPrice(perOutput)) return null;
            return { input: perInput, output: perOutput };
        }
    }
    return undefined;
}

/** API availability does not exclude models granted by the native Codex backend. */
export function isCodexRewriteBackend(scope: Pick<RewriteModelScope, "route" | "protocol">): boolean {
    if (scope.protocol !== "responses") return false;
    try { return /\/codex\/responses\/?$/.test(new URL(scope.route).pathname); } catch { return false; }
}
function declaredOutputTokenField(value: Row): RewriteOutputTokenField | undefined {
    const declared = own(value, "outputTokenField") ?? own(value, "maxTokensField");
    if (declared === "max_tokens" || declared === "max_completion_tokens") return declared;
    const supported = own(value, "supported_parameters");
    if (!Array.isArray(supported)) return undefined;
    return supported.includes("max_completion_tokens") ? "max_completion_tokens" : supported.includes("max_tokens") ? "max_tokens" : undefined;
}

function normalizedModel(value: unknown, scope: RewriteModelScope, fallbackId?: string): RewriteModel | undefined {
    if (!object(value)) return undefined;
    const rawId = first(own(value, "id"), own(value, "slug"), fallbackId, own(value, "name"));
    if (!validId(rawId)) return undefined;
    const id = scope.protocol === "google" && rawId.startsWith("models/") ? rawId.slice(7) : rawId;
    if (!validId(id)) return undefined;
    const cost = normalizedPrice(value, scope);
    if (cost === null) return undefined;
    const model: RewriteModel = { id };
    const outputTokenField = declaredOutputTokenField(value);
    if (outputTokenField) model.outputTokenField = outputTokenField;
    const name = first(own(value, "display_name"), own(value, "displayName"), own(value, "name"));
    if (typeof name === "string" && name.trim() && name !== rawId) model.name = name.trim();
    if (cost !== undefined) model.cost = cost;
    const limit = object(value.limit) ? value.limit : {}, limits = object(value.limits) ? value.limits : {};
    const provider = object(value.top_level_provider) ? value.top_level_provider : {};
    const outputLimit = first(own(value, "outputLimit"), own(value, "outputTokenLimit"), own(value, "output_token_limit"), own(value, "maxOutputTokens"), own(value, "max_output_tokens"), own(limit, "output"), own(limits, "output"), own(value, "output"), own(provider, "max_completion_tokens"));
    if (outputLimit !== undefined) {
        if (typeof outputLimit !== "number" || !Number.isSafeInteger(outputLimit) || outputLimit < 0) return undefined;
        model.outputLimit = outputLimit;
    }
    const capabilities = object(value.capabilities) ? value.capabilities : {};
    const modalities = object(value.modalities) ? value.modalities : {}, architecture = object(value.architecture) ? value.architecture : {};
    const inputModalities = first(own(value, "input_modalities"), own(value, "inputModalities"), own(value, "supported_input_modalities"), own(modalities, "input"), own(architecture, "input_modalities"));
    const outputModalities = first(own(value, "output_modalities"), own(value, "outputModalities"), own(value, "supported_output_modalities"), own(modalities, "output"), own(architecture, "output_modalities"));
    const textInput = textCapability(boolean(own(value, "textInput")), boolean(own(capabilities, "textInput")), boolean(own(capabilities, "text_input")), modality(inputModalities));
    let textOutput = textCapability(boolean(own(value, "textOutput")), boolean(own(capabilities, "textOutput")), boolean(own(capabilities, "text_output")), modality(outputModalities));
    const methods = first(own(value, "supportedGenerationMethods"), own(value, "supported_generation_methods"));
    if (Array.isArray(methods) && !methods.some(method => typeof method === "string" && ["generatecontent", "streamgeneratecontent", "chat", "chat.completions", "responses", "messages", "generate"].includes(method.toLowerCase()))) textOutput = false;
    if (typeof textInput === "boolean") model.textInput = textInput;
    if (typeof textOutput === "boolean") model.textOutput = textOutput;
    const lightweight = first(boolean(own(value, "lightweight")), boolean(own(capabilities, "lightweight")));
    if (typeof lightweight === "boolean") model.lightweight = lightweight;
    const created = first(own(value, "created_at"), own(value, "createdAt"), own(value, "created"));
    const createdAt = typeof created === "string" ? Date.parse(created) : typeof created === "number" ? created * 1000 : NaN;
    if (Number.isSafeInteger(createdAt) && createdAt > 0) model.createdAt = createdAt;
    // Anthropic lists each accepted thinking value with `supported`: an editor
    // needs no reasoning, and which value turns it off differs by model.
    const thinkingTypes = object(capabilities.thinking) && object(capabilities.thinking.types) ? capabilities.thinking.types : {};
    const accepts = (type: string): boolean => object(thinkingTypes[type]) && (thinkingTypes[type] as Row).supported === true;
    const thinkingOff = accepts("disabled") ? "disabled" : accepts("between_tools") ? "between_tools" : undefined;
    if (thinkingOff) model.thinkingOff = thinkingOff;
    const supportedProtocol = first(own(value, "protocol"), own(value, "supported_protocols"), own(value, "protocols"));
    if (own(value, "available") === false || own(value, "enabled") === false || own(value, "disabled") === true || own(value, "supported_in_api") === false && !isCodexRewriteBackend(scope)
        || typeof supportedProtocol === "string" && supportedProtocol !== scope.protocol
        || Array.isArray(supportedProtocol) && !supportedProtocol.includes(scope.protocol)) model.available = false;
    return model;
}

/** The caller supplies the provenance scope after authenticating its native
 * catalog. A global registry may enrich prices, never grant catalog membership. */
export function normalizeRewriteCatalog(value: unknown, scope: RewriteModelScope): RewriteModelCatalog | undefined {
    if (!validScope(scope)) return undefined;
    if (object(value) && own(value, "scope") !== undefined && (!validScope(value.scope) || !sameScope(scope, value.scope))) return undefined;
    const rows = Array.isArray(value) ? value : object(value) ? first(own(value, "data"), own(value, "models")) : undefined;
    const entries = Array.isArray(rows) ? rows.map(row => normalizedModel(row, scope))
        : object(rows) ? Object.entries(rows).map(([id, row]) => normalizedModel(row, scope, id)) : undefined;
    if (!entries) return undefined;
    return { scope: { ...scope }, models: entries.filter((model): model is RewriteModel => model !== undefined) };
}

function compatible(model: RewriteModel): boolean {
    return validId(model.id) && model.available !== false && model.textInput !== false && model.textOutput !== false
        && (model.outputLimit === undefined || Number.isSafeInteger(model.outputLimit) && model.outputLimit >= REWRITE_OUTPUT_TOKENS);
}
const lightweightName = /(?:^|[\/\s._:-])(?:nano|mini|haiku|flash|lite|luna)(?=$|[\/\s._:-])/i;
const premiumName = /(?:^|[\/\s._:-])(?:pro|max)(?=$|[\/\s._:-])/i;
function isLightweight(model: RewriteModel): boolean {
    const name = model.id + " " + (model.name ?? "");
    return !premiumName.test(name) && (model.lightweight === true || model.lightweight === undefined && lightweightName.test(name));
}
function signature(model: RewriteModel): string {
    return JSON.stringify([model.name, model.cost?.input, model.cost?.output, model.outputLimit, model.textInput, model.textOutput, model.lightweight, model.available, model.outputTokenField, model.createdAt, model.thinkingOff]);
}

/** No discovery, network selection call or model-name invention. Unknown and
 * unaffordable catalogs leave the original prompt untouched. */
export function selectRewriteModel(input: RewriteModelSelectionInput): RewriteModelSelection | undefined {
    const inputTokens = input.inputTokens ?? 1024, cap = input.maxEstimatedCost ?? DEFAULT_REWRITE_MAX_ESTIMATED_COST;
    if (!validScope(input.scope) || !Number.isSafeInteger(inputTokens) || inputTokens < 0 || !validPrice(cap)) return undefined;
    const rows = new Map<string, RewriteModel>(), conflicts = new Set<string>();
    for (const catalog of input.catalogs) {
        if (!catalog || !validScope(catalog.scope) || !sameScope(input.scope, catalog.scope)) continue;
        for (const model of catalog.models) {
            const before = rows.get(model.id);
            if (before && signature(before) !== signature(model)) conflicts.add(model.id);
            else rows.set(model.id, model);
        }
    }
    const eligible = [...rows.values()].filter(model => !conflicts.has(model.id) && compatible(model)).map(model => {
        if (model.cost === undefined) return { model, estimatedCost: undefined };
        if (!validPrice(model.cost.input) || !validPrice(model.cost.output)) return undefined;
        const estimatedCost = (model.cost.input * inputTokens + model.cost.output * REWRITE_OUTPUT_TOKENS) / 1_000_000;
        return Number.isFinite(estimatedCost) && estimatedCost <= cap ? { model, estimatedCost } : undefined;
    }).filter((entry): entry is { model: RewriteModel; estimatedCost: number | undefined } => entry !== undefined)
        .filter(entry => cap > 0 || entry.estimatedCost === 0);
    const result = (entry: typeof eligible[number], reason: RewriteModelSelection["reason"]): RewriteModelSelection => ({ model: entry.model.id, reason, ...(entry.estimatedCost !== undefined ? { estimatedCost: entry.estimatedCost } : {}), ...(entry.model.outputTokenField ? { outputTokenField: entry.model.outputTokenField } : {}), ...(entry.model.thinkingOff ? { thinkingOff: entry.model.thinkingOff } : {}) });
    for (const preferred of input.preferredModels ?? []) {
        const found = eligible.find(entry => entry.model.id === preferred);
        if (found) return result(found, "explicit");
    }
    const order = (a: typeof eligible[number], b: typeof eligible[number]): number => a.model.id < b.model.id ? -1 : a.model.id > b.model.id ? 1 : 0;
    const priced = eligible.filter(entry => entry.estimatedCost !== undefined).sort((a, b) => a.estimatedCost! - b.estimatedCost! || order(a, b));
    if (priced[0]) return result(priced[0], "price");
    // Without prices, the newest lightweight model of the catalog: an id order
    // picked claude-haiku-4-5-20251001 over claude-haiku-5-5, at ten times its price.
    const newest = (a: typeof eligible[number], b: typeof eligible[number]): number => (b.model.createdAt ?? 0) - (a.model.createdAt ?? 0) || order(a, b);
    const lightweight = eligible.filter(entry => isLightweight(entry.model)).sort(newest);
    return lightweight[0] ? result(lightweight[0], "lightweight") : undefined;
}
