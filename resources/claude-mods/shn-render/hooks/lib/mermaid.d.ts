export declare function renderMermaidASCII(source: string, options?: Record<string, unknown>): string
export declare function parseMermaid(source: string): { nodes: Map<string, { label: string }>; edges: { source: string; target: string; label?: string }[]; subgraphs: Sub[] }
interface Sub { label: string; children: Sub[] }
