// P20 / Pi - compares the three runs written by run.ps1 (clean, pi, claude).
// Usage: node compare.mjs <P20 pi scratch root>
// Prints, per variant: env NAMES the bash-tool child received that the Pi process's parent did
// not have (what Pi adds), and every difference in the model requests (system prompt, tools,
// headers, message shapes) against the clean run, plus stdout/stderr/exit and home files.
// Never prints environment VALUES (the dump is names only by construction).
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

const root = process.argv[2];
const variants = ["clean", "pi", "claude"];
const read = (p) => (fs.existsSync(p) ? fs.readFileSync(p, "utf8").replace(/^﻿/, "") : "");
const sha = (s) => crypto.createHash("sha256").update(s).digest("hex").slice(0, 12);

function load(v) {
  const dir = path.join(root, v);
  const reqs = read(path.join(dir, "capture.jsonl")).trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
  const parent = new Set(read(path.join(dir, "parent-env-names.txt")).split("\n").map((s) => s.trim()).filter(Boolean));
  let child = null;
  for (const r of reqs)
    for (const m of r.body.messages || [])
      for (const b of Array.isArray(m.content) ? m.content : [])
        if (b.type === "tool_result") {
          const text = (Array.isArray(b.content) ? b.content.map((c) => c.text || "").join("") : String(b.content || ""));
          const i = text.indexOf("P20-ENV-NAMES ");
          if (i >= 0) child = text.slice(i + 14).split(/\s+/).filter(Boolean);
        }
  const norm = (s) => s.replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, "<uuid>");
  const shape = reqs.map((r) => ({
    url: r.url,
    headers: Object.fromEntries(Object.entries(r.headers).filter(([k]) => !["content-length", "x-api-key", "authorization", "x-claude-code-session-id"].includes(k)).map(([k, v]) => [k, norm(String(v))])),
    system: sha(JSON.stringify(r.body.system || "")),
    tools: (r.body.tools || []).map((t) => t.name).join(","),
    toolsHash: sha(JSON.stringify(r.body.tools || [])),
    params: sha(JSON.stringify({ ...r.body, system: 0, messages: 0, tools: 0, metadata: 0 })),
    metadata: norm(JSON.stringify(r.body.metadata || null)),
    messages: (r.body.messages || []).map((m) => m.role + ":" + (Array.isArray(m.content) ? m.content.map((b) => b.type).join("|") : "text")).join(" "),
  }));
  return {
    dir, reqs, parent, child, shape,
    out: [1, 2].map((n) => ({ stdout: read(path.join(dir, `turn${n}.stdout.txt`)).trim(), stderr: read(path.join(dir, `turn${n}.stderr.txt`)).trim(), exit: (read(path.join(dir, `turn${n}.exit.txt`)).match(/exit=(\S*) killed=(\S*)/) || []).slice(1).join("/") })),
    files: read(path.join(dir, "home-files.txt")).split("\n").map((l) => l.split("\t")[0].replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, "<uuid>").replace(/\d{4}-\d\d-\d\dT[\d-]+Z/g, "<ts>")).filter(Boolean).sort(),
  };
}

const runs = Object.fromEntries(variants.map((v) => [v, load(v)]));
const base = runs.clean;
for (const v of variants) {
  const r = runs[v];
  console.log(`=== ${v}: ${r.reqs.length} requests; turn1 ${r.out[0].exit} "${r.out[0].stdout}"; turn2 ${r.out[1].exit} "${r.out[1].stdout}"; stderr bytes ${r.out[0].stderr.length}/${r.out[1].stderr.length}`);
  if (r.out[0].stderr || r.out[1].stderr) console.log(`  stderr: ${(r.out[0].stderr + " | " + r.out[1].stderr).slice(0, 400)}`);
  if (r.child) {
    const childSet = new Set(r.child);
    const added = r.child.filter((n) => !r.parent.has(n));
    const removed = [...r.parent].filter((n) => !childSet.has(n) && !childSet.has(n.toUpperCase()));
    console.log(`  bash child env: ${r.child.length} names; parent had ${r.parent.size}`);
    console.log(`  added by Pi (in child, not in parent): ${added.join(" ") || "(none)"}`);
    console.log(`  removed by Pi (in parent, not in child): ${removed.join(" ") || "(none)"}`);
    const markers = r.child.filter((n) => /^(PI_|AI_AGENT$|CLAUDE|CODEX|OPENCODE)/.test(n));
    console.log(`  marker-like names in child: ${markers.join(" ")}`);
  } else console.log("  (no env-name dump found in captures)");
  if (v === "clean") continue;
  const diffs = [];
  const n = Math.max(r.shape.length, base.shape.length);
  for (let i = 0; i < n; i++) {
    const a = base.shape[i], b = r.shape[i];
    if (!a || !b) { diffs.push(`request ${i + 1}: present only in ${a ? "clean" : v}`); continue; }
    for (const k of ["url", "system", "tools", "toolsHash", "params", "metadata", "messages"]) if (a[k] !== b[k]) diffs.push(`request ${i + 1} ${k}: clean=${a[k]} ${v}=${b[k]}`);
    const hk = new Set([...Object.keys(a.headers), ...Object.keys(b.headers)]);
    for (const k of hk) if (a.headers[k] !== b.headers[k]) diffs.push(`request ${i + 1} header ${k}: clean=${a.headers[k]} ${v}=${b.headers[k]}`);
  }
  const fa = base.files.join("\n"), fb = r.files.join("\n");
  if (fa !== fb) diffs.push(`home files differ:\n    clean: ${base.files.join(", ")}\n    ${v}: ${r.files.join(", ")}`);
  for (const t of [0, 1]) if (r.out[t].stdout !== base.out[t].stdout || r.out[t].exit !== base.out[t].exit) diffs.push(`turn${t + 1} output: clean="${base.out[t].stdout}" (${base.out[t].exit}) ${v}="${r.out[t].stdout}" (${r.out[t].exit})`);
  console.log(`  differences vs clean (requests, headers, home files, output): ${diffs.length ? "\n  - " + diffs.join("\n  - ") : "none"}`);
}
