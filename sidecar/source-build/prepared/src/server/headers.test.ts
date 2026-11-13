import assert from "node:assert/strict";
import { test } from "node:test";
import { UPSTREAM_HOP_HEADERS } from "./headers.ts";

// buildForwardTarget copies every inbound header except the members of this set, so membership is what
// keeps a header from reaching the model endpoint. The end-to-end proof is the native hand-off test, which
// reads the headers at a synthetic origin.
test("Shinatama's control marker is a hop header and never reaches the model endpoint", () => {
    assert.ok(UPSTREAM_HOP_HEADERS.has("x-shinatama-control"));
    assert.ok(UPSTREAM_HOP_HEADERS.has("x-bili-passthrough"));
    // Header names are matched lowercased; ordinary client headers are not in the set.
    for (const name of ["x-api-key", "authorization", "anthropic-version", "x-session-id"]) {
        assert.ok(!UPSTREAM_HOP_HEADERS.has(name), name);
    }
});
