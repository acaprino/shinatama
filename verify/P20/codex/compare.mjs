// P20 compare: what changes between the clean run and the runs with inherited agent markers.
// Normalises ids, timestamps and the per-variant decoy path, flattens every captured request (headers + body)
// to path=value lines, and prints the lines that differ, plus stderr, exit codes, home file names and egress.
// Usage: node compare.mjs --root <out dir> --base clean --variants codex-markers,claude-markers
import fs from "node:fs";
import path from "node:path";

const args = Object.fromEntries(process.argv.slice(2).reduce((a, v, i, all) => (v.startsWith("--") ? [...a, [v.slice(2), all[i + 1]]] : a), []));
const root = args.root, base = args.base || "clean";
const variants = (args.variants || "").split(",").filter(Boolean);
const read = (p) => (fs.existsSync(p) ? fs.readFileSync(p, "utf8") : "");

function norm(s, name) {
  return String(s)
    .replaceAll(`decoy-${name}`, "decoy-X").replaceAll(`\\\\decoy-${name}`, "\\\\decoy-X")
    .replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, "<uuid>")
    .replace(/\d{4}-\d{2}-\d{2}T\d{2}[:-]\d{2}[:-]\d{2}(\.\d+)?Z?/g, "<ts>")
    .replace(/\b(call|fc|resp|msg|rs)_h27[\w]*|p20env\d+|h27call\d+x\d/g, "<id>")
    .replace(/"(duration_ms|elapsed_ms|wall_time_seconds|create_time|created_at)":\s*[\d.]+/g, '"$1":<n>')
    .replace(/Wall time: [\d.]+ seconds/g, "Wall time: <n> seconds");
}
function flatten(o, pre = "", out = []) {
  if (o && typeof o === "object") {
    for (const [k, v] of Object.entries(o)) flatten(v, pre ? `${pre}.${k}` : k, out);
  } else out.push(`${pre}=${typeof o === "string" ? o.slice(0, 300) : o}`);
  return out;
}
function requests(name) {
  return read(path.join(root, name, "capture.jsonl")).split("\n").filter(Boolean).map((l) => {
    const r = JSON.parse(norm(l, name));
    delete r.n; delete r.ts; delete r.headers["content-length"];
    return flatten({ url: r.url, headers: r.headers, body: r.body });
  });
}
const lines = (name, file) => norm(read(path.join(root, name, file)), name).split(/\r?\n/).filter(Boolean);
const diff = (a, b) => ({ only_base: a.filter((x) => !b.includes(x)), only_variant: b.filter((x) => !a.includes(x)) });

const baseReq = requests(base);
for (const v of variants) {
  console.log(`\n=== ${v} vs ${base}`);
  const vr = requests(v);
  console.log(`requests: ${baseReq.length} vs ${vr.length}`);
  for (let i = 0; i < Math.max(baseReq.length, vr.length); i++) {
    const d = diff(baseReq[i] || [], vr[i] || []);
    // The P20-ENV tool output legitimately lists the injected names; report it separately.
    const isEnv = (x) => /P20ENV /.test(x);
    const env = { only_base: d.only_base.filter(isEnv).length, only_variant: d.only_variant.filter(isEnv).length };
    const rest = { only_base: d.only_base.filter((x) => !isEnv(x)), only_variant: d.only_variant.filter((x) => !isEnv(x)) };
    console.log(`request ${i + 1}: ${rest.only_base.length + rest.only_variant.length} differing field(s) besides the env dump${env.only_base + env.only_variant ? " (env dump output differs)" : ""}`);
    for (const x of rest.only_base.slice(0, 6)) console.log(`   - ${x.slice(0, 220)}`);
    for (const x of rest.only_variant.slice(0, 6)) console.log(`   + ${x.slice(0, 220)}`);
  }
  for (const f of ["t1.exit.txt", "t2.exit.txt"]) console.log(`${f}: ${lines(base, f).join()} vs ${lines(v, f).join()}`);
  for (const f of ["t1.stderr.txt", "t2.stderr.txt", "t1.stdout.jsonl", "t2.stdout.jsonl"]) {
    const d = diff(lines(base, f), lines(v, f));
    console.log(`${f}: ${d.only_base.length + d.only_variant.length} differing line(s)`);
    for (const x of [...d.only_base.map((s) => "   - " + s), ...d.only_variant.map((s) => "   + " + s)].slice(0, 6)) console.log(x.slice(0, 240));
  }
  const files = (n) => lines(n, "home-files.txt").map((s) => s.trim().replace(/^\d+\s+/, "").replace(/rollout-[^\\]+$/, "rollout-<id>.jsonl"));
  const fd = diff(files(base), files(v));
  console.log(`home file names: ${fd.only_base.length + fd.only_variant.length} differing; ${fd.only_base.map((s) => "-" + s).concat(fd.only_variant.map((s) => "+" + s)).join(" ")}`);
  console.log(`outside private home: ${lines(base, "decoy-outside-private.txt").length} vs ${lines(v, "decoy-outside-private.txt").length}; egress: ${lines(base, "egress.jsonl").length} vs ${lines(v, "egress.jsonl").length}`);
}
