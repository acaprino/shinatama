/** Musashi Terminal: advisory pressure policy; the real model window remains authoritative. */
/** `epochTokens` and `cacheTtlSeconds` tune the frontier (epoch.ts); absent, the frontier derives them. */
export type AdaptivePressureSettings = { enabled: boolean; maxTokens: number; targetRatio: number; growthWindow: number; epochTokens?: number; cacheTtlSeconds?: number };
type ThresholdInput = { contextTokens: number; outputTokens: number; reserveTokens?: number; ratio: number; maxTokens: number };

export function parseAdaptivePressure(value: unknown): AdaptivePressureSettings | undefined {
    if (value === undefined) return undefined;
    if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("compress.adaptivePressure must be an object");
    const raw = value as Record<string, unknown>;
    const own = (key: string): unknown => Object.hasOwn(raw, key) ? raw[key] : undefined;
    const withDefault = (key: string, fallback: number): unknown => own(key) === undefined ? fallback : own(key);
    const enabled = own("enabled"), maxTokens = withDefault("maxTokens", 160000), targetRatio = withDefault("targetRatio", 0.75), growthWindow = withDefault("growthWindow", 20);
    if (typeof enabled !== "boolean"
        || typeof maxTokens !== "number" || !Number.isSafeInteger(maxTokens) || maxTokens <= 0
        || typeof targetRatio !== "number" || !Number.isFinite(targetRatio) || targetRatio <= 0 || targetRatio >= 1
        || typeof growthWindow !== "number" || !Number.isSafeInteger(growthWindow) || growthWindow < 1 || growthWindow > 64) throw new Error("compress.adaptivePressure is invalid");
    const epochTokens = own("epochTokens"), cacheTtlSeconds = own("cacheTtlSeconds");
    if ((epochTokens !== undefined && (typeof epochTokens !== "number" || !Number.isSafeInteger(epochTokens) || epochTokens <= 0))
        || (cacheTtlSeconds !== undefined && (typeof cacheTtlSeconds !== "number" || !Number.isSafeInteger(cacheTtlSeconds) || cacheTtlSeconds <= 0))) throw new Error("compress.adaptivePressure is invalid");
    return { enabled, maxTokens, targetRatio, growthWindow, ...(epochTokens !== undefined ? { epochTokens } : {}), ...(cacheTtlSeconds !== undefined ? { cacheTtlSeconds } : {}) };
}

export function adaptivePressureThreshold(input: ThresholdInput): number | undefined {
    const { contextTokens, outputTokens, ratio, maxTokens } = input;
    const reserveTokens = input.reserveTokens === undefined ? Math.max(4096, Math.ceil(contextTokens * 0.05)) : input.reserveTokens;
    if (!Number.isFinite(contextTokens) || contextTokens <= 0 || !Number.isFinite(maxTokens) || maxTokens <= 0
        || !Number.isFinite(ratio) || ratio <= 0 || ratio > 1
        || !Number.isFinite(outputTokens) || outputTokens < 0 || !Number.isFinite(reserveTokens) || reserveTokens < 0) return undefined;
    const budget = contextTokens - outputTokens - reserveTokens;
    if (budget <= 0) return undefined;
    const fractional = ratio * budget;
    // Stable even when fractional / maxTokens would overflow.
    return fractional <= maxTokens ? fractional / Math.hypot(1, fractional / maxTokens) : maxTokens / Math.hypot(1, maxTokens / fractional);
}
