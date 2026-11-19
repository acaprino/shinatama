import { createHash } from "node:crypto";
import { markDirty, type Session } from "../session.js";
import { getStore } from "../persist.js";
import { PROMPT_REWRITE_INSTRUCTIONS } from "./prompt-rewrite-contract.js";
import { judgePromptRewrite, shouldRewritePrompt, validatePromptRewrite, type PromptRewriteCompletion } from "./prompt-rewrite-validation.js";
import { selectRewriteModel, type RewriteModelCatalog, type RewriteModelScope, type RewriteModelSelection, type RewriteOutputTokenField, type RewriteThinkingOff } from "./rewrite-models.js";

export type PromptRewriteSettings = { mode: "auto" | "off"; preferredModels?: Record<string, string[]>; maxCostUsd: number };
export type PromptRewriteInput = {
    session: Session;
    text: string;
    humanID: string;
    harness: string;
    scope: RewriteModelScope;
    currentModel?: string;
    settings(): PromptRewriteSettings | undefined;
    signal?: AbortSignal;
    isCurrent?(): boolean;
    listCatalog(signal: AbortSignal): Promise<RewriteModelCatalog | undefined>;
    complete(model: string, text: string, signal: AbortSignal, outputTokenField?: RewriteOutputTokenField, thinkingOff?: RewriteThinkingOff): Promise<PromptRewriteCompletion>;
    /** Must durably save metadata before resolving; supplied by the host or tests. */
    persist?(session: Session): Promise<void>;
};
type RewriteState = { version: 1; admitted: string[]; results: { key: string; text: string }[] };
type Diagnostic = { kind: "prompt-rewrite"; outcome: "rewritten" | "original"; reason: string; model?: string; selection?: RewriteModelSelection["reason"]; estimatedCost?: number };
type Dependencies = { timeoutMs?: number; now?: () => number; diagnostic?(record: Diagnostic): void };
const metadataKey = "musashiTerminalPromptRewrite";
const MAX_ADMISSIONS = 4096;
// Results are the per-human decision record: earlier notes are repeated from
// them, so evicting one would change the conversation's past on every turn.
const MAX_RESULTS = MAX_ADMISSIONS;
const CACHE_TTL_MS = 60_000;
const object = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const digest = (value: unknown): string => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const scopeKey = (scope: RewriteModelScope): string => digest([scope.route, scope.protocol, scope.authScope]);
const hashKey = (key: unknown): key is string => typeof key === "string" && /^[a-f0-9]{64}$/.test(key);
const resultKey = (sessionID: string, humanID: string, harness: string, route: string, text: string): string =>
    digest([sessionID, humanID, harness, route, digest(text)]);

/** Malformed or future durable state fails closed rather than forgetting admissions. */
function stateOf(session: Session): RewriteState | undefined {
    const value = session.metadata[metadataKey];
    if (value === undefined) return { version: 1, admitted: [], results: [] };
    if (!object(value) || value.version !== 1 || !Array.isArray(value.admitted) || !value.admitted.every(hashKey) ||
        !Array.isArray(value.results) || value.results.some(result => !object(result) || !hashKey(result.key) || typeof result.text !== "string")) return undefined;
    return value as RewriteState;
}

/** Rewrites already decided for earlier humans, recalled without catalog,
 * admission or completion. Validation matches the repeated-human path, so a
 * note that the current human would not show never returns in the past either. */
export function recallPromptRewrites(input: { session: Session; harness: string; scope: RewriteModelScope },
    humans: readonly { humanID: string; text: string }[]): (string | undefined)[] {
    const state = humans.length > 0 ? stateOf(input.session) : undefined;
    if (!state) return humans.map(() => undefined);
    const results = new Map(state.results.map(result => [result.key, result.text]));
    const route = scopeKey(input.scope);
    return humans.map(human => {
        const text = results.get(resultKey(input.session.id, human.humanID, input.harness, route, human.text));
        return text === undefined ? undefined : validatePromptRewrite({ text, complete: true }, human.text);
    });
}

