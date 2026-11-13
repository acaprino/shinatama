import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { register } from "node:module";
import { test } from "node:test";
import {
    canonicalControlText,
    controlTextHash,
    matchControlMarker,
    protectControlMessage,
    type ControlHuman,
} from "./control-marker.ts";

// The kernel imports its own files with `.js` specifiers; map them to the `.ts` sources so the real pipeline runs here.
register("data:text/javascript," + encodeURIComponent(
    "export async function resolve(s,c,n){if(s.startsWith('.')&&s.endsWith('.js')){try{return await n(s.slice(0,-3)+'.ts',c)}catch{}}return n(s,c)}",
), import.meta.url);
const kernel = await import("../../kernel/src/index.ts");

const sha = (text: string) => createHash("sha256").update(text, "utf8").digest("hex");
const BOOTSTRAP = "[shinatama handoff v1] nonce=abc123\nRead the hand-off file C:\\repo\\seed.md completely before answering.\nThen reply exactly `ready abc123` and wait.\n";
const human = (text: string, index = 4): ControlHuman => ({ text, parts: [text], index });
const header = (kind: string, text: string) => `${kind};${sha(canonicalControlText(text))}`;

type Core = import("acp-kernel").CoreMessage & { wireIndex: number };
const big = (n: number) => `line ${"x".repeat(60)}\n`.repeat(n);
const history = (control: string): Core[] => [
    { id: "u0", role: "user", contentType: "text", text: "Analizza il repository.", wireIndex: 0 },
    { id: "a1", role: "assistant", contentType: "tool-call", toolName: "Read", toolCallId: "t1", text: '{"file_path":"a.rs"}', wireIndex: 1 },
    { id: "r2", role: "user", contentType: "tool-result", toolName: "Read", toolCallId: "t1", text: big(400), wireIndex: 2 },
    { id: "a3", role: "assistant", contentType: "text", text: "Letto.", wireIndex: 3 },
    { id: "u4", role: "user", contentType: "text", text: control, wireIndex: 4 },
];
function run(messages: Core[], protect: ReturnType<typeof matchControlMarker>["marker"]) {
    const core = kernel.createCore();
    const base = kernel.defaultConfig(200_000, { ccr: { enabled: true, toolName: "acp_retrieve", minToolTokens: 1000, excludeTools: [], maxHeadChars: 96, retrieveInlineTokens: 4000 } });
    const config = protect ? protectControlMessage(base, protect) : base;
    const out = core.processTurn({ messages, state: kernel.createInitialState(), config, tokenCount: 20_000, renderTags: "text-only", contentStore: kernel.createContentStore() });
    return { out, text: (id: string) => out.messages.find(m => m.id === id)?.text, ref: (id: string) => out.state.messageRefs.byRaw[id] };
}

