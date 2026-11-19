// P20 helper: a minimal stdio MCP server (no tools) that first records the NAMES of the
// environment variables it was started with (<dir>/mcp-<pid>.txt), then answers initialize,
// tools/list, resources/list, prompts/list. Usage: node mcp-env.mjs <dir>
import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";
const [dir] = process.argv.slice(2);
fs.mkdirSync(dir, { recursive: true });
fs.writeFileSync(path.join(dir, `mcp-${process.pid}.txt`), Object.keys(process.env).sort().join("\n") + "\n");
const send = (m) => process.stdout.write(JSON.stringify(m) + "\n");
readline.createInterface({ input: process.stdin }).on("line", (line) => {
  let m;
  try { m = JSON.parse(line); } catch { return; }
  if (m.id === undefined) return;
  const results = {
    initialize: { protocolVersion: m.params?.protocolVersion || "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "p20env", version: "1.0.0" } },
    "tools/list": { tools: [] },
    "resources/list": { resources: [] },
    "prompts/list": { prompts: [] },
  };
  if (m.method in results) send({ jsonrpc: "2.0", id: m.id, result: results[m.method] });
  else send({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: "method not found" } });
});
