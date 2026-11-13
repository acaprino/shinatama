/** Workflow profiles change only compression thresholds and recent retention.
 * Selection reads the current native request; it never inherits another session's mode.
 * Signals: announced plugin x-bili-plugin-agent; Codex instructions/developer
 * <collaboration_mode> blocks; completed Claude EnterPlanMode/ExitPlanMode pairs.
 * User messages, documents, reasoning channels and tool descriptions are not signals.
 */
export const CONTEXT_PROFILE_FIELDS = ["maxContextLimit", "emergencyThresholdPercent", "nudgeGrowthTokens", "preserveRecentMessages", "preserveRecentTokens"] as const;
export type ContextProfileName = "analysis" | "build";
export type ContextProfileMode = "auto" | ContextProfileName;
export interface ContextProfile {
    maxContextLimit?: number | string;
    emergencyThresholdPercent?: number | string;
    nudgeGrowthTokens?: number;
    preserveRecentMessages?: number;
    preserveRecentTokens?: number;
}
export interface ContextProfileSettings {
    profiles?: Partial<Record<ContextProfileName, ContextProfile>>;
    agents?: Record<string, ContextProfileName>;
}
export interface ContextProfileSelection {
    name: ContextProfileName;
    source: "forced" | "plugin-agent" | "codex-collaboration" | "claude-plan-tool" | "conflict" | "fallback";
}
export interface ContextProfileRequest {
    protocol: string | null;
    body: unknown;
    headers: Record<string, string | string[] | undefined>;
    mode?: unknown;
    codex?: boolean;
    claude?: boolean;
}

const DEFAULT_PROFILES: Readonly<Record<ContextProfileName, Readonly<ContextProfile>>> = {
    analysis: { maxContextLimit: "65%", preserveRecentMessages: 12, preserveRecentTokens: 16_000, nudgeGrowthTokens: 30_000 },
    build: { maxContextLimit: "45%", preserveRecentMessages: 6, preserveRecentTokens: 6_000, nudgeGrowthTokens: 15_000 },
};
const DEFAULT_AGENTS: Readonly<Record<string, ContextProfileName>> = {
    plan: "analysis", analysis: "analysis", analyst: "analysis", reviewer: "analysis", mapper: "analysis", oracle: "analysis", docs: "analysis", instructions: "analysis",
    build: "build", implementation: "build", code: "build", explore: "build", general: "build",
};
function object(value: unknown): value is Record<string, unknown> {
    return value !== null && typeof value === "object" && !Array.isArray(value);
}
function own(value: Record<string, unknown>, key: string): unknown {
    return Object.hasOwn(value, key) ? value[key] : undefined;
}
function profileName(value: unknown): value is ContextProfileName {
    return value === "analysis" || value === "build";
}
function safeAgent(value: string): boolean {
    return /^[a-z][a-z0-9_-]{0,63}$/.test(value) && !["__proto__", "constructor", "prototype"].includes(value);
}
function percentage(value: unknown): value is number | string {
    // Match the upstream/Tritech accepted numeric syntax. The kernel reads
    // numeric values as ratios; absolute windows remain modelContextLimit.
    if (typeof value === "number") return Number.isFinite(value) && value > 0;
    return typeof value === "string" && /^\d+(?:\.\d+)?%$/.test(value.trim()) && parseFloat(value) > 0 && parseFloat(value) <= 100;
}

/** Only own properties and the five permitted fields survive parsing. Unknown
 * names/fields are discarded; invalid values on permitted fields reject the block.
 */
export function parseContextProfileSettings(value: unknown): ContextProfileSettings {
    if (!object(value)) return {};
    const result: ContextProfileSettings = {};
    const profiles = own(value, "profiles");
    if (profiles !== undefined) {
        if (!object(profiles)) throw new Error("compress.profiles must be an object");
        result.profiles = {};
        for (const name of ["analysis", "build"] as const) {
            const raw = own(profiles, name);
            if (raw === undefined) continue;
            if (!object(raw)) throw new Error(`compress.profiles.${name} must be an object`);
            const cleaned: ContextProfile = {};
            for (const field of CONTEXT_PROFILE_FIELDS) {
                const entry = own(raw, field);
                if (entry === undefined) continue;
                const valid = field === "maxContextLimit" || field === "emergencyThresholdPercent"
                    ? percentage(entry)
                    : typeof entry === "number" && Number.isSafeInteger(entry) && (field === "nudgeGrowthTokens" ? entry > 0 : entry >= 0);
                if (!valid) throw new Error(`compress.profiles.${name}.${field} is invalid`);
                Object.assign(cleaned, { [field]: typeof entry === "string" ? entry.trim() : entry });
            }
            result.profiles[name] = cleaned;
        }
    }
    const agents = own(value, "agents");
    if (agents !== undefined) {
        if (!object(agents)) throw new Error("compress.agents must be an object");
        result.agents = {};
        for (const [agent, name] of Object.entries(agents)) {
            if (safeAgent(agent) && profileName(name)) result.agents[agent] = name;
        }
    }
    return result;
}