/** Bound even dependencies that ignore AbortSignal, while consuming late rejections. */
async function within<T>(pending: Promise<T>, signal: AbortSignal): Promise<T> {
    signal.throwIfAborted();
    let stop: (() => void) | undefined;
    try {
        return await Promise.race([pending, new Promise<never>((_, reject) => {
            stop = () => reject(signal.reason);
            signal.addEventListener("abort", stop, { once: true });
        })]);
    } finally { if (stop) signal.removeEventListener("abort", stop); }
}

function preferences(settings: PromptRewriteSettings, input: PromptRewriteInput): string[] | undefined {
    const map = settings.preferredModels;
    if (!map) return undefined;
    for (const key of [input.scope.route, input.scope.protocol, input.harness, "*"]) {
        if (Object.hasOwn(map, key)) return map[key];
    }
    return undefined;
}

/**
 * One durable admission per exact human text/account/route. Failures preserve
 * the original and never trigger another completion for the same admission.
 */
export function createPromptRewriter(deps: Dependencies = {}): (input: PromptRewriteInput) => Promise<string | undefined> {
    const now = deps.now ?? Date.now;
    const timeoutMs = Math.min(3000, Math.max(1, deps.timeoutMs ?? 3000));
    const pending = new Map<string, Promise<string | undefined>>();
    const catalogs = new Map<string, { at: number; value: RewriteModelCatalog | undefined }>();
    const catalogWork = new Map<string, Promise<RewriteModelCatalog | undefined>>();
    const failures = new Map<string, number>();
    const report = (outcome: Diagnostic["outcome"], reason: string, selected?: RewriteModelSelection) => {
        try { deps.diagnostic?.({ kind: "prompt-rewrite", outcome, reason, ...(selected ? { model: selected.model, selection: selected.reason, ...(selected.estimatedCost !== undefined ? { estimatedCost: selected.estimatedCost } : {}) } : {}) }); } catch { /* diagnostics cannot affect the request */ }
    };
    const remember = <T>(map: Map<string, T>, key: string, value: T) => {
        if (map.size >= 128 && !map.has(key)) map.delete(map.keys().next().value!);
        map.set(key, value);
    };
    const persist = async (input: PromptRewriteInput, signal: AbortSignal) => {
        if (input.persist) { await within(Promise.resolve().then(() => { signal.throwIfAborted(); return input.persist!(input.session); }), signal); return; }
        const store = getStore();
        const expected = stateOf(input.session);
        if (!store.enabled || !expected) throw new Error("prompt_rewrite_persistence_unavailable");
        markDirty(input.session);
        if (!store.flushSync(input.session)) throw new Error("prompt_rewrite_persistence_unavailable");
        // The store can successfully retain a newer disk snapshot. A flush
        // result alone therefore does not prove this admission became durable.
        const loaded = store.loadSync(input.session.id, input.session.meta);
        const saved = loaded ? stateOf(loaded) : undefined;
        const admissions = new Set(saved?.admitted);
        const results = new Map(saved?.results.map(result => [result.key, result.text]));
        if (!saved || expected.admitted.some(key => !admissions.has(key)) ||
            expected.results.some(result => results.get(result.key) !== result.text)) throw new Error("prompt_rewrite_admission_not_durable");
        signal.throwIfAborted();
    };
    const catalog = async (input: PromptRewriteInput, key: string, signal: AbortSignal) => {
        const saved = catalogs.get(key);
        if (saved && now() - saved.at < CACHE_TTL_MS) return saved.value;
        let work = catalogWork.get(key);
        if (!work) {
            work = Promise.resolve().then(() => { signal.throwIfAborted(); return input.listCatalog(signal); });
            catalogWork.set(key, work);
            void work.finally(() => { if (catalogWork.get(key) === work) catalogWork.delete(key); }).catch(() => {});
        }
        try {
            const value = await within(work, signal);
            signal.throwIfAborted();
            remember(catalogs, key, { at: now(), value });
            return value;
        } finally {
            // A dependency may never settle after cancellation. Do not pin
            // future catalog discovery to that abandoned promise forever.
            if (signal.aborted && catalogWork.get(key) === work) catalogWork.delete(key);
        }
    };

    return async function rewrite(input): Promise<string | undefined> {
        const started = performance.now();
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(new DOMException("Prompt rewrite deadline exceeded", "TimeoutError")), timeoutMs);
        const signal = input.signal ? AbortSignal.any([controller.signal, input.signal]) : controller.signal;
        let accountRoute: string | undefined;
        let selected: RewriteModelSelection | undefined;
        try {
            const settings = input.settings();
            if (!settings || settings.mode !== "auto" || !Number.isFinite(settings.maxCostUsd) || settings.maxCostUsd < 0 ||
                !input.humanID || !input.harness || !shouldRewritePrompt(input.text) || signal.aborted || input.isCurrent?.() === false) return undefined;
            const settingsIdentity = digest(settings);
            const originalText = input.text, originalHuman = input.humanID, originalHarness = input.harness;
            const originalSession = input.session, originalSessionID = input.session.id;
            accountRoute = scopeKey(input.scope);
            const capturedScope = accountRoute;
            const active = () => {
                if (signal.aborted || performance.now() - started >= timeoutMs || input.isCurrent?.() === false ||
                    input.text !== originalText || input.humanID !== originalHuman || input.harness !== originalHarness ||
                    input.session !== originalSession || input.session.id !== originalSessionID ||
                    scopeKey(input.scope) !== capturedScope) return false;
                const current = input.settings();
                return current?.mode === "auto" && digest(current) === settingsIdentity;
            };
            const key = resultKey(input.session.id, input.humanID, input.harness, capturedScope, input.text);
            const state = stateOf(input.session);
            if (!state) return undefined;
            const known = state.results.find(result => result.key === key);
            if (known) return active() ? validatePromptRewrite({ text: known.text, complete: true }, input.text) : undefined;
            const inFlight = pending.get(key);
            if (inFlight) {
                const result = await within(inFlight, signal);
                return active() ? result : undefined;
            }
            if (state.admitted.includes(key) || state.admitted.length >= MAX_ADMISSIONS ||
                (failures.get(capturedScope) ?? 0) > now() || !input.persist && !getStore().enabled) return undefined;

            const work = (async (): Promise<string | undefined> => {
                const available = await catalog(input, capturedScope, signal);
                if (!active()) return undefined;
                selected = selectRewriteModel({ scope: input.scope, catalogs: [available],
                    preferredModels: preferences(settings, input), currentModel: input.currentModel,
                    inputTokens: Math.ceil(Buffer.byteLength(PROMPT_REWRITE_INSTRUCTIONS + "\n" + input.text) / 4),
                    maxEstimatedCost: settings.maxCostUsd });
                if (!selected) {
                    remember(failures, capturedScope, now() + CACHE_TTL_MS);
                    report("original", "no-authorized-economical-model");
                    return undefined;
                }
                const latest = stateOf(input.session);
                if (!latest || latest.admitted.includes(key) || latest.admitted.length >= MAX_ADMISSIONS || !active()) return undefined;
                // Never roll this admission back: persistence/transport may have
                // succeeded despite a timeout or an ambiguous local failure.
                input.session.metadata[metadataKey] = { ...latest, admitted: [...latest.admitted, key] };
                await persist(input, signal);
                if (!active()) return undefined;
                const { model, outputTokenField, thinkingOff } = selected;
                const result = await within(Promise.resolve().then(() => {
                    if (!active()) throw new Error("prompt_rewrite_stale_before_dispatch");
                    return input.complete(model, originalText, signal, outputTokenField, thinkingOff);
                }), signal);
                if (!active()) return undefined;
                const verdict = judgePromptRewrite(result, input.text, true);
                if (verdict.text === undefined) { report("original", verdict.reason, selected); return undefined; }
                const text = verdict.text;
                const accepted = stateOf(input.session);
                if (!accepted) return undefined;
                input.session.metadata[metadataKey] = { ...accepted,
                    results: [...accepted.results.filter(result => result.key !== key), { key, text }].slice(-MAX_RESULTS) };
                await persist(input, signal);
                if (!active()) return undefined;
                report("rewritten", "completed", selected);
                return text;
            })();
            pending.set(key, work);
            try { return await within(work, signal); }
            finally { if (pending.get(key) === work) pending.delete(key); }
        } catch {
            if (accountRoute && (!input.signal?.aborted || controller.signal.aborted)) remember(failures, accountRoute, now() + CACHE_TTL_MS);
            report("original", signal.aborted ? controller.signal.aborted ? "timeout" : "cancelled" : "failed", selected);
            return undefined;
        } finally { clearTimeout(timer); }
    };
}
