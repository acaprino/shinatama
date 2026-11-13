import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import {
    capabilitiesResponse,
    clearCapabilityStampsForTest,
    decideCcrGate,
    decideImageToolGate,
    stampCapabilities,
    type CapabilitiesDeps,
    type CapabilityReason,
    type CcrGateInput,
    type ImageToolGateInput,
} from "./capabilities.ts";
import { encoderState, resetEncoderStateForTest, trackEncoderImport } from "../encoder-status.ts";

const REASONS: ReadonlySet<CapabilityReason> = new Set([
    "none", "not_configured", "inject_tool_off", "codex_host", "plugin_wire_not_allowed",
    "text_protocol", "passthrough", "encoder_missing", "not_observed",
]);

function fakeSession(id: string, protocol: string, requests: number, profile?: string) {
    return {
        id,
        stats: { requests },
        meta: { protocol },
        metadata: profile ? { musashiTerminalContextProfile: { name: profile, source: "fallback" } } : {},
    } as unknown as NonNullable<ReturnType<CapabilitiesDeps["resolveSession"]>>;
}

function deps(session: ReturnType<typeof fakeSession> | undefined, over: Partial<CapabilitiesDeps> = {}): CapabilitiesDeps {
    return {
        passthrough: false,
        compress: {
            ccr: { enabled: true },
            absorb: { enabled: true, mediaOnly: true },
            tiers: true,
            outputSteering: { enabled: true, verbosityLevel: 2, effortRouting: false },
        },
        promptRewriteMode: "auto",
        imageRescaleEnabled: true,
        encoder: "not_observed",
        resolveSession: (id) => (session !== undefined && id === "native-1" ? session : undefined),
        ...over,
    };
}

const ccrBase: CcrGateInput = {
    injectTool: true, storeChannelOk: true, noInjectToolKnob: false,
    codexHost: false, pluginMode: false, pluginWireOk: true, ccrEnabled: true,
};
const imageBase: ImageToolGateInput = {
    rescaleEnabled: true, injectTool: true, storeChannelOk: true, noInjectToolKnob: false,
    codexHost: false, pluginMode: false,
};

function serve(session: ReturnType<typeof fakeSession>, ccr: CcrGateInput, image: ImageToolGateInput) {
    const c = decideCcrGate(ccr);
    stampCapabilities(session, { ...c, configured: ccr.ccrEnabled }, decideImageToolGate(image));
}

function report(session: ReturnType<typeof fakeSession>, over: Partial<CapabilitiesDeps> = {}): Record<string, any> {
    const r = capabilitiesResponse("/__bili/capabilities?native=native-1", deps(session, over));
    assert.equal(r.status, 200);
    return r.body;
}

beforeEach(() => {
    clearCapabilityStampsForTest();
    resetEncoderStateForTest();
});

test("codex_host_reports_ccr_disarmed_with_reason", () => {
    const s = fakeSession("proxy-sess-1", "responses", 12, "build");
    serve(s, { ...ccrBase, codexHost: true, pluginWireOk: false }, { ...imageBase, codexHost: true });
    const body = report(s);
    assert.equal(body.schemaVersion, 1);
    assert.equal(body.sessionId, "proxy-sess-1");
    assert.equal(body.nativeId, "native-1");
    assert.equal(body.protocol, "responses");
    assert.equal(body.requests, 12);
    assert.equal(body.profile, "build");
    assert.equal(body.passthrough, false);
    assert.deepEqual(body.ccr, { configured: true, applied: false, reason: "codex_host" });
    assert.deepEqual(body.readOriginalImage, { applied: false, reason: "codex_host" });
    assert.deepEqual(body.absorb, { configured: true, mediaOnly: true });
    assert.deepEqual(body.tiers, { configured: true });
    assert.deepEqual(body.steering, { configured: true, level: 2, effortRouting: false });
    assert.deepEqual(body.promptRewrite, { mode: "auto" });
});

test("anthropic_proxy_lane_reports_ccr_applied", () => {
    const s = fakeSession("proxy-sess-2", "anthropic", 3, "analysis");
    serve(s, { ...ccrBase, pluginWireOk: true }, imageBase);
    const body = report(s);
    assert.equal(body.protocol, "anthropic");
    assert.equal(body.profile, "analysis");
    assert.deepEqual(body.ccr, { configured: true, applied: true, reason: "none" });
    assert.deepEqual(body.readOriginalImage, { applied: true, reason: "none" });
});

test("plugin_google_wire_reports_plugin_wire_not_allowed", () => {
    const s = fakeSession("proxy-sess-3", "google", 5);
    serve(s, { ...ccrBase, pluginMode: true, pluginWireOk: false }, { ...imageBase, pluginMode: true });
    const body = report(s);
    assert.equal(body.protocol, "google");
    assert.equal(body.profile, null);
    assert.deepEqual(body.ccr, { configured: true, applied: false, reason: "plugin_wire_not_allowed" });
    // plugin lanes register acp_read_original_image natively, so its gate is not the CCR wire gate
    assert.deepEqual(body.readOriginalImage, { applied: true, reason: "none" });
});

test("unknown_session_is_404_not_defaults", () => {
    const s = fakeSession("proxy-sess-4", "anthropic", 1);
    serve(s, ccrBase, imageBase);
    for (const url of [
        "/__bili/capabilities?native=nope",
        "/__bili/capabilities",
        "/__bili/capabilities?native=",
        "/__bili/capabilities?other=native-1",
    ]) {
        const r = capabilitiesResponse(url, deps(s));
        assert.equal(r.status, 404, url);
        assert.deepEqual(r.body, { error: "unknown session" }, url);
    }
});

