import type { BiliMessage } from "./bili-message.js";
import type { ClusterCounter } from "./message-id.js";

// structuredClone copies large string payloads. Strings are immutable, so only
// containers need copying; retain aliases between raw item and layout slots.
function clonePiece<T>(value: T, seen = new Map<object, unknown>()): T {
  if (value === null || typeof value !== "object") return value;
  const known = seen.get(value);
  if (known !== undefined) return known as T;
  if (value instanceof Map) {
    const out = new Map(); seen.set(value, out);
    for (const [key, item] of value) out.set(clonePiece(key, seen), clonePiece(item, seen));
    return out as T;
  }
  if (value instanceof Set) {
    const out = new Set(); seen.set(value, out);
    for (const item of value) out.add(clonePiece(item, seen));
    return out as T;
  }
  if (Array.isArray(value)) {
    const out: unknown[] = []; seen.set(value, out);
    for (const item of value) out.push(clonePiece(item, seen));
    return out as T;
  }
  const out: Record<string, unknown> = {}; seen.set(value, out);
  for (const [key, item] of Object.entries(value)) {
    if (key === "__proto__") Object.defineProperty(out, key, { value: clonePiece(item, seen), writable: true, enumerable: true, configurable: true });
    else out[key] = clonePiece(item, seen);
  }
  return out as T;
}

// New HTTP requests contain fresh objects. Compare their full JSON structure,
// including key order (some identity seeds serialize objects), without building
// and hashing another copy of every historical text string.
function sameInput(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (!left || !right || typeof left !== "object" || typeof right !== "object") return false;
  if (Array.isArray(left) !== Array.isArray(right)) return false;
  const leftKeys = Object.keys(left), rightKeys = Object.keys(right);
  if (leftKeys.length !== rightKeys.length) return false;
  for (let i = 0; i < leftKeys.length; i++) {
    const key = leftKeys[i]!;
    if (key !== rightKeys[i] || !sameInput((left as Record<string, unknown>)[key], (right as Record<string, unknown>)[key])) return false;
  }
  return true;
}

function retainedSize(value: unknown, seen = new Set<object>()): number {
  if (typeof value === "string") return value.length * 2;
  if (!value || typeof value !== "object") return 8;
  if (seen.has(value)) return 0;
  seen.add(value);
  let bytes = 64;
  if (value instanceof Map) {
    for (const [key, item] of value) bytes += 32 + retainedSize(key, seen) + retainedSize(item, seen);
  } else if (value instanceof Set || Array.isArray(value)) {
    for (const item of value) bytes += 8 + retainedSize(item, seen);
  } else {
    for (const [key, item] of Object.entries(value)) bytes += 16 + key.length * 2 + retainedSize(item, seen);
  }
  return bytes;
}

/** Optional host-owned, exact-content cache for immutable projection pieces.
 *  Only wire conversion is reused. Callers replay ordering/pairing each pass;
 *  compression state, model settings and protection decisions never enter it. */
export class ProjectionMemo {
  private entries = new Map<string, { value: unknown; input: unknown; bytes: number; seen: number }>();
  private bytes = 0;
  private pass = 0;
  private scanning = false;
  hits = 0;
  misses = 0;
  constructor(readonly maxBytes = 32 * 1024 * 1024, readonly maxEntries = 8192) {}

  /** Protect later history from an early miss when the history exceeds capacity.
   *  Only visited entries may be evicted during a scan. An absent entry can go
   *  cold without turning every following lookup into another miss. */
  project<T>(convert: () => T): T {
    if (this.scanning) return convert();
    this.scanning = true;
    this.pass++;
    try {
      const result = convert();
      // Truncation, compaction and protocol changes leave obsolete positional
      // entries behind; release those only after the complete input was read.
      for (const [key, entry] of this.entries) {
        if (entry.seen === this.pass) continue;
        this.entries.delete(key);
        this.bytes -= entry.bytes;
      }
      return result;
    } finally {
      this.scanning = false;
    }
  }

  get<T>(key: string, input?: unknown): T | undefined {
    const entry = this.entries.get(key);
    if (entry && this.scanning) entry.seen = this.pass;
    if (!entry || !sameInput(input, entry.input)) { this.misses++; return undefined; }
    this.hits++;
    this.entries.delete(key);
    this.entries.set(key, entry);
    return clonePiece(entry.value) as T;
  }

  set(key: string, value: unknown, input?: unknown): void {
    // Conservative UTF-16 payload/container accounting without serializing a
    // second copy of long histories just to decide their memory admission.
    const bytes = key.length * 2 + retainedSize(input) + retainedSize(value) + 512;
    const previous = this.entries.get(key);
    if (previous) { this.entries.delete(key); this.bytes -= previous.bytes; }
    if (bytes > this.maxBytes || this.maxEntries <= 0) return;
    const evict: string[] = [];
    let remainingBytes = this.bytes, remainingEntries = this.entries.size;
    for (const [candidate, entry] of this.entries) {
      if (remainingBytes + bytes <= this.maxBytes && remainingEntries < this.maxEntries) break;
      // Preflight before evicting anything: admitting this piece must not
      // discard an unchanged piece that the forward scan has not reached.
      if (this.scanning && entry.seen !== this.pass) return;
      evict.push(candidate);
      remainingBytes -= entry.bytes;
      remainingEntries--;
    }
    for (const candidate of evict) this.entries.delete(candidate);
    this.entries.set(key, { value: clonePiece(value), input: clonePiece(input), bytes, seen: this.pass });
    this.bytes = remainingBytes + bytes;
  }

  get size(): number { return this.entries.size; }
  get retainedBytes(): number { return this.bytes; }
}

/** Duplicate ordinals depend on the current history, never on the cached pass. */
export function reclusterProjection(messages: BiliMessage[], clusters: ClusterCounter): Map<string, string> {
  const remap = new Map<string, string>();
  for (const message of messages) {
    const old = message.id;
    const base = old.replace(/^(h_[0-9a-f]{16})_\d+$/, "$1");
    message.id = clusters.next(base);
    remap.set(old, message.id);
  }
  return remap;
}

export function replayToolNames(messages: BiliMessage[], names: Map<string, string>, explicitName?: string): void {
  for (const message of messages) {
    if (message.contentType === "tool-call" && typeof message.toolCallId === "string" && typeof message.toolName === "string") names.set(message.toolCallId, message.toolName);
    if (message.contentType === "tool-result") {
      const name = explicitName || names.get(message.toolCallId ?? "");
      if (name) message.toolName = name;
      else delete message.toolName;
    }
  }
}
