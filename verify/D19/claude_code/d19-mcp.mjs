// D19 helper: a minimal stdio MCP server that offers ONE tool named after the server
// (d19-a-mcp -> tool d19_a_mcp_tool) and records that it was started (<logDir>/<name>-<pid>.started).
// Usage: node d19-mcp.mjs <server-name> <logDir>
import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";
const [name, logDir] = process.argv.slice(2);
if (logDir) { fs.mkdirSync(logDir, { recursive: true }); fs.writeFileSync(path.join(logDir, `${name}-${process.pid}.started`), new Date().toISOString() + "\n"); }
const tool = name.replace(/-/g, "_") + "_tool";
const send = (m) => process.stdout.write(JSON.stringify(m) + "\n");
readline.createInterface({ input: process.stdin }).on("line", (line) => {
  let m;
  try { m = JSON.parse(line); } catch { return; }
  if (m.id === undefined) return;
  const results = {
    initialize: { protocolVersion: m.params?.protocolVersion || "2025-06-18", capabilities: { tools: {} }, serverInfo: { name, version: "1.0.0" } },
    "tools/list": { tools: [{ name: tool, description: `D19 marker tool of MCP server ${name}`, inputSchema: { type: "object", properties: {} } }] },
    "tools/call": { content: [{ type: "text", text: `${tool} called` }] },
    "resources/list": { resources: [] },
    "prompts/list": { prompts: [] },
  };
  if (m.method in results) send({ jsonrpc: "2.0", id: m.id, result: results[m.method] });
  else send({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: "method not found" } });
});
