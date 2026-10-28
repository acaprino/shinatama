// Prints one line per record of a Claude Code session .jsonl: line number, type/subtype,
// uuid -> parentUuid (first 8 chars), flags, and the message's content blocks shortened.
// Usage: node inspect-session.mjs <session.jsonl> [--line N]   (--line N: that record verbatim)
import fs from "node:fs";

const file = process.argv[2];
const li = process.argv.indexOf("--line");
const only = li > 0 ? Number(process.argv[li + 1]) : null;
const s8 = (u) => (u ? String(u).slice(0, 8) : "-");
const short = (s, n = 110) => {
  const t = String(s ?? "").replace(/\s+/g, " ");
  return t.length > n ? t.slice(0, n) + `…(+${t.length - n})` : t;
};
const COMMON = new Set(["type", "subtype", "uuid", "parentUuid", "message", "sessionId", "cwd", "gitBranch", "timestamp", "version", "isSidechain", "userType", "entrypoint", "slug", "requestId", "content", "level"]);

fs.readFileSync(file, "utf8").split("\n").filter(Boolean).forEach((line, i) => {
  const r = JSON.parse(line);
  if (only != null) {
    if (i + 1 === only) console.log(JSON.stringify(r, null, 2));
    return;
  }
  const extra = Object.keys(r).filter((k) => !COMMON.has(k)).map((k) => {
    const v = r[k];
    return typeof v === "object" && v !== null ? `${k}={${Object.keys(v).join(",")}}` : `${k}=${short(v, 40)}`;
  });
  let body = "";
  const m = r.message;
  if (m) {
    const blocks = typeof m.content === "string" ? [{ type: "string", text: m.content }] : m.content || [];
    body = `${m.role}${m.id ? " " + m.id : ""}${m.stop_reason ? " stop=" + m.stop_reason : ""}: ` + blocks.map((b) => {
      if (b.type === "text" || b.type === "string") return `${b.type}(${short(b.text, 70)})`;
      if (b.type === "thinking") return `thinking(${short(b.thinking, 30)} sig=${b.signature})`;
      if (b.type === "tool_use") return `tool_use(${b.id} ${b.name})`;
      if (b.type === "tool_result") return `tool_result(${b.tool_use_id} ${short(JSON.stringify(b.content), 40)})`;
      return b.type;
    }).join(" + ");
  } else if (r.content != null) body = `content=${short(typeof r.content === "string" ? r.content : JSON.stringify(r.content), 90)}`;
  console.log(`${String(i + 1).padStart(3)} ${r.type}${r.subtype ? "/" + r.subtype : ""} ${s8(r.uuid)}<-${s8(r.parentUuid)} ${r.isSidechain ? "SIDECHAIN " : ""}${extra.join(" ")} | ${body}`);
});
