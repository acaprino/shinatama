// D19 - read the FIRST model request of a capture file (H27 mock format: one JSON line per
// request, {n, ts, method, url, headers, body}) and list the d19-* names it contains.
// Usage: node inspect.mjs <capture.jsonl> [--match <text in last user message>] [--out <summary.json>]
// Prints one JSON object: skills (from the system prompt's <name> tags), extension tools,
// MCP tools, context markers, every other d19 token, all tool names, and the system prompt length.
import fs from "node:fs";

const argv = process.argv.slice(2);
const file = argv[0];
const opt = (k) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : undefined; };
const match = opt("--match");
const out = opt("--out");

const lines = fs.existsSync(file) ? fs.readFileSync(file, "utf8").replace(/^﻿/, "").split(/\r?\n/).filter(Boolean) : [];
const reqs = lines.map((l) => JSON.parse(l)).filter((r) => r.method === "POST" && /\/messages(\?|$)/.test(r.url));
const textOf = (c) => typeof c === "string" ? c : Array.isArray(c) ? c.map((b) => b.text ?? (b.content ? textOf(b.content) : "")).join("\n") : "";
const lastUser = (b) => { const u = (b.messages || []).filter((m) => m.role === "user"); return u.length ? textOf(u[u.length - 1].content) : ""; };
const pick = match ? reqs.find((r) => lastUser(r.body).includes(match)) : reqs[0];

let result;
if (!pick) {
  result = { ok: false, requests: reqs.length, error: match ? `no request whose last user message contains ${match}` : "no /messages request" };
} else {
  const b = pick.body;
  const system = textOf(b.system);
  const messages = (b.messages || []).map((m) => `${m.role}: ${textOf(m.content)}`).join("\n");
  const tools = (b.tools || []).map((t) => t.name);
  const tok = (s) => [...new Set((s.match(/d19[-_][a-z0-9_]+(?:-[a-z0-9_]+)*/g) || []))].sort();
  const skillNames = [...system.matchAll(/<name>([^<]+)<\/name>/g)].map((m) => m[1].trim());
  result = {
    ok: true,
    requestN: pick.n,
    requests: reqs.length,
    lastUser: lastUser(b).slice(0, 120),
    skills: skillNames.filter((n) => n.startsWith("d19-")).sort(),
    allSkills: skillNames.sort(),
    extTools: tools.filter((t) => t.startsWith("d19_")).sort(),
    mcpTools: tools.filter((t) => t.startsWith("mcp__")).sort(),
    tools: tools,
    contextMarkers: tok(system).filter((t) => t.endsWith("-context")),
    systemMcp: tok(system).filter((t) => /mcp/.test(t)),
    systemD19: tok(system),
    toolDescD19: tok(JSON.stringify(b.tools || [])),
    messagesD19: tok(messages),
    systemChars: system.length,
    priorUserTurns: (b.messages || []).filter((m) => m.role === "user").map((m) => textOf(m.content).slice(0, 60)),
  };
  if (out) fs.writeFileSync(out.replace(/\.json$/, ".system.txt"), system);
}
const json = JSON.stringify(result, null, 1);
if (out) fs.writeFileSync(out, json);
process.stdout.write(JSON.stringify(result) + "\n");
