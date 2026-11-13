import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative, sep } from "node:path";
import { test } from "node:test";
import { retrievalExportDirFor, writeRetrievalExport } from "../src/retrieve-export.ts";

// Two sessions that start alike number their messages alike, so they hold the
// same ref (m00012) for different tool results. The kernel exports a retrieve to
// <dir>/<ref>.txt; the dir must therefore differ per session.
const REF = "m00012";

function exportFor(root: string, sessionId: string, text: string) {
    return { path: join(retrievalExportDirFor(root, sessionId), `${REF}.txt`), text };
}

test("same ref in two sessions exports to different paths and keeps each session's text", () => {
    const root = mkdtempSync(join(tmpdir(), "retrieve-export-"));
    try {
        const a = exportFor(root, "session-a", "RESULT_A ".repeat(100));
        const b = exportFor(root, "session-b", "RESULT_B ".repeat(100));
        assert.notEqual(a.path, b.path);
        assert.equal(writeRetrievalExport(a, () => assert.fail("write a")), true);
        assert.equal(writeRetrievalExport(b, () => assert.fail("write b")), true);
        assert.equal(readFileSync(a.path, "utf8"), a.text);
        assert.equal(readFileSync(b.path, "utf8"), b.text);
        assert.doesNotMatch(readFileSync(a.path, "utf8"), /RESULT_B/);
    } finally {
        rmSync(root, { recursive: true, force: true });
    }
});

test("the per-session directory is stable, stays under <state>/retrieve and never carries the raw id", () => {
    const root = mkdtempSync(join(tmpdir(), "retrieve-export-"));
    try {
        const hostile = ["../../escape", "a/b", "a\\b", "con", "C:\\x","x".repeat(500), "", "id with space\u0000"];
        const seen = new Set<string>();
        for (const id of hostile) {
            const dir = retrievalExportDirFor(root, id);
            assert.equal(dir, retrievalExportDirFor(root, id), "deterministic across calls");
            assert.equal(dirname(dir), join(root, "retrieve"), "one level under <state>/retrieve");
            assert.match(basename(dir), /^[0-9a-f]{16}$/, "filesystem-safe key");
            assert.ok(!relative(join(root, "retrieve"), dir).startsWith(".."), "confined");
            assert.ok(!dir.includes(sep + "img" + sep) && basename(dir) !== "img", "never the image-spill dir");
            seen.add(dir);
        }
        assert.equal(seen.size, hostile.length, "distinct ids get distinct directories");
    } finally {
        rmSync(root, { recursive: true, force: true });
    }
});