test("marked_message_skips_rewrite_ccr_and_fold", () => {
    const outcome = matchControlMarker(header("bootstrap", BOOTSTRAP), human(BOOTSTRAP));
    assert.equal(outcome.marker?.kind, "bootstrap");
    assert.equal(outcome.marker?.index, 4);
    // The rewrite hook returns before any auxiliary call when the marker is recognized; here the part the kernel owns.
    const marked = run(history(BOOTSTRAP), outcome.marker);
    // No tag, no reference: the message cannot be folded or advertised as a range boundary.
    assert.equal(marked.text("u4"), BOOTSTRAP);
    assert.equal(marked.ref("u4"), kernel.BLOCKED_REF);
    // Large results are still stored by CCR: protection is only for the control message.
    assert.match(marked.text("r2") ?? "", /\[acp-stored/);
    // The same history without the marker is tagged and gets a foldable ref.
    const ordinary = run(history(BOOTSTRAP), undefined);
    assert.match(ordinary.text("u4") ?? "", /^<acp tokens="[^"]+" type="text">m\d+<\/acp>\n/);
    assert.notEqual(ordinary.ref("u4"), kernel.BLOCKED_REF);
});

test("mismatched_hash_gets_ordinary_treatment", () => {
    const other = matchControlMarker(header("bootstrap", BOOTSTRAP), human(BOOTSTRAP + "one more line"));
    assert.equal(other.marker, undefined);
    assert.equal(other.reason, "mismatch");
    assert.equal(other.kind, "bootstrap");
    // A control text that is only part of the message by substring is not the message.
    assert.equal(matchControlMarker(header("seed", BOOTSTRAP), human(`before ${BOOTSTRAP}`)).reason, "mismatch");
    assert.equal(matchControlMarker(header("seed", BOOTSTRAP), undefined).reason, "mismatch");
    // Malformed values never match, whatever they carry.
    for (const value of ["", "bootstrap", "bootstrap;", `unknown;${sha(BOOTSTRAP)}`, `bootstrap;${sha(BOOTSTRAP).slice(1)}`, `bootstrap;${sha(BOOTSTRAP)};x`, `bootstrap,${sha(BOOTSTRAP)}`]) {
        assert.equal(matchControlMarker(value, human(BOOTSTRAP)).reason, "malformed", value);
    }
    assert.equal(matchControlMarker([header("bootstrap", BOOTSTRAP), header("bootstrap", BOOTSTRAP)], human(BOOTSTRAP)).reason, "malformed");
    // Ordinary treatment is the unprotected pipeline: tagged, foldable reference.
    const ordinary = run(history(BOOTSTRAP + "one more line"), other.marker);
    assert.match(ordinary.text("u4") ?? "", /^<acp /);
    assert.notEqual(ordinary.ref("u4"), kernel.BLOCKED_REF);
});

test("unmarked_message_is_unchanged_behaviour", () => {
    assert.deepEqual(matchControlMarker(undefined, human(BOOTSTRAP)), { reason: "absent" });
    // Without a marker the config is not touched: the same object comes back from the recognizer's caller.
    const base = kernel.defaultConfig(200_000);
    const messages = history(BOOTSTRAP);
    const first = run(messages, undefined);
    const second = run(messages, undefined);
    assert.deepEqual(first.out.messages, second.out.messages);
    assert.equal(base.isMessageProtected, undefined);
    // Protection composes with what the host already protects and touches nothing else.
    const marker = matchControlMarker(header("bootstrap", BOOTSTRAP), human(BOOTSTRAP)).marker!;
    const composed = protectControlMessage({ ...base, isMessageProtected: m => m.id === "u0" }, marker);
    const byId = Object.fromEntries(messages.map(m => [m.id, composed.isMessageProtected!(m)]));
    assert.deepEqual(byId, { u0: true, a1: false, r2: false, a3: false, u4: true });
});

test("the_marker_matches_one_text_part_and_the_canonical_text", () => {
    const reminder = "<system-reminder>\nToday is 2026-11-13.\n</system-reminder>";
    const parts: ControlHuman = { text: `${reminder}\n${BOOTSTRAP}`, parts: [reminder, BOOTSTRAP], index: 0 };
    assert.equal(matchControlMarker(header("bootstrap", BOOTSTRAP), parts).marker?.index, 0);
    // The terminal may convert line ends and trim the outer ones.
    const pasted = BOOTSTRAP.replace(/\n/g, "\r\n").trimEnd();
    assert.equal(matchControlMarker(header("bootstrap", BOOTSTRAP), human(pasted)).marker?.kind, "bootstrap");
    assert.equal(controlTextHash(pasted), controlTextHash(BOOTSTRAP));
    assert.equal(canonicalControlText("\r\n  a\r\nb \t\n"), "a\nb");
    // Uppercase hex and padding in the header are accepted; the kind must be one of the three.
    assert.equal(matchControlMarker(` receipt;${sha(canonicalControlText(BOOTSTRAP)).toUpperCase()} `, human(BOOTSTRAP)).marker?.kind, "receipt");
});
