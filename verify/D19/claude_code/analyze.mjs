// D19 helper: reads a slice of the mock capture (JSON lines) and reports, for the FIRST
// POST /v1/messages request that offers tools (the main turn), which D19 marker names it
// contains, the MCP tools it offers, and the same over all requests of the slice.
// Usage: node analyze.mjs <capture-slice.jsonl> [out.json] [only-requests-containing]
import fs from "node:fs";
const [file, out, must] = process.argv.slice(2);
const MARKERS = [
  // the home's own (U): installed+enabled plugin, its skill/agent/MCP, user-level folders, user MCP
  "d19-home-plugin", "d19-home-skill", "d19-home-agent", "d19-homeplug-mcp",
  "d19-user-skill", "d19-user-agent", "d19-user-cmd", "d19-home-mcp", "d19-local-mcp",
  // the decoy profile (~\.claude\skills) and an --add-dir skill
  "d19-profile-skill", "d19-adda-skill",
  // set A and set B: plugin, its skill, its plugin-level MCP, the --mcp-config MCP
  "d19-a-plugin", "d19-a-skill", "d19-aplug-mcp", "d19-a-mcp",
  "d19-b-plugin", "d19-b-skill", "d19-bplug-mcp", "d19-b-mcp",
];
const lines = fs.existsSync(file) ? fs.readFileSync(file, "utf8").split(/\r?\n/).filter(Boolean) : [];
const reqs = lines.map((l) => JSON.parse(l)).filter((r) => !must || JSON.stringify(r.body).includes(must));
const msgReqs = reqs.filter((r) => r.method === "POST" && /\/v1\/messages(\?|$)/.test(r.url));
const first = msgReqs.find((r) => Array.isArray(r.body?.tools) && r.body.tools.length) || msgReqs[0];
const scan = (r) => {
  const s = JSON.stringify(r.body);
  const found = MARKERS.filter((m) => s.includes(m) || s.includes(m.replace(/-/g, "_")));
  const other = [...new Set(s.match(/d19[-_][a-z0-9_-]+/g) || [])].filter((x) => !MARKERS.some((m) => x.startsWith(m) || x.startsWith(m.replace(/-/g, "_"))));
  const tools = (r.body?.tools || []).map((t) => t.name);
  return { n: r.n, url: r.url, model: r.body?.model, attributionReminder: s.includes("Attribution for git commits"), markers: found, otherD19: other, toolCount: tools.length, mcpTools: tools.filter((t) => t.startsWith("mcp__")), d19Tools: tools.filter((t) => /d19/i.test(t)) };
};
const result = {
  requests: reqs.length, messageRequests: msgReqs.length,
  first: first ? scan(first) : null,
  union: [...new Set(msgReqs.flatMap((r) => scan(r).markers))],
  urls: reqs.map((r) => `${r.n} ${r.method} ${r.url}`),
};
// Where each marker sits in the first request (tools / system / which message), for the record.
if (first) {
  const where = {};
  const b = first.body;
  for (const m of result.first.markers) {
    const w = [];
    if (JSON.stringify(b.tools || []).includes(m) || JSON.stringify(b.tools || []).includes(m.replace(/-/g, "_"))) w.push("tools");
    if (JSON.stringify(b.system || "").includes(m)) w.push("system");
    (b.messages || []).forEach((msg, i) => { if (JSON.stringify(msg).includes(m)) w.push(`messages[${i}].${msg.role}`); });
    where[m] = w;
  }
  result.first.where = where;
}
const text = JSON.stringify(result, null, 2);
if (out) fs.writeFileSync(out, text);
process.stdout.write(text + "\n");
