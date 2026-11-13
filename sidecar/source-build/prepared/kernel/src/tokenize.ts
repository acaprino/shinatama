import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

// Pure estimates are content-keyed: edits, folds, reasoning-policy changes and
// other sessions cannot reuse a different text's result. Bound both payload
// retention and entry count; very large one-off strings are not retained.
const estimateMemo = new Map<string, { key: string; tokens: number; bytes: number }>();
const ESTIMATE_MEMO_BYTES = 16 * 1024 * 1024;
const ESTIMATE_MEMO_ENTRIES = 8192;
let estimateMemoBytes = 0;

export function defaultCountTokens(text: string): number {
  if (!text) return 0;
  const cached = estimateMemo.get(text);
  if (cached) {
    // Keep the independently allocated key even when this caller supplies a
    // slice of a much larger reasoning/message string.
    estimateMemo.delete(cached.key);
    estimateMemo.set(cached.key, cached);
    return cached.tokens;
  }
  // CJK chars tokenize ~1:1 (chars/4 badly underestimates them). Count them
  // directly, then estimate the non-CJK remainder with chars/4 so digits,
  // punctuation, and symbols in code/JSON are not dropped to zero.
  const cjk = text.match(/[\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7af]/g);
  const cjkCount = cjk?.length ?? 0;
  const tokens = cjkCount + Math.ceil((text.length - cjkCount) / 4);
  const bytes = text.length * 2 + 64;
  if (bytes <= 1024 * 1024) {
    while (estimateMemo.size && (estimateMemoBytes + bytes > ESTIMATE_MEMO_BYTES || estimateMemo.size >= ESTIMATE_MEMO_ENTRIES)) {
      const oldest = estimateMemo.keys().next().value!;
      estimateMemoBytes -= estimateMemo.get(oldest)!.bytes;
      estimateMemo.delete(oldest);
    }
    // V8 slices can retain their entire source. Copy UTF-16 code units before
    // retention so the charged length bounds the actual backing storage too;
    // UTF-8 would change lone surrogates in otherwise valid JSON strings.
    const key = Buffer.from(text, "utf16le").toString("utf16le");
    estimateMemo.set(key, { key, tokens, bytes });
    estimateMemoBytes += bytes;
  }
  return tokens;
}

export function estimateMessageTokens(text: string | undefined): number {
  return defaultCountTokens(text ?? "");
}

/** Guarded contribution of a host-projected thinking payload to the metered
 *  message size. Non-finite or non-positive values are treated as absent (0),
 *  so a bad host value can never poison the accounting. */
export function thinkingTokenValue(thinking: number | undefined): number {
  return typeof thinking === "number" &&
    Number.isFinite(thinking) &&
    thinking > 0
    ? thinking
    : 0;
}

/** Total metered size of a core message: visible text plus any host-projected
 *  thinking payload (CoreMessage.thinkingTokens). Every per-message counting
 *  site (range recommendations, block compressedTokens, status reports, context
 *  breakdown) goes through this so all surfaces share one caliber. */
export function countMessageTokens(
  message: { text?: string; thinkingTokens?: number },
  countTokens: TokenCountFn = defaultCountTokens,
): number {
  return (
    countTokens(message.text ?? "") + thinkingTokenValue(message.thinkingTokens)
  );
}

export function estimateTokensFast(text: string): number {
  if (!text) return 0;
  return Math.ceil(text.length / 4);
}

export type TokenCountFn = (text: string) => number;

const BPE_SIZE_GUARD = 100_000;

export function createBpeTokenizer(): TokenCountFn {
  try {
    const mod = require("@anthropic-ai/tokenizer");
    const bpeCount = mod.countTokens ?? mod.default?.countTokens;
    if (typeof bpeCount !== "function") return defaultCountTokens;
    return (text: string) => {
      if (text.length > BPE_SIZE_GUARD) return defaultCountTokens(text);
      try {
        return bpeCount(text);
      } catch {
        return defaultCountTokens(text);
      }
    };
  } catch {
    return defaultCountTokens;
  }
}
