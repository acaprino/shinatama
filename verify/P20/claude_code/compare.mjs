// P20 compare: the 'clean' and 'markers' runs of run.ps1. Prints what differs: child env
// names per launcher (hooks, MCP, Bash tool), model requests (headers, system prompt, tools,
// metadata), files written, stderr. Usage: node compare.mjs [rootDir] [baselineNames.txt]
// rootDir defaults to %LOCALAPPDATA%\shinatama-verify\P20\claude_code. baselineNames: the
// env names of the process that launched claude (to tell what Claude itself adds).
import fs from "node:fs";
import path from "node:path";

const root = process.argv[2] || path.join(process.env.LOCALAPPDATA, "shinatama-verify", "P20", "claude_code");
const baseline = process.argv[3] ? new Set(fs.readFileSync(process.argv[3], "utf8").split(/\r?\n/).filter(Boolean)) : null;
const V = ["clean", "markers"];
const read = (f) => (fs.existsSync(f) ? fs.readFileSync(f, "utf8") : "");
const norm = (s) =>
  String(s)
    .replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, "<uuid>")
    .replace(/\\(clean|markers)\\/g, "\\<variant>\\")
    .replace(/-(clean|markers)-/g, "-<variant>-")
    .replace(/\b\d{4}-\d{2}-\d{2}T[\d:.]+Z\b/g, "<ts>")
    .replace(/sessions\\\d+\./g, "sessions\\<pid>.")
    .replace(/sessions\\\d+\.json/g, "sessions\\<pid>.json")
    .replace(/\.[0-9a-f]{64}\.key/g, ".<hash>.key")
    .replace(/backup\.\d+/g, "backup.<n>");
const setDiff = (a, b) => [...a].filter((x) => !b.has(x));
const out = [];
const say = (s = "") => out.push(s);

// 1. env names per launcher
say("## 1. Environment variable NAMES received by children (union per launcher)");
const dumps = {};
for (const v of V) {
  dumps[v] = {};
  const dir = path.join(root, v, "envdump");
  for (const f of fs.existsSync(dir) ? fs.readdirSync(dir) : []) {
    const label = f.replace(/-\d+\.txt$/, "");
    dumps[v][label] ??= new Set();
    for (const n of read(path.join(dir, f)).split(/\r?\n/).filter(Boolean)) dumps[v][label].add(n);
  }
}
const labels = [...new Set(V.flatMap((v) => Object.keys(dumps[v])))].sort();
const agentish = /^(CLAUDE|AI_AGENT|ANTHROPIC|TRACEPARENT|GIT_EDITOR|SHELL$|BUN_|MCP|OTEL|COREPACK|NoDefaultCurrentDirectoryInExePath)/;
for (const l of labels) {
  const a = dumps.clean[l] || new Set(), b = dumps.markers[l] || new Set();
  say(`- ${l}: clean ${a.size} names, markers ${b.size} names`);
  if (baseline) say(`    added by Claude (clean, not in launcher env): ${setDiff(a, baseline).sort().join(" ") || "-"}`);
  say(`    agent-ish names (clean): ${[...a].filter((n) => agentish.test(n)).sort().join(" ")}`);
  say(`    only in markers: ${setDiff(b, a).sort().join(" ") || "-"}`);
  say(`    only in clean:   ${setDiff(a, b).sort().join(" ") || "-"}`);
}

