// Prints a compact view of a mock capture file: one header line per request, then one line
// per content block of every message (role, block type, shortened text/ids, signature).
// Usage: node inspect-capture.mjs <capture.jsonl> [--full N]   (--full N: whole text of request N)
import fs from "node:fs";

const file = process.argv[2];
const fullIdx = process.argv.indexOf("--full");
const full = fullIdx > 0 ? Number(process.argv[fullIdx + 1]) : null;
const short = (s, n = 160) => {
  const t = String(s ?? "").replace(/\s+/g, " ");
  return t.length > n ? t.slice(0, n) + `…(+${t.length - n})` : t;
};

for (const line of fs.readFileSync(file, "utf8").split("\n").filter(Boolean)) {
  const e = JSON.parse(line);
  if (full != null && e.n !== full) continue;
  const b = e.body || {};
  const sys = Array.isArray(b.system) ? b.system.map((s) => s.text).join(" ") : b.system ?? "";
  console.log(
    `#${e.n} ${e.method} ${e.url} model=${b.model} stream=${b.stream} max_tokens=${b.max_tokens} ` +
      `thinking=${JSON.stringify(b.thinking)} tools=${(b.tools || []).length} msgs=${(b.messages || []).length} ` +
      `ctx_mgmt=${JSON.stringify(b.context_management)} beta=${e.headers?.["anthropic-beta"] ?? ""}`,
  );
  console.log(`   system: ${full != null ? sys : short(sys, 200)}`);
  (b.messages || []).forEach((m, i) => {
    const blocks = typeof m.content === "string" ? [{ type: "string", text: m.content }] : m.content;
    for (const x of blocks) {
      let d;
      if (x.type === "text" || x.type === "string") d = full != null ? x.text : short(x.text);
      else if (x.type === "thinking") d = `thinking=${short(x.thinking, 80)} signature=${x.signature}`;
      else if (x.type === "redacted_thinking") d = `data=${short(x.data, 40)}`;
      else if (x.type === "tool_use") d = `id=${x.id} name=${x.name} input=${JSON.stringify(x.input)}`;
      else if (x.type === "tool_result") d = `tool_use_id=${x.tool_use_id} is_error=${x.is_error ?? ""} content=${short(JSON.stringify(x.content), full != null ? 100000 : 120)}`;
      else d = short(JSON.stringify(x), 200);
      const cc = x.cache_control ? " [cache_control]" : "";
      console.log(`   [${i}] ${m.role} ${x.type}${cc}: ${d}`);
    }
  });
}
