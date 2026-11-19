import assert from "node:assert/strict";
import { mkdtempSync } from "node:fs";
import { register } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

// The sidecar imports its own files with `.js` specifiers and the kernel as `acp-kernel`; map both to the `.ts`
// sources so the real pipeline runs here. Sessions persist into a scratch folder, never the user's.
process.env.BILI_SESSIONS_DIR = mkdtempSync(join(tmpdir(), "adaptive-turn-test-"));
const kernelSource = pathToFileURL(join(import.meta.dirname, "..", "..", "kernel", "src")).href;
register("data:text/javascript," + encodeURIComponent(
    `const k=${JSON.stringify(kernelSource)};export async function resolve(s,c,n){` +
    "const m=/^acp-kernel(?:[/](wire|panel|persist))?$/.exec(s);if(m)return n(k+'/'+(m[1]?m[1]+'/':'')+'index.ts',c);" +
    "if(s.startsWith('.')&&s.endsWith('.js')){try{return await n(s.slice(0,-3)+'.ts',c)}catch{}}return n(s,c)}",
), import.meta.url);
const kernel = await import("acp-kernel");
const { processAdaptiveTurn } = await import("./adaptive-turn.ts");
const { epochRecord } = await import("./epoch.ts");

type Core = import("acp-kernel").CoreMessage & { wireIndex: number };
type Session = import("../session.ts").Session;
const id = (n: number) => "h_" + n.toString(16).padStart(16, "0");
const output = (step: number) => `build log ${step}\n${"compiled module ok\n".repeat(400)}`;
/** The id the wire converter gives the second message with the same content as `first`. */
const again = (first: number) => `${id(first)}_1`;

/** Three human turns (messages 1, 4 and 7), each running tools; the last one is still running (`calls` steps). The
 *  second turn's step printed the same output as the first turn's, so its result carries the converter's second-copy
 *  id, as repeated command outputs do in a real history. */
function history(calls: number): Core[] {
    const messages: Core[] = [];
    const push = (message: Omit<Core, "wireIndex">) => messages.push({ ...message, wireIndex: messages.length } as Core);
    const human = (text: string) => push({ id: id(messages.length + 1), role: "user", contentType: "text", text });
    const step = (call: number, result = id(messages.length + 2), text = output(call)) => {
        push({ id: id(messages.length + 1), role: "assistant", contentType: "tool-call", toolName: "shell", toolCallId: `c${call}`, text: `{"cmd":"make step${call}"}` });
        push({ id: result, role: "user", contentType: "tool-result", toolName: "shell", toolCallId: `c${call}`, text });
    };
    human("Prepare the release notes.");
    step(0);
    human("Now build and test everything.");
    step(1, again(3), output(0));
    human("Go on with the remaining steps.");
    for (let call = 2; call < 2 + calls; call++) step(call);
    return messages;
}
const FOLDED = [id(5), again(3)];

const WINDOW = 272_000;
const config = { ...kernel.defaultConfig(), modelContextLimit: 204_000, adaptivePressure: { enabled: true, maxTokens: 160_000 } };
const core = kernel.createCore();
const ids = (messages: readonly { id: string }[]) => messages.map(message => message.id);

/** A session whose epoch frontier stands at `frontier`, the last human message already judged, with the second turn's
 *  step already folded: the proxy's preflight folds wherever the history is, before or after the frontier. `billed`
 *  is the provider's count for the last request. */
function session(messages: Core[], frontier: { id: string; cause: string; step?: boolean }, billed = 0): Session {
    let state = core.processTurn({ messages, state: kernel.createInitialState(), config, tokenCount: 0, renderTags: "none" }).state;
    const refs = FOLDED.map(name => state.messageRefs.byRaw[name]!);
    const fold = core.applyCompression({ ranges: [{ startRef: refs[0]!, endRef: refs[1]!, summary: "Ran the build step with make; it compiled every module and reported no error." }], messages, state, config });
    assert.deepEqual(fold.state.blocks.filter(block => block.active).map(block => block.effectiveMessageIds), [FOLDED], JSON.stringify(fold.result));
    // The previous request's full kernel pass saw the whole history.
    state = core.processTurn({ messages, state: fold.state, config, tokenCount: 0, renderTags: "none" }).state;
    return {
        id: "adaptive-turn-test", state,
        stats: { lastInputTokens: billed, lastInputTokensSource: billed > 0 ? "usage" : undefined },
        metadata: { lastWindowSource: "registry-peek", musashiTerminalEpoch: { version: 1, epoch: 1, reached: 0, at: Date.now(), seen: id(7), frontier: { tokens: 1_000, ...frontier } } },
    } as unknown as Session;
}
function prepare(target: Session, messages: Core[]) {
    const turn = processAdaptiveTurn(core, { messages, state: target.state, config, tokenCount: 0, renderTags: "none" },
        { session: target, body: { model: "gpt-test", input: [] }, protocol: "responses", nativeWindow: WINDOW, enabled: true, injectsNudge: false });
    target.state = turn.state;
    return turn;
}
const folded = (messages: readonly { id: string }[]) => ids(messages).filter(name => FOLDED.includes(name));

test("an epoch turn leaves the kernel state of the whole request, so the next pass can fold it", () => {
    const messages = history(3);
    const target = session(messages, { id: id(4), cause: "pressure" });
    const turn = prepare(target, messages);
    // The fold after the frontier is still waiting for the frontier, not gone, and the request is the previous inbound.
    assert.deepEqual(turn.state.blocks.filter(block => block.active).map(block => block.effectiveMessageIds), [FOLDED]);
    assert.deepEqual(turn.state.lastPassIds, ids(messages));
    // The proxy's preflight runs the kernel over the same request: the folded result is the folded copy resent, not a
    // new one, so it keeps its id and every ref the kernel offers for folding is a message the request carries.
    const preflight = core.processTurn({ messages, state: target.state, config, tokenCount: 0, renderTags: "none" });
    const inbound = new Set(ids(messages));
    assert.deepEqual(Object.keys(preflight.state.messageRefs.byRaw).filter(name => !inbound.has(name)), []);
    assert.deepEqual(folded(preflight.messages), []);
});

test("a request the proxy would have to fold carries every accepted fold", () => {
    const messages = history(3);
    // The provider billed the last request near the window the proxy judges by (204 000 here), far above what the
    // local count sees: the folded step after the frontier must leave the wire now.
    const target = session(messages, { id: id(4), cause: "pressure" }, 190_000);
    const turn = prepare(target, messages);
    assert.equal(epochRecord(target)?.frontier?.id, id(13));
    assert.deepEqual(folded(turn.messages), []);
});

test("a turn that overflows again after an emergency moves the frontier again", () => {
    const messages = history(3);
    // An earlier emergency left the frontier at the first turn's last step; the second turn was folded after it.
    const target = session(messages, { id: id(3), cause: "emergency", step: true }, 190_000);
    const turn = prepare(target, messages);
    assert.equal(epochRecord(target)?.frontier?.id, id(13));
    assert.deepEqual(folded(turn.messages), []);
});

test("below the proxy's window the epoch keeps the past as it was sent", () => {
    const messages = history(3);
    const target = session(messages, { id: id(4), cause: "pressure" }, 20_000);
    const turn = prepare(target, messages);
    assert.equal(epochRecord(target)?.frontier?.id, id(4));
    assert.deepEqual(folded(turn.messages), FOLDED);
});
