// D19 - trivial stdio MCP server (newline-delimited JSON-RPC 2.0), no network, no files read.
// Usage: node mcp-stub.mjs <server-name> [fallback-log]
// Lists ONE tool named after the server: "<server name with - as _>_tool", so Pi declares it as
// mcp__<server>__<server>_tool. Every start is logged (name, pid, ppid) to $D19_MCP_LOG, else to
// the fallback log, so a run shows which servers Pi actually spawned even when their tools are
// hidden from the model.
import fs from "node:fs";
import readline from "node:readline";

const name = process.argv[2] || "d19-unnamed-mcp";
const tool = name.replace(/[^A-Za-z0-9_]/g, "_") + "_tool";
const log = process.env.D19_MCP_LOG || process.argv[3];
function note(line) {
  if (!log) return;
  try { fs.appendFileSync(log, `${new Date().toISOString()} ${name} pid=${process.pid} ppid=${process.ppid} ${line}\n`); } catch {}
}
note("start");

function send(msg) { process.stdout.write(JSON.stringify(msg) + "\n"); }

const rl = readline.createInterface({ input: process.stdin });
rl.on("line", (line) => {
  let m;
  try { m = JSON.parse(line); } catch { return; }
  if (m.id === undefined || m.id === null) return; // notifications
  switch (m.method) {
    case "initialize":
      note("initialize");
      send({ jsonrpc: "2.0", id: m.id, result: {
        protocolVersion: (m.params && m.params.protocolVersion) || "2025-06-18",
        capabilities: { tools: {} },
        serverInfo: { name, version: "1.0.0" },
      } });
      break;
    case "tools/list":
      note("tools/list");
      send({ jsonrpc: "2.0", id: m.id, result: { tools: [{
        name: tool,
        description: `D19 marker tool of the MCP server ${name}.`,
        inputSchema: { type: "object", properties: {}, additionalProperties: false },
      }] } });
      break;
    case "tools/call":
      send({ jsonrpc: "2.0", id: m.id, result: { content: [{ type: "text", text: `${tool} called` }] } });
      break;
    case "ping":
      send({ jsonrpc: "2.0", id: m.id, result: {} });
      break;
    default:
      send({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: `method not found: ${m.method}` } });
  }
});
rl.on("close", () => { note("stdin closed"); process.exit(0); });