// 2. model requests
say("\n## 2. Model requests (POST /v1/messages, count_tokens excluded), paired in order");
const caps = {};
for (const v of V) {
  caps[v] = read(path.join(root, v, "capture.jsonl")).split(/\r?\n/).filter(Boolean).map((l) => JSON.parse(l));
  say(`- ${v}: ${caps[v].length} requests: ${caps[v].map((c) => c.url.replace(/\?.*/, "")).join(", ")}`);
}
const msgs = (v) => caps[v].filter((c) => /\/v1\/messages(\?|$)/.test(c.url) && !/count_tokens/.test(c.url));
const [ma, mb] = [msgs("clean"), msgs("markers")];
const sysText = (b) => (Array.isArray(b?.system) ? b.system.map((x) => x.text || "").join("\n") : String(b?.system || ""));
const lineDiff = (x, y) => {
  const A = norm(x).split("\n"), B = norm(y).split("\n");
  const sa = new Set(A), sb = new Set(B);
  return { onlyA: A.filter((l) => !sb.has(l)), onlyB: B.filter((l) => !sa.has(l)) };
};
const ignoreHdr = new Set(["content-length", "x-stainless-retry-count", "x-stainless-timeout", "x-client-request-id", "x-claude-code-session-id"]);
for (let i = 0; i < Math.max(ma.length, mb.length); i++) {
  const a = ma[i], b = mb[i];
  if (!a || !b) { say(`- request #${i + 1}: present only in ${a ? "clean" : "markers"}`); continue; }
  const diffs = [];
  for (const k of new Set([...Object.keys(a.headers), ...Object.keys(b.headers)])) {
    if (ignoreHdr.has(k)) continue;
    if (norm(a.headers[k]) !== norm(b.headers[k])) diffs.push(`header ${k}: clean=${JSON.stringify(a.headers[k])} markers=${JSON.stringify(b.headers[k])}`);
  }
  const ka = Object.keys(a.body || {}).sort().join(","), kb = Object.keys(b.body || {}).sort().join(",");
  if (ka !== kb) diffs.push(`body keys: clean=[${ka}] markers=[${kb}]`);
  const ta = (a.body?.tools || []).map((t) => t.name).sort().join(","), tb = (b.body?.tools || []).map((t) => t.name).sort().join(",");
  if (ta !== tb) diffs.push(`tools: only clean=[${setDiff(new Set(ta.split(",")), new Set(tb.split(","))).join(",")}] only markers=[${setDiff(new Set(tb.split(",")), new Set(ta.split(","))).join(",")}]`);
  if (norm(JSON.stringify(a.body?.metadata)) !== norm(JSON.stringify(b.body?.metadata))) diffs.push(`metadata: clean=${norm(JSON.stringify(a.body?.metadata))} markers=${norm(JSON.stringify(b.body?.metadata))}`);
  const sd = lineDiff(sysText(a.body), sysText(b.body));
  for (const l of sd.onlyA) diffs.push(`system only clean: ${l.slice(0, 220)}`);
  for (const l of sd.onlyB) diffs.push(`system only markers: ${l.slice(0, 220)}`);
  const md = lineDiff(JSON.stringify(a.body?.messages || [], null, 1), JSON.stringify(b.body?.messages || [], null, 1));
  for (const l of md.onlyA.slice(0, 8)) diffs.push(`messages only clean: ${l.trim().slice(0, 220)}`);
  for (const l of md.onlyB.slice(0, 8)) diffs.push(`messages only markers: ${l.trim().slice(0, 220)}`);
  say(`- request #${i + 1} (${a.url.replace(/\?.*/, "")}): ${diffs.length ? "" : "no difference after normalisation"}`);
  for (const d of diffs) say(`    ${d}`);
}

// 3. files
say("\n## 3. Files written (home and work dir; uuids/pids normalised)");
for (const f of ["home-files.txt", "work-files.txt"]) {
  const [a, b] = V.map((v) => new Set(read(path.join(root, v, f)).split(/\r?\n/).filter(Boolean).map((l) => norm(l.split("\t")[0]))));
  say(`- ${f}: clean ${a.size}, markers ${b.size}`);
  say(`    only clean:   ${setDiff(a, b).join(" | ") || "-"}`);
  say(`    only markers: ${setDiff(b, a).join(" | ") || "-"}`);
}

// 4. stdout/stderr per step
say("\n## 4. stdout / stderr per step");
const steps = [...new Set(V.flatMap((v) => (fs.existsSync(path.join(root, v, "steps")) ? fs.readdirSync(path.join(root, v, "steps")) : [])))].sort();
for (const s of steps) {
  const [a, b] = V.map((v) => norm(read(path.join(root, v, "steps", s))).replace(/"(duration_ms|duration_api_ms|total_cost_usd|uuid)":[^,}]+/g, '"$1":<x>'));
  if (a !== b) say(`- ${s} differs:\n    clean:   ${a.slice(0, 400).replace(/\n/g, " ")}\n    markers: ${b.slice(0, 400).replace(/\n/g, " ")}`);
}
say("\n## 5. Summaries");
for (const v of V) say(read(path.join(root, v, "summary.txt")));
const text = out.join("\n");
fs.writeFileSync(path.join(root, "compare.txt"), text);
process.stdout.write(text + "\n");
