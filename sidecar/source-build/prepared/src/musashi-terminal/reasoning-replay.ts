// Musashi Terminal: collect Chat reasoning deltas without interpreting or merging encrypted payloads.
type Detail = Record<string, unknown>;
const object = (value: unknown): value is Detail => value !== null && typeof value === "object" && !Array.isArray(value);
const opaque = (value: Detail) => ["data", "encrypted", "encrypted_content", "signature", "thought_signature"].some(key => value[key] != null);
const textField = (value: Detail) => value.type === "reasoning.text" ? "text"
    : value.type === "reasoning.summary" || value.type === "summary" ? "summary" : undefined;

/** Only textual fragments with a common identity combine. Every opaque or unknown entry remains whole and ordered. */
export function mergeOpenaiReasoningDetails(previous: readonly unknown[], incoming: readonly unknown[]): unknown[] {
    const result = structuredClone([...previous]);
    for (const original of incoming) {
        const current = structuredClone(original);
        const field = object(current) && !opaque(current) ? textField(current) : undefined;
        const index = field && object(current) && typeof current[field] === "string" ? result.findLastIndex(value => {
            if (!object(value) || opaque(value) || value.type !== current.type || typeof value[field] !== "string") return false;
            if (["id", "index", "format"].some(key => value[key] != null && current[key] != null && value[key] !== current[key])) return false;
            return (current.id != null && current.id === value.id) || (current.index != null && current.index === value.index);
        }) : -1;
        if (index < 0 || !field || !object(current)) { result.push(current); continue; }
        const prior = result[index] as Detail;
        const merged: Detail = { ...prior, ...current, [field]: String(prior[field]) + String(current[field]) };
        for (const key of ["id", "index", "format"]) if (current[key] == null && prior[key] != null) merged[key] = prior[key];
        result[index] = merged;
    }
    return result;
}