function header(headers: ContextProfileRequest["headers"], name: string): string | undefined {
    const raw = Object.hasOwn(headers, name) ? headers[name] : undefined;
    return typeof raw === "string" && raw.trim() ? raw.trim() : undefined;
}
function texts(value: unknown): string[] {
    if (typeof value === "string") return [value];
    if (!Array.isArray(value)) return [];
    return value.flatMap((part): string[] => object(part) && ["text", "input_text"].includes(String(own(part, "type"))) && typeof own(part, "text") === "string" ? [String(own(part, "text"))] : []);
}
type ModeEvidence = ContextProfileName | "conflict" | undefined;
function collaborationText(value: unknown): ModeEvidence {
    const modes = new Set<ContextProfileName>();
    for (const text of texts(value)) {
        // Only complete native control blocks count. Their exact header is
        // supplied by Codex, not inferred from prose or a reasoning channel.
        for (const match of text.matchAll(/<collaboration_mode>\s*([\s\S]*?)<\/collaboration_mode>/g)) {
            const mode = /^# Collaboration Mode: (Plan|Default)(?:\r?\n|$)/.exec(match[1]?.trimStart() ?? "");
            if (mode) modes.add(mode[1] === "Plan" ? "analysis" : "build");
        }
    }
    return modes.size > 1 ? "conflict" : modes.values().next().value;
}
function codexEvidence(body: Record<string, unknown>): ModeEvidence {
    const instructions = collaborationText(own(body, "instructions"));
    let developer: ModeEvidence = undefined;
    const input = own(body, "input");
    if (Array.isArray(input)) {
        for (const item of input) {
            if (!object(item) || own(item, "role") !== "developer") continue;
            const type = own(item, "type");
            if (type !== undefined && type !== "message") continue;
            const mode = collaborationText(own(item, "content"));
            if (mode !== undefined) developer = mode; // ordered native mode updates supersede older ones
        }
    }
    if (instructions === "conflict" || developer === "conflict" || (instructions && developer && instructions !== developer)) return "conflict";
    return developer ?? instructions;
}
const DENIED_PLAN_RESULT = /\b(?:not approved|rejected|declined|denied|cancelled|canceled)\b/i;
const APPROVED_PLAN_RESULT = /^(?:User has approved (?:your|the) plan|Your plan has been approved|Plan approved)(?:[.!:\s]|$)/i;
function claudeEvidence(body: Record<string, unknown>): ModeEvidence {
    const messages = own(body, "messages");
    if (!Array.isArray(messages)) return undefined;
    const calls = new Map<string, "EnterPlanMode" | "ExitPlanMode">();
    const completed = new Set<string>();
    let mode: ModeEvidence = undefined;
    for (const message of messages) {
        if (!object(message)) continue;
        const content = own(message, "content");
        if (!Array.isArray(content)) continue;
        for (const block of content) {
            if (!object(block)) continue;
            if (own(message, "role") === "assistant" && own(block, "type") === "tool_use") {
                const name = own(block, "name");
                const id = own(block, "id");
                if (typeof id === "string" && id && (name === "EnterPlanMode" || name === "ExitPlanMode")) {
                    if (calls.has(id)) return "conflict";
                    calls.set(id, name);
                    mode = "analysis"; // pending/failed transitions remain conservative
                }
            }
            if (own(message, "role") !== "user" || own(block, "type") !== "tool_result") continue;
            const id = own(block, "tool_use_id");
            if (typeof id !== "string") continue;
            const name = calls.get(id);
            if (!name) continue;
            if (completed.has(id)) return "conflict";
            completed.add(id);
            const result = texts(own(block, "content")).join("\n").trim();
            const errorFlag = own(block, "is_error");
            const rejected = (errorFlag !== undefined && errorFlag !== false) || DENIED_PLAN_RESULT.test(result);
            if (name === "EnterPlanMode" || rejected || !APPROVED_PLAN_RESULT.test(result)) mode = "analysis";
            else mode = "build";
        }
    }
    return mode;
}

/** Forced choice selects the profile only; provider/model overrides still win. */
export function selectContextProfile(request: ContextProfileRequest, settings: ContextProfileSettings = {}): ContextProfileSelection {
    if (profileName(request.mode)) return { name: request.mode, source: "forced" };
    const evidence: Array<{ name: ModeEvidence; source: ContextProfileSelection["source"] }> = [];
    const announced = header(request.headers, "x-bili-plugin");
    const agent = header(request.headers, "x-bili-plugin-agent");
    if (announced && agent && safeAgent(agent)) {
        const configured = settings.agents && Object.hasOwn(settings.agents, agent) ? settings.agents[agent] : undefined;
        const mapped = configured ?? (Object.hasOwn(DEFAULT_AGENTS, agent) ? DEFAULT_AGENTS[agent] : undefined);
        if (profileName(mapped)) evidence.push({ name: mapped, source: "plugin-agent" });
    }
    if (object(request.body)) {
        if (request.codex && request.protocol === "responses") evidence.push({ name: codexEvidence(request.body), source: "codex-collaboration" });
        if (request.claude && request.protocol === "anthropic") evidence.push({ name: claudeEvidence(request.body), source: "claude-plan-tool" });
    }
    const active = evidence.filter((entry) => entry.name !== undefined);
    if (active.some((entry) => entry.name === "conflict") || new Set(active.map((entry) => entry.name)).size > 1) return { name: "analysis", source: "conflict" };
    const selected = active[0];
    return selected && profileName(selected.name) ? { name: selected.name, source: selected.source } : { name: "analysis", source: "fallback" };
}

/** global → selected profile → provider → model. Only the five whitelisted
 * values are overlaid; all other global settings retain their identity/value.
 */
export function applyContextProfile<T extends ContextProfileSettings>(compress: T, selected: ContextProfileSelection): T & ContextProfile {
    const profiles = compress.profiles;
    const configured = profiles && Object.hasOwn(profiles, selected.name) ? profiles[selected.name] : undefined;
    const defaults = DEFAULT_PROFILES[selected.name];
    const result = { ...compress };
    for (const field of CONTEXT_PROFILE_FIELDS) {
        const value = configured && Object.hasOwn(configured, field) ? configured[field] : defaults[field];
        if (value !== undefined) Object.assign(result, { [field]: value });
    }
    return result;
}
