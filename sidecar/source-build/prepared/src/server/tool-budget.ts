/** A budget view only; never forward this projection. OpenAI tool search
 * keeps standalone function descriptions visible, but hides deferred functions
 * inside namespaces until discovery. Eager and unknown tool shapes stay intact.
 * https://developers.openai.com/api/docs/guides/tools-tool-search */
export function modelVisibleTools(tools: unknown): unknown {
    if (!Array.isArray(tools) || !tools.some((t) => t?.type === "tool_search")) return tools ?? [];
    const project = (tool: unknown, inNamespace = false): unknown[] => {
        if (!tool || typeof tool !== "object" || Array.isArray(tool)) return [tool];
        const item = tool as Record<string, unknown>;
        if (item.type === "namespace" && typeof item.name === "string" && Array.isArray(item.tools)) {
            return [{ ...item, tools: item.tools.flatMap(child => project(child, true)) }];
        }
        if (item.type === "function" && typeof item.name === "string"
            && item.parameters && typeof item.parameters === "object"
            && !Array.isArray(item.parameters) && item.defer_loading === true) {
            if (inNamespace) return [];
            const { parameters: _parameters, ...visible } = item;
            return [visible];
        }
        return [tool];
    };
    return tools.flatMap(tool => project(tool));
}
