import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { register } from "node:module";
import { test } from "node:test";
import {
    canonicalControlText,
    controlMismatchLevel,
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

test("a_shifted_wire_index_never_selects_another_message", () => {
    const marker = matchControlMarker(header("bootstrap", BOOTSTRAP), human(BOOTSTRAP)).marker!;
    const reminder = "<system-reminder>\nToday is 2026-11-13.\n</system-reminder>";
    const withParts = matchControlMarker(header("bootstrap", BOOTSTRAP), { text: `${reminder}\n${BOOTSTRAP}`, parts: [reminder, BOOTSTRAP], index: 4 }).marker!;
    const core = (id: string, text: string, wireIndex: number): Core => ({ id, role: "user", contentType: "text", text, wireIndex });
    // Spliced messages moved the stamp: another user message now sits at the declared index and is not protected.
    const other = core("x", "Un altro messaggio dell utente.", 4);
    const moved = core("c", BOOTSTRAP, 3);
    assert.equal(protectControlMessage(kernel.defaultConfig(1000), marker).isMessageProtected!(other), false);
    // Fail-safe: the control message itself, now at another index, gets the ordinary treatment.
    assert.equal(protectControlMessage(kernel.defaultConfig(1000), marker).isMessageProtected!(moved), false);
    // At the right index only the texts the hash vouched for are protected: the reminder part and the control part.
    const protect = protectControlMessage(kernel.defaultConfig(1000), withParts).isMessageProtected!;
    assert.deepEqual([core("r", reminder, 4), core("c", BOOTSTRAP + "\r\n", 4), core("x", "other", 4), core("c2", BOOTSTRAP, 5)].map(m => protect(m)), [true, true, false, false]);
    // A matching index and text on an assistant or non-text core is not a control message.
    assert.equal(protect({ ...core("a", BOOTSTRAP, 4), role: "assistant" } as Core), false);
    assert.equal(protect({ ...core("t", BOOTSTRAP, 4), contentType: "tool-result" } as Core), false);
    // End to end through the kernel: with the stamp shifted the control message is tagged like any message.
    const messages = history(BOOTSTRAP).map(m => (m.id === "u4" ? { ...m, wireIndex: 3 } : m));
    const out = run(messages, marker);
    assert.match(out.text("u4") ?? "", /^<acp /);
});

test("a_mismatch_is_logged_at_info_on_the_first_human_turn_and_never_uses_up_the_budget", () => {
    const seen = new Set<string>();
    // Later human turns of a session with a static header are ordinary: debug, and nothing is consumed.
    assert.equal(controlMismatchLevel(seen, "Un altro messaggio.", false), "debug");
    assert.equal(controlMismatchLevel(seen, "Ancora un altro.", false), "debug");
    assert.equal(seen.size, 0);
    // The first human turn is where the declared message is expected: a changed paste is visible at info...
    assert.equal(controlMismatchLevel(seen, BOOTSTRAP + "changed", true), "info");
    // ...once per distinct received text...
    assert.equal(controlMismatchLevel(seen, BOOTSTRAP + "changed", true), "debug");
    assert.equal(controlMismatchLevel(seen, BOOTSTRAP + "changed again", true), "info");
    // ...and a request with no human text counts as one case.
    assert.equal(controlMismatchLevel(seen, undefined, true), "info");
    assert.equal(controlMismatchLevel(seen, undefined, true), "debug");
    // The set stays bounded.
    for (let n = 0; n < 40; n++) controlMismatchLevel(seen, `variant ${n}`, true);
    assert.ok(seen.size <= 16, String(seen.size));
    assert.equal(controlMismatchLevel(seen, "one more", true), "debug");
});
