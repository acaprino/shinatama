import { PROMPT_REWRITE_MAX_BYTES, PROMPT_REWRITE_TAG_CLOSE, PROMPT_REWRITE_TAG_OPEN } from "./prompt-rewrite-contract.js";

export type PromptRewriteCompletion = { text: string; complete: boolean; toolCalls?: boolean };
/** Why a reply kept the original: recorded in diagnostics, never the text itself. */
export type PromptRewriteRejection = "incomplete" | "tool-call" | "untagged" | "empty" | "oversized" | "unchanged" | "cosmetic" | "executable-syntax" | "literal-mismatch";
export type PromptRewriteVerdict = { text: string; reason?: undefined } | { text?: undefined; reason: PromptRewriteRejection };
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const commandNames = [
  "npm", "npx", "pnpm", "yarn", "bun", "git", "curl", "wget", "sudo", "bash", "sh", "powershell", "pwsh", "cmd",
  "python", "python3", "pip", "pip3", "uv", "docker", "kubectl", "rm", "chmod", "chown", "rmdir", "erase", "reboot", "shutdown",
  "ls", "cp", "mv", "mkdir", "touch", "echo", "printf", "tee", "kill", "copy", "move", "xcopy", "robocopy", "attrib", "chkdsk",
  "diskpart", "taskkill", "schtasks", "reg", "mkfs", "dd", "rsync", "ssh", "scp",
]
const commandPattern = new RegExp(`\\b(?:${commandNames.join("|")})\\b(?:[ \\t]+[-\\w./\\\\]+)?`, "gi")
// "del" is also an ordinary Italian preposition: require command context rather
// than interpreting phrases such as "la causa del pulsante" as executable text.
const delCommandPatterns = [
  /(?:^|[\r\n])\s*(del\b(?:[ \t]+[-\w./\\]+)?)/gi,
  /\b(?:esegui|eseguire|lancia|lanciare|run|execute|comando|command)(?:\s*:\s*|\s+)(?:(?:il|the)\s+)?(del\b(?:[ \t]+[-\w./\\]+)?)/gi,
]

function executableSyntax(text: string): boolean {
  return /```|~~~|<\/?[A-Za-z][\w:-]*(?:\s[^<>]*|\/?)>/u.test(text)
    || /(?:^|\r?\n)\s*(?:[\/!][\w.-]|[$>#]\s*\S)/u.test(text)
}
/** Local eligibility costs nothing. Commands, literal-heavy/code inputs and small follow-ups pass through. */
export function shouldRewritePrompt(text: string): boolean {
  const trimmed = text.trim()
  return trimmed.length >= 120 && trimmed.split(/\s+/u).length >= 20 && Buffer.byteLength(text) <= PROMPT_REWRITE_MAX_BYTES
    && !executableSyntax(trimmed)
}
// Backticks mark a literal without changing it: `git push` and git push are
// the same literal, so the span is compared by its content.
const backticked = /^`([^`\r\n]+)`$/
function literals(text: string): { all: Set<string>; quoted: Set<string> } {
  const technical = text.match(/`[^`\r\n]+`|https?:\/\/[^\s<>"']+|\b[A-Z]:[\\/][^\s"'<>]+|(?<![\w:])(?:@|\.\.?\/|\/)[\w./\\-]+|\b(?:[\w-]+[\\/])+[\w.-]+|\b[\w-]+\.[A-Za-z][A-Za-z0-9]{0,15}\b|\B-{1,2}[A-Za-z][\w-]*|\b\d+(?:[.,]\d+)*\b|\{\{[^{}]+\}\}|\$\{[^{}]+\}|\b(?:Add|Clear|Copy|Get|Invoke|Move|New|Remove|Set|Start|Stop|Test|Write)-[A-Z][A-Za-z]+\b/g) ?? []
  const commands = text.match(commandPattern) ?? []
  const contextual = delCommandPatterns.flatMap(pattern => [...text.matchAll(pattern)].map(match => match[1]!))
  const all = new Set<string>(), quoted = new Set<string>()
  for (const literal of [...technical, ...commands, ...contextual]) {
    const content = backticked.exec(literal)?.[1]
    all.add(content ?? literal)
    if (content === undefined) continue
    quoted.add(content)
    // The span consumed its match: the paths and flags inside it still count.
    for (const inner of literals(content).all) all.add(inner)
  }
  return { all, quoted }
}
function containsWords(text: string, words: string): boolean {
  for (let at = text.indexOf(words); at >= 0; at = text.indexOf(words, at + 1)) {
    if (!/[\p{L}\p{N}_]/u.test(text[at - 1] ?? "") && !/[\p{L}\p{N}_]/u.test(text[at + words.length] ?? "")) return true
  }
  return false
}
// Capitals, punctuation and spacing alone do not make a draft clearer for a model.
const folded = (text: string): string => text.normalize("NFC").toLowerCase().replace(/[\p{P}\s]+/gu, "")
/** The whole reply must be one tagged edited request; anything outside the
 *  tags means the editor answered the draft instead of editing it. */
function editedRequest(text: string): string | undefined {
  const trimmed = text.trim()
  if (!trimmed.startsWith(PROMPT_REWRITE_TAG_OPEN) || !trimmed.endsWith(PROMPT_REWRITE_TAG_CLOSE)) return undefined
  const inner = trimmed.slice(PROMPT_REWRITE_TAG_OPEN.length, trimmed.length - PROMPT_REWRITE_TAG_CLOSE.length)
  return inner.includes(PROMPT_REWRITE_TAG_OPEN) || inner.includes(PROMPT_REWRITE_TAG_CLOSE) ? undefined : inner
}
/** Judge a provider-neutral reply without ever changing the original. `tagged`
 *  applies to a fresh editor reply; a stored result is already unwrapped. */
export function judgePromptRewrite(value: unknown, original: string, tagged = false): PromptRewriteVerdict {
    if (!object(value) || value.complete !== true || typeof value.text !== "string") return { reason: "incomplete" };
    if (value.toolCalls === true) return { reason: "tool-call" };
    const unwrapped = tagged ? editedRequest(value.text) : value.text;
    if (unwrapped === undefined) return { reason: "untagged" };
    if (Buffer.byteLength(unwrapped) > PROMPT_REWRITE_MAX_BYTES) return { reason: "oversized" };
    const text = unwrapped.trim();
    if (!text) return { reason: "empty" };
    if (text.length > original.length * 1.5 + 200) return { reason: "oversized" };
    if (text === original.trim()) return { reason: "unchanged" };
    if (folded(text) === folded(original)) return { reason: "cosmetic" };
    if (executableSyntax(text)) return { reason: "executable-syntax" };
    const before = literals(original), after = literals(text);
    // A quoted span may hold words the draft had without marking them as literals.
    if ([...before.all].some(literal => !after.all.has(literal)) ||
        [...after.all].some(literal => !before.all.has(literal) && !(after.quoted.has(literal) && containsWords(original, literal)))) return { reason: "literal-mismatch" };
    return { text };
}
export function validatePromptRewrite(value: unknown, original: string): string | undefined {
    return judgePromptRewrite(value, original).text;
}