test("encoder_missing_is_reported_after_a_failed_import", async () => {
    const s = fakeSession("proxy-sess-5", "anthropic", 1);
    serve(s, ccrBase, imageBase);

    assert.equal(encoderState(), "not_observed");
    assert.deepEqual(report(s, { encoder: encoderState() }).imageRescale, { configured: true, applied: false, reason: "not_observed" });

    await assert.rejects(trackEncoderImport(() => Promise.reject(new Error("Cannot find package 'jimp'"))), /jimp/);
    assert.equal(encoderState(), "missing");
    assert.deepEqual(report(s, { encoder: encoderState() }).imageRescale, { configured: true, applied: false, reason: "encoder_missing" });

    // once an import has succeeded in this process it stays loaded; a later failure does not retract it
    resetEncoderStateForTest();
    assert.equal(await trackEncoderImport(() => Promise.resolve("module")), "module");
    assert.deepEqual(report(s, { encoder: encoderState() }).imageRescale, { configured: true, applied: true, reason: "none" });
    await assert.rejects(trackEncoderImport(() => Promise.reject(new Error("boom"))));
    assert.equal(encoderState(), "loaded");

    // knob off: not configured, whatever the encoder did
    assert.deepEqual(report(s, { encoder: "loaded", imageRescaleEnabled: false }).imageRescale, { configured: false, applied: false, reason: "not_configured" });
});

test("reason_is_the_one_stamped_by_the_request_not_recomputed", () => {
    const s = fakeSession("proxy-sess-6", "responses", 7);
    serve(s, { ...ccrBase, codexHost: true, pluginWireOk: false }, { ...imageBase, codexHost: true });
    const before = report(s);
    // config changes after the request: injectTool off, ccr disabled, rescale off. The endpoint must not re-run any gate.
    const after = report(s, {
        compress: { ccr: { enabled: false }, absorb: { enabled: false }, tiers: false, outputSteering: { enabled: false } },
        imageRescaleEnabled: false,
    });
    assert.deepEqual(after.ccr, before.ccr);
    assert.deepEqual(after.readOriginalImage, before.readOriginalImage);
    assert.deepEqual(after.ccr, { configured: true, applied: false, reason: "codex_host" });
    assert.deepEqual(after.absorb, { configured: false, mediaOnly: true });
    assert.deepEqual(after.steering, { configured: false, level: 2, effortRouting: true });

    // the next served request re-stamps (latest wins)
    serve(s, ccrBase, imageBase);
    assert.deepEqual(report(s).ccr, { configured: true, applied: true, reason: "none" });
});

test("a_session_that_has_served_nothing_reports_not_observed_and_global_passthrough_wins", () => {
    const s = fakeSession("proxy-sess-7", "anthropic", 0);
    const fresh = report(s);
    assert.deepEqual(fresh.ccr, { configured: true, applied: false, reason: "not_observed" });
    assert.deepEqual(fresh.readOriginalImage, { applied: false, reason: "not_observed" });
    serve(s, ccrBase, imageBase);
    const pt = report(s, { passthrough: true });
    assert.equal(pt.passthrough, true);
    assert.deepEqual(pt.ccr, { configured: true, applied: false, reason: "passthrough" });
    assert.deepEqual(pt.readOriginalImage, { applied: false, reason: "passthrough" });
});

test("gates_keep_the_original_arming_expressions_and_only_emit_closed_set_reasons", () => {
    const bools = [false, true];
    for (const injectTool of bools) for (const storeChannelOk of bools) for (const codexHost of bools)
        for (const pluginMode of bools) for (const pluginWireOk of bools) for (const ccrEnabled of bools)
            for (const noInjectToolKnob of bools) {
                const d = decideCcrGate({ injectTool, storeChannelOk, noInjectToolKnob, codexHost, pluginMode, pluginWireOk, ccrEnabled });
                // the stamp expression as it was before the reason existed
                const stampDefined = pluginMode ? (pluginWireOk && ccrEnabled) : ccrEnabled;
                assert.equal(d.armed, injectTool && storeChannelOk && !codexHost && stampDefined);
                assert.ok(REASONS.has(d.reason));
                assert.equal(d.armed, d.reason === "none");
            }
    for (const rescaleEnabled of bools) for (const injectTool of bools) for (const storeChannelOk of bools)
        for (const codexHost of bools) for (const pluginMode of bools) for (const noInjectToolKnob of bools) {
            const d = decideImageToolGate({ rescaleEnabled, injectTool, storeChannelOk, noInjectToolKnob, codexHost, pluginMode });
            assert.equal(d.armed, rescaleEnabled && !codexHost && ((injectTool && storeChannelOk) || pluginMode));
            assert.ok(REASONS.has(d.reason));
            assert.equal(d.armed, d.reason === "none");
        }
    // each non-codex closure names itself
    assert.equal(decideCcrGate({ ...ccrBase, injectTool: false }).reason, "inject_tool_off");
    assert.equal(decideCcrGate({ ...ccrBase, storeChannelOk: false }).reason, "text_protocol");
    assert.equal(decideCcrGate({ ...ccrBase, storeChannelOk: false, noInjectToolKnob: true }).reason, "inject_tool_off");
    assert.equal(decideCcrGate({ ...ccrBase, ccrEnabled: false }).reason, "not_configured");
    assert.equal(decideImageToolGate({ ...imageBase, storeChannelOk: false }).reason, "text_protocol");
});
