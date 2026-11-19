// D19: reads a mock capture (one JSON request per line) and reports, for the FIRST model request whose last user
// text contains --marker, every d19-* / d19_* name it contains and where (instructions, each input message by
// role, tool names), plus the full tool-name list and the lines that mention a SKILL.md.
// Usage: node inspect.mjs --capture <file.jsonl> --marker <text> [--out <file.json>]
import fs from "node:fs";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, v, i, a) => (v.startsWith("--") ? [...acc, [v.slice(2), a[i + 1]]] : acc), []),
);
const lines = fs.existsSync(args.capture) ? fs.readFileSync(args.capture, "utf8").split(/\r?\n/).filter(Boolean) : [];
const NAME_RE = /d19[-_][A-Za-z0-9_-]*[A-Za-z0-9]/g;
const uniq = (a) => [...new Set(a)].sort();
const names = (s) => uniq(String(s ?? "").match(NAME_RE) || []);
const textOf = (c) => (typeof c === "string" ? c : Array.isArray(c) ? c.map((b) => b?.text ?? b?.content ?? "").join("\n") : "");

function lastUserText(body) {
  const items = (body.input || []).filter((i) => i.role === "user");
  return items.length ? textOf(items[items.length - 1].content) : "";
}
function toolNames(tools) {
  const out = [];
  for (const t of tools || []) {
    if (t.type === "namespace" || Array.isArray(t.tools)) {
      for (const s of t.tools || []) out.push(`${t.name ?? t.namespace ?? "?"}/${s.name ?? s.function?.name ?? "?"}`);
    } else out.push(t.name ?? t.function?.name ?? t.type);
  }
  return out;
}

let found = null;
for (const l of lines) {
  let o;
  try { o = JSON.parse(l); } catch { continue; }
  if (o.method !== "POST" || !/responses/.test(o.url || "") || !o.body) continue;
  if (lastUserText(o.body).includes(args.marker)) { found = o; break; }
}
let report;
if (!found) report = { marker: args.marker, found: false, requests: lines.length };
else {
  const b = found.body;
  const byLocation = { instructions: names(b.instructions) };
  (b.input || []).forEach((it, i) => {
    const n = names(JSON.stringify(it));
    if (n.length) byLocation[`input[${i}] ${it.role ?? it.type}`] = n;
  });
  const tn = toolNames(b.tools);
  byLocation.tools = names(JSON.stringify(b.tools || []));
  const all = names(JSON.stringify(b));
  const skillLines = JSON.stringify(b).split(/\\n/).filter((s) => /SKILL\.md/i.test(s)).map((s) => s.slice(0, 300));
  report = {
    marker: args.marker, found: true, n: found.n, ts: found.ts, d19_all: all, d19_by_location: byLocation,
    tool_names: tn, skill_lines: skillLines,
  };
}
const txt = JSON.stringify(report, null, 1);
if (args.out) fs.writeFileSync(args.out, txt);
console.log(txt);
