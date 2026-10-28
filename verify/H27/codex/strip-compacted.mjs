// H27 codex leg - copy a rollout, reducing every `compacted` record's payload to {message} only (the
// shape older Codex versions wrote, and the least a converter would write), for probe-store.ps1 P6.
// Usage: node strip-compacted.mjs --in <rollout.jsonl> --out <rollout.jsonl>
import fs from "node:fs";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, v, i, a) => (v.startsWith("--") ? [...acc, [v.slice(2), a[i + 1]]] : acc), []),
);
let n = 0;
const lines = fs.readFileSync(args.in, "utf8").split("\n").map((line) => {
  if (!line.trim()) return line;
  const rec = JSON.parse(line);
  if (rec.type !== "compacted") return line;
  n += 1;
  return JSON.stringify({ ...rec, payload: { message: rec.payload.message } });
});
fs.writeFileSync(args.out, lines.join("\n"));
console.log(`compacted records reduced to {message}: ${n}`);
